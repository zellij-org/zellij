//! Trigger a command
use super::actions::Direction;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use kdl::*;
use crate::{kdl_get_child, kdl_get_child_entry_string_value, kdl_string_arguments};
use crate::input::config::ConfigError;

#[derive(Debug, Clone)]
pub enum TerminalAction {
    OpenFile(PathBuf, Option<usize>), // path to file and optional line_number
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
}

impl From<RunCommandAction> for RunCommand {
    fn from(action: RunCommandAction) -> Self {
        RunCommand {
            command: action.command,
            args: action.args,
            cwd: action.cwd,
        }
    }
}
