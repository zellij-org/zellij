use crate::input::{
    command::RunCommand,
    config::ConfigError,
    layout::{Layout, PaneLayout, Run, RunPlugin, RunPluginLocation, SplitDirection, SplitSize},
};

use kdl::*;

use std::collections::{HashMap, HashSet};
use std::str::FromStr;

use crate::{
    kdl_child_with_name, kdl_children_nodes, kdl_get_bool_property_or_child_value,
    kdl_get_bool_property_or_child_value_with_error, kdl_get_child,
    kdl_get_int_property_or_child_value, kdl_get_property_or_child,
    kdl_get_string_property_or_child_value, kdl_get_string_property_or_child_value_with_error,
    kdl_name, kdl_parsing_error, kdl_property_names, kdl_property_or_child_value_node,
    kdl_string_arguments,
};

use std::convert::TryFrom;
use std::path::PathBuf;
use std::vec::Vec;
use url::Url;

pub struct KdlLayoutParser<'a> {
    global_cwd: Option<PathBuf>,
    raw_layout: &'a str,
    tab_templates: HashMap<String, (PaneLayout, KdlNode)>,
    pane_templates: HashMap<String, (PaneLayout, KdlNode)>,
    default_tab_template: Option<(PaneLayout, KdlNode)>,
}

impl<'a> KdlLayoutParser<'a> {
    pub fn new(raw_layout: &'a str, global_cwd: Option<PathBuf>) -> Self {
        KdlLayoutParser {
            raw_layout,
            tab_templates: HashMap::new(),
            pane_templates: HashMap::new(),
            default_tab_template: None,
            global_cwd,
        }
    }
    fn is_a_reserved_word(&self, word: &str) -> bool {
        word == "pane"
            || word == "layout"
            || word == "pane_template"
            || word == "tab_template"
            || word == "default_tab_template"
            || word == "command"
            || word == "plugin"
            || word == "children"
            || word == "tab"
            || word == "args"
            || word == "borderless"
            || word == "focus"
            || word == "name"
            || word == "size"
            || word == "cwd"
            || word == "split_direction"
    }
    fn is_a_valid_pane_property(&self, property_name: &str) -> bool {
        property_name == "borderless"
            || property_name == "focus"
            || property_name == "name"
            || property_name == "size"
            || property_name == "plugin"
            || property_name == "command"
            || property_name == "cwd"
            || property_name == "args"
            || property_name == "split_direction"
            || property_name == "pane"
            || property_name == "children"
    }
    fn is_a_valid_tab_property(&self, property_name: &str) -> bool {
        property_name == "focus" || property_name == "name" || property_name == "split_direction"
    }
    fn assert_legal_node_name(&self, name: &str, kdl_node: &KdlNode) -> Result<(), ConfigError> {
        if name.contains(char::is_whitespace) {
            Err(ConfigError::new_kdl_error(
                format!("Node names ({}) cannot contain whitespace.", name),
                kdl_node.span().offset(),
                kdl_node.span().len(),
            ))
        } else if self.is_a_reserved_word(&name) {
            Err(ConfigError::new_kdl_error(
                format!("Node name '{}' is a reserved word.", name),
                kdl_node.span().offset(),
                kdl_node.span().len(),
            ))
        } else {
            Ok(())
        }
    }
    fn parse_split_size(&self, kdl_node: &KdlNode) -> Result<Option<SplitSize>, ConfigError> {
        if let Some(size) = kdl_get_string_property_or_child_value!(kdl_node, "size") {
            match SplitSize::from_str(size) {
                Ok(size) => Ok(Some(size)),
                Err(_e) => Err(kdl_parsing_error!(
                    format!(
                        "size should be a fixed number (eg. 1) or a quoted percent (eg. \"50%\")"
                    ),
                    kdl_node
                )),
            }
        } else if let Some(size) = kdl_get_int_property_or_child_value!(kdl_node, "size") {
            if size == 0 {
                return Err(kdl_parsing_error!(
                    format!("size should be greater than 0"),
                    kdl_node
                ));
            }
            Ok(Some(SplitSize::Fixed(size as usize)))
        } else if let Some(node) = kdl_property_or_child_value_node!(kdl_node, "size") {
            Err(kdl_parsing_error!(
                format!("size should be a fixed number (eg. 1) or a quoted percent (eg. \"50%\")"),
                node
            ))
        } else if let Some(node) = kdl_child_with_name!(kdl_node, "size") {
            Err(kdl_parsing_error!(
                format!(
                    "size cannot be bare, it should have a value (eg. 'size 1', or 'size \"50%\"')"
                ),
                node
            ))
        } else {
            Ok(None)
        }
    }
    fn parse_plugin_block(&self, plugin_block: &KdlNode) -> Result<Option<Run>, ConfigError> {
        let _allow_exec_host_cmd =
            kdl_get_bool_property_or_child_value_with_error!(plugin_block, "_allow_exec_host_cmd")
                .unwrap_or(false);
        let string_url =
            kdl_get_string_property_or_child_value_with_error!(plugin_block, "location").ok_or(
                ConfigError::new_kdl_error(
                    "Plugins must have a location".into(),
                    plugin_block.span().offset(),
                    plugin_block.span().len(),
                ),
            )?;
        let url_node = kdl_get_property_or_child!(plugin_block, "location").ok_or(
            ConfigError::new_kdl_error(
                "Plugins must have a location".into(),
                plugin_block.span().offset(),
                plugin_block.span().len(),
            ),
        )?;
        let url = Url::parse(string_url).map_err(|e| {
            ConfigError::new_kdl_error(
                format!("Failed to parse url: {:?}", e),
                url_node.span().offset(),
                url_node.span().len(),
            )
        })?;
        let location = RunPluginLocation::try_from(url)?;
        Ok(Some(Run::Plugin(RunPlugin {
            _allow_exec_host_cmd,
            location,
        })))
    }
    fn parse_args(&self, pane_node: &KdlNode) -> Result<Option<Vec<String>>, ConfigError> {
        match kdl_get_child!(pane_node, "args") {
            Some(kdl_args) => {
                if kdl_args.entries().is_empty() {
                    return Err(kdl_parsing_error!(format!("args cannot be empty and should contain one or more command arguments (eg. args \"-h\" \"-v\")"), kdl_args));
                }
                Ok(Some(
                    kdl_string_arguments!(kdl_args)
                        .iter()
                        .map(|s| String::from(*s))
                        .collect(),
                ))
            },
            None => Ok(None),
        }
    }
    fn parse_cwd(&self, kdl_node: &KdlNode) -> Result<Option<PathBuf>, ConfigError> {
        Ok(kdl_get_string_property_or_child_value_with_error!(kdl_node, "cwd")
            .and_then(|c| {
                match &self.global_cwd {
                    Some(global_cwd) => {
                        Some(global_cwd.join(c))
                    },
                    None => {
                        Some(PathBuf::from(c))
                    }
                }
            })
            .or_else(|| self.global_cwd.as_ref().map(|g| g.clone()))
        )
    }
    fn parse_pane_command(
        &self,
        pane_node: &KdlNode,
        is_template: bool,
    ) -> Result<Option<Run>, ConfigError> {
        let command = kdl_get_string_property_or_child_value_with_error!(pane_node, "command")
            .map(|c| PathBuf::from(c));
        let cwd = if is_template {
            // we fill the global_cwd for templates later
            kdl_get_string_property_or_child_value_with_error!(pane_node, "cwd")
                .map(|c| PathBuf::from(c))
        } else {
            self.parse_cwd(pane_node)?
        };
        let args = self.parse_args(pane_node)?;
        match (command, cwd, args, is_template) {
            (None, Some(cwd), _, _) =>  Ok(Some(Run::Cwd(cwd))),
            (None, _, Some(_args), false) => Err(ConfigError::new_kdl_error(
                "args can only be set if a command was specified".into(),
                pane_node.span().offset(),
                pane_node.span().len(),
            )),
            (Some(command), cwd, args, _) => Ok(Some(Run::Command(RunCommand {
                command,
                args: args.unwrap_or_else(|| vec![]),
                cwd,
                hold_on_close: true,
            }))),
            _ => Ok(None),
        }
    }
    fn parse_command_or_plugin_block(
        &self,
        kdl_node: &KdlNode,
    ) -> Result<Option<Run>, ConfigError> {
        let mut run = self.parse_pane_command(kdl_node, false)?;
        if let Some(plugin_block) = kdl_get_child!(kdl_node, "plugin") {
            if run.is_some() {
                return Err(ConfigError::new_kdl_error(
                    "Cannot have both a command and a plugin block for a single pane".into(),
                    plugin_block.span().offset(),
                    plugin_block.span().len(),
                ));
            }
            run = self.parse_plugin_block(plugin_block)?;
        }
        Ok(run)
    }
    fn parse_command_or_plugin_block_for_template(
        &self,
        kdl_node: &KdlNode,
    ) -> Result<Option<Run>, ConfigError> {
        let mut run = self.parse_pane_command(kdl_node, true)?;
        if let Some(plugin_block) = kdl_get_child!(kdl_node, "plugin") {
            if run.is_some() {
                return Err(ConfigError::new_kdl_error(
                    "Cannot have both a command and a plugin block for a single pane".into(),
                    plugin_block.span().offset(),
                    plugin_block.span().len(),
                ));
            }
            run = self.parse_plugin_block(plugin_block)?;
        }
        Ok(run)
    }
    fn parse_pane_node(&self, kdl_node: &KdlNode) -> Result<PaneLayout, ConfigError> {
        self.assert_valid_pane_properties(kdl_node)?;
        let borderless = kdl_get_bool_property_or_child_value_with_error!(kdl_node, "borderless");
        let focus = kdl_get_bool_property_or_child_value_with_error!(kdl_node, "focus");
        let name = kdl_get_string_property_or_child_value_with_error!(kdl_node, "name")
            .map(|name| name.to_string());
        let split_size = self.parse_split_size(kdl_node)?;
        let run = self.parse_command_or_plugin_block(kdl_node)?;
        let children_split_direction = self.parse_split_direction(kdl_node)?;
        let (external_children_index, children) = match kdl_children_nodes!(kdl_node) {
            Some(children) => self.parse_child_pane_nodes_for_pane(&children)?,
            None => (None, vec![]),
        };
        self.assert_no_mixed_children_and_properties(kdl_node)?;
        Ok(PaneLayout {
            borderless: borderless.unwrap_or_default(),
            focus,
            name,
            split_size,
            run,
            children_split_direction,
            external_children_index,
            children,
            ..Default::default()
        })
    }
    fn insert_children_to_pane_template(&self, kdl_node: &KdlNode, pane_template: &mut PaneLayout, pane_template_kdl_node: &KdlNode) -> Result<(), ConfigError> {
        let children_split_direction = self.parse_split_direction(kdl_node)?;
        let (external_children_index, pane_parts) = match kdl_children_nodes!(kdl_node) {
            Some(children) => self.parse_child_pane_nodes_for_pane(&children)?,
            None => (None, vec![]),
        };
        if pane_parts.len() > 0 {
            let child_panes_layout = PaneLayout {
                children_split_direction,
                children: pane_parts,
                external_children_index,
                ..Default::default()
            };
            self.assert_one_children_block(&pane_template, pane_template_kdl_node)?;
            self.insert_layout_children_or_error(
                pane_template,
                child_panes_layout,
                pane_template_kdl_node,
            )?;
        }
        Ok(())
    }
    fn parse_pane_node_with_template(
        &self,
        kdl_node: &KdlNode,
        mut pane_template: PaneLayout,
        pane_template_kdl_node: &KdlNode,
    ) -> Result<PaneLayout, ConfigError> {
        let borderless = kdl_get_bool_property_or_child_value_with_error!(kdl_node, "borderless");
        let focus = kdl_get_bool_property_or_child_value_with_error!(kdl_node, "focus");
        let name = kdl_get_string_property_or_child_value_with_error!(kdl_node, "name")
            .map(|name| name.to_string());
        let args = self.parse_args(kdl_node)?;
        let split_size = self.parse_split_size(kdl_node)?;
        let run = self.parse_command_or_plugin_block_for_template(kdl_node)?;
        self.assert_no_bare_args_in_pane_node_with_template(&run, &pane_template.run, &args, kdl_node)?;
        self.insert_children_to_pane_template(kdl_node, &mut pane_template, pane_template_kdl_node)?;
        pane_template.run = Run::merge(&pane_template.run, &run);
        self.populate_global_cwd_for_pane_run(&mut pane_template.run)?;
        if let (Some(Run::Command(pane_template_run_command)), Some(args)) = (pane_template.run.as_mut(), args) {
            if !args.is_empty() {
                pane_template_run_command.args = args.clone();
            }
        }
        if let Some(borderless) = borderless {
            pane_template.borderless = borderless;
        }
        if let Some(focus) = focus {
            pane_template.focus = Some(focus);
        }
        if let Some(name) = name {
            pane_template.name = Some(name);
        }
        if let Some(split_size) = split_size {
            pane_template.split_size = Some(split_size);
        }
        if let Some(index_of_children) = pane_template.external_children_index {
            pane_template
                .children
                .insert(index_of_children, PaneLayout::default());
        }
        pane_template.external_children_index = None;
        Ok(pane_template)
    }
    fn populate_global_cwd_for_pane_run(&self, pane_run: &mut Option<Run>) -> Result<(), ConfigError> {
        if let Some(global_cwd) = &self.global_cwd {
            match pane_run.as_mut() {
                Some(Run::Command(run_command)) =>  {
                    match run_command.cwd.as_mut() {
                        Some(run_command_cwd) => {
                            *run_command_cwd = global_cwd.join(&run_command_cwd);
                        },
                        None => {
                            run_command.cwd = Some(global_cwd.clone());
                        }
                    }
                },
                Some(Run::Cwd(pane_template_cwd)) => {
                    *pane_template_cwd = global_cwd.join(&pane_template_cwd);
                }
                _ => {}
            }
        }
        Ok(())
    }
    fn parse_split_direction(&self, kdl_node: &KdlNode) -> Result<SplitDirection, ConfigError> {
        match kdl_get_string_property_or_child_value_with_error!(kdl_node, "split_direction") {
            Some(direction) => match SplitDirection::from_str(direction) {
                Ok(split_direction) => Ok(split_direction),
                Err(_e) => Err(kdl_parsing_error!(
                    format!(
                        "split_direction should be either \"horizontal\" or \"vertical\" found: {}",
                        direction
                    ),
                    kdl_node
                )),
            },
            None => Ok(SplitDirection::default()),
        }
    }
    fn parse_pane_template_node(&mut self, kdl_node: &KdlNode) -> Result<(), ConfigError> {
        self.assert_valid_pane_properties(kdl_node)?;
        let template_name = kdl_get_string_property_or_child_value!(kdl_node, "name")
            .map(|s| s.to_string())
            .ok_or(ConfigError::new_kdl_error(
                "Pane templates must have a name".into(),
                kdl_node.span().offset(),
                kdl_node.span().len(),
            ))?;
        self.assert_legal_node_name(&template_name, kdl_node)?;
        let borderless = kdl_get_bool_property_or_child_value_with_error!(kdl_node, "borderless");
        let focus = kdl_get_bool_property_or_child_value_with_error!(kdl_node, "focus");
        let split_size = self.parse_split_size(kdl_node)?;
        let run = self.parse_command_or_plugin_block(kdl_node)?;
        let children_split_direction = self.parse_split_direction(kdl_node)?;
        let (external_children_index, pane_parts) = match kdl_children_nodes!(kdl_node) {
            Some(children) => self.parse_child_pane_nodes_for_pane(&children)?,
            None => (None, vec![]),
        };
        self.assert_no_mixed_children_and_properties(kdl_node)?;
        self.pane_templates.insert(
            template_name,
            (
                PaneLayout {
                    borderless: borderless.unwrap_or_default(),
                    focus,
                    split_size,
                    run,
                    children_split_direction,
                    external_children_index,
                    children: pane_parts,
                    ..Default::default()
                },
                kdl_node.clone(),
            ),
        );
        Ok(())
    }
    fn parse_tab_node(
        &mut self,
        kdl_node: &KdlNode,
    ) -> Result<(bool, Option<String>, PaneLayout), ConfigError> {
        // (is_focused, Option<tab_name>, PaneLayout)
        self.assert_valid_tab_properties(kdl_node)?;
        let tab_name =
            kdl_get_string_property_or_child_value!(kdl_node, "name").map(|s| s.to_string());
        let is_focused = kdl_get_bool_property_or_child_value!(kdl_node, "focus").unwrap_or(false);
        let children_split_direction = self.parse_split_direction(kdl_node)?;
        let children = match kdl_children_nodes!(kdl_node) {
            Some(children) => self.parse_child_pane_nodes_for_tab(children)?,
            None => vec![],
        };
        Ok((
            is_focused,
            tab_name,
            PaneLayout {
                children_split_direction,
                children,
                ..Default::default()
            },
        ))
    }
    fn parse_child_pane_nodes_for_tab(
        &self,
        children: &[KdlNode],
    ) -> Result<Vec<PaneLayout>, ConfigError> {
        let mut nodes = vec![];
        for child in children {
            if kdl_name!(child) == "pane" {
                nodes.push(self.parse_pane_node(child)?);
            } else if let Some((pane_template, pane_template_kdl_node)) =
                self.pane_templates.get(kdl_name!(child)).cloned()
            {
                nodes.push(self.parse_pane_node_with_template(
                    child,
                    pane_template,
                    &pane_template_kdl_node,
                )?);
            } else if self.is_a_valid_tab_property(kdl_name!(child)) {
                return Err(ConfigError::new_kdl_error(
                    format!("Tab property '{}' must be placed on the tab title line and not in the child braces", kdl_name!(child)),
                    child.span().offset(),
                    child.span().len()
                ));
            } else {
                return Err(ConfigError::new_kdl_error(
                    format!("Invalid tab property: {}", kdl_name!(child)),
                    child.span().offset(),
                    child.span().len(),
                ));
            }
        }
        if nodes.is_empty() {
            nodes.push(PaneLayout::default());
        }
        Ok(nodes)
    }
    fn parse_child_pane_nodes_for_pane(
        &self,
        children: &[KdlNode],
    ) -> Result<(Option<usize>, Vec<PaneLayout>), ConfigError> {
        // usize is external_children_index
        let mut external_children_index = None;
        let mut nodes = vec![];
        for (i, child) in children.iter().enumerate() {
            if kdl_name!(child) == "pane" {
                nodes.push(self.parse_pane_node(child)?);
            } else if kdl_name!(child) == "children" {
                let node_has_child_nodes = child.children().map(|c| !c.is_empty()).unwrap_or(false);
                let node_has_entries = !child.entries().is_empty();
                if node_has_child_nodes || node_has_entries {
                    return Err(ConfigError::new_kdl_error(
                        format!("The `children` node must be bare. All properties should be places on the node consuming this template."),
                        child.span().offset(),
                        child.span().len(),
                    ));
                }
                external_children_index = Some(i);
            } else if let Some((pane_template, pane_template_kdl_node)) =
                self.pane_templates.get(kdl_name!(child)).cloned()
            {
                nodes.push(self.parse_pane_node_with_template(
                    child,
                    pane_template,
                    &pane_template_kdl_node,
                )?);
            } else if !self.is_a_valid_pane_property(kdl_name!(child)) {
                return Err(ConfigError::new_kdl_error(
                    format!("Unknown pane property: {}", kdl_name!(child)),
                    child.span().offset(),
                    child.span().len(),
                ));
            }
        }
        Ok((external_children_index, nodes))
    }
    fn has_child_panes_tabs_or_templates(&self, kdl_node: &KdlNode) -> bool {
        if let Some(children) = kdl_children_nodes!(kdl_node) {
            for child in children {
                let child_node_name = kdl_name!(child);
                if child_node_name == "pane"
                    || child_node_name == "children"
                    || child_node_name == "tab"
                    || child_node_name == "children"
                {
                    return true;
                } else if let Some((_pane_template, _pane_template_kdl_node)) =
                    self.pane_templates.get(child_node_name).cloned()
                {
                    return true;
                }
            }
        }
        false
    }
    fn assert_no_bare_args_in_pane_node_with_template(&self, pane_run: &Option<Run>, pane_template_run: &Option<Run>, args: &Option<Vec<String>>, pane_node: &KdlNode) -> Result<(), ConfigError> {
        if let (None, None, true) =
            (pane_run, pane_template_run, args.is_some())
        {
            return Err(kdl_parsing_error!(
                format!("args can only be specified if a command was specified either in the pane_template or in the pane"),
                pane_node
            ));
        }
        Ok(())
    }
    fn assert_one_children_block(
        &self,
        layout: &PaneLayout,
        kdl_node: &KdlNode,
    ) -> Result<(), ConfigError> {
        let children_block_count = layout.children_block_count();
        if children_block_count != 1 {
            return Err(ConfigError::new_kdl_error(format!("This template has {} children blocks, only 1 is allowed when used to insert child panes", children_block_count), kdl_node.span().offset(), kdl_node.span().len()));
        }
        Ok(())
    }
    fn assert_valid_pane_properties(&self, pane_node: &KdlNode) -> Result<(), ConfigError> {
        for entry in pane_node.entries() {
            match entry
                .name()
                .map(|e| e.value())
                .or_else(|| entry.value().as_string())
            {
                Some(string_name) => {
                    if !self.is_a_valid_pane_property(string_name) {
                        return Err(ConfigError::new_kdl_error(
                            format!("Unknown pane property: {}", string_name),
                            entry.span().offset(),
                            entry.span().len(),
                        ));
                    }
                },
                None => {
                    return Err(ConfigError::new_kdl_error(
                        "Unknown pane property".into(),
                        entry.span().offset(),
                        entry.span().len(),
                    ));
                },
            }
        }
        Ok(())
    }
    fn assert_valid_tab_properties(&self, pane_node: &KdlNode) -> Result<(), ConfigError> {
        let all_property_names = kdl_property_names!(pane_node);
        for name in all_property_names {
            if !self.is_a_valid_tab_property(name) {
                return Err(ConfigError::new_kdl_error(
                    format!("Invalid tab property '{}'", name),
                    pane_node.span().offset(),
                    pane_node.span().len(),
                ));
            }
        }
        Ok(())
    }
    fn assert_no_mixed_children_and_properties(
        &self,
        kdl_node: &KdlNode,
    ) -> Result<(), ConfigError> {
        let has_borderless_prop =
            kdl_get_bool_property_or_child_value_with_error!(kdl_node, "borderless").is_some();
        let has_focus_prop =
            kdl_get_bool_property_or_child_value_with_error!(kdl_node, "focus").is_some();
        let has_non_cwd_run_prop = self.parse_command_or_plugin_block(kdl_node)?.map(|r| {
            match r {
                Run::Cwd(_) => false,
                _ => true
            }
        }).unwrap_or(false);
        let has_nested_nodes_or_children_block = self.has_child_panes_tabs_or_templates(kdl_node);
        if has_nested_nodes_or_children_block
            && (has_borderless_prop || has_focus_prop || has_non_cwd_run_prop)
        {
            let mut offending_nodes = vec![];
            if has_borderless_prop {
                offending_nodes.push("borderless");
            }
            if has_focus_prop {
                offending_nodes.push("focus");
            }
            if has_non_cwd_run_prop {
                offending_nodes.push("command/plugin");
            }
            Err(ConfigError::new_kdl_error(
                format!(
                    "Cannot have both properties ({}) and nested children",
                    offending_nodes.join(", ")
                ),
                kdl_node.span().offset(),
                kdl_node.span().len(),
            ))
        } else {
            Ok(())
        }
    }
    fn insert_layout_children_or_error(
        &self,
        layout: &mut PaneLayout,
        mut child_panes_layout: PaneLayout,
        kdl_node: &KdlNode,
    ) -> Result<(), ConfigError> {
        let successfully_inserted = layout.insert_children_layout(&mut child_panes_layout)?;
        if !successfully_inserted {
            Err(ConfigError::new_kdl_error(
                "This template does not have children".into(),
                kdl_node.span().offset(),
                kdl_node.span().len(),
            ))
        } else {
            Ok(())
        }
    }
    fn parse_tab_node_with_template(
        &self,
        kdl_node: &KdlNode,
        mut tab_layout: PaneLayout,
        tab_layout_kdl_node: &KdlNode,
    ) -> Result<(bool, Option<String>, PaneLayout), ConfigError> {
        // (is_focused, Option<tab_name>, PaneLayout)
        let tab_name =
            kdl_get_string_property_or_child_value!(kdl_node, "name").map(|s| s.to_string());
        let is_focused = kdl_get_bool_property_or_child_value!(kdl_node, "focus").unwrap_or(false);
        let children_split_direction = self.parse_split_direction(kdl_node)?;
        match kdl_children_nodes!(kdl_node) {
            Some(children) => {
                let child_panes = self.parse_child_pane_nodes_for_tab(children)?;
                let child_panes_layout = PaneLayout {
                    children_split_direction,
                    children: child_panes,
                    ..Default::default()
                };
                self.assert_one_children_block(&tab_layout, &tab_layout_kdl_node)?;
                self.insert_layout_children_or_error(
                    &mut tab_layout,
                    child_panes_layout,
                    &tab_layout_kdl_node,
                )?;
            },
            None => {
                if let Some(index_of_children) = tab_layout.external_children_index {
                    tab_layout
                        .children
                        .insert(index_of_children, PaneLayout::default());
                }
            },
        }
        tab_layout.external_children_index = None;
        Ok((is_focused, tab_name, tab_layout))
    }
    fn populate_one_tab_template(&mut self, kdl_node: &KdlNode) -> Result<(), ConfigError> {
        let template_name = kdl_get_string_property_or_child_value_with_error!(kdl_node, "name")
            .map(|s| s.to_string())
            .ok_or(ConfigError::new_kdl_error(
                "Tab templates must have a name".into(),
                kdl_node.span().offset(),
                kdl_node.span().len(),
            ))?;
        self.assert_legal_node_name(&template_name, kdl_node)?;
        if self.tab_templates.contains_key(&template_name) {
            return Err(ConfigError::new_kdl_error(
                format!(
                    "Duplicate definition of the \"{}\" tab_template",
                    template_name
                ),
                kdl_node.span().offset(),
                kdl_node.span().len(),
            ));
        }
        if self.pane_templates.contains_key(&template_name) {
            return Err(ConfigError::new_kdl_error(
                format!("There is already a pane_template with the name \"{}\" - can't have a tab_template with the same name", template_name),
                kdl_node.span().offset(),
                kdl_node.span().len(),
            ));
        }
        self.tab_templates.insert(
            template_name,
            (self.parse_tab_template_node(kdl_node)?, kdl_node.clone()),
        );
        Ok(())
    }
    fn populate_default_tab_template(&mut self, kdl_node: &KdlNode) -> Result<(), ConfigError> {
        self.default_tab_template =
            Some((self.parse_tab_template_node(kdl_node)?, kdl_node.clone()));
        Ok(())
    }
    fn parse_tab_template_node(&self, kdl_node: &KdlNode) -> Result<PaneLayout, ConfigError> {
        self.assert_valid_tab_properties(kdl_node)?;
        let children_split_direction = self.parse_split_direction(kdl_node)?;
        let mut tab_children = vec![];
        let mut external_children_index = None;
        if let Some(children) = kdl_children_nodes!(kdl_node) {
            for (i, child) in children.iter().enumerate() {
                if kdl_name!(child) == "pane" {
                    tab_children.push(self.parse_pane_node(child)?);
                } else if kdl_name!(child) == "children" {
                    let node_has_child_nodes =
                        child.children().map(|c| !c.is_empty()).unwrap_or(false);
                    let node_has_entries = !child.entries().is_empty();
                    if node_has_child_nodes || node_has_entries {
                        return Err(ConfigError::new_kdl_error(
                            format!("The `children` node must be bare. All properties should be places on the node consuming this template."),
                            child.span().offset(),
                            child.span().len(),
                        ));
                    }
                    external_children_index = Some(i);
                } else if let Some((pane_template, pane_template_kdl_node)) =
                    self.pane_templates.get(kdl_name!(child)).cloned()
                {
                    tab_children.push(self.parse_pane_node_with_template(
                        child,
                        pane_template,
                        &pane_template_kdl_node,
                    )?);
                } else if self.is_a_valid_tab_property(kdl_name!(child)) {
                    return Err(ConfigError::new_kdl_error(
                        format!("Tab property '{}' must be placed on the tab_template title line and not in the child braces", kdl_name!(child)),
                        child.span().offset(),
                        child.span().len()
                    ));
                } else {
                    return Err(ConfigError::new_kdl_error(
                        format!("Invalid tab_template property: {}", kdl_name!(child)),
                        child.span().offset(),
                        child.span().len(),
                    ));
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
            Some((template, _kdl_node)) => {
                let mut template = template.clone();
                if let Some(children_index) = template.external_children_index {
                    template
                        .children
                        .insert(children_index, PaneLayout::default())
                }
                template.external_children_index = None;
                Ok(Some(template))
            },
            None => Ok(None),
        }
    }
    pub fn get_pane_template_dependency_tree(
        &self,
        kdl_children: &'a [KdlNode],
    ) -> Result<HashMap<&'a str, HashSet<&'a str>>, ConfigError> {
        let mut dependency_tree = HashMap::new();
        for child in kdl_children {
            if kdl_name!(child) == "pane_template" {
                let template_name = kdl_get_string_property_or_child_value!(child, "name").ok_or(
                    ConfigError::new_kdl_error(
                        "Pane templates must have a name".into(),
                        child.span().offset(),
                        child.span().len(),
                    ),
                )?;
                let mut template_children = HashSet::new();
                self.get_pane_template_dependencies(child, &mut template_children)?;
                if dependency_tree.contains_key(template_name) {
                    return Err(ConfigError::new_kdl_error(
                        format!(
                            "Duplicate definition of the \"{}\" pane_template",
                            template_name
                        ),
                        child.span().offset(),
                        child.span().len(),
                    ));
                }
                dependency_tree.insert(template_name, template_children);
            }
        }
        Ok(dependency_tree)
    }
    fn get_pane_template_dependencies(
        &self,
        node: &'a KdlNode,
        all_dependencies: &mut HashSet<&'a str>,
    ) -> Result<(), ConfigError> {
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
    pub fn parse_pane_template_by_name(
        &mut self,
        pane_template_name: &str,
        kdl_children: &[KdlNode],
    ) -> Result<(), ConfigError> {
        for child in kdl_children.iter() {
            let child_name = kdl_name!(child);
            if child_name == "pane_template" {
                let child_name = kdl_get_string_property_or_child_value!(child, "name");
                if child_name == Some(pane_template_name) {
                    self.parse_pane_template_node(child)?;
                }
            }
        }
        Ok(())
    }
    fn populate_global_cwd(&mut self, layout_node: &KdlNode) -> Result<(), ConfigError> {
        // we only populate global cwd from the layout file if another wasn't explicitly passed to us
        if self.global_cwd.is_none() {
            if let Some(global_cwd) = kdl_get_string_property_or_child_value_with_error!(layout_node, "cwd") {
                self.global_cwd = Some(PathBuf::from(global_cwd));
            }
        }
        Ok(())
    }
    fn populate_pane_templates(
        &mut self,
        layout_children: &[KdlNode],
        kdl_layout: &KdlDocument,
    ) -> Result<(), ConfigError> {
        let mut pane_template_dependency_tree =
            self.get_pane_template_dependency_tree(layout_children)?;
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
                return Err(ConfigError::new_kdl_error(
                    "Circular dependency detected between pane templates.".into(),
                    kdl_layout.span().offset(),
                    kdl_layout.span().len(),
                ));
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
    fn layout_with_tabs(
        &self,
        tabs: Vec<(Option<String>, PaneLayout)>,
        focused_tab_index: Option<usize>,
    ) -> Result<Layout, ConfigError> {
        let template = self
            .default_template()?
            .unwrap_or_else(|| PaneLayout::default());

        Ok(Layout {
            tabs: tabs,
            template: Some(template),
            focused_tab_index,
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
        let template = self
            .default_template()?
            .unwrap_or_else(|| PaneLayout::default());
        Ok(Layout {
            template: Some(template),
            ..Default::default()
        })
    }
    fn populate_layout_child(
        &mut self,
        child: &KdlNode,
        child_tabs: &mut Vec<(bool, Option<String>, PaneLayout)>,
        child_panes: &mut Vec<PaneLayout>,
    ) -> Result<(), ConfigError> {
        let child_name = kdl_name!(child);
        if child_name == "pane" {
            if !child_tabs.is_empty() {
                return Err(ConfigError::new_kdl_error(
                    "Cannot have both tabs and panes in the same node".into(),
                    child.span().offset(),
                    child.span().len(),
                ));
            }
            child_panes.push(self.parse_pane_node(child)?);
        } else if child_name == "tab" {
            if !child_panes.is_empty() {
                return Err(ConfigError::new_kdl_error(
                    "Cannot have both tabs and panes in the same node".into(),
                    child.span().offset(),
                    child.span().len(),
                ));
            }
            match &self.default_tab_template {
                Some((default_tab_template, default_tab_template_kdl_node)) => {
                    let default_tab_template = default_tab_template.clone();
                    child_tabs.push(self.parse_tab_node_with_template(
                        child,
                        default_tab_template,
                        default_tab_template_kdl_node,
                    )?);
                },
                None => {
                    child_tabs.push(self.parse_tab_node(child)?);
                },
            }
        } else if let Some((tab_template, tab_template_kdl_node)) =
            self.tab_templates.get(child_name).cloned()
        {
            if !child_panes.is_empty() {
                return Err(ConfigError::new_kdl_error(
                    "Cannot have both tabs and panes in the same node".into(),
                    child.span().offset(),
                    child.span().len(),
                ));
            }
            child_tabs.push(self.parse_tab_node_with_template(
                child,
                tab_template,
                &tab_template_kdl_node,
            )?);
        } else if let Some((pane_template, pane_template_kdl_node)) =
            self.pane_templates.get(child_name).cloned()
        {
            if !child_tabs.is_empty() {
                return Err(ConfigError::new_kdl_error(
                    "Cannot have both tabs and panes in the same node".into(),
                    child.span().offset(),
                    child.span().len(),
                ));
            }
            child_panes.push(self.parse_pane_node_with_template(
                child,
                pane_template,
                &pane_template_kdl_node,
            )?);
        } else if !self.is_a_reserved_word(child_name) {
            return Err(ConfigError::new_kdl_error(
                format!("Unknown layout node: '{}'", child_name),
                child.span().offset(),
                child.span().len(),
            ));
        }
        Ok(())
    }
    pub fn parse(&mut self) -> Result<Layout, ConfigError> {
        let kdl_layout: KdlDocument = self.raw_layout.parse()?;
        let layout_node = kdl_layout
            .nodes()
            .iter()
            .find(|n| kdl_name!(n) == "layout")
            .ok_or(ConfigError::new_kdl_error(
                "No layout found".into(),
                kdl_layout.span().offset(),
                kdl_layout.span().len(),
            ))?;
        let has_multiple_layout_nodes = kdl_layout
            .nodes()
            .iter()
            .filter(|n| kdl_name!(n) == "layout")
            .count()
            > 1;
        if has_multiple_layout_nodes {
            return Err(ConfigError::new_kdl_error(
                "Only one layout node per file allowed".into(),
                kdl_layout.span().offset(),
                kdl_layout.span().len(),
            ));
        }
        let mut child_tabs = vec![];
        let mut child_panes = vec![];
        if let Some(children) = kdl_children_nodes!(layout_node) {
            self.populate_global_cwd(layout_node)?;
            self.populate_pane_templates(children, &kdl_layout)?;
            self.populate_tab_templates(children)?;
            for child in children {
                self.populate_layout_child(child, &mut child_tabs, &mut child_panes)?;
            }
        }
        if !child_tabs.is_empty() {
            let has_more_than_one_focused_tab = child_tabs
                .iter()
                .filter(|(is_focused, _, _)| *is_focused)
                .count()
                > 1;
            if has_more_than_one_focused_tab {
                return Err(ConfigError::new_kdl_error(
                    "Only one tab can be focused".into(),
                    kdl_layout.span().offset(),
                    kdl_layout.span().len(),
                ));
            }
            let focused_tab_index = child_tabs.iter().position(|(is_focused, _, _)| *is_focused);
            let child_tabs: Vec<(Option<String>, PaneLayout)> = child_tabs
                .drain(..)
                .map(|(_is_focused, tab_name, pane_layout)| (tab_name, pane_layout))
                .collect();
            self.layout_with_tabs(child_tabs, focused_tab_index)
        } else if !child_panes.is_empty() {
            self.layout_with_one_tab(child_panes)
        } else {
            self.layout_with_one_pane()
        }
    }
}
