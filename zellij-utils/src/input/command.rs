//! Trigger a command
use super::actions::Direction;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub enum TerminalAction {
    OpenFile(PathBuf),
    RunCommand(RunCommand),
}

#[derive(Clone, Debug, Deserialize, Default, Serialize, PartialEq, Eq)]
#[derive(knuffel::Decode)]
pub struct RunCommand {
    #[serde(alias = "cmd")]
    #[knuffel(argument)]
    pub command: PathBuf,
    #[serde(default)]
    #[knuffel(arguments)]
    pub args: Vec<String>,
    #[serde(default)]
    #[knuffel(property)]
    pub cwd: Option<PathBuf>,
}

/// Intermediate representation
#[derive(Clone, Debug, Deserialize, Default, Serialize, PartialEq, Eq)]
#[derive(knuffel::Decode)]
pub struct RunCommandAction {
    #[serde(rename = "cmd")]
    #[knuffel(argument)]
    pub command: PathBuf,
    #[serde(default)]
    #[knuffel(arguments)]
    pub args: Vec<String>,
    #[serde(default)]
    #[knuffel(property)]
    pub cwd: Option<PathBuf>,
    #[serde(default)]
    #[knuffel(property)]
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
