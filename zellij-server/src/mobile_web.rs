use crate::panes::PaneId;

#[derive(Debug, Clone, Copy)]
pub(crate) struct MobileWebPrefs {
    pub single_pane: bool,
    pub fit: bool,
    pub fullscreened_pane: Option<PaneId>,
}

impl Default for MobileWebPrefs {
    fn default() -> Self {
        MobileWebPrefs {
            single_pane: true,
            fit: true,
            fullscreened_pane: None,
        }
    }
}
