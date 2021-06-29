//! Trigger a command
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub enum TerminalAction {
    OpenFile(PathBuf),
    RunCommand(RunCommand),
}

#[derive(Clone, Debug, Deserialize, Default, Serialize, PartialEq)]
pub struct RunCommand {
    pub command: PathBuf,
    #[serde(default)]
    pub args: Vec<String>,
}
