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
        config::{Config, ConfigError},
    },
    pane_size::{Dimension, PaneGeom},
    setup,
};

use std::str::FromStr;

use super::plugins::{PluginTag, PluginsConfigError};
use serde::{Deserialize, Serialize};
use std::convert::TryFrom;
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
    EditFile(PathBuf, Option<usize>), // TODO: merge this with TerminalAction::OpenFile
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
            (Some(Run::Command(base_run_command)), Some(Run::EditFile(file_to_edit, line_number))) => {
                match &base_run_command.cwd {
                    Some(cwd) => Some(Run::EditFile(cwd.join(&file_to_edit), *line_number)),
                    None => Some(Run::EditFile(file_to_edit.clone(), *line_number))
                }
            },
            (Some(Run::Cwd(cwd)), Some(Run::EditFile(file_to_edit, line_number))) => {
                Some(Run::EditFile(cwd.join(&file_to_edit), *line_number))
            },
            (Some(_base), Some(other)) => Some(other.clone()),
            (Some(base), _) => Some(base.clone()),
            (None, Some(other)) => Some(other.clone()),
            (None, None) => None,
        }
    }
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
    pub fn insert_children_layout(
        &mut self,
        children_layout: &mut PaneLayout,
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
    pub fn position_panes_in_space(
        &self,
        space: &PaneGeom,
    ) -> Result<Vec<(PaneLayout, PaneGeom)>, &'static str> {
        let layouts = split_space(space, self, space);
        for (_pane_layout, pane_geom) in layouts.iter() {
            if !pane_geom.is_at_least_minimum_size() {
                return Err("No room on screen for this layout!");
            }
        }
        Ok(layouts)
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
            LayoutParts::Tabs(_tabs) => Err(ConfigError::new_kdl_error(
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
    pub fn stringified_from_path_or_default(
        layout_path: Option<&PathBuf>,
        layout_dir: Option<PathBuf>,
    ) -> Result<(String, String), ConfigError> {
        // (path_to_layout as String, stringified_layout)
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
        let (path_to_raw_layout, raw_layout) =
            Layout::stringified_from_path_or_default(layout_path, layout_dir)?;
        let layout = Layout::from_kdl(&raw_layout, path_to_raw_layout, None)?;
        let config = Config::from_kdl(&raw_layout, Some(config))?; // this merges the two config, with
        Ok((layout, config))
    }
    pub fn from_str(
        raw: &str,
        path_to_raw_layout: String,
        cwd: Option<PathBuf>,
    ) -> Result<Layout, ConfigError> {
        Layout::from_kdl(raw, path_to_raw_layout, cwd)
    }
    pub fn stringified_from_dir(
        layout: &PathBuf,
        layout_dir: Option<&PathBuf>,
    ) -> Result<(String, String), ConfigError> {
        // (path_to_layout as String, stringified_layout)
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
    pub fn stringified_from_path(layout_path: &Path) -> Result<(String, String), ConfigError> {
        // (path_to_layout as String, stringified_layout)
        let mut layout_file = File::open(&layout_path)
            .or_else(|_| File::open(&layout_path.with_extension("kdl")))
            .map_err(|e| ConfigError::IoPath(e, layout_path.into()))?;

        let mut kdl_layout = String::new();
        layout_file.read_to_string(&mut kdl_layout)?;
        Ok((layout_path.as_os_str().to_string_lossy().into(), kdl_layout))
    }
    pub fn stringified_from_default_assets(path: &Path) -> Result<(String, String), ConfigError> {
        // (path_to_layout as String, stringified_layout)
        // TODO: ideally these should not be hard-coded
        // we should load layouts by name from the config
        // and load them from a hashmap or some such
        match path.to_str() {
            Some("default") => Ok((
                "Default layout".into(),
                Self::stringified_default_from_assets()?,
            )),
            Some("strider") => Ok((
                "Strider layout".into(),
                Self::stringified_strider_from_assets()?,
            )),
            Some("disable-status-bar") => Ok((
                "Disable Status Bar layout".into(),
                Self::stringified_disable_status_from_assets()?,
            )),
            Some("compact") => Ok((
                "Compact layout".into(),
                Self::stringified_compact_from_assets()?,
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

    pub fn stringified_strider_from_assets() -> Result<String, ConfigError> {
        Ok(String::from_utf8(setup::STRIDER_LAYOUT.to_vec())?)
    }

    pub fn stringified_disable_status_from_assets() -> Result<String, ConfigError> {
        Ok(String::from_utf8(setup::NO_STATUS_LAYOUT.to_vec())?)
    }

    pub fn stringified_compact_from_assets() -> Result<String, ConfigError> {
        Ok(String::from_utf8(setup::COMPACT_BAR_LAYOUT.to_vec())?)
    }

    pub fn new_tab(&self) -> PaneLayout {
        match &self.template {
            Some(template) => template.clone(),
            None => PaneLayout::default(),
        }
    }

    pub fn is_empty(&self) -> bool {
        !self.tabs.is_empty()
    }
    // TODO: do we need both of these?
    pub fn has_tabs(&self) -> bool {
        !self.tabs.is_empty()
    }

    pub fn tabs(&self) -> Vec<(Option<String>, PaneLayout)> {
        // String is the tab name
        self.tabs.clone()
    }

    pub fn focused_tab_index(&self) -> Option<usize> {
        self.focused_tab_index
    }
}

fn split_space(
    space_to_split: &PaneGeom,
    layout: &PaneLayout,
    total_space_to_split: &PaneGeom,
) -> Vec<(PaneLayout, PaneGeom)> {
    let mut pane_positions = Vec::new();
    let sizes: Vec<Option<SplitSize>> =
        layout.children.iter().map(|part| part.split_size).collect();

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

    let flex_parts = sizes.iter().filter(|s| s.is_none()).count();

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
            let mut part_positions =
                split_space(part_position_and_size, part, total_space_to_split);
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
#[path = "./unit/layout_test.rs"]
#[cfg(test)]
mod layout_test;
