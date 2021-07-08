use serde::{Deserialize, Serialize};
use std::str::FromStr;
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

#[derive(Debug, Clone, PartialEq, EnumDiscriminants, ToString, Serialize, Deserialize)]
#[strum_discriminants(derive(EnumString, Hash, Serialize, Deserialize))]
#[strum_discriminants(name(EventType))]
#[non_exhaustive]
pub enum Event {
    ModeUpdate(ModeInfo),
    TabUpdate(Vec<TabInfo>),
    KeyPress(Key),
    Timer(f64),
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
    /// `Session` mode allows detaching sessions
    #[serde(alias = "session")]
    Session,
}

impl Default for InputMode {
    fn default() -> InputMode {
        InputMode::Normal
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum ThemeHue {
    Light,
    Dark,
}
impl Default for ThemeHue {
    fn default() -> ThemeHue {
        ThemeHue::Dark
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum PaletteColor {
    Rgb((u8, u8, u8)),
    EightBit(u8),
}
impl Default for PaletteColor {
    fn default() -> PaletteColor {
        PaletteColor::EightBit(0)
    }
}

impl FromStr for InputMode {
    type Err = Box<dyn std::error::Error>;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "normal" => Ok(InputMode::Normal),
            "resize" => Ok(InputMode::Resize),
            "locked" => Ok(InputMode::Locked),
            "pane" => Ok(InputMode::Pane),
            "tab" => Ok(InputMode::Tab),
            "scroll" => Ok(InputMode::Scroll),
            "renametab" => Ok(InputMode::RenameTab),
            "session" => Ok(InputMode::Session),
            e => Err(e.to_string().into()),
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum PaletteSource {
    Default,
    Xresources,
}
impl Default for PaletteSource {
    fn default() -> PaletteSource {
        PaletteSource::Default
    }
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash, Default)]
pub struct Palette {
    pub source: PaletteSource,
    pub theme_hue: ThemeHue,
    pub fg: PaletteColor,
    pub bg: PaletteColor,
    pub black: PaletteColor,
    pub red: PaletteColor,
    pub green: PaletteColor,
    pub yellow: PaletteColor,
    pub blue: PaletteColor,
    pub magenta: PaletteColor,
    pub cyan: PaletteColor,
    pub white: PaletteColor,
    pub orange: PaletteColor,
}

/// Represents the contents of the help message that is printed in the status bar,
/// which indicates the current [`InputMode`] and what the keybinds for that mode
/// are. Related to the default `status-bar` plugin.
#[derive(Default, Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ModeInfo {
    pub mode: InputMode,
    // FIXME: This should probably return Keys and Actions, then sort out strings plugin-side
    pub keybinds: Vec<(String, String)>, // <shortcut> => <shortcut description>
    pub palette: Palette,
    pub capabilities: PluginCapabilities,
    pub session_name: Option<String>,
}

impl ModeInfo {
    /// Creates a [`ModeInfo`] struct indicating the current [`InputMode`] and its keybinds
    /// (as pairs of [`String`]s).
    pub fn new(mode: InputMode, palette: Palette, capabilities: PluginCapabilities) -> Self {
        let keybinds = match mode {
            InputMode::Normal | InputMode::Locked => Vec::new(),
            InputMode::Resize => vec![("←↓↑→".to_string(), "Resize".to_string())],
            InputMode::Pane => vec![
                ("←↓↑→".to_string(), "Move focus".to_string()),
                ("p".to_string(), "Next".to_string()),
                ("n".to_string(), "New".to_string()),
                ("d".to_string(), "Down split".to_string()),
                ("r".to_string(), "Right split".to_string()),
                ("x".to_string(), "Close".to_string()),
                ("f".to_string(), "Fullscreen".to_string()),
            ],
            InputMode::Tab => vec![
                ("←↓↑→".to_string(), "Move focus".to_string()),
                ("n".to_string(), "New".to_string()),
                ("x".to_string(), "Close".to_string()),
                ("r".to_string(), "Rename".to_string()),
                ("s".to_string(), "Sync".to_string()),
            ],
            InputMode::Scroll => vec![
                ("↓↑".to_string(), "Scroll".to_string()),
                ("PgUp/PgDn".to_string(), "Scroll Page".to_string()),
            ],
            InputMode::RenameTab => vec![("Enter".to_string(), "when done".to_string())],
            InputMode::Session => vec![("d".to_string(), "Detach".to_string())],
        };

        let session_name = std::env::var("ZELLIJ_SESSION_NAME").ok();

        Self {
            mode,
            keybinds,
            palette,
            capabilities,
            session_name,
        }
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Hash, Deserialize, Serialize)]
pub struct TabInfo {
    /* subset of fields to publish to plugins */
    pub position: usize,
    pub name: String,
    pub active: bool,
    pub is_sync_panes_active: bool,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Deserialize, Serialize)]
pub struct PluginIds {
    pub plugin_id: u32,
    pub zellij_pid: u32,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Deserialize, Serialize)]
pub struct PluginCapabilities {
    pub arrow_fonts: bool,
}

impl Default for PluginCapabilities {
    fn default() -> PluginCapabilities {
        PluginCapabilities { arrow_fonts: true }
    }
}
