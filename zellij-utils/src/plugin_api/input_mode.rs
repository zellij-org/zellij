pub use super::generated_api::api::input_mode::{
    InputMode as ProtobufInputMode, InputModeMessage as ProtobufInputModeMessage,
};
use crate::data::InputMode;

use std::convert::TryFrom;

impl TryFrom<ProtobufInputMode> for InputMode {
    type Error = &'static str;
    fn try_from(protobuf_input_mode: ProtobufInputMode) -> Result<Self, &'static str> {
        match protobuf_input_mode {
            ProtobufInputMode::Normal => Ok(InputMode::Normal),
            ProtobufInputMode::Locked => Ok(InputMode::Locked),
            ProtobufInputMode::Resize => Ok(InputMode::Resize),
            ProtobufInputMode::Pane => Ok(InputMode::Pane),
            ProtobufInputMode::Tab => Ok(InputMode::Tab),
            ProtobufInputMode::Scroll => Ok(InputMode::Scroll),
            ProtobufInputMode::EnterSearch => Ok(InputMode::EnterSearch),
            ProtobufInputMode::Search => Ok(InputMode::Search),
            ProtobufInputMode::RenameTab => Ok(InputMode::RenameTab),
            ProtobufInputMode::RenamePane => Ok(InputMode::RenamePane),
            ProtobufInputMode::Session => Ok(InputMode::Session),
            ProtobufInputMode::Move => Ok(InputMode::Move),
            ProtobufInputMode::Prompt => Ok(InputMode::Prompt),
            ProtobufInputMode::Tmux => Ok(InputMode::Tmux),
        }
    }
}

impl TryFrom<InputMode> for ProtobufInputMode {
    type Error = &'static str;
    fn try_from(input_mode: InputMode) -> Result<Self, &'static str> {
        Ok(match input_mode {
            InputMode::Normal => ProtobufInputMode::Normal,
            InputMode::Locked => ProtobufInputMode::Locked,
            InputMode::Resize => ProtobufInputMode::Resize,
            InputMode::Pane => ProtobufInputMode::Pane,
            InputMode::Tab => ProtobufInputMode::Tab,
            InputMode::Scroll => ProtobufInputMode::Scroll,
            InputMode::EnterSearch => ProtobufInputMode::EnterSearch,
            InputMode::Search => ProtobufInputMode::Search,
            InputMode::RenameTab => ProtobufInputMode::RenameTab,
            InputMode::RenamePane => ProtobufInputMode::RenamePane,
            InputMode::Session => ProtobufInputMode::Session,
            InputMode::Move => ProtobufInputMode::Move,
            InputMode::Prompt => ProtobufInputMode::Prompt,
            InputMode::Tmux => ProtobufInputMode::Tmux,
        })
    }
}

impl TryFrom<ProtobufInputModeMessage> for InputMode {
    type Error = &'static str;
    fn try_from(protobuf_input_mode: ProtobufInputModeMessage) -> Result<Self, &'static str> {
        ProtobufInputMode::from_i32(protobuf_input_mode.input_mode)
            .and_then(|p| p.try_into().ok())
            .ok_or("Invalid input mode")
    }
}

impl TryFrom<InputMode> for ProtobufInputModeMessage {
    type Error = &'static str;
    fn try_from(input_mode: InputMode) -> Result<Self, &'static str> {
        let protobuf_input_mode: ProtobufInputMode = input_mode.try_into()?;
        Ok(ProtobufInputModeMessage {
            input_mode: protobuf_input_mode as i32,
        })
    }
}
