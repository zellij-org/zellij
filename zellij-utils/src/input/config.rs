//! Deserializes configuration options.
use std::fmt;
use std::fs::File;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use thiserror::Error;
use crate::data::{self, Palette, PaletteColor, PluginTag, CharOrArrow, Key, InputMode};
use super::layout::RunPluginLocation;

use super::actions::{Action, Direction};

use std::collections::{HashMap, HashSet};

use kdl::{KdlDocument, KdlValue, KdlNode};

use serde::{Deserialize, Serialize};
use std::convert::{TryFrom, TryInto};

use super::keybinds::Keybinds;
use super::options::{Options, OnForceClose, Clipboard};
use super::plugins::{PluginsConfig, PluginsConfigError, PluginConfig, PluginType};
use super::theme::{UiConfig, Theme, Themes, FrameConfig};
use crate::cli::{CliArgs, Command};
use crate::envs::EnvironmentVariables;
use crate::setup;
use crate::{entry_count, kdl_entries_as_i64, kdl_first_entry_as_string, kdl_first_entry_as_i64};

// const DEFAULT_CONFIG_FILE_NAME: &str = "config.yaml";
const DEFAULT_CONFIG_FILE_NAME: &str = "config.kdl";

type ConfigResult = Result<Config, ConfigError>;

// /// Intermediate deserialization config struct
// #[derive(Clone, Default, Debug, Deserialize, Serialize, PartialEq)]
// pub struct ConfigFromYaml {
//     #[serde(flatten)]
//     pub options: Option<Options>,
//     pub keybinds: Option<KeybindsFromYaml>,
//     pub themes: Option<ThemesFromYamlIntermediate>,
//     #[serde(flatten)]
//     pub env: Option<EnvironmentVariables>,
//     #[serde(default)]
//     pub plugins: PluginsConfigFromYaml,
//     pub ui: Option<UiConfig>,
// }

/// Main configuration.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Config {
    pub keybinds: Keybinds,
    pub options: Options,
    pub themes: Themes,
    pub plugins: PluginsConfig,
    pub ui: UiConfig,
    pub env: EnvironmentVariables,
}

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
    #[error("Config error: {0}")]
    Std(#[from] Box<dyn std::error::Error>),
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
            Some(theme_name) => self.themes.get_theme(theme_name).map(|theme| theme.palette),
            None => self.themes.get_theme("default").map(|theme| theme.palette)
        }
    }
    pub fn from_kdl(kdl_config: &str, base_config: Option<Config>) -> ConfigResult {
        let mut config = base_config.unwrap_or_else(|| Config::default());
        let kdl_config: KdlDocument = kdl_config.parse()?;
        // TODO: handle cases where we have more than one of these blocks (eg. two "keybinds")
        // this should give an informative parsing error
        if let Some(kdl_keybinds) = kdl_config.get("keybinds") {
            config.keybinds = Keybinds::from_kdl(&kdl_keybinds, config.keybinds)?;
        }
        config.options = Options::from_kdl(&kdl_config);
        if let Some(kdl_themes) = kdl_config.get("themes") {
            config.themes = Themes::from_kdl(kdl_themes)?;
        }
        if let Some(kdl_plugin_config) = kdl_config.get("plugins") {
            config.plugins = PluginsConfig::from_kdl(kdl_plugin_config)?;
        }
        if let Some(kdl_ui_config) = kdl_config.get("ui") {
            config.ui = UiConfig::from_kdl(&kdl_ui_config)?;
        }
        if let Some(env_config) = kdl_config.get("env") {
            config.env = EnvironmentVariables::from_kdl(&env_config)?;
        }
        Ok(config)
    }

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
            },
            Err(e) => Err(ConfigError::IoPath(e, path.into())),
        }
    }
}

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
        let ctrl_g_normal_mode_action = config
            .keybinds
            .get_actions_for_key_in_mode(&InputMode::Normal, &Key::Ctrl('g'));
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
        let alt_h_normal_mode_action = config
            .keybinds
            .get_actions_for_key_in_mode(
                &InputMode::Normal,
                &Key::Alt(CharOrArrow::Direction(data::Direction::Left))
            );
        let alt_left_normal_mode_action = config
            .keybinds
            .get_actions_for_key_in_mode(
                &InputMode::Normal,
                &Key::Alt(CharOrArrow::Char('h'))
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
        let z_in_pane_mode = config
            .keybinds
            .get_actions_for_key_in_mode(
                &InputMode::Pane,
                &Key::Char('z'),
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
        let z_in_pane_mode = config
            .keybinds
            .get_actions_for_key_in_mode(
                &InputMode::Pane,
                &Key::Char('z'),
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
        let z_in_pane_mode = config
            .keybinds
            .get_actions_for_key_in_mode(
                &InputMode::Pane,
                &Key::Char('z'),
            );
        let uppercase_z_in_pane_mode = config
            .keybinds
            .get_actions_for_key_in_mode(
                &InputMode::Pane,
                &Key::Char('Z'),
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
        let z_in_pane_mode = config
            .keybinds
            .get_actions_for_key_in_mode(
                &InputMode::Pane,
                &Key::Char('z'),
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
        let z_in_pane_mode = config
            .keybinds
            .get_actions_for_key_in_mode(
                &InputMode::Pane,
                &Key::Char('z'),
            );
        let r_in_pane_mode = config
            .keybinds
            .get_actions_for_key_in_mode(
                &InputMode::Pane,
                &Key::Char('r'),
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
        let ctrl_g_normal_mode_action = config
            .keybinds
            .get_actions_for_key_in_mode(&InputMode::Normal, &Key::Ctrl('g'));
        let z_in_pane_mode = config
            .keybinds
            .get_actions_for_key_in_mode(
                &InputMode::Pane,
                &Key::Char('z'),
            );
        let ctrl_r_in_normal_mode = config
            .keybinds
            .get_actions_for_key_in_mode(
                &InputMode::Normal,
                &Key::Ctrl('r'),
            );
        let r_in_pane_mode = config
            .keybinds
            .get_actions_for_key_in_mode(
                &InputMode::Pane,
                &Key::Char('r'),
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
        let ctrl_g_normal_mode_action = config
            .keybinds
            .get_actions_for_key_in_mode(
                &InputMode::Normal,
                &Key::Ctrl('g'),
            );
        let z_in_pane_mode = config
            .keybinds
            .get_actions_for_key_in_mode(
                &InputMode::Pane,
                &Key::Char('z'),
            );
        let r_in_pane_mode = config
            .keybinds
            .get_actions_for_key_in_mode(
                &InputMode::Pane,
                &Key::Char('r'),
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
        let ctrl_g_normal_mode_action = config
            .keybinds
            .get_actions_for_key_in_mode(
                &InputMode::Normal,
                &Key::Ctrl('g'),
            );
        let ctrl_g_pane_mode_action = config
            .keybinds
            .get_actions_for_key_in_mode(
                &InputMode::Pane,
                &Key::Ctrl('g'),
            );
        let r_in_pane_mode = config
            .keybinds
            .get_actions_for_key_in_mode(
                &InputMode::Pane,
                &Key::Char('r'),
            );
        let z_in_pane_mode = config
            .keybinds
            .get_actions_for_key_in_mode(
                &InputMode::Pane,
                &Key::Char('z'),
            );
        let t_in_pane_mode = config
            .keybinds
            .get_actions_for_key_in_mode(
                &InputMode::Pane,
                &Key::Char('t'),
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
        let ctrl_g_normal_mode_action = config
            .keybinds
            .get_actions_for_key_in_mode(&InputMode::Normal, &Key::Ctrl('g'));
        let ctrl_g_pane_mode_action = config
            .keybinds
            .get_actions_for_key_in_mode(&InputMode::Pane, &Key::Ctrl('g'));
        let r_in_pane_mode = config
            .keybinds
            .get_actions_for_key_in_mode(
                &InputMode::Pane,
                &Key::Char('r'),
            );
        let z_in_pane_mode = config
            .keybinds
            .get_actions_for_key_in_mode(
                &InputMode::Pane,
                &Key::Char('z'),
            );
        let t_in_pane_mode = config
            .keybinds
            .get_actions_for_key_in_mode(
                &InputMode::Pane,
                &Key::Char('t'),
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
        let z_in_pane_mode = config
            .keybinds
            .get_actions_for_key_in_mode(
                &InputMode::Pane,
                &Key::Char('z'),
            );
        assert_eq!(z_in_pane_mode, None, "Key was ultimately unbound");
    }


    #[test]
    fn can_define_options_in_configfile() {
        // TODO: consider writing a macro to generate a test like this for each option








        let config_contents = r#"
            simplified_ui true
            theme "my cool theme"
            default_mode "locked"
            default_shell "/path/to/my/shell"
            default_layout "/path/to/my/layout.kdl"
            layout_dir "/path/to/my/layout-dir"
            theme_dir "/path/to/my/theme-dir"
            mouse_mode false
            pane_frames false
            mirror_session true
            on_force_close "quit"
            scroll_buffer_size 100000
            copy_command "/path/to/my/copy-command"
            copy_clipboard "primary"
            copy_on_select false
            scrollback_editor "/path/to/my/scrollback-editor"
            session_name "my awesome session"
            attach_to_session true
        "#;
        let config = Config::from_kdl(config_contents, None).unwrap();
        assert_eq!(config.options.simplified_ui, Some(true), "Option set in config");
        assert_eq!(config.options.theme, Some(String::from("my cool theme")), "Option set in config");
        assert_eq!(config.options.default_mode, Some(InputMode::Locked), "Option set in config");
        assert_eq!(config.options.default_shell, Some(PathBuf::from("/path/to/my/shell")), "Option set in config");
        assert_eq!(config.options.default_layout, Some(PathBuf::from("/path/to/my/layout.kdl")), "Option set in config");
        assert_eq!(config.options.layout_dir, Some(PathBuf::from("/path/to/my/layout-dir")), "Option set in config");
        assert_eq!(config.options.theme_dir, Some(PathBuf::from("/path/to/my/theme-dir")), "Option set in config");
        assert_eq!(config.options.mouse_mode, Some(false), "Option set in config");
        assert_eq!(config.options.pane_frames, Some(false), "Option set in config");
        assert_eq!(config.options.mirror_session, Some(true), "Option set in config");
        assert_eq!(config.options.on_force_close, Some(OnForceClose::Quit), "Option set in config");
        assert_eq!(config.options.scroll_buffer_size, Some(100000), "Option set in config");
        assert_eq!(config.options.copy_command, Some(String::from("/path/to/my/copy-command")), "Option set in config");
        assert_eq!(config.options.copy_clipboard, Some(Clipboard::Primary), "Option set in config");
        assert_eq!(config.options.copy_on_select, Some(false), "Option set in config");
        assert_eq!(config.options.scrollback_editor, Some(PathBuf::from("/path/to/my/scrollback-editor")), "Option set in config");
        assert_eq!(config.options.session_name, Some(String::from("my awesome session")), "Option set in config");
        assert_eq!(config.options.attach_to_session, Some(true), "Option set in config");
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
        let expected_themes = Themes::from_data(expected_themes);
        assert_eq!(config.themes, expected_themes, "Theme defined in config");
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
        let expected_themes = Themes::from_data(expected_themes);
        assert_eq!(config.themes, expected_themes, "Theme defined in config");
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
        let expected_themes = Themes::from_data(expected_themes);
        assert_eq!(config.themes, expected_themes, "Theme defined in config");
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
        assert_eq!(config.ui, expected_ui_config, "Ui config defined in config");
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
