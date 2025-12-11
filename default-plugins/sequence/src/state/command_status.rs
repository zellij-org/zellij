use zellij_tile::prelude::*;

#[derive(Debug, Clone)]
pub enum CommandStatus {
    Exited(Option<i32>, Option<PaneId>),
    Running(Option<PaneId>),
    Pending,
    Interrupted(Option<PaneId>),
}

impl CommandStatus {
    pub fn get_pane_id(&self) -> Option<PaneId> {
        match self {
            CommandStatus::Exited(_, pane_id) => *pane_id,
            CommandStatus::Running(pane_id) => *pane_id,
            CommandStatus::Pending => None,
            CommandStatus::Interrupted(pane_id) => *pane_id,
        }
    }
}

impl Default for CommandStatus {
    fn default() -> Self {
        CommandStatus::Pending
    }
}
