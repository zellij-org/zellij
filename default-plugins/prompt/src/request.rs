use regex::Regex;
use serde_json::Value;
use std::time::Duration;
use zellij_utils::data::{PaneId, StyledText};
pub use zellij_utils::prompt::split_lines;
#[cfg(test)]
use zellij_utils::prompt::strip_line_ending;
use zellij_utils::prompt::{
    self, parse_bool, ChoiceItem, FormSpec, PipePrompt, PromptElement, PromptSpec,
};

#[derive(Debug, Clone, PartialEq)]
pub struct Common {
    pub title: Option<String>,
    pub timeout: Option<Duration>,
    pub default: Option<String>,
    pub json: bool,
    pub in_popup: bool,
}

#[derive(Debug, Clone)]
pub struct Pattern(pub Regex);

impl PartialEq for Pattern {
    fn eq(&self, other: &Self) -> bool {
        self.0.as_str() == other.0.as_str()
    }
}

impl Pattern {
    pub fn is_match(&self, text: &str) -> bool {
        self.0.is_match(text)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Spec {
    Confirm {
        message: StyledText,
        yes: String,
        no: String,
        default_yes: Option<bool>,
    },
    Choose {
        items: Vec<ChoiceItem>,
        multi: bool,
        labels: bool,
        null: bool,
        selected: Vec<String>,
        streaming: bool,
    },
    Input {
        message: Option<StyledText>,
        placeholder: Option<String>,
        validate: Option<Pattern>,
        required: bool,
        default: Option<String>,
    },
    Number {
        message: Option<StyledText>,
        min: Option<i64>,
        max: Option<i64>,
        step: i64,
        default: Option<i64>,
    },
    Toggle {
        message: Option<StyledText>,
        default: bool,
    },
    Select {
        message: StyledText,
        options: Vec<String>,
        default: Option<usize>,
    },
    Menu {
        items: Vec<ChoiceItem>,
    },
    Form {
        spec: FormSpec,
        patterns: Vec<Option<Pattern>>,
        default: Option<serde_json::Map<String, Value>>,
    },
    Notify {
        message: StyledText,
        pane_name: Option<String>,
        tab_name: Option<String>,
        show_pane_name: bool,
        show_tab_name: bool,
        caller_pane: Option<PaneId>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Request {
    pub element: PromptElement,
    pub common: Common,
    pub spec: Spec,
}

fn compile(pattern: &str) -> Result<Pattern, String> {
    prompt::compile_pattern(pattern).map(Pattern)
}

fn parse_default<T>(
    default: &Option<String>,
    parse: impl Fn(&str) -> Result<T, String>,
) -> Result<Option<T>, String> {
    match default {
        Some(default) => parse(default)
            .map(Some)
            .map_err(|e| format!("default: {}", e)),
        None => Ok(None),
    }
}

#[cfg(test)]
pub fn parse_request(
    name: &str,
    args: &std::collections::BTreeMap<String, String>,
    payload: Option<&str>,
) -> Result<Request, String> {
    request_from_pipe_prompt(prompt::parse_request(name, args, payload)?)
}

pub fn request_from_pipe_prompt(pipe_prompt: PipePrompt) -> Result<Request, String> {
    let in_popup = pipe_prompt.in_popup();
    let PipePrompt {
        request,
        json,
        null,
        labels,
        streaming,
        caller,
    } = pipe_prompt;
    let default = request.default.clone();
    let mut common = Common {
        title: request.title.clone(),
        timeout: request.timeout,
        default: default.clone(),
        json,
        in_popup,
    };
    let spec = match request.spec {
        PromptSpec::Confirm { message, yes, no } => Spec::Confirm {
            message,
            yes,
            no,
            default_yes: parse_default(&default, parse_bool)?,
        },
        PromptSpec::Choose {
            items,
            multi,
            selected,
        } => Spec::Choose {
            items,
            multi,
            labels,
            null,
            selected,
            streaming,
        },
        PromptSpec::Input {
            message,
            placeholder,
            validate,
            required,
        } => Spec::Input {
            message,
            placeholder,
            validate: match validate {
                Some(pattern) => Some(compile(&pattern).map_err(|e| format!("validate: {}", e))?),
                None => None,
            },
            required,
            default,
        },
        PromptSpec::Number {
            message,
            min,
            max,
            step,
        } => Spec::Number {
            message,
            min,
            max,
            step,
            default: parse_default(&default, |d| {
                d.trim()
                    .parse::<i64>()
                    .map_err(|_| format!("'{}' is not a whole number", d))
            })?,
        },
        PromptSpec::Toggle { message } => Spec::Toggle {
            message,
            default: parse_default(&default, parse_bool)?.unwrap_or(false),
        },
        PromptSpec::Select { message, options } => {
            let default = default
                .as_ref()
                .and_then(|default| options.iter().position(|o| o == default));
            Spec::Select {
                message,
                options,
                default,
            }
        },
        PromptSpec::Menu { items } => Spec::Menu { items },
        PromptSpec::Form { spec } => {
            let patterns = spec
                .fields
                .iter()
                .map(|field| match &field.validate {
                    Some(pattern) => compile(pattern).map(Some),
                    None => Ok(None),
                })
                .collect::<Result<Vec<_>, _>>()?;
            let default = match &default {
                Some(default) => match serde_json::from_str::<Value>(default) {
                    Ok(Value::Object(object)) => Some(object),
                    _ => return Err("default: must be a JSON object for form".to_owned()),
                },
                None => None,
            };
            if common.title.is_none() {
                common.title = spec.title.clone();
            }
            Spec::Form {
                spec,
                patterns,
                default,
            }
        },
        PromptSpec::Notify {
            message,
            show_pane_name,
            show_tab_name,
        } => Spec::Notify {
            message,
            pane_name: caller.pane_title,
            tab_name: caller.tab_name,
            show_pane_name,
            show_tab_name,
            caller_pane: caller.pane_id,
        },
    };
    Ok(Request {
        element: request.element,
        common,
        spec,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn args(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn confirm_reads_labels_default_and_popup_marker() {
        let request = parse_request(
            "confirm",
            &args(&[
                ("message", "Force push?"),
                ("yes", "Push"),
                ("no", "Cancel"),
                ("default", "no"),
                ("_caller_pane_title", "zsh"),
            ]),
            None,
        )
        .unwrap();
        assert_eq!(
            request.spec,
            Spec::Confirm {
                message: StyledText::plain("Force push?"),
                yes: "Push".to_owned(),
                no: "Cancel".to_owned(),
                default_yes: Some(false),
            }
        );
        assert!(request.common.in_popup);
        let floating = parse_request("confirm", &args(&[]), None).unwrap();
        assert!(!floating.common.in_popup);
    }

    #[test]
    fn notify_reads_its_text_and_the_calling_pane_title() {
        let request = parse_request(
            "notify",
            &args(&[
                ("message", "Build finished"),
                ("timeout", "5s"),
                ("_caller_pane_title", "make"),
                ("_caller_tab_name", "Build"),
                ("_caller_pane_id", "terminal_3"),
            ]),
            None,
        )
        .unwrap();
        assert_eq!(
            request.spec,
            Spec::Notify {
                message: StyledText::plain("Build finished"),
                pane_name: Some("make".to_owned()),
                tab_name: Some("Build".to_owned()),
                show_pane_name: true,
                show_tab_name: true,
                caller_pane: Some(PaneId::Terminal(3)),
            }
        );
        let without_names = parse_request(
            "notify",
            &args(&[
                ("message", "Saved"),
                ("no_pane_name", "true"),
                ("no_tab_name", "true"),
            ]),
            None,
        )
        .unwrap();
        assert_eq!(
            without_names.spec,
            Spec::Notify {
                message: StyledText::plain("Saved"),
                pane_name: None,
                tab_name: None,
                show_pane_name: false,
                show_tab_name: false,
                caller_pane: None,
            }
        );
        assert_eq!(request.common.timeout, Some(Duration::from_secs(5)));
        assert!(parse_request("notify", &args(&[]), None).is_err());
    }

    #[test]
    fn confirm_takes_the_message_from_the_payload() {
        let request = parse_request("confirm", &args(&[]), Some("Continue?\n")).unwrap();
        match request.spec {
            Spec::Confirm { message, yes, .. } => {
                assert_eq!(message.text, "Continue?");
                assert_eq!(yes, "Yes");
            },
            other => panic!("unexpected {:?}", other),
        }
    }

    #[test]
    fn choose_streams_from_the_payload_when_no_items_are_given() {
        let request = parse_request(
            "choose",
            &args(&[("multi", "true"), ("labels", "true")]),
            Some("main\tMain branch\n"),
        )
        .unwrap();
        match request.spec {
            Spec::Choose {
                items,
                multi,
                streaming,
                ..
            } => {
                assert_eq!(items, vec![ChoiceItem::labeled("main", "Main branch")]);
                assert!(multi);
                assert!(streaming);
            },
            other => panic!("unexpected {:?}", other),
        }
    }

    #[test]
    fn choose_reads_items_and_selected_from_arguments() {
        let request = parse_request(
            "choose",
            &args(&[
                ("items", r#"["a",["b","Bee"]]"#),
                ("selected", r#"["b"]"#),
                ("null", "true"),
            ]),
            None,
        )
        .unwrap();
        match request.spec {
            Spec::Choose {
                items,
                selected,
                null,
                streaming,
                ..
            } => {
                assert_eq!(
                    items,
                    vec![ChoiceItem::plain("a"), ChoiceItem::labeled("b", "Bee")]
                );
                assert_eq!(selected, vec!["b".to_owned()]);
                assert!(null);
                assert!(!streaming);
            },
            other => panic!("unexpected {:?}", other),
        }
    }

    #[test]
    fn a_payload_with_several_lines_becomes_several_items() {
        assert_eq!(
            split_lines("a\r\nb\n\nc\n", false, false),
            vec![
                ChoiceItem::plain("a"),
                ChoiceItem::plain("b"),
                ChoiceItem::plain(""),
                ChoiceItem::plain("c")
            ]
        );
        assert_eq!(
            split_lines("x\ty\0z\0", true, true),
            vec![ChoiceItem::labeled("x", "y"), ChoiceItem::plain("z")]
        );
    }

    #[test]
    fn nul_separated_payloads_keep_newlines() {
        assert_eq!(strip_line_ending("a\nb\0", true), "a\nb");
        assert_eq!(strip_line_ending("a\r\n", false), "a");
        assert_eq!(strip_line_ending("a\n\n", false), "a\n");
    }

    #[test]
    fn input_rejects_a_bad_pattern() {
        let error = parse_request("input", &args(&[("validate", "(")]), None).unwrap_err();
        assert!(error.starts_with("validate:"), "{}", error);
    }

    #[test]
    fn number_checks_bounds_step_and_default() {
        assert!(parse_request("number", &args(&[("min", "5"), ("max", "1")]), None).is_err());
        assert!(parse_request("number", &args(&[("step", "0")]), None).is_err());
        assert!(parse_request("number", &args(&[("default", "x")]), None).is_err());
        assert!(parse_request(
            "number",
            &args(&[("min", "1"), ("max", "5"), ("default", "9")]),
            None
        )
        .is_err());
        let request = parse_request(
            "number",
            &args(&[("min", "1"), ("max", "65535"), ("default", "8080")]),
            None,
        )
        .unwrap();
        assert_eq!(
            request.spec,
            Spec::Number {
                message: None,
                min: Some(1),
                max: Some(65535),
                step: 1,
                default: Some(8080),
            }
        );
    }

    #[test]
    fn toggle_and_select_read_their_defaults() {
        let toggle = parse_request("toggle", &args(&[("default", "on")]), None).unwrap();
        assert_eq!(
            toggle.spec,
            Spec::Toggle {
                message: None,
                default: true
            }
        );
        let select = parse_request(
            "select",
            &args(&[
                ("message", "License"),
                ("options", r#"["MIT","Apache-2.0"]"#),
                ("default", "Apache-2.0"),
            ]),
            None,
        )
        .unwrap();
        assert_eq!(
            select.spec,
            Spec::Select {
                message: StyledText::plain("License"),
                options: vec!["MIT".to_owned(), "Apache-2.0".to_owned()],
                default: Some(1),
            }
        );
        assert!(parse_request(
            "select",
            &args(&[("options", "MIT|GPL"), ("default", "BSD")]),
            None
        )
        .is_err());
    }

    #[test]
    fn unknown_elements_and_bad_timeouts_are_errors() {
        assert!(parse_request("slider", &args(&[]), None).is_err());
        assert!(parse_request("confirm", &args(&[("timeout", "soon")]), None).is_err());
        let request = parse_request("confirm", &args(&[("timeout", "2s")]), None).unwrap();
        assert_eq!(request.common.timeout, Some(Duration::from_secs(2)));
    }

    #[test]
    fn form_errors_name_the_field() {
        let error = parse_request(
            "form",
            &args(&[]),
            Some(r#"{"fields":[{"id":"name","type":"input","validate":"("}]}"#),
        )
        .unwrap_err();
        assert!(error.contains("\"name\""), "{}", error);
        assert!(parse_request("form", &args(&[]), None).is_err());
    }
}
