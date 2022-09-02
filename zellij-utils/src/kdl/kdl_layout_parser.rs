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
        layout::{Layout, PaneLayout, LayoutParts, SplitDirection, Run, RunPlugin, RunPluginLocation, SplitSize},
        plugins::{PluginTag, PluginsConfigError},
    },
    pane_size::{Dimension, PaneGeom},
    setup,
};

use kdl::*;

use std::str::FromStr;
use std::collections::{HashMap, HashSet};

use crate::{
    kdl_children,
    kdl_string_arguments,
    kdl_children_nodes,
    kdl_name,
    kdl_document_name,
    kdl_get_string_entry,
    kdl_get_int_entry,
    kdl_get_child_entry_bool_value,
    kdl_get_child_entry_string_value,
    kdl_get_child,
    kdl_get_bool_property_or_child_value,
    kdl_get_string_property_or_child_value,
    kdl_get_int_property_or_child_value,
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

pub struct KdlLayoutParser <'a>{
    kdl_layout: &'a KdlDocument,
    tab_templates: HashMap<String, PaneLayout>,
    pane_templates: HashMap<String, PaneLayout>,
    default_tab_template: Option<PaneLayout>,
}

impl <'a>KdlLayoutParser <'a> {
    pub fn new(kdl_layout: &'a KdlDocument) -> Self {
        KdlLayoutParser {
            kdl_layout,
            tab_templates: HashMap::new(),
            pane_templates: HashMap::new(),
            default_tab_template: None,
        }
    }
    fn is_a_reserved_word(&self, word: &str) -> bool {
        word == "pane" ||
        word == "layout" ||
        word == "pane_template" ||
        word == "tab_template" ||
        word == "default_tab_template" ||
        word == "command" ||
        word == "plugin" ||
        word == "children" ||
        word == "tab"
    }
    fn assert_legal_node_name(&self, name: &str) -> Result<(), ConfigError> {
        if name.contains(char::is_whitespace) {
            Err(ConfigError::KdlParsingError(format!("Node names ({}) cannot contain whitespace.", name)))
        } else if self.is_a_reserved_word(&name) {
            Err(ConfigError::KdlParsingError(format!("Node name '{}' is a reserved word.", name)))
        } else {
            Ok(())
        }
    }
    fn parse_split_size(&self, kdl_node: &KdlNode) -> Result<Option<SplitSize>, ConfigError> {
        if let Some(size) = kdl_get_string_property_or_child_value!(kdl_node, "size") {
            Ok(Some(SplitSize::from_str(size)?))
        } else if let Some(size) = kdl_get_int_property_or_child_value!(kdl_node, "size") {
            Ok(Some(SplitSize::Fixed(size as usize)))
        } else {
            Ok(None)
        }
    }
    fn parse_plugin_block(&self, plugin_block: &KdlNode) -> Result<Option<Run>, ConfigError> {
        let _allow_exec_host_cmd = kdl_get_bool_property_or_child_value!(plugin_block, "_allow_exec_host_cmd").unwrap_or(false);
        let string_url = kdl_get_string_property_or_child_value!(plugin_block, "location").ok_or(ConfigError::KdlParsingError("Plugins must have a location".into()))?;
        let url = Url::parse(string_url).map_err(|e| ConfigError::KdlParsingError(format!("Failed to aprse url: {:?}", e)))?;
        let location = RunPluginLocation::try_from(url)?;
        Ok(Some(Run::Plugin(RunPlugin {
            _allow_exec_host_cmd,
            location
        })))
    }
    fn parse_pane_command(&self, pane_node: &KdlNode) -> Result<Option<Run>, ConfigError> {
        let command = kdl_get_string_property_or_child_value!(pane_node, "command").map(|c| PathBuf::from(c));
        let cwd = kdl_get_string_property_or_child_value!(pane_node, "cwd").map(|c| PathBuf::from(c));
        let args = match kdl_get_child!(pane_node, "args") {
            Some(kdl_args) => Some(kdl_string_arguments!(kdl_args).iter().map(|s| String::from(*s)).collect()),
            None => None,
        };
        match (command, cwd, args) {
            (None, Some(_cwd), _) => {
                Err(ConfigError::KdlParsingError("Cwd can only be set if a command was specified".into()))
            }
            (None, _, Some(_args)) => {
                Err(ConfigError::KdlParsingError("Args can only be set if a command was specified".into()))
            }
            (Some(command), cwd, args) => {
                Ok(Some(Run::Command(RunCommand {
                    command,
                    args: args.unwrap_or_else(|| vec![]),
                    cwd
                })))
            }
            _ => Ok(None)
        }
    }
    fn parse_command_or_plugin_block(&self, kdl_node: &KdlNode) -> Result<Option<Run>, ConfigError> {
        let mut run = self.parse_pane_command(kdl_node)?;
        if let Some(plugin_block) = kdl_get_child!(kdl_node, "plugin") {
            if run.is_some() {
                return Err(ConfigError::KdlParsingError("Cannot have both a command and a plugin block for a single pane".into()));
            }
            run = self.parse_plugin_block(plugin_block)?;
        }
        Ok(run)
    }
    fn parse_pane_node(&self, kdl_node: &KdlNode) -> Result<PaneLayout, ConfigError> {
        let borderless = kdl_get_bool_property_or_child_value!(kdl_node, "borderless");
        let focus = kdl_get_bool_property_or_child_value!(kdl_node, "focus");
        let pane_name = kdl_get_string_property_or_child_value!(kdl_node, "name").map(|name| name.to_string());
        let split_size = self.parse_split_size(kdl_node)?;
        let run = self.parse_command_or_plugin_block(kdl_node)?;
        let direction = self.parse_split_direction(kdl_node)?;
        let (external_children_index, pane_parts) = match kdl_children_nodes!(kdl_node) {
            Some(children) => self.parse_child_pane_nodes_for_pane(&children)?,
            None => (None, vec![])
        };
        Ok(Layout {
            borderless: borderless.unwrap_or_default(),
            focus,
            pane_name,
            split_size,
            run,
            direction,
            external_children_index,
            parts: LayoutParts::Panes(pane_parts),
            ..Default::default()
        })
    }
    fn parse_pane_node_with_template(&self, kdl_node: &KdlNode, mut pane_layout: PaneLayout) -> Result<PaneLayout, ConfigError> {
        let direction = self.parse_split_direction(kdl_node)?;
        match kdl_children_nodes!(kdl_node) {
            Some(children) => {
                let (_, mut child_panes) = self.parse_child_pane_nodes_for_pane(&children)?;
                if child_panes.is_empty() {
                    child_panes.push(Layout::default());
                }
                let child_panes_layout = Layout {
                    direction,
                    parts: LayoutParts::Panes(child_panes),
                    ..Default::default()
                };
                self.assert_one_children_block(&pane_layout)?;
                self.insert_layout_children_or_error(&mut pane_layout, child_panes_layout)?;
            },
            None => {
                if let Some(index_of_children) = pane_layout.external_children_index {
                    pane_layout.parts.insert_pane(index_of_children, Layout::default())?;
                }
            }
        }
        pane_layout.external_children_index = None;
        Ok(pane_layout)
    }
    fn parse_split_direction(&self, kdl_node: &KdlNode) -> Result<SplitDirection, ConfigError> {
        Ok(match kdl_get_string_entry!(kdl_node, "split_direction") {
            Some(direction) => SplitDirection::from_str(direction)?,
            None => SplitDirection::default(),
        })
    }
    fn parse_pane_template_node(&mut self, kdl_node: &KdlNode) -> Result<(), ConfigError> { // String is the tab name
        let template_name = kdl_get_string_property_or_child_value!(kdl_node, "name").map(|s| s.to_string()).ok_or(ConfigError::KdlParsingError("Pane templates must have a name".into()))?;
        self.assert_legal_node_name(&template_name)?;
        let borderless = kdl_get_bool_property_or_child_value!(kdl_node, "borderless");
        let focus = kdl_get_bool_property_or_child_value!(kdl_node, "focus");
        let split_size = self.parse_split_size(kdl_node)?;
        let run = self.parse_command_or_plugin_block(kdl_node)?;
        let children_split_direction = self.parse_split_direction(kdl_node)?;
        let (external_children_index, pane_parts) = match kdl_children_nodes!(kdl_node) {
            Some(children) => self.parse_child_pane_nodes_for_pane(&children)?,
            None => (None, vec![])
        };
        self.pane_templates.insert(template_name, PaneLayout {
            borderless: borderless.unwrap_or_default(),
            focus,
            split_size,
            run,
            children_split_direction,
            external_children_index,
            children: pane_parts,
            ..Default::default()
        });
        Ok(())
    }
    fn parse_tab_node(&mut self, kdl_node: &KdlNode) -> Result<(Option<String>, PaneLayout), ConfigError> { // String is the tab name
        match self.default_tab_template.as_ref().map(|t| t.clone()) {
            Some(default_tab_template) => {
                self.parse_tab_node_with_template(kdl_node, default_tab_template)
            },
            None => {
                let tab_name = kdl_get_string_property_or_child_value!(kdl_node, "name").map(|s| s.to_string());
                let children_split_direction = match kdl_get_string_entry!(kdl_node, "split_direction") {
                    Some(direction) => SplitDirection::from_str(direction)?,
                    None => SplitDirection::default(),
                };
                let children = match kdl_children_nodes!(kdl_node) {
                    Some(children) => self.parse_child_pane_nodes_for_tab(children)?,
                    None => vec![],
                };
                Ok((tab_name, PaneLayout {
                    children_split_direction,
                    children,
                    ..Default::default()
                }))
            }
        }
    }
    fn parse_child_pane_nodes_for_tab(&self, children: &[KdlNode]) -> Result<Vec<PaneLayout>, ConfigError> {
        let mut nodes = vec![];
        for child in children {
            if kdl_name!(child) == "pane" {
                nodes.push(self.parse_pane_node(child)?);
            } else if let Some(pane_template) = self.pane_templates.get(kdl_name!(child)).cloned() {
                nodes.push(self.parse_pane_node_with_template(child, pane_template)?);
            }
        }
        if nodes.is_empty() {
            nodes.push(PaneLayout::default());
        }
        Ok(nodes)
    }
    fn parse_child_pane_nodes_for_pane(&self, children: &[KdlNode]) -> Result<(Option<usize>, Vec<PaneLayout>), ConfigError> { // usize is external_children_index
        let mut external_children_index = None;
        let mut nodes = vec![];
        for (i, child) in children.iter().enumerate() {
            if kdl_name!(child) == "pane" {
                nodes.push(self.parse_pane_node(child)?);
            } else if kdl_name!(child) == "children" {
                external_children_index = Some(i);
            } else if let Some(pane_template) = self.pane_templates.get(kdl_name!(child)).cloned() {
                nodes.push(self.parse_pane_node_with_template(child, pane_template)?);
            }
        }
        Ok((external_children_index, nodes))
    }
    fn assert_one_children_block(&self, layout: &PaneLayout) -> Result<(), ConfigError> {
        let children_block_count = layout.children_block_count();
        if children_block_count != 1 {
            return Err(ConfigError::KdlParsingError(format!("Layout has {} children blocks, only 1 is allowed", children_block_count)));
        }
        Ok(())
    }
    fn insert_layout_children_or_error(&self, layout: &mut PaneLayout, mut child_panes_layout: PaneLayout) -> Result<(), ConfigError> {
        let successfully_inserted = layout.insert_children_layout(&mut child_panes_layout)?;
        if !successfully_inserted {
            Err(ConfigError::KdlParsingError("This tab template does not have children".into()))
        } else {
            Ok(())
        }
    }
    fn parse_tab_node_with_template(&mut self, kdl_node: &KdlNode, mut tab_layout: PaneLayout) -> Result<(Option<String>, PaneLayout), ConfigError> { // String is the tab name
        let tab_name = kdl_get_string_property_or_child_value!(kdl_node, "name").map(|s| s.to_string());
        let children_split_direction = match kdl_get_string_entry!(kdl_node, "split_direction") {
            Some(direction) => SplitDirection::from_str(direction)?,
            None => SplitDirection::default(),
        };
        match kdl_children_nodes!(kdl_node) {
            Some(children) => {
                let child_panes = self.parse_child_pane_nodes_for_tab(children)?;
                let child_panes_layout = PaneLayout {
                    children_split_direction,
                    children: child_panes,
                    ..Default::default()
                };
                self.assert_one_children_block(&tab_layout)?;
                self.insert_layout_children_or_error(&mut tab_layout, child_panes_layout)?;
            },
            None => {
                if let Some(index_of_children) = tab_layout.external_children_index {
                    tab_layout.children.insert(index_of_children, PaneLayout::default());
                }
            }
        }
        tab_layout.external_children_index = None;
        Ok((tab_name, tab_layout))
    }
    fn populate_one_tab_template(&mut self, kdl_node: &KdlNode) -> Result<(), ConfigError> { // String is the tab name
        let template_name = kdl_get_string_property_or_child_value!(kdl_node, "name").map(|s| s.to_string()).ok_or(ConfigError::KdlParsingError("Tab templates must have a name".into()))?;
        self.assert_legal_node_name(&template_name)?;
        self.tab_templates.insert(template_name, self.parse_tab_template_node(kdl_node)?);
        Ok(())
    }
    fn populate_default_tab_template(&mut self, kdl_node: &KdlNode) -> Result<(), ConfigError> { // String is the tab name
        self.default_tab_template = Some(self.parse_tab_template_node(kdl_node)?);
        Ok(())
    }
    fn parse_tab_template_node(&self, kdl_node: &KdlNode) -> Result<PaneLayout, ConfigError> {
        let children_split_direction = match kdl_get_string_entry!(kdl_node, "split_direction") {
            Some(direction) => SplitDirection::from_str(direction)?,
            None => SplitDirection::default(),
        };
        let mut tab_children = vec![];
        let mut external_children_index = None;
        if let Some(children) = kdl_children_nodes!(kdl_node) {
            for (i, child) in children.iter().enumerate() {
                if kdl_name!(child) == "pane" {
                    tab_children.push(self.parse_pane_node(child)?);
                } else if kdl_name!(child) == "children" {
                    external_children_index = Some(i);
                } else if let Some(pane_template) = self.pane_templates.get(kdl_name!(child)).cloned() {
                    tab_children.push(self.parse_pane_node_with_template(child, pane_template)?);
                }
            }
        }
        Ok(PaneLayout {
            children_split_direction,
            children: tab_children,
            external_children_index,
            ..Default::default()
        })
    }
    fn default_template(&self) -> Result<Option<PaneLayout>, ConfigError> {
        match &self.default_tab_template {
            Some(template) => {
                let mut template = template.clone();
                if let Some(children_index) = template.external_children_index {
                    template.children.insert(children_index, PaneLayout::default())
                }
                template.external_children_index = None;
                Ok(Some(template))
            },
            None => Ok(None)
        }
    }
    pub fn get_pane_template_dependency_tree(&self, kdl_children: &'a [KdlNode]) -> Result<HashMap<&'a str, HashSet<&'a str>>, ConfigError> {
        let mut dependency_tree = HashMap::new();
        for child in kdl_children {
            if kdl_name!(child) == "pane_template" {
                let template_name = kdl_get_string_property_or_child_value!(child, "name")
                    .ok_or(ConfigError::KdlParsingError("Pane templates must have a name".into()))?;
                let mut template_children = HashSet::new();
                self.get_pane_template_dependencies(child, &mut template_children)?;
                dependency_tree.insert(template_name, template_children);
            }
        }
        Ok(dependency_tree)
    }
    fn get_pane_template_dependencies(&self, node: &'a KdlNode, all_dependencies: &mut HashSet<&'a str>) -> Result<(), ConfigError> {
        if let Some(children) = kdl_children_nodes!(node) {
            for child in children {
                let child_name = kdl_name!(child);
                if child_name == "pane" {
                    self.get_pane_template_dependencies(child, all_dependencies)?;
                } else if !self.is_a_reserved_word(child_name) {
                    all_dependencies.insert(child_name);
                    self.get_pane_template_dependencies(child, all_dependencies)?;
                }
            }
        }
        Ok(())
    }
    pub fn parse_pane_template_by_name(&mut self, pane_template_name: &str, kdl_children: &[KdlNode]) -> Result<(), ConfigError> {
        for child in kdl_children.iter() {
            let child_name = kdl_name!(child);
            if child_name == "pane_template" {
                let child_name = kdl_get_string_property_or_child_value!(child, "name");
                if child_name == Some(pane_template_name) {
                    self.parse_pane_template_node(child)?;
                }
            }
        };
        Ok(())
    }
    fn populate_pane_templates(&mut self, layout_children: &[KdlNode]) -> Result<(), ConfigError> {
        let mut pane_template_dependency_tree = self.get_pane_template_dependency_tree(layout_children)?;
        let mut pane_template_names_to_parse: Vec<&str> = vec![];
        // toposort the dependency tree so that we parse the pane_templates before their
        // dependencies
        while !pane_template_dependency_tree.is_empty() {
            let mut candidates: Vec<&str> = vec![];
            for (pane_tempalte, dependencies) in pane_template_dependency_tree.iter() {
                if dependencies.is_empty() {
                    candidates.push(pane_tempalte);
                }
            }
            if candidates.is_empty() {
                return Err(ConfigError::KdlParsingError("Circular dependency detected between pane templates.".into()));
            }
            for candidate_to_remove in candidates {
                pane_template_dependency_tree.remove(candidate_to_remove);
                for (_pane_tempalte, dependencies) in pane_template_dependency_tree.iter_mut() {
                    dependencies.remove(candidate_to_remove);
                }
                pane_template_names_to_parse.push(candidate_to_remove);
            }
        }
        // once we've toposorted, parse the sorted list in order
        for pane_template_name in pane_template_names_to_parse {
            self.parse_pane_template_by_name(pane_template_name, &layout_children)?;
        }
        Ok(())
    }
    fn populate_tab_templates(&mut self, layout_children: &[KdlNode]) -> Result<(), ConfigError> {
        for child in layout_children.iter() {
            let child_name = kdl_name!(child);
            if child_name == "tab_template" {
                self.populate_one_tab_template(child)?;
            } else if child_name == "default_tab_template" {
                self.populate_default_tab_template(child)?;
            }
        }
        Ok(())
    }
    fn layout_with_tabs(&self, tabs: Vec<(Option<String>, PaneLayout)>) -> Result<Layout, ConfigError> {
        let template = self.default_template()?.unwrap_or_else(|| PaneLayout::default());

        Ok(Layout {
            tabs: tabs,
            template: Some(template),
            ..Default::default()
        })
    }
    fn layout_with_one_tab(&self, panes: Vec<PaneLayout>) -> Result<Layout, ConfigError> {
        let main_tab_layout = PaneLayout {
            children: panes,
            ..Default::default()
        };
        let default_template = self.default_template()?;
        let tabs = if default_template.is_none() {
            // in this case, the layout will be created as the default template and we don't need
            // to explicitly place it in the first tab
            vec![]
        } else {
            vec![(None, main_tab_layout.clone())]
        };
        let template = default_template.unwrap_or_else(|| main_tab_layout.clone());
        // create a layout with one tab that has these child panes
        Ok(Layout {
            tabs,
            template: Some(template),
            ..Default::default()
        })
    }
    fn layout_with_one_pane(&self) -> Result<Layout, ConfigError> {
        let template = self.default_template()?.unwrap_or_else(|| PaneLayout::default());
        Ok(Layout {
            template: Some(template),
            ..Default::default()
        })
    }
    fn populate_layout_child(&mut self, child: &KdlNode, child_tabs: &mut Vec<(Option<String>, PaneLayout)>, child_panes: &mut Vec<PaneLayout>) -> Result<(), ConfigError> {
        let child_name = kdl_name!(child);
        if child_name == "pane" {
            if !child_tabs.is_empty() {
                return Err(ConfigError::KdlParsingError("Cannot have both tabs and panes in the same node".into()));
            }
            child_panes.push(self.parse_pane_node(child)?);
        } else if child_name == "tab" {
            if !child_panes.is_empty() {
                return Err(ConfigError::KdlParsingError("Cannot have both tabs and panes in the same node".into()));
            }
            child_tabs.push(self.parse_tab_node(child)?);
        } else if let Some(tab_template) = self.tab_templates.get(child_name).cloned() {
            if !child_panes.is_empty() {
                return Err(ConfigError::KdlParsingError("Cannot have both tabs and panes in the same node".into()));
            }
            child_tabs.push(self.parse_tab_node_with_template(child, tab_template)?);
        } else if let Some(pane_template) = self.pane_templates.get(child_name).cloned() {
            if !child_tabs.is_empty() {
                return Err(ConfigError::KdlParsingError("Cannot have both tabs and panes in the same node".into()));
            }
            child_panes.push(self.parse_pane_node_with_template(child, pane_template)?);
        }
        Ok(())
    }
    pub fn parse(&mut self) -> Result<Layout, ConfigError> {
        let layout_node = self.kdl_layout.nodes().iter().find(|n| kdl_name!(n) == "layout").ok_or(ConfigError::KdlParsingError("No layout found".into()))?;
        let mut child_tabs = vec![];
        let mut child_panes = vec![];
        if let Some(children) = kdl_children_nodes!(layout_node) {
            self.populate_pane_templates(children)?;
            self.populate_tab_templates(children)?;
            for child in children {
                self.populate_layout_child(child, &mut child_tabs, &mut child_panes)?;
            }
        }
        if !child_tabs.is_empty() {
            self.layout_with_tabs(child_tabs)
        } else if !child_panes.is_empty() {
            self.layout_with_one_tab(child_panes)
        } else {
            self.layout_with_one_pane()
        }
    }
}
