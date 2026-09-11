pub use super::generated_api::api::command::{
    Command as ProtobufCommand, EnvVar as ProtobufEnvVar,
};
use crate::data::CommandToRun;

use std::convert::TryFrom;
use std::path::PathBuf;

impl TryFrom<ProtobufCommand> for CommandToRun {
    type Error = &'static str;
    fn try_from(protobuf_command: ProtobufCommand) -> Result<Self, &'static str> {
        let path = PathBuf::from(protobuf_command.path);
        let args = protobuf_command.args;
        let cwd = protobuf_command.cwd.map(|c| PathBuf::from(c));
        let env_vars = protobuf_command
            .env_vars
            .into_iter()
            .map(|ProtobufEnvVar { name, value }| (name, value))
            .collect();
        Ok(CommandToRun {
            path,
            args,
            cwd,
            env_vars,
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
            env_vars: command_to_run
                .env_vars
                .into_iter()
                .map(|(name, value)| ProtobufEnvVar { name, value })
                .collect(),
        })
    }
}
