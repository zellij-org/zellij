use crate::data::{CommandOrPlugin, LayoutInfo};
use crate::input::options::Options;
use crate::pane_size::Size;
use crate::{
    home::{find_default_config_dir, get_theme_dir},
    input::{config::Config, layout::Layout, theme::Themes},
    setup::{get_default_themes, themes_with_defaults},
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

pub const HOST_TERMINAL_ENV_VARS: [&str; 9] = [
    "SSH_CONNECTION",
    "SSH_TTY",
    "TERM",
    "TERM_PROGRAM",
    "TERM_PROGRAM_VERSION",
    "KITTY_WINDOW_ID",
    "WEZTERM_PANE",
    "ITERM_SESSION_ID",
    "GHOSTTY_RESOURCES_DIR",
];

pub fn host_terminal_env() -> BTreeMap<String, String> {
    host_terminal_env_from(|name| std::env::var(name).ok())
}

pub const DESKTOP_TERMINAL_ENV_KEY: &str = "ZELLIJ_DESKTOP_TERMINAL";

pub fn client_location(
    host_terminal_env: &BTreeMap<String, String>,
    is_web_client: bool,
) -> Option<String> {
    if is_web_client {
        return Some("through the web server".to_owned());
    }
    if host_terminal_env.contains_key(DESKTOP_TERMINAL_ENV_KEY) {
        return None;
    }
    if let Some(connection) = host_terminal_env.get("SSH_CONNECTION") {
        return Some(match connection.split_whitespace().next() {
            Some(address) if !address.is_empty() => {
                format!("from a terminal, over SSH from {}", address)
            },
            _ => "from a terminal, over SSH".to_owned(),
        });
    }
    if host_terminal_env.contains_key("SSH_TTY") {
        return Some("from a terminal, over SSH".to_owned());
    }
    Some("from a terminal".to_owned())
}

pub fn host_terminal_env_from<F: Fn(&str) -> Option<String>>(
    lookup: F,
) -> BTreeMap<String, String> {
    HOST_TERMINAL_ENV_VARS
        .iter()
        .filter_map(|name| lookup(name).map(|value| (name.to_string(), value)))
        .collect()
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct CliAssets {
    pub config_file_path: Option<PathBuf>,
    pub config_dir: Option<PathBuf>,
    pub should_ignore_config: bool,
    pub configuration_options: Option<Options>, // merged from everywhere: there are the source of truth
    pub layout: Option<LayoutInfo>,
    pub terminal_window_size: Size,
    pub data_dir: Option<PathBuf>,
    pub is_debug: bool,
    pub max_panes: Option<usize>,
    pub force_run_layout_commands: bool,
    pub cwd: Option<PathBuf>,
    pub host_terminal_env: BTreeMap<String, String>,
    pub initial_panes: Option<Vec<CommandOrPlugin>>,
    #[serde(default)]
    pub switch_context: Option<crate::session_history::SwitchContext>,
}

impl CliAssets {
    pub fn load_config_and_layout(&self) -> (Config, Layout) {
        let mut config = {
            if self.should_ignore_config {
                Config::from_default_assets().unwrap_or_else(|_| Default::default())
            } else if let Some(ref path) = self.config_file_path {
                let default_config =
                    Config::from_default_assets().unwrap_or_else(|_| Default::default());
                Config::from_path(path, Some(default_config.clone()))
                    .unwrap_or_else(|_| default_config)
            } else {
                Config::from_default_assets().unwrap_or_else(|_| Default::default())
            }
        };
        let config_dir = self.config_dir.clone().or_else(find_default_config_dir);
        let keybinds_dir_override = self
            .configuration_options
            .as_ref()
            .and_then(|options| options.keybinds_dir.clone());
        if config.keybinds_layers.config_dir != config_dir || keybinds_dir_override.is_some() {
            config.keybinds_layers.config_dir = config_dir;
            config.keybinds_layers.keybinds_dir_override = keybinds_dir_override;
            config.resolve_keybinds();
        }

        let (mut layout, mut config_with_merged_layout_opts) = {
            let layout_dir = self
                .configuration_options
                .as_ref()
                .and_then(|e| e.layout_dir.clone())
                .or_else(|| config.options.layout_dir.clone())
                .or_else(|| {
                    self.config_dir
                        .clone()
                        .or_else(find_default_config_dir)
                        .map(|dir| dir.join("layouts"))
                });
            self.layout.as_ref().and_then(|layout_info| {
                Layout::from_layout_info_with_config(&layout_dir, layout_info, Some(config.clone()))
                    .ok()
            })
        }
        .map(|(layout, config)| (layout, config))
        .unwrap_or_else(|| (Layout::default_layout_asset(), config));

        if self.force_run_layout_commands {
            layout.recursively_add_start_suspended(Some(false));
        }

        config_with_merged_layout_opts.themes = themes_with_defaults(
            get_default_themes(),
            config_with_merged_layout_opts.themes.clone(),
        );

        let user_theme_dir = self
            .configuration_options
            .as_ref()
            .and_then(|o| o.theme_dir.clone())
            .or_else(|| {
                config_with_merged_layout_opts
                    .options
                    .theme_dir
                    .clone()
                    .or_else(|| {
                        get_theme_dir(self.config_dir.clone().or_else(find_default_config_dir))
                    })
                    .filter(|dir| dir.exists())
            });
        if let Some(themes) = user_theme_dir.and_then(|u| Themes::from_dir(u).ok()) {
            config_with_merged_layout_opts.themes =
                config_with_merged_layout_opts.themes.merge(themes);
        }

        (config_with_merged_layout_opts, layout)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_whitelisted_host_terminal_variables_are_captured() {
        let env = host_terminal_env_from(|name| match name {
            "TERM" => Some("xterm-kitty".to_owned()),
            "KITTY_WINDOW_ID" => Some("3".to_owned()),
            "SECRET_TOKEN" => Some("hunter2".to_owned()),
            _ => None,
        });
        assert_eq!(
            env,
            [
                ("TERM".to_owned(), "xterm-kitty".to_owned()),
                ("KITTY_WINDOW_ID".to_owned(), "3".to_owned())
            ]
            .into_iter()
            .collect::<BTreeMap<String, String>>(),
            "unset and non-whitelisted variables are left out"
        );
    }

    #[test]
    fn only_a_local_desktop_window_is_known_to_share_the_desktop() {
        let window: BTreeMap<String, String> =
            [(DESKTOP_TERMINAL_ENV_KEY.to_owned(), "1".to_owned())]
                .into_iter()
                .collect();
        assert_eq!(client_location(&window, false), None);
        assert_eq!(
            client_location(&window, true).as_deref(),
            Some("through the web server")
        );
        assert_eq!(
            client_location(&BTreeMap::new(), false).as_deref(),
            Some("from a terminal")
        );
        let over_ssh = host_terminal_env_from(|name| match name {
            "SSH_CONNECTION" => Some("192.168.1.5 50022 192.168.1.2 22".to_owned()),
            _ => None,
        });
        assert_eq!(
            client_location(&over_ssh, false).as_deref(),
            Some("from a terminal, over SSH from 192.168.1.5")
        );
        let tty_only = host_terminal_env_from(|name| match name {
            "SSH_TTY" => Some("/dev/pts/3".to_owned()),
            _ => None,
        });
        assert_eq!(
            client_location(&tty_only, false).as_deref(),
            Some("from a terminal, over SSH")
        );
        let spoofed = host_terminal_env_from(|name| match name {
            DESKTOP_TERMINAL_ENV_KEY => Some("1".to_owned()),
            _ => None,
        });
        assert_eq!(
            client_location(&spoofed, false).as_deref(),
            Some("from a terminal"),
            "a terminal cannot pass itself off as a desktop window"
        );
    }

    #[test]
    fn a_host_without_any_of_the_known_variables_yields_an_empty_env() {
        assert!(host_terminal_env_from(|_| None).is_empty());
    }
}
