use super::{ChainType, CommandStatus};
use std::path::PathBuf;
use std::time::Instant;
use zellij_tile::prelude::PaneId;

#[derive(Debug, Clone)]
pub struct CommandEntry {
    text: String,
    cwd: Option<PathBuf>,
    pub(super) chain_type: ChainType,
    pub(super) status: CommandStatus,
    pub(super) start_time: std::time::Instant,
}

impl Default for CommandEntry {
    fn default() -> Self {
        CommandEntry {
            text: String::default(),
            cwd: None,
            chain_type: ChainType::default(),
            status: CommandStatus::default(),
            start_time: Instant::now(),
        }
    }
}

impl CommandEntry {
    pub fn new(text: &str, cwd: Option<PathBuf>) -> Self {
        CommandEntry {
            text: text.to_owned(),
            cwd,
            ..Default::default()
        }
    }
    pub fn with_and(mut self) -> Self {
        self.chain_type = ChainType::And;
        self
    }
    pub fn get_text(&self) -> String {
        self.text.clone()
    }
    pub fn set_text(&mut self, text: String) {
        self.text = text;
    }
    pub fn clear_text(&mut self) {
        self.text.clear();
    }
    pub fn get_chain_type(&self) -> ChainType {
        self.chain_type
    }
    pub fn set_chain_type(&mut self, chain_type: ChainType) {
        self.chain_type = chain_type;
    }
    pub fn get_status(&self) -> CommandStatus {
        self.status.clone()
    }
    pub fn set_status(&mut self, status: CommandStatus) {
        self.status = status;
    }
    pub fn get_pane_id(&self) -> Option<PaneId> {
        self.status.get_pane_id()
    }
    pub fn is_empty(&self) -> bool {
        self.text.trim().is_empty()
    }
    pub fn fill_chain_type_if_empty(&mut self) {
        if let ChainType::None = self.chain_type {
            self.chain_type = ChainType::And;
        }
    }
    pub fn clear_chain_type(&mut self) {
        self.chain_type = ChainType::None;
    }
    pub fn clear_status(&mut self) {
        self.status = CommandStatus::Pending;
    }
    pub fn cycle_chain_type(&mut self) {
        self.chain_type.cycle_next();
    }
    pub fn get_cwd(&self) -> Option<PathBuf> {
        self.cwd.clone()
    }
    pub fn set_cwd(&mut self, cwd: Option<PathBuf>) {
        self.cwd = cwd;
    }
}
