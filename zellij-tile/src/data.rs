use colors_transform::{Color, Rgb};
use serde::{Deserialize, Serialize};
use strum_macros::{EnumDiscriminants, EnumIter, EnumString, ToString};
use xrdb::Colors;
pub mod colors {
    pub const WHITE: (u8, u8, u8) = (238, 238, 238);
    pub const GREEN: (u8, u8, u8) = (175, 255, 0);
    pub const GRAY: (u8, u8, u8) = (68, 68, 68);
    pub const BRIGHT_GRAY: (u8, u8, u8) = (138, 138, 138);
    pub const RED: (u8, u8, u8) = (135, 0, 0);
    pub const BLACK: (u8, u8, u8) = (0, 0, 0);
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Palette {
    pub fg: (u8, u8, u8),
    pub bg: (u8, u8, u8),
    pub black: (u8, u8, u8),
    pub red: (u8, u8, u8),
    pub green: (u8, u8, u8),
    pub yellow: (u8, u8, u8),
    pub blue: (u8, u8, u8),
    pub magenta: (u8, u8, u8),
    pub cyan: (u8, u8, u8),
    pub white: (u8, u8, u8),
}

impl Palette {
    pub fn new() -> Self {
        let palette = match Colors::new("xresources") {
            Some(colors) => {
                let fg = colors.fg.unwrap();
                let fg_imm = &fg;
                let fg_hex: &str = &fg_imm;
                let fg = Rgb::from_hex_str(fg_hex).unwrap().as_tuple();
                let fg = (fg.0 as u8, fg.1 as u8, fg.2 as u8);
                let bg = colors.bg.unwrap();
                let bg_imm = &bg;
                let bg_hex: &str = &bg_imm;
                let bg = Rgb::from_hex_str(bg_hex).unwrap().as_tuple();
                let bg = (bg.0 as u8, bg.1 as u8, bg.2 as u8);
                let colors: Vec<(u8, u8, u8)> = colors
                    .colors
                    .iter()
                    .map(|c| {
                        let c = c.clone();
                        let imm_str = &c.unwrap();
                        let hex_str: &str = &imm_str;
                        let rgb = Rgb::from_hex_str(hex_str).unwrap().as_tuple();
                        (rgb.0 as u8, rgb.1 as u8, rgb.2 as u8)
                    })
                    .collect();
                Self {
                    fg,
                    bg,
                    black: colors[0],
                    red: colors[1],
                    green: colors[2],
                    yellow: colors[3],
                    blue: colors[4],
                    magenta: colors[5],
                    cyan: colors[6],
                    white: colors[7],
                }
            }
            None => Self {
                fg: colors::BRIGHT_GRAY,
                bg: colors::BLACK,
                black: colors::BLACK,
                red: colors::RED,
                green: colors::GREEN,
                yellow: colors::GRAY,
                blue: colors::GRAY,
                magenta: colors::GRAY,
                cyan: colors::GRAY,
                white: colors::WHITE,
            },
        };
        palette
    }
}
impl Default for Palette {
    fn default() -> Palette {
        Palette::new()
    }
}

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

#[derive(Debug, Clone, EnumDiscriminants, ToString, Serialize, Deserialize)]
#[strum_discriminants(derive(EnumString, Hash, Serialize, Deserialize))]
#[strum_discriminants(name(EventType))]
pub enum Event {
    ModeUpdate(ModeInfo),
    TabUpdate(Vec<TabInfo>),
    KeyPress(Key),
}

/// Describes the different input modes, which change the way that keystrokes will be interpreted.
#[derive(Debug, PartialEq, Eq, Hash, Copy, Clone, EnumIter, Serialize, Deserialize)]
pub enum InputMode {
    /// In `Normal` mode, input is always written to the terminal, except for the shortcuts leading
    /// to other modes
    Normal,
    /// In `Locked` mode, input is always written to the terminal and all shortcuts are disabled
    /// except the one leading back to normal mode
    Locked,
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
pub struct ModeInfo {
    pub mode: InputMode,
    // FIXME: This should probably return Keys and Actions, then sort out strings plugin-side
    pub keybinds: Vec<(String, String)>, // <shortcut> => <shortcut description>
    pub palette: Palette,
}

#[derive(Debug, Default, Clone, Deserialize, Serialize)]
pub struct TabInfo {
    /* subset of fields to publish to plugins */
    pub position: usize,
    pub name: String,
    pub active: bool,
}
