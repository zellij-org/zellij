//! Trigger a command
use crate::envs::EnvironmentVariables;

use super::actions::Direction;
use serde::{Deserialize, Serialize};
use std::{env, path::PathBuf};

#[derive(Debug, Clone)]
pub enum TerminalAction {
    OpenFile(OpenFile),
    RunCommand(RunCommand),
}

#[derive(Clone, Debug, Deserialize, Default, Serialize, PartialEq, Eq)]
pub struct RunCommand {
    #[serde(alias = "cmd")]
    pub command: PathBuf,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub cwd: Option<PathBuf>,
    #[serde(default)]
    pub env: EnvironmentVariables,
    #[serde(default)]
    pub hold_on_close: bool,
    #[serde(default)]
    pub hold_on_start: bool,
}

impl std::fmt::Display for RunCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut command: String = self
            .command
            .as_path()
            .as_os_str()
            .to_string_lossy()
            .to_string();
        for arg in &self.args {
            command.push(' ');
            command.push_str(arg);
        }
        write!(f, "{}", command)
    }
}

/// Intermediate representation
#[derive(Clone, Debug, Deserialize, Default, Serialize, PartialEq, Eq)]
pub struct RunCommandAction {
    #[serde(rename = "cmd")]
    pub command: PathBuf,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub cwd: Option<PathBuf>,
    #[serde(default)]
    pub env: EnvironmentVariables,

    #[serde(default)]
    pub hold_on_close: bool,
    #[serde(default)]
    pub hold_on_start: bool,

    #[serde(default)]
    pub direction: Option<Direction>,
}

impl From<RunCommandAction> for RunCommand {
    fn from(action: RunCommandAction) -> Self {
        RunCommand {
            command: action.command,
            args: action.args,
            cwd: action.cwd,
            env: action.env,
            hold_on_close: action.hold_on_close,
            hold_on_start: action.hold_on_start,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Default, Serialize, PartialEq, Eq)]
pub struct OpenFile {
    #[serde(default)]
    pub file_name: PathBuf,
    #[serde(default)]
    pub line_number: Option<usize>,
    #[serde(default)]
    pub cwd: Option<PathBuf>,
    #[serde(default)]
    pub env: EnvironmentVariables,
}

/// Intermediate representation
#[derive(Clone, Debug, Deserialize, Default, Serialize, PartialEq, Eq)]
pub struct OpenFileAction {
    #[serde(rename = "file")]
    pub file_name: PathBuf,
    #[serde(default)]
    pub line_number: Option<usize>,
    #[serde(default)]
    pub cwd: Option<PathBuf>,
    #[serde(default)]
    pub env: EnvironmentVariables,

    #[serde(default)]
    pub direction: Option<Direction>,
    #[serde(default)]
    pub floating: bool,
}

impl From<OpenFileAction> for OpenFile {
    fn from(action: OpenFileAction) -> Self {
        OpenFile {
            file_name: action.file_name,
            line_number: action.line_number,
            cwd: action.cwd,
            env: action.env,
        }
    }
}

impl OpenFile {
    pub fn to_run_action(
        self,
        default_editor: Option<PathBuf>,
    ) -> (RunCommand, Option<RunCommand>) {
        let mut failover_cmd_args = None;
        let mut command = default_editor.unwrap_or_else(|| {
            PathBuf::from(
                env::var("EDITOR")
                    .unwrap_or_else(|_| env::var("VISUAL").unwrap_or_else(|_| "vi".into())),
            )
        });
        let mut args = vec![];
        if !command.is_dir() {
            separate_command_arguments(&mut command, &mut args);
        }
        let file_to_open = self
            .file_name
            .into_os_string()
            .into_string()
            .expect("Not valid Utf8 Encoding");
        if let Some(line_number) = self.line_number {
            if command.ends_with("vim")
                || command.ends_with("nvim")
                || command.ends_with("emacs")
                || command.ends_with("nano")
                || command.ends_with("kak")
            {
                failover_cmd_args = Some(vec![file_to_open.clone()]);
                args.push(format!("+{}", line_number));
            }
        }
        args.push(file_to_open);
        let cmd = RunCommand {
            command,
            args,
            cwd: self.cwd,
            hold_on_close: false,
            hold_on_start: false,
            env: self.env,
        };
        let failover_cmd = if let Some(failover_cmd_args) = failover_cmd_args {
            let mut cmd = cmd.clone();
            cmd.args = failover_cmd_args;
            Some(cmd)
        } else {
            None
        };
        (cmd, failover_cmd)
    }
}

// this is a utility method to separate the arguments from a pathbuf before we turn it into a
// Command. eg. "/usr/bin/vim -e" ==> "/usr/bin/vim" + "-e" (the latter will be pushed to args)
fn separate_command_arguments(command: &mut PathBuf, args: &mut Vec<String>) {
    if let Some(file_name) = command
        .file_name()
        .and_then(|f_n| f_n.to_str())
        .map(|f_n| f_n.to_string())
    {
        let mut file_name_parts = file_name.split_ascii_whitespace();
        if let Some(first_part) = file_name_parts.next() {
            command.set_file_name(first_part);
            for part in file_name_parts {
                args.push(String::from(part));
            }
        }
    }
}
