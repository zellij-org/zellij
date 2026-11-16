pub use super::generated_api::api::command::Command as ProtobufCommand;
use crate::data::{BorderStyleOverride, CommandToRun};

use std::convert::TryFrom;
use std::path::PathBuf;

impl TryFrom<ProtobufCommand> for CommandToRun {
    type Error = &'static str;
    fn try_from(protobuf_command: ProtobufCommand) -> Result<Self, &'static str> {
        let path = PathBuf::from(protobuf_command.path);
        let args = protobuf_command.args;
        let cwd = protobuf_command.cwd.map(|c| PathBuf::from(c));
        let border_style = protobuf_command.border_style.map(BorderStyleOverride::from);
        Ok(CommandToRun {
            path,
            args,
            cwd,
            border_style,
        })
    }
}

impl TryFrom<CommandToRun> for ProtobufCommand {
    type Error = &'static str;
    fn try_from(command_to_run: CommandToRun) -> Result<Self, &'static str> {
        Ok(ProtobufCommand {
            path: command_to_run.path.display().to_string(),
            args: command_to_run.args,
            cwd: command_to_run.cwd.map(|c| c.display().to_string()),
            border_style: command_to_run.border_style.map(|b| b.into()),
        })
    }
}
