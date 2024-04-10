//! The layout system.
//  Layouts have been moved from [`zellij-server`] to
//  [`zellij-utils`] in order to provide more helpful
//  error messages to the user until a more general
//  logging system is in place.
//  In case there is a logging system in place evaluate,
//  if [`zellij-utils`], or [`zellij-server`] is a proper
//  place.
//  If plugins should be able to depend on the layout system
//  then [`zellij-utils`] could be a proper place.
use crate::{
    data::{Direction, LayoutInfo},
    home::{default_layout_dir, find_default_config_dir},
    input::{
        command::RunCommand,
        config::{Config, ConfigError},
    },
    pane_size::{Constraint, Dimension, PaneGeom},
    setup::{self},
};

use std::cmp::Ordering;
use std::fmt::{Display, Formatter};
use std::str::FromStr;

use super::plugins::{PluginAliases, PluginTag, PluginsConfigError};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::vec::Vec;
use std::{
    fmt,
    ops::Not,
    path::{Path, PathBuf},
};
use std::{fs::File, io::prelude::*};
use url::Url;

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, Clone, Copy)]
pub enum SplitDirection {
    Horizontal,
    Vertical,
}

impl Not for SplitDirection {
    type Output = Self;

    fn not(self) -> Self::Output {
        match self {
            SplitDirection::Horizontal => SplitDirection::Vertical,
            SplitDirection::Vertical => SplitDirection::Horizontal,
        }
    }
}

impl From<Direction> for SplitDirection {
    fn from(direction: Direction) -> Self {
        match direction {
            Direction::Left | Direction::Right => SplitDirection::Horizontal,
            Direction::Down | Direction::Up => SplitDirection::Vertical,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
pub enum SplitSize {
    #[serde(alias = "percent")]
    Percent(usize), // 1 to 100
    #[serde(alias = "fixed")]
    Fixed(usize), // An absolute number of columns or rows
}

impl SplitSize {
    pub fn to_fixed(&self, full_size: usize) -> usize {
        match self {
            SplitSize::Percent(percent) => {
                ((*percent as f64 / 100.0) * full_size as f64).floor() as usize
            },
            SplitSize::Fixed(fixed) => *fixed,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Hash)]
pub enum RunPluginOrAlias {
    RunPlugin(RunPlugin),
    Alias(PluginAlias),
}

impl Default for RunPluginOrAlias {
    fn default() -> Self {
        RunPluginOrAlias::RunPlugin(Default::default())
    }
}

impl RunPluginOrAlias {
    pub fn location_string(&self) -> String {
        match self {
            RunPluginOrAlias::RunPlugin(run_plugin) => run_plugin.location.display(),
            RunPluginOrAlias::Alias(plugin_alias) => plugin_alias.name.clone(),
        }
    }
    pub fn populate_run_plugin_if_needed(&mut self, plugin_aliases: &PluginAliases) {
        if let RunPluginOrAlias::Alias(run_plugin_alias) = self {
            if run_plugin_alias.run_plugin.is_some() {
                log::warn!("Overriding plugin alias");
            }
            let merged_run_plugin = plugin_aliases
                .aliases
                .get(run_plugin_alias.name.as_str())
                .map(|r| {
                    let mut merged_run_plugin = r.clone().merge_configuration(
                        &run_plugin_alias
                            .configuration
                            .as_ref()
                            .map(|c| c.inner().clone()),
                    );
                    // if the alias has its own cwd, it should always override the alias
                    // value's cwd
                    if run_plugin_alias.initial_cwd.is_some() {
                        merged_run_plugin.initial_cwd = run_plugin_alias.initial_cwd.clone();
                    }
                    merged_run_plugin
                });
            run_plugin_alias.run_plugin = merged_run_plugin;
        }
    }
    pub fn get_run_plugin(&self) -> Option<RunPlugin> {
        match self {
            RunPluginOrAlias::RunPlugin(run_plugin) => Some(run_plugin.clone()),
            RunPluginOrAlias::Alias(plugin_alias) => plugin_alias.run_plugin.clone(),
        }
    }
    pub fn get_configuration(&self) -> Option<PluginUserConfiguration> {
        self.get_run_plugin().map(|r| r.configuration.clone())
    }
    pub fn get_initial_cwd(&self) -> Option<PathBuf> {
        self.get_run_plugin().and_then(|r| r.initial_cwd.clone())
    }
    pub fn from_url(
        url: &str,
        configuration: &Option<BTreeMap<String, String>>,
        alias_dict: Option<&PluginAliases>,
        cwd: Option<PathBuf>,
    ) -> Result<Self, String> {
        match RunPluginLocation::parse(&url, cwd) {
            Ok(location) => Ok(RunPluginOrAlias::RunPlugin(RunPlugin {
                _allow_exec_host_cmd: false,
                location,
                configuration: configuration
                    .as_ref()
                    .map(|c| PluginUserConfiguration::new(c.clone()))
                    .unwrap_or_default(),
                ..Default::default()
            })),
            Err(PluginsConfigError::InvalidUrlScheme(_))
            | Err(PluginsConfigError::InvalidUrl(..)) => {
                let mut plugin_alias = PluginAlias::new(&url, configuration, None);
                if let Some(alias_dict) = alias_dict {
                    plugin_alias.run_plugin = alias_dict
                        .aliases
                        .get(url)
                        .map(|r| r.clone().merge_configuration(configuration));
                }
                Ok(RunPluginOrAlias::Alias(plugin_alias))
            },
            Err(e) => {
                return Err(format!("Failed to parse plugin location {url}: {}", e));
            },
        }
    }
    pub fn is_equivalent_to_run(&self, run: &Option<Run>) -> bool {
        match (self, run) {
            (
                RunPluginOrAlias::Alias(self_alias),
                Some(Run::Plugin(RunPluginOrAlias::Alias(run_alias))),
            ) => {
                self_alias.name == run_alias.name
                    && self_alias
                        .configuration
                        .as_ref()
                        // we do the is_empty() checks because an empty configuration is the same as no
                        // configuration (i.e. None)
                        .and_then(|c| if c.inner().is_empty() { None } else { Some(c) })
                        == run_alias.configuration.as_ref().and_then(|c| {
                            if c.inner().is_empty() {
                                None
                            } else {
                                Some(c)
                            }
                        })
            },
            (
                RunPluginOrAlias::Alias(self_alias),
                Some(Run::Plugin(RunPluginOrAlias::RunPlugin(other_run_plugin))),
            ) => self_alias.run_plugin.as_ref() == Some(other_run_plugin),
            (
                RunPluginOrAlias::RunPlugin(self_run_plugin),
                Some(Run::Plugin(RunPluginOrAlias::RunPlugin(other_run_plugin))),
            ) => self_run_plugin == other_run_plugin,
            _ => false,
        }
    }
    pub fn with_initial_cwd(mut self, initial_cwd: Option<PathBuf>) -> Self {
        match self {
            RunPluginOrAlias::RunPlugin(ref mut run_plugin) => {
                run_plugin.initial_cwd = initial_cwd;
            },
            RunPluginOrAlias::Alias(ref mut alias) => {
                alias.initial_cwd = initial_cwd;
            },
        }
        self
    }
    pub fn add_initial_cwd(&mut self, initial_cwd: &PathBuf) {
        match self {
            RunPluginOrAlias::RunPlugin(ref mut run_plugin) => {
                run_plugin.initial_cwd = Some(initial_cwd.clone());
            },
            RunPluginOrAlias::Alias(ref mut alias) => {
                alias.initial_cwd = Some(initial_cwd.clone());
            },
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub enum Run {
    #[serde(rename = "plugin")]
    Plugin(RunPluginOrAlias),
    #[serde(rename = "command")]
    Command(RunCommand),
    EditFile(PathBuf, Option<usize>, Option<PathBuf>), // TODO: merge this with TerminalAction::OpenFile
    Cwd(PathBuf),
}

impl Run {
    pub fn merge(base: &Option<Run>, other: &Option<Run>) -> Option<Run> {
        // This method is necessary to merge between pane_templates and their consumers
        // TODO: reconsider the way we parse command/edit/plugin pane_templates from layouts to prevent this
        // madness
        // TODO: handle Plugin variants once there's a need
        match (base, other) {
            (Some(Run::Command(base_run_command)), Some(Run::Command(other_run_command))) => {
                let mut merged = other_run_command.clone();
                if merged.cwd.is_none() && base_run_command.cwd.is_some() {
                    merged.cwd = base_run_command.cwd.clone();
                }
                if merged.args.is_empty() && !base_run_command.args.is_empty() {
                    merged.args = base_run_command.args.clone();
                }
                Some(Run::Command(merged))
            },
            (Some(Run::Command(base_run_command)), Some(Run::Cwd(other_cwd))) => {
                let mut merged = base_run_command.clone();
                merged.cwd = Some(other_cwd.clone());
                Some(Run::Command(merged))
            },
            (Some(Run::Cwd(base_cwd)), Some(Run::Command(other_command))) => {
                let mut merged = other_command.clone();
                if merged.cwd.is_none() {
                    merged.cwd = Some(base_cwd.clone());
                }
                Some(Run::Command(merged))
            },
            (
                Some(Run::Command(base_run_command)),
                Some(Run::EditFile(file_to_edit, line_number, edit_cwd)),
            ) => match &base_run_command.cwd {
                Some(cwd) => Some(Run::EditFile(
                    cwd.join(&file_to_edit),
                    *line_number,
                    Some(cwd.join(edit_cwd.clone().unwrap_or_default())),
                )),
                None => Some(Run::EditFile(
                    file_to_edit.clone(),
                    *line_number,
                    edit_cwd.clone(),
                )),
            },
            (Some(Run::Cwd(cwd)), Some(Run::EditFile(file_to_edit, line_number, edit_cwd))) => {
                let cwd = edit_cwd.clone().unwrap_or(cwd.clone());
                Some(Run::EditFile(
                    cwd.join(&file_to_edit),
                    *line_number,
                    Some(cwd),
                ))
            },
            (Some(_base), Some(other)) => Some(other.clone()),
            (Some(base), _) => Some(base.clone()),
            (None, Some(other)) => Some(other.clone()),
            (None, None) => None,
        }
    }
    pub fn add_cwd(&mut self, cwd: &PathBuf) {
        match self {
            Run::Command(run_command) => match run_command.cwd.as_mut() {
                Some(run_cwd) => {
                    *run_cwd = cwd.join(&run_cwd);
                },
                None => {
                    run_command.cwd = Some(cwd.clone());
                },
            },
            Run::EditFile(path_to_file, _line_number, edit_cwd) => {
                match edit_cwd.as_mut() {
                    Some(edit_cwd) => {
                        *edit_cwd = cwd.join(&edit_cwd);
                    },
                    None => {
                        let _ = edit_cwd.insert(cwd.clone());
                    },
                };
                *path_to_file = cwd.join(&path_to_file);
            },
            Run::Cwd(path) => {
                *path = cwd.join(&path);
            },
            Run::Plugin(run_plugin_or_alias) => {
                run_plugin_or_alias.add_initial_cwd(&cwd);
            },
        }
    }
    pub fn add_args(&mut self, args: Option<Vec<String>>) {
        // overrides the args of a Run::Command if they are Some
        // and not empty
        if let Some(args) = args {
            if let Run::Command(run_command) = self {
                if !args.is_empty() {
                    run_command.args = args.clone();
                }
            }
        }
    }
    pub fn add_close_on_exit(&mut self, close_on_exit: Option<bool>) {
        // overrides the hold_on_close of a Run::Command if it is Some
        // and not empty
        if let Some(close_on_exit) = close_on_exit {
            if let Run::Command(run_command) = self {
                run_command.hold_on_close = !close_on_exit;
            }
        }
    }
    pub fn add_start_suspended(&mut self, start_suspended: Option<bool>) {
        // overrides the hold_on_start of a Run::Command if they are Some
        // and not empty
        if let Some(start_suspended) = start_suspended {
            if let Run::Command(run_command) = self {
                run_command.hold_on_start = start_suspended;
            }
        }
    }
    pub fn is_same_category(first: &Option<Run>, second: &Option<Run>) -> bool {
        match (first, second) {
            (Some(Run::Plugin(..)), Some(Run::Plugin(..))) => true,
            (Some(Run::Command(..)), Some(Run::Command(..))) => true,
            (Some(Run::EditFile(..)), Some(Run::EditFile(..))) => true,
            (Some(Run::Cwd(..)), Some(Run::Cwd(..))) => true,
            _ => false,
        }
    }
    pub fn is_terminal(run: &Option<Run>) -> bool {
        match run {
            Some(Run::Command(..)) | Some(Run::EditFile(..)) | Some(Run::Cwd(..)) | None => true,
            _ => false,
        }
    }
    pub fn get_cwd(&self) -> Option<PathBuf> {
        match self {
            Run::Plugin(_) => None, // TBD
            Run::Command(run_command) => run_command.cwd.clone(),
            Run::EditFile(_file, _line_num, cwd) => cwd.clone(),
            Run::Cwd(cwd) => Some(cwd.clone()),
        }
    }
    pub fn get_run_plugin(&self) -> Option<RunPlugin> {
        match self {
            Run::Plugin(RunPluginOrAlias::RunPlugin(run_plugin)) => Some(run_plugin.clone()),
            Run::Plugin(RunPluginOrAlias::Alias(plugin_alias)) => {
                plugin_alias.run_plugin.as_ref().map(|r| r.clone())
            },
            _ => None,
        }
    }
    pub fn populate_run_plugin_if_needed(&mut self, alias_dict: &PluginAliases) {
        match self {
            Run::Plugin(run_plugin_alias) => {
                run_plugin_alias.populate_run_plugin_if_needed(alias_dict)
            },
            _ => {},
        }
    }
}

#[allow(clippy::derive_hash_xor_eq)]
#[derive(Debug, Serialize, Deserialize, Clone, Hash, Default)]
pub struct RunPlugin {
    #[serde(default)]
    pub _allow_exec_host_cmd: bool,
    pub location: RunPluginLocation,
    pub configuration: PluginUserConfiguration,
    pub initial_cwd: Option<PathBuf>,
}

impl RunPlugin {
    pub fn from_url(url: &str) -> Result<Self, PluginsConfigError> {
        let location = RunPluginLocation::parse(url, None)?;
        Ok(RunPlugin {
            location,
            ..Default::default()
        })
    }
    pub fn with_configuration(mut self, configuration: BTreeMap<String, String>) -> Self {
        self.configuration = PluginUserConfiguration::new(configuration);
        self
    }
    pub fn with_initial_cwd(mut self, initial_cwd: Option<PathBuf>) -> Self {
        self.initial_cwd = initial_cwd;
        self
    }
    pub fn merge_configuration(mut self, configuration: &Option<BTreeMap<String, String>>) -> Self {
        if let Some(configuration) = configuration {
            self.configuration.merge(configuration);
        }
        self
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Hash, Default, Eq)]
pub struct PluginAlias {
    pub name: String,
    pub configuration: Option<PluginUserConfiguration>,
    pub initial_cwd: Option<PathBuf>,
    pub run_plugin: Option<RunPlugin>,
}

impl PartialEq for PluginAlias {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name && self.configuration == other.configuration
    }
}

impl PluginAlias {
    pub fn new(
        name: &str,
        configuration: &Option<BTreeMap<String, String>>,
        initial_cwd: Option<PathBuf>,
    ) -> Self {
        PluginAlias {
            name: name.to_owned(),
            configuration: configuration
                .as_ref()
                .map(|c| PluginUserConfiguration::new(c.clone())),
            initial_cwd,
            ..Default::default()
        }
    }
    pub fn set_caller_cwd_if_not_set(&mut self, caller_cwd: Option<PathBuf>) {
        // we do this only for an alias because in all other cases this will be handled by the
        // "cwd" configuration key above
        // for an alias we might have cases where the cwd is defined on the alias but we still
        // want to pass the "caller" cwd for the plugin the alias resolves into (eg. a
        // filepicker that has access to the whole filesystem but wants to start in a specific
        // folder)
        if let Some(caller_cwd) = caller_cwd {
            if self
                .configuration
                .as_ref()
                .map(|c| c.inner().get("caller_cwd").is_none())
                .unwrap_or(true)
            {
                let configuration = self
                    .configuration
                    .get_or_insert_with(|| PluginUserConfiguration::new(BTreeMap::new()));
                configuration.insert("caller_cwd", caller_cwd.display().to_string());
            }
        }
    }
}

#[allow(clippy::derive_hash_xor_eq)]
impl PartialEq for RunPlugin {
    fn eq(&self, other: &Self) -> bool {
        // TODO: normalize paths here if the location is a file so that relative/absolute paths
        // will work properly
        (&self.location, &self.configuration) == (&other.location, &other.configuration)
    }
}
impl Eq for RunPlugin {}

#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct PluginUserConfiguration(BTreeMap<String, String>);

impl PluginUserConfiguration {
    pub fn new(mut configuration: BTreeMap<String, String>) -> Self {
        // reserved words
        configuration.remove("hold_on_close");
        configuration.remove("hold_on_start");
        configuration.remove("cwd");
        configuration.remove("name");
        configuration.remove("direction");
        configuration.remove("floating");
        configuration.remove("move_to_focused_tab");

        PluginUserConfiguration(configuration)
    }
    pub fn inner(&self) -> &BTreeMap<String, String> {
        &self.0
    }
    pub fn insert(&mut self, config_key: impl Into<String>, config_value: impl Into<String>) {
        self.0.insert(config_key.into(), config_value.into());
    }
    pub fn merge(&mut self, other_config: &BTreeMap<String, String>) {
        for (key, value) in other_config {
            self.0.insert(key.to_owned(), value.clone());
        }
    }
}

impl FromStr for PluginUserConfiguration {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut ret = BTreeMap::new();
        let configs = s.split(',');
        for config in configs {
            let mut config = config.split('=');
            let key = config.next().ok_or("invalid configuration key")?.to_owned();
            let value = config.map(|c| c.to_owned()).collect::<Vec<_>>().join("=");
            ret.insert(key, value);
        }
        Ok(PluginUserConfiguration(ret))
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Hash)]
pub enum RunPluginLocation {
    File(PathBuf),
    Zellij(PluginTag),
    Remote(String),
}

impl Default for RunPluginLocation {
    fn default() -> Self {
        RunPluginLocation::File(Default::default())
    }
}

impl RunPluginLocation {
    pub fn parse(location: &str, cwd: Option<PathBuf>) -> Result<Self, PluginsConfigError> {
        let url = Url::parse(location)?;

        let decoded_path = percent_encoding::percent_decode_str(url.path()).decode_utf8_lossy();

        match url.scheme() {
            "zellij" => Ok(Self::Zellij(PluginTag::new(decoded_path))),
            "file" => {
                let path = if location.starts_with("file:/") {
                    // Path is absolute, its safe to use URL path.
                    //
                    // This is the case if the scheme and : delimiter are followed by a / slash
                    PathBuf::from(decoded_path.as_ref())
                } else if location.starts_with("file:~") {
                    // Unwrap is safe here since location is a valid URL
                    PathBuf::from(location.strip_prefix("file:").unwrap())
                } else {
                    // URL dep doesn't handle relative paths with `file` schema properly,
                    // it always makes them absolute. Use raw location string instead.
                    //
                    // Unwrap is safe here since location is a valid URL
                    let stripped = location.strip_prefix("file:").unwrap();
                    match cwd {
                        Some(cwd) => cwd.join(stripped),
                        None => PathBuf::from(stripped),
                    }
                };
                let path = match shellexpand::full(&path.to_string_lossy().to_string()) {
                    Ok(s) => PathBuf::from(s.as_ref()),
                    Err(e) => {
                        log::error!("Failed to shell expand plugin path: {}", e);
                        path
                    },
                };
                Ok(Self::File(path))
            },
            "https" | "http" => Ok(Self::Remote(url.as_str().to_owned())),
            _ => Err(PluginsConfigError::InvalidUrlScheme(url)),
        }
    }
    pub fn display(&self) -> String {
        match self {
            RunPluginLocation::File(pathbuf) => format!("file:{}", pathbuf.display()),
            RunPluginLocation::Zellij(plugin_tag) => format!("zellij:{}", plugin_tag),
            RunPluginLocation::Remote(url) => String::from(url),
        }
    }
}

impl From<&RunPluginLocation> for Url {
    fn from(location: &RunPluginLocation) -> Self {
        let url = match location {
            RunPluginLocation::File(path) => format!(
                "file:{}",
                path.clone().into_os_string().into_string().unwrap()
            ),
            RunPluginLocation::Zellij(tag) => format!("zellij:{}", tag),
            RunPluginLocation::Remote(url) => String::from(url),
        };
        Self::parse(&url).unwrap()
    }
}

impl fmt::Display for RunPluginLocation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        match self {
            Self::File(path) => write!(
                f,
                "{}",
                path.clone().into_os_string().into_string().unwrap()
            ),
            Self::Zellij(tag) => write!(f, "{}", tag),
            Self::Remote(url) => write!(f, "{}", url),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Ord, PartialOrd, Serialize, Deserialize)]
pub enum LayoutConstraint {
    MaxPanes(usize),
    MinPanes(usize),
    ExactPanes(usize),
    NoConstraint,
}

impl Display for LayoutConstraint {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        match self {
            LayoutConstraint::MaxPanes(max_panes) => write!(f, "max_panes={}", max_panes),
            LayoutConstraint::MinPanes(min_panes) => write!(f, "min_panes={}", min_panes),
            LayoutConstraint::ExactPanes(exact_panes) => write!(f, "exact_panes={}", exact_panes),
            LayoutConstraint::NoConstraint => write!(f, ""),
        }
    }
}

pub type SwapTiledLayout = (BTreeMap<LayoutConstraint, TiledPaneLayout>, Option<String>); // Option<String> is the swap layout name
pub type SwapFloatingLayout = (
    BTreeMap<LayoutConstraint, Vec<FloatingPaneLayout>>,
    Option<String>,
); // Option<String> is the swap layout name

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
pub struct Layout {
    pub tabs: Vec<(Option<String>, TiledPaneLayout, Vec<FloatingPaneLayout>)>,
    pub focused_tab_index: Option<usize>,
    pub template: Option<(TiledPaneLayout, Vec<FloatingPaneLayout>)>,
    pub swap_layouts: Vec<(TiledPaneLayout, Vec<FloatingPaneLayout>)>,
    pub swap_tiled_layouts: Vec<SwapTiledLayout>,
    pub swap_floating_layouts: Vec<SwapFloatingLayout>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub enum PercentOrFixed {
    Percent(usize), // 1 to 100
    Fixed(usize),   // An absolute number of columns or rows
}

impl From<Dimension> for PercentOrFixed {
    fn from(dimension: Dimension) -> Self {
        match dimension.constraint {
            Constraint::Percent(percent) => PercentOrFixed::Percent(percent as usize),
            Constraint::Fixed(fixed_size) => PercentOrFixed::Fixed(fixed_size),
        }
    }
}

impl PercentOrFixed {
    pub fn to_position(&self, whole: usize) -> usize {
        match self {
            PercentOrFixed::Percent(percent) => {
                (whole as f64 / 100.0 * *percent as f64).ceil() as usize
            },
            PercentOrFixed::Fixed(fixed) => {
                if *fixed > whole {
                    whole
                } else {
                    *fixed
                }
            },
        }
    }
}

impl PercentOrFixed {
    pub fn is_zero(&self) -> bool {
        match self {
            PercentOrFixed::Percent(percent) => *percent == 0,
            PercentOrFixed::Fixed(fixed) => *fixed == 0,
        }
    }
}

impl FromStr for PercentOrFixed {
    type Err = Box<dyn std::error::Error>;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.chars().last() == Some('%') {
            let char_count = s.chars().count();
            let percent_size = usize::from_str_radix(&s[..char_count.saturating_sub(1)], 10)?;
            if percent_size <= 100 {
                Ok(PercentOrFixed::Percent(percent_size))
            } else {
                Err("Percent must be between 0 and 100".into())
            }
        } else {
            let fixed_size = usize::from_str_radix(s, 10)?;
            Ok(PercentOrFixed::Fixed(fixed_size))
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
pub struct FloatingPaneLayout {
    pub name: Option<String>,
    pub height: Option<PercentOrFixed>,
    pub width: Option<PercentOrFixed>,
    pub x: Option<PercentOrFixed>,
    pub y: Option<PercentOrFixed>,
    pub run: Option<Run>,
    pub focus: Option<bool>,
    pub already_running: bool,
    pub pane_initial_contents: Option<String>,
}

impl FloatingPaneLayout {
    pub fn add_cwd_to_layout(&mut self, cwd: &PathBuf) {
        match self.run.as_mut() {
            Some(run) => run.add_cwd(cwd),
            None => {
                self.run = Some(Run::Cwd(cwd.clone()));
            },
        }
    }
    pub fn add_start_suspended(&mut self, start_suspended: Option<bool>) {
        if let Some(run) = self.run.as_mut() {
            run.add_start_suspended(start_suspended);
        }
    }
}

impl From<&TiledPaneLayout> for FloatingPaneLayout {
    fn from(pane_layout: &TiledPaneLayout) -> Self {
        FloatingPaneLayout {
            name: pane_layout.name.clone(),
            run: pane_layout.run.clone(),
            focus: pane_layout.focus,
            ..Default::default()
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
pub struct TiledPaneLayout {
    pub children_split_direction: SplitDirection,
    pub name: Option<String>,
    pub children: Vec<TiledPaneLayout>,
    pub split_size: Option<SplitSize>,
    pub run: Option<Run>,
    pub borderless: bool,
    pub focus: Option<bool>,
    pub external_children_index: Option<usize>,
    pub children_are_stacked: bool,
    pub is_expanded_in_stack: bool,
    pub exclude_from_sync: Option<bool>,
    pub run_instructions_to_ignore: Vec<Option<Run>>,
    pub hide_floating_panes: bool, // only relevant if this is the base layout
    pub pane_initial_contents: Option<String>,
}

impl TiledPaneLayout {
    pub fn insert_children_layout(
        &mut self,
        children_layout: &mut TiledPaneLayout,
    ) -> Result<bool, ConfigError> {
        // returns true if successfully inserted and false otherwise
        match self.external_children_index {
            Some(external_children_index) => {
                self.children
                    .insert(external_children_index, children_layout.clone());
                self.external_children_index = None;
                Ok(true)
            },
            None => {
                for pane in self.children.iter_mut() {
                    if pane.insert_children_layout(children_layout)? {
                        return Ok(true);
                    }
                }
                Ok(false)
            },
        }
    }
    pub fn insert_children_nodes(
        &mut self,
        children_nodes: &mut Vec<TiledPaneLayout>,
    ) -> Result<bool, ConfigError> {
        // returns true if successfully inserted and false otherwise
        match self.external_children_index {
            Some(external_children_index) => {
                children_nodes.reverse();
                for child_node in children_nodes.drain(..) {
                    self.children.insert(external_children_index, child_node);
                }
                self.external_children_index = None;
                Ok(true)
            },
            None => {
                for pane in self.children.iter_mut() {
                    if pane.insert_children_nodes(children_nodes)? {
                        return Ok(true);
                    }
                }
                Ok(false)
            },
        }
    }
    pub fn children_block_count(&self) -> usize {
        let mut count = 0;
        if self.external_children_index.is_some() {
            count += 1;
        }
        for pane in &self.children {
            count += pane.children_block_count();
        }
        count
    }
    pub fn pane_count(&self) -> usize {
        if self.children.is_empty() {
            1 // self
        } else {
            let mut pane_count = 0;
            for child in &self.children {
                pane_count += child.pane_count();
            }
            pane_count
        }
    }
    pub fn position_panes_in_space(
        &self,
        space: &PaneGeom,
        max_panes: Option<usize>,
        ignore_percent_split_sizes: bool,
    ) -> Result<Vec<(TiledPaneLayout, PaneGeom)>, &'static str> {
        let layouts = match max_panes {
            Some(max_panes) => {
                let mut layout_to_split = self.clone();
                let pane_count_in_layout = layout_to_split.pane_count();
                if max_panes > pane_count_in_layout {
                    // the + 1 here is because this was previously an "actual" pane and will now
                    // become just a container, so we need to account for it too
                    // TODO: make sure this works when the `children` node has sibling nodes,
                    // because we really should support that
                    let children_count = (max_panes - pane_count_in_layout) + 1;
                    let mut extra_children = vec![TiledPaneLayout::default(); children_count];
                    if !layout_to_split.has_focused_node() {
                        if let Some(last_child) = extra_children.last_mut() {
                            last_child.focus = Some(true);
                        }
                    }
                    let _ = layout_to_split.insert_children_nodes(&mut extra_children);
                } else {
                    layout_to_split.truncate(max_panes);
                }
                if !layout_to_split.has_focused_node() {
                    layout_to_split.focus_deepest_pane();
                }

                split_space(space, &layout_to_split, space, ignore_percent_split_sizes)?
            },
            None => split_space(space, self, space, ignore_percent_split_sizes)?,
        };
        for (_pane_layout, pane_geom) in layouts.iter() {
            if !pane_geom.is_at_least_minimum_size() {
                return Err("No room on screen for this layout!");
            }
        }
        Ok(layouts)
    }
    pub fn extract_run_instructions(&self) -> Vec<Option<Run>> {
        // the order of these run instructions is significant and needs to be the same
        // as the order of the "flattened" layout panes received from eg. position_panes_in_space
        let mut run_instructions = vec![];
        if self.children.is_empty() {
            run_instructions.push(self.run.clone());
        }
        for child in &self.children {
            let mut child_run_instructions = child.extract_run_instructions();
            run_instructions.append(&mut child_run_instructions);
        }
        let mut successfully_ignored = 0;
        for instruction_to_ignore in &self.run_instructions_to_ignore {
            if let Some(position) = run_instructions
                .iter()
                .position(|i| i == instruction_to_ignore)
            {
                run_instructions.remove(position);
                successfully_ignored += 1;
            }
        }
        // we need to do this because if we have an ignored instruction that does not match any
        // running instruction, we'll have an extra pane and our state will be messed up and we'll
        // crash (this can happen for example when breaking a plugin pane into a new tab that does
        // not have room for it but has a terminal instead)
        if successfully_ignored < self.run_instructions_to_ignore.len() {
            for _ in 0..self
                .run_instructions_to_ignore
                .len()
                .saturating_sub(successfully_ignored)
            {
                if let Some(position) = run_instructions.iter().position(|i| i == &None) {
                    run_instructions.remove(position);
                }
            }
        }
        run_instructions
    }
    pub fn ignore_run_instruction(&mut self, run_instruction: Option<Run>) {
        self.run_instructions_to_ignore.push(run_instruction);
    }
    pub fn with_one_pane() -> Self {
        let mut default_layout = TiledPaneLayout::default();
        default_layout.children = vec![TiledPaneLayout::default()];
        default_layout
    }
    pub fn add_cwd_to_layout(&mut self, cwd: &PathBuf) {
        match self.run.as_mut() {
            Some(run) => run.add_cwd(cwd),
            None => {
                self.run = Some(Run::Cwd(cwd.clone()));
            },
        }
        for child in self.children.iter_mut() {
            child.add_cwd_to_layout(cwd);
        }
    }
    pub fn populate_plugin_aliases_in_layout(&mut self, plugin_aliases: &PluginAliases) {
        match self.run.as_mut() {
            Some(run) => run.populate_run_plugin_if_needed(plugin_aliases),
            _ => {},
        }
        for child in self.children.iter_mut() {
            child.populate_plugin_aliases_in_layout(plugin_aliases);
        }
    }
    pub fn deepest_depth(&self) -> usize {
        let mut deepest_child_depth = 0;
        for child in self.children.iter() {
            let child_deepest_depth = child.deepest_depth();
            if child_deepest_depth > deepest_child_depth {
                deepest_child_depth = child_deepest_depth;
            }
        }
        deepest_child_depth + 1
    }
    pub fn focus_deepest_pane(&mut self) {
        let mut deepest_child_index = None;
        let mut deepest_path = 0;
        for (i, child) in self.children.iter().enumerate() {
            let child_deepest_path = child.deepest_depth();
            if child_deepest_path >= deepest_path {
                deepest_path = child_deepest_path;
                deepest_child_index = Some(i)
            }
        }
        match deepest_child_index {
            Some(deepest_child_index) => {
                if let Some(child) = self.children.get_mut(deepest_child_index) {
                    child.focus_deepest_pane();
                }
            },
            None => {
                self.focus = Some(true);
            },
        }
    }
    pub fn truncate(&mut self, max_panes: usize) -> usize {
        // returns remaining children length
        // if max_panes is 1, it means there's only enough panes for this node,
        // if max_panes is 0, this is probably the root layout being called with 0 max panes
        if max_panes <= 1 {
            while !self.children.is_empty() {
                // this is a special case: we're truncating a pane that was previously a logical
                // container but now should be an actual pane - so here we'd like to use its
                // deepest "non-logical" child in order to get all its attributes (eg. borderless)
                let first_child = self.children.remove(0);
                drop(std::mem::replace(self, first_child));
            }
            self.children.clear();
        } else if max_panes <= self.children.len() {
            self.children.truncate(max_panes);
            self.children.iter_mut().for_each(|l| l.children.clear());
        } else {
            let mut remaining_panes = max_panes
                - self
                    .children
                    .iter()
                    .filter(|c| c.children.is_empty())
                    .count();
            for child in self.children.iter_mut() {
                if remaining_panes > 1 && child.children.len() > 0 {
                    remaining_panes =
                        remaining_panes.saturating_sub(child.truncate(remaining_panes));
                } else {
                    child.children.clear();
                }
            }
        }
        if self.children.len() > 0 {
            self.children.len()
        } else {
            1 // just me
        }
    }
    pub fn has_focused_node(&self) -> bool {
        if self.focus.map(|f| f).unwrap_or(false) {
            return true;
        };
        for child in &self.children {
            if child.has_focused_node() {
                return true;
            }
        }
        false
    }
    pub fn recursively_add_start_suspended(&mut self, start_suspended: Option<bool>) {
        if let Some(run) = self.run.as_mut() {
            run.add_start_suspended(start_suspended);
        }
        for child in self.children.iter_mut() {
            child.recursively_add_start_suspended(start_suspended);
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub enum LayoutParts {
    Tabs(Vec<(Option<String>, Layout)>), // String is the tab name
    Panes(Vec<Layout>),
}

impl LayoutParts {
    pub fn is_empty(&self) -> bool {
        match self {
            LayoutParts::Panes(panes) => panes.is_empty(),
            LayoutParts::Tabs(tabs) => tabs.is_empty(),
        }
    }
    pub fn insert_pane(&mut self, index: usize, layout: Layout) -> Result<(), ConfigError> {
        match self {
            LayoutParts::Panes(panes) => {
                panes.insert(index, layout);
                Ok(())
            },
            LayoutParts::Tabs(_tabs) => Err(ConfigError::new_layout_kdl_error(
                "Trying to insert a pane into a tab layout".into(),
                0,
                0,
            )),
        }
    }
}

impl Default for LayoutParts {
    fn default() -> Self {
        LayoutParts::Panes(vec![])
    }
}

impl Layout {
    // the first layout will either be the default one
    pub fn list_available_layouts(
        layout_dir: Option<PathBuf>,
        default_layout_name: &Option<String>,
    ) -> Vec<LayoutInfo> {
        let mut available_layouts = layout_dir
            .clone()
            .or_else(|| default_layout_dir())
            .and_then(|layout_dir| match std::fs::read_dir(layout_dir) {
                Ok(layout_files) => Some(layout_files),
                Err(e) => {
                    log::error!("Failed to read layout dir: {:?}", e);
                    None
                },
            })
            .map(|layout_files| {
                let mut available_layouts = vec![];
                for file in layout_files {
                    if let Ok(file) = file {
                        if Layout::from_path_or_default_without_config(
                            Some(&file.path()),
                            layout_dir.clone(),
                        )
                        .is_ok()
                        {
                            if let Some(file_name) = file.path().file_stem() {
                                available_layouts
                                    .push(LayoutInfo::File(file_name.to_string_lossy().to_string()))
                            }
                        }
                    }
                }
                available_layouts
            })
            .unwrap_or_else(Default::default);
        let default_layout_name = default_layout_name
            .as_ref()
            .map(|d| d.as_str())
            .unwrap_or("default");
        available_layouts.push(LayoutInfo::BuiltIn("default".to_owned()));
        available_layouts.push(LayoutInfo::BuiltIn("strider".to_owned()));
        available_layouts.push(LayoutInfo::BuiltIn("disable-status-bar".to_owned()));
        available_layouts.push(LayoutInfo::BuiltIn("compact".to_owned()));
        available_layouts.sort_by(|a, b| {
            let a_name = a.name();
            let b_name = b.name();
            if a_name == default_layout_name {
                return Ordering::Less;
            } else if b_name == default_layout_name {
                return Ordering::Greater;
            } else {
                a_name.cmp(&b_name)
            }
        });
        available_layouts
    }
    pub fn stringified_from_path_or_default(
        layout_path: Option<&PathBuf>,
        layout_dir: Option<PathBuf>,
    ) -> Result<(String, String, Option<(String, String)>), ConfigError> {
        // (path_to_layout as String, stringified_layout, Option<path_to_swap_layout as String, stringified_swap_layout>)
        match layout_path {
            Some(layout_path) => {
                // The way we determine where to look for the layout is similar to
                // how a path would look for an executable.
                // See the gh issue for more: https://github.com/zellij-org/zellij/issues/1412#issuecomment-1131559720
                if layout_path.extension().is_some() || layout_path.components().count() > 1 {
                    // We look localy!
                    Layout::stringified_from_path(layout_path)
                } else {
                    // We look in the default dir
                    Layout::stringified_from_dir(layout_path, layout_dir.as_ref())
                }
            },
            None => Layout::stringified_from_dir(
                &std::path::PathBuf::from("default"),
                layout_dir.as_ref(),
            ),
        }
    }
    pub fn from_path_or_default(
        layout_path: Option<&PathBuf>,
        layout_dir: Option<PathBuf>,
        config: Config,
    ) -> Result<(Layout, Config), ConfigError> {
        let (path_to_raw_layout, raw_layout, raw_swap_layouts) =
            Layout::stringified_from_path_or_default(layout_path, layout_dir)?;
        let layout = Layout::from_kdl(
            &raw_layout,
            path_to_raw_layout,
            raw_swap_layouts
                .as_ref()
                .map(|(r, f)| (r.as_str(), f.as_str())),
            None,
        )?;
        let config = Config::from_kdl(&raw_layout, Some(config))?; // this merges the two config, with
        Ok((layout, config))
    }
    pub fn from_path_or_default_without_config(
        layout_path: Option<&PathBuf>,
        layout_dir: Option<PathBuf>,
    ) -> Result<Layout, ConfigError> {
        let (path_to_raw_layout, raw_layout, raw_swap_layouts) =
            Layout::stringified_from_path_or_default(layout_path, layout_dir)?;
        let layout = Layout::from_kdl(
            &raw_layout,
            path_to_raw_layout,
            raw_swap_layouts
                .as_ref()
                .map(|(r, f)| (r.as_str(), f.as_str())),
            None,
        )?;
        Ok(layout)
    }
    pub fn from_default_assets(
        layout_name: &Path,
        _layout_dir: Option<PathBuf>,
        config: Config,
    ) -> Result<(Layout, Config), ConfigError> {
        let (path_to_raw_layout, raw_layout, raw_swap_layouts) =
            Layout::stringified_from_default_assets(layout_name)?;
        let layout = Layout::from_kdl(
            &raw_layout,
            path_to_raw_layout,
            raw_swap_layouts
                .as_ref()
                .map(|(r, f)| (r.as_str(), f.as_str())),
            None,
        )?;
        let config = Config::from_kdl(&raw_layout, Some(config))?; // this merges the two config, with
        Ok((layout, config))
    }
    pub fn from_str(
        raw: &str,
        path_to_raw_layout: String,
        swap_layouts: Option<(&str, &str)>, // Option<path_to_swap_layout, stringified_swap_layout>
        cwd: Option<PathBuf>,
    ) -> Result<Layout, ConfigError> {
        Layout::from_kdl(raw, path_to_raw_layout, swap_layouts, cwd)
    }
    pub fn stringified_from_dir(
        layout: &PathBuf,
        layout_dir: Option<&PathBuf>,
    ) -> Result<(String, String, Option<(String, String)>), ConfigError> {
        // (path_to_layout as String, stringified_layout, Option<path_to_swap_layout as String, stringified_swap_layout>)
        match layout_dir {
            Some(dir) => {
                let layout_path = &dir.join(layout);
                if layout_path.with_extension("kdl").exists() {
                    Self::stringified_from_path(layout_path)
                } else {
                    Layout::stringified_from_default_assets(layout)
                }
            },
            None => {
                let home = find_default_config_dir();
                let Some(home) = home else {
                    return Layout::stringified_from_default_assets(layout);
                };

                let layout_path = &home.join(layout);
                Self::stringified_from_path(layout_path)
            },
        }
    }
    pub fn stringified_from_path(
        layout_path: &Path,
    ) -> Result<(String, String, Option<(String, String)>), ConfigError> {
        // (path_to_layout as String, stringified_layout, Option<path_to_swap_layout as String, stringified_swap_layout>)
        let mut layout_file = File::open(&layout_path)
            .or_else(|_| File::open(&layout_path.with_extension("kdl")))
            .map_err(|e| ConfigError::IoPath(e, layout_path.into()))?;

        let swap_layout_and_path = Layout::swap_layout_and_path(&layout_path);

        let mut kdl_layout = String::new();
        layout_file
            .read_to_string(&mut kdl_layout)
            .map_err(|e| ConfigError::IoPath(e, layout_path.into()))?;
        Ok((
            layout_path.as_os_str().to_string_lossy().into(),
            kdl_layout,
            swap_layout_and_path,
        ))
    }
    pub fn stringified_from_default_assets(
        path: &Path,
    ) -> Result<(String, String, Option<(String, String)>), ConfigError> {
        // (path_to_layout as String, stringified_layout, Option<path_to_swap_layout as String, stringified_swap_layout>)
        // TODO: ideally these should not be hard-coded
        // we should load layouts by name from the config
        // and load them from a hashmap or some such
        match path.to_str() {
            Some("default") => Ok((
                "Default layout".into(),
                Self::stringified_default_from_assets()?,
                Some((
                    "Default swap layout".into(),
                    Self::stringified_default_swap_from_assets()?,
                )),
            )),
            Some("strider") => Ok((
                "Strider layout".into(),
                Self::stringified_strider_from_assets()?,
                Some((
                    "Strider swap layout".into(),
                    Self::stringified_strider_swap_from_assets()?,
                )),
            )),
            Some("disable-status-bar") => Ok((
                "Disable Status Bar layout".into(),
                Self::stringified_disable_status_from_assets()?,
                None,
            )),
            Some("compact") => Ok((
                "Compact layout".into(),
                Self::stringified_compact_from_assets()?,
                Some((
                    "Compact layout swap".into(),
                    Self::stringified_compact_swap_from_assets()?,
                )),
            )),
            Some("welcome") => Ok((
                "Welcome screen layout".into(),
                Self::stringified_welcome_from_assets()?,
                None,
            )),
            None | Some(_) => Err(ConfigError::IoPath(
                std::io::Error::new(std::io::ErrorKind::Other, "The layout was not found"),
                path.into(),
            )),
        }
    }
    pub fn stringified_default_from_assets() -> Result<String, ConfigError> {
        Ok(String::from_utf8(setup::DEFAULT_LAYOUT.to_vec())?)
    }
    pub fn stringified_default_swap_from_assets() -> Result<String, ConfigError> {
        Ok(String::from_utf8(setup::DEFAULT_SWAP_LAYOUT.to_vec())?)
    }
    pub fn stringified_strider_from_assets() -> Result<String, ConfigError> {
        Ok(String::from_utf8(setup::STRIDER_LAYOUT.to_vec())?)
    }
    pub fn stringified_strider_swap_from_assets() -> Result<String, ConfigError> {
        Ok(String::from_utf8(setup::STRIDER_SWAP_LAYOUT.to_vec())?)
    }

    pub fn stringified_disable_status_from_assets() -> Result<String, ConfigError> {
        Ok(String::from_utf8(setup::NO_STATUS_LAYOUT.to_vec())?)
    }

    pub fn stringified_compact_from_assets() -> Result<String, ConfigError> {
        Ok(String::from_utf8(setup::COMPACT_BAR_LAYOUT.to_vec())?)
    }

    pub fn stringified_compact_swap_from_assets() -> Result<String, ConfigError> {
        Ok(String::from_utf8(setup::COMPACT_BAR_SWAP_LAYOUT.to_vec())?)
    }

    pub fn stringified_welcome_from_assets() -> Result<String, ConfigError> {
        Ok(String::from_utf8(setup::WELCOME_LAYOUT.to_vec())?)
    }

    pub fn new_tab(&self) -> (TiledPaneLayout, Vec<FloatingPaneLayout>) {
        self.template.clone().unwrap_or_default()
    }

    pub fn is_empty(&self) -> bool {
        !self.tabs.is_empty()
    }
    // TODO: do we need both of these?
    pub fn has_tabs(&self) -> bool {
        !self.tabs.is_empty()
    }

    pub fn tabs(&self) -> Vec<(Option<String>, TiledPaneLayout, Vec<FloatingPaneLayout>)> {
        // String is the tab name
        self.tabs.clone()
    }

    pub fn focused_tab_index(&self) -> Option<usize> {
        self.focused_tab_index
    }

    pub fn recursively_add_start_suspended(&mut self, start_suspended: Option<bool>) {
        for (_tab_name, tiled_panes, floating_panes) in self.tabs.iter_mut() {
            tiled_panes.recursively_add_start_suspended(start_suspended);
            for floating_pane in floating_panes.iter_mut() {
                floating_pane.add_start_suspended(start_suspended);
            }
        }
    }

    fn swap_layout_and_path(path: &Path) -> Option<(String, String)> {
        // Option<path, stringified_swap_layout>
        let mut swap_layout_path = PathBuf::from(path);
        swap_layout_path.set_extension("swap.kdl");
        match File::open(&swap_layout_path) {
            Ok(mut stringified_swap_layout_file) => {
                let mut swap_kdl_layout = String::new();
                match stringified_swap_layout_file.read_to_string(&mut swap_kdl_layout) {
                    Ok(..) => Some((
                        swap_layout_path.as_os_str().to_string_lossy().into(),
                        swap_kdl_layout,
                    )),
                    Err(_e) => None,
                }
            },
            Err(_e) => None,
        }
    }
    pub fn populate_plugin_aliases_in_layout(&mut self, plugin_aliases: &PluginAliases) {
        for tab in self.tabs.iter_mut() {
            tab.1.populate_plugin_aliases_in_layout(plugin_aliases);
            for floating_pane_layout in tab.2.iter_mut() {
                floating_pane_layout
                    .run
                    .as_mut()
                    .map(|f| f.populate_run_plugin_if_needed(&plugin_aliases));
            }
        }
        if let Some(template) = self.template.as_mut() {
            template.0.populate_plugin_aliases_in_layout(plugin_aliases);
            for floating_pane_layout in template.1.iter_mut() {
                floating_pane_layout
                    .run
                    .as_mut()
                    .map(|f| f.populate_run_plugin_if_needed(&plugin_aliases));
            }
        }
    }
    pub fn add_cwd_to_layout(&mut self, cwd: &PathBuf) {
        for (_, tiled_pane_layout, floating_panes) in self.tabs.iter_mut() {
            tiled_pane_layout.add_cwd_to_layout(&cwd);
            for floating_pane in floating_panes {
                floating_pane.add_cwd_to_layout(&cwd);
            }
        }
        if let Some((tiled_pane_layout, floating_panes)) = self.template.as_mut() {
            tiled_pane_layout.add_cwd_to_layout(&cwd);
            for floating_pane in floating_panes {
                floating_pane.add_cwd_to_layout(&cwd);
            }
        }
    }
}

fn split_space(
    space_to_split: &PaneGeom,
    layout: &TiledPaneLayout,
    total_space_to_split: &PaneGeom,
    ignore_percent_split_sizes: bool,
) -> Result<Vec<(TiledPaneLayout, PaneGeom)>, &'static str> {
    let sizes: Vec<Option<SplitSize>> = if layout.children_are_stacked {
        let index_of_expanded_pane = layout.children.iter().position(|p| p.is_expanded_in_stack);
        let mut sizes: Vec<Option<SplitSize>> = layout
            .children
            .iter()
            .map(|_part| Some(SplitSize::Fixed(1)))
            .collect();
        if let Some(index_of_expanded_pane) = index_of_expanded_pane {
            *sizes.get_mut(index_of_expanded_pane).unwrap() = None;
        } else if let Some(last_size) = sizes.last_mut() {
            *last_size = None;
        }
        sizes
    } else if ignore_percent_split_sizes {
        layout
            .children
            .iter()
            .map(|part| match part.split_size {
                Some(SplitSize::Percent(_)) => None,
                split_size => split_size,
            })
            .collect()
    } else {
        layout.children.iter().map(|part| part.split_size).collect()
    };

    let mut split_geom = Vec::new();
    let (
        mut current_position,
        split_dimension_space,
        inherited_dimension,
        total_split_dimension_space,
    ) = match layout.children_split_direction {
        SplitDirection::Vertical => (
            space_to_split.x,
            space_to_split.cols,
            space_to_split.rows,
            total_space_to_split.cols,
        ),
        SplitDirection::Horizontal => (
            space_to_split.y,
            space_to_split.rows,
            space_to_split.cols,
            total_space_to_split.rows,
        ),
    };

    let min_size_for_panes = sizes.iter().fold(0, |acc, size| match size {
        Some(SplitSize::Percent(_)) | None => acc + 1, // TODO: minimum height/width as relevant here
        Some(SplitSize::Fixed(fixed)) => acc + fixed,
    });
    if min_size_for_panes > split_dimension_space.as_usize() {
        return Err("Not enough room for panes"); // TODO: use error infra
    }

    let flex_parts = sizes.iter().filter(|s| s.is_none()).count();
    let total_fixed_size = sizes.iter().fold(0, |acc, s| {
        if let Some(SplitSize::Fixed(fixed)) = s {
            acc + fixed
        } else {
            acc
        }
    });

    let mut total_pane_size = 0;
    for (&size, _part) in sizes.iter().zip(&*layout.children) {
        let mut split_dimension = match size {
            Some(SplitSize::Percent(percent)) => Dimension::percent(percent as f64),
            Some(SplitSize::Fixed(size)) => Dimension::fixed(size),
            None => {
                let free_percent = if let Some(p) = split_dimension_space.as_percent() {
                    p - sizes
                        .iter()
                        .map(|&s| match s {
                            Some(SplitSize::Percent(ip)) => ip as f64,
                            _ => 0.0,
                        })
                        .sum::<f64>()
                } else {
                    panic!("Implicit sizing within fixed-size panes is not supported");
                };
                Dimension::percent(free_percent / flex_parts as f64)
            },
        };

        split_dimension.adjust_inner(
            total_split_dimension_space
                .as_usize()
                .saturating_sub(total_fixed_size),
        );
        total_pane_size += split_dimension.as_usize();

        let geom = match layout.children_split_direction {
            SplitDirection::Vertical => PaneGeom {
                x: current_position,
                y: space_to_split.y,
                cols: split_dimension,
                rows: inherited_dimension,
                is_stacked: layout.children_are_stacked,
            },
            SplitDirection::Horizontal => PaneGeom {
                x: space_to_split.x,
                y: current_position,
                cols: inherited_dimension,
                rows: split_dimension,
                is_stacked: layout.children_are_stacked,
            },
        };
        split_geom.push(geom);
        current_position += split_dimension.as_usize();
    }
    adjust_geoms_for_rounding_errors(
        total_pane_size,
        &mut split_geom,
        split_dimension_space,
        layout.children_split_direction,
    );
    let mut pane_positions = Vec::new();
    for (i, part) in layout.children.iter().enumerate() {
        let part_position_and_size = split_geom.get(i).unwrap();
        if !part.children.is_empty() {
            let mut part_positions = split_space(
                part_position_and_size,
                part,
                total_space_to_split,
                ignore_percent_split_sizes,
            )?;
            pane_positions.append(&mut part_positions);
        } else {
            let part = part.clone();
            pane_positions.push((part, *part_position_and_size));
        }
    }
    if pane_positions.is_empty() {
        let layout = layout.clone();
        pane_positions.push((layout, space_to_split.clone()));
    }
    Ok(pane_positions)
}

fn adjust_geoms_for_rounding_errors(
    total_pane_size: usize,
    split_geoms: &mut Vec<PaneGeom>,
    split_dimension_space: Dimension,
    children_split_direction: SplitDirection,
) {
    if total_pane_size < split_dimension_space.as_usize() {
        // add extra space from rounding errors to the last pane

        let increase_by = split_dimension_space
            .as_usize()
            .saturating_sub(total_pane_size);
        let position_of_last_flexible_geom = split_geoms
            .iter()
            .rposition(|s_g| s_g.is_flexible_in_direction(children_split_direction));
        position_of_last_flexible_geom
            .map(|p| split_geoms.iter_mut().skip(p))
            .map(|mut flexible_geom_and_following_geoms| {
                if let Some(flexible_geom) = flexible_geom_and_following_geoms.next() {
                    match children_split_direction {
                        SplitDirection::Vertical => flexible_geom.cols.increase_inner(increase_by),
                        SplitDirection::Horizontal => {
                            flexible_geom.rows.increase_inner(increase_by)
                        },
                    }
                }
                for following_geom in flexible_geom_and_following_geoms {
                    match children_split_direction {
                        SplitDirection::Vertical => {
                            following_geom.x += increase_by;
                        },
                        SplitDirection::Horizontal => {
                            following_geom.y += increase_by;
                        },
                    }
                }
            });
    } else if total_pane_size > split_dimension_space.as_usize() {
        // remove extra space from rounding errors to the last pane
        let decrease_by = total_pane_size - split_dimension_space.as_usize();
        let position_of_last_flexible_geom = split_geoms
            .iter()
            .rposition(|s_g| s_g.is_flexible_in_direction(children_split_direction));
        position_of_last_flexible_geom
            .map(|p| split_geoms.iter_mut().skip(p))
            .map(|mut flexible_geom_and_following_geoms| {
                if let Some(flexible_geom) = flexible_geom_and_following_geoms.next() {
                    match children_split_direction {
                        SplitDirection::Vertical => flexible_geom.cols.decrease_inner(decrease_by),
                        SplitDirection::Horizontal => {
                            flexible_geom.rows.decrease_inner(decrease_by)
                        },
                    }
                }
                for following_geom in flexible_geom_and_following_geoms {
                    match children_split_direction {
                        SplitDirection::Vertical => {
                            following_geom.x = following_geom.x.saturating_sub(decrease_by)
                        },
                        SplitDirection::Horizontal => {
                            following_geom.y = following_geom.y.saturating_sub(decrease_by)
                        },
                    }
                }
            });
    }
}

impl Default for SplitDirection {
    fn default() -> Self {
        SplitDirection::Horizontal
    }
}

impl FromStr for SplitDirection {
    type Err = Box<dyn std::error::Error>;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "vertical" | "Vertical" => Ok(SplitDirection::Vertical),
            "horizontal" | "Horizontal" => Ok(SplitDirection::Horizontal),
            _ => Err("split direction must be either vertical or horizontal".into()),
        }
    }
}

impl FromStr for SplitSize {
    type Err = Box<dyn std::error::Error>;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.chars().last() == Some('%') {
            let char_count = s.chars().count();
            let percent_size = usize::from_str_radix(&s[..char_count.saturating_sub(1)], 10)?;
            if percent_size > 0 && percent_size <= 100 {
                Ok(SplitSize::Percent(percent_size))
            } else {
                Err("Percent must be between 0 and 100".into())
            }
        } else {
            let fixed_size = usize::from_str_radix(s, 10)?;
            Ok(SplitSize::Fixed(fixed_size))
        }
    }
}

// The unit test location.
#[path = "./unit/layout_test.rs"]
#[cfg(test)]
mod layout_test;
