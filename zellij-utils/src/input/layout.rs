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
        config::{Config, ConfigError, LayoutNameInTabError},
    },
    pane_size::{Dimension, PaneGeom},
    setup,
};

use kdl::*;

use std::str::FromStr;
use std::collections::{HashMap, HashSet};

use super::{
    plugins::{PluginTag, PluginsConfigError},
};
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

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
pub enum SplitSize {
    #[serde(alias = "percent")]
    Percent(usize), // 1 to 100
    #[serde(alias = "fixed")]
    Fixed(usize), // An absolute number of columns or rows
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub enum Run {
    #[serde(rename = "plugin")]
    Plugin(RunPlugin),
    #[serde(rename = "command")]
    Command(RunCommand),
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct RunPlugin {
    #[serde(default)]
    pub _allow_exec_host_cmd: bool,
    pub location: RunPluginLocation,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
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

// // The layout struct ultimately used to build the layouts.
// #[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
// pub struct Layout {
//     pub direction: SplitDirection,
//     #[serde(default)]
//     pub pane_name: Option<String>,
//     #[serde(default)]
//     pub parts: LayoutParts,
//     pub split_size: Option<SplitSize>,
//     pub run: Option<Run>,
//     #[serde(default)]
//     pub borderless: bool,
//     pub focus: Option<bool>,
//     pub external_children_index: Option<usize>,
//     pub focused_tab_index: Option<usize>,
//     pub template: Option<Box<Layout>>,
// }

// TODO: CONTINUE HERE - keep following the compiler in kdl_layout_parser.rs
// to work with these new refactored structs
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
pub struct Layout {
    pub tabs: Vec<(Option<String>, PaneLayout)>,
    pub focused_tab_index: Option<usize>,
    pub template: Option<PaneLayout>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
pub struct PaneLayout {
    pub children_split_direction: SplitDirection,
    pub name: Option<String>,
    pub children: Vec<PaneLayout>,
    pub split_size: Option<SplitSize>,
    pub run: Option<Run>,
    pub borderless: bool,
    pub focus: Option<bool>,
    pub external_children_index: Option<usize>,
}

impl PaneLayout {
    pub fn insert_children_layout(&mut self, children_layout: &mut PaneLayout) -> Result<bool, ConfigError> {
        // returns true if successfully inserted and false otherwise
        match self.external_children_index {
            Some(external_children_index) => {
                self.children.insert(external_children_index, children_layout.clone());
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
            }
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
    pub fn position_panes_in_space(&self, space: &PaneGeom) -> Vec<(PaneLayout, PaneGeom)> {
        let res = split_space(space, self, space);
        log::info!("res: {:#?}", res);
        res
    }
    pub fn extract_run_instructions(&self) -> Vec<Option<Run>> {
        let mut run_instructions = vec![];
        if self.children.is_empty() {
            run_instructions.push(self.run.clone());
        }
        for child in &self.children {
            let mut child_run_instructions = child.extract_run_instructions();
            run_instructions.append(&mut child_run_instructions);
        }
        run_instructions
    }
    pub fn with_one_pane() -> Self {
        // TODO: do we need this?
        let mut default_layout = PaneLayout::default();
        default_layout.children = vec![PaneLayout::default()];
        default_layout
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
            LayoutParts::Panes(panes) => {
                panes.is_empty()
            },
            LayoutParts::Tabs(tabs) => {
                tabs.is_empty()
            }
        }
    }
    pub fn insert_pane(&mut self, index: usize, layout: Layout) -> Result<(), ConfigError> {
        match self {
            LayoutParts::Panes(panes) => {
                panes.insert(index, layout);
                Ok(())
            },
            LayoutParts::Tabs(_tabs) => {
                Err(ConfigError::KdlParsingError("Trying to insert a pane into a tab layout".into()))
            }
        }
    }
}

impl Default for LayoutParts {
    fn default() -> Self {
        LayoutParts::Panes(vec![])
    }
}

impl Layout {
    pub fn stringified_from_path_or_default(layout_path: Option<&PathBuf>, layout_dir: Option<PathBuf>) -> Result<String, ConfigError> {
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
            None => {
                Layout::stringified_from_dir(
                    &std::path::PathBuf::from("default"),
                    layout_dir.as_ref(),
                )
            }
        }
    }
    pub fn from_path_or_default(layout_path: Option<&PathBuf>, layout_dir: Option<PathBuf>, config: Config) -> Result<(Layout, Config), ConfigError> {
        let raw_layout = Layout::stringified_from_path_or_default(layout_path, layout_dir)?;
        let kdl_layout: KdlDocument = raw_layout.parse()?;
        let layout = Layout::from_kdl(&kdl_layout)?;
        let config = Config::from_kdl(&raw_layout, Some(config))?; // this merges the two config, with
        Ok((layout, config))
    }
    pub fn from_str(raw: &str) -> Result<Layout, ConfigError> {
        let kdl_layout: KdlDocument = raw.parse()?;
        Layout::from_kdl(&kdl_layout)
    }
    pub fn stringified_from_dir(
        layout: &PathBuf,
        layout_dir: Option<&PathBuf>,
    ) -> Result<String, ConfigError> {
        match layout_dir {
            Some(dir) => {
                let layout_path = &dir.join(layout);
                if layout_path.with_extension("kdl").exists() {
                    Self::stringified_from_path(layout_path)
                } else {
                    Layout::stringified_from_default_assets(layout)
                }
            },
            None => Layout::stringified_from_default_assets(layout),
        }
    }
    pub fn stringified_from_path(layout_path: &Path) -> Result<String, ConfigError> {
        let mut layout_file = File::open(&layout_path)
            .or_else(|_| File::open(&layout_path.with_extension("kdl")))
            .map_err(|e| ConfigError::IoPath(e, layout_path.into()))?;

        let mut kdl_layout = String::new();
        layout_file.read_to_string(&mut kdl_layout)?;
        Ok(kdl_layout)
    }
    pub fn stringified_from_default_assets(path: &Path) -> Result<String, ConfigError> {
        // TODO: ideally these should not be hard-coded
        // we should load layouts by name from the config
        // and load them from a hashmap or some such
        match path.to_str() {
            Some("default") => Self::stringified_default_from_assets(),
            Some("strider") => Self::stringified_strider_from_assets(),
            Some("disable-status-bar") => Self::stringified_disable_status_from_assets(),
            Some("compact") => Self::stringified_compact_from_assets(),
            None | Some(_) => Err(ConfigError::IoPath(
                std::io::Error::new(std::io::ErrorKind::Other, "The layout was not found"),
                path.into(),
            )),
        }
    }
    pub fn stringified_default_from_assets() -> Result<String, ConfigError> {
        Ok(String::from_utf8(setup::DEFAULT_LAYOUT.to_vec())?)
    }

    pub fn stringified_strider_from_assets() -> Result<String, ConfigError> {
        Ok(String::from_utf8(setup::STRIDER_LAYOUT.to_vec())?)
    }

    pub fn stringified_disable_status_from_assets() -> Result<String, ConfigError> {
        Ok(String::from_utf8(setup::NO_STATUS_LAYOUT.to_vec())?)
    }

    pub fn stringified_compact_from_assets() -> Result<String, ConfigError> {
        Ok(String::from_utf8(setup::COMPACT_BAR_LAYOUT.to_vec())?)
    }

//     pub fn total_terminal_panes(&self) -> usize {
//         // TODO: better
//         let mut total_panes = 0;
//         match &self.parts {
//             LayoutParts::Panes(parts) => {
//                 total_panes += parts.len();
//                 for part in parts {
//                     match part.run {
//                         Some(Run::Command(_)) | None => {
//                             total_panes += part.total_terminal_panes();
//                         },
//                         Some(Run::Plugin(_)) => {},
//                     }
//                 }
//                 total_panes
//             },
//             LayoutParts::Tabs(tabs) => {
//                 // let parts = tabs.values();
//                 total_panes += tabs.len();
//                 for tab in tabs {
//                     let (_tab_name, part) = tab;
//                     match part.run {
//                         Some(Run::Command(_)) | None => {
//                             total_panes += part.total_terminal_panes();
//                         },
//                         Some(Run::Plugin(_)) => {},
//                     }
//                 }
//                 total_panes
//             }
//         }
//     }

//     pub fn total_borderless_panes(&self) -> usize {
//         // TODO: better
//         let mut total_borderless_panes = 0;
//         match &self.parts {
//             LayoutParts::Panes(parts) => {
//                 total_borderless_panes += parts.iter().filter(|p| p.borderless).count();
//                 for part in parts {
//                     total_borderless_panes += part.total_borderless_panes();
//                 }
//                 total_borderless_panes
//             },
//             LayoutParts::Tabs(tabs) => {
//                 total_borderless_panes += tabs.iter().filter(|(_, p)| p.borderless).count();
//                 for part in tabs {
//                     let (_part_name, part) = part;
//                     total_borderless_panes += part.total_borderless_panes();
//                 }
//                 total_borderless_panes
//             }
//         }
//     }
//     pub fn extract_run_instructions(&self) -> Vec<Option<Run>> {
//         // TODO: better
//         let mut run_instructions = vec![];
//         match &self.parts {
//             LayoutParts::Panes(parts) => {
//                 if parts.is_empty() {
//                     run_instructions.push(self.run.clone());
//                 }
//                 for part in parts {
//                     let mut current_runnables = part.extract_run_instructions();
//                     run_instructions.append(&mut current_runnables);
//                 }
//             },
//             LayoutParts::Tabs(tabs) => {
//                 if tabs.len() == 0 {
//                     run_instructions.push(self.run.clone());
//                 }
//                 for tab in tabs {
//                     let (_part_name, part) = tab;
//                     let mut current_runnables = part.extract_run_instructions();
//                     run_instructions.append(&mut current_runnables);
//                 }
//             }
//         }
//         run_instructions
//     }

//     pub fn position_panes_in_space(&self, space: &PaneGeom) -> Vec<(PaneLayout, PaneGeom)> {
//         split_space(space, self)
//     }

    pub fn new_tab(&self) -> PaneLayout {
        match &self.template {
            Some(template) => template.clone(),
            None => PaneLayout::default()
        }
    }

    pub fn is_empty(&self) -> bool {
        !self.tabs.is_empty()
//         match &self.parts {
//             LayoutParts::Tabs(tabs) => tabs.is_empty(),
//             LayoutParts::Panes(panes) => panes.is_empty(),
//         }
    }
    // TODO: do we need both of these?
    pub fn has_tabs(&self) -> bool {
        !self.tabs.is_empty()
    }

    pub fn tabs(&self) -> Vec<(Option<String>, PaneLayout)> { // String is the tab name
        self.tabs.clone()
//         match &self.parts {
//             LayoutParts::Tabs(tabs) => tabs.clone(),
//             _ => vec![]
//         }
    }

    pub fn focused_tab_index(&self) -> Option<usize> {
        self.focused_tab_index
    }

//     pub fn children_block_count(&self) -> usize {
//         let mut count = 0;
//         if self.external_children_index.is_some() {
//             count += 1;
//         }
//         match &self.parts {
//             LayoutParts::Tabs(tabs) => {
//                 for tab in tabs {
//                     count += tab.1.children_block_count();
//                 }
//             }
//             LayoutParts::Panes(panes) => {
//                 for pane in panes {
//                     count += pane.children_block_count();
//                 }
//             }
//         }
//         count
//     }
//     pub fn insert_children_layout(&mut self, children_layout: &mut Layout) -> Result<bool, ConfigError> {
//         // returns true if successfully inserted and false otherwise
//         let external_children_index = self.external_children_index;
//         match &mut self.parts {
//             LayoutParts::Tabs(tabs) => Err(ConfigError::KdlParsingError("Cannot insert child layout in tabs".into())),
//             LayoutParts::Panes(panes) => {
//                 match external_children_index {
//                     Some(external_children_index) => {
//                         panes.insert(external_children_index, children_layout.clone());
//                         self.external_children_index = None;
//                         Ok(true)
//                     },
//                     None => {
//                         for pane in panes.iter_mut() {
//                             if pane.insert_children_layout(children_layout)? {
//                                 return Ok(true);
//                             }
//                         }
//                         Ok(false)
//                     }
//                 }
//             }
//         }
//     }
}

// fn layout_size(direction: SplitDirection, layout: &Layout) -> usize {
//     fn child_layout_size(
//         direction: SplitDirection,
//         parent_direction: SplitDirection,
//         layout: &Layout,
//     ) -> usize {
//         let size = if parent_direction == direction { 1 } else { 0 };
//         let parts_is_empty = match &layout.parts {
//             LayoutParts::Panes(parts) => parts.is_empty(),
//             LayoutParts::Tabs(tabs) => tabs.len() == 0
//         };
//         // if layout.parts.is_empty() {
//         if parts_is_empty {
//             size
//         } else {
//             match &layout.parts {
//                 LayoutParts::Panes(parts) => {
//                     let children_size = parts
//                         .iter()
//                         .map(|p| child_layout_size(direction, layout.direction, p))
//                         .sum();
//                     max(size, children_size)
//                 },
//                 LayoutParts::Tabs(tabs) => {
//                     let children_size = tabs
//                         .iter()
//                         .map(|(_, p)| child_layout_size(direction, layout.direction, p))
//                         .sum();
//                     max(size, children_size)
//                 }
//             }
//         }
//     }
//     child_layout_size(direction, direction, layout)
// }

fn split_space(space_to_split: &PaneGeom, layout: &PaneLayout, total_space_to_split: &PaneGeom) -> Vec<(PaneLayout, PaneGeom)> {
    let mut pane_positions = Vec::new();
    let sizes: Vec<Option<SplitSize>> = layout.children.iter().map(|part| part.split_size).collect();

    let mut split_geom = Vec::new();
    let (mut current_position, split_dimension_space, mut inherited_dimension, total_split_dimension_space, total_inherited_dimension_space) =
        match layout.children_split_direction {
            SplitDirection::Vertical => (space_to_split.x, space_to_split.cols, space_to_split.rows, total_space_to_split.cols, total_space_to_split.rows),
            SplitDirection::Horizontal => (space_to_split.y, space_to_split.rows, space_to_split.cols, total_space_to_split.rows, total_space_to_split.cols),
        };

    let flex_parts = sizes.iter().filter(|s| s.is_none()).count();

    let mut total_pane_size = 0;
    for (&size, part) in sizes.iter().zip(&*layout.children) {
        let mut split_dimension = match size {
            Some(SplitSize::Percent(percent)) => Dimension::percent(percent as f64),
            Some(SplitSize::Fixed(size)) => Dimension::fixed(size),
            None => {
                let total_fixed_size = split_dimension_space.as_usize();
                let free_percent = if let Some(p) = split_dimension_space.as_percent() {
                    p - sizes
                        .iter()
                        .map(|&s| {
                            match s {
                                Some(SplitSize::Percent(ip)) => ip as f64,
                                Some(SplitSize::Fixed(fixed)) => (fixed as f64 / total_fixed_size as f64) * 100.0,
                                _ => 0.0,
                            }
                        })
                        .sum::<f64>()
                } else {
                    panic!("Implicit sizing within fixed-size panes is not supported");
                };
                Dimension::percent(free_percent / flex_parts as f64)
            },
        };
        split_dimension.adjust_inner(total_split_dimension_space.as_usize());
        total_pane_size += split_dimension.as_usize();

        let geom = match layout.children_split_direction {
            SplitDirection::Vertical => PaneGeom {
                x: current_position,
                y: space_to_split.y,
                cols: split_dimension,
                rows: inherited_dimension,
            },
            SplitDirection::Horizontal => PaneGeom {
                x: space_to_split.x,
                y: current_position,
                cols: inherited_dimension,
                rows: split_dimension,
            },
        };
        split_geom.push(geom);
        current_position += split_dimension.as_usize();
    }

    // add extra space from rounding errors to the last pane
    if total_pane_size < split_dimension_space.as_usize() {
        let increase_by = split_dimension_space.as_usize() - total_pane_size;
        if let Some(last_geom) = split_geom.last_mut() {
            match layout.children_split_direction {
                SplitDirection::Vertical => last_geom.cols.increase_inner(increase_by),
                SplitDirection::Horizontal => last_geom.rows.increase_inner(increase_by),
            }
        }
    }
    for (i, part) in layout.children.iter().enumerate() {
        let part_position_and_size = split_geom.get(i).unwrap();
        if !part.children.is_empty() {
            let mut part_positions = split_space(part_position_and_size, part, total_space_to_split);
            pane_positions.append(&mut part_positions);
        } else {
            pane_positions.push((part.clone(), *part_position_and_size));
        }
    }
    if pane_positions.is_empty() {
        pane_positions.push((layout.clone(), space_to_split.clone()));
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
                Ok(Self::File(path))
            },
            _ => Err(PluginsConfigError::InvalidUrl(url)),
        }
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
            Ok(SplitSize::Percent(percent_size))
        } else {
            let fixed_size = usize::from_str_radix(s, 10)?;
            Ok(SplitSize::Fixed(fixed_size))
        }
    }
}

// The unit test location.
#[cfg(test)]
#[path = "./unit/layout_test.rs"]
mod layout_test;
