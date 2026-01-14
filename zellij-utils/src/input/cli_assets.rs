use crate::data::LayoutInfo;
use crate::input::options::Options;
use crate::pane_size::Size;
use crate::{
    home::find_default_config_dir,
    input::{config::Config, layout::Layout, theme::Themes},
    setup::{get_default_themes, get_theme_dir},
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

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
}

impl CliAssets {
    pub fn load_config_and_layout(&self) -> (Config, Layout) {
        let config = {
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

        config_with_merged_layout_opts.themes = config_with_merged_layout_opts
            .themes
            .merge(get_default_themes());

        let user_theme_dir = self
            .configuration_options
            .as_ref()
            .and_then(|o| o.theme_dir.clone())
            .or_else(|| {
                get_theme_dir(config_with_merged_layout_opts.options.theme_dir.clone())
                    .or_else(find_default_config_dir)
                    .filter(|dir| dir.exists())
            });
        if let Some(themes) = user_theme_dir.and_then(|u| Themes::from_dir(u).ok()) {
            config_with_merged_layout_opts.themes =
                config_with_merged_layout_opts.themes.merge(themes);
        }

        (config_with_merged_layout_opts, layout)
    }
}
