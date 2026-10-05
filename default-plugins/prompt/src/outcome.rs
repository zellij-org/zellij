use serde_json::{json, Value};
use zellij_utils::prompt::{parse_bool, EXIT_ANSWERED, EXIT_CANCELLED, EXIT_ERROR, EXIT_TIMEOUT};

use crate::request::{Request, Spec};

#[derive(Debug, Clone, PartialEq)]
pub enum Answer {
    Confirmed(bool),
    Text(String),
    Number(i64),
    Bool(bool),
    Choice(String),
    Choices(Vec<String>),
    Form(Vec<(String, Value)>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    Answered(Answer),
    Cancelled,
    TimedOut(Option<Answer>),
    Error(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Reply {
    pub output: String,
    pub exit_code: i32,
}

impl Answer {
    fn to_json(&self) -> Value {
        match self {
            Answer::Confirmed(yes) => Value::Bool(*yes),
            Answer::Text(text) => Value::String(text.clone()),
            Answer::Number(number) => json!(number),
            Answer::Bool(on) => Value::Bool(*on),
            Answer::Choice(choice) => Value::String(choice.clone()),
            Answer::Choices(choices) => {
                Value::Array(choices.iter().cloned().map(Value::String).collect())
            },
            Answer::Form(values) => Value::Object(values.iter().cloned().collect()),
        }
    }
    fn exit_code(&self) -> i32 {
        match self {
            Answer::Confirmed(false) => EXIT_CANCELLED,
            _ => EXIT_ANSWERED,
        }
    }
    fn to_text(&self, null: bool) -> String {
        let end = if null { "\0" } else { "\n" };
        match self {
            Answer::Confirmed(_) => String::new(),
            Answer::Text(text) => format!("{}{}", text, end),
            Answer::Number(number) => format!("{}{}", number, end),
            Answer::Bool(on) => format!("{}{}", on, end),
            Answer::Choice(choice) => format!("{}{}", choice, end),
            Answer::Choices(choices) => choices
                .iter()
                .map(|choice| format!("{}{}", choice, end))
                .collect(),
            Answer::Form(values) => format!("{}\n", ordered_object(values)),
        }
    }
}

pub fn ordered_object(values: &[(String, Value)]) -> String {
    let members: Vec<String> = values
        .iter()
        .map(|(key, value)| format!("{}:{}", Value::String(key.clone()), value))
        .collect();
    format!("{{{}}}", members.join(","))
}

pub fn default_answer(request: &Request) -> Result<Option<Answer>, String> {
    let Some(default) = &request.common.default else {
        return Ok(None);
    };
    let answer = match &request.spec {
        Spec::Confirm { .. } => Answer::Confirmed(parse_bool(default)?),
        Spec::Choose { multi: true, .. } => Answer::Choices(vec![default.clone()]),
        Spec::Choose { .. } | Spec::Select { .. } | Spec::Menu { .. } => {
            Answer::Choice(default.clone())
        },
        Spec::Input { .. } => Answer::Text(default.clone()),
        Spec::Number { .. } => Answer::Number(
            default
                .trim()
                .parse()
                .map_err(|_| format!("'{}' is not a whole number", default))?,
        ),
        Spec::Toggle { .. } => Answer::Bool(parse_bool(default)?),
        Spec::Form { default, .. } => {
            Answer::Form(default.clone().unwrap_or_default().into_iter().collect())
        },
        Spec::Notify { .. } => return Ok(None),
    };
    Ok(Some(answer))
}

pub fn reply_for(outcome: &Outcome, json_output: bool, null: bool) -> Reply {
    if json_output {
        let (value, exit_code) = match outcome {
            Outcome::Answered(Answer::Form(values)) => (
                Value::String(format!(
                    "{{\"result\":\"answered\",\"value\":{}}}",
                    ordered_object(values)
                )),
                EXIT_ANSWERED,
            ),
            Outcome::Answered(answer) => (
                json!({"result": "answered", "value": answer.to_json()}),
                answer.exit_code(),
            ),
            Outcome::Cancelled => (json!({"result": "cancelled"}), EXIT_CANCELLED),
            Outcome::TimedOut(Some(answer)) => (
                json!({"result": "timeout", "value": answer.to_json()}),
                answer.exit_code(),
            ),
            Outcome::TimedOut(None) => (json!({"result": "timeout"}), EXIT_TIMEOUT),
            Outcome::Error(message) => (json!({"result": "error", "error": message}), EXIT_ERROR),
        };
        let output = match value {
            Value::String(raw) => raw,
            other => other.to_string(),
        };
        return Reply {
            output: format!("{}\n", output),
            exit_code,
        };
    }
    match outcome {
        Outcome::Answered(answer) | Outcome::TimedOut(Some(answer)) => Reply {
            output: answer.to_text(null),
            exit_code: answer.exit_code(),
        },
        Outcome::Cancelled => Reply {
            output: String::new(),
            exit_code: EXIT_CANCELLED,
        },
        Outcome::TimedOut(None) => Reply {
            output: String::new(),
            exit_code: EXIT_TIMEOUT,
        },
        Outcome::Error(message) => Reply {
            output: format!("zellij prompt: {}\n", message),
            exit_code: EXIT_ERROR,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::request::parse_request;
    use std::collections::BTreeMap;

    fn request(name: &str, pairs: &[(&str, &str)]) -> Request {
        let args: BTreeMap<String, String> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        parse_request(name, &args, None).unwrap()
    }

    #[test]
    fn plain_text_output_for_each_answer() {
        let cases = vec![
            (Answer::Confirmed(true), "", 0),
            (Answer::Confirmed(false), "", 1),
            (Answer::Text("hi".to_owned()), "hi\n", 0),
            (Answer::Number(8080), "8080\n", 0),
            (Answer::Bool(true), "true\n", 0),
            (Answer::Bool(false), "false\n", 0),
            (Answer::Choice("main".to_owned()), "main\n", 0),
            (
                Answer::Choices(vec!["a".to_owned(), "b".to_owned()]),
                "a\nb\n",
                0,
            ),
            (Answer::Choices(vec![]), "", 0),
        ];
        for (answer, output, exit_code) in cases {
            let reply = reply_for(&Outcome::Answered(answer.clone()), false, false);
            assert_eq!(reply.output, output, "{:?}", answer);
            assert_eq!(reply.exit_code, exit_code, "{:?}", answer);
        }
    }

    #[test]
    fn nul_separated_output() {
        let reply = reply_for(
            &Outcome::Answered(Answer::Choices(vec!["a b".to_owned(), "c".to_owned()])),
            false,
            true,
        );
        assert_eq!(reply.output, "a b\0c\0");
    }

    #[test]
    fn form_output_is_one_json_object() {
        let values = vec![
            ("name".to_owned(), json!("demo")),
            ("license".to_owned(), json!("MIT")),
            ("ci".to_owned(), json!(true)),
            ("port".to_owned(), json!(8080)),
            ("files".to_owned(), json!(["a"])),
        ];
        let reply = reply_for(
            &Outcome::Answered(Answer::Form(values.clone())),
            false,
            false,
        );
        assert_eq!(
            reply.output,
            "{\"name\":\"demo\",\"license\":\"MIT\",\"ci\":true,\"port\":8080,\"files\":[\"a\"]}\n"
        );
        assert_eq!(reply.exit_code, 0);
        let reply = reply_for(&Outcome::Answered(Answer::Form(values)), true, false);
        assert!(reply
            .output
            .starts_with("{\"result\":\"answered\",\"value\":{\"name\":\"demo\""));
    }

    #[test]
    fn json_output_says_why_the_prompt_ended() {
        let answered = reply_for(
            &Outcome::Answered(Answer::Choice("x".to_owned())),
            true,
            false,
        );
        assert_eq!(
            answered.output,
            "{\"result\":\"answered\",\"value\":\"x\"}\n"
        );
        assert_eq!(answered.exit_code, 0);
        let cancelled = reply_for(&Outcome::Cancelled, true, false);
        assert_eq!(cancelled.output, "{\"result\":\"cancelled\"}\n");
        assert_eq!(cancelled.exit_code, 1);
        let timeout = reply_for(&Outcome::TimedOut(None), true, false);
        assert_eq!(timeout.output, "{\"result\":\"timeout\"}\n");
        assert_eq!(timeout.exit_code, 124);
        let refused = reply_for(&Outcome::Answered(Answer::Confirmed(false)), true, false);
        assert_eq!(
            refused.output,
            "{\"result\":\"answered\",\"value\":false}\n"
        );
        assert_eq!(refused.exit_code, 1);
    }

    #[test]
    fn exit_codes_for_cancel_timeout_and_error() {
        assert_eq!(reply_for(&Outcome::Cancelled, false, false).exit_code, 1);
        assert_eq!(
            reply_for(&Outcome::TimedOut(None), false, false).exit_code,
            124
        );
        let error = reply_for(&Outcome::Error("bad".to_owned()), false, false);
        assert_eq!(error.exit_code, 2);
        assert!(error.output.contains("bad"));
    }

    #[test]
    fn timeout_uses_the_default_when_given() {
        let with_default = request("input", &[("timeout", "1s"), ("default", "fallback")]);
        let answer = default_answer(&with_default).unwrap();
        let reply = reply_for(&Outcome::TimedOut(answer), false, false);
        assert_eq!(reply.output, "fallback\n");
        assert_eq!(reply.exit_code, 0);
        let without_default = request("input", &[("timeout", "1s")]);
        let reply = reply_for(
            &Outcome::TimedOut(default_answer(&without_default).unwrap()),
            false,
            false,
        );
        assert_eq!(reply.output, "");
        assert_eq!(reply.exit_code, 124);
        let confirm = request("confirm", &[("default", "no")]);
        let reply = reply_for(
            &Outcome::TimedOut(default_answer(&confirm).unwrap()),
            true,
            false,
        );
        assert_eq!(reply.output, "{\"result\":\"timeout\",\"value\":false}\n");
        assert_eq!(reply.exit_code, 1);
    }
}
