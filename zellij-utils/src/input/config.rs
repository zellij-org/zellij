//! Deserializes configuration options.
use std::fmt;
use std::fs::File;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use thiserror::Error;
use crate::data::{self, Palette, PaletteColor, PluginTag, CharOrArrow};
use super::layout::RunPluginLocation;

use crate::input::{InputMode, Key};
use super::actions::{Action, Direction};

use std::collections::{HashMap, HashSet};

use kdl::{KdlDocument, KdlValue, KdlNode};

use serde::{Deserialize, Serialize};
use std::convert::{TryFrom, TryInto};

use super::keybinds::{Keybinds, KeybindsFromYaml};
use super::options::{Options, OnForceClose, Clipboard};
use super::plugins::{PluginsConfig, PluginsConfigError, PluginsConfigFromYaml, PluginConfig, PluginType};
use super::theme::{ThemesFromYaml, UiConfig, Theme, FrameConfig};
use crate::cli::{CliArgs, Command};
use crate::envs::EnvironmentVariables;
use crate::setup;
use crate::{entry_count, kdl_entries_as_i64, kdl_first_entry_as_string, kdl_first_entry_as_i64};

// const DEFAULT_CONFIG_FILE_NAME: &str = "config.yaml";
const DEFAULT_CONFIG_FILE_NAME: &str = "config.kdl";

type ConfigResult = Result<Config, ConfigError>;

/// Intermediate deserialization config struct
#[derive(Clone, Default, Debug, Deserialize, Serialize, PartialEq)]
pub struct ConfigFromYaml {
    #[serde(flatten)]
    pub options: Option<Options>,
    pub keybinds: Option<KeybindsFromYaml>,
    pub themes: Option<ThemesFromYaml>,
    #[serde(flatten)]
    pub env: Option<EnvironmentVariables>,
    #[serde(default)]
    pub plugins: PluginsConfigFromYaml,
    pub ui: Option<UiConfig>,
}

/// Main configuration.
#[derive(Debug, Clone, PartialEq, Deserialize, knuffel::Decode)]
pub struct Config {
    pub keybinds: HashMap<InputMode, HashMap<Key, Vec<Action>>>, // TODO: make this a type
    pub options: Options,
    pub themes: Option<HashMap<String, Theme>>, // TODO: typify?
    pub plugins: PluginsConfig,
    // pub ui: Option<UiConfigFromYaml>,
    pub ui: Option<UiConfig>,
    pub env: EnvironmentVariables,
}
// #[derive(Debug, Clone, PartialEq, Deserialize)]
// pub struct Config {
//     pub keybinds: Keybinds,
//     pub options: Options,
//     pub themes: Option<ThemesFromYaml>,
//     pub plugins: PluginsConfig,
//     pub ui: Option<UiConfigFromYaml>,
//     pub env: EnvironmentVariablesFromYaml,
// }

#[derive(Error, Debug)]
pub enum ConfigError {
    // Deserialization error
    #[error("Deserialization error: {0}")]
    KdlDeserializationError(#[from] kdl::KdlError),
    #[error("KdlDeserialization error: {0}")]
    KdlParsingError(String),
    // Deserialization error
    #[error("Deserialization error: {0}")]
    Serde(#[from] serde_yaml::Error),
    // Io error
    #[error("IoError: {0}")]
    Io(#[from] io::Error),
    // Io error with path context
    #[error("IoError: {0}, File: {1}")]
    IoPath(io::Error, PathBuf),
    // Internal Deserialization Error
    #[error("FromUtf8Error: {0}")]
    FromUtf8(#[from] std::string::FromUtf8Error),
    // Naming a part in a tab is unsupported
    #[error("There was an error in the layout file, {0}")]
    LayoutNameInTab(#[from] LayoutNameInTabError),
    // Plugins have a semantic error, usually trying to parse two of the same tag
    #[error("PluginsError: {0}")]
    PluginsError(#[from] PluginsConfigError),
}

impl Default for Config {
    fn default() -> Self {
        // let keybinds = Keybinds::default();
        let keybinds = HashMap::new();
        let options = Options::default();
        let themes = None;
        let env = EnvironmentVariables::default();
        let plugins = PluginsConfig::default();
        let ui = None;

        Config {
            keybinds,
            options,
            themes,
            plugins,
            env,
            ui,
        }
    }
}

impl TryFrom<&CliArgs> for Config {
    type Error = ConfigError;

    fn try_from(opts: &CliArgs) -> ConfigResult {
        if let Some(ref path) = opts.config {
            let default_config = Config::from_default_assets()?;
            return Config::from_path(path, Some(default_config));
            // return Config::new(path);
        }

        if let Some(Command::Setup(ref setup)) = opts.command {
            if setup.clean {
                return Config::from_default_assets();
            }
        }

        let config_dir = opts
            .config_dir
            .clone()
            .or_else(setup::find_default_config_dir);

        if let Some(ref config) = config_dir {
            let path = config.join(DEFAULT_CONFIG_FILE_NAME);
            if path.exists() {
                let default_config = Config::from_default_assets()?;
                Config::from_path(&path, Some(default_config))
                // Config::new(&path)
            } else {
                Config::from_default_assets()
            }
        } else {
            Config::from_default_assets()
        }
    }
}

impl Config {
    pub fn theme_config(&self, opts: &Options) -> Option<Palette> {
        match &opts.theme {
            // Some(theme) => from_yaml.from_default_theme(theme.to_owned()),
            Some(theme_name) => self.themes.as_ref()
                .and_then(|themes| themes.get(theme_name))
                .map(|theme| theme.palette),
            // None => from_yaml.from_default_theme("default".into()),
            None => self.themes.as_ref()
                .and_then(|themes| themes.get("default"))
                .map(|theme| theme.palette)
        }
    }
    /// Uses defaults, but lets config override them.
//     pub fn from_yaml(yaml_config: &str) -> ConfigResult {
//         let maybe_config_from_yaml: Option<ConfigFromYaml> = match serde_yaml::from_str(yaml_config)
//         {
//             Err(e) => {
//                 // needs direct check, as `[ErrorImpl]` is private
//                 // https://github.com/dtolnay/serde-yaml/issues/121
//                 if yaml_config.is_empty() {
//                     return Ok(Config::default());
//                 }
//                 return Err(ConfigError::Serde(e));
//             },
//             Ok(config) => config,
//         };
//
//         match maybe_config_from_yaml {
//             None => Ok(Config::default()),
//             Some(config) => config.try_into(),
//         }
//     }
    /// Uses defaults, but lets config override them.
    pub fn parse_keybindings(kdl_keybinds: &KdlNode, keybindings_to_override: Option<HashMap<InputMode, HashMap<Key, Vec<Action>>>>) -> Result<HashMap<InputMode, HashMap<Key, Vec<Action>>>, String> {
        let clear_defaults = kdl_keybinds.get("clear-defaults").and_then(|c| c.value().as_bool()).unwrap_or(false) == true;
        let mut keybinds_from_config: HashMap<InputMode, HashMap<Key, Vec<Action>>> = if clear_defaults {HashMap::new()} else {keybindings_to_override.unwrap_or_else(|| HashMap::new())};
        for mode in kdl_keybinds.children().unwrap().nodes() {
            let mode_name = mode.name().value();
            if mode_name == "unbind" {
                continue;
            }
            let input_mode = InputMode::try_from(mode_name).unwrap();
            let input_mode_keybinds = keybinds_from_config.entry(input_mode).or_insert_with(HashMap::new);
            let clear_defaults_for_mode = mode.get("clear-defaults").is_some();
            if clear_defaults_for_mode {
                input_mode_keybinds.clear();
            }
            for key_block in mode.children().unwrap().nodes() {
                let key_block_name = key_block.name().value();
                if key_block_name == "bind" {
                    let keys: Vec<Key> = key_block.entries().iter().map(|key_shortcut| {
                        let key_shortcut = key_shortcut.value();
                        Key::try_from(key_shortcut).unwrap()
                    }).collect();
                    let actions: Vec<Action> = key_block.children().unwrap().nodes().iter().map(|action| {
                        let action_name = action.name().value();
                        let action_arguments: Vec<&KdlValue> = action.entries().iter().map(|arg| arg.value()).collect();
                        let action_children: Vec<&KdlDocument> = action.children().iter().copied().collect();
                        Action::try_from((action_name, action_arguments, action_children)).unwrap()
                    }).collect();
                    for key in keys {
                        input_mode_keybinds.insert(key, actions.clone());
                    }
                }
            }
            for key_block in mode.children().unwrap().nodes() {
                // we loop twice so that the unbinds always happen after the binds
                let key_block_name = key_block.name().value();
                if key_block_name == "unbind" {
                    let keys: Vec<Key> = key_block.entries().iter().map(|key_shortcut| {
                        let key_shortcut = key_shortcut.value();
                        Key::try_from(key_shortcut).unwrap()
                    }).collect();
                    for key in keys {
                        input_mode_keybinds.remove(&key);
                    }
                }
            }
            // keybinds_from_config.insert(input_mode, input_mode_keybinds);
        }
        if let Some(global_unbind) = kdl_keybinds.children().and_then(|c| c.get("unbind")) {
            let keys: Vec<Key> = global_unbind.entries().iter().map(|key_shortcut| {
                let key_shortcut = key_shortcut.value();
                Key::try_from(key_shortcut).unwrap()
            }).collect();
            for mode in keybinds_from_config.values_mut() {
                for key in &keys {
                    mode.remove(&key);
                }
            }
        };
        Ok(keybinds_from_config)
    }
    fn parse_options(kdl_config: &KdlDocument) -> Options {
        // parse options
        let on_force_close = kdl_config.get("on_force_close")
            .and_then(|on_force_close| on_force_close.entries().iter().next())
            .and_then(|on_force_close| on_force_close.value().as_string())
            .and_then(|on_force_close| OnForceClose::from_str(on_force_close).ok());
        let simplified_ui = kdl_config.get("simplified_ui")
            .and_then(|simplified_ui| simplified_ui.entries().iter().next())
            .and_then(|simplified_ui| simplified_ui.value().as_bool());
        let default_shell = kdl_config.get("default_shell")
            .and_then(|default_shell| default_shell.entries().iter().next())
            .and_then(|default_shell| default_shell.value().as_string())
            .map(|default_shell| PathBuf::from(default_shell));
        let pane_frames = kdl_config.get("pane_frames")
            .and_then(|pane_frames| pane_frames.entries().iter().next())
            .and_then(|pane_frames| pane_frames.value().as_bool());
        let theme = kdl_config.get("theme")
            .and_then(|theme| theme.entries().iter().next())
            .and_then(|theme| theme.value().as_string())
            .map(|theme| theme.to_string());
        let default_mode = kdl_config.get("default_mode")
            .and_then(|default_mode| default_mode.entries().iter().next())
            .and_then(|default_mode| default_mode.value().as_string())
            .and_then(|default_mode| InputMode::try_from(default_mode).ok());
        let mouse_mode = kdl_config.get("mouse_mode")
            .and_then(|mouse_mode| mouse_mode.entries().iter().next())
            .and_then(|mouse_mode| mouse_mode.value().as_bool());
        let scroll_buffer_size = kdl_config.get("scroll_buffer_size")
            .and_then(|scroll_buffer_size| scroll_buffer_size.entries().iter().next())
            .and_then(|scroll_buffer_size| scroll_buffer_size.value().as_i64())
            .map(|scroll_buffer_size| scroll_buffer_size as usize);
        let copy_command = kdl_config.get("copy_command")
            .and_then(|copy_command| copy_command.entries().iter().next())
            .and_then(|copy_command| copy_command.value().as_string())
            .map(|copy_command| copy_command.to_string());
        let copy_clipboard = kdl_config.get("copy_clipboard")
            .and_then(|copy_clipboard| copy_clipboard.entries().iter().next())
            .and_then(|copy_clipboard| copy_clipboard.value().as_string())
            .and_then(|copy_clipboard| Clipboard::from_str(copy_clipboard).ok());
        let copy_on_select = kdl_config.get("copy_on_select")
            .and_then(|copy_on_select| copy_on_select.entries().iter().next())
            .and_then(|copy_on_select| copy_on_select.value().as_bool());
        let scrollback_editor = kdl_config.get("scrollback_editor")
            .and_then(|scrollback_editor| scrollback_editor.entries().iter().next())
            .and_then(|scrollback_editor| scrollback_editor.value().as_string())
            .map(|scrollback_editor| PathBuf::from(scrollback_editor));
        let mirror_session = kdl_config.get("mirror_session")
            .and_then(|mirror_session| mirror_session.entries().iter().next())
            .and_then(|mirror_session| mirror_session.value().as_bool());
        Options {
            simplified_ui,
            theme,
            default_mode,
            default_shell,
            default_layout: Some(PathBuf::from("default")), // TODO
            layout_dir: None, // TODO
            mouse_mode,
            pane_frames,
            mirror_session,
            on_force_close,
            scroll_buffer_size,
            copy_command,
            copy_clipboard,
            copy_on_select,
            scrollback_editor,
        }
    }
    pub fn parse_themes(themes_from_kdl: &KdlNode) -> Result<HashMap<String, Theme>, String> {
        let mut themes: HashMap<String, Theme> = HashMap::new();
        for theme_config in themes_from_kdl.children().unwrap().nodes() { // TODO: no unwraps here or anywhere
            let theme_name = theme_config.name().value();
            let theme_colors = theme_config.children().unwrap();
            let theme = Theme {
                palette: Palette {
                    fg: PaletteColor::try_from(("fg", theme_colors))?,
                    bg: PaletteColor::try_from(("bg", theme_colors))?,
                    red: PaletteColor::try_from(("red", theme_colors))?,
                    green: PaletteColor::try_from(("green", theme_colors))?,
                    yellow: PaletteColor::try_from(("yellow", theme_colors))?,
                    blue: PaletteColor::try_from(("blue", theme_colors))?,
                    magenta: PaletteColor::try_from(("magenta", theme_colors))?,
                    orange: PaletteColor::try_from(("orange", theme_colors))?,
                    cyan: PaletteColor::try_from(("cyan", theme_colors))?,
                    black: PaletteColor::try_from(("black", theme_colors))?,
                    white: PaletteColor::try_from(("white", theme_colors))?,
                    ..Default::default()
                }
            };
            themes.insert(theme_name.into(), theme);
        }
        Ok(themes)
    }
    pub fn parse_plugins(kdl_plugin_config: &KdlNode) -> Result<HashMap<PluginTag, PluginConfig>, String> {
        let mut plugins: HashMap<PluginTag, PluginConfig> = HashMap::new();
        for plugin_config in kdl_plugin_config.children().unwrap().nodes() { // TODO: no unwraps here or anywhere
            let plugin_name = String::from(plugin_config.name().value());
            let plugin_tag = PluginTag::new(&plugin_name);
            let path = plugin_config.children().unwrap()
                .get("path")
                .ok_or("Plugin path not found")?
                .entries()
                .iter()
                .next()
                .ok_or("Plugin path not found")?
                .value()
                .as_string()
                .ok_or("Invalid plugin path")?;
            let path = PathBuf::from(path);
            // let allow_exec_host_cmd = plugin_config.children().unwrap().get("_allow_exec_host_cmd").and_then(|a| a.value().as_bool()).unwrap_or(false);
            let allow_exec_host_cmd = plugin_config.children().unwrap()
                .get("_allow_exec_host_cmd")
                .and_then(|a| a.entries().iter().next())
                .and_then(|a| a.value().as_bool())
                .unwrap_or(false);
            let plugin_config = PluginConfig {
                path,
                run: PluginType::Pane(None),
                location: RunPluginLocation::Zellij(plugin_tag.clone()),
                _allow_exec_host_cmd: allow_exec_host_cmd,
            };
            plugins.insert(plugin_tag, plugin_config);
        }
        Ok(plugins)
    }
    pub fn parse_ui_config(kdl_ui_config: &KdlNode) -> Result<UiConfig, String> {
        let mut ui_config = UiConfig::default();
        if let Some(pane_frames) = kdl_ui_config.children().unwrap().get("pane_frames") {
            let rounded_corners = pane_frames.children().unwrap().get("rounded_corners").unwrap().entries().iter().next().unwrap().value().as_bool().unwrap_or(false);
            let frame_config = FrameConfig { rounded_corners };
            ui_config.pane_frames = frame_config;
        }
        Ok(ui_config)
    }
    pub fn parse_env_variables_config(kdl_env_variables: &KdlNode) -> Result<HashMap<String, String>, String> {
        let mut env: HashMap<String, String> = HashMap::new();
        for env_var in kdl_env_variables.children().unwrap().nodes() { // TODO: no unwraps here or anywhere
            let env_var_name = String::from(env_var.name().value());
            let env_var_value = env_var
                .entries()
                .iter()
                .next()
                .ok_or("environment variable must have a value")?;
            let env_var_str_value = env_var_value
                .value()
                .as_string()
                .map(|s| s.to_string());
            let env_var_int_value = env_var_value
                .value()
                .as_i64()
                .map(|s| format!("{}", s.to_string()));
            let env_var_value = env_var_str_value
                .or(env_var_int_value)
                .ok_or(format!("Failed to parse env var value: {:?}", env_var_value))?;
            env.insert(env_var_name, env_var_value);
        }
        Ok(env)
    }
    pub fn from_kdl(kdl_config: &str, base_config: Option<Config>) -> ConfigResult {
        // TODO: CONTINUE HERE
        // - adapt the existing tests to work with the new way
        // - then write new tests
        // - then refactor
        // - then move on to layouts and everything else (what?)
        let mut config = base_config.unwrap_or_else(|| Config::default());
        let kdl_config: KdlDocument = kdl_config.parse()?;

        if let Some(keybinds) = kdl_config.get("keybinds") {
            let keybinds_from_config = Config::parse_keybindings(&keybinds, Some(config.keybinds)).map_err(|e| ConfigError::KdlParsingError(format!("Failed to parse keybindings: {:?}", e)))?;
            config.keybinds = keybinds_from_config;
        }

        let options = Config::parse_options(&kdl_config);
        config.options = options;

        if let Some(kdl_themes) = kdl_config.get("themes") {
            let themes = Config::parse_themes(&kdl_themes).map_err(|e| ConfigError::KdlParsingError(format!("Failed to parse themes: {:?}", e)))?;
            config.themes = Some(themes);
        }

        if let Some(kdl_plugin_config) = kdl_config.get("plugins") {
            let plugins = Config::parse_plugins(&kdl_plugin_config).map_err(|e| ConfigError::KdlParsingError(format!("Failed to parse plugins: {:?}", e)))?;
            config.plugins = PluginsConfig::from_data(plugins);
        }

        if let Some(kdl_ui_config) = kdl_config.get("ui") {
            let ui_config = Config::parse_ui_config(&kdl_ui_config).map_err(|e| ConfigError::KdlParsingError(format!("Failed to parse ui config: {:?}", e)))?;
            config.ui = Some(ui_config);
        }

        if let Some(env_config) = kdl_config.get("env") {
            let env = Config::parse_env_variables_config(&env_config).map_err(|e| ConfigError::KdlParsingError(format!("Failed to parse env variable config: {:?}", e)))?;
            config.env = EnvironmentVariables::from_data(env);
        }

        Ok(config)
    }

    /// Deserializes from given path.
//     pub fn new(path: &Path) -> ConfigResult {
//         match File::open(path) {
//             Ok(mut file) => {
//                 let mut kdl_config = String::new();
//                 file.read_to_string(&mut kdl_config)
//                     .map_err(|e| ConfigError::IoPath(e, path.to_path_buf()))?;
//                 Ok(Config::from_kdl(&kdl_config)?)
//
// //                 let mut yaml_config = String::new();
// //                 file.read_to_string(&mut yaml_config)
// //                     .map_err(|e| ConfigError::IoPath(e, path.to_path_buf()))?;
// //                 Ok(Config::from_yaml(&yaml_config)?)
//             },
//             Err(e) => Err(ConfigError::IoPath(e, path.into())),
//         }
//     }

    /// Gets default configuration from assets
    // TODO Deserialize the Config from bytes &[u8],
    // once serde-yaml supports zero-copy
    pub fn from_default_assets() -> ConfigResult {
        let cfg = String::from_utf8(setup::DEFAULT_CONFIG.to_vec())?;
        // Self::from_yaml(&cfg)
        Self::from_kdl(&cfg, None)
    }
    pub fn from_path(path: &PathBuf, default_config: Option<Config>) -> ConfigResult {
        match File::open(path) {
            Ok(mut file) => {
                let mut kdl_config = String::new();
                file.read_to_string(&mut kdl_config)
                    .map_err(|e| ConfigError::IoPath(e, path.to_path_buf()))?;
                Config::from_kdl(&kdl_config, default_config)

//                 let mut yaml_config = String::new();
//                 file.read_to_string(&mut yaml_config)
//                     .map_err(|e| ConfigError::IoPath(e, path.to_path_buf()))?;
//                 Ok(Config::from_yaml(&yaml_config)?)
            },
            Err(e) => Err(ConfigError::IoPath(e, path.into())),
        }
    }

    /// Merges two Config structs into one Config struct
    /// `other` overrides `self`.
    pub fn merge(&self, other: Self) -> Self {
        Self {
            // TODO: merge keybinds in a way that preserves "unbind" attribute
            keybinds: self.keybinds.clone(),
            options: self.options.merge(other.options),
            themes: self.themes.clone(), // TODO
            env: self.env.merge(other.env),
            plugins: self.plugins.merge(other.plugins),
            ui: self.ui, // TODO
        }
    }
}

// impl TryFrom<ConfigFromYaml> for Config {
//     type Error = ConfigError;
//
//     fn try_from(config_from_yaml: ConfigFromYaml) -> ConfigResult {
//         // let keybinds = Keybinds::get_default_keybinds_with_config(config_from_yaml.keybinds);
//         let keybinds = HashMap::new();
//         let options = Options::from_yaml(config_from_yaml.options);
//         let themes = config_from_yaml.themes;
//         let env = config_from_yaml.env.unwrap_or_default();
//         let plugins = PluginsConfig::get_plugins_with_default(config_from_yaml.plugins.try_into()?);
//         let ui = config_from_yaml.ui;
//         Ok(Self {
//             keybinds,
//             options,
//             plugins,
//             themes,
//             env,
//             ui,
//         })
//     }
// }

// TODO: Split errors up into separate modules
#[derive(Debug, Clone)]
pub struct LayoutNameInTabError;

impl fmt::Display for LayoutNameInTabError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "LayoutNameInTabError:
The `parts` inside the `tabs` can't be named. For example:
---
tabs:
  - direction: Vertical
    name: main
    parts:
      - direction: Vertical
        name: section # <== The part section can't be named.
      - direction: Vertical
  - direction: Vertical
    name: test
"
        )
    }
}

impl std::error::Error for LayoutNameInTabError {
    fn description(&self) -> &str {
        "The `parts` inside the `tabs` can't be named."
    }
}

// The unit test location.
#[cfg(test)]
mod config_test {
    use std::io::Write;

    use tempfile::tempdir;

    use super::*;

    #[test]
    fn try_from_cli_args_with_config() {
        // makes sure loading a config file with --config tries to load the config
        let arbitrary_config = PathBuf::from("nonexistent.yaml");
        let opts = CliArgs {
            config: Some(arbitrary_config),
            ..Default::default()
        };
        println!("OPTS= {:?}", opts);
        let result = Config::try_from(&opts);
        assert!(result.is_err());
    }

    #[test]
    fn try_from_cli_args_with_option_clean() {
        // makes sure --clean works... TODO: how can this actually fail now?
        use crate::setup::Setup;
        let opts = CliArgs {
            command: Some(Command::Setup(Setup {
                clean: true,
                ..Setup::default()
            })),
            ..Default::default()
        };
        let result = Config::try_from(&opts);
        assert!(result.is_ok());
    }

    #[test]
    fn try_from_cli_args_with_config_dir() {
        let mut opts = CliArgs::default();
        let tmp = tempdir().unwrap();
        File::create(tmp.path().join(DEFAULT_CONFIG_FILE_NAME))
            .unwrap()
            .write_all(b"keybinds: invalid\n")
            .unwrap();
        opts.config_dir = Some(tmp.path().to_path_buf());
        let result = Config::try_from(&opts);
        assert!(result.is_err());
    }

    #[test]
    fn try_from_cli_args_with_config_dir_without_config() {
        let mut opts = CliArgs::default();
        let tmp = tempdir().unwrap();
        opts.config_dir = Some(tmp.path().to_path_buf());
        let result = Config::try_from(&opts);
        assert_eq!(result.unwrap(), Config::from_default_assets().unwrap());
    }

    #[test]
    fn try_from_cli_args_default() {
        let opts = CliArgs::default();
        let result = Config::try_from(&opts);
        assert_eq!(result.unwrap(), Config::from_default_assets().unwrap());
    }

    // TODO: CONTINUE HERE (04/08) - write these test cases, then refactor

    #[test]
    fn can_define_keybindings_in_configfile() {
        let config_contents = r#"
            keybinds {
                normal {
                    bind "Ctrl g" { SwitchToMode "Locked"; }
                }
            }
        "#;
        let config = Config::from_kdl(config_contents, None).unwrap();
        let ctrl_g_normal_mode_action = config.keybinds
            .get(&InputMode::Normal)
            .and_then(|normal_mode_keybindings| normal_mode_keybindings.get(&Key::Ctrl('g')));
        assert_eq!(ctrl_g_normal_mode_action, Some(&vec![Action::SwitchToMode(InputMode::Locked)]), "Keybinding successfully defined in config");
    }

    #[test]
    fn can_define_multiple_keybinds_for_same_action() {
        let config_contents = r#"
            keybinds {
                normal {
                    bind "Alt h" "Alt Left" { MoveFocusOrTab "Left"; }
                }
            }
        "#;
        let config = Config::from_kdl(config_contents, None).unwrap();
        let alt_h_normal_mode_action = config.keybinds
            .get(&InputMode::Normal)
            .and_then(|normal_mode_keybindings|
                 normal_mode_keybindings.get(&Key::Alt(CharOrArrow::Direction(data::Direction::Left)))
             );
        let alt_left_normal_mode_action = config.keybinds
            .get(&InputMode::Normal)
            .and_then(|normal_mode_keybindings|
                normal_mode_keybindings.get(&Key::Alt(CharOrArrow::Char('h')))
            );
        assert_eq!(alt_h_normal_mode_action, Some(&vec![Action::MoveFocusOrTab(Direction::Left)]), "First keybinding successfully defined in config");
        assert_eq!(alt_left_normal_mode_action, Some(&vec![Action::MoveFocusOrTab(Direction::Left)]), "Second keybinding successfully defined in config");
    }

    #[test]
    fn can_define_series_of_actions_for_same_keybinding() {
        let config_contents = r#"
            keybinds {
                pane {
                    bind "z" { TogglePaneFrames; SwitchToMode "Normal"; }
                }
            }
        "#;
        let config = Config::from_kdl(config_contents, None).unwrap();
        let z_in_pane_mode = config.keybinds
            .get(&InputMode::Pane)
            .and_then(|pane_mode_keybindings|
                 pane_mode_keybindings.get(&Key::Char('z'))
             );
        assert_eq!(z_in_pane_mode, Some(&vec![Action::TogglePaneFrames, Action::SwitchToMode(InputMode::Normal)]), "Action series successfully defined");
    }

    #[test]
    fn keybindings_bind_order_is_preserved() {
        let config_contents = r#"
            keybinds {
                pane {
                    bind "z" { TogglePaneFrames; SwitchToMode "Normal"; }
                    bind "z" { SwitchToMode "Resize"; }
                }
            }
        "#;
        let config = Config::from_kdl(config_contents, None).unwrap();
        let z_in_pane_mode = config.keybinds
            .get(&InputMode::Pane)
            .and_then(|pane_mode_keybindings|
                 pane_mode_keybindings.get(&Key::Char('z'))
             );
        assert_eq!(z_in_pane_mode, Some(&vec![Action::SwitchToMode(InputMode::Resize)]), "Second keybinding was applied");
    }

    #[test]
    fn uppercase_and_lowercase_keybindings_are_distinct() {
        let config_contents = r#"
            keybinds {
                pane {
                    bind "z" { TogglePaneFrames; SwitchToMode "Normal"; }
                    bind "Z" { SwitchToMode "Resize"; }
                }
            }
        "#;
        let config = Config::from_kdl(config_contents, None).unwrap();
        let z_in_pane_mode = config.keybinds
            .get(&InputMode::Pane)
            .and_then(|pane_mode_keybindings|
                 pane_mode_keybindings.get(&Key::Char('z'))
             );
        let uppercase_z_in_pane_mode = config.keybinds
            .get(&InputMode::Pane)
            .and_then(|pane_mode_keybindings|
                 pane_mode_keybindings.get(&Key::Char('Z'))
             );
        assert_eq!(z_in_pane_mode, Some(&vec![Action::TogglePaneFrames, Action::SwitchToMode(InputMode::Normal)]), "Lowercase z successfully bound");
        assert_eq!(uppercase_z_in_pane_mode, Some(&vec![Action::SwitchToMode(InputMode::Resize)]), "Uppercase z successfully bound");
    }

    #[test]
    fn can_override_keybindings() {
        let default_config_contents = r#"
            keybinds {
                pane {
                    bind "z" { TogglePaneFrames; SwitchToMode "Normal"; }
                }
            }
        "#;
        let config_contents = r#"
            keybinds {
                pane {
                    bind "z" { SwitchToMode "Resize"; }
                }
            }
        "#;
        let default_config = Config::from_kdl(default_config_contents, None).unwrap();
        let config = Config::from_kdl(config_contents, Some(default_config)).unwrap();
        let z_in_pane_mode = config.keybinds
            .get(&InputMode::Pane)
            .and_then(|pane_mode_keybindings|
                 pane_mode_keybindings.get(&Key::Char('z'))
             );
        assert_eq!(z_in_pane_mode, Some(&vec![Action::SwitchToMode(InputMode::Resize)]), "Keybinding from config overrode keybinding from default config");
    }

    #[test]
    fn can_add_to_default_keybindings() {
        // this test just makes sure keybindings defined in a custom config are added to different
        // keybindings defined in the default config
        let default_config_contents = r#"
            keybinds {
                pane {
                    bind "z" { TogglePaneFrames; SwitchToMode "Normal"; }
                }
            }
        "#;
        let config_contents = r#"
            keybinds {
                pane {
                    bind "r" { SwitchToMode "Resize"; }
                }
            }
        "#;
        let default_config = Config::from_kdl(default_config_contents, None).unwrap();
        let config = Config::from_kdl(config_contents, Some(default_config)).unwrap();
        let z_in_pane_mode = config.keybinds
            .get(&InputMode::Pane)
            .and_then(|pane_mode_keybindings|
                 pane_mode_keybindings.get(&Key::Char('z'))
             );
        let r_in_pane_mode = config.keybinds
            .get(&InputMode::Pane)
            .and_then(|pane_mode_keybindings|
                 pane_mode_keybindings.get(&Key::Char('r'))
             );
        assert_eq!(z_in_pane_mode, Some(&vec![Action::TogglePaneFrames, Action::SwitchToMode(InputMode::Normal)]), "Keybinding from default config bound");
        assert_eq!(r_in_pane_mode, Some(&vec![Action::SwitchToMode(InputMode::Resize)]), "Keybinding from custom config bound as well");
    }

    #[test]
    fn can_clear_default_keybindings() {
        let default_config_contents = r#"
            keybinds {
                normal {
                    bind "Ctrl g" { SwitchToMode "Locked"; }
                }
                pane {
                    bind "z" { TogglePaneFrames; SwitchToMode "Normal"; }
                }
            }
        "#;
        let config_contents = r#"
            keybinds clear-defaults=true {
                normal {
                    bind "Ctrl r" { SwitchToMode "Locked"; }
                }
                pane {
                    bind "r" { SwitchToMode "Resize"; }
                }
            }
        "#;
        let default_config = Config::from_kdl(default_config_contents, None).unwrap();
        let config = Config::from_kdl(config_contents, Some(default_config)).unwrap();
        let ctrl_g_normal_mode_action = config.keybinds
            .get(&InputMode::Normal)
            .and_then(|normal_mode_keybindings| normal_mode_keybindings.get(&Key::Ctrl('g')));
        let z_in_pane_mode = config.keybinds
            .get(&InputMode::Pane)
            .and_then(|pane_mode_keybindings|
                 pane_mode_keybindings.get(&Key::Char('z'))
             );
        let ctrl_r_in_normal_mode = config.keybinds
            .get(&InputMode::Normal)
            .and_then(|pane_mode_keybindings|
                 pane_mode_keybindings.get(&Key::Ctrl('r'))
             );
        let r_in_pane_mode = config.keybinds
            .get(&InputMode::Pane)
            .and_then(|pane_mode_keybindings|
                 pane_mode_keybindings.get(&Key::Char('r'))
             );
        assert_eq!(ctrl_g_normal_mode_action, None, "Keybinding from normal mode in default config cleared");
        assert_eq!(z_in_pane_mode, None, "Keybinding from pane mode in default config cleared");
        assert_eq!(r_in_pane_mode, Some(&vec![Action::SwitchToMode(InputMode::Resize)]), "Keybinding from pane mode in custom config still bound");
        assert_eq!(ctrl_r_in_normal_mode, Some(&vec![Action::SwitchToMode(InputMode::Locked)]), "Keybinding from normal mode in custom config still bound");
    }

    #[test]
    fn can_clear_default_keybindings_per_single_mode() {
        let default_config_contents = r#"
            keybinds {
                normal {
                    bind "Ctrl g" { SwitchToMode "Locked"; }
                }
                pane {
                    bind "z" { TogglePaneFrames; SwitchToMode "Normal"; }
                }
            }
        "#;
        let config_contents = r#"
            keybinds {
                pane clear-defaults=true {
                    bind "r" { SwitchToMode "Resize"; }
                }
            }
        "#;
        let default_config = Config::from_kdl(default_config_contents, None).unwrap();
        let config = Config::from_kdl(config_contents, Some(default_config)).unwrap();
        let ctrl_g_normal_mode_action = config.keybinds
            .get(&InputMode::Normal)
            .and_then(|normal_mode_keybindings| normal_mode_keybindings.get(&Key::Ctrl('g')));
        let z_in_pane_mode = config.keybinds
            .get(&InputMode::Pane)
            .and_then(|pane_mode_keybindings|
                 pane_mode_keybindings.get(&Key::Char('z'))
             );
        let r_in_pane_mode = config.keybinds
            .get(&InputMode::Pane)
            .and_then(|pane_mode_keybindings|
                 pane_mode_keybindings.get(&Key::Char('r'))
             );
        assert_eq!(ctrl_g_normal_mode_action, Some(&vec![Action::SwitchToMode(InputMode::Locked)]), "Keybind in different mode from default config not cleared");
        assert_eq!(z_in_pane_mode, None, "Keybinding from pane mode in default config cleared");
        assert_eq!(r_in_pane_mode, Some(&vec![Action::SwitchToMode(InputMode::Resize)]), "Keybinding from pane mode in custom config still bound");
    }

    #[test]
    fn can_unbind_multiple_keys_globally() {
        let default_config_contents = r#"
            keybinds {
                normal {
                    bind "Ctrl g" { SwitchToMode "Locked"; }
                }
                pane {
                    bind "Ctrl g" { SwitchToMode "Locked"; }
                    bind "z" { TogglePaneFrames; SwitchToMode "Normal"; }
                    bind "r" { TogglePaneFrames; }
                }
            }
        "#;
        let config_contents = r#"
            keybinds {
                unbind "Ctrl g" "z"
                pane {
                    bind "t" { SwitchToMode "Tab"; }
                }
            }
        "#;
        let default_config = Config::from_kdl(default_config_contents, None).unwrap();
        let config = Config::from_kdl(config_contents, Some(default_config)).unwrap();
        let ctrl_g_normal_mode_action = config.keybinds
            .get(&InputMode::Normal)
            .and_then(|normal_mode_keybindings| normal_mode_keybindings.get(&Key::Ctrl('g')));
        let ctrl_g_pane_mode_action = config.keybinds
            .get(&InputMode::Pane)
            .and_then(|normal_mode_keybindings| normal_mode_keybindings.get(&Key::Ctrl('g')));
        let r_in_pane_mode = config.keybinds
            .get(&InputMode::Pane)
            .and_then(|pane_mode_keybindings|
                 pane_mode_keybindings.get(&Key::Char('r'))
             );
        let z_in_pane_mode = config.keybinds
            .get(&InputMode::Pane)
            .and_then(|pane_mode_keybindings|
                 pane_mode_keybindings.get(&Key::Char('z'))
             );
        let t_in_pane_mode = config.keybinds
            .get(&InputMode::Pane)
            .and_then(|pane_mode_keybindings|
                 pane_mode_keybindings.get(&Key::Char('t'))
             );
        assert_eq!(ctrl_g_normal_mode_action, None, "First keybind uncleared in one mode");
        assert_eq!(ctrl_g_pane_mode_action, None, "First keybind uncleared in another mode");
        assert_eq!(z_in_pane_mode, None, "Second keybind cleared as well");
        assert_eq!(r_in_pane_mode, Some(&vec![Action::TogglePaneFrames]), "Unrelated keybinding in default config still bound");
        assert_eq!(t_in_pane_mode, Some(&vec![Action::SwitchToMode(InputMode::Tab)]), "Keybinding from custom config still bound");
    }

    #[test]
    fn can_unbind_multiple_keys_per_single_mode() {
        let default_config_contents = r#"
            keybinds {
                normal {
                    bind "Ctrl g" { SwitchToMode "Locked"; }
                }
                pane {
                    bind "Ctrl g" { SwitchToMode "Locked"; }
                    bind "z" { TogglePaneFrames; SwitchToMode "Normal"; }
                    bind "r" { TogglePaneFrames; }
                }
            }
        "#;
        let config_contents = r#"
            keybinds {
                pane {
                    unbind "Ctrl g" "z"
                    bind "t" { SwitchToMode "Tab"; }
                }
            }
        "#;
        let default_config = Config::from_kdl(default_config_contents, None).unwrap();
        let config = Config::from_kdl(config_contents, Some(default_config)).unwrap();
        let ctrl_g_normal_mode_action = config.keybinds
            .get(&InputMode::Normal)
            .and_then(|normal_mode_keybindings| normal_mode_keybindings.get(&Key::Ctrl('g')));
        let ctrl_g_pane_mode_action = config.keybinds
            .get(&InputMode::Pane)
            .and_then(|normal_mode_keybindings| normal_mode_keybindings.get(&Key::Ctrl('g')));
        let r_in_pane_mode = config.keybinds
            .get(&InputMode::Pane)
            .and_then(|pane_mode_keybindings|
                 pane_mode_keybindings.get(&Key::Char('r'))
             );
        let z_in_pane_mode = config.keybinds
            .get(&InputMode::Pane)
            .and_then(|pane_mode_keybindings|
                 pane_mode_keybindings.get(&Key::Char('z'))
             );
        let t_in_pane_mode = config.keybinds
            .get(&InputMode::Pane)
            .and_then(|pane_mode_keybindings|
                 pane_mode_keybindings.get(&Key::Char('t'))
             );
        assert_eq!(ctrl_g_normal_mode_action, Some(&vec![Action::SwitchToMode(InputMode::Locked)]), "Keybind in different mode not cleared");
        assert_eq!(ctrl_g_pane_mode_action, None, "First Keybind cleared in its mode");
        assert_eq!(z_in_pane_mode, None, "Second keybind cleared in its mode as well");
        assert_eq!(r_in_pane_mode, Some(&vec![Action::TogglePaneFrames]), "Unrelated keybinding in default config still bound");
        assert_eq!(t_in_pane_mode, Some(&vec![Action::SwitchToMode(InputMode::Tab)]), "Keybinding from custom config still bound");
    }

    #[test]
    fn keybindings_unbinds_happen_after_binds() {
        let config_contents = r#"
            keybinds {
                pane {
                    unbind "z"
                    bind "z" { SwitchToMode "Resize"; }
                }
            }
        "#;
        let config = Config::from_kdl(config_contents, None).unwrap();
        let z_in_pane_mode = config.keybinds
            .get(&InputMode::Pane)
            .and_then(|pane_mode_keybindings|
                 pane_mode_keybindings.get(&Key::Char('z'))
             );
        assert_eq!(z_in_pane_mode, None, "Key was ultimately unbound");
    }


    #[test]
    fn can_define_options_in_configfile() {
        // TODO: consider writing a macro to generate a test like this for each option
        let config_contents = r#"
            simplified_ui true
        "#;
        let config = Config::from_kdl(config_contents, None).unwrap();
        let simplified_ui_in_config = config.options.simplified_ui;
        assert_eq!(simplified_ui_in_config, Some(true), "Option set in config");
    }

    #[test]
    fn can_define_themes_in_configfile() {
        let config_contents = r#"
            themes {
                dracula {
                    fg 248 248 242
                    bg 40 42 54
                    red 255 85 85
                    green 80 250 123
                    yellow 241 250 140
                    blue 98 114 164
                    magenta 255 121 198
                    orange 255 184 108
                    cyan 139 233 253
                    black 0 0 0
                    white 255 255 255
                }
            }
        "#;
        let config = Config::from_kdl(config_contents, None).unwrap();
        let mut expected_themes = HashMap::new();
        expected_themes.insert("dracula".into(), Theme {
            palette: Palette {
                fg: PaletteColor::Rgb((248, 248, 242)),
                bg: PaletteColor::Rgb((40, 42, 54)),
                red: PaletteColor::Rgb((255, 85, 85)),
                green: PaletteColor::Rgb((80, 250, 123)),
                yellow: PaletteColor::Rgb((241, 250, 140)),
                blue: PaletteColor::Rgb((98, 114, 164)),
                magenta: PaletteColor::Rgb((255, 121, 198)),
                orange: PaletteColor::Rgb((255, 184, 108)),
                cyan: PaletteColor::Rgb((139, 233, 253)),
                black: PaletteColor::Rgb((0, 0, 0)),
                white: PaletteColor::Rgb((255, 255, 255)),
                ..Default::default()
            }
        });
        assert_eq!(config.themes, Some(expected_themes), "Theme defined in config");
    }

    #[test]
    fn can_define_multiple_themes_including_hex_themes_in_configfile() {
        let config_contents = r##"
            themes {
                dracula {
                    fg 248 248 242
                    bg 40 42 54
                    red 255 85 85
                    green 80 250 123
                    yellow 241 250 140
                    blue 98 114 164
                    magenta 255 121 198
                    orange 255 184 108
                    cyan 139 233 253
                    black 0 0 0
                    white 255 255 255
                }
                nord {
                    fg "#D8DEE9"
                    bg "#2E3440"
                    black "#3B4252"
                    red "#BF616A"
                    green "#A3BE8C"
                    yellow "#EBCB8B"
                    blue "#81A1C1"
                    magenta "#B48EAD"
                    cyan "#88C0D0"
                    white "#E5E9F0"
                    orange "#D08770"
                }
            }
        "##;
        let config = Config::from_kdl(config_contents, None).unwrap();
        let mut expected_themes = HashMap::new();
        expected_themes.insert("dracula".into(), Theme {
            palette: Palette {
                fg: PaletteColor::Rgb((248, 248, 242)),
                bg: PaletteColor::Rgb((40, 42, 54)),
                red: PaletteColor::Rgb((255, 85, 85)),
                green: PaletteColor::Rgb((80, 250, 123)),
                yellow: PaletteColor::Rgb((241, 250, 140)),
                blue: PaletteColor::Rgb((98, 114, 164)),
                magenta: PaletteColor::Rgb((255, 121, 198)),
                orange: PaletteColor::Rgb((255, 184, 108)),
                cyan: PaletteColor::Rgb((139, 233, 253)),
                black: PaletteColor::Rgb((0, 0, 0)),
                white: PaletteColor::Rgb((255, 255, 255)),
                ..Default::default()
            }
        });
        expected_themes.insert("nord".into(), Theme {
            palette: Palette {
                fg: PaletteColor::Rgb((216, 222, 233)),
                bg: PaletteColor::Rgb((46, 52, 64)),
                black: PaletteColor::Rgb((59, 66, 82)),
                red: PaletteColor::Rgb((191, 97, 106)),
                green: PaletteColor::Rgb((163, 190, 140)),
                yellow: PaletteColor::Rgb((235, 203, 139)),
                blue: PaletteColor::Rgb((129, 161, 193)),
                magenta: PaletteColor::Rgb((180, 142, 173)),
                cyan: PaletteColor::Rgb((136, 192, 208)),
                white: PaletteColor::Rgb((229, 233, 240)),
                orange: PaletteColor::Rgb((208, 135, 112)),
                ..Default::default()
            }
        });
        assert_eq!(config.themes, Some(expected_themes), "Theme defined in config");
    }

    #[test]
    fn can_define_eight_bit_themes() {
        let config_contents = r#"
            themes {
                eight_bit_theme {
                    fg 248
                    bg 40
                    red 255
                    green 80
                    yellow 241
                    blue 98
                    magenta 255
                    orange 255
                    cyan 139
                    black 1
                    white 255
                }
            }
        "#;
        let config = Config::from_kdl(config_contents, None).unwrap();
        let mut expected_themes = HashMap::new();
        expected_themes.insert("eight_bit_theme".into(), Theme {
            palette: Palette {
                fg: PaletteColor::EightBit(248),
                bg: PaletteColor::EightBit(40),
                red: PaletteColor::EightBit(255),
                green: PaletteColor::EightBit(80),
                yellow: PaletteColor::EightBit(241),
                blue: PaletteColor::EightBit(98),
                magenta: PaletteColor::EightBit(255),
                orange: PaletteColor::EightBit(255),
                cyan: PaletteColor::EightBit(139),
                black: PaletteColor::EightBit(1),
                white: PaletteColor::EightBit(255),
                ..Default::default()
            }
        });
        assert_eq!(config.themes, Some(expected_themes), "Theme defined in config");
    }

    #[test]
    fn can_define_plugin_configuration_in_configfile() {
        let config_contents = r#"
            plugins {
                tab-bar { path "tab-bar"; }
                status-bar { path "status-bar"; }
                strider {
                    path "strider"
                    _allow_exec_host_cmd true
                }
                compact-bar { path "compact-bar"; }
            }
        "#;
        let config = Config::from_kdl(config_contents, None).unwrap();
        let mut expected_plugin_configuration = HashMap::new();
        expected_plugin_configuration.insert(PluginTag::new("tab-bar"), PluginConfig {
            path: PathBuf::from("tab-bar"),
            run: PluginType::Pane(None),
            location: RunPluginLocation::Zellij(PluginTag::new("tab-bar")),
            _allow_exec_host_cmd: false
        });
        expected_plugin_configuration.insert(PluginTag::new("status-bar"), PluginConfig {
            path: PathBuf::from("status-bar"),
            run: PluginType::Pane(None),
            location: RunPluginLocation::Zellij(PluginTag::new("status-bar")),
            _allow_exec_host_cmd: false
        });
        expected_plugin_configuration.insert(PluginTag::new("strider"), PluginConfig {
            path: PathBuf::from("strider"),
            run: PluginType::Pane(None),
            location: RunPluginLocation::Zellij(PluginTag::new("strider")),
            _allow_exec_host_cmd: true
        });
        expected_plugin_configuration.insert(PluginTag::new("compact-bar"), PluginConfig {
            path: PathBuf::from("compact-bar"),
            run: PluginType::Pane(None),
            location: RunPluginLocation::Zellij(PluginTag::new("compact-bar")),
            _allow_exec_host_cmd: false
        });
        assert_eq!(config.plugins, PluginsConfig::from_data(expected_plugin_configuration), "Plugins defined in config");
    }

    #[test]
    fn can_define_ui_configuration_in_configfile() {
        let config_contents = r#"
            ui {
                pane_frames {
                    rounded_corners true
                }
            }
        "#;
        let config = Config::from_kdl(config_contents, None).unwrap();
        let expected_ui_config = UiConfig {
            pane_frames: FrameConfig {
                rounded_corners: true
            }
        };
        assert_eq!(config.ui, Some(expected_ui_config), "Ui config defined in config");
    }

    #[test]
    fn can_define_env_variables_in_config_file() {
        let config_contents = r#"
            env {
                RUST_BACKTRACE 1
                SOME_OTHER_VAR "foo"
            }
        "#;
        let config = Config::from_kdl(config_contents, None).unwrap();
        let mut expected_env_config = HashMap::new();
        expected_env_config.insert("RUST_BACKTRACE".into(), "1".into());
        expected_env_config.insert("SOME_OTHER_VAR".into(), "foo".into());
        assert_eq!(config.env, EnvironmentVariables::from_data(expected_env_config), "Env variables defined in config");
    }

}
