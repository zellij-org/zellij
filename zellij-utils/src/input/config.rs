use crate::data::Styling;
use miette::{Diagnostic, LabeledSpan, NamedSource, SourceCode};
use std::fs::File;
use std::io::{self, Read};
use std::path::PathBuf;
use thiserror::Error;

use std::convert::TryFrom;

use super::keybinds::Keybinds;
use super::options::Options;
use super::plugins::{PluginAliases, PluginsConfigError};
use super::theme::{Themes, UiConfig};
use crate::cli::{CliArgs, Command};
use crate::envs::EnvironmentVariables;
use crate::{home, setup};

const DEFAULT_CONFIG_FILE_NAME: &str = "config.kdl";

type ConfigResult = Result<Config, ConfigError>;

/// Main configuration.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Config {
    pub keybinds: Keybinds,
    pub options: Options,
    pub themes: Themes,
    pub plugins: PluginAliases,
    pub ui: UiConfig,
    pub env: EnvironmentVariables,
}

#[derive(Error, Debug)]
pub struct KdlError {
    pub error_message: String,
    pub src: Option<NamedSource>,
    pub offset: Option<usize>,
    pub len: Option<usize>,
    pub help_message: Option<String>,
}

impl KdlError {
    pub fn add_src(mut self, src_name: String, src_input: String) -> Self {
        self.src = Some(NamedSource::new(src_name, src_input));
        self
    }
}

impl std::fmt::Display for KdlError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        write!(f, "Failed to parse Zellij configuration")
    }
}
use std::fmt::Display;

impl Diagnostic for KdlError {
    fn source_code(&self) -> Option<&dyn SourceCode> {
        match self.src.as_ref() {
            Some(src) => Some(src),
            None => None,
        }
    }
    fn help<'a>(&'a self) -> Option<Box<dyn Display + 'a>> {
        match &self.help_message {
            Some(help_message) => Some(Box::new(help_message)),
            None => Some(Box::new(format!("For more information, please see our configuration guide: https://zellij.dev/documentation/configuration.html")))
        }
    }
    fn labels(&self) -> Option<Box<dyn Iterator<Item = LabeledSpan> + '_>> {
        if let (Some(offset), Some(len)) = (self.offset, self.len) {
            let label = LabeledSpan::new(Some(self.error_message.clone()), offset, len);
            Some(Box::new(std::iter::once(label)))
        } else {
            None
        }
    }
}

#[derive(Error, Debug, Diagnostic)]
pub enum ConfigError {
    // Deserialization error
    #[error("Deserialization error: {0}")]
    KdlDeserializationError(#[from] kdl::KdlError),
    #[error("KdlDeserialization error: {0}")]
    KdlError(KdlError), // TODO: consolidate these
    #[error("Config error: {0}")]
    Std(#[from] Box<dyn std::error::Error>),
    // Io error with path context
    #[error("IoError: {0}, File: {1}")]
    IoPath(io::Error, PathBuf),
    // Internal Deserialization Error
    #[error("FromUtf8Error: {0}")]
    FromUtf8(#[from] std::string::FromUtf8Error),
    // Plugins have a semantic error, usually trying to parse two of the same tag
    #[error("PluginsError: {0}")]
    PluginsError(#[from] PluginsConfigError),
    #[error("{0}")]
    ConversionError(#[from] ConversionError),
    #[error("{0}")]
    DownloadError(String),
}

impl ConfigError {
    pub fn new_kdl_error(error_message: String, offset: usize, len: usize) -> Self {
        ConfigError::KdlError(KdlError {
            error_message,
            src: None,
            offset: Some(offset),
            len: Some(len),
            help_message: None,
        })
    }
    pub fn new_layout_kdl_error(error_message: String, offset: usize, len: usize) -> Self {
        ConfigError::KdlError(KdlError {
            error_message,
            src: None,
            offset: Some(offset),
            len: Some(len),
            help_message: Some(format!("For more information, please see our layout guide: https://zellij.dev/documentation/creating-a-layout.html")),
        })
    }
}

#[derive(Debug, Error)]
pub enum ConversionError {
    #[error("{0}")]
    UnknownInputMode(String),
}

impl TryFrom<&CliArgs> for Config {
    type Error = ConfigError;

    fn try_from(opts: &CliArgs) -> ConfigResult {
        if let Some(ref path) = opts.config {
            let default_config = Config::from_default_assets()?;
            return Config::from_path(path, Some(default_config));
        }

        if let Some(Command::Setup(ref setup)) = opts.command {
            if setup.clean {
                return Config::from_default_assets();
            }
        }

        let config_dir = opts
            .config_dir
            .clone()
            .or_else(home::find_default_config_dir);

        if let Some(ref config) = config_dir {
            let path = config.join(DEFAULT_CONFIG_FILE_NAME);
            if path.exists() {
                let default_config = Config::from_default_assets()?;
                Config::from_path(&path, Some(default_config))
            } else {
                Config::from_default_assets()
            }
        } else {
            Config::from_default_assets()
        }
    }
}

impl Config {
    pub fn theme_config(&self, opts: &Options) -> Option<Styling> {
        match &opts.theme {
            Some(theme_name) => self.themes.get_theme(theme_name).map(|theme| theme.palette),
            None => self.themes.get_theme("default").map(|theme| theme.palette),
        }
    }
    /// Gets default configuration from assets
    pub fn from_default_assets() -> ConfigResult {
        let cfg = String::from_utf8(setup::DEFAULT_CONFIG.to_vec())?;
        match Self::from_kdl(&cfg, None) {
            Ok(config) => Ok(config),
            Err(ConfigError::KdlError(kdl_error)) => Err(ConfigError::KdlError(
                kdl_error.add_src("Default built-in-configuration".into(), cfg),
            )),
            Err(e) => Err(e),
        }
    }
    pub fn from_path(path: &PathBuf, default_config: Option<Config>) -> ConfigResult {
        match File::open(path) {
            Ok(mut file) => {
                let mut kdl_config = String::new();
                file.read_to_string(&mut kdl_config)
                    .map_err(|e| ConfigError::IoPath(e, path.to_path_buf()))?;
                match Config::from_kdl(&kdl_config, default_config) {
                    Ok(config) => Ok(config),
                    Err(ConfigError::KdlDeserializationError(kdl_error)) => {
                        let error_message = match kdl_error.kind {
                            kdl::KdlErrorKind::Context("valid node terminator") => {
                                format!("Failed to deserialize KDL node. \nPossible reasons:\n{}\n{}\n{}\n{}",
                                "- Missing `;` after a node name, eg. { node; another_node; }",
                                "- Missing quotations (\") around an argument node eg. { first_node \"argument_node\"; }",
                                "- Missing an equal sign (=) between node arguments on a title line. eg. argument=\"value\"",
                                "- Found an extraneous equal sign (=) between node child arguments and their values. eg. { argument=\"value\" }")
                            },
                            _ => {
                                String::from(kdl_error.help.unwrap_or("Kdl Deserialization Error"))
                            },
                        };
                        let kdl_error = KdlError {
                            error_message,
                            src: Some(NamedSource::new(
                                path.as_path().as_os_str().to_string_lossy(),
                                kdl_config,
                            )),
                            offset: Some(kdl_error.span.offset()),
                            len: Some(kdl_error.span.len()),
                            help_message: None,
                        };
                        Err(ConfigError::KdlError(kdl_error))
                    },
                    Err(ConfigError::KdlError(kdl_error)) => {
                        Err(ConfigError::KdlError(kdl_error.add_src(
                            path.as_path().as_os_str().to_string_lossy().to_string(),
                            kdl_config,
                        )))
                    },
                    Err(e) => Err(e),
                }
            },
            Err(e) => Err(ConfigError::IoPath(e, path.into())),
        }
    }
    pub fn merge(&mut self, other: Config) -> Result<(), ConfigError> {
        self.options = self.options.merge(other.options);
        self.keybinds.merge(other.keybinds.clone());
        self.themes = self.themes.merge(other.themes);
        self.plugins.merge(other.plugins);
        self.ui = self.ui.merge(other.ui);
        self.env = self.env.merge(other.env);
        Ok(())
    }
}

#[cfg(test)]
mod config_test {
    use super::*;
    use crate::data::{InputMode, Palette, PaletteColor, PluginTag, StyleDeclaration, Styling};
    use crate::input::layout::{RunPlugin, RunPluginLocation};
    use crate::input::options::{Clipboard, OnForceClose};
    use crate::input::plugins::{PluginConfig, PluginType};
    use crate::input::theme::{FrameConfig, Theme, Themes, UiConfig};
    use std::collections::{BTreeMap, HashMap};
    use std::io::Write;
    use tempfile::tempdir;

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
    fn can_define_options_in_configfile() {
        let config_contents = r#"
            simplified_ui true
            theme "my cool theme"
            default_mode "locked"
            default_shell "/path/to/my/shell"
            default_cwd "/path"
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
        assert_eq!(
            config.options.simplified_ui,
            Some(true),
            "Option set in config"
        );
        assert_eq!(
            config.options.theme,
            Some(String::from("my cool theme")),
            "Option set in config"
        );
        assert_eq!(
            config.options.default_mode,
            Some(InputMode::Locked),
            "Option set in config"
        );
        assert_eq!(
            config.options.default_shell,
            Some(PathBuf::from("/path/to/my/shell")),
            "Option set in config"
        );
        assert_eq!(
            config.options.default_cwd,
            Some(PathBuf::from("/path")),
            "Option set in config"
        );
        assert_eq!(
            config.options.default_layout,
            Some(PathBuf::from("/path/to/my/layout.kdl")),
            "Option set in config"
        );
        assert_eq!(
            config.options.layout_dir,
            Some(PathBuf::from("/path/to/my/layout-dir")),
            "Option set in config"
        );
        assert_eq!(
            config.options.theme_dir,
            Some(PathBuf::from("/path/to/my/theme-dir")),
            "Option set in config"
        );
        assert_eq!(
            config.options.mouse_mode,
            Some(false),
            "Option set in config"
        );
        assert_eq!(
            config.options.pane_frames,
            Some(false),
            "Option set in config"
        );
        assert_eq!(
            config.options.mirror_session,
            Some(true),
            "Option set in config"
        );
        assert_eq!(
            config.options.on_force_close,
            Some(OnForceClose::Quit),
            "Option set in config"
        );
        assert_eq!(
            config.options.scroll_buffer_size,
            Some(100000),
            "Option set in config"
        );
        assert_eq!(
            config.options.copy_command,
            Some(String::from("/path/to/my/copy-command")),
            "Option set in config"
        );
        assert_eq!(
            config.options.copy_clipboard,
            Some(Clipboard::Primary),
            "Option set in config"
        );
        assert_eq!(
            config.options.copy_on_select,
            Some(false),
            "Option set in config"
        );
        assert_eq!(
            config.options.scrollback_editor,
            Some(PathBuf::from("/path/to/my/scrollback-editor")),
            "Option set in config"
        );
        assert_eq!(
            config.options.session_name,
            Some(String::from("my awesome session")),
            "Option set in config"
        );
        assert_eq!(
            config.options.attach_to_session,
            Some(true),
            "Option set in config"
        );
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
        expected_themes.insert(
            "dracula".into(),
            Theme {
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
                .into(),
            },
        );
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
        expected_themes.insert(
            "dracula".into(),
            Theme {
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
                .into(),
            },
        );
        expected_themes.insert(
            "nord".into(),
            Theme {
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
                .into(),
            },
        );
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
        expected_themes.insert(
            "eight_bit_theme".into(),
            Theme {
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
                .into(),
            },
        );
        let expected_themes = Themes::from_data(expected_themes);
        assert_eq!(config.themes, expected_themes, "Theme defined in config");
    }

    #[test]
    fn can_define_style_for_theme_with_hex() {
        let config_contents = r##"
            themes {
                named_theme {
                    styling {
                        text_unselected {
                            base "#DCD7BA"
                            emphasis_1 "#DCD7CD"
                            emphasis_2 "#DCD8DD"
                            emphasis_3 "#DCD899"
                            emphasis_4 "#ACD7CD"
                            background   "#1F1F28"
                        }
                        text_selected {
                            base "#16161D"
                            emphasis_1 "#16161D"
                            emphasis_2 "#16161D"
                            emphasis_3 "#16161D"
                            emphasis_4 "#16161D"
                            background   "#9CABCA"
                        }
                        ribbon_unselected {
                            base "#DCD7BA"
                            emphasis_1 "#7FB4CA"
                            emphasis_2 "#A3D4D5"
                            emphasis_3 "#7AA89F"
                            emphasis_4 "#DCD819"
                            background   "#252535"
                        }
                        ribbon_selected {
                            base "#16161D"
                            emphasis_1 "#181820"
                            emphasis_2 "#1A1A22"
                            emphasis_3 "#2A2A37"
                            emphasis_4 "#363646"
                            background   "#76946A"
                        }
                        table_title {
                            base "#DCD7BA"
                            emphasis_1 "#7FB4CA"
                            emphasis_2 "#A3D4D5"
                            emphasis_3 "#7AA89F"
                            emphasis_4 "#DCD819"
                            background   "#252535"
                        }
                        table_cell_unselected {
                            base "#DCD7BA"
                            emphasis_1 "#DCD7CD"
                            emphasis_2 "#DCD8DD"
                            emphasis_3 "#DCD899"
                            emphasis_4 "#ACD7CD"
                            background   "#1F1F28"
                        }
                        table_cell_selected {
                            base "#16161D"
                            emphasis_1 "#181820"
                            emphasis_2 "#1A1A22"
                            emphasis_3 "#2A2A37"
                            emphasis_4 "#363646"
                            background   "#76946A"
                        }
                        list_unselected {
                            base "#DCD7BA"
                            emphasis_1 "#DCD7CD"
                            emphasis_2 "#DCD8DD"
                            emphasis_3 "#DCD899"
                            emphasis_4 "#ACD7CD"
                            background   "#1F1F28"
                        }
                        list_selected {
                            base "#16161D"
                            emphasis_1 "#181820"
                            emphasis_2 "#1A1A22"
                            emphasis_3 "#2A2A37"
                            emphasis_4 "#363646"
                            background   "#76946A"
                        }
                        frame_unselected {
                            base "#DCD8DD"
                            emphasis_1 "#7FB4CA"
                            emphasis_2 "#A3D4D5"
                            emphasis_3 "#7AA89F"
                            emphasis_4 "#DCD819"
                        }
                        frame_selected {
                            base "#76946A"
                            emphasis_1 "#C34043"
                            emphasis_2 "#C8C093"
                            emphasis_3 "#ACD7CD"
                            emphasis_4 "#DCD819"
                        }
                        exit_code_success {
                            base "#76946A"
                            emphasis_1 "#76946A"
                            emphasis_2 "#76946A"
                            emphasis_3 "#76946A"
                            emphasis_4 "#76946A"
                        }
                        exit_code_error {
                            base "#C34043"
                            emphasis_1 "#C34043"
                            emphasis_2 "#C34043"
                            emphasis_3 "#C34043"
                            emphasis_4 "#C34043"
                        }
                    }
                }
            }
            "##;

        let config = Config::from_kdl(config_contents, None).unwrap();
        let mut expected_themes = HashMap::new();
        expected_themes.insert(
            "named_theme".into(),
            Theme {
                palette: Styling {
                    text_unselected: StyleDeclaration {
                        base: PaletteColor::Rgb((220, 215, 186)),
                        emphasis_1: PaletteColor::Rgb((220, 215, 205)),
                        emphasis_2: PaletteColor::Rgb((220, 216, 221)),
                        emphasis_3: PaletteColor::Rgb((220, 216, 153)),
                        emphasis_4: PaletteColor::Rgb((172, 215, 205)),
                        background: PaletteColor::Rgb((31, 31, 40)),
                    },
                    text_selected: StyleDeclaration {
                        base: PaletteColor::Rgb((22, 22, 29)),
                        emphasis_1: PaletteColor::Rgb((22, 22, 29)),
                        emphasis_2: PaletteColor::Rgb((22, 22, 29)),
                        emphasis_3: PaletteColor::Rgb((22, 22, 29)),
                        emphasis_4: PaletteColor::Rgb((22, 22, 29)),
                        background: PaletteColor::Rgb((156, 171, 202)),
                    },
                    ribbon_unselected: StyleDeclaration {
                        base: PaletteColor::Rgb((220, 215, 186)),
                        emphasis_1: PaletteColor::Rgb((127, 180, 202)),
                        emphasis_2: PaletteColor::Rgb((163, 212, 213)),
                        emphasis_3: PaletteColor::Rgb((122, 168, 159)),
                        emphasis_4: PaletteColor::Rgb((220, 216, 25)),
                        background: PaletteColor::Rgb((37, 37, 53)),
                    },
                    ribbon_selected: StyleDeclaration {
                        base: PaletteColor::Rgb((22, 22, 29)),
                        emphasis_1: PaletteColor::Rgb((24, 24, 32)),
                        emphasis_2: PaletteColor::Rgb((26, 26, 34)),
                        emphasis_3: PaletteColor::Rgb((42, 42, 55)),
                        emphasis_4: PaletteColor::Rgb((54, 54, 70)),
                        background: PaletteColor::Rgb((118, 148, 106)),
                    },
                    table_title: StyleDeclaration {
                        base: PaletteColor::Rgb((220, 215, 186)),
                        emphasis_1: PaletteColor::Rgb((127, 180, 202)),
                        emphasis_2: PaletteColor::Rgb((163, 212, 213)),
                        emphasis_3: PaletteColor::Rgb((122, 168, 159)),
                        emphasis_4: PaletteColor::Rgb((220, 216, 25)),
                        background: PaletteColor::Rgb((37, 37, 53)),
                    },
                    table_cell_unselected: StyleDeclaration {
                        base: PaletteColor::Rgb((220, 215, 186)),
                        emphasis_1: PaletteColor::Rgb((220, 215, 205)),
                        emphasis_2: PaletteColor::Rgb((220, 216, 221)),
                        emphasis_3: PaletteColor::Rgb((220, 216, 153)),
                        emphasis_4: PaletteColor::Rgb((172, 215, 205)),
                        background: PaletteColor::Rgb((31, 31, 40)),
                    },
                    table_cell_selected: StyleDeclaration {
                        base: PaletteColor::Rgb((22, 22, 29)),
                        emphasis_1: PaletteColor::Rgb((24, 24, 32)),
                        emphasis_2: PaletteColor::Rgb((26, 26, 34)),
                        emphasis_3: PaletteColor::Rgb((42, 42, 55)),
                        emphasis_4: PaletteColor::Rgb((54, 54, 70)),
                        background: PaletteColor::Rgb((118, 148, 106)),
                    },
                    list_unselected: StyleDeclaration {
                        base: PaletteColor::Rgb((220, 215, 186)),
                        emphasis_1: PaletteColor::Rgb((220, 215, 205)),
                        emphasis_2: PaletteColor::Rgb((220, 216, 221)),
                        emphasis_3: PaletteColor::Rgb((220, 216, 153)),
                        emphasis_4: PaletteColor::Rgb((172, 215, 205)),
                        background: PaletteColor::Rgb((31, 31, 40)),
                    },
                    list_selected: StyleDeclaration {
                        base: PaletteColor::Rgb((22, 22, 29)),
                        emphasis_1: PaletteColor::Rgb((24, 24, 32)),
                        emphasis_2: PaletteColor::Rgb((26, 26, 34)),
                        emphasis_3: PaletteColor::Rgb((42, 42, 55)),
                        emphasis_4: PaletteColor::Rgb((54, 54, 70)),
                        background: PaletteColor::Rgb((118, 148, 106)),
                    },
                    frame_unselected: StyleDeclaration {
                        base: PaletteColor::Rgb((220, 216, 221)),
                        emphasis_1: PaletteColor::Rgb((127, 180, 202)),
                        emphasis_2: PaletteColor::Rgb((163, 212, 213)),
                        emphasis_3: PaletteColor::Rgb((122, 168, 159)),
                        emphasis_4: PaletteColor::Rgb((220, 216, 25)),
                        ..Default::default()
                    },
                    frame_selected: StyleDeclaration {
                        base: PaletteColor::Rgb((118, 148, 106)),
                        emphasis_1: PaletteColor::Rgb((195, 64, 67)),
                        emphasis_2: PaletteColor::Rgb((200, 192, 147)),
                        emphasis_3: PaletteColor::Rgb((172, 215, 205)),
                        emphasis_4: PaletteColor::Rgb((220, 216, 25)),
                        ..Default::default()
                    },
                    exit_code_success: StyleDeclaration {
                        base: PaletteColor::Rgb((118, 148, 106)),
                        emphasis_1: PaletteColor::Rgb((118, 148, 106)),
                        emphasis_2: PaletteColor::Rgb((118, 148, 106)),
                        emphasis_3: PaletteColor::Rgb((118, 148, 106)),
                        emphasis_4: PaletteColor::Rgb((118, 148, 106)),
                        ..Default::default()
                    },
                    exit_code_error: StyleDeclaration {
                        base: PaletteColor::Rgb((195, 64, 67)),
                        emphasis_1: PaletteColor::Rgb((195, 64, 67)),
                        emphasis_2: PaletteColor::Rgb((195, 64, 67)),
                        emphasis_3: PaletteColor::Rgb((195, 64, 67)),
                        emphasis_4: PaletteColor::Rgb((195, 64, 67)),
                        ..Default::default()
                    },
                    ..Default::default()
                },
            },
        );
        let expected_themes = Themes::from_data(expected_themes);
        assert_eq!(config.themes, expected_themes, "Theme defined in config")
    }

    #[test]
    fn can_define_plugin_configuration_in_configfile() {
        let config_contents = r#"
            plugins {
                tab-bar location="zellij:tab-bar"
                status-bar location="zellij:status-bar"
                strider location="zellij:strider"
                compact-bar location="zellij:compact-bar"
                session-manager location="zellij:session-manager"
                welcome-screen location="zellij:session-manager" {
                    welcome_screen true
                }
                filepicker location="zellij:strider"
            }
        "#;
        let config = Config::from_kdl(config_contents, None).unwrap();
        let mut expected_plugin_configuration = BTreeMap::new();
        expected_plugin_configuration.insert(
            "tab-bar".to_owned(),
            RunPlugin::from_url("zellij:tab-bar").unwrap(),
        );
        expected_plugin_configuration.insert(
            "status-bar".to_owned(),
            RunPlugin::from_url("zellij:status-bar").unwrap(),
        );
        expected_plugin_configuration.insert(
            "strider".to_owned(),
            RunPlugin::from_url("zellij:strider").unwrap(),
        );
        expected_plugin_configuration.insert(
            "compact-bar".to_owned(),
            RunPlugin::from_url("zellij:compact-bar").unwrap(),
        );
        expected_plugin_configuration.insert(
            "session-manager".to_owned(),
            RunPlugin::from_url("zellij:session-manager").unwrap(),
        );
        let mut welcome_screen_configuration = BTreeMap::new();
        welcome_screen_configuration.insert("welcome_screen".to_owned(), "true".to_owned());
        expected_plugin_configuration.insert(
            "welcome-screen".to_owned(),
            RunPlugin::from_url("zellij:session-manager")
                .unwrap()
                .with_configuration(welcome_screen_configuration),
        );
        expected_plugin_configuration.insert(
            "filepicker".to_owned(),
            RunPlugin::from_url("zellij:strider").unwrap(),
        );
        assert_eq!(
            config.plugins,
            PluginAliases::from_data(expected_plugin_configuration),
            "Plugins defined in config"
        );
    }

    #[test]
    fn can_define_ui_configuration_in_configfile() {
        let config_contents = r#"
            ui {
                pane_frames {
                    rounded_corners true
                    hide_session_name true
                }
            }
        "#;
        let config = Config::from_kdl(config_contents, None).unwrap();
        let expected_ui_config = UiConfig {
            pane_frames: FrameConfig {
                rounded_corners: true,
                hide_session_name: true,
            },
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
        assert_eq!(
            config.env,
            EnvironmentVariables::from_data(expected_env_config),
            "Env variables defined in config"
        );
    }
}
