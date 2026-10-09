use std::collections::{BTreeMap, VecDeque};

use zellij_tile::prelude::actions::Action;
use zellij_tile::prelude::*;

pub const CLICK_SEQUENCE_CONTEXT_KEY: &str = "bars_click_sequence";

#[derive(Debug, Clone, PartialEq)]
pub struct ClickRegion {
    pub start: usize,
    pub end: usize,
    pub actions: Vec<Action>,
}

impl ClickRegion {
    pub fn new(start: usize, end: usize, actions: Vec<Action>) -> Self {
        ClickRegion {
            start,
            end,
            actions,
        }
    }
    pub fn contains(&self, col: usize) -> bool {
        col >= self.start && col < self.end
    }
    pub fn shifted(&self, by: usize) -> Self {
        ClickRegion {
            start: self.start + by,
            end: self.end + by,
            actions: self.actions.clone(),
        }
    }
}

pub fn region_at(regions: &[ClickRegion], col: usize) -> Option<&ClickRegion> {
    regions.iter().find(|region| region.contains(col))
}

pub fn actions_for_key(
    keybinds: &[(KeyWithModifier, Vec<Action>)],
    key: &KeyWithModifier,
    stripped_modifiers: &[KeyModifier],
) -> Option<Vec<Action>> {
    let mut full_key = key.clone();
    for modifier in stripped_modifiers {
        full_key.key_modifiers.insert(*modifier);
    }
    keybinds
        .iter()
        .find(|(bound_key, _)| *bound_key == full_key)
        .map(|(_, actions)| actions.clone())
        .filter(|actions| !actions.is_empty())
}

#[derive(Debug, Default)]
pub struct ActionRunner {
    queues: BTreeMap<ClientId, (String, u64, VecDeque<Action>)>,
    next_id: u64,
}

impl ActionRunner {
    fn tag(owner: &str, id: u64) -> String {
        format!("{}:{}", owner, id)
    }

    fn context(owner: &str, id: u64) -> BTreeMap<String, String> {
        let mut context = BTreeMap::new();
        context.insert(CLICK_SEQUENCE_CONTEXT_KEY.to_owned(), Self::tag(owner, id));
        context
    }

    pub fn run(&mut self, owner: &str, client_id: ClientId, actions: Vec<Action>) {
        let mut queue: VecDeque<Action> = actions.into();
        let Some(first) = queue.pop_front() else {
            return;
        };
        self.next_id += 1;
        let id = self.next_id;
        if queue.is_empty() {
            self.queues.remove(&client_id);
        } else {
            self.queues.insert(client_id, (owner.to_owned(), id, queue));
        }
        run_action(first, Self::context(owner, id));
    }

    pub fn action_completed(&mut self, owner: &str, client_id: ClientId, event: &Event) -> bool {
        let Event::ActionComplete(_, _, context) = event else {
            return false;
        };
        let Some(tag) = context.get(CLICK_SEQUENCE_CONTEXT_KEY) else {
            return false;
        };
        let Some((queue_owner, id, queue)) = self.queues.get_mut(&client_id) else {
            return false;
        };
        if queue_owner != owner || *tag != Self::tag(owner, *id) {
            return false;
        }
        let id = *id;
        match queue.pop_front() {
            Some(next) => {
                if queue.is_empty() {
                    self.queues.remove(&client_id);
                }
                run_action(next, Self::context(owner, id));
            },
            None => {
                self.queues.remove(&client_id);
            },
        }
        true
    }

    #[cfg(test)]
    pub fn pending_for(&self, client_id: ClientId) -> usize {
        self.queues
            .get(&client_id)
            .map(|(_, _, queue)| queue.len())
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn completion(owner: &str, id: u64) -> Event {
        Event::ActionComplete(Action::NoOp, None, ActionRunner::context(owner, id))
    }

    #[test]
    fn actions_run_one_after_another() {
        let mut runner = ActionRunner::default();
        runner.run(
            "status",
            1,
            vec![Action::ScrollUp, Action::ScrollDown, Action::Detach],
        );
        assert_eq!(runner.pending_for(1), 2);
        assert!(!runner.action_completed("compact", 1, &completion("compact", 1)));
        assert!(!runner.action_completed("status", 2, &completion("status", 1)));
        assert_eq!(runner.pending_for(1), 2);
        assert!(runner.action_completed("status", 1, &completion("status", 1)));
        assert_eq!(runner.pending_for(1), 1);
        assert!(runner.action_completed("status", 1, &completion("status", 1)));
        assert_eq!(runner.pending_for(1), 0);
        assert!(!runner.action_completed("status", 1, &completion("status", 1)));
    }

    #[test]
    fn a_new_click_replaces_the_rest_of_an_older_sequence() {
        let mut runner = ActionRunner::default();
        runner.run("status", 1, vec![Action::ScrollUp, Action::ScrollDown]);
        runner.run("status", 1, vec![Action::Detach, Action::Quit]);
        assert!(!runner.action_completed("status", 1, &completion("status", 1)));
        assert!(runner.action_completed("status", 1, &completion("status", 2)));
        assert_eq!(runner.pending_for(1), 0);
    }

    #[test]
    fn single_actions_leave_nothing_pending() {
        let mut runner = ActionRunner::default();
        runner.run("status", 1, vec![Action::Detach]);
        assert_eq!(runner.pending_for(1), 0);
        runner.run("status", 1, vec![]);
        assert_eq!(runner.pending_for(1), 0);
    }

    #[test]
    fn keys_are_found_with_their_stripped_modifiers_restored() {
        let ctrl_p = KeyWithModifier::new(BareKey::Char('p')).with_ctrl_modifier();
        let keybinds = vec![
            (
                ctrl_p.clone(),
                vec![Action::SwitchToMode {
                    input_mode: InputMode::Pane,
                }],
            ),
            (KeyWithModifier::new(BareKey::Char('x')), vec![]),
        ];
        assert_eq!(
            actions_for_key(&keybinds, &ctrl_p, &[]),
            Some(vec![Action::SwitchToMode {
                input_mode: InputMode::Pane
            }])
        );
        assert_eq!(
            actions_for_key(
                &keybinds,
                &KeyWithModifier::new(BareKey::Char('p')),
                &[KeyModifier::Ctrl]
            ),
            Some(vec![Action::SwitchToMode {
                input_mode: InputMode::Pane
            }])
        );
        assert_eq!(
            actions_for_key(&keybinds, &KeyWithModifier::new(BareKey::Char('p')), &[]),
            None
        );
        assert_eq!(
            actions_for_key(&keybinds, &KeyWithModifier::new(BareKey::Char('x')), &[]),
            None
        );
    }

    #[test]
    fn regions_are_found_by_column_and_can_be_shifted() {
        let regions = vec![
            ClickRegion::new(2, 4, vec![Action::Detach]),
            ClickRegion::new(4, 5, vec![Action::Quit]),
        ];
        assert_eq!(region_at(&regions, 1), None);
        assert_eq!(region_at(&regions, 3).map(|r| r.start), Some(2));
        assert_eq!(region_at(&regions, 4).map(|r| r.start), Some(4));
        assert_eq!(regions[0].shifted(10).start, 12);
    }
}
