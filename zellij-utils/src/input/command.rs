//! Trigger a command
use crate::data::Direction;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub enum TerminalAction {
    OpenFile(PathBuf, Option<usize>, Option<PathBuf>), // path to file (should be absolute), optional line_number and an
    // optional cwd
    RunCommand(RunCommand),
}

impl TerminalAction {
    pub fn change_cwd(&mut self, new_cwd: PathBuf) {
        match self {
            TerminalAction::OpenFile(_, _, cwd) => {
                *cwd = Some(new_cwd);
            },
            TerminalAction::RunCommand(run_command) => {
                run_command.cwd = Some(new_cwd);
            },
        }
    }
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
    pub hold_on_close: bool,
    #[serde(default)]
    pub hold_on_start: bool,
    #[serde(default)]
    pub hide_title: bool,
}

impl std::fmt::Display for RunCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // uhhh..
        if self.hide_title {
            return write!(f, "");
        }

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
    pub direction: Option<Direction>,
    #[serde(default)]
    pub hold_on_close: bool,
    #[serde(default)]
    pub hold_on_start: bool,
    #[serde(default)]
    pub hide_title: bool,
}

impl From<RunCommandAction> for RunCommand {
    fn from(action: RunCommandAction) -> Self {
        RunCommand {
            command: action.command,
            args: action.args,
            cwd: action.cwd,
            hold_on_close: action.hold_on_close,
            hold_on_start: action.hold_on_start,
            hide_title: action.hide_title,
        }
    }
}

impl From<RunCommand> for RunCommandAction {
    fn from(run_command: RunCommand) -> Self {
        RunCommandAction {
            command: run_command.command,
            args: run_command.args,
            cwd: run_command.cwd,
            direction: None,
            hold_on_close: run_command.hold_on_close,
            hold_on_start: run_command.hold_on_start,
            hide_title: run_command.hide_title,
        }
    }
}

impl RunCommand {
    pub fn new(command: PathBuf) -> Self {
        RunCommand {
            command,
            ..Default::default()
        }
    }
    pub fn with_cwd(mut self, cwd: PathBuf) -> Self {
        self.cwd = Some(cwd);
        self
    }
}
