pub use super::generated_api::api::prompt::{
    prompt_request, prompt_result, prompt_value, ChoiceItem as ProtobufChoiceItem,
    ChooseSpec as ProtobufChooseSpec, ConfirmSpec as ProtobufConfirmSpec,
    FormField as ProtobufFormField, FormFieldKind as ProtobufFormFieldKind,
    FormSpec as ProtobufFormSpec, InputSpec as ProtobufInputSpec, MenuSpec as ProtobufMenuSpec,
    NotifySpec as ProtobufNotifySpec, NumberSpec as ProtobufNumberSpec,
    PromptCancelled as ProtobufPromptCancelled, PromptFormEntry as ProtobufPromptFormEntry,
    PromptFormValue as ProtobufPromptFormValue, PromptPlacement as ProtobufPromptPlacement,
    PromptPlacementKind as ProtobufPromptPlacementKind, PromptRequest as ProtobufPromptRequest,
    PromptResult as ProtobufPromptResult, PromptTimedOut as ProtobufPromptTimedOut,
    PromptValue as ProtobufPromptValue, PromptValueList as ProtobufPromptValueList,
    SelectSpec as ProtobufSelectSpec, ToggleSpec as ProtobufToggleSpec,
};
use crate::data::PaneId;
use crate::prompt::{
    ChoiceItem, FormField, FormFieldKind, FormSpec, PromptElement, PromptPlacement, PromptRequest,
    PromptResult, PromptSpec, PromptValue,
};

use std::convert::TryFrom;
use std::str::FromStr;
use std::time::Duration;

impl From<ChoiceItem> for ProtobufChoiceItem {
    fn from(item: ChoiceItem) -> Self {
        ProtobufChoiceItem {
            value: item.value,
            label: item.label,
            emphasis: item.emphasis.into_iter().map(|index| index as u32).collect(),
        }
    }
}

impl From<ProtobufChoiceItem> for ChoiceItem {
    fn from(item: ProtobufChoiceItem) -> Self {
        ChoiceItem {
            value: item.value,
            label: item.label,
            emphasis: item
                .emphasis
                .into_iter()
                .map(|index| index as usize)
                .collect(),
        }
    }
}

impl From<FormFieldKind> for ProtobufFormFieldKind {
    fn from(kind: FormFieldKind) -> Self {
        match kind {
            FormFieldKind::Input => ProtobufFormFieldKind::Input,
            FormFieldKind::Select => ProtobufFormFieldKind::Select,
            FormFieldKind::Toggle => ProtobufFormFieldKind::Toggle,
            FormFieldKind::Number => ProtobufFormFieldKind::Number,
            FormFieldKind::Choose => ProtobufFormFieldKind::Choose,
        }
    }
}

impl From<ProtobufFormFieldKind> for FormFieldKind {
    fn from(kind: ProtobufFormFieldKind) -> Self {
        match kind {
            ProtobufFormFieldKind::Input => FormFieldKind::Input,
            ProtobufFormFieldKind::Select => FormFieldKind::Select,
            ProtobufFormFieldKind::Toggle => FormFieldKind::Toggle,
            ProtobufFormFieldKind::Number => FormFieldKind::Number,
            ProtobufFormFieldKind::Choose => FormFieldKind::Choose,
        }
    }
}

impl From<FormField> for ProtobufFormField {
    fn from(field: FormField) -> Self {
        ProtobufFormField {
            id: field.id,
            kind: ProtobufFormFieldKind::from(field.kind) as i32,
            label: field.label,
            required: field.required,
            validate: field.validate,
            placeholder: field.placeholder,
            options: field.options,
            multi: field.multi,
            min: field.min,
            max: field.max,
            step: field.step,
            default_json: field.default.map(|value| value.to_string()),
        }
    }
}

impl TryFrom<ProtobufFormField> for FormField {
    type Error = &'static str;
    fn try_from(field: ProtobufFormField) -> Result<Self, &'static str> {
        let kind = ProtobufFormFieldKind::try_from(field.kind)
            .map_err(|_| "Invalid form field kind")?
            .into();
        let default = match field.default_json {
            Some(text) => {
                Some(serde_json::from_str(&text).map_err(|_| "Invalid form field default")?)
            },
            None => None,
        };
        Ok(FormField {
            id: field.id,
            kind,
            label: field.label,
            required: field.required,
            validate: field.validate,
            placeholder: field.placeholder,
            options: field.options,
            multi: field.multi,
            min: field.min,
            max: field.max,
            step: field.step,
            default,
        })
    }
}

impl From<FormSpec> for ProtobufFormSpec {
    fn from(spec: FormSpec) -> Self {
        ProtobufFormSpec {
            title: spec.title,
            message: spec.message.map(Into::into),
            fields: spec.fields.into_iter().map(Into::into).collect(),
            submit_label: spec.submit_label,
            cancel_label: spec.cancel_label,
        }
    }
}

impl TryFrom<ProtobufFormSpec> for FormSpec {
    type Error = &'static str;
    fn try_from(spec: ProtobufFormSpec) -> Result<Self, &'static str> {
        Ok(FormSpec {
            title: spec.title,
            message: spec.message.map(Into::into),
            fields: spec
                .fields
                .into_iter()
                .map(FormField::try_from)
                .collect::<Result<Vec<_>, _>>()?,
            submit_label: spec.submit_label,
            cancel_label: spec.cancel_label,
        })
    }
}

impl From<PromptPlacement> for ProtobufPromptPlacement {
    fn from(placement: PromptPlacement) -> Self {
        let mut protobuf_placement = ProtobufPromptPlacement {
            kind: ProtobufPromptPlacementKind::Pane as i32,
            pane_id: None,
            pane_is_plugin: false,
            corner: None,
        };
        match placement {
            PromptPlacement::Pane(PaneId::Terminal(id)) => {
                protobuf_placement.pane_id = Some(id);
            },
            PromptPlacement::Pane(PaneId::Plugin(id)) => {
                protobuf_placement.pane_id = Some(id);
                protobuf_placement.pane_is_plugin = true;
            },
            PromptPlacement::Center => {
                protobuf_placement.kind = ProtobufPromptPlacementKind::Center as i32;
            },
            PromptPlacement::Mouse => {
                protobuf_placement.kind = ProtobufPromptPlacementKind::Mouse as i32;
            },
            PromptPlacement::Cursor => {
                protobuf_placement.kind = ProtobufPromptPlacementKind::Cursor as i32;
            },
            PromptPlacement::Corner(corner) => {
                protobuf_placement.kind = ProtobufPromptPlacementKind::Corner as i32;
                protobuf_placement.corner = Some(corner.to_string());
            },
        }
        protobuf_placement
    }
}

impl TryFrom<ProtobufPromptPlacement> for PromptPlacement {
    type Error = &'static str;
    fn try_from(placement: ProtobufPromptPlacement) -> Result<Self, &'static str> {
        match ProtobufPromptPlacementKind::try_from(placement.kind)
            .map_err(|_| "Invalid prompt placement")?
        {
            ProtobufPromptPlacementKind::Pane => {
                let id = placement.pane_id.ok_or("Prompt placement without a pane")?;
                Ok(PromptPlacement::Pane(if placement.pane_is_plugin {
                    PaneId::Plugin(id)
                } else {
                    PaneId::Terminal(id)
                }))
            },
            ProtobufPromptPlacementKind::Center => Ok(PromptPlacement::Center),
            ProtobufPromptPlacementKind::Mouse => Ok(PromptPlacement::Mouse),
            ProtobufPromptPlacementKind::Cursor => Ok(PromptPlacement::Cursor),
            ProtobufPromptPlacementKind::Corner => placement
                .corner
                .as_deref()
                .and_then(|corner| corner.parse().ok())
                .map(PromptPlacement::Corner)
                .ok_or("Invalid prompt corner"),
        }
    }
}

impl From<PromptSpec> for prompt_request::Spec {
    fn from(spec: PromptSpec) -> Self {
        match spec {
            PromptSpec::Confirm { message, yes, no } => {
                prompt_request::Spec::Confirm(ProtobufConfirmSpec {
                    message: Some(message.into()),
                    yes,
                    no,
                })
            },
            PromptSpec::Choose {
                items,
                multi,
                selected,
            } => prompt_request::Spec::Choose(ProtobufChooseSpec {
                items: items.into_iter().map(Into::into).collect(),
                multi,
                selected,
            }),
            PromptSpec::Input {
                message,
                placeholder,
                validate,
                required,
            } => prompt_request::Spec::Input(ProtobufInputSpec {
                message: message.map(Into::into),
                placeholder,
                validate,
                required,
            }),
            PromptSpec::Number {
                message,
                min,
                max,
                step,
            } => prompt_request::Spec::Number(ProtobufNumberSpec {
                message: message.map(Into::into),
                min,
                max,
                step,
            }),
            PromptSpec::Toggle { message } => prompt_request::Spec::Toggle(ProtobufToggleSpec {
                message: message.map(Into::into),
            }),
            PromptSpec::Select { message, options } => {
                prompt_request::Spec::Select(ProtobufSelectSpec {
                    message: Some(message.into()),
                    options,
                })
            },
            PromptSpec::Menu {
                items,
                remember,
                padded,
            } => prompt_request::Spec::Menu(ProtobufMenuSpec {
                items: items.into_iter().map(Into::into).collect(),
                remember,
                padded,
            }),
            PromptSpec::Form { spec } => prompt_request::Spec::Form(spec.into()),
            PromptSpec::Notify {
                message,
                show_pane_name,
                show_tab_name,
            } => prompt_request::Spec::Notify(ProtobufNotifySpec {
                message: Some(message.into()),
                show_pane_name,
                show_tab_name,
            }),
        }
    }
}

impl TryFrom<prompt_request::Spec> for PromptSpec {
    type Error = &'static str;
    fn try_from(spec: prompt_request::Spec) -> Result<Self, &'static str> {
        Ok(match spec {
            prompt_request::Spec::Confirm(spec) => PromptSpec::Confirm {
                message: spec.message.map(Into::into).unwrap_or_default(),
                yes: spec.yes,
                no: spec.no,
            },
            prompt_request::Spec::Choose(spec) => PromptSpec::Choose {
                items: spec.items.into_iter().map(Into::into).collect(),
                multi: spec.multi,
                selected: spec.selected,
            },
            prompt_request::Spec::Input(spec) => PromptSpec::Input {
                message: spec.message.map(Into::into),
                placeholder: spec.placeholder,
                validate: spec.validate,
                required: spec.required,
            },
            prompt_request::Spec::Number(spec) => PromptSpec::Number {
                message: spec.message.map(Into::into),
                min: spec.min,
                max: spec.max,
                step: spec.step,
            },
            prompt_request::Spec::Toggle(spec) => PromptSpec::Toggle {
                message: spec.message.map(Into::into),
            },
            prompt_request::Spec::Select(spec) => PromptSpec::Select {
                message: spec.message.map(Into::into).unwrap_or_default(),
                options: spec.options,
            },
            prompt_request::Spec::Menu(spec) => PromptSpec::Menu {
                items: spec.items.into_iter().map(Into::into).collect(),
                remember: spec.remember,
                padded: spec.padded,
            },
            prompt_request::Spec::Form(spec) => PromptSpec::Form {
                spec: spec.try_into()?,
            },
            prompt_request::Spec::Notify(spec) => PromptSpec::Notify {
                message: spec.message.map(Into::into).unwrap_or_default(),
                show_pane_name: spec.show_pane_name,
                show_tab_name: spec.show_tab_name,
            },
        })
    }
}

impl From<PromptRequest> for ProtobufPromptRequest {
    fn from(request: PromptRequest) -> Self {
        ProtobufPromptRequest {
            element: request.element.name().to_owned(),
            title: request.title,
            timeout_ms: request.timeout.map(|timeout| timeout.as_millis() as u64),
            default: request.default,
            placement: request.placement.map(Into::into),
            focused: request.focused,
            capture_all_keys: request.capture_all_keys,
            spec: Some(request.spec.into()),
        }
    }
}

impl TryFrom<ProtobufPromptRequest> for PromptRequest {
    type Error = &'static str;
    fn try_from(request: ProtobufPromptRequest) -> Result<Self, &'static str> {
        Ok(PromptRequest {
            element: PromptElement::from_str(&request.element)
                .map_err(|_| "Invalid prompt element")?,
            title: request.title,
            timeout: request.timeout_ms.map(Duration::from_millis),
            default: request.default,
            placement: match request.placement {
                Some(placement) => Some(placement.try_into()?),
                None => None,
            },
            focused: request.focused,
            capture_all_keys: request.capture_all_keys,
            spec: request
                .spec
                .ok_or("Prompt request without a spec")?
                .try_into()?,
        })
    }
}

impl From<PromptValue> for ProtobufPromptValue {
    fn from(value: PromptValue) -> Self {
        let value = match value {
            PromptValue::Text(text) => prompt_value::Value::Text(text),
            PromptValue::Number(number) => prompt_value::Value::Number(number),
            PromptValue::Bool(on) => prompt_value::Value::BoolValue(on),
            PromptValue::Choice(choice) => prompt_value::Value::Choice(choice),
            PromptValue::Choices(values) => {
                prompt_value::Value::Choices(ProtobufPromptValueList { values })
            },
            PromptValue::Form(entries) => prompt_value::Value::Form(ProtobufPromptFormValue {
                entries: entries
                    .into_iter()
                    .map(|(key, value)| ProtobufPromptFormEntry {
                        key,
                        value: Some(value.into()),
                    })
                    .collect(),
            }),
        };
        ProtobufPromptValue { value: Some(value) }
    }
}

impl TryFrom<ProtobufPromptValue> for PromptValue {
    type Error = &'static str;
    fn try_from(value: ProtobufPromptValue) -> Result<Self, &'static str> {
        Ok(match value.value.ok_or("Empty prompt value")? {
            prompt_value::Value::Text(text) => PromptValue::Text(text),
            prompt_value::Value::Number(number) => PromptValue::Number(number),
            prompt_value::Value::BoolValue(on) => PromptValue::Bool(on),
            prompt_value::Value::Choice(choice) => PromptValue::Choice(choice),
            prompt_value::Value::Choices(list) => PromptValue::Choices(list.values),
            prompt_value::Value::Form(form) => PromptValue::Form(
                form.entries
                    .into_iter()
                    .map(|entry| {
                        let value = entry.value.ok_or("Empty prompt form value")?;
                        Ok((entry.key, PromptValue::try_from(value)?))
                    })
                    .collect::<Result<_, &'static str>>()?,
            ),
        })
    }
}

impl From<PromptResult> for ProtobufPromptResult {
    fn from(result: PromptResult) -> Self {
        let result = match result {
            PromptResult::Answered(value) => prompt_result::Result::Answered(value.into()),
            PromptResult::Confirmed(yes) => prompt_result::Result::Confirmed(yes),
            PromptResult::Cancelled => prompt_result::Result::Cancelled(ProtobufPromptCancelled {}),
            PromptResult::TimedOut(value) => {
                prompt_result::Result::TimedOut(ProtobufPromptTimedOut {
                    value: value.map(Into::into),
                })
            },
            PromptResult::Error(message) => prompt_result::Result::Error(message),
        };
        ProtobufPromptResult {
            result: Some(result),
        }
    }
}

impl TryFrom<ProtobufPromptResult> for PromptResult {
    type Error = &'static str;
    fn try_from(result: ProtobufPromptResult) -> Result<Self, &'static str> {
        Ok(match result.result.ok_or("Empty prompt result")? {
            prompt_result::Result::Answered(value) => PromptResult::Answered(value.try_into()?),
            prompt_result::Result::Confirmed(yes) => PromptResult::Confirmed(yes),
            prompt_result::Result::Cancelled(_) => PromptResult::Cancelled,
            prompt_result::Result::TimedOut(timed_out) => {
                PromptResult::TimedOut(match timed_out.value {
                    Some(value) => Some(value.try_into()?),
                    None => None,
                })
            },
            prompt_result::Result::Error(message) => PromptResult::Error(message),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::{PopupCorner, StyledText};
    use std::collections::BTreeMap;

    fn round_trip_request(request: PromptRequest) {
        let protobuf: ProtobufPromptRequest = request.clone().into();
        assert_eq!(PromptRequest::try_from(protobuf), Ok(request));
    }

    #[test]
    fn every_request_survives_protobuf() {
        round_trip_request(
            PromptRequest::menu([
                ChoiceItem::labeled("d", "Detach").emphasize_first(6),
                ChoiceItem::plain("c"),
            ])
            .remember("Always do this")
            .padded()
            .capture_all_keys(true),
        );
        round_trip_request(
            PromptRequest::confirm("Go?")
                .yes("Go")
                .no("Stay")
                .timeout(Duration::from_secs(3))
                .default("yes")
                .title("T"),
        );
        round_trip_request(
            PromptRequest::choose(vec![ChoiceItem::plain("a"), ChoiceItem::labeled("b", "B")])
                .multi()
                .selected(vec!["a"])
                .placement(PromptPlacement::Pane(PaneId::Plugin(4))),
        );
        round_trip_request(
            PromptRequest::input("Name")
                .placeholder("p")
                .validate("^x$")
                .required()
                .placement(PromptPlacement::Pane(PaneId::Terminal(2))),
        );
        round_trip_request(
            PromptRequest::number("N")
                .min(-1)
                .max(9)
                .step(2)
                .placement(PromptPlacement::Mouse),
        );
        round_trip_request(PromptRequest::toggle("On").placement(PromptPlacement::Cursor));
        round_trip_request(PromptRequest::toggle(StyledText {
            text: "Use CI".to_owned(),
            indices: vec![vec![], vec![4, 5]],
        }));
        round_trip_request(
            PromptRequest::select("S", vec!["a", "b"]).placement(PromptPlacement::Center),
        );
        round_trip_request(PromptRequest::menu(vec![("v", "Label")]));
        let colored = StyledText {
            text: "Delete x?".to_owned(),
            indices: vec![vec![], vec![], vec![], vec![7]],
        };
        round_trip_request(PromptRequest::confirm(colored.clone()));
        round_trip_request(PromptRequest::input(colored.clone()));
        round_trip_request(PromptRequest::select(colored, vec!["a"]));
        round_trip_request(
            PromptRequest::form(vec![FormField::toggle("again", "Don't ask again")])
                .message(StyledText {
                    text: "Delete it?".to_owned(),
                    indices: vec![vec![], vec![], vec![], vec![7, 8]],
                })
                .buttons("Delete", "Cancel"),
        );
        round_trip_request(PromptRequest::form(vec![
            FormField::input("name", "Name")
                .required()
                .validate("^a$")
                .placeholder("p"),
            FormField::select("l", "L", vec!["MIT"]).default_value("MIT"),
            FormField::toggle("t", "T").default_value(true),
            FormField::number("n", "N")
                .min(1)
                .max(3)
                .step(1)
                .default_value(2),
            FormField::choose("c", "C", vec!["a", "b"]).multi(),
        ]));
        round_trip_request(
            PromptRequest::notify("Done")
                .hide_pane_name()
                .placement(PromptPlacement::Corner(PopupCorner::BottomLeft)),
        );
    }

    #[test]
    fn every_result_survives_protobuf() {
        let mut form = BTreeMap::new();
        form.insert("a".to_owned(), PromptValue::Text("x".to_owned()));
        form.insert("b".to_owned(), PromptValue::Number(3));
        form.insert("c".to_owned(), PromptValue::Bool(true));
        form.insert("d".to_owned(), PromptValue::Choice("y".to_owned()));
        form.insert(
            "e".to_owned(),
            PromptValue::Choices(vec!["p".to_owned(), "q".to_owned()]),
        );
        let results = vec![
            PromptResult::Answered(PromptValue::Text("t".to_owned())),
            PromptResult::Answered(PromptValue::Number(-4)),
            PromptResult::Answered(PromptValue::Bool(false)),
            PromptResult::Answered(PromptValue::Choice("c".to_owned())),
            PromptResult::Answered(PromptValue::Choices(vec![])),
            PromptResult::Answered(PromptValue::Form(form)),
            PromptResult::Confirmed(true),
            PromptResult::Confirmed(false),
            PromptResult::Cancelled,
            PromptResult::TimedOut(None),
            PromptResult::TimedOut(Some(PromptValue::Text("d".to_owned()))),
            PromptResult::Error("bad".to_owned()),
        ];
        for result in results {
            let protobuf: ProtobufPromptResult = result.clone().into();
            assert_eq!(PromptResult::try_from(protobuf), Ok(result));
        }
    }
}
