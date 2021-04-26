use std::fmt;
use serde::{Deserialize, Serialize};
use strum_macros::{EnumDiscriminants, EnumIter, EnumString, ToString};

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

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LogLevel {
    DEBUG,
    INFO,
    LOG,
    WARN,
    ERROR,
}
impl Default for LogLevel {
    fn default() -> Self {
        LogLevel::LOG
    }
}
impl fmt::Display for LogLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LogLevel::DEBUG => write!(f, "DEBUG"),
            LogLevel::INFO => write!(f, "INFO"),
            LogLevel::LOG => write!(f, "LOG"),
            LogLevel::WARN => write!(f, "WARN"),
            LogLevel::ERROR => write!(f, "ERROR"),
        }
    }
}

#[derive(Debug, Clone, EnumDiscriminants, ToString, Serialize, Deserialize)]
#[strum_discriminants(derive(EnumString, Hash, Serialize, Deserialize))]
#[strum_discriminants(name(EventType))]
pub enum Event {
    ModeUpdate(ModeInfo),
    TabUpdate(Vec<TabInfo>),
    KeyPress(Key),
    Log(String, Option<LogLevel>),
}

/// Describes the different input modes, which change the way that keystrokes will be interpreted.
#[derive(Debug, PartialEq, Eq, Hash, Copy, Clone, EnumIter, Serialize, Deserialize)]
pub enum InputMode {
    /// In `Normal` mode, input is always written to the terminal, except for the shortcuts leading
    /// to other modes
    #[serde(alias = "normal")]
    Normal,
    /// In `Locked` mode, input is always written to the terminal and all shortcuts are disabled
    /// except the one leading back to normal mode
    #[serde(alias = "locked")]
    Locked,
    /// `Resize` mode allows resizing the different existing panes.
    #[serde(alias = "resize")]
    Resize,
    /// `Pane` mode allows creating and closing panes, as well as moving between them.
    #[serde(alias = "pane")]
    Pane,
    /// `Tab` mode allows creating and closing tabs, as well as moving between them.
    #[serde(alias = "tab")]
    Tab,
    /// `Scroll` mode allows scrolling up and down within a pane.
    #[serde(alias = "scroll")]
    Scroll,
    #[serde(alias = "renametab")]
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
pub struct ModeInfo {
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
