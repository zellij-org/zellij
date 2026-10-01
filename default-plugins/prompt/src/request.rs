use regex::Regex;
use serde_json::Value;
use std::collections::BTreeMap;
use std::str::FromStr;
use std::time::Duration;
use zellij_utils::data::PaneId;
use zellij_utils::prompt::{
    self, decode_items, decode_list, parse_bool, parse_duration, parse_form_spec, ChoiceItem,
    FormSpec, PromptElement,
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
        message: String,
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
        message: Option<String>,
        placeholder: Option<String>,
        validate: Option<Pattern>,
        required: bool,
        default: Option<String>,
    },
    Number {
        message: Option<String>,
        min: Option<i64>,
        max: Option<i64>,
        step: i64,
        default: Option<i64>,
    },
    Toggle {
        message: Option<String>,
        default: bool,
    },
    Select {
        message: String,
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
        message: String,
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
    Regex::new(pattern).map(Pattern).map_err(|e| e.to_string())
}

fn flag(args: &BTreeMap<String, String>, key: &str) -> Result<bool, String> {
    match args.get(key) {
        None => Ok(false),
        Some(value) if value.is_empty() => Ok(true),
        Some(value) => parse_bool(value).map_err(|e| format!("{}: {}", key, e)),
    }
}

fn integer(args: &BTreeMap<String, String>, key: &str) -> Result<Option<i64>, String> {
    match args.get(key) {
        None => Ok(None),
        Some(value) => value
            .trim()
            .parse::<i64>()
            .map(Some)
            .map_err(|_| format!("{}: '{}' is not a whole number", key, value)),
    }
}

pub fn strip_line_ending(payload: &str, null: bool) -> String {
    if null {
        return payload.trim_end_matches('\0').to_owned();
    }
    let without_newline = payload.strip_suffix('\n').unwrap_or(payload);
    without_newline
        .strip_suffix('\r')
        .unwrap_or(without_newline)
        .to_owned()
}

pub fn split_lines(payload: &str, null: bool, labels: bool) -> Vec<ChoiceItem> {
    let body = strip_line_ending(payload, null);
    let separator = if null { '\0' } else { '\n' };
    body.split(separator)
        .map(|line| {
            let line = if null {
                line
            } else {
                line.strip_suffix('\r').unwrap_or(line)
            };
            ChoiceItem::from_line(line, labels)
        })
        .collect()
}

pub fn parse_request(
    name: &str,
    args: &BTreeMap<String, String>,
    payload: Option<&str>,
) -> Result<Request, String> {
    let element = PromptElement::from_str(name)?;
    let timeout = match args.get(prompt::ARG_TIMEOUT) {
        Some(timeout) => Some(parse_duration(timeout).map_err(|e| format!("timeout: {}", e))?),
        None => None,
    };
    let default = args.get(prompt::ARG_DEFAULT).cloned();
    let mut common = Common {
        title: args.get(prompt::ARG_TITLE).cloned(),
        timeout,
        default: default.clone(),
        json: flag(args, prompt::ARG_JSON)?,
        in_popup: args.contains_key(prompt::CALLER_PANE_TITLE_ARG),
    };
    let message = args.get(prompt::ARG_MESSAGE).cloned();
    let spec = match element {
        PromptElement::Confirm => {
            let message = message
                .or_else(|| payload.map(|p| strip_line_ending(p, false)))
                .unwrap_or_else(|| "Are you sure?".to_owned());
            let default_yes = match &default {
                Some(default) => Some(parse_bool(default).map_err(|e| format!("default: {}", e))?),
                None => None,
            };
            Spec::Confirm {
                message,
                yes: args
                    .get(prompt::ARG_YES)
                    .cloned()
                    .unwrap_or_else(|| "Yes".to_owned()),
                no: args
                    .get(prompt::ARG_NO)
                    .cloned()
                    .unwrap_or_else(|| "No".to_owned()),
                default_yes,
            }
        },
        PromptElement::Choose => {
            let null = flag(args, prompt::ARG_NULL)?;
            let labels = flag(args, prompt::ARG_LABELS)?;
            let (items, streaming) = match args.get(prompt::ARG_ITEMS) {
                Some(items) => (decode_items(items), false),
                None => (
                    payload
                        .map(|p| split_lines(p, null, labels))
                        .unwrap_or_default(),
                    payload.is_some(),
                ),
            };
            Spec::Choose {
                items,
                multi: flag(args, prompt::ARG_MULTI)?,
                labels,
                null,
                selected: args
                    .get(prompt::ARG_SELECTED)
                    .map(|s| decode_list(s))
                    .unwrap_or_default(),
                streaming,
            }
        },
        PromptElement::Input => {
            let validate = match args.get(prompt::ARG_VALIDATE) {
                Some(pattern) => Some(compile(pattern).map_err(|e| format!("validate: {}", e))?),
                None => None,
            };
            Spec::Input {
                message,
                placeholder: args.get(prompt::ARG_PLACEHOLDER).cloned(),
                validate,
                required: flag(args, prompt::ARG_REQUIRED)?,
                default,
            }
        },
        PromptElement::Number => {
            let min = integer(args, prompt::ARG_MIN)?;
            let max = integer(args, prompt::ARG_MAX)?;
            if let (Some(min), Some(max)) = (min, max) {
                if min > max {
                    return Err("min is larger than max".to_owned());
                }
            }
            let step = integer(args, prompt::ARG_STEP)?.unwrap_or(1);
            if step <= 0 {
                return Err("step must be larger than 0".to_owned());
            }
            let default = match &default {
                Some(default) => Some(
                    default
                        .trim()
                        .parse::<i64>()
                        .map_err(|_| format!("default: '{}' is not a whole number", default))?,
                ),
                None => None,
            };
            if let Some(error) = default.and_then(|d| crate::ui::range_error(d, min, max)) {
                return Err(format!("default: {}", error.to_lowercase()));
            }
            Spec::Number {
                message,
                min,
                max,
                step,
                default,
            }
        },
        PromptElement::Toggle => Spec::Toggle {
            message,
            default: match &default {
                Some(default) => parse_bool(default).map_err(|e| format!("default: {}", e))?,
                None => false,
            },
        },
        PromptElement::Select => {
            let options = args
                .get(prompt::ARG_OPTIONS)
                .map(|o| decode_list(o))
                .unwrap_or_default();
            if options.is_empty() {
                return Err("select needs at least one option".to_owned());
            }
            let default = match &default {
                Some(default) => {
                    Some(options.iter().position(|o| o == default).ok_or_else(|| {
                        format!("default: '{}' is not one of the options", default)
                    })?)
                },
                None => None,
            };
            Spec::Select {
                message: message.unwrap_or_default(),
                options,
                default,
            }
        },
        PromptElement::Menu => {
            let items = args
                .get(prompt::ARG_ITEMS)
                .map(|i| decode_items(i))
                .unwrap_or_default();
            if items.is_empty() {
                return Err("menu needs at least one item".to_owned());
            }
            Spec::Menu { items }
        },
        PromptElement::Form => {
            let text = payload.ok_or_else(|| {
                "form needs the form description as the message payload".to_owned()
            })?;
            let spec = parse_form_spec(text)?;
            let patterns = spec
                .fields
                .iter()
                .map(|field| match &field.validate {
                    Some(pattern) => compile(pattern).map(Some).map_err(|e| {
                        format!(
                            "invalid form description: field \"{}\": invalid \"validate\" pattern: {}",
                            field.id, e
                        )
                    }),
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
            Spec::Form {
                spec,
                patterns,
                default,
            }
        },
        PromptElement::Notify => {
            let message = message
                .or_else(|| payload.map(|p| strip_line_ending(p, false)))
                .ok_or_else(|| "notify needs the text to show".to_owned())?;
            let show_pane_name = !flag(args, prompt::ARG_NO_PANE_NAME)?;
            let show_tab_name = !flag(args, prompt::ARG_NO_TAB_NAME)?;
            let caller_pane = args
                .get(prompt::CALLER_PANE_ID_ARG)
                .and_then(|pane_id| PaneId::from_str(pane_id).ok());
            Spec::Notify {
                message,
                pane_name: args.get(prompt::CALLER_PANE_TITLE_ARG).cloned(),
                tab_name: args.get(prompt::CALLER_TAB_NAME_ARG).cloned(),
                show_pane_name,
                show_tab_name,
                caller_pane,
            }
        },
    };
    if let Spec::Form {
        spec: form_spec, ..
    } = &spec
    {
        if common.title.is_none() {
            common.title = form_spec.title.clone();
        }
    }
    Ok(Request {
        element,
        common,
        spec,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

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
                message: "Force push?".to_owned(),
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
                message: "Build finished".to_owned(),
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
                message: "Saved".to_owned(),
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
                assert_eq!(message, "Continue?");
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
                message: "License".to_owned(),
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
