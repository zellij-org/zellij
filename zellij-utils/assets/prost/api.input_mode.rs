#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct InputModeMessage {
    #[prost(enumeration="InputMode", tag="1")]
    pub input_mode: i32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum InputMode {
    /// / In `Normal` mode, input is always written to the terminal, except for the shortcuts leading
    /// / to other modes
    Normal = 0,
    /// / In `Locked` mode, input is always written to the terminal and all shortcuts are disabled
    /// / except the one leading back to normal mode
    Locked = 1,
    /// / `Resize` mode allows resizing the different existing panes.
    Resize = 2,
    /// / `Pane` mode allows creating and closing panes, as well as moving between them.
    Pane = 3,
    /// / `Tab` mode allows creating and closing tabs, as well as moving between them.
    Tab = 4,
    /// / `Scroll` mode allows scrolling up and down within a pane.
    Scroll = 5,
    /// / `EnterSearch` mode allows for typing in the needle for a search in the scroll buffer of a pane.
    EnterSearch = 6,
    /// / `Search` mode allows for searching a term in a pane (superset of `Scroll`).
    Search = 7,
    /// / `RenameTab` mode allows assigning a new name to a tab.
    RenameTab = 8,
    /// / `RenamePane` mode allows assigning a new name to a pane.
    RenamePane = 9,
    /// / `Session` mode allows detaching sessions
    Session = 10,
    /// / `Move` mode allows moving the different existing panes within a tab
    Move = 11,
    /// / `Prompt` mode allows interacting with active prompts.
    Prompt = 12,
    /// / `Tmux` mode allows for basic tmux keybindings functionality
    Tmux = 13,
}
impl InputMode {
    /// String value of the enum field names used in the ProtoBuf definition.
    ///
    /// The values are not transformed in any way and thus are considered stable
    /// (if the ProtoBuf definition does not change) and safe for programmatic use.
    pub fn as_str_name(&self) -> &'static str {
        match self {
            InputMode::Normal => "Normal",
            InputMode::Locked => "Locked",
            InputMode::Resize => "Resize",
            InputMode::Pane => "Pane",
            InputMode::Tab => "Tab",
            InputMode::Scroll => "Scroll",
            InputMode::EnterSearch => "EnterSearch",
            InputMode::Search => "Search",
            InputMode::RenameTab => "RenameTab",
            InputMode::RenamePane => "RenamePane",
            InputMode::Session => "Session",
            InputMode::Move => "Move",
            InputMode::Prompt => "Prompt",
            InputMode::Tmux => "Tmux",
        }
    }
    /// Creates an enum from field names used in the ProtoBuf definition.
    pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
        match value {
            "Normal" => Some(Self::Normal),
            "Locked" => Some(Self::Locked),
            "Resize" => Some(Self::Resize),
            "Pane" => Some(Self::Pane),
            "Tab" => Some(Self::Tab),
            "Scroll" => Some(Self::Scroll),
            "EnterSearch" => Some(Self::EnterSearch),
            "Search" => Some(Self::Search),
            "RenameTab" => Some(Self::RenameTab),
            "RenamePane" => Some(Self::RenamePane),
            "Session" => Some(Self::Session),
            "Move" => Some(Self::Move),
            "Prompt" => Some(Self::Prompt),
            "Tmux" => Some(Self::Tmux),
            _ => None,
        }
    }
}
