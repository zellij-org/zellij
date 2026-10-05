use crate::app::Effect;
use crate::outcome::{reply_for, Answer, Outcome};
use serde_json::Value;
use std::collections::BTreeMap;
use zellij_utils::prompt::{
    FormFieldKind, PipePrompt, PromptResult, PromptSpec, PromptValue, EXIT_ANSWERED, EXIT_CANCELLED,
};

#[derive(Debug, Clone, PartialEq)]
pub enum Destination {
    Cli {
        pipe_id: String,
        json: bool,
        null: bool,
    },
    Plugin {
        request_id: u64,
        field_kinds: BTreeMap<String, FormFieldKind>,
    },
}

impl Destination {
    pub fn cli(pipe_id: String) -> Self {
        Destination::Cli {
            pipe_id,
            json: false,
            null: false,
        }
    }
    pub fn plugin(request_id: u64) -> Self {
        Destination::Plugin {
            request_id,
            field_kinds: BTreeMap::new(),
        }
    }
    pub fn is_cli(&self) -> bool {
        matches!(self, Destination::Cli { .. })
    }
    pub fn for_request(self, pipe_prompt: &PipePrompt) -> Self {
        match self {
            Destination::Cli { pipe_id, .. } => Destination::Cli {
                pipe_id,
                json: pipe_prompt.json,
                null: pipe_prompt.null,
            },
            Destination::Plugin { request_id, .. } => Destination::Plugin {
                request_id,
                field_kinds: match &pipe_prompt.request.spec {
                    PromptSpec::Form { spec } => spec
                        .fields
                        .iter()
                        .map(|field| (field.id.clone(), field.kind))
                        .collect(),
                    _ => BTreeMap::new(),
                },
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Intent {
    Do(Effect),
    Hold(Destination),
    Pending(Destination),
    Acknowledge(Destination),
    NoticeClicked(Destination),
    Reply(Destination, Outcome),
}

pub fn expand(intents: Vec<Intent>) -> Vec<Effect> {
    let mut effects = vec![];
    for intent in intents {
        match intent {
            Intent::Do(effect) => effects.push(effect),
            Intent::Hold(Destination::Cli { pipe_id, .. }) => {
                effects.push(Effect::BlockPipe(pipe_id))
            },
            Intent::Pending(Destination::Cli { pipe_id, .. }) => {
                effects.push(Effect::SetExitCode(pipe_id, EXIT_CANCELLED))
            },
            Intent::Acknowledge(Destination::Cli { pipe_id, .. }) => {
                effects.push(Effect::SetExitCode(pipe_id.clone(), EXIT_ANSWERED));
                effects.push(Effect::UnblockPipe(pipe_id));
            },
            Intent::NoticeClicked(Destination::Plugin { request_id, .. }) => effects.push(
                Effect::ReplyToPrompt(request_id, PromptResult::Answered(PromptValue::Bool(true))),
            ),
            Intent::Reply(
                Destination::Cli {
                    pipe_id,
                    json,
                    null,
                },
                outcome,
            ) => {
                let reply = reply_for(&outcome, json, null);
                if !reply.output.is_empty() {
                    effects.push(Effect::Output(pipe_id.clone(), reply.output));
                }
                effects.push(Effect::SetExitCode(pipe_id.clone(), reply.exit_code));
                effects.push(Effect::UnblockPipe(pipe_id));
            },
            Intent::Reply(
                Destination::Plugin {
                    request_id,
                    field_kinds,
                },
                outcome,
            ) => effects.push(Effect::ReplyToPrompt(
                request_id,
                prompt_result(&outcome, &field_kinds),
            )),
            Intent::Hold(Destination::Plugin { .. })
            | Intent::Pending(Destination::Plugin { .. })
            | Intent::Acknowledge(Destination::Plugin { .. })
            | Intent::NoticeClicked(Destination::Cli { .. }) => {},
        }
    }
    effects
}

pub fn prompt_result(
    outcome: &Outcome,
    field_kinds: &BTreeMap<String, FormFieldKind>,
) -> PromptResult {
    match outcome {
        Outcome::Answered(Answer::Confirmed(yes)) => PromptResult::Confirmed(*yes),
        Outcome::Answered(answer) => PromptResult::Answered(prompt_value(answer, field_kinds)),
        Outcome::Cancelled => PromptResult::Cancelled,
        Outcome::TimedOut(answer) => PromptResult::TimedOut(
            answer
                .as_ref()
                .map(|answer| prompt_value(answer, field_kinds)),
        ),
        Outcome::Error(message) => PromptResult::Error(message.clone()),
    }
}

fn prompt_value(answer: &Answer, field_kinds: &BTreeMap<String, FormFieldKind>) -> PromptValue {
    match answer {
        Answer::Confirmed(yes) => PromptValue::Bool(*yes),
        Answer::Text(text) => PromptValue::Text(text.clone()),
        Answer::Number(number) => PromptValue::Number(*number),
        Answer::Bool(on) => PromptValue::Bool(*on),
        Answer::Choice(choice) => PromptValue::Choice(choice.clone()),
        Answer::Choices(choices) => PromptValue::Choices(choices.clone()),
        Answer::Form(values) => PromptValue::Form(
            values
                .iter()
                .filter_map(|(id, value)| {
                    form_value(value, field_kinds.get(id)).map(|value| (id.clone(), value))
                })
                .collect(),
        ),
    }
}

fn form_value(value: &Value, kind: Option<&FormFieldKind>) -> Option<PromptValue> {
    match value {
        Value::Null => None,
        Value::Bool(on) => Some(PromptValue::Bool(*on)),
        Value::Number(number) => {
            Some(PromptValue::Number(number.as_i64().unwrap_or_else(|| {
                number.as_f64().unwrap_or_default() as i64
            })))
        },
        Value::String(text) => match kind {
            Some(FormFieldKind::Select) | Some(FormFieldKind::Choose) => {
                Some(PromptValue::Choice(text.clone()))
            },
            _ => Some(PromptValue::Text(text.clone())),
        },
        Value::Array(values) => Some(PromptValue::Choices(
            values
                .iter()
                .map(|value| match value {
                    Value::String(text) => text.clone(),
                    other => other.to_string(),
                })
                .collect(),
        )),
        Value::Object(_) => Some(PromptValue::Text(value.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_form_answer_becomes_a_typed_map() {
        let mut field_kinds = BTreeMap::new();
        field_kinds.insert("name".to_owned(), FormFieldKind::Input);
        field_kinds.insert("license".to_owned(), FormFieldKind::Select);
        field_kinds.insert("ci".to_owned(), FormFieldKind::Toggle);
        field_kinds.insert("port".to_owned(), FormFieldKind::Number);
        field_kinds.insert("files".to_owned(), FormFieldKind::Choose);
        let answer = Answer::Form(vec![
            ("name".to_owned(), json!("app")),
            ("license".to_owned(), json!("MIT")),
            ("ci".to_owned(), json!(true)),
            ("port".to_owned(), json!(8080)),
            ("files".to_owned(), json!(["a", "b"])),
        ]);
        let mut expected = BTreeMap::new();
        expected.insert("name".to_owned(), PromptValue::Text("app".to_owned()));
        expected.insert("license".to_owned(), PromptValue::Choice("MIT".to_owned()));
        expected.insert("ci".to_owned(), PromptValue::Bool(true));
        expected.insert("port".to_owned(), PromptValue::Number(8080));
        expected.insert(
            "files".to_owned(),
            PromptValue::Choices(vec!["a".to_owned(), "b".to_owned()]),
        );
        assert_eq!(
            prompt_result(&Outcome::Answered(answer), &field_kinds),
            PromptResult::Answered(PromptValue::Form(expected))
        );
    }

    #[test]
    fn every_outcome_has_a_plugin_result() {
        let kinds = BTreeMap::new();
        assert_eq!(
            prompt_result(&Outcome::Answered(Answer::Confirmed(false)), &kinds),
            PromptResult::Confirmed(false)
        );
        assert_eq!(
            prompt_result(&Outcome::Cancelled, &kinds),
            PromptResult::Cancelled
        );
        assert_eq!(
            prompt_result(&Outcome::TimedOut(Some(Answer::Number(3))), &kinds),
            PromptResult::TimedOut(Some(PromptValue::Number(3)))
        );
        assert_eq!(
            prompt_result(&Outcome::Error("x".to_owned()), &kinds),
            PromptResult::Error("x".to_owned())
        );
        assert_eq!(
            expand(vec![Intent::Reply(
                Destination::plugin(4),
                Outcome::Cancelled
            )]),
            vec![Effect::ReplyToPrompt(4, PromptResult::Cancelled)]
        );
        assert!(expand(vec![
            Intent::Hold(Destination::plugin(4)),
            Intent::Pending(Destination::plugin(4)),
            Intent::Acknowledge(Destination::plugin(4)),
        ])
        .is_empty());
    }
}
