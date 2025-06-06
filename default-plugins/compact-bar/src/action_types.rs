use zellij_tile::prelude::actions::Action;
use zellij_tile::prelude::*;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ActionType {
    MoveFocus,
    MovePaneWithDirection,
    MovePaneWithoutDirection,
    ResizeIncrease,
    ResizeDecrease,
    ResizeAny,
    Search,
    NewPaneWithDirection,
    NewPaneWithoutDirection,
    BreakPaneLeftOrRight,
    GoToAdjacentTab,
    Scroll,
    PageScroll,
    HalfPageScroll,
    SessionManager,
    Configuration,
    PluginManager,
    About,
    SwitchToMode(InputMode),
    TogglePaneEmbedOrFloating,
    ToggleFocusFullscreen,
    ToggleFloatingPanes,
    CloseFocus,
    CloseTab,
    ToggleActiveSyncTab,
    ToggleTab,
    BreakPane,
    EditScrollback,
    NewTab,
    Detach,
    Quit,
    Other(String), // Fallback for unhandled actions
}

impl ActionType {
    pub fn description(&self) -> String {
        match self {
            ActionType::MoveFocus => "Move focus".to_string(),
            ActionType::MovePaneWithDirection => "Move pane".to_string(),
            ActionType::MovePaneWithoutDirection => "Move pane".to_string(),
            ActionType::ResizeIncrease => "Increase size in direction".to_string(),
            ActionType::ResizeDecrease => "Decrease size in direction".to_string(),
            ActionType::ResizeAny => "Increase or decrease size".to_string(),
            ActionType::Search => "Search".to_string(),
            ActionType::NewPaneWithDirection => "Split right/down".to_string(),
            ActionType::NewPaneWithoutDirection => "New pane".to_string(),
            ActionType::BreakPaneLeftOrRight => "Break pane to adjacent tab".to_string(),
            ActionType::GoToAdjacentTab => "Move tab focus".to_string(),
            ActionType::Scroll => "Scroll".to_string(),
            ActionType::PageScroll => "Scroll page".to_string(),
            ActionType::HalfPageScroll => "Scroll half Page".to_string(),
            ActionType::SessionManager => "Session manager".to_string(),
            ActionType::PluginManager => "Plugin manager".to_string(),
            ActionType::Configuration => "Configuration".to_string(),
            ActionType::About => "About Zellij".to_string(),
            ActionType::SwitchToMode(input_mode) if input_mode == &InputMode::RenamePane => {
                "Rename pane".to_string()
            },
            ActionType::SwitchToMode(input_mode) if input_mode == &InputMode::RenameTab => {
                "Rename tab".to_string()
            },
            ActionType::SwitchToMode(input_mode) if input_mode == &InputMode::EnterSearch => {
                "Search".to_string()
            },
            ActionType::SwitchToMode(input_mode) if input_mode == &InputMode::Locked => {
                "Lock".to_string()
            },
            ActionType::SwitchToMode(input_mode) if input_mode == &InputMode::Normal => {
                "Unlock".to_string()
            },
            ActionType::SwitchToMode(input_mode) => format!("{:?}", input_mode),
            ActionType::TogglePaneEmbedOrFloating => "Float or embed".to_string(),
            ActionType::ToggleFocusFullscreen => "Toggle fullscreen".to_string(),
            ActionType::ToggleFloatingPanes => "Show/hide floating panes".to_string(),
            ActionType::CloseFocus => "Close pane".to_string(),
            ActionType::CloseTab => "Close tab".to_string(),
            ActionType::ToggleActiveSyncTab => "Sync panes in tab".to_string(),
            ActionType::ToggleTab => "Circle tab focus".to_string(),
            ActionType::BreakPane => "Break pane to new tab".to_string(),
            ActionType::EditScrollback => "Open pane scrollback in editor".to_string(),
            ActionType::NewTab => "New tab".to_string(),
            ActionType::Detach => "Detach".to_string(),
            ActionType::Quit => "Quit".to_string(),
            ActionType::Other(_) => "Other action".to_string(),
        }
    }

    pub fn from_action(action: &Action) -> Self {
        match action {
            Action::MoveFocus(_) => ActionType::MoveFocus,
            Action::MovePane(Some(_)) => ActionType::MovePaneWithDirection,
            Action::MovePane(None) => ActionType::MovePaneWithoutDirection,
            Action::Resize(Resize::Increase, Some(_)) => ActionType::ResizeIncrease,
            Action::Resize(Resize::Decrease, Some(_)) => ActionType::ResizeDecrease,
            Action::Resize(_, None) => ActionType::ResizeAny,
            Action::Search(_) => ActionType::Search,
            Action::NewPane(Some(_), _, _) => ActionType::NewPaneWithDirection,
            Action::NewPane(None, _, _) => ActionType::NewPaneWithoutDirection,
            Action::BreakPaneLeft | Action::BreakPaneRight => ActionType::BreakPaneLeftOrRight,
            Action::GoToPreviousTab | Action::GoToNextTab => ActionType::GoToAdjacentTab,
            Action::ScrollUp | Action::ScrollDown => ActionType::Scroll,
            Action::PageScrollUp | Action::PageScrollDown => ActionType::PageScroll,
            Action::HalfPageScrollUp | Action::HalfPageScrollDown => ActionType::HalfPageScroll,
            Action::SwitchToMode(input_mode) => ActionType::SwitchToMode(*input_mode),
            Action::TogglePaneEmbedOrFloating => ActionType::TogglePaneEmbedOrFloating,
            Action::ToggleFocusFullscreen => ActionType::ToggleFocusFullscreen,
            Action::ToggleFloatingPanes => ActionType::ToggleFloatingPanes,
            Action::CloseFocus => ActionType::CloseFocus,
            Action::CloseTab => ActionType::CloseTab,
            Action::ToggleActiveSyncTab => ActionType::ToggleActiveSyncTab,
            Action::ToggleTab => ActionType::ToggleTab,
            Action::BreakPane => ActionType::BreakPane,
            Action::EditScrollback => ActionType::EditScrollback,
            Action::Detach => ActionType::Detach,
            Action::Quit => ActionType::Quit,
            action if action.launches_plugin("session-manager") => ActionType::SessionManager,
            action if action.launches_plugin("configuration") => ActionType::Configuration,
            action if action.launches_plugin("plugin-manager") => ActionType::PluginManager,
            action if action.launches_plugin("zellij:about") => ActionType::About,
            action if matches!(action, Action::NewTab(..)) => ActionType::NewTab,
            _ => ActionType::Other(format!("{:?}", action)),
        }
    }
}
