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

impl RunCommand {
    pub fn from_kdl(kdl_node: &KdlNode) -> Result<Self, ConfigError> {
        let command = PathBuf::from(kdl_get_child_entry_string_value!(kdl_node, "cmd").ok_or(ConfigError::KdlParsingError("Command must have a cmd value".into()))?);
        let cwd = kdl_get_child_entry_string_value!(kdl_node, "cmd").map(|c| PathBuf::from(c));
        let args = match kdl_get_child!(kdl_node, "args") {
            Some(kdl_args) => {
                kdl_string_arguments!(kdl_args).iter().map(|s| String::from(*s)).collect()
            },
            None => vec![]
        };
        Ok(RunCommand {
            command,
            args,
            cwd,
        })
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
