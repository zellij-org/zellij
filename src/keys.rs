use serde::{Deserialize, Serialize};

#[derive(Debug, PartialOrd, PartialEq, Eq, Clone, Copy, Hash, Serialize, Deserialize)]
pub struct KeyEvent {
    pub code: KeyCode,
    pub modifiers: KeyModifiers,
}

#[derive(Debug, PartialOrd, PartialEq, Eq, Clone, Copy, Hash, Serialize, Deserialize)]
pub struct KeyModifiers {
    pub bits: u8,
    //pub shift: bool,
    //pub ctrl: bool,
    //pub alt: bool,
}

#[derive(Debug, PartialOrd, PartialEq, Eq, Clone, Copy, Hash, Serialize, Deserialize)]
pub enum KeyCode {
    Backspace,
    Enter,
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    PageUp,
    PageDown,
    Tab,
    BackTab,
    Delete,
    Insert,
    F(u8),
    Char(char),
    Null,
    Esc,
}
