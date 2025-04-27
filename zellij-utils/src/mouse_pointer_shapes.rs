use serde::{Deserialize, Serialize};

/// Mouse pointer shapes as defined in the OSC 22 protocol.
/// See https://sw.kovidgoyal.net/kitty/pointer-shapes/
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum MousePointerShape {
    /// Default cursor (arrow)
    Default,
    /// Text cursor (I-beam)
    Text,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MousePointerShapeProtocolMode {
    XTerm,
    Kitty,
}

impl MousePointerShape {
    /// See https://sw.kovidgoyal.net/kitty/pointer-shapes/
    fn kitty_name(&self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Text => "text",
        }
    }

    /// See https://github.com/xterm-x11/xterm-snapshots/blob/5b7a08a3482b425c97/xterm.man#L4674
    fn xterm_name(&self) -> &'static str {
        match self {
            Self::Default => "left_ptr",
            Self::Text => "xterm",
        }
    }

    pub fn generate_set_mouse_pointer_escape_sequence(&self, mode: MousePointerShapeProtocolMode) -> String {
        let name = match mode {
            MousePointerShapeProtocolMode::XTerm => self.xterm_name(),
            MousePointerShapeProtocolMode::Kitty => self.kitty_name(),
        };
        format!("\x1b]22;{}\x1b\\", name)
    }
}
