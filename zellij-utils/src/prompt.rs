use crate::data::{PaneId, PopupCorner, StyledText};
use regex::Regex;
use serde::{Deserialize, Serialize};
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
pub const CALLER_TAB_NAME_ARG: &str = "_caller_tab_name";

pub const ARG_MESSAGE: &str = "message";
pub const ARG_MESSAGE_STYLES: &str = "message_styles";
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
pub const ARG_NO_PANE_NAME: &str = "no_pane_name";
pub const ARG_NO_TAB_NAME: &str = "no_tab_name";

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
    Notify,
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
            PromptElement::Notify => "notify",
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
            "notify" => Ok(PromptElement::Notify),
            other => Err(format!(
                "unknown prompt element '{}', expected one of: confirm, choose, input, number, toggle, select, menu, form, notify",
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

impl From<&str> for ChoiceItem {
    fn from(value: &str) -> Self {
        ChoiceItem::plain(value)
    }
}

impl From<String> for ChoiceItem {
    fn from(value: String) -> Self {
        ChoiceItem::plain(value)
    }
}

impl From<(&str, &str)> for ChoiceItem {
    fn from((value, label): (&str, &str)) -> Self {
        ChoiceItem::labeled(value, label)
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
    pub message: Option<StyledText>,
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
    let message = match object.get("message") {
        None | Some(Value::Null) => None,
        Some(Value::String(s)) => Some(StyledText {
            text: s.clone(),
            indices: match object.get("message_styles") {
                None | Some(Value::Null) => vec![],
                Some(styles) => styles_from_json(styles).map_err(|e| {
                    format!("invalid form description: \"message_styles\" {}", e)
                })?,
            },
        }),
        Some(_) => return Err("invalid form description: \"message\" must be a string".to_owned()),
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
        message,
        fields,
        submit_label,
        cancel_label,
    })
}

pub fn styles_to_json(indices: &[Vec<usize>]) -> Value {
    Value::Array(
        indices
            .iter()
            .map(|level| Value::Array(level.iter().map(|index| Value::from(*index)).collect()))
            .collect(),
    )
}

pub fn styles_from_json(value: &Value) -> Result<Vec<Vec<usize>>, String> {
    let invalid = || "must be a list of lists of character positions".to_owned();
    value
        .as_array()
        .ok_or_else(invalid)?
        .iter()
        .map(|level| {
            level
                .as_array()
                .ok_or_else(invalid)?
                .iter()
                .map(|index| index.as_u64().map(|i| i as usize).ok_or_else(invalid))
                .collect()
        })
        .collect()
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

impl FormFieldKind {
    pub fn name(&self) -> &'static str {
        match self {
            FormFieldKind::Input => "input",
            FormFieldKind::Select => "select",
            FormFieldKind::Toggle => "toggle",
            FormFieldKind::Number => "number",
            FormFieldKind::Choose => "choose",
        }
    }
}

impl FormField {
    pub fn new(id: impl Into<String>, kind: FormFieldKind, label: impl Into<String>) -> Self {
        FormField {
            id: id.into(),
            kind,
            label: label.into(),
            required: false,
            validate: None,
            placeholder: None,
            options: vec![],
            multi: false,
            min: None,
            max: None,
            step: None,
            default: None,
        }
    }
    pub fn input(id: impl Into<String>, label: impl Into<String>) -> Self {
        FormField::new(id, FormFieldKind::Input, label)
    }
    pub fn toggle(id: impl Into<String>, label: impl Into<String>) -> Self {
        FormField::new(id, FormFieldKind::Toggle, label)
    }
    pub fn number(id: impl Into<String>, label: impl Into<String>) -> Self {
        FormField::new(id, FormFieldKind::Number, label)
    }
    pub fn select(
        id: impl Into<String>,
        label: impl Into<String>,
        options: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        FormField::new(id, FormFieldKind::Select, label).options(options)
    }
    pub fn choose(
        id: impl Into<String>,
        label: impl Into<String>,
        options: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        FormField::new(id, FormFieldKind::Choose, label).options(options)
    }
    pub fn options(mut self, options: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.options = options.into_iter().map(Into::into).collect();
        self
    }
    pub fn required(mut self) -> Self {
        self.required = true;
        self
    }
    pub fn multi(mut self) -> Self {
        self.multi = true;
        self
    }
    pub fn validate(mut self, pattern: impl Into<String>) -> Self {
        self.validate = Some(pattern.into());
        self
    }
    pub fn placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.placeholder = Some(placeholder.into());
        self
    }
    pub fn min(mut self, min: i64) -> Self {
        self.min = Some(min);
        self
    }
    pub fn max(mut self, max: i64) -> Self {
        self.max = Some(max);
        self
    }
    pub fn step(mut self, step: i64) -> Self {
        self.step = Some(step);
        self
    }
    pub fn default_value(mut self, value: impl Into<Value>) -> Self {
        self.default = Some(value.into());
        self
    }
    fn to_json(&self) -> Value {
        let mut object = Map::new();
        object.insert("id".to_owned(), Value::String(self.id.clone()));
        object.insert("type".to_owned(), Value::String(self.kind.name().to_owned()));
        object.insert("label".to_owned(), Value::String(self.label.clone()));
        if self.required {
            object.insert("required".to_owned(), Value::Bool(true));
        }
        if let Some(validate) = &self.validate {
            object.insert("validate".to_owned(), Value::String(validate.clone()));
        }
        if let Some(placeholder) = &self.placeholder {
            object.insert("placeholder".to_owned(), Value::String(placeholder.clone()));
        }
        if !self.options.is_empty() {
            object.insert(
                "options".to_owned(),
                Value::Array(self.options.iter().cloned().map(Value::String).collect()),
            );
        }
        if self.multi {
            object.insert("multi".to_owned(), Value::Bool(true));
        }
        for (key, value) in [("min", self.min), ("max", self.max), ("step", self.step)] {
            if let Some(value) = value {
                object.insert(key.to_owned(), Value::from(value));
            }
        }
        if let Some(default) = &self.default {
            object.insert("default".to_owned(), default.clone());
        }
        Value::Object(object)
    }
}

impl FormSpec {
    pub fn new(fields: Vec<FormField>) -> Self {
        FormSpec {
            title: None,
            message: None,
            fields,
            submit_label: "Save".to_owned(),
            cancel_label: "Cancel".to_owned(),
        }
    }
    pub fn to_json(&self) -> String {
        let mut object = Map::new();
        if let Some(title) = &self.title {
            object.insert("title".to_owned(), Value::String(title.clone()));
        }
        if let Some(message) = &self.message {
            object.insert("message".to_owned(), Value::String(message.text.clone()));
            if !message.is_plain() {
                object.insert("message_styles".to_owned(), styles_to_json(&message.indices));
            }
        }
        object.insert(
            "fields".to_owned(),
            Value::Array(self.fields.iter().map(|field| field.to_json()).collect()),
        );
        let mut buttons = Map::new();
        buttons.insert("submit".to_owned(), Value::String(self.submit_label.clone()));
        buttons.insert("cancel".to_owned(), Value::String(self.cancel_label.clone()));
        object.insert("buttons".to_owned(), Value::Object(buttons));
        Value::Object(object).to_string()
    }
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }
    pub fn message(mut self, message: impl Into<StyledText>) -> Self {
        self.message = Some(message.into());
        self
    }
    pub fn buttons(mut self, submit: impl Into<String>, cancel: impl Into<String>) -> Self {
        self.submit_label = submit.into();
        self.cancel_label = cancel.into();
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PromptPlacement {
    Pane(PaneId),
    Center,
    Mouse,
    Cursor,
    Corner(PopupCorner),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PromptValue {
    Text(String),
    Number(i64),
    Bool(bool),
    Choice(String),
    Choices(Vec<String>),
    Form(BTreeMap<String, PromptValue>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PromptResult {
    Answered(PromptValue),
    Confirmed(bool),
    Cancelled,
    TimedOut(Option<PromptValue>),
    Error(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum PromptSpec {
    Confirm {
        message: StyledText,
        yes: String,
        no: String,
    },
    Choose {
        items: Vec<ChoiceItem>,
        multi: bool,
        selected: Vec<String>,
    },
    Input {
        message: Option<StyledText>,
        placeholder: Option<String>,
        validate: Option<String>,
        required: bool,
    },
    Number {
        message: Option<StyledText>,
        min: Option<i64>,
        max: Option<i64>,
        step: i64,
    },
    Toggle {
        message: Option<StyledText>,
    },
    Select {
        message: StyledText,
        options: Vec<String>,
    },
    Menu {
        items: Vec<ChoiceItem>,
    },
    Form {
        spec: FormSpec,
    },
    Notify {
        message: StyledText,
        show_pane_name: bool,
        show_tab_name: bool,
    },
}

impl PromptSpec {
    pub fn element(&self) -> PromptElement {
        match self {
            PromptSpec::Confirm { .. } => PromptElement::Confirm,
            PromptSpec::Choose { .. } => PromptElement::Choose,
            PromptSpec::Input { .. } => PromptElement::Input,
            PromptSpec::Number { .. } => PromptElement::Number,
            PromptSpec::Toggle { .. } => PromptElement::Toggle,
            PromptSpec::Select { .. } => PromptElement::Select,
            PromptSpec::Menu { .. } => PromptElement::Menu,
            PromptSpec::Form { .. } => PromptElement::Form,
            PromptSpec::Notify { .. } => PromptElement::Notify,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PromptRequest {
    pub element: PromptElement,
    pub title: Option<String>,
    pub timeout: Option<Duration>,
    pub default: Option<String>,
    pub placement: Option<PromptPlacement>,
    pub focused: bool,
    pub spec: PromptSpec,
}

impl PromptRequest {
    pub fn new(spec: PromptSpec) -> Self {
        PromptRequest {
            element: spec.element(),
            title: None,
            timeout: None,
            default: None,
            placement: None,
            focused: !matches!(spec, PromptSpec::Notify { .. }),
            spec,
        }
    }
    /// A yes/no question. The answer arrives as `PromptResult::Confirmed(bool)`.
    pub fn confirm(message: impl Into<StyledText>) -> Self {
        PromptRequest::new(PromptSpec::Confirm {
            message: message.into(),
            yes: "Yes".to_owned(),
            no: "No".to_owned(),
        })
    }
    /// A list to pick from. The answer is `PromptValue::Choice`, or `PromptValue::Choices` with `multi()`.
    pub fn choose(items: impl IntoIterator<Item = impl Into<ChoiceItem>>) -> Self {
        PromptRequest::new(PromptSpec::Choose {
            items: items.into_iter().map(Into::into).collect(),
            multi: false,
            selected: vec![],
        })
    }
    /// A line of text. The answer is `PromptValue::Text`.
    pub fn input(message: impl Into<StyledText>) -> Self {
        PromptRequest::new(PromptSpec::Input {
            message: Some(message.into()),
            placeholder: None,
            validate: None,
            required: false,
        })
    }
    /// A whole number. The answer is `PromptValue::Number`.
    pub fn number(message: impl Into<StyledText>) -> Self {
        PromptRequest::new(PromptSpec::Number {
            message: Some(message.into()),
            min: None,
            max: None,
            step: 1,
        })
    }
    /// An on/off switch. The answer is `PromptValue::Bool`.
    pub fn toggle(message: impl Into<StyledText>) -> Self {
        PromptRequest::new(PromptSpec::Toggle {
            message: Some(message.into()),
        })
    }
    /// A dropdown. The answer is `PromptValue::Choice`.
    pub fn select(
        message: impl Into<StyledText>,
        options: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        PromptRequest::new(PromptSpec::Select {
            message: message.into(),
            options: options.into_iter().map(Into::into).collect(),
        })
    }
    /// A menu of actions. The answer is `PromptValue::Choice` with the value of the chosen item.
    pub fn menu(items: impl IntoIterator<Item = impl Into<ChoiceItem>>) -> Self {
        PromptRequest::new(PromptSpec::Menu {
            items: items.into_iter().map(Into::into).collect(),
        })
    }
    /// Several fields at once. The answer is `PromptValue::Form`, keyed by field id.
    pub fn form(fields: Vec<FormField>) -> Self {
        PromptRequest::form_spec(FormSpec::new(fields))
    }
    /// A form with its own title and button labels. The answer is `PromptValue::Form`.
    pub fn form_spec(spec: FormSpec) -> Self {
        PromptRequest::new(PromptSpec::Form { spec })
    }
    /// A notice that does not take focus, shown in the top right corner by default.
    /// No result is sent when it closes by timeout, by its close mark or by `close_self`;
    /// `PromptResult::Answered(PromptValue::Bool(true))` is sent only when the user clicks it.
    pub fn notify(message: impl Into<StyledText>) -> Self {
        PromptRequest::new(PromptSpec::Notify {
            message: message.into(),
            show_pane_name: true,
            show_tab_name: true,
        })
        .placement(PromptPlacement::Corner(PopupCorner::TopRight))
    }
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }
    /// The text shown above the fields of a form, or the question of the other elements.
    /// Pass a `Text` to colour parts of it.
    /// A form with a message is laid out as a dialog: centered, and Enter submits it.
    pub fn message(mut self, text: impl Into<StyledText>) -> Self {
        let text = text.into();
        match &mut self.spec {
            PromptSpec::Confirm { message, .. }
            | PromptSpec::Select { message, .. }
            | PromptSpec::Notify { message, .. } => *message = text,
            PromptSpec::Input { message, .. }
            | PromptSpec::Number { message, .. }
            | PromptSpec::Toggle { message } => *message = Some(text),
            PromptSpec::Form { spec } => spec.message = Some(text),
            PromptSpec::Choose { .. } | PromptSpec::Menu { .. } => {},
        }
        self
    }
    pub fn buttons(mut self, submit: impl Into<String>, cancel: impl Into<String>) -> Self {
        if let PromptSpec::Form { spec } = &mut self.spec {
            spec.submit_label = submit.into();
            spec.cancel_label = cancel.into();
        }
        self
    }
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }
    pub fn default(mut self, default: impl Into<String>) -> Self {
        self.default = Some(default.into());
        self
    }
    pub fn placement(mut self, placement: PromptPlacement) -> Self {
        self.placement = Some(placement);
        self
    }
    pub fn focused(mut self, focused: bool) -> Self {
        self.focused = focused;
        self
    }
    pub fn yes(mut self, label: impl Into<String>) -> Self {
        if let PromptSpec::Confirm { yes, .. } = &mut self.spec {
            *yes = label.into();
        }
        self
    }
    pub fn no(mut self, label: impl Into<String>) -> Self {
        if let PromptSpec::Confirm { no, .. } = &mut self.spec {
            *no = label.into();
        }
        self
    }

    pub fn multi(mut self) -> Self {
        if let PromptSpec::Choose { multi, .. } = &mut self.spec {
            *multi = true;
        }
        self
    }
    pub fn selected(mut self, values: impl IntoIterator<Item = impl Into<String>>) -> Self {
        if let PromptSpec::Choose { selected, .. } = &mut self.spec {
            *selected = values.into_iter().map(Into::into).collect();
        }
        self
    }
    pub fn placeholder(mut self, text: impl Into<String>) -> Self {
        if let PromptSpec::Input { placeholder, .. } = &mut self.spec {
            *placeholder = Some(text.into());
        }
        self
    }
    pub fn validate(mut self, pattern: impl Into<String>) -> Self {
        if let PromptSpec::Input { validate, .. } = &mut self.spec {
            *validate = Some(pattern.into());
        }
        self
    }
    pub fn required(mut self) -> Self {
        if let PromptSpec::Input { required, .. } = &mut self.spec {
            *required = true;
        }
        self
    }
    pub fn min(mut self, value: i64) -> Self {
        if let PromptSpec::Number { min, .. } = &mut self.spec {
            *min = Some(value);
        }
        self
    }
    pub fn max(mut self, value: i64) -> Self {
        if let PromptSpec::Number { max, .. } = &mut self.spec {
            *max = Some(value);
        }
        self
    }
    pub fn step(mut self, value: i64) -> Self {
        if let PromptSpec::Number { step, .. } = &mut self.spec {
            *step = value;
        }
        self
    }
    pub fn hide_pane_name(mut self) -> Self {
        if let PromptSpec::Notify { show_pane_name, .. } = &mut self.spec {
            *show_pane_name = false;
        }
        self
    }
    pub fn hide_tab_name(mut self) -> Self {
        if let PromptSpec::Notify { show_tab_name, .. } = &mut self.spec {
            *show_tab_name = false;
        }
        self
    }
    pub fn is_notice(&self) -> bool {
        matches!(self.spec, PromptSpec::Notify { .. })
    }
    pub fn check(&self) -> Result<(), String> {
        if self.element != self.spec.element() {
            return Err(format!(
                "the element is {} but the request describes a {}",
                self.element,
                self.spec.element()
            ));
        }
        let (name, args, payload) = self.to_pipe_message();
        parse_request(&name, &args, payload.as_deref()).map(|_| ())
    }
    pub fn to_pipe_message(&self) -> (String, BTreeMap<String, String>, Option<String>) {
        let mut args = BTreeMap::new();
        fn insert(args: &mut BTreeMap<String, String>, key: &str, value: String) {
            args.insert(key.to_owned(), value);
        }
        fn set_styled(args: &mut BTreeMap<String, String>, message: &StyledText) {
            insert(args, ARG_MESSAGE, message.text.clone());
            if !message.is_plain() {
                insert(args, ARG_MESSAGE_STYLES, styles_to_json(&message.indices).to_string());
            }
        }
        if let Some(title) = &self.title {
            insert(&mut args, ARG_TITLE, title.clone());
        }
        if let Some(timeout) = self.timeout {
            insert(&mut args, ARG_TIMEOUT, format!("{}ms", timeout.as_millis()));
        }
        if let Some(default) = &self.default {
            insert(&mut args, ARG_DEFAULT, default.clone());
        }
        let mut payload = None;
        match &self.spec {
            PromptSpec::Confirm { message, yes, no } => {
                set_styled(&mut args, message);
                insert(&mut args, ARG_YES, yes.clone());
                insert(&mut args, ARG_NO, no.clone());
            },
            PromptSpec::Choose {
                items,
                multi,
                selected,
            } => {
                insert(&mut args, ARG_ITEMS, encode_items(items));
                if *multi {
                    insert(&mut args, ARG_MULTI, "true".to_owned());
                }
                if !selected.is_empty() {
                    insert(&mut args, ARG_SELECTED, encode_list(selected));
                }
            },
            PromptSpec::Input {
                message,
                placeholder,
                validate,
                required,
            } => {
                if let Some(message) = message {
                    set_styled(&mut args, message);
                }
                if let Some(placeholder) = placeholder {
                    insert(&mut args, ARG_PLACEHOLDER, placeholder.clone());
                }
                if let Some(validate) = validate {
                    insert(&mut args, ARG_VALIDATE, validate.clone());
                }
                if *required {
                    insert(&mut args, ARG_REQUIRED, "true".to_owned());
                }
            },
            PromptSpec::Number {
                message,
                min,
                max,
                step,
            } => {
                if let Some(message) = message {
                    set_styled(&mut args, message);
                }
                if let Some(min) = min {
                    insert(&mut args, ARG_MIN, min.to_string());
                }
                if let Some(max) = max {
                    insert(&mut args, ARG_MAX, max.to_string());
                }
                insert(&mut args, ARG_STEP, step.to_string());
            },
            PromptSpec::Toggle { message } => {
                if let Some(message) = message {
                    set_styled(&mut args, message);
                }
            },
            PromptSpec::Select { message, options } => {
                if !message.text.is_empty() {
                    set_styled(&mut args, message);
                }
                insert(&mut args, ARG_OPTIONS, encode_list(options));
            },
            PromptSpec::Menu { items } => {
                insert(&mut args, ARG_ITEMS, encode_items(items));
            },
            PromptSpec::Form { spec } => {
                payload = Some(spec.to_json());
            },
            PromptSpec::Notify {
                message,
                show_pane_name,
                show_tab_name,
            } => {
                set_styled(&mut args, message);
                if !show_pane_name {
                    insert(&mut args, ARG_NO_PANE_NAME, "true".to_owned());
                }
                if !show_tab_name {
                    insert(&mut args, ARG_NO_TAB_NAME, "true".to_owned());
                }
            },
        }
        (self.spec.element().name().to_owned(), args, payload)
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct PromptCallerInfo {
    pub pane_id: Option<PaneId>,
    pub pane_title: Option<String>,
    pub tab_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PipePrompt {
    pub request: PromptRequest,
    pub json: bool,
    pub null: bool,
    pub labels: bool,
    pub streaming: bool,
    pub caller: PromptCallerInfo,
}

impl PipePrompt {
    pub fn in_popup(&self) -> bool {
        self.caller.pane_title.is_some()
    }
}

pub fn range_error(value: i64, min: Option<i64>, max: Option<i64>) -> Option<String> {
    match (min, max) {
        (Some(min), Some(max)) if value < min || value > max => {
            Some(format!("Must be between {} and {}", min, max))
        },
        (Some(min), None) if value < min => Some(format!("Must be at least {}", min)),
        (None, Some(max)) if value > max => Some(format!("Must be at most {}", max)),
        _ => None,
    }
}

pub fn compile_pattern(pattern: &str) -> Result<Regex, String> {
    Regex::new(pattern).map_err(|e| e.to_string())
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

fn parse_default_bool(default: &Option<String>) -> Result<Option<bool>, String> {
    match default {
        Some(default) => parse_bool(default)
            .map(Some)
            .map_err(|e| format!("default: {}", e)),
        None => Ok(None),
    }
}

pub fn parse_request(
    name: &str,
    args: &BTreeMap<String, String>,
    payload: Option<&str>,
) -> Result<PipePrompt, String> {
    let element = PromptElement::from_str(name)?;
    let timeout = match args.get(ARG_TIMEOUT) {
        Some(timeout) => Some(parse_duration(timeout).map_err(|e| format!("timeout: {}", e))?),
        None => None,
    };
    let default = args.get(ARG_DEFAULT).cloned();
    let json = flag(args, ARG_JSON)?;
    let message = args.get(ARG_MESSAGE).cloned();
    let message_styles = match args.get(ARG_MESSAGE_STYLES) {
        Some(styles) => serde_json::from_str::<Value>(styles)
            .map_err(|_| "message_styles: not valid JSON".to_owned())
            .and_then(|value| styles_from_json(&value))
            .map_err(|e| format!("message_styles: {}", e))?,
        None => vec![],
    };
    let styled = |text: String| StyledText {
        text,
        indices: message_styles.clone(),
    };
    let mut null = false;
    let mut labels = false;
    let mut streaming = false;
    let spec = match element {
        PromptElement::Confirm => {
            let message = message
                .or_else(|| payload.map(|p| strip_line_ending(p, false)))
                .unwrap_or_else(|| "Are you sure?".to_owned());
            parse_default_bool(&default)?;
            PromptSpec::Confirm {
                message: styled(message),
                yes: args
                    .get(ARG_YES)
                    .cloned()
                    .unwrap_or_else(|| "Yes".to_owned()),
                no: args.get(ARG_NO).cloned().unwrap_or_else(|| "No".to_owned()),
            }
        },
        PromptElement::Choose => {
            null = flag(args, ARG_NULL)?;
            labels = flag(args, ARG_LABELS)?;
            let items = match args.get(ARG_ITEMS) {
                Some(items) => decode_items(items),
                None => {
                    streaming = payload.is_some();
                    payload
                        .map(|p| split_lines(p, null, labels))
                        .unwrap_or_default()
                },
            };
            PromptSpec::Choose {
                items,
                multi: flag(args, ARG_MULTI)?,
                selected: args
                    .get(ARG_SELECTED)
                    .map(|s| decode_list(s))
                    .unwrap_or_default(),
            }
        },
        PromptElement::Input => {
            let validate = args.get(ARG_VALIDATE).cloned();
            if let Some(pattern) = &validate {
                compile_pattern(pattern).map_err(|e| format!("validate: {}", e))?;
            }
            PromptSpec::Input {
                message: message.map(styled),
                placeholder: args.get(ARG_PLACEHOLDER).cloned(),
                validate,
                required: flag(args, ARG_REQUIRED)?,
            }
        },
        PromptElement::Number => {
            let min = integer(args, ARG_MIN)?;
            let max = integer(args, ARG_MAX)?;
            if let (Some(min), Some(max)) = (min, max) {
                if min > max {
                    return Err("min is larger than max".to_owned());
                }
            }
            let step = integer(args, ARG_STEP)?.unwrap_or(1);
            if step <= 0 {
                return Err("step must be larger than 0".to_owned());
            }
            if let Some(default) = &default {
                let value = default
                    .trim()
                    .parse::<i64>()
                    .map_err(|_| format!("default: '{}' is not a whole number", default))?;
                if let Some(error) = range_error(value, min, max) {
                    return Err(format!("default: {}", error.to_lowercase()));
                }
            }
            PromptSpec::Number {
                message: message.map(styled),
                min,
                max,
                step,
            }
        },
        PromptElement::Toggle => {
            parse_default_bool(&default)?;
            PromptSpec::Toggle {
                message: message.map(styled),
            }
        },
        PromptElement::Select => {
            let options = args
                .get(ARG_OPTIONS)
                .map(|o| decode_list(o))
                .unwrap_or_default();
            if options.is_empty() {
                return Err("select needs at least one option".to_owned());
            }
            if let Some(default) = &default {
                if !options.iter().any(|o| o == default) {
                    return Err(format!("default: '{}' is not one of the options", default));
                }
            }
            PromptSpec::Select {
                message: styled(message.unwrap_or_default()),
                options,
            }
        },
        PromptElement::Menu => {
            let items = args
                .get(ARG_ITEMS)
                .map(|i| decode_items(i))
                .unwrap_or_default();
            if items.is_empty() {
                return Err("menu needs at least one item".to_owned());
            }
            PromptSpec::Menu { items }
        },
        PromptElement::Form => {
            let text = payload.ok_or_else(|| {
                "form needs the form description as the message payload".to_owned()
            })?;
            let spec = parse_form_spec(text)?;
            check_form_patterns(&spec, |pattern| compile_pattern(pattern).map(|_| ()))?;
            if let Some(default) = &default {
                match serde_json::from_str::<Value>(default) {
                    Ok(Value::Object(_)) => {},
                    _ => return Err("default: must be a JSON object for form".to_owned()),
                }
            }
            PromptSpec::Form { spec }
        },
        PromptElement::Notify => {
            let message = message
                .or_else(|| payload.map(|p| strip_line_ending(p, false)))
                .ok_or_else(|| "notify needs the text to show".to_owned())?;
            PromptSpec::Notify {
                message: styled(message),
                show_pane_name: !flag(args, ARG_NO_PANE_NAME)?,
                show_tab_name: !flag(args, ARG_NO_TAB_NAME)?,
            }
        },
    };
    let caller = PromptCallerInfo {
        pane_id: args
            .get(CALLER_PANE_ID_ARG)
            .and_then(|pane_id| PaneId::from_str(pane_id).ok()),
        pane_title: args.get(CALLER_PANE_TITLE_ARG).cloned(),
        tab_name: args.get(CALLER_TAB_NAME_ARG).cloned(),
    };
    let mut request = PromptRequest::new(spec);
    request.title = args.get(ARG_TITLE).cloned();
    request.timeout = timeout;
    request.default = default;
    Ok(PipePrompt {
        request,
        json,
        null,
        labels,
        streaming,
        caller,
    })
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
        assert_eq!(spec.message, None);
        let dialog = parse_form_spec(
            r#"{ "message": "Delete it?", "fields": [ { "id": "a", "type": "toggle" } ] }"#,
        )
        .unwrap();
        assert_eq!(dialog.message, Some(StyledText::plain("Delete it?")));
        assert_eq!(parse_form_spec(&dialog.to_json()), Ok(dialog));
        assert!(parse_form_spec(r#"{ "message": 1, "fields": [ { "id": "a", "type": "toggle" } ] }"#).is_err());
        let styled = parse_form_spec(
            r#"{ "message": "Delete Alt n?", "message_styles": [ [], [], [], [7, 8, 9, 10, 11] ], "fields": [ { "id": "a", "type": "toggle" } ] }"#,
        )
        .unwrap();
        assert_eq!(
            styled.message,
            Some(StyledText {
                text: "Delete Alt n?".to_owned(),
                indices: vec![vec![], vec![], vec![], vec![7, 8, 9, 10, 11]],
            })
        );
        assert_eq!(parse_form_spec(&styled.to_json()), Ok(styled));
        assert!(parse_form_spec(
            r#"{ "message": "x", "message_styles": [ [ "a" ] ], "fields": [ { "id": "a", "type": "toggle" } ] }"#
        )
        .is_err());
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

    fn every_kind_of_request() -> Vec<PromptRequest> {
        vec![
            PromptRequest::confirm("Delete branch?")
                .yes("Delete")
                .no("Cancel")
                .timeout(Duration::from_secs(30))
                .default("no")
                .placement(PromptPlacement::Center),
            PromptRequest::choose(vec![ChoiceItem::plain("a"), ChoiceItem::labeled("b", "Bee")])
                .multi()
                .selected(vec!["b"])
                .title("Pick"),
            PromptRequest::input("Name")
                .placeholder("your name")
                .validate("^[a-z]+$")
                .required()
                .default("x"),
            PromptRequest::number("Port")
                .min(1)
                .max(65535)
                .step(10)
                .default("8080"),
            PromptRequest::toggle("Enable").default("on"),
            PromptRequest::select("License", vec!["MIT", "Apache-2.0"]).default("MIT"),
            PromptRequest::menu(vec![("open", "Open"), ("delete", "Delete")]),
            PromptRequest::form(vec![FormField::toggle("again", "Don't ask again")])
                .title("Confirm")
                .message(StyledText {
                    text: "Delete Alt n from Normal mode?".to_owned(),
                    indices: vec![vec![], vec![], vec![], vec![7, 8, 9, 10, 11]],
                })
                .buttons("Delete", "Cancel"),
            PromptRequest::toggle(StyledText {
                text: "Enable CI".to_owned(),
                indices: vec![vec![7, 8]],
            }),
            PromptRequest::confirm(StyledText {
                text: "Delete main?".to_owned(),
                indices: vec![vec![], vec![], vec![], vec![7, 8, 9, 10]],
            }),
            PromptRequest::input(StyledText {
                text: "Name of the branch".to_owned(),
                indices: vec![vec![12, 13]],
            }),
            PromptRequest::select(
                StyledText {
                    text: "License".to_owned(),
                    indices: vec![vec![0]],
                },
                vec!["MIT"],
            ),
            PromptRequest::notify(StyledText {
                text: "Build failed".to_owned(),
                indices: vec![vec![], vec![], vec![], vec![6, 7, 8, 9, 10, 11]],
            }),
            PromptRequest::form(vec![
                FormField::input("name", "Name").required().validate("^[a-z]+$"),
                FormField::select("license", "License", vec!["MIT", "GPL"])
                    .default_value("MIT"),
                FormField::toggle("ci", "Use CI").default_value(true),
                FormField::number("port", "Port").min(1).max(10).step(1),
                FormField::choose("files", "Files", vec!["a", "b"]).multi(),
            ])
            .default(r#"{"name":"x"}"#),
            PromptRequest::notify("Build finished").hide_tab_name(),
        ]
    }

    #[test]
    fn every_builder_makes_a_valid_request() {
        for request in every_kind_of_request() {
            assert_eq!(request.check(), Ok(()), "{:?}", request);
            assert_eq!(request.element, request.spec.element());
        }
        assert!(!PromptRequest::notify("x").focused);
        assert!(PromptRequest::confirm("x").focused);
        assert_eq!(
            PromptRequest::notify("x").placement,
            Some(PromptPlacement::Corner(PopupCorner::TopRight))
        );
    }

    #[test]
    fn requests_survive_the_trip_through_pipe_arguments() {
        for request in every_kind_of_request() {
            let (name, args, payload) = request.to_pipe_message();
            let parsed = parse_request(&name, &args, payload.as_deref()).unwrap();
            let mut expected = request.clone();
            expected.placement = None;
            expected.focused = !request.is_notice();
            assert_eq!(parsed.request, expected);
            assert!(!parsed.json && !parsed.null && !parsed.streaming);
            assert!(!parsed.in_popup());
        }
    }

    #[test]
    fn cli_arguments_parse_into_the_shared_request() {
        let mut args = BTreeMap::new();
        args.insert(ARG_MESSAGE.to_owned(), "Go?".to_owned());
        args.insert(ARG_JSON.to_owned(), "true".to_owned());
        args.insert(CALLER_PANE_TITLE_ARG.to_owned(), "zsh".to_owned());
        args.insert(CALLER_PANE_ID_ARG.to_owned(), "terminal_3".to_owned());
        let parsed = parse_request("confirm", &args, None).unwrap();
        assert_eq!(parsed.request, PromptRequest::confirm("Go?"));
        assert!(parsed.json);
        assert!(parsed.in_popup());
        assert_eq!(parsed.caller.pane_id, Some(PaneId::Terminal(3)));
        let streamed = parse_request("choose", &BTreeMap::new(), Some("a\nb\n")).unwrap();
        assert!(streamed.streaming);
        assert_eq!(
            streamed.request.spec,
            PromptSpec::Choose {
                items: vec![ChoiceItem::plain("a"), ChoiceItem::plain("b")],
                multi: false,
                selected: vec![],
            }
        );
    }

    #[test]
    fn invalid_requests_are_refused_with_a_reason() {
        let bad_pattern = PromptRequest::input("x").validate("(");
        assert!(bad_pattern.check().unwrap_err().starts_with("validate:"));
        let bad_range = PromptRequest::number("x").min(5).max(1);
        assert!(bad_range.check().is_err());
        let bad_default = PromptRequest::select("x", vec!["a"]).default("b");
        assert!(bad_default.check().is_err());
        assert!(PromptRequest::menu(Vec::<&str>::new()).check().is_err());
        let mut mismatched = PromptRequest::confirm("x");
        mismatched.element = PromptElement::Menu;
        assert!(mismatched.check().is_err());
        let bad_form = PromptRequest::form(vec![FormField::input("a", "A").validate("(")]);
        assert!(bad_form.check().unwrap_err().contains("\"a\""));
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
