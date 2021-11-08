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
    input::{
        command::RunCommand,
        config::{ConfigError, LayoutNameInTabError},
    },
    pane_size::{Dimension, PaneGeom},
    setup,
};
use crate::{serde, serde_yaml};

use super::plugins::{PluginTag, PluginsConfigError};
use serde::{Deserialize, Serialize};
use std::convert::{TryFrom, TryInto};
use std::vec::Vec;
use std::{
    cmp::max,
    fmt, fs,
    ops::Not,
    path::{Path, PathBuf},
};
use std::{fs::File, io::prelude::*};
use url::Url;

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, Clone, Copy)]
#[serde(crate = "self::serde")]
pub enum Direction {
    Horizontal,
    Vertical,
}

impl Not for Direction {
    type Output = Self;

    fn not(self) -> Self::Output {
        match self {
            Direction::Horizontal => Direction::Vertical,
            Direction::Vertical => Direction::Horizontal,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq)]
#[serde(crate = "self::serde")]
pub enum SplitSize {
    Percent(f64), // 1 to 100
    Fixed(usize), // An absolute number of columns or rows
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(crate = "self::serde")]
pub enum Run {
    #[serde(rename = "plugin")]
    Plugin(RunPlugin),
    #[serde(rename = "command")]
    Command(RunCommand),
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(crate = "self::serde")]
pub enum RunFromYaml {
    #[serde(rename = "plugin")]
    Plugin(RunPluginFromYaml),
    #[serde(rename = "command")]
    Command(RunCommand),
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(crate = "self::serde")]
pub struct RunPluginFromYaml {
    #[serde(default)]
    pub _allow_exec_host_cmd: bool,
    pub location: Url,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(crate = "self::serde")]
pub struct RunPlugin {
    #[serde(default)]
    pub _allow_exec_host_cmd: bool,
    pub location: RunPluginLocation,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(crate = "self::serde")]
pub enum RunPluginLocation {
    File(PathBuf),
    Zellij(PluginTag),
}

impl From<&RunPluginLocation> for Url {
    fn from(location: &RunPluginLocation) -> Self {
        let url = match location {
            RunPluginLocation::File(path) => format!(
                "file:{}",
                path.clone().into_os_string().into_string().unwrap()
            ),
            RunPluginLocation::Zellij(tag) => format!("zellij:{}", tag),
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
        }
    }
}

// The layout struct ultimately used to build the layouts.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(crate = "self::serde")]
pub struct Layout {
    pub direction: Direction,
    #[serde(default)]
    pub parts: Vec<Layout>,
    pub split_size: Option<SplitSize>,
    pub run: Option<Run>,
    #[serde(default)]
    pub borderless: bool,
}

// The struct that is used to deserialize the layout from
// a yaml configuration file
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(crate = "self::serde")]
#[serde(default)]
pub struct LayoutFromYaml {
    #[serde(default)]
    pub session: SessionFromYaml,
    #[serde(default)]
    pub template: LayoutTemplate,
    #[serde(default)]
    pub borderless: bool,
    #[serde(default)]
    pub tabs: Vec<TabLayout>,
}

type LayoutFromYamlResult = Result<LayoutFromYaml, ConfigError>;

impl LayoutFromYaml {
    pub fn new(layout_path: &Path) -> LayoutFromYamlResult {
        let mut layout_file = File::open(&layout_path)
            .or_else(|_| File::open(&layout_path.with_extension("yaml")))
            .map_err(|e| ConfigError::IoPath(e, layout_path.into()))?;

        let mut layout = String::new();
        layout_file.read_to_string(&mut layout)?;
        let layout: Option<LayoutFromYaml> = match serde_yaml::from_str(&layout) {
            Err(e) => {
                // needs direct check, as `[ErrorImpl]` is private
                // https://github.com/dtolnay/serde-yaml/issues/121
                if layout.is_empty() {
                    return Ok(LayoutFromYaml::default());
                }
                return Err(ConfigError::Serde(e));
            }
            Ok(config) => config,
        };

        match layout {
            Some(layout) => {
                for tab in layout.tabs.clone() {
                    tab.check()?;
                }
                Ok(layout)
            }
            None => Ok(LayoutFromYaml::default()),
        }
    }

    // It wants to use Path here, but that doesn't compile.
    #[allow(clippy::ptr_arg)]
    pub fn from_dir(layout: &PathBuf, layout_dir: Option<&PathBuf>) -> LayoutFromYamlResult {
        match layout_dir {
            Some(dir) => Self::new(&dir.join(layout))
                .or_else(|_| Self::from_default_assets(layout.as_path())),
            None => Self::from_default_assets(layout.as_path()),
        }
    }

    pub fn from_path_or_default(
        layout: Option<&PathBuf>,
        layout_path: Option<&PathBuf>,
        layout_dir: Option<PathBuf>,
    ) -> Option<LayoutFromYamlResult> {
        layout
            .map(|p| LayoutFromYaml::from_dir(p, layout_dir.as_ref()))
            .or_else(|| layout_path.map(|p| LayoutFromYaml::new(p)))
            .or_else(|| {
                Some(LayoutFromYaml::from_dir(
                    &std::path::PathBuf::from("default"),
                    layout_dir.as_ref(),
                ))
            })
    }
    // Currently still needed but on nightly
    // this is already possible:
    // HashMap<&'static str, Vec<u8>>
    pub fn from_default_assets(path: &Path) -> LayoutFromYamlResult {
        match path.to_str() {
            Some("default") => Self::default_from_assets(),
            Some("strider") => Self::strider_from_assets(),
            Some("disable-status-bar") => Self::disable_status_from_assets(),
            None | Some(_) => Err(ConfigError::IoPath(
                std::io::Error::new(std::io::ErrorKind::Other, "The layout was not found"),
                path.into(),
            )),
        }
    }

    // TODO Deserialize the assets from bytes &[u8],
    // once serde-yaml supports zero-copy
    pub fn default_from_assets() -> LayoutFromYamlResult {
        let layout: LayoutFromYaml =
            serde_yaml::from_str(String::from_utf8(setup::DEFAULT_LAYOUT.to_vec())?.as_str())?;
        Ok(layout)
    }

    pub fn strider_from_assets() -> LayoutFromYamlResult {
        let layout: LayoutFromYaml =
            serde_yaml::from_str(String::from_utf8(setup::STRIDER_LAYOUT.to_vec())?.as_str())?;
        Ok(layout)
    }

    pub fn disable_status_from_assets() -> LayoutFromYamlResult {
        let layout: LayoutFromYaml =
            serde_yaml::from_str(String::from_utf8(setup::NO_STATUS_LAYOUT.to_vec())?.as_str())?;
        Ok(layout)
    }
}

// The struct that is used to deserialize the session from
// a yaml configuration file
#[derive(Debug, Default, Serialize, Deserialize, Clone, PartialEq)]
#[serde(crate = "self::serde")]
pub struct SessionFromYaml {
    pub name: Option<String>,
    #[serde(default = "default_as_some_true")]
    pub attach: Option<bool>,
}

fn default_as_some_true() -> Option<bool> {
    Some(true)
}

// The struct that carries the information template that is used to
// construct the layout
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(crate = "self::serde")]
pub struct LayoutTemplate {
    pub direction: Direction,
    #[serde(default)]
    pub borderless: bool,
    #[serde(default)]
    pub parts: Vec<LayoutTemplate>,
    #[serde(default)]
    pub body: bool,
    pub split_size: Option<SplitSize>,
    pub run: Option<RunFromYaml>,
}

impl LayoutTemplate {
    // Insert an optional `[TabLayout]` at the correct postion
    pub fn insert_tab_layout(mut self, tab_layout: Option<TabLayout>) -> Self {
        if self.body {
            return tab_layout.unwrap_or_default().into();
        }
        for (i, part) in self.parts.clone().iter().enumerate() {
            if part.body {
                self.parts.push(tab_layout.unwrap_or_default().into());
                self.parts.swap_remove(i);
                break;
            }
            // recurse
            let new_part = part.clone().insert_tab_layout(tab_layout.clone());
            self.parts.push(new_part);
            self.parts.swap_remove(i);
        }
        self
    }

    fn from_vec_tab_layout(tab_layout: Vec<TabLayout>) -> Vec<Self> {
        tab_layout
            .iter()
            .map(|tab_layout| Self::from(tab_layout.to_owned()))
            .collect()
    }
}

// The tab-layout struct used to specify each individual tab.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(crate = "self::serde")]
pub struct TabLayout {
    #[serde(default)]
    pub direction: Direction,
    #[serde(default)]
    pub borderless: bool,
    #[serde(default)]
    pub parts: Vec<TabLayout>,
    pub split_size: Option<SplitSize>,
    #[serde(default)]
    pub name: String,
    pub run: Option<RunFromYaml>,
}

impl TabLayout {
    fn check(&self) -> Result<TabLayout, ConfigError> {
        for part in self.parts.iter() {
            part.check()?;
            if !part.name.is_empty() {
                return Err(ConfigError::LayoutNameInTab(LayoutNameInTabError));
            }
        }
        Ok(self.clone())
    }
}

impl Layout {
    pub fn total_terminal_panes(&self) -> usize {
        let mut total_panes = 0;
        total_panes += self.parts.len();
        for part in self.parts.iter() {
            match part.run {
                Some(Run::Command(_)) | None => {
                    total_panes += part.total_terminal_panes();
                }
                Some(Run::Plugin(_)) => {}
            }
        }
        total_panes
    }

    pub fn total_borderless_panes(&self) -> usize {
        let mut total_borderless_panes = 0;
        total_borderless_panes += self.parts.iter().filter(|p| p.borderless).count();
        for part in self.parts.iter() {
            total_borderless_panes += part.total_borderless_panes();
        }
        total_borderless_panes
    }
    pub fn extract_run_instructions(&self) -> Vec<Option<Run>> {
        let mut run_instructions = vec![];
        if self.parts.is_empty() {
            run_instructions.push(self.run.clone());
        }
        for part in self.parts.iter() {
            let mut current_runnables = part.extract_run_instructions();
            run_instructions.append(&mut current_runnables);
        }
        run_instructions
    }

    pub fn position_panes_in_space(&self, space: &PaneGeom) -> Vec<(Layout, PaneGeom)> {
        split_space(space, self)
    }

    pub fn merge_layout_parts(&mut self, mut parts: Vec<Layout>) {
        self.parts.append(&mut parts);
    }

    fn from_vec_tab_layout(tab_layout: Vec<TabLayout>) -> Result<Vec<Self>, ConfigError> {
        tab_layout
            .iter()
            .map(|tab_layout| Layout::try_from(tab_layout.to_owned()))
            .collect()
    }

    fn from_vec_template_layout(
        layout_template: Vec<LayoutTemplate>,
    ) -> Result<Vec<Self>, ConfigError> {
        layout_template
            .iter()
            .map(|layout_template| Layout::try_from(layout_template.to_owned()))
            .collect()
    }
}

fn layout_size(direction: Direction, layout: &Layout) -> usize {
    fn child_layout_size(
        direction: Direction,
        parent_direction: Direction,
        layout: &Layout,
    ) -> usize {
        let size = if parent_direction == direction { 1 } else { 0 };
        if layout.parts.is_empty() {
            size
        } else {
            let children_size = layout
                .parts
                .iter()
                .map(|p| child_layout_size(direction, layout.direction, p))
                .sum::<usize>();
            max(size, children_size)
        }
    }
    child_layout_size(direction, direction, layout)
}

fn split_space(space_to_split: &PaneGeom, layout: &Layout) -> Vec<(Layout, PaneGeom)> {
    let mut pane_positions = Vec::new();
    let sizes: Vec<Option<SplitSize>> = layout.parts.iter().map(|part| part.split_size).collect();

    let mut split_geom = Vec::new();
    let (mut current_position, split_dimension_space, mut inherited_dimension) =
        match layout.direction {
            Direction::Vertical => (space_to_split.x, space_to_split.cols, space_to_split.rows),
            Direction::Horizontal => (space_to_split.y, space_to_split.rows, space_to_split.cols),
        };

    let flex_parts = sizes.iter().filter(|s| s.is_none()).count();

    for (&size, part) in sizes.iter().zip(&layout.parts) {
        let split_dimension = match size {
            Some(SplitSize::Percent(percent)) => Dimension::percent(percent),
            Some(SplitSize::Fixed(size)) => Dimension::fixed(size),
            None => {
                let free_percent = if let Some(p) = split_dimension_space.as_percent() {
                    p - sizes
                        .iter()
                        .map(|&s| {
                            if let Some(SplitSize::Percent(ip)) = s {
                                ip
                            } else {
                                0.0
                            }
                        })
                        .sum::<f64>()
                } else {
                    panic!("Implicit sizing within fixed-size panes is not supported");
                };
                Dimension::percent(free_percent / flex_parts as f64)
            }
        };
        inherited_dimension.set_inner(
            layout
                .parts
                .iter()
                .map(|p| layout_size(!layout.direction, p))
                .max()
                .unwrap(),
        );
        let geom = match layout.direction {
            Direction::Vertical => PaneGeom {
                x: current_position,
                y: space_to_split.y,
                cols: split_dimension,
                rows: inherited_dimension,
            },
            Direction::Horizontal => PaneGeom {
                x: space_to_split.x,
                y: current_position,
                cols: inherited_dimension,
                rows: split_dimension,
            },
        };
        split_geom.push(geom);
        current_position += layout_size(layout.direction, part);
    }

    for (i, part) in layout.parts.iter().enumerate() {
        let part_position_and_size = split_geom.get(i).unwrap();
        if !part.parts.is_empty() {
            let mut part_positions = split_space(part_position_and_size, part);
            pane_positions.append(&mut part_positions);
        } else {
            pane_positions.push((part.clone(), *part_position_and_size));
        }
    }
    pane_positions
}

impl TryFrom<Url> for RunPluginLocation {
    type Error = PluginsConfigError;

    fn try_from(url: Url) -> Result<Self, Self::Error> {
        match url.scheme() {
            "zellij" => Ok(Self::Zellij(PluginTag::new(url.path()))),
            "file" => {
                let path = PathBuf::from(url.path());
                let canonicalize = |p: &Path| {
                    fs::canonicalize(p)
                        .map_err(|_| PluginsConfigError::InvalidPluginLocation(p.to_owned()))
                };
                canonicalize(&path)
                    .or_else(|_| match path.strip_prefix("/") {
                        Ok(path) => canonicalize(path),
                        Err(_) => Err(PluginsConfigError::InvalidPluginLocation(path.to_owned())),
                    })
                    .map(Self::File)
            }
            _ => Err(PluginsConfigError::InvalidUrl(url)),
        }
    }
}

impl TryFrom<RunFromYaml> for Run {
    type Error = PluginsConfigError;

    fn try_from(run: RunFromYaml) -> Result<Self, Self::Error> {
        match run {
            RunFromYaml::Command(command) => Ok(Run::Command(command)),
            RunFromYaml::Plugin(plugin) => Ok(Run::Plugin(RunPlugin {
                _allow_exec_host_cmd: plugin._allow_exec_host_cmd,
                location: plugin.location.try_into()?,
            })),
        }
    }
}

impl TryFrom<TabLayout> for Layout {
    type Error = ConfigError;

    fn try_from(tab: TabLayout) -> Result<Self, Self::Error> {
        Ok(Layout {
            direction: tab.direction,
            borderless: tab.borderless,
            parts: Self::from_vec_tab_layout(tab.parts)?,
            split_size: tab.split_size,
            run: tab.run.map(Run::try_from).transpose()?,
        })
    }
}

impl From<TabLayout> for LayoutTemplate {
    fn from(tab: TabLayout) -> Self {
        Self {
            direction: tab.direction,
            borderless: tab.borderless,
            parts: Self::from_vec_tab_layout(tab.parts),
            body: false,
            split_size: tab.split_size,
            run: tab.run,
        }
    }
}

impl TryFrom<LayoutTemplate> for Layout {
    type Error = ConfigError;

    fn try_from(template: LayoutTemplate) -> Result<Self, Self::Error> {
        Ok(Layout {
            direction: template.direction,
            borderless: template.borderless,
            parts: Self::from_vec_template_layout(template.parts)?,
            split_size: template.split_size,
            run: template
                .run
                .map(Run::try_from)
                // FIXME: This is just Result::transpose but that method is unstable, when it
                // stabalizes we should swap this out.
                .map_or(Ok(None), |r| r.map(Some))?,
        })
    }
}

impl Default for TabLayout {
    fn default() -> Self {
        Self {
            direction: Direction::Horizontal,
            borderless: false,
            parts: vec![],
            split_size: None,
            run: None,
            name: String::new(),
        }
    }
}

impl Default for LayoutTemplate {
    fn default() -> Self {
        Self {
            direction: Direction::Horizontal,
            body: false,
            borderless: false,
            parts: vec![LayoutTemplate {
                direction: Direction::Horizontal,
                body: true,
                borderless: false,
                split_size: None,
                run: None,
                parts: vec![],
            }],
            split_size: None,
            run: None,
        }
    }
}

impl Default for LayoutFromYaml {
    fn default() -> Self {
        Self {
            session: SessionFromYaml::default(),
            template: LayoutTemplate::default(),
            borderless: false,
            tabs: vec![],
        }
    }
}

impl Default for Direction {
    fn default() -> Self {
        Direction::Horizontal
    }
}

// The unit test location.
#[cfg(test)]
#[path = "./unit/layout_test.rs"]
mod layout_test;
