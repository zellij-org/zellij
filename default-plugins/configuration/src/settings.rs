use zellij_tile::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    Appearance,
    Themes,
    PaneFrames,
    Keys,
    ContextMenu,
    MouseAndClipboard,
    PanesAndLayouts,
    ScrollbackAndEditor,
    Sessions,
    TerminalSupport,
    Web,
    PluginsAndEnvironment,
    FoldersAndFiles,
}

pub const CATEGORIES: [Category; 13] = [
    Category::Appearance,
    Category::PaneFrames,
    Category::Keys,
    Category::MouseAndClipboard,
    Category::ContextMenu,
    Category::Themes,
    Category::PanesAndLayouts,
    Category::ScrollbackAndEditor,
    Category::Sessions,
    Category::TerminalSupport,
    Category::Web,
    Category::PluginsAndEnvironment,
    Category::FoldersAndFiles,
];

impl Category {
    pub fn title(&self) -> &'static str {
        match self {
            Category::Appearance => "Appearance",
            Category::Themes => "Themes",
            Category::PaneFrames => "Pane frames and borders",
            Category::Keys => "Keys",
            Category::ContextMenu => "Right-click menu",
            Category::MouseAndClipboard => "Mouse and clipboard",
            Category::PanesAndLayouts => "Panes and layouts",
            Category::ScrollbackAndEditor => "Scrollback and editor",
            Category::Sessions => "Sessions",
            Category::TerminalSupport => "Terminal support",
            Category::Web => "Web",
            Category::PluginsAndEnvironment => "Plugins and environment",
            Category::FoldersAndFiles => "Files and folders",
        }
    }
    pub fn is_keys_screen(&self) -> bool {
        matches!(self, Category::Keys)
    }
    pub fn is_page(&self) -> bool {
        matches!(
            self,
            Category::Themes | Category::ContextMenu | Category::PluginsAndEnvironment
        )
    }
    pub fn has_rows(&self) -> bool {
        !self.is_keys_screen() && !self.is_page()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    OnlyYou,
    Everyone,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextCheck {
    AnyText,
    Path,
    IpAddress,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingKind {
    Toggle,
    OptionalToggle,
    Choice(&'static [&'static str]),
    OptionalChoice(&'static [&'static str]),
    Theme { can_be_unset: bool },
    Text(TextCheck),
    Number { min: i64, max: i64, step: i64 },
    Keybindings,
    Block,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SettingInfo {
    pub name: &'static str,
    pub category: Category,
    pub description: &'static str,
    pub kind: SettingKind,
    pub default: &'static str,
    pub scope: Scope,
}

pub const UNSET_CHOICE: &str = "(not set)";
pub const MISSING_SUFFIX: &str = " (missing)";

pub fn option_value(label: &str) -> &str {
    if let Some(value) = INPUT_MODES
        .iter()
        .copied()
        .find(|value| mode_choice_label(value) == label)
    {
        return value;
    }
    label.strip_suffix(MISSING_SUFFIX).unwrap_or(label)
}

pub fn mode_choice_label(value: &str) -> String {
    crate::page::mode_label(value)
        .map(|label| label.to_owned())
        .unwrap_or_else(|| value.to_owned())
}

const LINE_STYLES: &[&str] = &["single", "double", "heavy", "dashed", "heavy_dashed"];
pub const INPUT_MODES: &[&str] = &[
    "normal", "locked", "pane", "tab", "resize", "move", "scroll", "session", "tmux",
];

fn info(
    name: &'static str,
    category: Category,
    description: &'static str,
    kind: SettingKind,
    default: &'static str,
    scope: Scope,
) -> SettingInfo {
    SettingInfo {
        name,
        category,
        description,
        kind,
        default,
        scope,
    }
}

pub fn describe(key: SettingKey) -> SettingInfo {
    use Category::*;
    use Scope::*;
    use SettingKind::*;
    match key {
        SettingKey::Theme => info(
            "Theme",
            Appearance,
            "Colour theme, previewed live while browsing",
            Theme {
                can_be_unset: false,
            },
            "default",
            Everyone,
        ),
        SettingKey::ThemeDark => info(
            "Theme (dark terminal)",
            Appearance,
            "Theme used when the terminal reports a dark palette",
            Theme { can_be_unset: true },
            "not set",
            Everyone,
        ),
        SettingKey::ThemeLight => info(
            "Theme (light terminal)",
            Appearance,
            "Theme used when the terminal reports a light palette",
            Theme { can_be_unset: true },
            "not set",
            Everyone,
        ),
        SettingKey::ExplicitThemeHue => info(
            "Theme hue",
            Appearance,
            "Pin dark or light instead of following the terminal",
            OptionalChoice(&["dark", "light"]),
            "not set",
            Everyone,
        ),
        SettingKey::SimplifiedUi => info(
            "Simplified UI",
            Appearance,
            "Avoid arrow glyphs for fonts that lack them",
            Toggle,
            "false",
            Everyone,
        ),
        SettingKey::PaneFrames => info(
            "Pane frames",
            Appearance,
            "Draw frames around panes",
            Toggle,
            "true",
            Everyone,
        ),
        SettingKey::PaneFrameStyle => info(
            "Pane frame style",
            Appearance,
            "How much of each pane frame to draw",
            Choice(&["full", "titles", "none"]),
            "titles",
            Everyone,
        ),
        SettingKey::VisualBell => info(
            "Visual bell",
            Appearance,
            "Flash pane and tab frames on a terminal bell",
            Toggle,
            "true",
            Everyone,
        ),
        SettingKey::StyledUnderlines => info(
            "Styled underlines",
            Appearance,
            "Pass curly and coloured underlines to the terminal",
            Toggle,
            "true",
            Everyone,
        ),
        SettingKey::Osc8Hyperlinks => info(
            "OSC 8 hyperlinks",
            Appearance,
            "Pass clickable hyperlinks to the terminal",
            Toggle,
            "true",
            Everyone,
        ),
        SettingKey::ThemeDir => info(
            "Theme folder",
            FoldersAndFiles,
            "Folder searched for extra theme files",
            Text(TextCheck::Path),
            "themes in the config folder",
            Everyone,
        ),
        SettingKey::FrameRoundedCorners => info(
            "Rounded corners",
            PaneFrames,
            "Round the corners of pane frames",
            Toggle,
            "false",
            Everyone,
        ),
        SettingKey::FrameHideSessionName => info(
            "Hide session name",
            PaneFrames,
            "Hide the session name in pane frames",
            Toggle,
            "false",
            OnlyYou,
        ),
        SettingKey::FrameBorderStyle => info(
            "Border style",
            PaneFrames,
            "Line style for every side of tiled pane borders",
            OptionalChoice(LINE_STYLES),
            "single",
            Everyone,
        ),
        SettingKey::FrameBorderTop => info(
            "Border top",
            PaneFrames,
            "Line style for the top border of tiled panes",
            OptionalChoice(LINE_STYLES),
            "border style",
            Everyone,
        ),
        SettingKey::FrameBorderRight => info(
            "Border right",
            PaneFrames,
            "Line style for the right border of tiled panes",
            OptionalChoice(LINE_STYLES),
            "border style",
            Everyone,
        ),
        SettingKey::FrameBorderBottom => info(
            "Border bottom",
            PaneFrames,
            "Line style for the bottom border of tiled panes",
            OptionalChoice(LINE_STYLES),
            "border style",
            Everyone,
        ),
        SettingKey::FrameBorderLeft => info(
            "Border left",
            PaneFrames,
            "Line style for the left border of tiled panes",
            OptionalChoice(LINE_STYLES),
            "border style",
            Everyone,
        ),
        SettingKey::FrameBorderRoundedCorners => info(
            "Border rounded corners",
            PaneFrames,
            "Rounded corners for tiled panes only",
            OptionalToggle,
            "rounded corners",
            Everyone,
        ),
        SettingKey::FrameFloatingBorderStyle => info(
            "Floating border style",
            PaneFrames,
            "Line style for every side of floating pane borders",
            OptionalChoice(LINE_STYLES),
            "border style",
            Everyone,
        ),
        SettingKey::FrameFloatingBorderTop => info(
            "Floating border top",
            PaneFrames,
            "Line style for the top border of floating panes",
            OptionalChoice(LINE_STYLES),
            "floating border style",
            Everyone,
        ),
        SettingKey::FrameFloatingBorderRight => info(
            "Floating border right",
            PaneFrames,
            "Line style for the right border of floating panes",
            OptionalChoice(LINE_STYLES),
            "floating border style",
            Everyone,
        ),
        SettingKey::FrameFloatingBorderBottom => info(
            "Floating border bottom",
            PaneFrames,
            "Line style for the bottom border of floating panes",
            OptionalChoice(LINE_STYLES),
            "floating border style",
            Everyone,
        ),
        SettingKey::FrameFloatingBorderLeft => info(
            "Floating border left",
            PaneFrames,
            "Line style for the left border of floating panes",
            OptionalChoice(LINE_STYLES),
            "floating border style",
            Everyone,
        ),
        SettingKey::FrameFloatingBorderRoundedCorners => info(
            "Floating rounded corners",
            PaneFrames,
            "Rounded corners for floating panes only",
            OptionalToggle,
            "border rounded corners",
            Everyone,
        ),
        SettingKey::Keybinds => info(
            "Keybindings",
            Keys,
            "Keybinding preset, leader keys and your own keybindings",
            SettingKind::Keybindings,
            "default preset",
            OnlyYou,
        ),
        SettingKey::PluginAliases => info(
            "Plugin aliases",
            PluginsAndEnvironment,
            "Names for plugins, used by layouts, keybindings and other plugins",
            Block,
            "built-in aliases",
            Everyone,
        ),
        SettingKey::LoadPlugins => info(
            "Plugins loaded at start",
            PluginsAndEnvironment,
            "Plugins loaded in the background when a session starts",
            Block,
            "built-in list",
            Everyone,
        ),
        SettingKey::Env => info(
            "Environment variables",
            PluginsAndEnvironment,
            "Variables set for new panes",
            Block,
            "none",
            Everyone,
        ),
        SettingKey::ContextMenu => info(
            "Right-click menu items",
            ContextMenu,
            "Items of the right-click menus",
            Block,
            "built-in items",
            OnlyYou,
        ),
        SettingKey::Themes => info(
            "Themes",
            Themes,
            "Themes defined in the config file",
            Block,
            "none",
            OnlyYou,
        ),
        SettingKey::MouseMode => info(
            "Mouse mode",
            MouseAndClipboard,
            "Handle mouse events (Shift bypasses it)",
            Toggle,
            "true",
            Everyone,
        ),
        SettingKey::AdvancedMouseActions => info(
            "Advanced mouse actions",
            MouseAndClipboard,
            "Hover effects and pane grouping with the mouse",
            Toggle,
            "true",
            Everyone,
        ),
        SettingKey::MouseScrollResize => info(
            "Ctrl+wheel resizes",
            MouseAndClipboard,
            "Ctrl and the mouse wheel resize panes",
            Toggle,
            "true",
            Everyone,
        ),
        SettingKey::ScrollModeSync => info(
            "Scrolling enters scroll mode",
            MouseAndClipboard,
            "Scrolling a pane enters and leaves scroll mode",
            Toggle,
            "true",
            Everyone,
        ),
        SettingKey::MouseHoverEffects => info(
            "Hover effects",
            MouseAndClipboard,
            "Highlight frames and show help text on hover",
            Toggle,
            "true",
            Everyone,
        ),
        SettingKey::MouseHoverTips => info(
            "Hover tips",
            MouseAndClipboard,
            "Show resize and grouping tips on hover",
            Toggle,
            "true",
            Everyone,
        ),
        SettingKey::FocusFollowsMouse => info(
            "Focus follows mouse",
            MouseAndClipboard,
            "Focus the pane under the mouse",
            Toggle,
            "false",
            Everyone,
        ),
        SettingKey::MouseClickThrough => info(
            "Click through",
            MouseAndClipboard,
            "A focusing click is also sent to the pane",
            Toggle,
            "false",
            Everyone,
        ),
        SettingKey::ContextMenuEnabled => info(
            "Right-click menu",
            MouseAndClipboard,
            "A right click opens the menu; when off it goes to the focused pane",
            Toggle,
            "true",
            Everyone,
        ),
        SettingKey::Osc133CommandSelection => info(
            "Select command output",
            MouseAndClipboard,
            "Triple-click selects a marked command and its output",
            Toggle,
            "true",
            Everyone,
        ),
        SettingKey::WordSeparators => info(
            "Word separators",
            MouseAndClipboard,
            "Characters that end a word on double-click",
            Text(TextCheck::AnyText),
            "[]{}<>()",
            Everyone,
        ),
        SettingKey::CopyCommand => info(
            "Copy command",
            MouseAndClipboard,
            "Command that receives copied text instead of OSC 52",
            Text(TextCheck::AnyText),
            "not set",
            Everyone,
        ),
        SettingKey::CopyClipboard => info(
            "Copy clipboard",
            MouseAndClipboard,
            "Clipboard that copied text goes to",
            Choice(&["system", "primary"]),
            "system",
            Everyone,
        ),
        SettingKey::CopyOnSelect => info(
            "Copy on select",
            MouseAndClipboard,
            "Copy text as soon as it is selected",
            Toggle,
            "true",
            Everyone,
        ),
        SettingKey::DangerouslyEnablePasteBufferRead => info(
            "Allow clipboard reads",
            MouseAndClipboard,
            "Let programs read the clipboard (risky)",
            Toggle,
            "false",
            Everyone,
        ),
        SettingKey::DefaultMode => info(
            "Default mode",
            PanesAndLayouts,
            "Input mode to return to after each action",
            Choice(INPUT_MODES),
            "normal",
            OnlyYou,
        ),
        SettingKey::DefaultShell => info(
            "Default shell",
            FoldersAndFiles,
            "Program started in new terminal panes",
            Text(TextCheck::Path),
            "$SHELL",
            Everyone,
        ),
        SettingKey::DefaultCwd => info(
            "Default folder",
            FoldersAndFiles,
            "Starting folder for new panes",
            Text(TextCheck::Path),
            "current folder",
            Everyone,
        ),
        SettingKey::DefaultLayout => info(
            "Default layout",
            FoldersAndFiles,
            "Layout name or path used for new sessions",
            Text(TextCheck::Path),
            "default",
            Everyone,
        ),
        SettingKey::KeybindsDir => info(
            "Keybinding preset folder",
            FoldersAndFiles,
            "Folder searched for keybinding presets",
            Text(TextCheck::Path),
            "keybinds in the config folder",
            Everyone,
        ),
        SettingKey::LayoutDir => info(
            "Layout folder",
            FoldersAndFiles,
            "Folder searched for layouts",
            Text(TextCheck::Path),
            "layouts in the config folder",
            Everyone,
        ),
        SettingKey::AutoLayout => info(
            "Auto layout",
            PanesAndLayouts,
            "Arrange panes with swap layouts when possible",
            Toggle,
            "true",
            Everyone,
        ),
        SettingKey::StackedResize => info(
            "Stacked resize",
            PanesAndLayouts,
            "Stack panes when resizing beyond their size",
            Toggle,
            "true",
            Everyone,
        ),
        SettingKey::StackedPaneList => info(
            "Stacked pane list",
            PanesAndLayouts,
            "Show stacked panes as a list of titles",
            Toggle,
            "true",
            Everyone,
        ),
        SettingKey::NestedSessionHandling => info(
            "Nested sessions",
            PanesAndLayouts,
            "What to do with a Zellij session inside a pane",
            Choice(&["ask", "fullscreen", "descend", "never"]),
            "ask",
            Everyone,
        ),
        SettingKey::ScrollBufferSize => info(
            "Scrollback lines",
            ScrollbackAndEditor,
            "Lines kept in each pane's scrollback",
            Number {
                min: 0,
                max: 1_000_000,
                step: 1000,
            },
            "10000",
            Everyone,
        ),
        SettingKey::ScrollbackEditor => info(
            "Scrollback editor",
            FoldersAndFiles,
            "Editor that opens a pane's scrollback",
            Text(TextCheck::Path),
            "$EDITOR or $VISUAL",
            Everyone,
        ),
        SettingKey::SessionName => info(
            "Session name",
            Sessions,
            "Name of the session created when Zellij starts",
            Text(TextCheck::AnyText),
            "random name",
            Everyone,
        ),
        SettingKey::AttachToSession => info(
            "Attach to named session",
            Sessions,
            "Attach to the named session if it already exists",
            Toggle,
            "false",
            Everyone,
        ),
        SettingKey::SessionSerialization => info(
            "Save sessions",
            Sessions,
            "Save sessions so they can be resurrected",
            Toggle,
            "true",
            Everyone,
        ),
        SettingKey::SerializePaneViewport => info(
            "Save pane contents",
            Sessions,
            "Save visible pane contents with the session",
            Toggle,
            "false",
            Everyone,
        ),
        SettingKey::ScrollbackLinesToSerialize => info(
            "Saved scrollback lines",
            Sessions,
            "Scrollback lines saved with pane contents (0 = all)",
            Number {
                min: 0,
                max: 1_000_000,
                step: 100,
            },
            "0",
            Everyone,
        ),
        SettingKey::SerializationInterval => info(
            "Save interval (seconds)",
            Sessions,
            "How often sessions are saved",
            Number {
                min: 1,
                max: 86_400,
                step: 10,
            },
            "60",
            Everyone,
        ),
        SettingKey::MirrorSession => info(
            "Mirror session",
            Sessions,
            "All users see and control the same view",
            Toggle,
            "false",
            Everyone,
        ),
        SettingKey::OnForceClose => info(
            "On force close",
            Sessions,
            "Detach or quit when the terminal closes",
            Choice(&["detach", "quit"]),
            "detach",
            Everyone,
        ),
        SettingKey::DisableSessionMetadata => info(
            "Disable session metadata",
            Sessions,
            "Do not write session details to disk",
            Toggle,
            "false",
            Everyone,
        ),
        SettingKey::PostCommandDiscoveryHook => info(
            "Command discovery hook",
            Sessions,
            "Command that edits discovered commands before saving",
            Text(TextCheck::AnyText),
            "not set",
            Everyone,
        ),
        SettingKey::ShowStartupTips => info(
            "Startup tips",
            Sessions,
            "Show a tip when a session starts",
            Toggle,
            "true",
            Everyone,
        ),
        SettingKey::ShowReleaseNotes => info(
            "Release notes",
            Sessions,
            "Show release notes after an upgrade",
            Toggle,
            "true",
            Everyone,
        ),
        SettingKey::SupportKittyKeyboardProtocol => info(
            "Kitty keyboard protocol",
            TerminalSupport,
            "Use the kitty keyboard protocol when available",
            Toggle,
            "true",
            Everyone,
        ),
        SettingKey::SupportKittyGraphicsProtocol => info(
            "Kitty graphics protocol",
            TerminalSupport,
            "Pass images through when the terminal supports it",
            Toggle,
            "true",
            Everyone,
        ),
        SettingKey::HostNotificationProtocol => info(
            "Notification protocol",
            TerminalSupport,
            "How notifications reach the outer terminal",
            Choice(&["auto", "osc9", "osc99", "bell", "off"]),
            "auto",
            Everyone,
        ),
        SettingKey::WebServer => info(
            "Web server",
            Web,
            "Start the web server with new sessions",
            Toggle,
            "false",
            Everyone,
        ),
        SettingKey::WebSharing => info(
            "Web sharing",
            Web,
            "Whether new sessions can be shared in a browser",
            Choice(&["on", "off", "disabled"]),
            "off",
            Everyone,
        ),
        SettingKey::WebServerIp => info(
            "Web server address",
            Web,
            "IP address the web server listens on",
            Text(TextCheck::IpAddress),
            "127.0.0.1",
            Everyone,
        ),
        SettingKey::WebServerPort => info(
            "Web server port",
            Web,
            "Port the web server listens on",
            Number {
                min: 1,
                max: 65_535,
                step: 1,
            },
            "8082",
            Everyone,
        ),
        SettingKey::WebServerCert => info(
            "Web server certificate",
            Web,
            "TLS certificate file for the web server",
            Text(TextCheck::Path),
            "not set",
            Everyone,
        ),
        SettingKey::WebServerKey => info(
            "Web server key",
            Web,
            "TLS key file for the web server",
            Text(TextCheck::Path),
            "not set",
            Everyone,
        ),
        SettingKey::EnforceHttpsForLocalhost => info(
            "HTTPS on localhost",
            Web,
            "Require HTTPS even on localhost",
            Toggle,
            "false",
            Everyone,
        ),
        SettingKey::ClientAsyncWorkerTasks => info(
            "Web worker tasks",
            Web,
            "Worker tasks per web client (0 = one per core)",
            Number {
                min: 0,
                max: 256,
                step: 1,
            },
            "4",
            Everyone,
        ),
        SettingKey::WebClientFont => info(
            "Web client font",
            Web,
            "Font used by the browser terminal",
            Text(TextCheck::AnyText),
            "monospace",
            Everyone,
        ),
        SettingKey::WebClientFontSize => info(
            "Web client font size",
            Web,
            "Font size used by the browser terminal",
            Number {
                min: 1,
                max: 200,
                step: 1,
            },
            "browser default",
            Everyone,
        ),
        SettingKey::WebClientCursorBlink => info(
            "Web client cursor blink",
            Web,
            "Blink the cursor in the browser terminal",
            Toggle,
            "false",
            Everyone,
        ),
        SettingKey::WebClientCursorStyle => info(
            "Web client cursor style",
            Web,
            "Cursor shape in the browser terminal",
            OptionalChoice(&["block", "bar", "underline"]),
            "block",
            Everyone,
        ),
        SettingKey::WebClientCursorInactiveStyle => info(
            "Web client inactive cursor",
            Web,
            "Cursor shape when the browser tab is not focused",
            OptionalChoice(&["outline", "block", "bar", "underline", "no_style"]),
            "outline",
            Everyone,
        ),
        SettingKey::WebClientMacOptionIsMeta => info(
            "Web client Option is Meta",
            Web,
            "Treat the macOS Option key as Meta",
            Toggle,
            "true",
            Everyone,
        ),
        SettingKey::WebClientBaseUrl => info(
            "Web client base URL",
            Web,
            "Path prefix the web client is served under",
            Text(TextCheck::AnyText),
            "/",
            Everyone,
        ),
    }
}

pub const SECTION_ORDER: [&str; 21] = [
    "Theme",
    "Display",
    "Text",
    "Frames",
    "Tiled pane borders",
    "Floating pane borders",
    "Mouse",
    "Selection",
    "Clipboard",
    "New panes",
    "Layouts",
    "Nested sessions",
    "Startup",
    "Resurrection",
    "Behaviour",
    "Server",
    "Browser client",
    "Folders",
    "Starting points",
    "Programs",
    "",
];

pub fn section(key: SettingKey) -> &'static str {
    use SettingKey::*;
    match key {
        Theme | ThemeDark | ThemeLight | ExplicitThemeHue => "Theme",
        ThemeDir | LayoutDir | KeybindsDir => "Folders",
        DefaultLayout | DefaultCwd => "Starting points",
        DefaultShell | ScrollbackEditor => "Programs",
        SimplifiedUi | PaneFrames | PaneFrameStyle | VisualBell => "Display",
        StyledUnderlines | Osc8Hyperlinks => "Text",
        FrameRoundedCorners | FrameHideSessionName => "Frames",
        FrameBorderStyle
        | FrameBorderTop
        | FrameBorderRight
        | FrameBorderBottom
        | FrameBorderLeft
        | FrameBorderRoundedCorners => "Tiled pane borders",
        FrameFloatingBorderStyle
        | FrameFloatingBorderTop
        | FrameFloatingBorderRight
        | FrameFloatingBorderBottom
        | FrameFloatingBorderLeft
        | FrameFloatingBorderRoundedCorners => "Floating pane borders",
        MouseMode | AdvancedMouseActions | MouseScrollResize | ScrollModeSync
        | MouseHoverEffects | MouseHoverTips | FocusFollowsMouse | MouseClickThrough
        | ContextMenuEnabled => "Mouse",
        Osc133CommandSelection | WordSeparators => "Selection",
        CopyCommand | CopyClipboard | CopyOnSelect | DangerouslyEnablePasteBufferRead => {
            "Clipboard"
        },
        DefaultMode => "New panes",
        AutoLayout | StackedResize | StackedPaneList => "Layouts",
        NestedSessionHandling => "Nested sessions",
        SessionName | AttachToSession | ShowStartupTips | ShowReleaseNotes => "Startup",
        SessionSerialization
        | SerializePaneViewport
        | ScrollbackLinesToSerialize
        | SerializationInterval
        | PostCommandDiscoveryHook => "Resurrection",
        MirrorSession | OnForceClose | DisableSessionMetadata => "Behaviour",
        WebServer
        | WebSharing
        | WebServerIp
        | WebServerPort
        | WebServerCert
        | WebServerKey
        | EnforceHttpsForLocalhost
        | ClientAsyncWorkerTasks => "Server",
        WebClientFont
        | WebClientFontSize
        | WebClientCursorBlink
        | WebClientCursorStyle
        | WebClientCursorInactiveStyle
        | WebClientMacOptionIsMeta
        | WebClientBaseUrl => "Browser client",
        ScrollBufferSize
        | SupportKittyKeyboardProtocol
        | SupportKittyGraphicsProtocol
        | HostNotificationProtocol
        | Keybinds
        | PluginAliases
        | LoadPlugins
        | Env
        | Themes
        | ContextMenu => "",
    }
}

fn section_index(key: SettingKey) -> usize {
    SECTION_ORDER
        .iter()
        .position(|name| *name == section(key))
        .unwrap_or(SECTION_ORDER.len())
}

pub fn category_index(category: Category) -> usize {
    CATEGORIES
        .iter()
        .position(|c| *c == category)
        .unwrap_or(CATEGORIES.len())
}

pub fn sort_for_display(keys: &mut Vec<SettingKey>) {
    keys.sort_by_key(|key| (category_index(describe(*key).category), section_index(*key)));
}

pub fn is_row_kind(kind: SettingKind) -> bool {
    !matches!(kind, SettingKind::Keybindings | SettingKind::Block)
}

pub fn settings_in(category: Category) -> Vec<SettingKey> {
    if !category.has_rows() {
        return vec![];
    }
    let mut keys = SettingKey::all()
        .into_iter()
        .filter(|key| {
            let info = describe(*key);
            info.category == category && is_row_kind(info.kind)
        })
        .collect::<Vec<_>>();
    sort_for_display(&mut keys);
    keys
}

pub fn check_text(check: TextCheck, text: &str) -> Result<(), String> {
    match check {
        TextCheck::AnyText => Ok(()),
        TextCheck::Path => {
            if text.chars().any(|c| c.is_control()) {
                Err("A path cannot contain control characters".to_owned())
            } else {
                Ok(())
            }
        },
        TextCheck::IpAddress => text
            .parse::<std::net::IpAddr>()
            .map(|_| ())
            .map_err(|_| "Not an IP address".to_owned()),
    }
}

pub fn kdl_string(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len() + 2);
    escaped.push('"');
    for character in text.chars() {
        match character {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\t' => escaped.push_str("\\t"),
            '\r' => escaped.push_str("\\r"),
            c => escaped.push(c),
        }
    }
    escaped.push('"');
    escaped
}

pub fn kdl_for(key: SettingKey, value: &str) -> String {
    key.kdl_snippet(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_setting_has_a_category_and_a_fitting_element() {
        for key in SettingKey::all() {
            let info = describe(key);
            assert!(!info.name.is_empty(), "{} has no name", key);
            assert!(!info.description.is_empty(), "{} has no description", key);
            assert!(
                CATEGORIES.contains(&info.category),
                "{} has no category",
                key
            );
            match info.kind {
                SettingKind::Number { min, max, step } => {
                    assert!(min < max && step > 0, "{} has bad limits", key)
                },
                SettingKind::Choice(choices) | SettingKind::OptionalChoice(choices) => {
                    assert!(!choices.is_empty(), "{} has no choices", key)
                },
                SettingKind::Keybindings => assert_eq!(key, SettingKey::Keybinds),
                SettingKind::Block => assert!(key.is_block(), "{} is not a block", key),
                _ => {},
            }
            assert!(
                !info.category.is_keys_screen() || info.kind == SettingKind::Keybindings,
                "{} is in a keys screen but is not shown there",
                key
            );
            let shape_fits = match info.kind {
                SettingKind::Toggle | SettingKind::OptionalToggle => {
                    key.value_shape() == SettingValueShape::Flag
                },
                SettingKind::Number { .. } => key.value_shape() == SettingValueShape::Number,
                _ => key.value_shape() == SettingValueShape::Text,
            };
            assert!(
                shape_fits,
                "{} is written to the file in the wrong shape",
                key
            );
        }
    }

    #[test]
    fn every_category_with_rows_has_settings() {
        for category in CATEGORIES {
            let has_rows = !settings_in(category).is_empty();
            assert_eq!(has_rows, category.has_rows(), "{}", category.title());
        }
    }

    #[test]
    fn every_named_section_is_in_the_display_order() {
        for key in SettingKey::all() {
            assert!(SECTION_ORDER.contains(&section(key)), "{}", key);
        }
    }

    #[test]
    fn a_missing_theme_label_maps_back_to_its_name() {
        assert_eq!(option_value("nord (missing)"), "nord");
        assert_eq!(option_value("nord"), "nord");
        assert_eq!(option_value(UNSET_CHOICE), UNSET_CHOICE);
    }

    #[test]
    fn values_are_written_as_kdl_of_the_right_type() {
        assert_eq!(kdl_for(SettingKey::MouseMode, "false"), "mouse_mode false");
        assert_eq!(kdl_for(SettingKey::Theme, "nord"), "theme \"nord\"");
        assert_eq!(
            kdl_for(SettingKey::CopyCommand, "a \"b\" \\c"),
            "copy_command \"a \\\"b\\\" \\\\c\""
        );
        assert_eq!(
            kdl_for(SettingKey::FrameBorderRoundedCorners, "true"),
            "ui {\n    pane_frames {\n        border_rounded_corners true\n    }\n}"
        );
        assert_eq!(
            kdl_for(SettingKey::WebClientFontSize, "14"),
            "web_client {\n    font_size 14\n}"
        );
    }

    #[test]
    fn mode_choices_are_shown_capitalized_and_saved_lowercase() {
        assert_eq!(mode_choice_label("normal"), "Normal");
        assert_eq!(option_value("Normal"), "normal");
        assert_eq!(option_value("Tmux"), "tmux");
        assert_eq!(option_value("nord"), "nord");
    }
}
