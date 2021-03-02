use serde::{Deserialize, Serialize};

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
// FIXME: use same struct from main crate?
// Maybe zellij should import zellij-tile, or some other crate that has these
// shared types. Maybe this will be easier after the server-client split...
#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct Help {
    pub mode: InputMode,
    pub keybinds: Vec<(String, String)>,
}
#[derive(Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct TabInfo {
    pub position: usize,
    pub _name: String, // FIXME: Implement this soon!
    pub active: bool,
}

// TODO: use same struct from main crate?
#[derive(Debug, Clone, Deserialize, Serialize)]
pub enum InputMode {
    Normal,
    Command,
    Resize,
    Pane,
    Tab,
    Scroll,
    Exiting,
}

impl Default for InputMode {
    fn default() -> InputMode {
        InputMode::Normal
    }
}
