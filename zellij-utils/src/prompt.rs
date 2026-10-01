use serde_json::{Map, Value};
use std::collections::{BTreeMap, HashSet};
use std::fmt;
use std::str::FromStr;
use std::time::Duration;

pub const PROMPT_PLUGIN_ALIAS: &str = "prompt";
pub const PROMPT_PLUGIN_URL: &str = "zellij:prompt";
pub const CALLER_ARG_PREFIX: &str = "_caller_";
pub const CALLER_PANE_ID_ARG: &str = "_caller_pane_id";
pub const CALLER_PANE_TITLE_ARG: &str = "_caller_pane_title";

pub const ARG_MESSAGE: &str = "message";
pub const ARG_TITLE: &str = "title";
pub const ARG_TIMEOUT: &str = "timeout";
pub const ARG_DEFAULT: &str = "default";
pub const ARG_JSON: &str = "json";
pub const ARG_YES: &str = "yes";
pub const ARG_NO: &str = "no";
pub const ARG_ITEMS: &str = "items";
pub const ARG_OPTIONS: &str = "options";
pub const ARG_SELECTED: &str = "selected";
pub const ARG_MULTI: &str = "multi";
pub const ARG_LABELS: &str = "labels";
pub const ARG_NULL: &str = "null";
pub const ARG_PLACEHOLDER: &str = "placeholder";
pub const ARG_VALIDATE: &str = "validate";
pub const ARG_REQUIRED: &str = "required";
pub const ARG_MIN: &str = "min";
pub const ARG_MAX: &str = "max";
pub const ARG_STEP: &str = "step";

pub const EXIT_ANSWERED: i32 = 0;
pub const EXIT_CANCELLED: i32 = 1;
pub const EXIT_ERROR: i32 = 2;
pub const EXIT_TIMEOUT: i32 = 124;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PromptElement {
    Confirm,
    Choose,
    Input,
    Number,
    Toggle,
    Select,
    Menu,
    Form,
}

impl PromptElement {
    pub fn name(&self) -> &'static str {
        match self {
            PromptElement::Confirm => "confirm",
            PromptElement::Choose => "choose",
            PromptElement::Input => "input",
            PromptElement::Number => "number",
            PromptElement::Toggle => "toggle",
            PromptElement::Select => "select",
            PromptElement::Menu => "menu",
            PromptElement::Form => "form",
        }
    }
}

impl fmt::Display for PromptElement {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.name())
    }
}

impl FromStr for PromptElement {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "confirm" => Ok(PromptElement::Confirm),
            "choose" => Ok(PromptElement::Choose),
            "input" => Ok(PromptElement::Input),
            "number" => Ok(PromptElement::Number),
            "toggle" => Ok(PromptElement::Toggle),
            "select" => Ok(PromptElement::Select),
            "menu" => Ok(PromptElement::Menu),
            "form" => Ok(PromptElement::Form),
            other => Err(format!(
                "unknown prompt element '{}', expected one of: confirm, choose, input, number, toggle, select, menu, form",
                other
            )),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChoiceItem {
    pub value: String,
    pub label: Option<String>,
}

impl ChoiceItem {
    pub fn plain(value: impl Into<String>) -> Self {
        ChoiceItem {
            value: value.into(),
            label: None,
        }
    }
    pub fn labeled(value: impl Into<String>, label: impl Into<String>) -> Self {
        ChoiceItem {
            value: value.into(),
            label: Some(label.into()),
        }
    }
    pub fn display(&self) -> &str {
        self.label.as_deref().unwrap_or(&self.value)
    }
    pub fn from_item_arg(arg: &str) -> Self {
        match arg.split_once('=') {
            Some((value, label)) => ChoiceItem::labeled(value, label),
            None => ChoiceItem::plain(arg),
        }
    }
    pub fn from_line(line: &str, with_labels: bool) -> Self {
        if with_labels {
            match line.split_once('\t') {
                Some((value, label)) => ChoiceItem::labeled(value, label),
                None => ChoiceItem::plain(line),
            }
        } else {
            ChoiceItem::plain(line)
        }
    }
}

pub fn encode_list(items: &[String]) -> String {
    Value::Array(items.iter().cloned().map(Value::String).collect()).to_string()
}

pub fn decode_list(encoded: &str) -> Vec<String> {
    match serde_json::from_str::<Value>(encoded) {
        Ok(Value::Array(values)) => values
            .into_iter()
            .map(|value| match value {
                Value::String(s) => s,
                other => other.to_string(),
            })
            .collect(),
        _ if encoded.is_empty() => vec![],
        _ => encoded.split('|').map(|s| s.to_owned()).collect(),
    }
}

pub fn encode_items(items: &[ChoiceItem]) -> String {
    Value::Array(
        items
            .iter()
            .map(|item| match &item.label {
                Some(label) => Value::Array(vec![
                    Value::String(item.value.clone()),
                    Value::String(label.clone()),
                ]),
                None => Value::String(item.value.clone()),
            })
            .collect(),
    )
    .to_string()
}

pub fn decode_items(encoded: &str) -> Vec<ChoiceItem> {
    match serde_json::from_str::<Value>(encoded) {
        Ok(Value::Array(values)) => values
            .into_iter()
            .map(|value| match value {
                Value::String(s) => ChoiceItem::plain(s),
                Value::Array(pair) => {
                    let mut parts = pair.into_iter().map(|part| match part {
                        Value::String(s) => s,
                        other => other.to_string(),
                    });
                    let value = parts.next().unwrap_or_default();
                    match parts.next() {
                        Some(label) => ChoiceItem::labeled(value, label),
                        None => ChoiceItem::plain(value),
                    }
                },
                other => ChoiceItem::plain(other.to_string()),
            })
            .collect(),
        _ if encoded.is_empty() => vec![],
        _ => encoded.split('|').map(ChoiceItem::plain).collect(),
    }
}

pub fn parse_bool(value: &str) -> Result<bool, String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "on" | "true" | "yes" | "y" | "1" => Ok(true),
        "off" | "false" | "no" | "n" | "0" => Ok(false),
        other => Err(format!("'{}' is not on/off or true/false", other)),
    }
}

pub fn parse_duration(value: &str) -> Result<Duration, String> {
    let value = value.trim();
    let invalid = || {
        format!(
            "invalid duration '{}', expected for example 30s, 500ms, 2m or 1h",
            value
        )
    };
    let split_at = value
        .find(|c: char| !(c.is_ascii_digit() || c == '.'))
        .unwrap_or(value.len());
    let (number, unit) = value.split_at(split_at);
    let number: f64 = number.parse().map_err(|_| invalid())?;
    if !number.is_finite() || number < 0.0 {
        return Err(invalid());
    }
    let seconds = match unit.trim() {
        "" | "s" | "sec" | "secs" | "second" | "seconds" => number,
        "ms" => number / 1000.0,
        "m" | "min" | "mins" | "minute" | "minutes" => number * 60.0,
        "h" | "hr" | "hour" | "hours" => number * 3600.0,
        _ => return Err(invalid()),
    };
    Ok(Duration::from_secs_f64(seconds))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormFieldKind {
    Input,
    Select,
    Toggle,
    Number,
    Choose,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FormField {
    pub id: String,
    pub kind: FormFieldKind,
    pub label: String,
    pub required: bool,
    pub validate: Option<String>,
    pub placeholder: Option<String>,
    pub options: Vec<String>,
    pub multi: bool,
    pub min: Option<i64>,
    pub max: Option<i64>,
    pub step: Option<i64>,
    pub default: Option<Value>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FormSpec {
    pub title: Option<String>,
    pub fields: Vec<FormField>,
    pub submit_label: String,
    pub cancel_label: String,
}

fn field_error(id: &str, message: impl fmt::Display) -> String {
    format!("invalid form description: field \"{}\": {}", id, message)
}

fn optional_string(object: &Map<String, Value>, key: &str, id: &str) -> Result<Option<String>, String> {
    match object.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => Ok(Some(s.clone())),
        Some(_) => Err(field_error(id, format!("\"{}\" must be a string", key))),
    }
}

fn optional_bool(object: &Map<String, Value>, key: &str, id: &str) -> Result<bool, String> {
    match object.get(key) {
        None | Some(Value::Null) => Ok(false),
        Some(Value::Bool(b)) => Ok(*b),
        Some(_) => Err(field_error(id, format!("\"{}\" must be true or false", key))),
    }
}

fn optional_integer(object: &Map<String, Value>, key: &str, id: &str) -> Result<Option<i64>, String> {
    match object.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Number(n)) => n
            .as_i64()
            .map(Some)
            .ok_or_else(|| field_error(id, format!("\"{}\" must be a whole number", key))),
        Some(_) => Err(field_error(id, format!("\"{}\" must be a number", key))),
    }
}

fn parse_field(index: usize, value: &Value, seen_ids: &mut HashSet<String>) -> Result<FormField, String> {
    let position = format!("#{}", index + 1);
    let object = value
        .as_object()
        .ok_or_else(|| field_error(&position, "must be an object"))?;
    let id = match object.get("id") {
        Some(Value::String(id)) if !id.trim().is_empty() => id.clone(),
        Some(Value::String(_)) => return Err(field_error(&position, "\"id\" must not be empty")),
        Some(_) => return Err(field_error(&position, "\"id\" must be a string")),
        None => return Err(field_error(&position, "missing \"id\"")),
    };
    if !seen_ids.insert(id.clone()) {
        return Err(field_error(&id, "the id is used by more than one field"));
    }
    let kind = match object.get("type") {
        Some(Value::String(kind)) => match kind.as_str() {
            "input" => FormFieldKind::Input,
            "select" => FormFieldKind::Select,
            "toggle" => FormFieldKind::Toggle,
            "number" => FormFieldKind::Number,
            "choose" => FormFieldKind::Choose,
            other => {
                return Err(field_error(
                    &id,
                    format!(
                        "unknown type \"{}\", expected input, select, toggle, number or choose",
                        other
                    ),
                ))
            },
        },
        Some(_) => return Err(field_error(&id, "\"type\" must be a string")),
        None => return Err(field_error(&id, "missing \"type\"")),
    };
    let label = optional_string(object, "label", &id)?.unwrap_or_else(|| id.clone());
    let required = optional_bool(object, "required", &id)?;
    let validate = optional_string(object, "validate", &id)?;
    let placeholder = optional_string(object, "placeholder", &id)?;
    let multi = optional_bool(object, "multi", &id)?;
    let min = optional_integer(object, "min", &id)?;
    let max = optional_integer(object, "max", &id)?;
    let step = optional_integer(object, "step", &id)?;
    let options = match object.get("options") {
        None | Some(Value::Null) => vec![],
        Some(Value::Array(values)) => values
            .iter()
            .map(|value| match value {
                Value::String(s) => Ok(s.clone()),
                _ => Err(field_error(&id, "\"options\" must be a list of strings")),
            })
            .collect::<Result<Vec<_>, _>>()?,
        Some(_) => return Err(field_error(&id, "\"options\" must be a list of strings")),
    };
    let default = match object.get("default") {
        None | Some(Value::Null) => None,
        Some(value) => Some(value.clone()),
    };
    match kind {
        FormFieldKind::Select | FormFieldKind::Choose if options.is_empty() => {
            return Err(field_error(&id, "\"options\" must list at least one option"));
        },
        _ => {},
    }
    if let (Some(min), Some(max)) = (min, max) {
        if min > max {
            return Err(field_error(&id, "\"min\" is larger than \"max\""));
        }
    }
    if let Some(step) = step {
        if step <= 0 {
            return Err(field_error(&id, "\"step\" must be larger than 0"));
        }
    }
    if let Some(default) = &default {
        match kind {
            FormFieldKind::Input => {
                if !default.is_string() {
                    return Err(field_error(&id, "\"default\" must be a string"));
                }
            },
            FormFieldKind::Select => match default.as_str() {
                Some(s) if options.iter().any(|o| o == s) => {},
                Some(s) => {
                    return Err(field_error(
                        &id,
                        format!("\"default\" \"{}\" is not one of the options", s),
                    ))
                },
                None => return Err(field_error(&id, "\"default\" must be a string")),
            },
            FormFieldKind::Toggle => {
                if !default.is_boolean() {
                    return Err(field_error(&id, "\"default\" must be true or false"));
                }
            },
            FormFieldKind::Number => match default.as_i64() {
                Some(n) => {
                    if min.map(|min| n < min).unwrap_or(false)
                        || max.map(|max| n > max).unwrap_or(false)
                    {
                        return Err(field_error(&id, "\"default\" is outside \"min\"..\"max\""));
                    }
                },
                None => return Err(field_error(&id, "\"default\" must be a whole number")),
            },
            FormFieldKind::Choose => {
                let defaults: Vec<&str> = match default {
                    Value::String(s) => vec![s.as_str()],
                    Value::Array(values) => values.iter().filter_map(|v| v.as_str()).collect(),
                    _ => {
                        return Err(field_error(
                            &id,
                            "\"default\" must be a string or a list of strings",
                        ))
                    },
                };
                if let Some(missing) = defaults.iter().find(|d| !options.iter().any(|o| o == *d)) {
                    return Err(field_error(
                        &id,
                        format!("\"default\" \"{}\" is not one of the options", missing),
                    ));
                }
                if !multi && defaults.len() > 1 {
                    return Err(field_error(
                        &id,
                        "\"default\" lists several options but \"multi\" is not set",
                    ));
                }
            },
        }
    }
    Ok(FormField {
        id,
        kind,
        label,
        required,
        validate,
        placeholder,
        options,
        multi,
        min,
        max,
        step,
        default,
    })
}

pub fn parse_form_spec(text: &str) -> Result<FormSpec, String> {
    let value: Value = serde_json::from_str(text)
        .map_err(|e| format!("invalid form description: not valid JSON: {}", e))?;
    let object = value
        .as_object()
        .ok_or_else(|| "invalid form description: must be a JSON object".to_owned())?;
    let title = match object.get("title") {
        None | Some(Value::Null) => None,
        Some(Value::String(s)) => Some(s.clone()),
        Some(_) => return Err("invalid form description: \"title\" must be a string".to_owned()),
    };
    let fields = match object.get("fields") {
        Some(Value::Array(fields)) if !fields.is_empty() => fields,
        Some(Value::Array(_)) => {
            return Err("invalid form description: \"fields\" must not be empty".to_owned())
        },
        Some(_) => return Err("invalid form description: \"fields\" must be a list".to_owned()),
        None => return Err("invalid form description: missing \"fields\"".to_owned()),
    };
    let mut seen_ids = HashSet::new();
    let fields = fields
        .iter()
        .enumerate()
        .map(|(index, field)| parse_field(index, field, &mut seen_ids))
        .collect::<Result<Vec<_>, _>>()?;
    let (submit_label, cancel_label) = match object.get("buttons") {
        None | Some(Value::Null) => ("Save".to_owned(), "Cancel".to_owned()),
        Some(Value::Object(buttons)) => {
            let label = |key: &str, fallback: &str| -> Result<String, String> {
                match buttons.get(key) {
                    None | Some(Value::Null) => Ok(fallback.to_owned()),
                    Some(Value::String(s)) if !s.is_empty() => Ok(s.clone()),
                    Some(_) => Err(format!(
                        "invalid form description: \"buttons.{}\" must be a non-empty string",
                        key
                    )),
                }
            };
            (label("submit", "Save")?, label("cancel", "Cancel")?)
        },
        Some(_) => return Err("invalid form description: \"buttons\" must be an object".to_owned()),
    };
    Ok(FormSpec {
        title,
        fields,
        submit_label,
        cancel_label,
    })
}

pub fn check_form_patterns(
    spec: &FormSpec,
    compile: impl Fn(&str) -> Result<(), String>,
) -> Result<(), String> {
    for field in &spec.fields {
        if let Some(pattern) = &field.validate {
            compile(pattern).map_err(|e| {
                field_error(&field.id, format!("invalid \"validate\" pattern: {}", e))
            })?;
        }
    }
    Ok(())
}

pub fn strip_caller_args(args: &mut BTreeMap<String, String>) {
    args.retain(|key, _| !key.starts_with(CALLER_ARG_PREFIX));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations_accept_common_units() {
        assert_eq!(parse_duration("30s"), Ok(Duration::from_secs(30)));
        assert_eq!(parse_duration("500ms"), Ok(Duration::from_millis(500)));
        assert_eq!(parse_duration("2m"), Ok(Duration::from_secs(120)));
        assert_eq!(parse_duration("1h"), Ok(Duration::from_secs(3600)));
        assert_eq!(parse_duration("1.5"), Ok(Duration::from_millis(1500)));
        assert!(parse_duration("soon").is_err());
        assert!(parse_duration("5 parsecs").is_err());
    }

    #[test]
    fn lists_round_trip_and_accept_pipe_separated_text() {
        let items = vec!["a,b".to_owned(), "c|d".to_owned()];
        assert_eq!(decode_list(&encode_list(&items)), items);
        assert_eq!(decode_list("x|y"), vec!["x".to_owned(), "y".to_owned()]);
        assert!(decode_list("").is_empty());
    }

    #[test]
    fn labeled_items_round_trip() {
        let items = vec![ChoiceItem::plain("a"), ChoiceItem::labeled("b", "Bee")];
        assert_eq!(decode_items(&encode_items(&items)), items);
        assert_eq!(ChoiceItem::from_item_arg("v=Label"), ChoiceItem::labeled("v", "Label"));
        assert_eq!(ChoiceItem::from_line("v\tLabel", true), ChoiceItem::labeled("v", "Label"));
        assert_eq!(ChoiceItem::from_line("v\tLabel", false), ChoiceItem::plain("v\tLabel"));
    }

    #[test]
    fn the_example_form_description_is_accepted() {
        let spec = parse_form_spec(
            r#"{ "title": "New project",
              "fields": [
                { "id": "name", "type": "input", "label": "Name", "validate": "^[a-z-]+$", "required": true },
                { "id": "license", "type": "select", "label": "License", "options": ["MIT", "Apache-2.0"], "default": "MIT" },
                { "id": "ci", "type": "toggle", "label": "Use CI", "default": true },
                { "id": "port", "type": "number", "label": "Port", "min": 1, "max": 65535, "default": 8080 },
                { "id": "files", "type": "choose", "label": "Files", "options": ["a", "b"], "multi": true } ],
              "buttons": { "submit": "Create", "cancel": "Cancel" } }"#,
        )
        .unwrap();
        assert_eq!(spec.title.as_deref(), Some("New project"));
        assert_eq!(spec.fields.len(), 5);
        assert_eq!(spec.submit_label, "Create");
        assert_eq!(spec.fields[3].kind, FormFieldKind::Number);
    }

    #[test]
    fn form_errors_name_the_bad_field() {
        let error = parse_form_spec(
            r#"{ "fields": [ { "id": "port", "type": "number", "default": "x" } ] }"#,
        )
        .unwrap_err();
        assert!(error.contains("\"port\""), "{}", error);
        let error = parse_form_spec(
            r#"{ "fields": [ { "id": "license", "type": "select", "options": ["MIT"], "default": "GPL" } ] }"#,
        )
        .unwrap_err();
        assert!(error.contains("\"license\"") && error.contains("GPL"), "{}", error);
        let error = parse_form_spec(r#"{ "fields": [ { "id": "a", "type": "slider" } ] }"#)
            .unwrap_err();
        assert!(error.contains("\"a\"") && error.contains("slider"), "{}", error);
        let error = parse_form_spec(r#"{ "fields": [ { "type": "input" } ] }"#).unwrap_err();
        assert!(error.contains("#1"), "{}", error);
        let error = parse_form_spec(
            r#"{ "fields": [ { "id": "a", "type": "input" }, { "id": "a", "type": "toggle" } ] }"#,
        )
        .unwrap_err();
        assert!(error.contains("\"a\""), "{}", error);
        assert!(parse_form_spec("not json").is_err());
        assert!(parse_form_spec(r#"{ "fields": [] }"#).is_err());
    }

    #[test]
    fn bad_patterns_are_reported_with_the_field() {
        let spec = parse_form_spec(
            r#"{ "fields": [ { "id": "name", "type": "input", "validate": "(" } ] }"#,
        )
        .unwrap();
        let error = check_form_patterns(&spec, |p| {
            if p == "(" {
                Err("unclosed group".to_owned())
            } else {
                Ok(())
            }
        })
        .unwrap_err();
        assert!(error.contains("\"name\"") && error.contains("unclosed group"));
    }

    #[test]
    fn caller_args_are_stripped() {
        let mut args = BTreeMap::new();
        args.insert(CALLER_PANE_TITLE_ARG.to_owned(), "fake".to_owned());
        args.insert(ARG_TITLE.to_owned(), "t".to_owned());
        strip_caller_args(&mut args);
        assert_eq!(args.len(), 1);
    }
}
