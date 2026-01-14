// This is a converter from the old yaml config to the new KDL config.
//
// It is supposed to be mostly self containing - please refrain from adding to it, importing
// from it or changing it
use std::fmt;
use std::path::PathBuf;

use serde::de::{Error, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::HashMap;
use url::Url;

const ON_FORCE_CLOSE_DESCRIPTION: &'static str = "
// Choose what to do when zellij receives SIGTERM, SIGINT, SIGQUIT or SIGHUP
// eg. when terminal window with an active zellij session is closed
// Options:
//   - detach (Default)
//   - quit
//
";

const SIMPLIFIED_UI_DESCRIPTION: &'static str = "
// Send a request for a simplified ui (without arrow fonts) to plugins
// Options:
//   - true
//   - false (Default)
//
";

const DEFAULT_SHELL_DESCRIPTION: &'static str = "
// Choose the path to the default shell that zellij will use for opening new panes
// Default: $SHELL
//
";

const PANE_FRAMES_DESCRIPTION: &'static str = "
// Toggle between having pane frames around the panes
// Options:
//   - true (default)
//   - false
//
";

const DEFAULT_THEME_DESCRIPTION: &'static str = "
// Choose the theme that is specified in the themes section.
// Default: default
//
";

const DEFAULT_MODE_DESCRIPTION: &'static str = "
// Choose the mode that zellij uses when starting up.
// Default: normal
//
";

const MOUSE_MODE_DESCRIPTION: &'static str = "
// Toggle enabling the mouse mode.
// On certain configurations, or terminals this could
// potentially interfere with copying text.
// Options:
//   - true (default)
//   - false
//
";

const SCROLL_BUFFER_SIZE_DESCRIPTION: &'static str = "
// Configure the scroll back buffer size
// This is the number of lines zellij stores for each pane in the scroll back
// buffer. Excess number of lines are discarded in a FIFO fashion.
// Valid values: positive integers
// Default value: 10000
//
";

const COPY_COMMAND_DESCRIPTION: &'static str = "
// Provide a command to execute when copying text. The text will be piped to
// the stdin of the program to perform the copy. This can be used with
// terminal emulators which do not support the OSC 52 ANSI control sequence
// that will be used by default if this option is not set.
// Examples:
//
// copy_command \"xclip -selection clipboard\" // x11
// copy_command \"wl-copy\"                    // wayland
// copy_command \"pbcopy\"                     // osx
";

const COPY_CLIPBOARD_DESCRIPTION: &'static str = "
// Choose the destination for copied text
// Allows using the primary selection buffer (on x11/wayland) instead of the system clipboard.
// Does not apply when using copy_command.
// Options:
//   - system (default)
//   - primary
//
";

const COPY_ON_SELECT_DESCRIPTION: &'static str = "
// Enable or disable automatic copy (and clear) of selection when releasing mouse
// Default: true
//
";

const SCROLLBACK_EDITOR_DESCRIPTION: &'static str = "
// Path to the default editor to use to edit pane scrollbuffer
// Default: $EDITOR or $VISUAL
//
";

const MIRROR_SESSION_DESCRIPTION: &'static str = "
// When attaching to an existing session with other users,
// should the session be mirrored (true)
// or should each user have their own cursor (false)
// Default: false
//
";

const DEFAULT_LAYOUT_DESCRIPTION: &'static str = "
// The name of the default layout to load on startup
// Default: \"default\"
//
";

const LAYOUT_DIR_DESCRIPTION: &'static str = "
// The folder in which Zellij will look for layouts
//
";

const THEME_DIR_DESCRIPTION: &'static str = "
// The folder in which Zellij will look for themes
//
";

fn options_yaml_to_options_kdl(options_yaml: &OldOptions, no_comments: bool) -> String {
    let mut options_kdl = String::new();

    macro_rules! push_option {
        ($attribute_name:ident, $description_text:ident, $present_pattern:expr) => {
            if !no_comments {
                options_kdl.push_str($description_text);
            }
            if let Some($attribute_name) = &options_yaml.$attribute_name {
                options_kdl.push_str(&format!($present_pattern, $attribute_name));
                options_kdl.push('\n');
            };
        };
        ($attribute_name:ident, $description_text:ident, $present_pattern:expr, $absent_pattern:expr) => {
            if !no_comments {
                options_kdl.push_str($description_text);
            }
            match &options_yaml.$attribute_name {
                Some($attribute_name) => {
                    options_kdl.push_str(&format!($present_pattern, $attribute_name));
                },
                None => {
                    if !no_comments {
                        options_kdl.push_str(&format!($absent_pattern));
                    }
                },
            };
            if !no_comments || options_yaml.$attribute_name.is_some() {
                options_kdl.push('\n');
            }
        };
    }

    push_option!(
        on_force_close,
        ON_FORCE_CLOSE_DESCRIPTION,
        "on_force_close \"{}\"",
        "// on_force_close \"quit\""
    );
    push_option!(
        simplified_ui,
        SIMPLIFIED_UI_DESCRIPTION,
        "simplified_ui {}",
        "// simplified_ui true"
    );
    push_option!(
        default_shell,
        DEFAULT_SHELL_DESCRIPTION,
        "default_shell {:?}",
        "// default_shell \"fish\""
    );
    push_option!(
        pane_frames,
        PANE_FRAMES_DESCRIPTION,
        "pane_frames {}",
        "// pane_frames true"
    );
    push_option!(
        theme,
        DEFAULT_THEME_DESCRIPTION,
        "theme {:?} ",
        "// theme \"default\""
    );
    push_option!(
        default_layout,
        DEFAULT_LAYOUT_DESCRIPTION,
        "default_layout {:?}",
        "// default_layout \"compact\""
    );
    push_option!(
        default_mode,
        DEFAULT_MODE_DESCRIPTION,
        "default_mode \"{}\"",
        "// default_mode \"locked\""
    );
    push_option!(
        mouse_mode,
        MOUSE_MODE_DESCRIPTION,
        "mouse_mode {}",
        "// mouse_mode false"
    );
    push_option!(
        scroll_buffer_size,
        SCROLL_BUFFER_SIZE_DESCRIPTION,
        "scroll_buffer_size {}",
        "// scroll_buffer_size 10000"
    );
    push_option!(copy_command, COPY_COMMAND_DESCRIPTION, "copy_command {:?}");
    push_option!(
        copy_clipboard,
        COPY_CLIPBOARD_DESCRIPTION,
        "copy_clipboard \"{}\"",
        "// copy_clipboard \"primary\""
    );
    push_option!(
        copy_on_select,
        COPY_ON_SELECT_DESCRIPTION,
        "copy_on_select {}",
        "// copy_on_select false"
    );
    push_option!(
        scrollback_editor,
        SCROLLBACK_EDITOR_DESCRIPTION,
        "scrollback_editor {:?}",
        "// scrollback_editor \"/usr/bin/vim\""
    );
    push_option!(
        mirror_session,
        MIRROR_SESSION_DESCRIPTION,
        "mirror_session {}",
        "// mirror_session true"
    );
    push_option!(
        layout_dir,
        LAYOUT_DIR_DESCRIPTION,
        "layout_dir {:?}",
        "// layout_dir /path/to/my/layout_dir"
    );
    push_option!(
        theme_dir,
        THEME_DIR_DESCRIPTION,
        "theme_dir {:?}",
        "// theme_dir \"/path/to/my/theme_dir\""
    );

    options_kdl
}

fn env_yaml_to_env_kdl(env_yaml: &OldEnvironmentVariablesFromYaml) -> String {
    let mut env_kdl = String::new();
    let mut env_vars: Vec<(String, String)> = env_yaml
        .env
        .iter()
        .map(|(name, val)| (name.clone(), val.clone()))
        .collect();
    env_vars.sort_unstable();
    env_kdl.push_str("env {\n");
    for (name, val) in env_vars {
        env_kdl.push_str(&format!("    {} \"{}\"\n", name, val));
    }
    env_kdl.push_str("}\n");
    env_kdl
}

fn plugins_yaml_to_plugins_kdl(plugins_yaml_to_plugins_kdl: &OldPluginsConfigFromYaml) -> String {
    let mut plugins_kdl = String::new();
    if !&plugins_yaml_to_plugins_kdl.0.is_empty() {
        plugins_kdl.push_str("\n");
        plugins_kdl.push_str("plugins {\n")
    }
    for plugin_config in &plugins_yaml_to_plugins_kdl.0 {
        if plugin_config._allow_exec_host_cmd {
            plugins_kdl.push_str(&format!(
                "    {} {{ path {:?}; _allow_exec_host_cmd true; }}\n",
                plugin_config.tag.0, plugin_config.path
            ));
        } else {
            plugins_kdl.push_str(&format!(
                "    {} {{ path {:?}; }}\n",
                plugin_config.tag.0, plugin_config.path
            ));
        }
    }
    if !&plugins_yaml_to_plugins_kdl.0.is_empty() {
        plugins_kdl.push_str("}\n")
    }
    plugins_kdl
}

fn ui_config_yaml_to_ui_config_kdl(ui_config_yaml: &OldUiConfigFromYaml) -> String {
    let mut kdl_ui_config = String::new();
    if ui_config_yaml.pane_frames.rounded_corners {
        kdl_ui_config.push_str("\n");
        kdl_ui_config.push_str("ui {\n");
        kdl_ui_config.push_str("    pane_frames {\n");
        kdl_ui_config.push_str("        rounded_corners true\n");
        kdl_ui_config.push_str("    }\n");
        kdl_ui_config.push_str("}\n");
    } else {
        // I'm not sure this is a thing, but since it's possible, why not?
        kdl_ui_config.push_str("\n");
        kdl_ui_config.push_str("ui {\n");
        kdl_ui_config.push_str("    pane_frames {\n");
        kdl_ui_config.push_str("        rounded_corners false\n");
        kdl_ui_config.push_str("    }\n");
        kdl_ui_config.push_str("}\n");
    }
    kdl_ui_config
}

fn theme_config_yaml_to_theme_config_kdl(
    theme_config_yaml: &OldThemesFromYamlIntermediate,
) -> String {
    macro_rules! theme_color {
        ($theme:ident, $color:ident, $color_name:expr, $kdl_theme_config:expr) => {
            match $theme.palette.$color {
                OldPaletteColorFromYaml::Rgb((r, g, b)) => {
                    $kdl_theme_config
                        .push_str(&format!("        {} {} {} {}\n", $color_name, r, g, b));
                },
                OldPaletteColorFromYaml::EightBit(eight_bit_color) => {
                    $kdl_theme_config
                        .push_str(&format!("        {} {}\n", $color_name, eight_bit_color));
                },
                OldPaletteColorFromYaml::Hex(OldHexColor(r, g, b)) => {
                    $kdl_theme_config
                        .push_str(&format!("        {} {} {} {}\n", $color_name, r, g, b));
                },
            }
        };
    }

    let mut kdl_theme_config = String::new();
    if !theme_config_yaml.0.is_empty() {
        kdl_theme_config.push_str("themes {\n")
    }
    let mut themes: Vec<(String, OldTheme)> = theme_config_yaml
        .0
        .iter()
        .map(|(theme_name, theme)| (theme_name.clone(), theme.clone()))
        .collect();
    themes.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    for (theme_name, theme) in themes {
        kdl_theme_config.push_str(&format!("    {} {{\n", theme_name));
        theme_color!(theme, fg, "fg", kdl_theme_config);
        theme_color!(theme, bg, "bg", kdl_theme_config);
        theme_color!(theme, black, "black", kdl_theme_config);
        theme_color!(theme, red, "red", kdl_theme_config);
        theme_color!(theme, green, "green", kdl_theme_config);
        theme_color!(theme, yellow, "yellow", kdl_theme_config);
        theme_color!(theme, blue, "blue", kdl_theme_config);
        theme_color!(theme, magenta, "magenta", kdl_theme_config);
        theme_color!(theme, cyan, "cyan", kdl_theme_config);
        theme_color!(theme, white, "white", kdl_theme_config);
        theme_color!(theme, orange, "orange", kdl_theme_config);
        kdl_theme_config.push_str("    }\n");
    }
    if !theme_config_yaml.0.is_empty() {
        kdl_theme_config.push_str("}\n")
    }
    kdl_theme_config
}

fn keybinds_yaml_to_keybinds_kdl(keybinds_yaml: &OldKeybindsFromYaml) -> String {
    let mut kdl_keybinds = String::new();
    let modes = vec![
        // mode sort order
        OldInputMode::Normal,
        OldInputMode::Locked,
        OldInputMode::Pane,
        OldInputMode::Tab,
        OldInputMode::Resize,
        OldInputMode::Move,
        OldInputMode::Scroll,
        OldInputMode::Session,
        OldInputMode::Search,
        OldInputMode::EnterSearch,
        OldInputMode::RenameTab,
        OldInputMode::RenamePane,
        OldInputMode::Prompt,
        OldInputMode::Tmux,
    ];

    // title and global unbinds / clear-defaults
    match &keybinds_yaml.unbind {
        OldUnbind::Keys(keys_to_unbind) => {
            kdl_keybinds.push_str("keybinds {\n");
            let key_string: String = keys_to_unbind
                .iter()
                .map(|k| format!("\"{}\"", k))
                .collect::<Vec<String>>()
                .join(" ");
            kdl_keybinds.push_str(&format!("    unbind {}\n", key_string));
        },
        OldUnbind::All(should_unbind_all_defaults) => {
            if *should_unbind_all_defaults {
                kdl_keybinds.push_str("keybinds clear-defaults=true {\n");
            } else {
                kdl_keybinds.push_str("keybinds {\n");
            }
        },
    }

    for mode in modes {
        if let Some(mode_keybinds) = keybinds_yaml.keybinds.get(&mode) {
            let mut should_clear_mode_defaults = false;
            let mut kdl_mode_keybinds = String::new();
            for key_action_unbind in mode_keybinds {
                match key_action_unbind {
                    OldKeyActionUnbind::KeyAction(key_action) => {
                        let keys = &key_action.key;
                        let actions = &key_action.action;
                        let key_string: String = keys
                            .iter()
                            .map(|k| {
                                if k == &OldKey::Char('\\') {
                                    format!("r\"{}\"", k)
                                } else {
                                    format!("\"{}\"", k)
                                }
                            })
                            .collect::<Vec<String>>()
                            .join(" ");
                        let actions_string: String = actions
                            .iter()
                            .map(|a| format!("{};", a))
                            .collect::<Vec<String>>()
                            .join(" ");
                        kdl_mode_keybinds.push_str(&format!(
                            "        bind {} {{ {} }}\n",
                            key_string, actions_string
                        ));
                    },
                    OldKeyActionUnbind::Unbind(unbind) => match &unbind.unbind {
                        OldUnbind::Keys(keys_to_unbind) => {
                            let key_string: String = keys_to_unbind
                                .iter()
                                .map(|k| format!("\"{}\"", k))
                                .collect::<Vec<String>>()
                                .join(" ");
                            kdl_mode_keybinds.push_str(&format!("        unbind {}\n", key_string));
                        },
                        OldUnbind::All(unbind_all) => {
                            if *unbind_all {
                                should_clear_mode_defaults = true;
                            }
                        },
                    },
                }
            }
            if should_clear_mode_defaults {
                kdl_keybinds.push_str(&format!("    {} clear-defaults=true {{\n", mode));
            } else {
                kdl_keybinds.push_str(&format!("    {} {{\n", mode));
            }
            kdl_keybinds.push_str(&kdl_mode_keybinds);
            kdl_keybinds.push_str("    }\n");
        }
    }
    kdl_keybinds.push_str("}\n");
    kdl_keybinds
}

pub fn config_yaml_to_config_kdl(
    raw_yaml_config: &str,
    no_comments: bool,
) -> Result<String, String> {
    // returns the raw kdl config
    let config_from_yaml: OldConfigFromYaml = serde_yaml::from_str(raw_yaml_config)
        .map_err(|e| format!("Failed to parse yaml: {:?}", e))?;
    let mut kdl_config = String::new();
    if let Some(old_config_keybinds) = config_from_yaml.keybinds.as_ref() {
        kdl_config.push_str(&keybinds_yaml_to_keybinds_kdl(old_config_keybinds));
    }
    if let Some(old_config_options) = config_from_yaml.options.as_ref() {
        kdl_config.push_str(&options_yaml_to_options_kdl(
            old_config_options,
            no_comments,
        ));
    }
    if let Some(old_config_env_variables) = config_from_yaml.env.as_ref() {
        kdl_config.push_str(&env_yaml_to_env_kdl(old_config_env_variables));
    }
    kdl_config.push_str(&plugins_yaml_to_plugins_kdl(&config_from_yaml.plugins));
    if let Some(old_ui_config) = config_from_yaml.ui.as_ref() {
        kdl_config.push_str(&ui_config_yaml_to_ui_config_kdl(old_ui_config));
    }
    if let Some(old_themes_config) = config_from_yaml.themes.as_ref() {
        kdl_config.push_str(&theme_config_yaml_to_theme_config_kdl(old_themes_config));
    }
    Ok(kdl_config)
}

#[derive(Clone, Default, Debug, Deserialize, Serialize, PartialEq)]
pub struct OldConfigFromYaml {
    #[serde(flatten)]
    pub options: Option<OldOptions>,
    pub keybinds: Option<OldKeybindsFromYaml>,
    pub themes: Option<OldThemesFromYamlIntermediate>,
    #[serde(flatten)]
    pub env: Option<OldEnvironmentVariablesFromYaml>,
    #[serde(default)]
    pub plugins: OldPluginsConfigFromYaml,
    pub ui: Option<OldUiConfigFromYaml>,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct OldKeybindsFromYaml {
    #[serde(flatten)]
    keybinds: HashMap<OldInputMode, Vec<OldKeyActionUnbind>>,
    #[serde(default)]
    unbind: OldUnbind,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Deserialize, Serialize)]
#[serde(untagged)]
enum OldUnbind {
    // This is the correct order, don't rearrange!
    // Suspected Bug in the untagged macro.
    // 1. Keys
    Keys(Vec<OldKey>),
    // 2. All
    All(bool),
}

impl Default for OldUnbind {
    fn default() -> OldUnbind {
        OldUnbind::All(false)
    }
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(untagged)]
enum OldKeyActionUnbind {
    KeyAction(OldKeyActionFromYaml),
    Unbind(OldUnbindFromYaml),
}

/// Intermediate struct used for deserialisation
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
struct OldKeyActionFromYaml {
    action: Vec<OldAction>,
    key: Vec<OldKey>,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
struct OldUnbindFromYaml {
    unbind: OldUnbind,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct OldThemesFromYamlIntermediate(HashMap<String, OldTheme>);

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash, Default)]
struct OldPaletteFromYaml {
    pub fg: OldPaletteColorFromYaml,
    pub bg: OldPaletteColorFromYaml,
    pub black: OldPaletteColorFromYaml,
    pub red: OldPaletteColorFromYaml,
    pub green: OldPaletteColorFromYaml,
    pub yellow: OldPaletteColorFromYaml,
    pub blue: OldPaletteColorFromYaml,
    pub magenta: OldPaletteColorFromYaml,
    pub cyan: OldPaletteColorFromYaml,
    pub white: OldPaletteColorFromYaml,
    pub orange: OldPaletteColorFromYaml,
}

/// Intermediate deserialization enum
// This is here in order to make the untagged enum work
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(untagged)]
enum OldPaletteColorFromYaml {
    Rgb((u8, u8, u8)),
    EightBit(u8),
    Hex(OldHexColor),
}

impl From<OldHexColor> for (u8, u8, u8) {
    fn from(e: OldHexColor) -> (u8, u8, u8) {
        let OldHexColor(r, g, b) = e;
        (r, g, b)
    }
}

struct OldHexColorVisitor();

impl<'de> Visitor<'de> for OldHexColorVisitor {
    type Value = OldHexColor;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        write!(formatter, "a hex color in the format #RGB or #RRGGBB")
    }

    fn visit_str<E>(self, s: &str) -> Result<Self::Value, E>
    where
        E: Error,
    {
        if let Some(stripped) = s.strip_prefix('#') {
            return self.visit_str(stripped);
        }

        if s.len() == 3 {
            Ok(OldHexColor(
                u8::from_str_radix(&s[0..1], 16).map_err(E::custom)? * 0x11,
                u8::from_str_radix(&s[1..2], 16).map_err(E::custom)? * 0x11,
                u8::from_str_radix(&s[2..3], 16).map_err(E::custom)? * 0x11,
            ))
        } else if s.len() == 6 {
            Ok(OldHexColor(
                u8::from_str_radix(&s[0..2], 16).map_err(E::custom)?,
                u8::from_str_radix(&s[2..4], 16).map_err(E::custom)?,
                u8::from_str_radix(&s[4..6], 16).map_err(E::custom)?,
            ))
        } else {
            Err(Error::custom(
                "Hex color must be of form \"#RGB\" or \"#RRGGBB\"",
            ))
        }
    }
}

impl<'de> Deserialize<'de> for OldHexColor {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_str(OldHexColorVisitor())
    }
}

impl Default for OldPaletteColorFromYaml {
    fn default() -> Self {
        OldPaletteColorFromYaml::EightBit(0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
struct OldHexColor(u8, u8, u8);

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
struct OldTheme {
    #[serde(flatten)]
    palette: OldPaletteFromYaml,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Deserialize, Serialize)]
pub struct OldUiConfigFromYaml {
    pub pane_frames: OldFrameConfigFromYaml,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Deserialize, Serialize)]
pub struct OldFrameConfigFromYaml {
    pub rounded_corners: bool,
}

#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct OldEnvironmentVariablesFromYaml {
    env: HashMap<String, String>,
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize)]
pub struct OldPluginsConfigFromYaml(Vec<OldPluginConfigFromYaml>);

#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize)]
struct OldPluginConfigFromYaml {
    pub path: PathBuf,
    pub tag: OldPluginTag,
    #[serde(default)]
    pub run: OldPluginTypeFromYaml,
    #[serde(default)]
    pub config: serde_yaml::Value,
    #[serde(default)]
    pub _allow_exec_host_cmd: bool,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
enum OldPluginTypeFromYaml {
    Headless,
    Pane,
}

impl Default for OldPluginTypeFromYaml {
    fn default() -> Self {
        Self::Pane
    }
}

/// Tag used to identify the plugin in layout and config yaml files
#[derive(Debug, Default, Clone, PartialEq, Eq, Hash, Deserialize, Serialize)]
struct OldPluginTag(String);

#[derive(Copy, Clone, Debug, PartialEq, Deserialize, Serialize)]
pub enum OldOnForceClose {
    #[serde(alias = "quit")]
    Quit,
    #[serde(alias = "detach")]
    Detach,
}

impl std::fmt::Display for OldOnForceClose {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        match self {
            Self::Quit => write!(f, "quit"),
            Self::Detach => write!(f, "detach"),
        }
    }
}

impl Default for OldOnForceClose {
    fn default() -> Self {
        Self::Detach
    }
}

#[derive(Deserialize, Serialize, Debug, Clone, Copy, PartialEq)]
pub enum OldClipboard {
    #[serde(alias = "system")]
    System,
    #[serde(alias = "primary")]
    Primary,
}

impl std::fmt::Display for OldClipboard {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        match self {
            Self::System => write!(f, "system"),
            Self::Primary => write!(f, "primary"),
        }
    }
}

impl Default for OldClipboard {
    fn default() -> Self {
        Self::System
    }
}

#[derive(Clone, Default, Debug, PartialEq, Deserialize, Serialize)]
pub struct OldOptions {
    #[serde(default)]
    pub simplified_ui: Option<bool>,
    pub theme: Option<String>,
    pub default_mode: Option<OldInputMode>,
    pub default_shell: Option<PathBuf>,
    pub default_layout: Option<PathBuf>,
    pub layout_dir: Option<PathBuf>,
    pub theme_dir: Option<PathBuf>,
    #[serde(default)]
    pub mouse_mode: Option<bool>,
    #[serde(default)]
    pub pane_frames: Option<bool>,
    #[serde(default)]
    pub mirror_session: Option<bool>,
    pub on_force_close: Option<OldOnForceClose>,
    pub scroll_buffer_size: Option<usize>,
    #[serde(default)]
    pub copy_command: Option<String>,
    #[serde(default)]
    pub copy_clipboard: Option<OldClipboard>,
    #[serde(default)]
    pub copy_on_select: Option<bool>,
    pub scrollback_editor: Option<PathBuf>,
}

/// Describes the different input modes, which change the way that keystrokes will be interpreted.
#[derive(Debug, PartialEq, Eq, Hash, Copy, Clone, Serialize, Deserialize)]
pub enum OldInputMode {
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
    /// `EnterSearch` mode allows for typing in the needle for a search in the scroll buffer of a pane.
    #[serde(alias = "entersearch")]
    EnterSearch,
    /// `Search` mode allows for searching a term in a pane (superset of `Scroll`).
    #[serde(alias = "search")]
    Search,
    /// `RenameTab` mode allows assigning a new name to a tab.
    #[serde(alias = "renametab")]
    RenameTab,
    /// `RenamePane` mode allows assigning a new name to a pane.
    #[serde(alias = "renamepane")]
    RenamePane,
    /// `Session` mode allows detaching sessions
    #[serde(alias = "session")]
    Session,
    /// `Move` mode allows moving the different existing panes within a tab
    #[serde(alias = "move")]
    Move,
    /// `Prompt` mode allows interacting with active prompts.
    #[serde(alias = "prompt")]
    Prompt,
    /// `Tmux` mode allows for basic tmux keybindings functionality
    #[serde(alias = "tmux")]
    Tmux,
}

impl std::fmt::Display for OldInputMode {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        match self {
            Self::Normal => write!(f, "normal"),
            Self::Locked => write!(f, "locked"),
            Self::Resize => write!(f, "resize"),
            Self::Pane => write!(f, "pane"),
            Self::Tab => write!(f, "tab"),
            Self::Scroll => write!(f, "scroll"),
            Self::EnterSearch => write!(f, "entersearch"),
            Self::Search => write!(f, "search"),
            Self::RenameTab => write!(f, "RenameTab"),
            Self::RenamePane => write!(f, "RenamePane"),
            Self::Session => write!(f, "session"),
            Self::Move => write!(f, "move"),
            Self::Prompt => write!(f, "prompt"),
            Self::Tmux => write!(f, "tmux"),
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
enum OldKey {
    PageDown,
    PageUp,
    Left,
    Down,
    Up,
    Right,
    Home,
    End,
    Backspace,
    Delete,
    Insert,
    F(u8),
    Char(char),
    Alt(OldCharOrArrow),
    Ctrl(char),
    BackTab,
    Null,
    Esc,
}

impl std::fmt::Display for OldKey {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        match self {
            Self::PageDown => write!(f, "PageDown"),
            Self::PageUp => write!(f, "PageUp"),
            Self::Left => write!(f, "Left"),
            Self::Down => write!(f, "Down"),
            Self::Up => write!(f, "Up"),
            Self::Right => write!(f, "Right"),
            Self::Home => write!(f, "Home"),
            Self::End => write!(f, "End"),
            Self::Backspace => write!(f, "Backspace"),
            Self::Delete => write!(f, "Delete"),
            Self::Insert => write!(f, "Insert"),
            Self::F(index) => write!(f, "F{}", index),
            Self::Char(c) => match c {
                '\n' => write!(f, "Enter"),
                '\t' => write!(f, "Tab"),
                '\"' => write!(f, "\\\""), // make sure it is escaped because otherwise it will be
                // seen as a KDL string starter/terminator
                ' ' => write!(f, "Space"),
                _ => write!(f, "{}", c),
            },
            Self::Alt(char_or_arrow) => match char_or_arrow {
                OldCharOrArrow::Char(c) => write!(f, "Alt {}", c),
                OldCharOrArrow::Direction(direction) => match direction {
                    OldDirection::Left => write!(f, "Alt Left"),
                    OldDirection::Right => write!(f, "Alt Right"),
                    OldDirection::Up => write!(f, "Alt Up"),
                    OldDirection::Down => write!(f, "Alt Down"),
                },
            },
            Self::Ctrl(c) => write!(f, "Ctrl {}", c),
            Self::BackTab => write!(f, "Tab"),
            Self::Null => write!(f, "Null"),
            Self::Esc => write!(f, "Esc"),
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
#[serde(untagged)]
enum OldCharOrArrow {
    Char(char),
    Direction(OldDirection),
}

/// The four directions (left, right, up, down).
#[derive(Eq, Clone, Copy, Debug, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
enum OldDirection {
    Left,
    Right,
    Up,
    Down,
}

impl std::fmt::Display for OldDirection {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        match self {
            Self::Left => write!(f, "Left"),
            Self::Right => write!(f, "Right"),
            Self::Up => write!(f, "Up"),
            Self::Down => write!(f, "Down"),
        }
    }
}

impl Default for OldDirection {
    fn default() -> Self {
        OldDirection::Left
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
enum OldAction {
    Quit,
    Write(Vec<u8>),
    WriteChars(String),
    SwitchToMode(OldInputMode),
    Resize(OldResizeDirection),
    FocusNextPane,
    FocusPreviousPane,
    SwitchFocus,
    MoveFocus(OldDirection),
    MoveFocusOrTab(OldDirection),
    MovePane(Option<OldDirection>),
    DumpScreen(String),
    EditScrollback,
    ScrollUp,
    ScrollUpAt(OldPosition),
    ScrollDown,
    ScrollDownAt(OldPosition),
    ScrollToBottom,
    PageScrollUp,
    PageScrollDown,
    HalfPageScrollUp,
    HalfPageScrollDown,
    ToggleFocusFullscreen,
    TogglePaneFrames,
    ToggleActiveSyncTab,
    NewPane(Option<OldDirection>),
    TogglePaneEmbedOrFloating,
    ToggleFloatingPanes,
    CloseFocus,
    PaneNameInput(Vec<u8>),
    UndoRenamePane,
    NewTab(Option<OldTabLayout>),
    NoOp,
    GoToNextTab,
    GoToPreviousTab,
    CloseTab,
    GoToTab(u32),
    ToggleTab,
    TabNameInput(Vec<u8>),
    UndoRenameTab,
    Run(OldRunCommandAction),
    Detach,
    LeftClick(OldPosition),
    RightClick(OldPosition),
    MiddleClick(OldPosition),
    LeftMouseRelease(OldPosition),
    RightMouseRelease(OldPosition),
    MiddleMouseRelease(OldPosition),
    MouseHoldLeft(OldPosition),
    MouseHoldRight(OldPosition),
    MouseHoldMiddle(OldPosition),
    Copy,
    Confirm,
    Deny,
    SkipConfirm(Box<OldAction>),
    SearchInput(Vec<u8>),
    Search(OldSearchDirection),
    SearchToggleOption(OldSearchOption),
}

impl std::fmt::Display for OldAction {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        match self {
            Self::Quit => write!(f, "Quit"),
            Self::Write(bytes) => write!(
                f,
                "Write {}",
                bytes
                    .iter()
                    .map(|c| format!("{}", *c))
                    .collect::<Vec<String>>()
                    .join(" ")
            ),
            Self::WriteChars(chars) => write!(f, "WriteChars \"{}\"", chars),
            Self::SwitchToMode(input_mode) => write!(f, "SwitchToMode \"{}\"", input_mode),
            Self::Resize(resize_direction) => write!(f, "Resize \"{}\"", resize_direction),
            Self::FocusNextPane => write!(f, "FocusNextPane"),
            Self::FocusPreviousPane => write!(f, "FocusPreviousPane"),
            Self::SwitchFocus => write!(f, "SwitchFocus"),
            Self::MoveFocus(direction) => write!(f, "MoveFocus \"{}\"", direction),
            Self::MoveFocusOrTab(direction) => write!(f, "MoveFocusOrTab \"{}\"", direction),
            Self::MovePane(direction) => match direction {
                Some(direction) => write!(f, "MovePane \"{}\"", direction),
                None => write!(f, "MovePane"),
            },
            Self::DumpScreen(file) => write!(f, "DumpScreen \"{}\"", file),
            Self::EditScrollback => write!(f, "EditScrollback"),
            Self::ScrollUp => write!(f, "ScrollUp"),
            Self::ScrollDown => write!(f, "ScrollDown"),
            Self::ScrollToBottom => write!(f, "ScrollToBottom"),
            Self::PageScrollUp => write!(f, "PageScrollUp"),
            Self::PageScrollDown => write!(f, "PageScrollDown"),
            Self::HalfPageScrollUp => write!(f, "HalfPageScrollUp"),
            Self::HalfPageScrollDown => write!(f, "HalfPageScrollDown"),
            Self::ToggleFocusFullscreen => write!(f, "ToggleFocusFullscreen"),
            Self::TogglePaneFrames => write!(f, "TogglePaneFrames"),
            Self::ToggleActiveSyncTab => write!(f, "ToggleActiveSyncTab"),
            Self::NewPane(direction) => match direction {
                Some(direction) => write!(f, "NewPane \"{}\"", direction),
                None => write!(f, "NewPane"),
            },
            Self::TogglePaneEmbedOrFloating => write!(f, "TogglePaneEmbedOrFloating"),
            Self::ToggleFloatingPanes => write!(f, "ToggleFloatingPanes"),
            Self::CloseFocus => write!(f, "CloseFocus"),
            Self::PaneNameInput(bytes) => write!(
                f,
                "PaneNameInput {}",
                bytes
                    .iter()
                    .map(|c| format!("{}", *c))
                    .collect::<Vec<String>>()
                    .join(" ")
            ),
            Self::UndoRenamePane => write!(f, "UndoRenamePane"),
            Self::NewTab(_) => write!(f, "NewTab"),
            Self::NoOp => write!(f, "NoOp"),
            Self::GoToNextTab => write!(f, "GoToNextTab"),
            Self::GoToPreviousTab => write!(f, "GoToPreviousTab"),
            Self::CloseTab => write!(f, "CloseTab"),
            Self::GoToTab(index) => write!(f, "GoToTab {}", index),
            Self::ToggleTab => write!(f, "ToggleTab"),
            // Self::TabNameInput(bytes) => write!(f, "TabNameInput {}", format!("{}", bytes.iter().map(|c| format!("{}", *c)).collect::<Vec<String>>().join(" "))),
            Self::TabNameInput(bytes) => write!(
                f,
                "TabNameInput {}",
                bytes
                    .iter()
                    .map(|c| format!("{}", *c))
                    .collect::<Vec<String>>()
                    .join(" ")
            ),
            Self::UndoRenameTab => write!(f, "UndoRenameTab"),
            Self::Run(run_command_action) => {
                let mut run_block_serialized = format!("Run {:?}", run_command_action.command);
                for arg in &run_command_action.args {
                    run_block_serialized.push_str(&format!(" \"{}\"", arg));
                }
                match (&run_command_action.cwd, run_command_action.direction) {
                    (Some(cwd), Some(direction)) => {
                        run_block_serialized.push_str(&format!(
                            "{{ cwd {:?}; direction \"{}\"; }}",
                            cwd, direction
                        ));
                    },
                    (None, Some(direction)) => {
                        run_block_serialized
                            .push_str(&format!("{{ direction \"{}\"; }}", direction));
                    },
                    (Some(cwd), None) => {
                        run_block_serialized.push_str(&format!("{{ cwd {:?}; }}", cwd));
                    },
                    (None, None) => {},
                }
                write!(f, "{}", run_block_serialized)
            },
            Self::Detach => write!(f, "Detach"),
            Self::Copy => write!(f, "Copy"),
            Self::Confirm => write!(f, "Confirm"),
            Self::Deny => write!(f, "Deny"),
            Self::SearchInput(bytes) => write!(
                f,
                "SearchInput {}",
                bytes
                    .iter()
                    .map(|c| format!("{}", *c))
                    .collect::<Vec<String>>()
                    .join(" ")
            ),
            Self::Search(direction) => write!(f, "Search \"{}\"", direction),
            Self::SearchToggleOption(option) => write!(f, "SearchToggleOption \"{}\"", option),
            _ => Err(std::fmt::Error),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
enum OldSearchDirection {
    Down,
    Up,
}

impl std::fmt::Display for OldSearchDirection {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        match self {
            Self::Down => write!(f, "Down"),
            Self::Up => write!(f, "Up"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
enum OldSearchOption {
    CaseSensitivity,
    WholeWord,
    Wrap,
}

impl std::fmt::Display for OldSearchOption {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        match self {
            Self::CaseSensitivity => write!(f, "CaseSensitivity"),
            Self::WholeWord => write!(f, "WholeWord"),
            Self::Wrap => write!(f, "Wrap"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
enum OldResizeDirection {
    Left,
    Right,
    Up,
    Down,
    Increase,
    Decrease,
}

impl std::fmt::Display for OldResizeDirection {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        match self {
            Self::Left => write!(f, "Left"),
            Self::Right => write!(f, "Right"),
            Self::Up => write!(f, "Up"),
            Self::Down => write!(f, "Down"),
            Self::Increase => write!(f, "Increase"),
            Self::Decrease => write!(f, "Decrease"),
        }
    }
}

#[derive(Debug, Hash, Copy, Clone, PartialEq, Eq, PartialOrd, Deserialize, Serialize)]
struct OldPosition {
    pub line: OldLine,
    pub column: OldColumn,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd)]
struct OldLine(pub isize);
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd)]
struct OldColumn(pub usize);

#[derive(Clone, Debug, Deserialize, Default, Serialize, PartialEq, Eq)]
struct OldRunCommandAction {
    #[serde(rename = "cmd")]
    pub command: PathBuf,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub cwd: Option<PathBuf>,
    #[serde(default)]
    pub direction: Option<OldDirection>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
struct OldTabLayout {
    #[serde(default)]
    pub direction: OldDirection,
    pub pane_name: Option<String>,
    #[serde(default)]
    pub borderless: bool,
    #[serde(default)]
    pub parts: Vec<OldTabLayout>,
    pub split_size: Option<OldSplitSize>,
    #[serde(default)]
    pub name: String,
    pub focus: Option<bool>,
    pub run: Option<OldRunFromYaml>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
enum OldSplitSize {
    #[serde(alias = "percent")]
    Percent(u64), // 1 to 100
    #[serde(alias = "fixed")]
    Fixed(usize), // An absolute number of columns or rows
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
enum OldRunFromYaml {
    #[serde(rename = "plugin")]
    Plugin(OldRunPluginFromYaml),
    #[serde(rename = "command")]
    Command(OldRunCommand),
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
struct OldRunPluginFromYaml {
    #[serde(default)]
    pub _allow_exec_host_cmd: bool,
    pub location: Url,
}

#[derive(Clone, Debug, Deserialize, Default, Serialize, PartialEq, Eq)]
pub struct OldRunCommand {
    #[serde(alias = "cmd")]
    pub command: PathBuf,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub cwd: Option<PathBuf>,
}

// The unit test location.
#[path = "./unit/convert_config_tests.rs"]
#[cfg(test)]
mod convert_config_test;
