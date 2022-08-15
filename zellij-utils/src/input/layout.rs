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

use kdl::*;

use std::str::FromStr;
use std::collections::HashMap;

use crate::{
    kdl_children,
    kdl_children_nodes,
    kdl_name,
    kdl_document_name,
    kdl_get_string_entry,
    kdl_get_int_entry,
    kdl_get_child_entry_bool_value,
    kdl_get_child_entry_string_value,
    kdl_get_child,
};

use super::{
    // config::ConfigFromYaml,
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

// #[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
// #[serde(crate = "self::serde")]
// pub enum RunFromYaml {
//     #[serde(rename = "plugin")]
//     Plugin(RunPluginFromYaml),
//     #[serde(rename = "command")]
//     Command(RunCommand),
// }

// #[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
// #[serde(crate = "self::serde")]
// pub struct RunPluginFromYaml {
//     #[serde(default)]
//     pub _allow_exec_host_cmd: bool,
//     pub location: Url,
// }

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct RunPlugin {
    #[serde(default)]
    pub _allow_exec_host_cmd: bool,
    pub location: RunPluginLocation,
}

impl RunPlugin {
    pub fn from_kdl(kdl_node: &KdlNode) -> Result<Self, ConfigError> {
        let _allow_exec_host_cmd = kdl_get_child_entry_bool_value!(kdl_node, "_allow_exec_host_cmd").unwrap_or(false);
        let string_url = kdl_get_child_entry_string_value!(kdl_node, "location").ok_or(ConfigError::KdlParsingError("Plugins must have a location".into()))?;
        let url = Url::parse(string_url).map_err(|e| ConfigError::KdlParsingError(format!("Failed to aprse url: {:?}", e)))?;
        let location = RunPluginLocation::try_from(url)?;
        Ok(RunPlugin {
            _allow_exec_host_cmd,
            location,
        })
    }
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

// The layout struct ultimately used to build the layouts.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
pub struct Layout {
    pub direction: SplitDirection,
    #[serde(default)]
    pub pane_name: Option<String>,
    #[serde(default)]
    pub parts: LayoutParts,
    pub split_size: Option<SplitSize>,
    pub run: Option<Run>,
    #[serde(default)]
    pub borderless: bool,
    pub focus: Option<bool>,
    pub tabs_index_in_children: Option<usize>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub enum LayoutParts {
    Tabs(Vec<(String, Layout)>), // String is the tab name
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
}

impl Default for LayoutParts {
    fn default() -> Self {
        LayoutParts::Panes(vec![])
    }
}

// The struct that is used to deserialize the layout from
// a yaml configuration file, is needed because of:
// https://github.com/bincode-org/bincode/issues/245
// flattened fields don't retain size information.
// #[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
// #[serde(crate = "self::serde")]
// #[serde(default)]
// pub struct LayoutFromYamlIntermediate {
//     #[serde(default)]
//     pub template: LayoutTemplate,
//     #[serde(default)]
//     pub borderless: bool,
//     #[serde(default)]
//     pub tabs: Vec<TabLayout>,
//     #[serde(default)]
//     pub session: SessionFromYaml,
//     #[serde(flatten)]
//     pub config: Option<ConfigFromYaml>,
// }

// // The struct that is used to deserialize the layout from
// // a yaml configuration file
// #[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
// #[serde(crate = "self::serde")]
// #[serde(default)]
// pub struct LayoutFromYaml {
//     #[serde(default)]
//     pub session: SessionFromYaml,
//     #[serde(default)]
//     pub template: LayoutTemplate,
//     #[serde(default)]
//     pub borderless: bool,
//     #[serde(default)]
//     pub tabs: Vec<TabLayout>,
// }

// type LayoutFromYamlIntermediateResult = Result<LayoutFromYamlIntermediate, ConfigError>;

// impl LayoutFromYamlIntermediate {
//     pub fn from_path(layout_path: &Path) -> LayoutFromYamlIntermediateResult {
//         let mut layout_file = File::open(&layout_path)
//             .or_else(|_| File::open(&layout_path.with_extension("yaml")))
//             .map_err(|e| ConfigError::IoPath(e, layout_path.into()))?;
//
//         let mut layout = String::new();
//         layout_file.read_to_string(&mut layout)?;
//         let layout: Option<LayoutFromYamlIntermediate> = match serde_yaml::from_str(&layout) {
//             Err(e) => {
//                 // needs direct check, as `[ErrorImpl]` is private
//                 // https://github.com/dtolnay/serde-yaml/issues/121
//                 if layout.is_empty() {
//                     return Ok(LayoutFromYamlIntermediate::default());
//                 }
//                 return Err(ConfigError::Serde(e));
//             },
//             Ok(config) => config,
//         };
//
//         match layout {
//             Some(layout) => {
//                 for tab in layout.tabs.clone() {
//                     tab.check()?;
//                 }
//                 Ok(layout)
//             },
//             None => Ok(LayoutFromYamlIntermediate::default()),
//         }
//     }
//
//     pub fn from_yaml(yaml: &str) -> LayoutFromYamlIntermediateResult {
//         let layout: LayoutFromYamlIntermediate = match serde_yaml::from_str(yaml) {
//             Err(e) => {
//                 // needs direct check, as `[ErrorImpl]` is private
//                 // https://github.com/dtolnay/serde-yaml/issues/121
//                 if yaml.is_empty() {
//                     return Ok(LayoutFromYamlIntermediate::default());
//                 }
//                 return Err(ConfigError::Serde(e));
//             },
//             Ok(config) => config,
//         };
//         Ok(layout)
//     }
//
//     pub fn to_layout_and_config(&self) -> (LayoutFromYaml, Option<ConfigFromYaml>) {
//         let config = self.config.clone();
//         let layout = self.clone().into();
//         (layout, config)
//     }
//
//     pub fn from_path_or_default(
//         layout: Option<&PathBuf>,
//         layout_dir: Option<PathBuf>,
//     ) -> Option<LayoutFromYamlIntermediateResult> {
//         layout
//             .map(|layout| {
//                 // The way we determine where to look for the layout is similar to
//                 // how a path would look for an executable.
//                 // See the gh issue for more: https://github.com/zellij-org/zellij/issues/1412#issuecomment-1131559720
//                 if layout.extension().is_some() || layout.components().count() > 1 {
//                     // We look localy!
//                     LayoutFromYamlIntermediate::from_path(layout)
//                 } else {
//                     // We look in the default dir
//                     LayoutFromYamlIntermediate::from_dir(layout, layout_dir.as_ref())
//                 }
//             })
//             .or_else(|| {
//                 Some(LayoutFromYamlIntermediate::from_dir(
//                     &std::path::PathBuf::from("default"),
//                     layout_dir.as_ref(),
//                 ))
//             })
//     }
//
//     // It wants to use Path here, but that doesn't compile.
//     #[allow(clippy::ptr_arg)]
//     pub fn from_dir(
//         layout: &PathBuf,
//         layout_dir: Option<&PathBuf>,
//     ) -> LayoutFromYamlIntermediateResult {
//         match layout_dir {
//             Some(dir) => {
//                 let layout_path = &dir.join(layout);
//                 if layout_path.with_extension("yaml").exists() {
//                     Self::from_path(layout_path)
//                 } else {
//                     LayoutFromYamlIntermediate::from_default_assets(layout)
//                 }
//             },
//             None => LayoutFromYamlIntermediate::from_default_assets(layout),
//         }
//     }
//     // Currently still needed but on nightly
//     // this is already possible:
//     // HashMap<&'static str, Vec<u8>>
//     pub fn from_default_assets(path: &Path) -> LayoutFromYamlIntermediateResult {
//         match path.to_str() {
//             Some("default") => Self::default_from_assets(),
//             Some("strider") => Self::strider_from_assets(),
//             Some("disable-status-bar") => Self::disable_status_from_assets(),
//             Some("compact") => Self::compact_from_assets(),
//             None | Some(_) => Err(ConfigError::IoPath(
//                 std::io::Error::new(std::io::ErrorKind::Other, "The layout was not found"),
//                 path.into(),
//             )),
//         }
//     }
//
//     // TODO Deserialize the assets from bytes &[u8],
//     // once serde-yaml supports zero-copy
//     pub fn default_from_assets() -> LayoutFromYamlIntermediateResult {
//         let layout: LayoutFromYamlIntermediate =
//             serde_yaml::from_str(&String::from_utf8(setup::DEFAULT_LAYOUT.to_vec())?)?;
//         Ok(layout)
//     }
//
//     pub fn strider_from_assets() -> LayoutFromYamlIntermediateResult {
//         let layout: LayoutFromYamlIntermediate =
//             serde_yaml::from_str(&String::from_utf8(setup::STRIDER_LAYOUT.to_vec())?)?;
//         Ok(layout)
//     }
//
//     pub fn disable_status_from_assets() -> LayoutFromYamlIntermediateResult {
//         let layout: LayoutFromYamlIntermediate =
//             serde_yaml::from_str(&String::from_utf8(setup::NO_STATUS_LAYOUT.to_vec())?)?;
//         Ok(layout)
//     }
//
//     pub fn compact_from_assets() -> LayoutFromYamlIntermediateResult {
//         let layout: LayoutFromYamlIntermediate =
//             serde_yaml::from_str(&String::from_utf8(setup::COMPACT_BAR_LAYOUT.to_vec())?)?;
//         Ok(layout)
//     }
// }

// type LayoutFromYamlResult = Result<LayoutFromYaml, ConfigError>;

// impl LayoutFromYaml {
//     pub fn new(layout_path: &Path) -> LayoutFromYamlResult {
//         let mut layout_file = File::open(&layout_path)
//             .or_else(|_| File::open(&layout_path.with_extension("yaml")))
//             .map_err(|e| ConfigError::IoPath(e, layout_path.into()))?;
//
//         let mut layout = String::new();
//         layout_file.read_to_string(&mut layout)?;
//         let layout: Option<LayoutFromYaml> = match serde_yaml::from_str(&layout) {
//             Err(e) => {
//                 // needs direct check, as `[ErrorImpl]` is private
//                 // https://github.com/dtolnay/serde-yaml/issues/121
//                 if layout.is_empty() {
//                     return Ok(LayoutFromYaml::default());
//                 }
//                 return Err(ConfigError::Serde(e));
//             },
//             Ok(config) => config,
//         };
//
//         match layout {
//             Some(layout) => {
//                 for tab in layout.tabs.clone() {
//                     tab.check()?;
//                 }
//                 Ok(layout)
//             },
//             None => Ok(LayoutFromYaml::default()),
//         }
//     }
//
//     // It wants to use Path here, but that doesn't compile.
//     #[allow(clippy::ptr_arg)]
//     pub fn from_dir(layout: &PathBuf, layout_dir: Option<&PathBuf>) -> LayoutFromYamlResult {
//         match layout_dir {
//             Some(dir) => {
//                 Self::new(&dir.join(layout)).or_else(|_| Self::from_default_assets(layout))
//             },
//             None => Self::from_default_assets(layout),
//         }
//     }
//
//     pub fn from_path_or_default(
//         layout: Option<&PathBuf>,
//         layout_path: Option<&PathBuf>,
//         layout_dir: Option<PathBuf>,
//     ) -> Option<LayoutFromYamlResult> {
//         layout
//             .map(|p| LayoutFromYaml::from_dir(p, layout_dir.as_ref()))
//             .or_else(|| layout_path.map(|p| LayoutFromYaml::new(p)))
//             .or_else(|| {
//                 Some(LayoutFromYaml::from_dir(
//                     &std::path::PathBuf::from("default"),
//                     layout_dir.as_ref(),
//                 ))
//             })
//     }
//
//     // Currently still needed but on nightly
//     // this is already possible:
//     // HashMap<&'static str, Vec<u8>>
//     pub fn from_default_assets(path: &Path) -> LayoutFromYamlResult {
//         match path.to_str() {
//             Some("default") => Self::default_from_assets(),
//             Some("strider") => Self::strider_from_assets(),
//             Some("disable-status-bar") => Self::disable_status_from_assets(),
//             None | Some(_) => Err(ConfigError::IoPath(
//                 std::io::Error::new(std::io::ErrorKind::Other, "The layout was not found"),
//                 path.into(),
//             )),
//         }
//     }
//
//     // TODO Deserialize the assets from bytes &[u8],
//     // once serde-yaml supports zero-copy
//     pub fn default_from_assets() -> LayoutFromYamlResult {
//         let layout: LayoutFromYaml =
//             serde_yaml::from_str(&String::from_utf8(setup::DEFAULT_LAYOUT.to_vec())?)?;
//         Ok(layout)
//     }
//
//     pub fn strider_from_assets() -> LayoutFromYamlResult {
//         let layout: LayoutFromYaml =
//             serde_yaml::from_str(&String::from_utf8(setup::STRIDER_LAYOUT.to_vec())?)?;
//         Ok(layout)
//     }
//
//     pub fn disable_status_from_assets() -> LayoutFromYamlResult {
//         let layout: LayoutFromYaml =
//             serde_yaml::from_str(&String::from_utf8(setup::NO_STATUS_LAYOUT.to_vec())?)?;
//         Ok(layout)
//     }
// }

// // The struct that is used to deserialize the session from
// // a yaml configuration file
// #[derive(Debug, Default, Serialize, Deserialize, Clone, PartialEq)]
// #[serde(crate = "self::serde")]
// pub struct SessionFromYaml {
//     pub name: Option<String>,
//     #[serde(default = "default_as_some_true")]
//     pub attach: Option<bool>,
// }
//
// fn default_as_some_true() -> Option<bool> {
//     Some(true)
// }

// // The struct that carries the information template that is used to
// // construct the layout
// #[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
// #[serde(crate = "self::serde")]
// pub struct LayoutTemplate {
//     pub direction: SplitDirection,
//     #[serde(default)]
//     pub pane_name: Option<String>,
//     #[serde(default)]
//     pub borderless: bool,
//     #[serde(default)]
//     pub parts: Vec<LayoutTemplate>,
//     #[serde(default)]
//     pub body: bool,
//     pub split_size: Option<SplitSize>,
//     pub focus: Option<bool>,
//     pub run: Option<RunFromYaml>,
// }

// impl LayoutTemplate {
//     // Insert an optional `[TabLayout]` at the correct position
//     pub fn insert_tab_layout(mut self, tab_layout: Option<TabLayout>) -> Self {
//         if self.body {
//             return tab_layout.unwrap_or_default().into();
//         }
//         for (i, part) in self.parts.clone().iter().enumerate() {
//             if part.body {
//                 self.parts.push(tab_layout.unwrap_or_default().into());
//                 self.parts.swap_remove(i);
//                 break;
//             }
//             // recurse
//             let new_part = part.clone().insert_tab_layout(tab_layout.clone());
//             self.parts.push(new_part);
//             self.parts.swap_remove(i);
//         }
//         self
//     }
//
//     fn from_vec_tab_layout(tab_layout: Vec<TabLayout>) -> Vec<Self> {
//         tab_layout
//             .iter()
//             .map(|tab_layout| Self::from(tab_layout.to_owned()))
//             .collect()
//     }
// }
//
// // The tab-layout struct used to specify each individual tab.
// #[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
// #[serde(crate = "self::serde")]
// pub struct TabLayout {
//     #[serde(default)]
//     pub direction: SplitDirection,
//     pub pane_name: Option<String>,
//     #[serde(default)]
//     pub borderless: bool,
//     #[serde(default)]
//     pub parts: Vec<TabLayout>,
//     pub split_size: Option<SplitSize>,
//     #[serde(default)]
//     pub name: String,
//     pub focus: Option<bool>,
//     pub run: Option<RunFromYaml>,
// }
//
// impl TabLayout {
//     fn check(&self) -> Result<TabLayout, ConfigError> {
//         for part in &self.parts {
//             part.check()?;
//             if !part.name.is_empty() {
//                 return Err(ConfigError::LayoutNameInTab(LayoutNameInTabError));
//             }
//         }
//         Ok(self.clone())
//     }
// }

impl Layout {
    pub fn with_one_pane() -> Self {
        let mut default_layout = Layout::default();
        default_layout.parts = LayoutParts::Panes(vec![Layout::default()]);
        default_layout
    }
    pub fn from_path_or_default(layout_path: Option<&PathBuf>, layout_dir: Option<PathBuf>) -> Result<Self, ConfigError> {
        match layout_path {
            Some(layout_path) => {
                // The way we determine where to look for the layout is similar to
                // how a path would look for an executable.
                // See the gh issue for more: https://github.com/zellij-org/zellij/issues/1412#issuecomment-1131559720
                if layout_path.extension().is_some() || layout_path.components().count() > 1 {
                    // We look localy!
                    Layout::from_path(layout_path)
                } else {
                    // We look in the default dir
                    Layout::from_dir(layout_path, layout_dir.as_ref())
                }
            },
            None => {
                Layout::from_dir(
                    &std::path::PathBuf::from("default"),
                    layout_dir.as_ref(),
                )
            }
        }
    }
    pub fn from_dir(
        layout: &PathBuf,
        layout_dir: Option<&PathBuf>,
    ) -> Result<Self, ConfigError> {
        match layout_dir {
            Some(dir) => {
                let layout_path = &dir.join(layout);
                if layout_path.with_extension("kdl").exists() {
                    Self::from_path(layout_path)
                } else {
                    Layout::from_default_assets(layout)
                }
            },
            None => Layout::from_default_assets(layout),
        }
    }
    pub fn from_path(layout_path: &Path) -> Result<Self, ConfigError> {
        let mut layout_file = File::open(&layout_path)
            .or_else(|_| File::open(&layout_path.with_extension("kdl")))
            .map_err(|e| ConfigError::IoPath(e, layout_path.into()))?;

        let mut kdl_layout = String::new();
        layout_file.read_to_string(&mut kdl_layout)?;
        let kdl_layout: KdlDocument = kdl_layout.parse()?;
        Layout::from_kdl(&kdl_layout, None)
    }
    pub fn from_kdl(kdl_layout: &KdlDocument, direction: Option<SplitDirection>) -> Result<Self, ConfigError> {
        let mut tabs = vec![];
        let layout_node = kdl_layout.nodes().iter().find(|n| kdl_name!(n) == "layout").ok_or(ConfigError::KdlParsingError("No layout found".into()))?;
        fn parse_kdl_layout (kdl_layout: &KdlNode, direction: Option<SplitDirection>, tabs: &mut Vec<Layout>) -> Result<Layout, ConfigError> {
            let borderless: bool = kdl_get_child_entry_bool_value!(kdl_layout, "borderless").unwrap_or(false);
            let focus = kdl_get_child_entry_bool_value!(kdl_layout, "focus");
            let pane_name = kdl_get_child_entry_string_value!(kdl_layout, "name");
            let direction = direction.unwrap_or_default();
            let mut split_size = None;
            if let Some(string_split_size) = kdl_get_string_entry!(kdl_layout, "size") {
                // "10%" => SplitSize::Percent(10) or 10 => SplitSize::Fixed(10)
                split_size = Some(SplitSize::from_str(string_split_size)?);
            }
            if let Some(int_split_size) = kdl_get_int_entry!(kdl_layout, "size") {
                split_size = Some(SplitSize::Fixed(int_split_size as usize));
            }
            let mut run = None;
            if let Some(kdl_command_block) = kdl_get_child!(kdl_layout, "command") {
                run = Some(Run::Command(RunCommand::from_kdl(kdl_command_block)?));
            }
            if let Some(kdl_plugin_block) = kdl_get_child!(kdl_layout, "plugin") {
                if run.is_some() {
                    return Err(ConfigError::KdlParsingError("Cannot have both a command and a plugin block for a single pane".into()));
                }
                run = Some(Run::Plugin(RunPlugin::from_kdl(kdl_plugin_block)?));
            }
            let mut layout_parts = vec![];
            let mut tabs_index_in_children = None;
            if let Some(kdl_parts) = kdl_get_child!(kdl_layout, "parts") {
                let direction = kdl_get_string_entry!(kdl_parts, "direction").ok_or(ConfigError::KdlParsingError("no direction found for layout part".into()))?;
                let direction = SplitDirection::from_str(direction)?;
                // let mut parts: Vec<Layout> = vec![];
                // if let Some(children) = kdl_children_nodes!(kdl_layout) {
                if let Some(children) = kdl_children_nodes!(kdl_parts) {
                    for (i, child) in children.iter().enumerate() {
                        let child_name = kdl_name!(child);
                        if child_name == "layout" {
                            layout_parts.push(parse_kdl_layout(&child, Some(direction), tabs)?);
                        } else if child_name == "tabs" {
                            tabs_index_in_children = Some(i);
                            if !tabs.is_empty() {
                                return Err(ConfigError::KdlParsingError(format!("Only one 'tabs' section allowed per layout...")));
                            }
                            match kdl_children_nodes!(child) {
                                Some(children) => {
                                    for child in children {
                                        let tab_layout = parse_kdl_layout(&child, Some(direction), tabs)?;
                                        tabs.push(tab_layout);
                                    }

                                },
                                None => tabs.push(Layout::with_one_pane()),
                            }
                        } else {
                            return Err(ConfigError::KdlParsingError(format!("Unknown layout part: {:?}", child_name)));
                        }
                    }
                }
            }
            Ok(Layout {
                direction,
                pane_name: None, // TODO
                parts: LayoutParts::Panes(layout_parts),
                split_size,
                run,
                borderless,
                focus: None, // TODO
                tabs_index_in_children,
            })
        }
        let mut base_layout = parse_kdl_layout(layout_node, direction, &mut tabs)?;
        if !tabs.is_empty() {
            let mut root_layout = Layout::default();
            let mut tab_parts: Vec<(String, Layout)> = vec![];

            for (i, tab) in tabs.drain(..).enumerate() {
                let tab_name = format!("{}", i); // TODO: support tab name in layout
                let mut layout_for_tab = base_layout.clone();
                layout_for_tab.insert_tab_layout(&tab);
                tab_parts.push((tab_name, layout_for_tab));

            }
            root_layout.parts = LayoutParts::Tabs(tab_parts);
            Ok(root_layout)
        } else {
            Ok(base_layout)
        }

    }
//         let layout: Option<LayoutFromYamlIntermediate> = match serde_yaml::from_str(&layout) {
//             Err(e) => {
//                 // needs direct check, as `[ErrorImpl]` is private
//                 // https://github.com/dtolnay/serde-yaml/issues/121
//                 if layout.is_empty() {
//                     return Ok(Layout::default());
//                 }
//                 return Err(ConfigError::Serde(e));
//             },
//             Ok(config) => config,
//         };
//
//         match layout {
//             Some(layout) => {
//                 for tab in layout.tabs.clone() {
//                     tab.check()?;
//                 }
//                 Ok(layout)
//             },
//             None => Ok(LayoutFromYamlIntermediate::default()),
//         }
//    }

    pub fn from_default_assets(path: &Path) -> Result<Self, ConfigError> {
        // TODO: ideally these should not be hard-coded
        // we should load layouts by name from the config
        // and load them from a hashmap or some such
        match path.to_str() {
            Some("default") => Self::default_from_assets(),
            Some("strider") => Self::strider_from_assets(),
            Some("disable-status-bar") => Self::disable_status_from_assets(),
            Some("compact") => Self::compact_from_assets(),
            None | Some(_) => Err(ConfigError::IoPath(
                std::io::Error::new(std::io::ErrorKind::Other, "The layout was not found"),
                path.into(),
            )),
        }
    }

    pub fn default_from_assets() -> Result<Layout, ConfigError> {
        let kdl_layout = String::from_utf8(setup::DEFAULT_LAYOUT.to_vec())?;
        let kdl_layout: KdlDocument = kdl_layout.parse()?;
        Layout::from_kdl(&kdl_layout, None)
    }

    pub fn strider_from_assets() -> Result<Layout, ConfigError> {
        let kdl_layout = String::from_utf8(setup::STRIDER_LAYOUT.to_vec())?;
        let kdl_layout: KdlDocument = kdl_layout.parse()?;
        Layout::from_kdl(&kdl_layout, None)
    }

    pub fn disable_status_from_assets() -> Result<Layout, ConfigError> {
        let kdl_layout = String::from_utf8(setup::NO_STATUS_LAYOUT.to_vec())?;
        let kdl_layout: KdlDocument = kdl_layout.parse()?;
        Layout::from_kdl(&kdl_layout, None)
    }

    pub fn compact_from_assets() -> Result<Layout, ConfigError> {
        let kdl_layout = String::from_utf8(setup::COMPACT_BAR_LAYOUT.to_vec())?;
        let kdl_layout: KdlDocument = kdl_layout.parse()?;
        Layout::from_kdl(&kdl_layout, None)
    }

    pub fn total_terminal_panes(&self) -> usize {
        // TODO: better
        let mut total_panes = 0;
        match &self.parts {
            LayoutParts::Panes(parts) => {
                total_panes += parts.len();
                for part in parts {
                    match part.run {
                        Some(Run::Command(_)) | None => {
                            total_panes += part.total_terminal_panes();
                        },
                        Some(Run::Plugin(_)) => {},
                    }
                }
                total_panes
            },
            LayoutParts::Tabs(tabs) => {
                // let parts = tabs.values();
                total_panes += tabs.len();
                for tab in tabs {
                    let (_tab_name, part) = tab;
                    match part.run {
                        Some(Run::Command(_)) | None => {
                            total_panes += part.total_terminal_panes();
                        },
                        Some(Run::Plugin(_)) => {},
                    }
                }
                total_panes
            }
        }
    }

    pub fn total_borderless_panes(&self) -> usize {
        // TODO: better
        let mut total_borderless_panes = 0;
        match &self.parts {
            LayoutParts::Panes(parts) => {
                total_borderless_panes += parts.iter().filter(|p| p.borderless).count();
                for part in parts {
                    total_borderless_panes += part.total_borderless_panes();
                }
                total_borderless_panes
            },
            LayoutParts::Tabs(tabs) => {
                // let parts = tabs.values();
                total_borderless_panes += tabs.iter().filter(|(_, p)| p.borderless).count();
                for part in tabs {
                    let (_part_name, part) = part;
                    total_borderless_panes += part.total_borderless_panes();
                }
                total_borderless_panes
            }
        }
    }
    pub fn extract_run_instructions(&self) -> Vec<Option<Run>> {
        // TODO: better
        let mut run_instructions = vec![];
        match &self.parts {
            LayoutParts::Panes(parts) => {
                if parts.is_empty() {
                    run_instructions.push(self.run.clone());
                }
                for part in parts {
                    let mut current_runnables = part.extract_run_instructions();
                    run_instructions.append(&mut current_runnables);
                }
            },
            LayoutParts::Tabs(tabs) => {
                if tabs.len() == 0 {
                    run_instructions.push(self.run.clone());
                }
                for tab in tabs {
                    let (_part_name, part) = tab;
                    let mut current_runnables = part.extract_run_instructions();
                    run_instructions.append(&mut current_runnables);
                }
            }
        }
        run_instructions
    }

    pub fn position_panes_in_space(&self, space: &PaneGeom) -> Vec<(Layout, PaneGeom)> {
        split_space(space, self)
    }

    pub fn merge_layout_parts(&mut self, mut parts: Vec<Layout>) {
        // TODO
        unimplemented!()
        // self.parts.append(&mut parts);
    }

    pub fn insert_tab_layout(&mut self, tab_layout: &Layout) -> Result<(), &'static str> {
        match self.tabs_index_in_children {
            Some(tabs_index_in_children) => {
                match &mut self.parts {
                    LayoutParts::Panes(panes) => {
                        panes.insert(tabs_index_in_children, tab_layout.clone());
                        Ok(())
                    },
                    LayoutParts::Tabs(_) => {
                        Err("Only top layout part can have a tabs block")
                    }
                }
            },
            None => {
                match &mut self.parts {
                    LayoutParts::Panes(panes) => {
                        for child in panes.iter_mut() {
                            if let Ok(_) = child.insert_tab_layout(tab_layout) {
                                return Ok(());
                            }
                        }
                        Err("no place to insert tabs here")
                    },
                    LayoutParts::Tabs(_) => {
                        Err("Only top layout part can have a tabs block")
                    }
                }
            }
        }
    }

    pub fn has_tabs(&self) -> bool {
        // TODO: CONTINUE HERE (15/08) - implement these, then test with:
        // - cargo make build && target/debug/zellij
        unimplemented!()
    }

    pub fn tabs(&self) -> Vec<(Layout, String)> { // String is the tab name
        unimplemented!()
    }

    pub fn focused_tab_index(&self) -> Option<usize> {
        unimplemented!()
    }

//     fn from_vec_tab_layout(tab_layout: Vec<TabLayout>) -> Result<Vec<Self>, ConfigError> {
//         tab_layout
//             .iter()
//             .map(|tab_layout| Layout::try_from(tab_layout.to_owned()))
//             .collect()
//     }

//     fn from_vec_template_layout(
//         layout_template: Vec<LayoutTemplate>,
//     ) -> Result<Vec<Self>, ConfigError> {
//         layout_template
//             .iter()
//             .map(|layout_template| Layout::try_from(layout_template.to_owned()))
//             .collect()
//     }
}

fn layout_size(direction: SplitDirection, layout: &Layout) -> usize {
    fn child_layout_size(
        direction: SplitDirection,
        parent_direction: SplitDirection,
        layout: &Layout,
    ) -> usize {
        let size = if parent_direction == direction { 1 } else { 0 };
        let parts_is_empty = match &layout.parts {
            LayoutParts::Panes(parts) => parts.is_empty(),
            LayoutParts::Tabs(tabs) => tabs.len() == 0
        };
        // if layout.parts.is_empty() {
        if parts_is_empty {
            size
        } else {
            match &layout.parts {
                LayoutParts::Panes(parts) => {
                    let children_size = parts
                        .iter()
                        .map(|p| child_layout_size(direction, layout.direction, p))
                        .sum();
                    max(size, children_size)
                },
                LayoutParts::Tabs(tabs) => {
                    let children_size = tabs
                        .iter()
                        .map(|(_, p)| child_layout_size(direction, layout.direction, p))
                        .sum();
                    max(size, children_size)
                }
            }
        }
    }
    child_layout_size(direction, direction, layout)
}

fn split_space(space_to_split: &PaneGeom, layout: &Layout) -> Vec<(Layout, PaneGeom)> {
    match &layout.parts {
        LayoutParts::Panes(parts) => {
            let mut pane_positions = Vec::new();
            let sizes: Vec<Option<SplitSize>> = parts.iter().map(|part| part.split_size).collect();

            let mut split_geom = Vec::new();
            let (mut current_position, split_dimension_space, mut inherited_dimension) =
                match layout.direction {
                    SplitDirection::Vertical => (space_to_split.x, space_to_split.cols, space_to_split.rows),
                    SplitDirection::Horizontal => (space_to_split.y, space_to_split.rows, space_to_split.cols),
                };

            let flex_parts = sizes.iter().filter(|s| s.is_none()).count();

            for (&size, part) in sizes.iter().zip(&*parts) {
                let split_dimension = match size {
                    Some(SplitSize::Percent(percent)) => Dimension::percent(percent as f64),
                    Some(SplitSize::Fixed(size)) => Dimension::fixed(size),
                    None => {
                        let free_percent = if let Some(p) = split_dimension_space.as_percent() {
                            p - sizes
                                .iter()
                                .map(|&s| {
                                    if let Some(SplitSize::Percent(ip)) = s {
                                        ip as f64
                                    } else {
                                        0.0
                                    }
                                })
                                .sum::<f64>()
                        } else {
                            panic!("Implicit sizing within fixed-size panes is not supported");
                        };
                        Dimension::percent(free_percent / flex_parts as f64)
                    },
                };
                inherited_dimension.set_inner(
                    parts
                        .iter()
                        .map(|p| layout_size(!layout.direction, p))
                        .max()
                        .unwrap(),
                );
                let geom = match layout.direction {
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
                current_position += layout_size(layout.direction, part);
            }

            for (i, part) in parts.iter().enumerate() {
                let part_position_and_size = split_geom.get(i).unwrap();
                if !part.parts.is_empty() {
                    let mut part_positions = split_space(part_position_and_size, part);
                    pane_positions.append(&mut part_positions);
                } else {
                    pane_positions.push((part.clone(), *part_position_and_size));
                }
            }
            pane_positions
        },
        LayoutParts::Tabs(tabs) => {
            // TODO
            unimplemented!()
        }
    }
//     let mut pane_positions = Vec::new();
//     // let sizes: Vec<Option<SplitSize>> = layout.parts.iter().map(|part| part.split_size).collect();
//     let sizes: Vec<Option<SplitSize>> = match layout.parts {
//         LayoutParts::Panes(parts) => parts.iter().map(|part| part.split_size).collect(),
//         LayoutParts::Tabs(tabs) => tabs.values().map(|part| part.split_size).collect(),
//     };
//
//     let mut split_geom = Vec::new();
//     let (mut current_position, split_dimension_space, mut inherited_dimension) =
//         match layout.direction {
//             SplitDirection::Vertical => (space_to_split.x, space_to_split.cols, space_to_split.rows),
//             SplitDirection::Horizontal => (space_to_split.y, space_to_split.rows, space_to_split.cols),
//         };
//
//     let flex_parts = sizes.iter().filter(|s| s.is_none()).count();
//
//     let parts = match layout.parts {
//         LayoutParts::Panes(parts)
//     }
//     for (&size, part) in sizes.iter().zip(&layout.parts) {
//         let split_dimension = match size {
//             Some(SplitSize::Percent(percent)) => Dimension::percent(percent),
//             Some(SplitSize::Fixed(size)) => Dimension::fixed(size),
//             None => {
//                 let free_percent = if let Some(p) = split_dimension_space.as_percent() {
//                     p - sizes
//                         .iter()
//                         .map(|&s| {
//                             if let Some(SplitSize::Percent(ip)) = s {
//                                 ip
//                             } else {
//                                 0.0
//                             }
//                         })
//                         .sum::<f64>()
//                 } else {
//                     panic!("Implicit sizing within fixed-size panes is not supported");
//                 };
//                 Dimension::percent(free_percent / flex_parts as f64)
//             },
//         };
//         inherited_dimension.set_inner(
//             layout
//                 .parts
//                 .iter()
//                 .map(|p| layout_size(!layout.direction, p))
//                 .max()
//                 .unwrap(),
//         );
//         let geom = match layout.direction {
//             SplitDirection::Vertical => PaneGeom {
//                 x: current_position,
//                 y: space_to_split.y,
//                 cols: split_dimension,
//                 rows: inherited_dimension,
//             },
//             SplitDirection::Horizontal => PaneGeom {
//                 x: space_to_split.x,
//                 y: current_position,
//                 cols: inherited_dimension,
//                 rows: split_dimension,
//             },
//         };
//         split_geom.push(geom);
//         current_position += layout_size(layout.direction, part);
//     }
//
//     for (i, part) in layout.parts.iter().enumerate() {
//         let part_position_and_size = split_geom.get(i).unwrap();
//         if !part.parts.is_empty() {
//             let mut part_positions = split_space(part_position_and_size, part);
//             pane_positions.append(&mut part_positions);
//         } else {
//             pane_positions.push((part.clone(), *part_position_and_size));
//         }
//     }
//     pane_positions
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
            },
            _ => Err(PluginsConfigError::InvalidUrl(url)),
        }
    }
}

// impl TryFrom<RunFromYaml> for Run {
//     type Error = PluginsConfigError;
//
//     fn try_from(run: RunFromYaml) -> Result<Self, Self::Error> {
//         match run {
//             RunFromYaml::Command(command) => Ok(Run::Command(command)),
//             RunFromYaml::Plugin(plugin) => Ok(Run::Plugin(RunPlugin {
//                 _allow_exec_host_cmd: plugin._allow_exec_host_cmd,
//                 location: plugin.location.try_into()?,
//             })),
//         }
//     }
// }
//
// impl From<LayoutFromYamlIntermediate> for LayoutFromYaml {
//     fn from(layout_from_yaml_intermediate: LayoutFromYamlIntermediate) -> Self {
//         Self {
//             template: layout_from_yaml_intermediate.template,
//             borderless: layout_from_yaml_intermediate.borderless,
//             tabs: layout_from_yaml_intermediate.tabs,
//             session: layout_from_yaml_intermediate.session,
//         }
//     }
// }
//
// impl From<LayoutFromYaml> for LayoutFromYamlIntermediate {
//     fn from(layout_from_yaml: LayoutFromYaml) -> Self {
//         Self {
//             template: layout_from_yaml.template,
//             borderless: layout_from_yaml.borderless,
//             tabs: layout_from_yaml.tabs,
//             config: None,
//             session: layout_from_yaml.session,
//         }
//     }
// }
//
// impl Default for LayoutFromYamlIntermediate {
//     fn default() -> Self {
//         LayoutFromYaml::default().into()
//     }
// }
//
// impl TryFrom<TabLayout> for Layout {
//     type Error = ConfigError;
//
//     fn try_from(tab: TabLayout) -> Result<Self, Self::Error> {
//         Ok(Layout {
//             direction: tab.direction,
//             pane_name: tab.pane_name,
//             borderless: tab.borderless,
//             parts: LayoutParts::Panes(Self::from_vec_tab_layout(tab.parts)?),
//             split_size: tab.split_size,
//             focus: tab.focus,
//             run: tab.run.map(Run::try_from).transpose()?,
//             tabs_index_in_children: None,
//         })
//     }
// }
//
// impl From<TabLayout> for LayoutTemplate {
//     fn from(tab: TabLayout) -> Self {
//         Self {
//             direction: tab.direction,
//             pane_name: tab.pane_name,
//             borderless: tab.borderless,
//             parts: Self::from_vec_tab_layout(tab.parts),
//             body: false,
//             split_size: tab.split_size,
//             focus: tab.focus,
//             run: tab.run,
//         }
//     }
// }
//
// impl TryFrom<LayoutTemplate> for Layout {
//     type Error = ConfigError;
//
//     fn try_from(template: LayoutTemplate) -> Result<Self, Self::Error> {
//         Ok(Layout {
//             direction: template.direction,
//             pane_name: template.pane_name,
//             borderless: template.borderless,
//             parts: LayoutParts::Panes(Self::from_vec_template_layout(template.parts)?),
//             split_size: template.split_size,
//             focus: template.focus,
//             run: template
//                 .run
//                 .map(Run::try_from)
//                 // FIXME: This is just Result::transpose but that method is unstable, when it
//                 // stabalizes we should swap this out.
//                 .map_or(Ok(None), |r| r.map(Some))?,
//             tabs_index_in_children: None,
//         })
//     }
// }
//
// impl Default for TabLayout {
//     fn default() -> Self {
//         Self {
//             direction: SplitDirection::Horizontal,
//             borderless: false,
//             parts: vec![],
//             split_size: None,
//             run: None,
//             name: String::new(),
//             pane_name: None,
//             focus: None,
//         }
//     }
// }
//
// impl Default for LayoutTemplate {
//     fn default() -> Self {
//         Self {
//             direction: SplitDirection::Horizontal,
//             pane_name: None,
//             body: false,
//             borderless: false,
//             parts: vec![LayoutTemplate {
//                 direction: SplitDirection::Horizontal,
//                 pane_name: None,
//                 body: true,
//                 borderless: false,
//                 split_size: None,
//                 focus: None,
//                 run: None,
//                 parts: vec![],
//             }],
//             split_size: None,
//             focus: None,
//             run: None,
//         }
//     }
// }

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
            let percent_size = usize::from_str_radix(&s[..char_count], 10)?;
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
