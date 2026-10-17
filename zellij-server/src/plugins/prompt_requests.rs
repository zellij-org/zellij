use super::PluginId;
use crate::ClientId;
use std::collections::{BTreeMap, HashMap};
use zellij_utils::data::{Event, PipeMessage};
use zellij_utils::prompt::PromptResult;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PromptCaller {
    pub plugin_id: PluginId,
    pub client_id: ClientId,
    pub request_id: u64,
}

pub const SERVER_NOTICE_CALLER_ID: PluginId = u32::MAX;
pub const CLOSE_DIALOGUE_CALLER_ID: PluginId = u32::MAX - 1;

impl PromptCaller {
    pub fn close_dialogue(client_id: ClientId, request_id: u64) -> Self {
        PromptCaller {
            plugin_id: CLOSE_DIALOGUE_CALLER_ID,
            client_id,
            request_id,
        }
    }
    pub fn is_close_dialogue(&self) -> bool {
        self.plugin_id == CLOSE_DIALOGUE_CALLER_ID
    }
    pub fn result_event(
        &self,
        result: PromptResult,
    ) -> (Option<PluginId>, Option<ClientId>, Event) {
        (
            Some(self.plugin_id),
            Some(self.client_id),
            Event::PromptResult(self.request_id, result),
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PopupRequest {
    Cli(String),
    Prompt(PromptCaller),
}

#[derive(Debug, Clone, PartialEq)]
pub enum PromptAction {
    Deliver(PromptCaller, PromptResult),
    Send(PluginId, ClientId, PipeMessage),
    Close(PluginId),
}

#[derive(Debug)]
enum PromptState {
    Opening(PipeMessage),
    Open(PluginId),
}

#[derive(Debug)]
struct PendingPrompt {
    state: PromptState,
    is_notice: bool,
}

#[derive(Debug, Default)]
pub struct PromptRequests {
    pending: HashMap<PromptCaller, PendingPrompt>,
}

impl PromptRequests {
    pub fn start(&mut self, caller: PromptCaller, message: PipeMessage, is_notice: bool) {
        self.pending.insert(
            caller,
            PendingPrompt {
                state: PromptState::Opening(message),
                is_notice,
            },
        );
    }
    pub fn attach(
        &mut self,
        caller: PromptCaller,
        prompt_plugin_id: PluginId,
        prompt_client_id: ClientId,
        caller_args: BTreeMap<String, String>,
    ) -> Vec<PromptAction> {
        let Some(pending) = self.pending.get_mut(&caller) else {
            return vec![PromptAction::Close(prompt_plugin_id)];
        };
        match std::mem::replace(&mut pending.state, PromptState::Open(prompt_plugin_id)) {
            PromptState::Opening(mut message) => {
                message.args.extend(caller_args);
                vec![PromptAction::Send(
                    prompt_plugin_id,
                    prompt_client_id,
                    message,
                )]
            },
            PromptState::Open(previous_plugin_id) => {
                pending.state = PromptState::Open(previous_plugin_id);
                vec![PromptAction::Close(prompt_plugin_id)]
            },
        }
    }
    pub fn fail(&mut self, caller: PromptCaller, error: String) -> Vec<PromptAction> {
        match self.pending.remove(&caller) {
            Some(_) => vec![PromptAction::Deliver(caller, PromptResult::Error(error))],
            None => vec![],
        }
    }
    pub fn reply(
        &mut self,
        prompt_plugin_id: PluginId,
        request_id: u64,
        result: PromptResult,
    ) -> Vec<PromptAction> {
        let caller = self.pending.iter().find_map(|(caller, pending)| {
            let answered_by_this_prompt = matches!(
                pending.state,
                PromptState::Open(plugin_id) if plugin_id == prompt_plugin_id
            );
            (answered_by_this_prompt && caller.request_id == request_id).then_some(*caller)
        });
        match caller {
            Some(caller) => {
                self.pending.remove(&caller);
                vec![PromptAction::Deliver(caller, result)]
            },
            None => vec![],
        }
    }
    pub fn plugin_unloaded(&mut self, plugin_id: PluginId) -> Vec<PromptAction> {
        let mut actions = self.prompt_closed(plugin_id);
        actions.append(&mut self.callers_gone(|caller| caller.plugin_id == plugin_id));
        actions
    }
    pub fn withdraw(&mut self, caller: PromptCaller) -> Vec<PromptAction> {
        self.callers_gone(|pending| *pending == caller)
    }
    pub fn caller_reloaded(&mut self, plugin_id: PluginId) -> Vec<PromptAction> {
        self.callers_gone(|caller| caller.plugin_id == plugin_id)
    }
    pub fn caller_instance_gone(
        &mut self,
        plugin_id: PluginId,
        client_id: ClientId,
    ) -> Vec<PromptAction> {
        self.callers_gone(|caller| caller.plugin_id == plugin_id && caller.client_id == client_id)
    }
    #[cfg(test)]
    pub fn is_waiting_for(&self, caller: &PromptCaller) -> bool {
        self.pending.contains_key(caller)
    }
    fn prompt_closed(&mut self, prompt_plugin_id: PluginId) -> Vec<PromptAction> {
        let callers: Vec<PromptCaller> = self
            .pending
            .iter()
            .filter(|(_, pending)| {
                matches!(pending.state, PromptState::Open(plugin_id) if plugin_id == prompt_plugin_id)
            })
            .map(|(caller, _)| *caller)
            .collect();
        let mut actions = vec![];
        for caller in callers {
            if let Some(pending) = self.pending.remove(&caller) {
                if !pending.is_notice {
                    actions.push(PromptAction::Deliver(caller, PromptResult::Cancelled));
                }
            }
        }
        actions
    }
    fn callers_gone(&mut self, is_gone: impl Fn(&PromptCaller) -> bool) -> Vec<PromptAction> {
        let callers: Vec<PromptCaller> = self
            .pending
            .keys()
            .filter(|caller| is_gone(caller))
            .copied()
            .collect();
        let mut actions = vec![];
        for caller in callers {
            if let Some(PendingPrompt {
                state: PromptState::Open(prompt_plugin_id),
                ..
            }) = self.pending.remove(&caller)
            {
                actions.push(PromptAction::Close(prompt_plugin_id));
            }
        }
        actions
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zellij_utils::data::PipeSource;
    use zellij_utils::prompt::PromptValue;

    fn caller(plugin_id: PluginId, client_id: ClientId, request_id: u64) -> PromptCaller {
        PromptCaller {
            plugin_id,
            client_id,
            request_id,
        }
    }

    fn message(caller: PromptCaller) -> PipeMessage {
        PipeMessage::new(
            PipeSource::PromptRequest {
                caller_plugin_id: caller.plugin_id,
                request_id: caller.request_id,
            },
            "confirm",
            &None,
            &None,
            true,
        )
    }

    fn opened(requests: &mut PromptRequests, caller: PromptCaller, prompt_plugin_id: PluginId) {
        requests.start(caller, message(caller), false);
        let mut caller_args = BTreeMap::new();
        caller_args.insert("_caller_pane_title".to_owned(), "fixture".to_owned());
        let actions = requests.attach(caller, prompt_plugin_id, caller.client_id, caller_args);
        match &actions[..] {
            [PromptAction::Send(plugin_id, client_id, message)] => {
                assert_eq!(*plugin_id, prompt_plugin_id);
                assert_eq!(*client_id, caller.client_id);
                assert_eq!(
                    message.args.get("_caller_pane_title").map(|s| s.as_str()),
                    Some("fixture")
                );
            },
            other => panic!("unexpected {:?}", other),
        }
    }

    #[test]
    fn the_answer_reaches_only_the_caller_that_asked() {
        let mut requests = PromptRequests::default();
        let asker = caller(1, 1, 7);
        let same_plugin_other_user = caller(1, 2, 7);
        opened(&mut requests, asker, 10);
        opened(&mut requests, same_plugin_other_user, 11);
        let answer = PromptResult::Confirmed(true);
        assert_eq!(
            requests.reply(10, 7, answer.clone()),
            vec![PromptAction::Deliver(asker, answer.clone())]
        );
        assert_eq!(asker.result_event(answer.clone()).0, Some(1));
        assert_eq!(asker.result_event(answer).1, Some(1));
        assert!(requests.is_waiting_for(&same_plugin_other_user));
        assert!(requests.reply(10, 7, PromptResult::Cancelled).is_empty());
    }

    #[test]
    fn a_reply_from_another_plugin_or_for_another_request_is_ignored() {
        let mut requests = PromptRequests::default();
        let asker = caller(1, 1, 7);
        opened(&mut requests, asker, 10);
        assert!(requests.reply(99, 7, PromptResult::Cancelled).is_empty());
        assert!(requests.reply(10, 8, PromptResult::Cancelled).is_empty());
        assert!(requests.is_waiting_for(&asker));
    }

    #[test]
    fn the_caller_closing_closes_its_prompt_and_sends_nothing() {
        let mut requests = PromptRequests::default();
        let asker = caller(1, 1, 7);
        let other = caller(2, 1, 1);
        opened(&mut requests, asker, 10);
        opened(&mut requests, other, 11);
        assert_eq!(requests.plugin_unloaded(1), vec![PromptAction::Close(10)]);
        assert!(requests.plugin_unloaded(10).is_empty());
        assert!(requests.is_waiting_for(&other));
    }

    #[test]
    fn the_caller_closing_while_the_prompt_opens_closes_it_once_it_arrives() {
        let mut requests = PromptRequests::default();
        let asker = caller(1, 1, 7);
        requests.start(asker, message(asker), false);
        assert!(requests.plugin_unloaded(1).is_empty());
        assert_eq!(
            requests.attach(asker, 10, 1, BTreeMap::new()),
            vec![PromptAction::Close(10)]
        );
    }

    #[test]
    fn a_reloaded_caller_or_a_detached_caller_instance_closes_its_prompts() {
        let mut requests = PromptRequests::default();
        let asker = caller(1, 1, 7);
        let other_user = caller(1, 2, 7);
        opened(&mut requests, asker, 10);
        opened(&mut requests, other_user, 11);
        assert_eq!(
            requests.caller_instance_gone(1, 2),
            vec![PromptAction::Close(11)]
        );
        assert_eq!(requests.caller_reloaded(1), vec![PromptAction::Close(10)]);
    }

    #[test]
    fn the_prompt_closing_without_an_answer_sends_cancelled() {
        let mut requests = PromptRequests::default();
        let asker = caller(1, 1, 7);
        opened(&mut requests, asker, 10);
        assert_eq!(
            requests.plugin_unloaded(10),
            vec![PromptAction::Deliver(asker, PromptResult::Cancelled)]
        );
        assert!(!requests.is_waiting_for(&asker));
    }

    #[test]
    fn a_notice_closing_sends_nothing_and_a_click_is_delivered() {
        let mut requests = PromptRequests::default();
        let asker = caller(1, 1, 7);
        requests.start(asker, message(asker), true);
        requests.attach(asker, 10, 1, BTreeMap::new());
        assert!(requests.plugin_unloaded(10).is_empty());
        let clicked = caller(1, 1, 8);
        requests.start(clicked, message(clicked), true);
        requests.attach(clicked, 11, 1, BTreeMap::new());
        let answer = PromptResult::Answered(PromptValue::Bool(true));
        assert_eq!(
            requests.reply(11, 8, answer.clone()),
            vec![PromptAction::Deliver(clicked, answer)]
        );
    }

    #[test]
    fn a_withdrawn_prompt_closes_without_an_answer() {
        let mut requests = PromptRequests::default();
        let asker = PromptCaller::close_dialogue(1, 3);
        let other = caller(1, 1, 3);
        opened(&mut requests, asker, 10);
        opened(&mut requests, other, 11);
        assert_eq!(requests.withdraw(asker), vec![PromptAction::Close(10)]);
        assert!(requests.plugin_unloaded(10).is_empty());
        assert!(requests.is_waiting_for(&other));
        let opening = PromptCaller::close_dialogue(1, 4);
        requests.start(opening, message(opening), false);
        assert!(requests.withdraw(opening).is_empty());
        assert_eq!(
            requests.attach(opening, 12, 1, BTreeMap::new()),
            vec![PromptAction::Close(12)]
        );
    }

    #[test]
    fn a_prompt_that_cannot_open_reports_an_error() {
        let mut requests = PromptRequests::default();
        let asker = caller(1, 1, 7);
        requests.start(asker, message(asker), false);
        assert_eq!(
            requests.fail(asker, "no user".to_owned()),
            vec![PromptAction::Deliver(
                asker,
                PromptResult::Error("no user".to_owned())
            )]
        );
        assert!(requests.fail(asker, "again".to_owned()).is_empty());
    }
}
