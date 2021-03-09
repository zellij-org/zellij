use serde::{Deserialize, Serialize};
use strum_macros::{EnumDiscriminants, EnumIter, ToString};

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Key {
    Backspace,
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    PageUp,
    PageDown,
    BackTab,
    Delete,
    Insert,
    F(u8),
    Char(char),
    Alt(char),
    Ctrl(char),
    Null,
    Esc,
}

#[derive(Debug, EnumDiscriminants, ToString, Serialize, Deserialize)]
#[strum_discriminants(derive(Hash, Serialize, Deserialize))]
#[strum_discriminants(name(EventType))]
pub enum Event {
    ModeUpdate(Help), // FIXME: Rename the `Help` struct
    TabUpdate(TabInfo),
    KeyPress(Key),
}

/// Describes the different input modes, which change the way that keystrokes will be interpreted.
#[derive(Debug, PartialEq, Eq, Hash, Copy, Clone, EnumIter, Serialize, Deserialize)]
pub enum InputMode {
    /// In `Normal` mode, input is always written to the terminal, except for one special input that
    /// triggers the switch to [`InputMode::Command`] mode.
    Normal,
    /// In `Command` mode, input is bound to actions (more precisely, sequences of actions).
    /// `Command` mode gives access to the other modes non-`InputMode::Normal` modes.
    /// etc.
    Command,
    /// `Resize` mode allows resizing the different existing panes.
    Resize,
    /// `Pane` mode allows creating and closing panes, as well as moving between them.
    Pane,
    /// `Tab` mode allows creating and closing tabs, as well as moving between them.
    Tab,
    /// `Scroll` mode allows scrolling up and down within a pane.
    Scroll,
    RenameTab,
}

impl Default for InputMode {
    fn default() -> InputMode {
        InputMode::Normal
    }
}

/// Represents the contents of the help message that is printed in the status bar,
/// which indicates the current [`InputMode`] and what the keybinds for that mode
/// are. Related to the default `status-bar` plugin.
#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct Help {
    pub mode: InputMode,
    // FIXME: This should probably return Keys and Actions, then sort out strings plugin-side
    pub keybinds: Vec<(String, String)>, // <shortcut> => <shortcut description>
}

#[derive(Debug, Default, Clone, Deserialize, Serialize)]
pub struct TabInfo {
    /* subset of fields to publish to plugins */
    pub position: usize,
    pub name: String,
    pub active: bool,
}
