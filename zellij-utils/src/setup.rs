use crate::{
    cli::{CliArgs, Command},
    consts::{
        FEATURES, SYSTEM_DEFAULT_CONFIG_DIR, SYSTEM_DEFAULT_DATA_DIR_PREFIX, VERSION,
        ZELLIJ_PROJ_DIR,
    },
    input::{
        config::{Config, ConfigError},
        layout::{LayoutFromYaml, LayoutFromYamlIntermediate},
        options::Options,
    },
};
use directories_next::BaseDirs;
use serde::{Deserialize, Serialize};
use std::{convert::TryFrom, io::Write, path::Path, path::PathBuf, process};
use structopt::StructOpt;

const CONFIG_LOCATION: &str = ".config/zellij";
const CONFIG_NAME: &str = "config.yaml";
static ARROW_SEPARATOR: &str = "";

#[cfg(not(test))]
/// Goes through a predefined list and checks for an already
/// existing config directory, returns the first match
pub fn find_default_config_dir() -> Option<PathBuf> {
    default_config_dirs()
        .into_iter()
        .filter(|p| p.is_some())
        .find(|p| p.clone().unwrap().exists())
        .flatten()
}

#[cfg(test)]
pub fn find_default_config_dir() -> Option<PathBuf> {
    None
}

/// Order in which config directories are checked
fn default_config_dirs() -> Vec<Option<PathBuf>> {
    vec![
        home_config_dir(),
        Some(xdg_config_dir()),
        Some(Path::new(SYSTEM_DEFAULT_CONFIG_DIR).to_path_buf()),
    ]
}

/// Looks for an existing dir, uses that, else returns a
/// dir matching the config spec.
pub fn get_default_data_dir() -> PathBuf {
    vec![
        xdg_data_dir(),
        Path::new(SYSTEM_DEFAULT_DATA_DIR_PREFIX).join("share/zellij"),
    ]
    .into_iter()
    .find(|p| p.exists())
    .unwrap_or_else(xdg_data_dir)
}

pub fn xdg_config_dir() -> PathBuf {
    ZELLIJ_PROJ_DIR.config_dir().to_owned()
}

pub fn xdg_data_dir() -> PathBuf {
    ZELLIJ_PROJ_DIR.data_dir().to_owned()
}

pub fn home_config_dir() -> Option<PathBuf> {
    if let Some(user_dirs) = BaseDirs::new() {
        let config_dir = user_dirs.home_dir().join(CONFIG_LOCATION);
        Some(config_dir)
    } else {
        None
    }
}

pub fn get_layout_dir(config_dir: Option<PathBuf>) -> Option<PathBuf> {
    config_dir.map(|dir| dir.join("layouts"))
}

pub fn dump_asset(asset: &[u8]) -> std::io::Result<()> {
    std::io::stdout().write_all(asset)?;
    Ok(())
}

pub const DEFAULT_CONFIG: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/",
    "assets/config/default.yaml"
));

pub const DEFAULT_LAYOUT: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/",
    "assets/layouts/default.yaml"
));

pub const STRIDER_LAYOUT: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/",
    "assets/layouts/strider.yaml"
));

pub const NO_STATUS_LAYOUT: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/",
    "assets/layouts/disable-status-bar.yaml"
));

pub fn dump_default_config() -> std::io::Result<()> {
    dump_asset(DEFAULT_CONFIG)
}

pub fn dump_specified_layout(layout: &str) -> std::io::Result<()> {
    match layout {
        "strider" => dump_asset(STRIDER_LAYOUT),
        "default" => dump_asset(DEFAULT_LAYOUT),
        "disable-status" => dump_asset(NO_STATUS_LAYOUT),
        not_found => Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("Layout: {} not found", not_found),
        )),
    }
}

#[derive(Debug, Default, Clone, StructOpt, Serialize, Deserialize)]
pub struct Setup {
    /// Dump the default configuration file to stdout
    #[structopt(long)]
    pub dump_config: bool,
    /// Disables loading of configuration file at default location,
    /// loads the defaults that zellij ships with
    #[structopt(long)]
    pub clean: bool,
    /// Checks the configuration of zellij and displays
    /// currently used directories
    #[structopt(long)]
    pub check: bool,

    /// Dump the specified layout file to stdout
    #[structopt(long)]
    pub dump_layout: Option<String>,
    /// Generates completion for the specified shell
    #[structopt(long)]
    pub generate_completion: Option<String>,
}

impl Setup {
    /// Entrypoint from main
    /// Merges options from the config file and the command line options
    /// into `[Options]`, the command line options superceeding the layout
    /// file options, superceeding the config file options:
    /// 1. command line options (`zellij options`)
    /// 2. layout options
    ///    (`layout.yaml` / `zellij --layout` / `zellij --layout-path`)
    /// 3. config options (`config.yaml`)
    pub fn from_options(
        opts: &CliArgs,
    ) -> Result<(Config, Option<LayoutFromYaml>, Options), ConfigError> {
        let clean = match &opts.command {
            Some(Command::Setup(ref setup)) => setup.clean,
            _ => false,
        };

        // setup functions that don't require deserialisation of the config
        if let Some(Command::Setup(ref setup)) = &opts.command {
            setup.from_cli().map_or_else(
                |e| {
                    eprintln!("{:?}", e);
                    process::exit(1);
                },
                |_| {},
            );
        };

        let config = if !clean {
            match Config::try_from(opts) {
                Ok(config) => config,
                Err(e) => {
                    return Err(e);
                }
            }
        } else {
            Config::default()
        };

        let config_options = Options::from_cli(&config.options, opts.command.clone());

        let layout_dir = config_options
            .layout_dir
            .clone()
            .or_else(|| get_layout_dir(opts.config_dir.clone().or_else(find_default_config_dir)));
        let layout_result = LayoutFromYamlIntermediate::from_path_or_default(
            opts.layout.as_ref(),
            opts.layout_path.as_ref(),
            layout_dir,
        );
        let layout = match layout_result {
            None => None,
            Some(Ok(layout)) => Some(layout),
            Some(Err(e)) => {
                return Err(e);
            }
        };

        if let Some(Command::Setup(ref setup)) = &opts.command {
            setup
                .from_cli_with_options(opts, &config_options)
                .map_or_else(
                    |e| {
                        eprintln!("{:?}", e);
                        process::exit(1);
                    },
                    |_| {},
                );
        };

        Setup::merge_config_with_layout(config, layout, config_options)
    }

    /// General setup helpers
    pub fn from_cli(&self) -> std::io::Result<()> {
        if self.clean {
            return Ok(());
        }

        if self.dump_config {
            dump_default_config()?;
            std::process::exit(0);
        }

        if let Some(shell) = &self.generate_completion {
            Self::generate_completion(shell.into());
            std::process::exit(0);
        }

        if let Some(layout) = &self.dump_layout {
            dump_specified_layout(layout)?;
            std::process::exit(0);
        }

        Ok(())
    }

    /// Checks the merged configuration
    pub fn from_cli_with_options(
        &self,
        opts: &CliArgs,
        config_options: &Options,
    ) -> std::io::Result<()> {
        if self.check {
            Setup::check_defaults_config(opts, config_options)?;
            std::process::exit(0);
        }
        Ok(())
    }

    fn merge_config_with_layout(
        config: Config,
        layout: Option<LayoutFromYamlIntermediate>,
        config_options: Options,
    ) -> Result<(Config, Option<LayoutFromYaml>, Options), ConfigError> {
        let (layout, layout_config) = match layout.map(|l| l.to_layout_and_config()) {
            None => (None, None),
            Some((layout, layout_config)) => (Some(layout), layout_config),
        };

        let (config, config_options) = if let Some(layout_config) = layout_config {
            let config_options = if let Some(options) = layout_config.options.clone() {
                config_options.merge(options)
            } else {
                config_options
            };
            let config = config.merge(layout_config.try_into()?);
            (config, config_options)
        } else {
            (config, config_options)
        };
        Ok((config, layout, config_options))
    }

    pub fn check_defaults_config(opts: &CliArgs, config_options: &Options) -> std::io::Result<()> {
        let data_dir = opts.data_dir.clone().unwrap_or_else(get_default_data_dir);
        let config_dir = opts.config_dir.clone().or_else(find_default_config_dir);
        let plugin_dir = data_dir.join("plugins");
        let layout_dir = config_options
            .layout_dir
            .clone()
            .or_else(|| get_layout_dir(config_dir.clone()));
        let system_data_dir = PathBuf::from(SYSTEM_DEFAULT_DATA_DIR_PREFIX).join("share/zellij");
        let config_file = opts
            .config
            .clone()
            .or_else(|| config_dir.clone().map(|p| p.join(CONFIG_NAME)));

        // according to
        // https://gist.github.com/egmontkob/eb114294efbcd5adb1944c9f3cb5feda
        let hyperlink_start = "\u{1b}]8;;";
        let hyperlink_mid = "\u{1b}\\";
        let hyperlink_end = "\u{1b}]8;;\u{1b}\\";

        let mut message = String::new();

        message.push_str(&format!("[Version]: {:?}\n", VERSION));
        if let Some(config_dir) = config_dir {
            message.push_str(&format!("[CONFIG DIR]: {:?}\n", config_dir));
        } else {
            message.push_str("[CONFIG DIR]: Not Found\n");
            let mut default_config_dirs = default_config_dirs()
                .iter()
                .filter_map(|p| p.clone())
                .collect::<Vec<PathBuf>>();
            default_config_dirs.dedup();
            message.push_str(
                " On your system zellij looks in the following config directories by default:\n",
            );
            for dir in default_config_dirs {
                message.push_str(&format!(" {:?}\n", dir));
            }
        }
        if let Some(config_file) = config_file {
            message.push_str(&format!("[CONFIG FILE]: {:?}\n", config_file));
            match Config::new(&config_file) {
                Ok(_) => message.push_str("[CONFIG FILE]: Well defined.\n"),
                Err(e) => message.push_str(&format!("[CONFIG ERROR]: {}\n", e)),
            }
        } else {
            message.push_str("[CONFIG FILE]: Not Found\n");
            message.push_str(&format!(
                " By default zellij looks for a file called [{}] in the configuration directory\n",
                CONFIG_NAME
            ));
        }
        message.push_str(&format!("[DATA DIR]: {:?}\n", data_dir));
        message.push_str(&format!("[PLUGIN DIR]: {:?}\n", plugin_dir));
        if let Some(layout_dir) = layout_dir {
            message.push_str(&format!("[LAYOUT DIR]: {:?}\n", layout_dir));
        } else {
            message.push_str("[CONFIG FILE]: Not Found\n");
        }
        message.push_str(&format!("[SYSTEM DATA DIR]: {:?}\n", system_data_dir));

        message.push_str(&format!("[ARROW SEPARATOR]: {}\n", ARROW_SEPARATOR));
        message.push_str(" Is the [ARROW_SEPARATOR] displayed correctly?\n");
        message.push_str(" If not you may want to either start zellij with a compatible mode: 'zellij options --simplified-ui true'\n");
        let mut hyperlink_compat = String::new();
        hyperlink_compat.push_str(hyperlink_start);
        hyperlink_compat.push_str("https://zellij.dev/documentation/compatibility.html#the-status-bar-fonts-dont-render-correctly");
        hyperlink_compat.push_str(hyperlink_mid);
        hyperlink_compat.push_str("https://zellij.dev/documentation/compatibility.html#the-status-bar-fonts-dont-render-correctly");
        hyperlink_compat.push_str(hyperlink_end);
        message.push_str(&format!(
            " Or check the font that is in use:\n {}\n",
            hyperlink_compat
        ));
        message.push_str("[MOUSE INTERACTION]: \n");
        message.push_str(" Can be temporarily disabled through pressing the [SHIFT] key.\n");
        message.push_str(" If that doesn't fix any issues consider to disable the mouse handling of zellij: 'zellij options --disable-mouse-mode'\n");

        message.push_str(&format!("[FEATURES]: {:?}\n", FEATURES));
        let mut hyperlink = String::new();
        hyperlink.push_str(hyperlink_start);
        hyperlink.push_str("https://www.zellij.dev/documentation/");
        hyperlink.push_str(hyperlink_mid);
        hyperlink.push_str("zellij.dev/documentation");
        hyperlink.push_str(hyperlink_end);
        message.push_str(&format!("[DOCUMENTATION]: {}\n", hyperlink));
        //printf '\e]8;;http://example.com\e\\This is a link\e]8;;\e\\\n'

        std::io::stdout().write_all(message.as_bytes())?;

        Ok(())
    }
    fn generate_completion(shell: String) {
        let shell = match shell.as_ref() {
            "bash" => structopt::clap::Shell::Bash,
            "fish" => structopt::clap::Shell::Fish,
            "zsh" => structopt::clap::Shell::Zsh,
            "powerShell" => structopt::clap::Shell::PowerShell,
            "elvish" => structopt::clap::Shell::Elvish,
            other => {
                eprintln!("Unsupported shell: {}", other);
                std::process::exit(1);
            }
        };
        let mut out = std::io::stdout();
        CliArgs::clap().gen_completions_to("zellij", shell, &mut out);
    }
}

#[cfg(test)]
mod setup_test {
    use super::Setup;
    use crate::input::{
        config::{Config, ConfigError},
        keybinds::Keybinds,
        layout::{LayoutFromYaml, LayoutFromYamlIntermediate},
        options::Options,
    };

    fn deserialise_config_and_layout(
        config: &str,
        layout: &str,
    ) -> Result<(Config, LayoutFromYamlIntermediate), ConfigError> {
        let config = Config::from_yaml(&config)?;
        let layout = LayoutFromYamlIntermediate::from_yaml(&layout)?;
        Ok((config, layout))
    }

    #[test]
    fn empty_config_empty_layout() {
        let goal = Config::default();
        let config = r"";
        let layout = r"";
        let config_layout_result = deserialise_config_and_layout(config, layout);
        let (config, layout) = config_layout_result.unwrap();
        let config_options = Options::default();
        let (config, _layout, _config_options) =
            Setup::merge_config_with_layout(config, Some(layout), config_options).unwrap();
        assert_eq!(config, goal);
    }

    #[test]
    fn config_empty_layout() {
        let mut goal = Config::default();
        goal.options.default_shell = Some(std::path::PathBuf::from("fish"));
        let config = r"---
        default_shell: fish";
        let layout = r"";
        let config_layout_result = deserialise_config_and_layout(config, layout);
        let (config, layout) = config_layout_result.unwrap();
        let config_options = Options::default();
        let (config, _layout, _config_options) =
            Setup::merge_config_with_layout(config, Some(layout), config_options).unwrap();
        assert_eq!(config, goal);
    }

    #[test]
    fn layout_overwrites_config() {
        let mut goal = Config::default();
        goal.options.default_shell = Some(std::path::PathBuf::from("bash"));
        let config = r"---
        default_shell: fish";
        let layout = r"---
        default_shell: bash";
        let config_layout_result = deserialise_config_and_layout(config, layout);
        let (config, layout) = config_layout_result.unwrap();
        let config_options = Options::default();
        let (config, _layout, _config_options) =
            Setup::merge_config_with_layout(config, Some(layout), config_options).unwrap();
        assert_eq!(config, goal);
    }

    #[test]
    fn empty_config_nonempty_layout() {
        let mut goal = Config::default();
        goal.options.default_shell = Some(std::path::PathBuf::from("bash"));
        let config = r"";
        let layout = r"---
        default_shell: bash";
        let config_layout_result = deserialise_config_and_layout(config, layout);
        let (config, layout) = config_layout_result.unwrap();
        let config_options = Options::default();
        let (config, _layout, _config_options) =
            Setup::merge_config_with_layout(config, Some(layout), config_options).unwrap();
        assert_eq!(config, goal);
    }

    #[test]
    fn nonempty_config_nonempty_layout() {
        let mut goal = Config::default();
        goal.options.default_shell = Some(std::path::PathBuf::from("bash"));
        goal.options.default_mode = Some(zellij_tile::prelude::InputMode::Locked);
        let config = r"---
        default_mode: locked";
        let layout = r"---
        default_shell: bash";
        let config_layout_result = deserialise_config_and_layout(config, layout);
        let (config, layout) = config_layout_result.unwrap();
        let config_options = Options::default();
        let (config, _layout, _config_options) =
            Setup::merge_config_with_layout(config, Some(layout), config_options).unwrap();
        assert_eq!(config, goal);
    }
}
