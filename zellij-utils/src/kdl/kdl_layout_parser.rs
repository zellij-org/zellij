use crate::input::{
    command::RunCommand,
    config::ConfigError,
    layout::{
        FloatingPaneLayout, Layout, LayoutConstraint, PercentOrFixed, PluginUserConfiguration, Run,
        RunPluginOrAlias, SplitDirection, SplitSize, SwapFloatingLayout, SwapTiledLayout,
        TiledPaneLayout,
    },
};

use kdl::*;

use std::collections::{BTreeMap, HashMap, HashSet};
use std::str::FromStr;

use crate::{
    kdl_child_with_name, kdl_children_nodes, kdl_first_entry_as_bool, kdl_first_entry_as_i64,
    kdl_first_entry_as_string, kdl_get_bool_property_or_child_value,
    kdl_get_bool_property_or_child_value_with_error, kdl_get_child,
    kdl_get_int_property_or_child_value, kdl_get_property_or_child,
    kdl_get_string_property_or_child_value, kdl_get_string_property_or_child_value_with_error,
    kdl_name, kdl_parsing_error, kdl_property_names, kdl_property_or_child_value_node,
    kdl_string_arguments,
};

use std::path::PathBuf;
use std::vec::Vec;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PaneOrFloatingPane {
    Pane(TiledPaneLayout),
    FloatingPane(FloatingPaneLayout),
    Either(TiledPaneLayout),
}

pub struct KdlLayoutParser<'a> {
    global_cwd: Option<PathBuf>,
    raw_layout: &'a str,
    tab_templates: HashMap<String, (TiledPaneLayout, Vec<FloatingPaneLayout>, KdlNode)>,
    pane_templates: HashMap<String, (PaneOrFloatingPane, KdlNode)>,
    default_tab_template: Option<(TiledPaneLayout, Vec<FloatingPaneLayout>, KdlNode)>,
    new_tab_template: Option<(TiledPaneLayout, Vec<FloatingPaneLayout>)>,
    file_name: Option<PathBuf>,
}

impl<'a> KdlLayoutParser<'a> {
    pub fn new(
        raw_layout: &'a str,
        global_cwd: Option<PathBuf>,
        file_name: Option<String>,
    ) -> Self {
        KdlLayoutParser {
            raw_layout,
            tab_templates: HashMap::new(),
            pane_templates: HashMap::new(),
            default_tab_template: None,
            new_tab_template: None,
            global_cwd,
            file_name: file_name.map(|f| PathBuf::from(f)),
        }
    }
    fn is_a_reserved_word(&self, word: &str) -> bool {
        // note that it's important that none of these words happens to also be a config property,
        // otherwise they might collide
        word == "pane"
            || word == "layout"
            || word == "pane_template"
            || word == "tab_template"
            || word == "default_tab_template"
            || word == "new_tab_template"
            || word == "command"
            || word == "edit"
            || word == "plugin"
            || word == "children"
            || word == "tab"
            || word == "args"
            || word == "close_on_exit"
            || word == "start_suspended"
            || word == "borderless"
            || word == "focus"
            || word == "name"
            || word == "size"
            || word == "cwd"
            || word == "split_direction"
            || word == "swap_tiled_layout"
            || word == "swap_floating_layout"
            || word == "hide_floating_panes"
            || word == "contents_file"
    }
    fn is_a_valid_pane_property(&self, property_name: &str) -> bool {
        property_name == "borderless"
            || property_name == "focus"
            || property_name == "name"
            || property_name == "size"
            || property_name == "plugin"
            || property_name == "command"
            || property_name == "edit"
            || property_name == "cwd"
            || property_name == "args"
            || property_name == "close_on_exit"
            || property_name == "start_suspended"
            || property_name == "split_direction"
            || property_name == "pane"
            || property_name == "children"
            || property_name == "stacked"
            || property_name == "expanded"
            || property_name == "exclude_from_sync"
            || property_name == "contents_file"
    }
    fn is_a_valid_floating_pane_property(&self, property_name: &str) -> bool {
        property_name == "borderless"
            || property_name == "focus"
            || property_name == "name"
            || property_name == "plugin"
            || property_name == "command"
            || property_name == "edit"
            || property_name == "cwd"
            || property_name == "args"
            || property_name == "close_on_exit"
            || property_name == "start_suspended"
            || property_name == "x"
            || property_name == "y"
            || property_name == "width"
            || property_name == "height"
            || property_name == "pinned"
            || property_name == "contents_file"
    }
    fn is_a_valid_tab_property(&self, property_name: &str) -> bool {
        property_name == "focus"
            || property_name == "name"
            || property_name == "split_direction"
            || property_name == "cwd"
            || property_name == "floating_panes"
            || property_name == "children"
            || property_name == "max_panes"
            || property_name == "min_panes"
            || property_name == "exact_panes"
            || property_name == "hide_floating_panes"
    }
    pub fn is_a_reserved_plugin_property(property_name: &str) -> bool {
        property_name == "location"
            || property_name == "_allow_exec_host_cmd"
            || property_name == "path"
    }
    fn assert_legal_node_name(&self, name: &str, kdl_node: &KdlNode) -> Result<(), ConfigError> {
        if name.contains(char::is_whitespace) {
            Err(ConfigError::new_layout_kdl_error(
                format!("Node names ({}) cannot contain whitespace.", name),
                kdl_node.span().offset(),
                kdl_node.span().len(),
            ))
        } else if self.is_a_reserved_word(&name) {
            Err(ConfigError::new_layout_kdl_error(
                format!("Node name '{}' is a reserved word.", name),
                kdl_node.span().offset(),
                kdl_node.span().len(),
            ))
        } else {
            Ok(())
        }
    }
    fn assert_legal_template_name(
        &self,
        name: &str,
        kdl_node: &KdlNode,
    ) -> Result<(), ConfigError> {
        if name.is_empty() {
            Err(ConfigError::new_layout_kdl_error(
                format!("Template names cannot be empty"),
                kdl_node.span().offset(),
                kdl_node.span().len(),
            ))
        } else if name.contains(')') || name.contains('(') {
            Err(ConfigError::new_layout_kdl_error(
                format!("Template names cannot contain parantheses"),
                kdl_node.span().offset(),
                kdl_node.span().len(),
            ))
        } else if name
            .chars()
            .next()
            .map(|first_char| first_char.is_numeric())
            .unwrap_or(false)
        {
            Err(ConfigError::new_layout_kdl_error(
                format!("Template names cannot start with numbers"),
                kdl_node.span().offset(),
                kdl_node.span().len(),
            ))
        } else {
            Ok(())
        }
    }
    fn assert_no_grandchildren_in_stack(
        &self,
        children: &[KdlNode],
        is_part_of_stack: bool,
    ) -> Result<(), ConfigError> {
        if is_part_of_stack {
            for child in children {
                if kdl_name!(child) == "pane" || self.pane_templates.get(kdl_name!(child)).is_some()
                {
                    return Err(ConfigError::new_layout_kdl_error(
                        format!("Stacked panes cannot have children"),
                        child.span().offset(),
                        child.span().len(),
                    ));
                }
            }
        }
        Ok(())
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
    fn parse_percent_or_fixed(
        &self,
        kdl_node: &KdlNode,
        value_name: &str,
        can_be_zero: bool,
    ) -> Result<Option<PercentOrFixed>, ConfigError> {
        if let Some(size) = kdl_get_string_property_or_child_value!(kdl_node, value_name) {
            match PercentOrFixed::from_str(size) {
                Ok(size) => {
                    if !can_be_zero && size.is_zero() {
                        Err(kdl_parsing_error!(
                            format!("{} should be greater than 0", value_name),
                            kdl_node
                        ))
                    } else {
                        Ok(Some(size))
                    }
                },
                Err(_e) => Err(kdl_parsing_error!(
                    format!(
                        "{} should be a fixed number (eg. 1) or a quoted percent (eg. \"50%\")",
                        value_name
                    ),
                    kdl_node
                )),
            }
        } else if let Some(size) = kdl_get_int_property_or_child_value!(kdl_node, value_name) {
            if size == 0 && !can_be_zero {
                return Err(kdl_parsing_error!(
                    format!("{} should be greater than 0", value_name),
                    kdl_node
                ));
            }
            Ok(Some(PercentOrFixed::Fixed(size as usize)))
        } else if let Some(node) = kdl_property_or_child_value_node!(kdl_node, "size") {
            Err(kdl_parsing_error!(
                format!(
                    "{} should be a fixed number (eg. 1) or a quoted percent (eg. \"50%\")",
                    value_name
                ),
                node
            ))
        } else if let Some(node) = kdl_child_with_name!(kdl_node, "size") {
            Err(kdl_parsing_error!(
                format!(
                    "{} cannot be bare, it should have a value (eg. 'size 1', or 'size \"50%\"')",
                    value_name
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
                ConfigError::new_layout_kdl_error(
                    "Plugins must have a location".into(),
                    plugin_block.span().offset(),
                    plugin_block.span().len(),
                ),
            )?;
        let url_node = kdl_get_property_or_child!(plugin_block, "location").ok_or(
            ConfigError::new_layout_kdl_error(
                "Plugins must have a location".into(),
                plugin_block.span().offset(),
                plugin_block.span().len(),
            ),
        )?;
        let configuration = KdlLayoutParser::parse_plugin_user_configuration(&plugin_block)?;
        let initial_cwd =
            kdl_get_string_property_or_child_value!(&plugin_block, "cwd").map(|s| PathBuf::from(s));
        let cwd = self.cwd_prefix(initial_cwd.as_ref())?;
        let run_plugin_or_alias = RunPluginOrAlias::from_url(
            &string_url,
            &Some(configuration.inner().clone()),
            None,
            cwd.clone(),
        )
        .map_err(|e| {
            ConfigError::new_kdl_error(
                format!("Failed to parse plugin: {}", e),
                url_node.span().offset(),
                url_node.span().len(),
            )
        })?
        .with_initial_cwd(cwd);
        Ok(Some(Run::Plugin(run_plugin_or_alias)))
    }
    pub fn parse_plugin_user_configuration(
        plugin_block: &KdlNode,
    ) -> Result<PluginUserConfiguration, ConfigError> {
        let mut configuration = BTreeMap::new();
        for user_configuration_entry in plugin_block.entries() {
            let name = user_configuration_entry.name();
            let value = user_configuration_entry.value();
            if let Some(name) = name {
                let name = name.to_string();
                if KdlLayoutParser::is_a_reserved_plugin_property(&name) {
                    continue;
                }
                configuration.insert(name, value.to_string());
            }
            // we ignore "bare" (eg. `plugin i_am_a_bare_true_argument { arg_one 1; }`) entries
            // to prevent diverging behaviour with the keybindings config
        }
        if let Some(user_config) = kdl_children_nodes!(plugin_block) {
            for user_configuration_entry in user_config {
                let config_entry_name = kdl_name!(user_configuration_entry);
                if KdlLayoutParser::is_a_reserved_plugin_property(&config_entry_name) {
                    continue;
                }
                let config_entry_str_value = kdl_first_entry_as_string!(user_configuration_entry)
                    .map(|s| format!("{}", s.to_string()));
                let config_entry_int_value = kdl_first_entry_as_i64!(user_configuration_entry)
                    .map(|s| format!("{}", s.to_string()));
                let config_entry_bool_value = kdl_first_entry_as_bool!(user_configuration_entry)
                    .map(|s| format!("{}", s.to_string()));
                let config_entry_children = user_configuration_entry
                    .children()
                    .map(|s| format!("{}", s.to_string().trim()));
                let config_entry_value = config_entry_str_value
                    .or(config_entry_int_value)
                    .or(config_entry_bool_value)
                    .or(config_entry_children)
                    .ok_or(ConfigError::new_kdl_error(
                        format!(
                            "Failed to parse plugin block configuration: {:?}",
                            user_configuration_entry
                        ),
                        plugin_block.span().offset(),
                        plugin_block.span().len(),
                    ))?;
                configuration.insert(config_entry_name.into(), config_entry_value);
            }
        }
        Ok(PluginUserConfiguration::new(configuration))
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
    fn cwd_prefix(&self, tab_cwd: Option<&PathBuf>) -> Result<Option<PathBuf>, ConfigError> {
        Ok(match (&self.global_cwd, tab_cwd) {
            (Some(global_cwd), Some(tab_cwd)) => Some(global_cwd.join(tab_cwd)),
            (None, Some(tab_cwd)) => Some(tab_cwd.clone()),
            (Some(global_cwd), None) => Some(global_cwd.clone()),
            (None, None) => None,
        })
    }
    fn parse_path(
        &self,
        kdl_node: &KdlNode,
        name: &'static str,
    ) -> Result<Option<PathBuf>, ConfigError> {
        match kdl_get_string_property_or_child_value_with_error!(kdl_node, name) {
            Some(s) => match shellexpand::full(s) {
                Ok(s) => Ok(Some(PathBuf::from(s.as_ref()))),
                Err(e) => Err(kdl_parsing_error!(e.to_string(), kdl_node)),
            },
            None => Ok(None),
        }
    }
    fn parse_pane_command(
        &self,
        pane_node: &KdlNode,
        is_template: bool,
    ) -> Result<Option<Run>, ConfigError> {
        let command = self.parse_path(pane_node, "command")?;
        let edit = self.parse_path(pane_node, "edit")?;
        let cwd = self.parse_path(pane_node, "cwd")?;
        let args = self.parse_args(pane_node)?;
        let close_on_exit =
            kdl_get_bool_property_or_child_value_with_error!(pane_node, "close_on_exit");
        let start_suspended =
            kdl_get_bool_property_or_child_value_with_error!(pane_node, "start_suspended");
        if !is_template {
            self.assert_no_bare_attributes_in_pane_node(
                &command,
                &args,
                &close_on_exit,
                &start_suspended,
                pane_node,
            )?;
        }
        let hold_on_close = close_on_exit.map(|c| !c).unwrap_or(true);
        let hold_on_start = start_suspended.map(|c| c).unwrap_or(false);
        match (command, edit, cwd) {
            (None, None, Some(cwd)) => Ok(Some(Run::Cwd(cwd))),
            (Some(command), None, cwd) => Ok(Some(Run::Command(RunCommand {
                command,
                args: args.unwrap_or_else(|| vec![]),
                cwd,
                hold_on_close,
                hold_on_start,
                ..Default::default()
            }))),
            (None, Some(edit), Some(cwd)) => {
                Ok(Some(Run::EditFile(cwd.join(edit), None, Some(cwd))))
            },
            (None, Some(edit), None) => Ok(Some(Run::EditFile(edit, None, None))),
            (Some(_command), Some(_edit), _) => Err(ConfigError::new_layout_kdl_error(
                "cannot have both a command and an edit instruction for the same pane".into(),
                pane_node.span().offset(),
                pane_node.span().len(),
            )),
            _ => Ok(None),
        }
    }
    fn parse_command_plugin_or_edit_block(
        &self,
        kdl_node: &KdlNode,
    ) -> Result<Option<Run>, ConfigError> {
        let mut run = self.parse_pane_command(kdl_node, false)?;
        if let Some(plugin_block) = kdl_get_child!(kdl_node, "plugin") {
            let has_non_cwd_run_prop = run
                .map(|r| match r {
                    Run::Cwd(_) => false,
                    _ => true,
                })
                .unwrap_or(false);
            if has_non_cwd_run_prop {
                return Err(ConfigError::new_layout_kdl_error(
                    "Cannot have both a command/edit and a plugin block for a single pane".into(),
                    plugin_block.span().offset(),
                    plugin_block.span().len(),
                ));
            }
            run = self.parse_plugin_block(plugin_block)?;
        }
        Ok(run)
    }
    fn parse_command_plugin_or_edit_block_for_template(
        &self,
        kdl_node: &KdlNode,
    ) -> Result<Option<Run>, ConfigError> {
        let mut run = self.parse_pane_command(kdl_node, true)?;
        if let Some(plugin_block) = kdl_get_child!(kdl_node, "plugin") {
            let has_non_cwd_run_prop = run
                .map(|r| match r {
                    Run::Cwd(_) => false,
                    _ => true,
                })
                .unwrap_or(false);
            if has_non_cwd_run_prop {
                return Err(ConfigError::new_layout_kdl_error(
                    "Cannot have both a command/edit and a plugin block for a single pane".into(),
                    plugin_block.span().offset(),
                    plugin_block.span().len(),
                ));
            }
            run = self.parse_plugin_block(plugin_block)?;
        }
        Ok(run)
    }
    fn parse_pane_node(
        &self,
        kdl_node: &KdlNode,
        is_part_of_stack: bool,
    ) -> Result<TiledPaneLayout, ConfigError> {
        self.assert_valid_pane_properties(kdl_node)?;
        let children_are_stacked =
            kdl_get_bool_property_or_child_value_with_error!(kdl_node, "stacked").unwrap_or(false);
        let is_expanded_in_stack =
            kdl_get_bool_property_or_child_value_with_error!(kdl_node, "expanded").unwrap_or(false);
        let borderless = kdl_get_bool_property_or_child_value_with_error!(kdl_node, "borderless");
        let focus = kdl_get_bool_property_or_child_value_with_error!(kdl_node, "focus");
        let name = kdl_get_string_property_or_child_value_with_error!(kdl_node, "name")
            .map(|name| name.to_string());
        let exclude_from_sync =
            kdl_get_bool_property_or_child_value_with_error!(kdl_node, "exclude_from_sync");
        let contents_file =
            kdl_get_string_property_or_child_value_with_error!(kdl_node, "contents_file");
        let split_size = self.parse_split_size(kdl_node)?;
        let run = self.parse_command_plugin_or_edit_block(kdl_node)?;
        let children_split_direction = self.parse_split_direction(kdl_node)?;
        let (external_children_index, children) = match kdl_children_nodes!(kdl_node) {
            Some(children) => {
                self.assert_no_grandchildren_in_stack(&children, is_part_of_stack)?;
                self.parse_child_pane_nodes_for_pane(&children, children_are_stacked)?
            },
            None => (None, vec![]),
        };
        if children_are_stacked && external_children_index.is_none() && children.is_empty() {
            return Err(ConfigError::new_layout_kdl_error(
                format!("A stacked pane must have children nodes or possibly a \"children\" node if in a swap_layout"),
                kdl_node.span().offset(),
                kdl_node.span().len(),
            ));
        } else if children_are_stacked && children_split_direction == SplitDirection::Vertical {
            return Err(ConfigError::new_layout_kdl_error(
                format!("Stacked panes cannot be vertical"),
                kdl_node.span().offset(),
                kdl_node.span().len(),
            ));
        } else if is_expanded_in_stack && !is_part_of_stack {
            return Err(ConfigError::new_layout_kdl_error(
                format!("An expanded pane must be part of a stack"),
                kdl_node.span().offset(),
                kdl_node.span().len(),
            ));
        }
        self.assert_no_mixed_children_and_properties(kdl_node)?;
        let pane_initial_contents = contents_file.and_then(|contents_file| {
            self.file_name
                .as_ref()
                .and_then(|f| f.parent())
                .and_then(|parent_folder| {
                    std::fs::read_to_string(parent_folder.join(contents_file)).ok()
                })
        });
        Ok(TiledPaneLayout {
            borderless: borderless.unwrap_or_default(),
            focus,
            name,
            split_size,
            run,
            children_split_direction,
            external_children_index,
            exclude_from_sync,
            children,
            children_are_stacked,
            is_expanded_in_stack,
            pane_initial_contents,
            ..Default::default()
        })
    }
    fn parse_floating_pane_node(
        &self,
        kdl_node: &KdlNode,
    ) -> Result<FloatingPaneLayout, ConfigError> {
        self.assert_valid_floating_pane_properties(kdl_node)?;
        let height = self.parse_percent_or_fixed(kdl_node, "height", false)?;
        let width = self.parse_percent_or_fixed(kdl_node, "width", false)?;
        let x = self.parse_percent_or_fixed(kdl_node, "x", true)?;
        let y = self.parse_percent_or_fixed(kdl_node, "y", true)?;
        let pinned = kdl_get_bool_property_or_child_value_with_error!(kdl_node, "pinned");
        let run = self.parse_command_plugin_or_edit_block(kdl_node)?;
        let focus = kdl_get_bool_property_or_child_value_with_error!(kdl_node, "focus");
        let name = kdl_get_string_property_or_child_value_with_error!(kdl_node, "name")
            .map(|name| name.to_string());
        let contents_file =
            kdl_get_string_property_or_child_value_with_error!(kdl_node, "contents_file");
        self.assert_no_mixed_children_and_properties(kdl_node)?;
        let pane_initial_contents = contents_file.and_then(|contents_file| {
            self.file_name
                .as_ref()
                .and_then(|f| f.parent())
                .and_then(|parent_folder| {
                    std::fs::read_to_string(parent_folder.join(contents_file)).ok()
                })
        });
        Ok(FloatingPaneLayout {
            name,
            height,
            width,
            x,
            y,
            run,
            focus,
            pinned,
            pane_initial_contents,
            ..Default::default()
        })
    }
    fn insert_children_to_pane_template(
        &self,
        kdl_node: &KdlNode,
        pane_template: &mut TiledPaneLayout,
        pane_template_kdl_node: &KdlNode,
    ) -> Result<(), ConfigError> {
        let children_are_stacked =
            kdl_get_bool_property_or_child_value_with_error!(kdl_node, "stacked")
                .unwrap_or(pane_template.children_are_stacked);
        let is_expanded_in_stack =
            kdl_get_bool_property_or_child_value_with_error!(kdl_node, "expanded")
                .unwrap_or(pane_template.children_are_stacked);
        let children_split_direction = self.parse_split_direction(kdl_node)?;
        let (external_children_index, pane_parts) = match kdl_children_nodes!(kdl_node) {
            Some(children) => {
                self.parse_child_pane_nodes_for_pane(&children, children_are_stacked)?
            },
            None => (None, vec![]),
        };
        if pane_parts.len() > 0 {
            let child_panes_layout = TiledPaneLayout {
                children_split_direction,
                children: pane_parts,
                external_children_index,
                children_are_stacked,
                is_expanded_in_stack,
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
    fn populate_external_children_index(
        &self,
        kdl_node: &KdlNode,
    ) -> Result<Option<usize>, ConfigError> {
        // Option<external_children_index>
        if let Some(pane_child_nodes) = kdl_children_nodes!(kdl_node) {
            for (i, child) in pane_child_nodes.iter().enumerate() {
                if kdl_name!(child) == "children" {
                    if let Some(grand_children) = kdl_children_nodes!(child) {
                        let grand_children: Vec<&str> =
                            grand_children.iter().map(|g| kdl_name!(g)).collect();
                        if !grand_children.is_empty() {
                            return Err(ConfigError::new_layout_kdl_error(
                                format!(
                                    "Invalid `children` properties: {}",
                                    grand_children.join(", ")
                                ),
                                child.span().offset(),
                                child.span().len(),
                            ));
                        }
                    }
                    return Ok(Some(i));
                }
            }
        }
        return Ok(None);
    }
    fn parse_pane_node_with_template(
        &self,
        kdl_node: &KdlNode,
        pane_template: PaneOrFloatingPane,
        should_mark_external_children_index: bool,
        pane_template_kdl_node: &KdlNode,
    ) -> Result<TiledPaneLayout, ConfigError> {
        match pane_template {
            PaneOrFloatingPane::Pane(mut pane_template)
            | PaneOrFloatingPane::Either(mut pane_template) => {
                let borderless =
                    kdl_get_bool_property_or_child_value_with_error!(kdl_node, "borderless");
                let focus = kdl_get_bool_property_or_child_value_with_error!(kdl_node, "focus");
                let name = kdl_get_string_property_or_child_value_with_error!(kdl_node, "name")
                    .map(|name| name.to_string());
                let children_are_stacked =
                    kdl_get_bool_property_or_child_value_with_error!(kdl_node, "stacked");
                let is_expanded_in_stack =
                    kdl_get_bool_property_or_child_value_with_error!(kdl_node, "expanded");
                let args = self.parse_args(kdl_node)?;
                let close_on_exit =
                    kdl_get_bool_property_or_child_value_with_error!(kdl_node, "close_on_exit");
                let start_suspended =
                    kdl_get_bool_property_or_child_value_with_error!(kdl_node, "start_suspended");
                let split_size = self.parse_split_size(kdl_node)?;
                let run = self.parse_command_plugin_or_edit_block_for_template(kdl_node)?;
                let exclude_from_sync =
                    kdl_get_bool_property_or_child_value_with_error!(kdl_node, "exclude_from_sync");

                let external_children_index = if should_mark_external_children_index {
                    self.populate_external_children_index(kdl_node)?
                } else {
                    None
                };
                self.assert_no_bare_attributes_in_pane_node_with_template(
                    &run,
                    &pane_template.run,
                    &args,
                    &close_on_exit,
                    &start_suspended,
                    kdl_node,
                )?;
                self.insert_children_to_pane_template(
                    kdl_node,
                    &mut pane_template,
                    pane_template_kdl_node,
                )?;
                pane_template.run = Run::merge(&pane_template.run, &run);
                if let Some(pane_template_run_command) = pane_template.run.as_mut() {
                    // we need to do this because panes consuming a pane_template
                    // can have bare args without a command
                    pane_template_run_command.add_args(args);
                    pane_template_run_command.add_close_on_exit(close_on_exit);
                    pane_template_run_command.add_start_suspended(start_suspended);
                };
                if let Some(borderless) = borderless {
                    pane_template.borderless = borderless;
                }
                if let Some(focus) = focus {
                    pane_template.focus = Some(focus);
                }
                if let Some(name) = name {
                    pane_template.name = Some(name);
                }
                if let Some(exclude_from_sync) = exclude_from_sync {
                    pane_template.exclude_from_sync = Some(exclude_from_sync);
                }
                if let Some(split_size) = split_size {
                    pane_template.split_size = Some(split_size);
                }
                if let Some(index_of_children) = pane_template.external_children_index {
                    pane_template.children.insert(
                        index_of_children,
                        TiledPaneLayout {
                            children_are_stacked: children_are_stacked.unwrap_or_default(),
                            ..Default::default()
                        },
                    );
                }
                if let Some(children_are_stacked) = children_are_stacked {
                    pane_template.children_are_stacked = children_are_stacked;
                }
                if let Some(is_expanded_in_stack) = is_expanded_in_stack {
                    pane_template.is_expanded_in_stack = is_expanded_in_stack;
                }
                pane_template.external_children_index = external_children_index;
                Ok(pane_template)
            },
            PaneOrFloatingPane::FloatingPane(_) => {
                let pane_template_name = kdl_get_string_property_or_child_value_with_error!(
                    pane_template_kdl_node,
                    "name"
                )
                .map(|name| name.to_string());
                Err(ConfigError::new_layout_kdl_error(
                    format!("pane_template {}, is a floating pane template (derived from its properties) and cannot be applied to a tiled pane", pane_template_name.unwrap_or("".into())),
                    kdl_node.span().offset(),
                    kdl_node.span().len(),
                ))
            },
        }
    }
    fn parse_floating_pane_node_with_template(
        &self,
        kdl_node: &KdlNode,
        pane_template: PaneOrFloatingPane,
        pane_template_kdl_node: &KdlNode,
    ) -> Result<FloatingPaneLayout, ConfigError> {
        match pane_template {
            PaneOrFloatingPane::Pane(_) => {
                let pane_template_name = kdl_get_string_property_or_child_value_with_error!(
                    pane_template_kdl_node,
                    "name"
                )
                .map(|name| name.to_string());
                Err(ConfigError::new_layout_kdl_error(
                    format!("pane_template {}, is a non-floating pane template (derived from its properties) and cannot be applied to a floating pane", pane_template_name.unwrap_or("".into())),
                    kdl_node.span().offset(),
                    kdl_node.span().len(),
                ))
            },
            PaneOrFloatingPane::FloatingPane(mut pane_template) => {
                let focus = kdl_get_bool_property_or_child_value_with_error!(kdl_node, "focus");
                let name = kdl_get_string_property_or_child_value_with_error!(kdl_node, "name")
                    .map(|name| name.to_string());
                let args = self.parse_args(kdl_node)?;
                let close_on_exit =
                    kdl_get_bool_property_or_child_value_with_error!(kdl_node, "close_on_exit");
                let start_suspended =
                    kdl_get_bool_property_or_child_value_with_error!(kdl_node, "start_suspended");
                let run = self.parse_command_plugin_or_edit_block_for_template(kdl_node)?;
                self.assert_no_bare_attributes_in_pane_node_with_template(
                    &run,
                    &pane_template.run,
                    &args,
                    &close_on_exit,
                    &start_suspended,
                    kdl_node,
                )?;
                pane_template.run = Run::merge(&pane_template.run, &run);
                if let Some(pane_template_run_command) = pane_template.run.as_mut() {
                    // we need to do this because panes consuming a pane_template
                    // can have bare args without a command
                    pane_template_run_command.add_args(args);
                    pane_template_run_command.add_close_on_exit(close_on_exit);
                    pane_template_run_command.add_start_suspended(start_suspended);
                };
                if let Some(focus) = focus {
                    pane_template.focus = Some(focus);
                }
                if let Some(name) = name {
                    pane_template.name = Some(name);
                }
                let height = self.parse_percent_or_fixed(kdl_node, "height", false)?;
                let width = self.parse_percent_or_fixed(kdl_node, "width", false)?;
                let x = self.parse_percent_or_fixed(kdl_node, "x", true)?;
                let y = self.parse_percent_or_fixed(kdl_node, "y", true)?;
                let pinned = kdl_get_bool_property_or_child_value_with_error!(kdl_node, "pinned");
                if let Some(height) = height {
                    pane_template.height = Some(height);
                }
                if let Some(width) = width {
                    pane_template.width = Some(width);
                }
                if let Some(y) = y {
                    pane_template.y = Some(y);
                }
                if let Some(x) = x {
                    pane_template.x = Some(x);
                }
                if let Some(pinned) = pinned {
                    pane_template.pinned = Some(pinned);
                }
                Ok(pane_template)
            },
            PaneOrFloatingPane::Either(mut pane_template) => {
                let focus = kdl_get_bool_property_or_child_value_with_error!(kdl_node, "focus");
                let name = kdl_get_string_property_or_child_value_with_error!(kdl_node, "name")
                    .map(|name| name.to_string());
                let args = self.parse_args(kdl_node)?;
                let close_on_exit =
                    kdl_get_bool_property_or_child_value_with_error!(kdl_node, "close_on_exit");
                let start_suspended =
                    kdl_get_bool_property_or_child_value_with_error!(kdl_node, "start_suspended");
                let run = self.parse_command_plugin_or_edit_block_for_template(kdl_node)?;
                self.assert_no_bare_attributes_in_pane_node_with_template(
                    &run,
                    &pane_template.run,
                    &args,
                    &close_on_exit,
                    &start_suspended,
                    kdl_node,
                )?;
                pane_template.run = Run::merge(&pane_template.run, &run);
                if let Some(pane_template_run_command) = pane_template.run.as_mut() {
                    // we need to do this because panes consuming a pane_template
                    // can have bare args without a command
                    pane_template_run_command.add_args(args);
                    pane_template_run_command.add_close_on_exit(close_on_exit);
                    pane_template_run_command.add_start_suspended(start_suspended);
                };
                if let Some(focus) = focus {
                    pane_template.focus = Some(focus);
                }
                if let Some(name) = name {
                    pane_template.name = Some(name);
                }
                let height = self.parse_percent_or_fixed(kdl_node, "height", false)?;
                let width = self.parse_percent_or_fixed(kdl_node, "width", false)?;
                let x = self.parse_percent_or_fixed(kdl_node, "x", true)?;
                let y = self.parse_percent_or_fixed(kdl_node, "y", true)?;
                let pinned = kdl_get_bool_property_or_child_value_with_error!(kdl_node, "pinned");
                let mut floating_pane = FloatingPaneLayout::from(&pane_template);
                if let Some(height) = height {
                    floating_pane.height = Some(height);
                }
                if let Some(width) = width {
                    floating_pane.width = Some(width);
                }
                if let Some(y) = y {
                    floating_pane.y = Some(y);
                }
                if let Some(x) = x {
                    floating_pane.x = Some(x);
                }
                if let Some(pinned) = pinned {
                    floating_pane.pinned = Some(pinned);
                }
                Ok(floating_pane)
            },
        }
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
    fn has_only_neutral_pane_template_properties(
        &self,
        kdl_node: &KdlNode,
    ) -> Result<bool, ConfigError> {
        // pane properties
        let borderless = kdl_get_bool_property_or_child_value_with_error!(kdl_node, "borderless");
        let children_are_stacked =
            kdl_get_bool_property_or_child_value_with_error!(kdl_node, "stacked");
        let is_expanded_in_stack =
            kdl_get_bool_property_or_child_value_with_error!(kdl_node, "expanded");
        let split_size = self.parse_split_size(kdl_node)?;
        let split_direction =
            kdl_get_string_property_or_child_value_with_error!(kdl_node, "split_direction");
        let has_children_nodes = self.has_child_nodes(kdl_node);

        // floating pane properties
        let height = self.parse_percent_or_fixed(kdl_node, "height", false)?;
        let width = self.parse_percent_or_fixed(kdl_node, "width", false)?;
        let x = self.parse_percent_or_fixed(kdl_node, "x", true)?;
        let y = self.parse_percent_or_fixed(kdl_node, "y", true)?;
        let pinned = kdl_get_string_property_or_child_value_with_error!(kdl_node, "pinned");

        let has_pane_properties = borderless.is_some()
            || split_size.is_some()
            || split_direction.is_some()
            || children_are_stacked.is_some()
            || is_expanded_in_stack.is_some()
            || has_children_nodes;
        let has_floating_pane_properties =
            height.is_some() || width.is_some() || x.is_some() || y.is_some() || pinned.is_some();
        if has_pane_properties || has_floating_pane_properties {
            Ok(false)
        } else {
            Ok(true)
        }
    }
    fn differentiate_pane_and_floating_pane_template(
        &self,
        kdl_node: &KdlNode,
    ) -> Result<bool, ConfigError> {
        // returns true if it's a floating_pane template, false if not

        // pane properties
        let borderless = kdl_get_bool_property_or_child_value_with_error!(kdl_node, "borderless");
        let children_are_stacked =
            kdl_get_bool_property_or_child_value_with_error!(kdl_node, "stacked");
        let is_expanded_in_stack =
            kdl_get_bool_property_or_child_value_with_error!(kdl_node, "expanded");
        let split_size = self.parse_split_size(kdl_node)?;
        let split_direction =
            kdl_get_string_property_or_child_value_with_error!(kdl_node, "split_direction");
        let has_children_nodes = self.has_child_nodes(kdl_node);

        // floating pane properties
        let height = self.parse_percent_or_fixed(kdl_node, "height", false)?;
        let width = self.parse_percent_or_fixed(kdl_node, "width", false)?;
        let x = self.parse_percent_or_fixed(kdl_node, "x", true)?;
        let y = self.parse_percent_or_fixed(kdl_node, "y", true)?;
        let pinned = kdl_get_bool_property_or_child_value_with_error!(kdl_node, "pinned");

        let has_pane_properties = borderless.is_some()
            || split_size.is_some()
            || split_direction.is_some()
            || children_are_stacked.is_some()
            || is_expanded_in_stack.is_some()
            || has_children_nodes;
        let has_floating_pane_properties =
            height.is_some() || width.is_some() || x.is_some() || y.is_some() || pinned.is_some();

        if has_pane_properties && has_floating_pane_properties {
            let mut pane_properties = vec![];
            if borderless.is_some() {
                pane_properties.push("borderless");
            }
            if children_are_stacked.is_some() {
                pane_properties.push("stacked");
            }
            if is_expanded_in_stack.is_some() {
                pane_properties.push("expanded");
            }
            if split_size.is_some() {
                pane_properties.push("split_size");
            }
            if split_direction.is_some() {
                pane_properties.push("split_direction");
            }
            if has_children_nodes {
                pane_properties.push("child nodes");
            }
            let mut floating_pane_properties = vec![];
            if height.is_some() {
                floating_pane_properties.push("height");
            }
            if width.is_some() {
                floating_pane_properties.push("width");
            }
            if x.is_some() {
                floating_pane_properties.push("x");
            }
            if y.is_some() {
                floating_pane_properties.push("y");
            }
            if pinned.is_some() {
                floating_pane_properties.push("pinned");
            }
            Err(ConfigError::new_layout_kdl_error(
                format!(
                    "A pane_template cannot have both pane ({}) and floating pane ({}) properties",
                    pane_properties.join(", "),
                    floating_pane_properties.join(", ")
                ),
                kdl_node.span().offset(),
                kdl_node.span().len(),
            ))
        } else if has_floating_pane_properties {
            Ok(true)
        } else {
            Ok(false)
        }
    }
    fn parse_pane_template_node(&mut self, kdl_node: &KdlNode) -> Result<(), ConfigError> {
        let template_name = kdl_get_string_property_or_child_value!(kdl_node, "name")
            .map(|s| s.to_string())
            .ok_or(ConfigError::new_layout_kdl_error(
                "Pane templates must have a name".into(),
                kdl_node.span().offset(),
                kdl_node.span().len(),
            ))?;
        self.assert_legal_node_name(&template_name, kdl_node)?;
        self.assert_legal_template_name(&template_name, kdl_node)?;
        let focus = kdl_get_bool_property_or_child_value_with_error!(kdl_node, "focus");
        let run = self.parse_command_plugin_or_edit_block(kdl_node)?;

        let is_floating = self.differentiate_pane_and_floating_pane_template(&kdl_node)?;
        let can_be_either_floating_or_tiled =
            self.has_only_neutral_pane_template_properties(&kdl_node)?;
        if can_be_either_floating_or_tiled {
            self.assert_valid_pane_or_floating_pane_properties(kdl_node)?;
            self.pane_templates.insert(
                template_name,
                (
                    PaneOrFloatingPane::Either(TiledPaneLayout {
                        focus,
                        run,
                        ..Default::default()
                    }),
                    kdl_node.clone(),
                ),
            );
        } else if is_floating {
            self.assert_valid_floating_pane_properties(kdl_node)?;
            // floating pane properties
            let height = self.parse_percent_or_fixed(kdl_node, "height", false)?;
            let width = self.parse_percent_or_fixed(kdl_node, "width", false)?;
            let x = self.parse_percent_or_fixed(kdl_node, "x", true)?;
            let y = self.parse_percent_or_fixed(kdl_node, "y", true)?;
            let pinned = kdl_get_bool_property_or_child_value_with_error!(kdl_node, "pinned");
            self.pane_templates.insert(
                template_name,
                (
                    PaneOrFloatingPane::FloatingPane(FloatingPaneLayout {
                        focus,
                        run,
                        height,
                        width,
                        x,
                        y,
                        pinned,
                        ..Default::default()
                    }),
                    kdl_node.clone(),
                ),
            );
        } else {
            self.assert_valid_pane_properties(kdl_node)?;
            // pane properties
            let borderless =
                kdl_get_bool_property_or_child_value_with_error!(kdl_node, "borderless");
            let children_are_stacked =
                kdl_get_bool_property_or_child_value_with_error!(kdl_node, "stacked")
                    .unwrap_or(false);
            let is_expanded_in_stack =
                kdl_get_bool_property_or_child_value_with_error!(kdl_node, "expanded")
                    .unwrap_or(false);
            let split_size = self.parse_split_size(kdl_node)?;
            let children_split_direction = self.parse_split_direction(kdl_node)?;
            let (external_children_index, pane_parts) = match kdl_children_nodes!(kdl_node) {
                Some(children) => {
                    self.parse_child_pane_nodes_for_pane(&children, children_are_stacked)?
                },
                None => (None, vec![]),
            };
            self.assert_no_mixed_children_and_properties(kdl_node)?;
            self.pane_templates.insert(
                template_name,
                (
                    PaneOrFloatingPane::Pane(TiledPaneLayout {
                        borderless: borderless.unwrap_or_default(),
                        focus,
                        split_size,
                        run,
                        children_split_direction,
                        external_children_index,
                        children: pane_parts,
                        children_are_stacked,
                        is_expanded_in_stack,
                        ..Default::default()
                    }),
                    kdl_node.clone(),
                ),
            );
        }

        Ok(())
    }
    fn parse_tab_node(
        &mut self,
        kdl_node: &KdlNode,
    ) -> Result<
        (
            bool,
            Option<String>,
            TiledPaneLayout,
            Vec<FloatingPaneLayout>,
        ),
        ConfigError,
    > {
        // (is_focused, Option<tab_name>, PaneLayout, Vec<FloatingPaneLayout>)
        self.assert_valid_tab_properties(kdl_node)?;
        let tab_name =
            kdl_get_string_property_or_child_value!(kdl_node, "name").map(|s| s.to_string());
        let tab_cwd = self.parse_path(kdl_node, "cwd")?;
        let is_focused = kdl_get_bool_property_or_child_value!(kdl_node, "focus").unwrap_or(false);
        let hide_floating_panes =
            kdl_get_bool_property_or_child_value!(kdl_node, "hide_floating_panes").unwrap_or(false);
        let children_split_direction = self.parse_split_direction(kdl_node)?;
        let mut child_floating_panes = vec![];
        let children = match kdl_children_nodes!(kdl_node) {
            Some(children) => {
                let should_mark_external_children_index = false;
                self.parse_child_pane_nodes_for_tab(
                    children,
                    should_mark_external_children_index,
                    &mut child_floating_panes,
                )?
            },
            None => vec![],
        };
        let mut pane_layout = TiledPaneLayout {
            children_split_direction,
            children,
            hide_floating_panes,
            ..Default::default()
        };
        if let Some(cwd_prefix) = &self.cwd_prefix(tab_cwd.as_ref())? {
            pane_layout.add_cwd_to_layout(&cwd_prefix);
            for floating_pane in child_floating_panes.iter_mut() {
                floating_pane.add_cwd_to_layout(&cwd_prefix);
            }
        }
        Ok((is_focused, tab_name, pane_layout, child_floating_panes))
    }
    fn parse_child_pane_nodes_for_tab(
        &self,
        children: &[KdlNode],
        should_mark_external_children_index: bool,
        child_floating_panes: &mut Vec<FloatingPaneLayout>,
    ) -> Result<Vec<TiledPaneLayout>, ConfigError> {
        let mut nodes = vec![];
        let is_part_of_stack = false;
        for child in children {
            if kdl_name!(child) == "pane" {
                nodes.push(self.parse_pane_node(child, is_part_of_stack)?);
            } else if let Some((pane_template, pane_template_kdl_node)) =
                self.pane_templates.get(kdl_name!(child)).cloned()
            {
                nodes.push(self.parse_pane_node_with_template(
                    child,
                    pane_template,
                    should_mark_external_children_index,
                    &pane_template_kdl_node,
                )?);
            } else if kdl_name!(child) == "floating_panes" {
                self.populate_floating_pane_children(child, child_floating_panes)?;
            } else if self.is_a_valid_tab_property(kdl_name!(child)) {
                return Err(ConfigError::new_layout_kdl_error(
                    format!("Tab property '{}' must be placed on the tab title line and not in the child braces", kdl_name!(child)),
                    child.span().offset(),
                    child.span().len()
                ));
            } else {
                return Err(ConfigError::new_layout_kdl_error(
                    format!("Invalid tab property: {}", kdl_name!(child)),
                    child.span().offset(),
                    child.span().len(),
                ));
            }
        }
        if nodes.is_empty() {
            nodes.push(TiledPaneLayout::default());
        }
        Ok(nodes)
    }
    fn parse_child_pane_nodes_for_pane(
        &self,
        children: &[KdlNode],
        is_part_of_stack: bool,
    ) -> Result<(Option<usize>, Vec<TiledPaneLayout>), ConfigError> {
        // usize is external_children_index
        let mut external_children_index = None;
        let mut nodes = vec![];
        for (i, child) in children.iter().enumerate() {
            if kdl_name!(child) == "pane" {
                nodes.push(self.parse_pane_node(child, is_part_of_stack)?);
            } else if kdl_name!(child) == "children" {
                if let Some(grand_children) = kdl_children_nodes!(child) {
                    let grand_children: Vec<&str> =
                        grand_children.iter().map(|g| kdl_name!(g)).collect();
                    if !grand_children.is_empty() {
                        return Err(ConfigError::new_layout_kdl_error(
                            format!(
                                "Invalid `children` properties: {}",
                                grand_children.join(", ")
                            ),
                            child.span().offset(),
                            child.span().len(),
                        ));
                    }
                }
                external_children_index = Some(i);
            } else if let Some((pane_template, pane_template_kdl_node)) =
                self.pane_templates.get(kdl_name!(child)).cloned()
            {
                let should_mark_external_children_index = false;
                nodes.push(self.parse_pane_node_with_template(
                    child,
                    pane_template,
                    should_mark_external_children_index,
                    &pane_template_kdl_node,
                )?);
            } else if !self.is_a_valid_pane_property(kdl_name!(child)) {
                return Err(ConfigError::new_layout_kdl_error(
                    format!("Unknown pane property: {}", kdl_name!(child)),
                    child.span().offset(),
                    child.span().len(),
                ));
            }
        }
        Ok((external_children_index, nodes))
    }
    fn has_child_nodes(&self, kdl_node: &KdlNode) -> bool {
        if let Some(children) = kdl_children_nodes!(kdl_node) {
            for child in children {
                if kdl_name!(child) == "pane"
                    || kdl_name!(child) == "children"
                    || self.pane_templates.get(kdl_name!(child)).is_some()
                {
                    return true;
                }
            }
        }
        return false;
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
    fn assert_no_bare_attributes_in_pane_node_with_template(
        &self,
        pane_run: &Option<Run>,
        pane_template_run: &Option<Run>,
        args: &Option<Vec<String>>,
        close_on_exit: &Option<bool>,
        start_suspended: &Option<bool>,
        pane_node: &KdlNode,
    ) -> Result<(), ConfigError> {
        if let (None, None, true) = (pane_run, pane_template_run, args.is_some()) {
            return Err(kdl_parsing_error!(
                format!("args can only be specified if a command was specified either in the pane_template or in the pane"),
                pane_node
            ));
        }
        if let (None, None, true) = (pane_run, pane_template_run, close_on_exit.is_some()) {
            return Err(kdl_parsing_error!(
                format!("close_on_exit can only be specified if a command was specified either in the pane_template or in the pane"),
                pane_node
            ));
        }
        if let (None, None, true) = (pane_run, pane_template_run, start_suspended.is_some()) {
            return Err(kdl_parsing_error!(
                format!("start_suspended can only be specified if a command was specified either in the pane_template or in the pane"),
                pane_node
            ));
        }
        Ok(())
    }
    fn assert_no_bare_attributes_in_pane_node(
        &self,
        command: &Option<PathBuf>,
        args: &Option<Vec<String>>,
        close_on_exit: &Option<bool>,
        start_suspended: &Option<bool>,
        pane_node: &KdlNode,
    ) -> Result<(), ConfigError> {
        if command.is_none() {
            if close_on_exit.is_some() {
                return Err(ConfigError::new_layout_kdl_error(
                    "close_on_exit can only be set if a command was specified".into(),
                    pane_node.span().offset(),
                    pane_node.span().len(),
                ));
            }
            if start_suspended.is_some() {
                return Err(ConfigError::new_layout_kdl_error(
                    "start_suspended can only be set if a command was specified".into(),
                    pane_node.span().offset(),
                    pane_node.span().len(),
                ));
            }
            if args.is_some() {
                return Err(ConfigError::new_layout_kdl_error(
                    "args can only be set if a command was specified".into(),
                    pane_node.span().offset(),
                    pane_node.span().len(),
                ));
            }
        }
        Ok(())
    }
    fn assert_one_children_block(
        &self,
        layout: &TiledPaneLayout,
        kdl_node: &KdlNode,
    ) -> Result<(), ConfigError> {
        let children_block_count = layout.children_block_count();
        if children_block_count != 1 {
            return Err(ConfigError::new_layout_kdl_error(format!("This template has {} children blocks, only 1 is allowed when used to insert child panes", children_block_count), kdl_node.span().offset(), kdl_node.span().len()));
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
                        return Err(ConfigError::new_layout_kdl_error(
                            format!("Unknown pane property: {}", string_name),
                            entry.span().offset(),
                            entry.span().len(),
                        ));
                    }
                },
                None => {
                    return Err(ConfigError::new_layout_kdl_error(
                        "Unknown pane property".into(),
                        entry.span().offset(),
                        entry.span().len(),
                    ));
                },
            }
        }
        Ok(())
    }
    fn assert_valid_floating_pane_properties(
        &self,
        pane_node: &KdlNode,
    ) -> Result<(), ConfigError> {
        for entry in pane_node.entries() {
            match entry
                .name()
                .map(|e| e.value())
                .or_else(|| entry.value().as_string())
            {
                Some(string_name) => {
                    if !self.is_a_valid_floating_pane_property(string_name) {
                        return Err(ConfigError::new_layout_kdl_error(
                            format!("Unknown floating pane property: {}", string_name),
                            entry.span().offset(),
                            entry.span().len(),
                        ));
                    }
                },
                None => {
                    return Err(ConfigError::new_layout_kdl_error(
                        "Unknown floating pane property".into(),
                        entry.span().offset(),
                        entry.span().len(),
                    ));
                },
            }
        }
        Ok(())
    }
    fn assert_valid_pane_or_floating_pane_properties(
        &self,
        pane_node: &KdlNode,
    ) -> Result<(), ConfigError> {
        for entry in pane_node.entries() {
            match entry
                .name()
                .map(|e| e.value())
                .or_else(|| entry.value().as_string())
            {
                Some(string_name) => {
                    if !self.is_a_valid_floating_pane_property(string_name)
                        || !self.is_a_valid_pane_property(string_name)
                    {
                        return Err(ConfigError::new_layout_kdl_error(
                            format!("Unknown pane property: {}", string_name),
                            entry.span().offset(),
                            entry.span().len(),
                        ));
                    }
                },
                None => {
                    return Err(ConfigError::new_layout_kdl_error(
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
                return Err(ConfigError::new_layout_kdl_error(
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
        let has_cwd_prop = self.parse_path(kdl_node, "cwd")?.is_some();
        let has_non_cwd_run_prop = self
            .parse_command_plugin_or_edit_block(kdl_node)?
            .map(|r| match r {
                Run::Cwd(_) => false,
                _ => true,
            })
            .unwrap_or(false);
        let has_nested_nodes_or_children_block = self.has_child_panes_tabs_or_templates(kdl_node);
        if has_nested_nodes_or_children_block
            && (has_borderless_prop || has_non_cwd_run_prop || has_cwd_prop)
        {
            let mut offending_nodes = vec![];
            if has_borderless_prop {
                offending_nodes.push("borderless");
            }
            if has_non_cwd_run_prop {
                offending_nodes.push("command/edit/plugin");
            }
            if has_cwd_prop {
                offending_nodes.push("cwd");
            }
            Err(ConfigError::new_layout_kdl_error(
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
        layout: &mut TiledPaneLayout,
        mut child_panes_layout: TiledPaneLayout,
        kdl_node: &KdlNode,
    ) -> Result<(), ConfigError> {
        let successfully_inserted = layout.insert_children_layout(&mut child_panes_layout)?;
        if !successfully_inserted {
            Err(ConfigError::new_layout_kdl_error(
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
        mut tab_layout: TiledPaneLayout,
        mut tab_template_floating_panes: Vec<FloatingPaneLayout>,
        should_mark_external_children_index: bool,
        tab_layout_kdl_node: &KdlNode,
    ) -> Result<
        (
            bool,
            Option<String>,
            TiledPaneLayout,
            Vec<FloatingPaneLayout>,
        ),
        ConfigError,
    > {
        // (is_focused, Option<tab_name>, PaneLayout, Vec<FloatingPaneLayout>)
        let tab_name =
            kdl_get_string_property_or_child_value!(kdl_node, "name").map(|s| s.to_string());
        let tab_cwd = self.parse_path(kdl_node, "cwd")?;
        let is_focused = kdl_get_bool_property_or_child_value!(kdl_node, "focus").unwrap_or(false);
        let children_split_direction = self.parse_split_direction(kdl_node)?;
        match kdl_children_nodes!(kdl_node) {
            Some(children) => {
                let child_panes = self.parse_child_pane_nodes_for_tab(
                    children,
                    should_mark_external_children_index,
                    &mut tab_template_floating_panes,
                )?;
                let child_panes_layout = TiledPaneLayout {
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
                        .insert(index_of_children, TiledPaneLayout::default());
                }
            },
        }
        if let Some(cwd_prefix) = self.cwd_prefix(tab_cwd.as_ref())? {
            tab_layout.add_cwd_to_layout(&cwd_prefix);
            for floating_pane in tab_template_floating_panes.iter_mut() {
                floating_pane.add_cwd_to_layout(&cwd_prefix);
            }
        }
        tab_layout.external_children_index = None;
        Ok((
            is_focused,
            tab_name,
            tab_layout,
            tab_template_floating_panes,
        ))
    }
    fn populate_one_tab_template(&mut self, kdl_node: &KdlNode) -> Result<(), ConfigError> {
        let template_name = kdl_get_string_property_or_child_value_with_error!(kdl_node, "name")
            .map(|s| s.to_string())
            .ok_or(ConfigError::new_layout_kdl_error(
                "Tab templates must have a name".into(),
                kdl_node.span().offset(),
                kdl_node.span().len(),
            ))?;
        self.assert_legal_node_name(&template_name, kdl_node)?;
        self.assert_legal_template_name(&template_name, kdl_node)?;
        if self.tab_templates.contains_key(&template_name) {
            return Err(ConfigError::new_layout_kdl_error(
                format!(
                    "Duplicate definition of the \"{}\" tab_template",
                    template_name
                ),
                kdl_node.span().offset(),
                kdl_node.span().len(),
            ));
        }
        if self.pane_templates.contains_key(&template_name) {
            return Err(ConfigError::new_layout_kdl_error(
                format!("There is already a pane_template with the name \"{}\" - can't have a tab_template with the same name", template_name),
                kdl_node.span().offset(),
                kdl_node.span().len(),
            ));
        }
        let (tab_template, tab_template_floating_panes) = self.parse_tab_template_node(kdl_node)?;
        self.tab_templates.insert(
            template_name,
            (tab_template, tab_template_floating_panes, kdl_node.clone()),
        );
        Ok(())
    }
    fn populate_default_tab_template(&mut self, kdl_node: &KdlNode) -> Result<(), ConfigError> {
        let (tab_template, tab_template_floating_panes) = self.parse_tab_template_node(kdl_node)?;
        self.default_tab_template =
            Some((tab_template, tab_template_floating_panes, kdl_node.clone()));
        Ok(())
    }
    fn populate_new_tab_template(&mut self, kdl_node: &KdlNode) -> Result<(), ConfigError> {
        let (_is_focused, _tab_name, tab_template, tab_template_floating_panes) =
            self.parse_tab_node(kdl_node)?;
        self.new_tab_template = Some((tab_template, tab_template_floating_panes));
        Ok(())
    }
    fn parse_tab_template_node(
        &self,
        kdl_node: &KdlNode,
    ) -> Result<(TiledPaneLayout, Vec<FloatingPaneLayout>), ConfigError> {
        self.assert_valid_tab_properties(kdl_node)?;
        let children_split_direction = self.parse_split_direction(kdl_node)?;
        let mut tab_children = vec![];
        let mut tab_floating_children = vec![];
        let mut external_children_index = None;
        let mut children_index_offset = 0;
        let is_part_of_stack = false;
        if let Some(children) = kdl_children_nodes!(kdl_node) {
            for (i, child) in children.iter().enumerate() {
                if kdl_name!(child) == "pane" {
                    tab_children.push(self.parse_pane_node(child, is_part_of_stack)?);
                } else if kdl_name!(child) == "children" {
                    let node_has_child_nodes =
                        child.children().map(|c| !c.is_empty()).unwrap_or(false);
                    let node_has_entries = !child.entries().is_empty();
                    if node_has_child_nodes || node_has_entries {
                        return Err(ConfigError::new_layout_kdl_error(
                            format!("The `children` node must be bare. All properties should be places on the node consuming this template."),
                            child.span().offset(),
                            child.span().len(),
                        ));
                    }
                    external_children_index = Some(i.saturating_sub(children_index_offset));
                } else if let Some((pane_template, pane_template_kdl_node)) =
                    self.pane_templates.get(kdl_name!(child)).cloned()
                {
                    let should_mark_external_children_index = false;
                    tab_children.push(self.parse_pane_node_with_template(
                        child,
                        pane_template,
                        should_mark_external_children_index,
                        &pane_template_kdl_node,
                    )?);
                } else if kdl_name!(child) == "floating_panes" {
                    children_index_offset += 1;
                    self.populate_floating_pane_children(child, &mut tab_floating_children)?;
                } else if self.is_a_valid_tab_property(kdl_name!(child)) {
                    return Err(ConfigError::new_layout_kdl_error(
                        format!("Tab property '{}' must be placed on the tab_template title line and not in the child braces", kdl_name!(child)),
                        child.span().offset(),
                        child.span().len()
                    ));
                } else {
                    return Err(ConfigError::new_layout_kdl_error(
                        format!("Invalid tab_template property: {}", kdl_name!(child)),
                        child.span().offset(),
                        child.span().len(),
                    ));
                }
            }
        }
        Ok((
            TiledPaneLayout {
                children_split_direction,
                children: tab_children,
                external_children_index,
                ..Default::default()
            },
            tab_floating_children,
        ))
    }
    fn default_template(&self) -> Result<Option<TiledPaneLayout>, ConfigError> {
        match &self.default_tab_template {
            Some((template, _template_floating_panes, _kdl_node)) => {
                let mut template = template.clone();
                if let Some(children_index) = template.external_children_index {
                    template
                        .children
                        .insert(children_index, TiledPaneLayout::default())
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
                    ConfigError::new_layout_kdl_error(
                        "Pane templates must have a name".into(),
                        child.span().offset(),
                        child.span().len(),
                    ),
                )?;
                let mut template_children = HashSet::new();
                self.get_pane_template_dependencies(child, &mut template_children)?;
                if dependency_tree.contains_key(template_name) {
                    return Err(ConfigError::new_layout_kdl_error(
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
        let all_pane_template_names: HashSet<&str> = dependency_tree.keys().cloned().collect();
        for (_pane_template_name, dependencies) in dependency_tree.iter_mut() {
            dependencies.retain(|d| all_pane_template_names.contains(d));
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
            self.global_cwd = self.parse_path(layout_node, "cwd")?;
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
                return Err(ConfigError::new_layout_kdl_error(
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
            } else if child_name == "new_tab_template" {
                self.populate_new_tab_template(child)?;
            }
        }
        Ok(())
    }
    fn populate_swap_tiled_layouts(
        &mut self,
        layout_children: &[KdlNode],
        swap_tiled_layouts: &mut Vec<SwapTiledLayout>,
    ) -> Result<(), ConfigError> {
        for child in layout_children.iter() {
            let child_name = kdl_name!(child);
            if child_name == "swap_tiled_layout" {
                let swap_layout_name =
                    kdl_get_string_property_or_child_value!(child, "name").map(|n| String::from(n));
                if let Some(swap_tiled_layout_group) = kdl_children_nodes!(child) {
                    let mut swap_tiled_layout = BTreeMap::new();
                    for layout in swap_tiled_layout_group {
                        let layout_node_name = kdl_name!(layout);
                        if layout_node_name == "tab" {
                            let layout_constraint = self.parse_constraint(layout)?;

                            match &self.default_tab_template {
                                Some((
                                    default_tab_template,
                                    _default_tab_template_floating_panes,
                                    default_tab_template_kdl_node,
                                )) => {
                                    let default_tab_template = default_tab_template.clone();
                                    let layout = self
                                        .populate_one_swap_tiled_layout_with_template(
                                            layout,
                                            default_tab_template,
                                            default_tab_template_kdl_node.clone(),
                                        )?;
                                    swap_tiled_layout.insert(layout_constraint, layout);
                                },
                                None => {
                                    let layout = self.populate_one_swap_tiled_layout(layout)?;
                                    swap_tiled_layout.insert(layout_constraint, layout);
                                },
                            }
                        } else if let Some((
                            tab_template,
                            _tab_template_floating_panes,
                            tab_template_kdl_node,
                        )) = self.tab_templates.get(layout_node_name).cloned()
                        {
                            let layout_constraint = self.parse_constraint(layout)?;
                            let layout = self.populate_one_swap_tiled_layout_with_template(
                                layout,
                                tab_template,
                                tab_template_kdl_node,
                            )?;
                            swap_tiled_layout.insert(layout_constraint, layout);
                        } else {
                            return Err(ConfigError::new_layout_kdl_error(
                                format!("Unknown layout node: '{}'", layout_node_name),
                                layout.span().offset(),
                                layout.span().len(),
                            ));
                        }
                    }
                    swap_tiled_layouts.push((swap_tiled_layout, swap_layout_name));
                }
            }
        }
        Ok(())
    }
    fn populate_swap_floating_layouts(
        &mut self,
        layout_children: &[KdlNode],
        swap_floating_layouts: &mut Vec<SwapFloatingLayout>,
    ) -> Result<(), ConfigError> {
        for child in layout_children.iter() {
            let child_name = kdl_name!(child);
            if child_name == "swap_floating_layout" {
                let swap_layout_name =
                    kdl_get_string_property_or_child_value!(child, "name").map(|n| String::from(n));
                if let Some(swap_floating_layout_group) = kdl_children_nodes!(child) {
                    let mut swap_floating_layout = BTreeMap::new();
                    for layout in swap_floating_layout_group {
                        let layout_node_name = kdl_name!(layout);
                        if layout_node_name == "floating_panes" {
                            let layout_constraint = self.parse_constraint(layout)?;
                            let layout = self.populate_one_swap_floating_layout(layout)?;
                            swap_floating_layout.insert(layout_constraint, layout);
                        } else if let Some((
                            tab_template,
                            tab_template_floating_panes,
                            tab_template_kdl_node,
                        )) = self.tab_templates.get(layout_node_name).cloned()
                        {
                            let layout_constraint = self.parse_constraint(layout)?;
                            let layout = self.populate_one_swap_floating_layout_with_template(
                                layout,
                                tab_template,
                                tab_template_floating_panes,
                                tab_template_kdl_node,
                            )?;
                            swap_floating_layout.insert(layout_constraint, layout);
                        } else {
                            return Err(ConfigError::new_layout_kdl_error(
                                format!("Unknown layout node: '{}'", layout_node_name),
                                layout.span().offset(),
                                layout.span().len(),
                            ));
                        }
                    }
                    swap_floating_layouts.push((swap_floating_layout, swap_layout_name));
                }
            }
        }
        Ok(())
    }
    fn parse_constraint(&mut self, layout_node: &KdlNode) -> Result<LayoutConstraint, ConfigError> {
        if let Some(max_panes) = kdl_get_string_property_or_child_value!(layout_node, "max_panes") {
            return Err(kdl_parsing_error!(
                format!(
                    "max_panes should be a fixed number (eg. 1) and not a quoted string (\"{}\")",
                    max_panes
                ),
                layout_node
            ));
        };
        if let Some(min_panes) = kdl_get_string_property_or_child_value!(layout_node, "min_panes") {
            return Err(kdl_parsing_error!(
                format!(
                    "min_panes should be a fixed number (eg. 1) and not a quoted string (\"{}\")",
                    min_panes
                ),
                layout_node
            ));
        };
        if let Some(exact_panes) =
            kdl_get_string_property_or_child_value!(layout_node, "exact_panes")
        {
            return Err(kdl_parsing_error!(
                format!(
                    "exact_panes should be a fixed number (eg. 1) and not a quoted string (\"{}\")",
                    exact_panes,
                ),
                layout_node
            ));
        };
        let max_panes = kdl_get_int_property_or_child_value!(layout_node, "max_panes");
        let min_panes = kdl_get_int_property_or_child_value!(layout_node, "min_panes");
        let exact_panes = kdl_get_int_property_or_child_value!(layout_node, "exact_panes");
        let mut constraint_count = 0;
        let mut constraint = None;
        if let Some(max_panes) = max_panes {
            constraint_count += 1;
            constraint = Some(LayoutConstraint::MaxPanes(max_panes as usize));
        }
        if let Some(min_panes) = min_panes {
            constraint_count += 1;
            constraint = Some(LayoutConstraint::MinPanes(min_panes as usize));
        }
        if let Some(exact_panes) = exact_panes {
            constraint_count += 1;
            constraint = Some(LayoutConstraint::ExactPanes(exact_panes as usize));
        }
        if constraint_count > 1 {
            return Err(kdl_parsing_error!(
                format!("cannot have more than one constraint (eg. max_panes + min_panes)'"),
                layout_node
            ));
        }
        Ok(constraint.unwrap_or(LayoutConstraint::NoConstraint))
    }
    fn populate_one_swap_tiled_layout(
        &self,
        layout_node: &KdlNode,
    ) -> Result<TiledPaneLayout, ConfigError> {
        self.assert_valid_tab_properties(layout_node)?;
        let children_split_direction = self.parse_split_direction(layout_node)?;
        let mut child_floating_panes = vec![];
        let children = match kdl_children_nodes!(layout_node) {
            Some(children) => {
                let should_mark_external_children_index = true;
                self.parse_child_pane_nodes_for_tab(
                    children,
                    should_mark_external_children_index,
                    &mut child_floating_panes,
                )?
            },
            None => vec![],
        };
        let pane_layout = TiledPaneLayout {
            children_split_direction,
            children,
            ..Default::default()
        };
        Ok(pane_layout)
    }
    fn populate_one_swap_tiled_layout_with_template(
        &self,
        layout_node: &KdlNode,
        tab_template: TiledPaneLayout,
        tab_template_kdl_node: KdlNode,
    ) -> Result<TiledPaneLayout, ConfigError> {
        let should_mark_external_children_index = true;
        let layout = self.parse_tab_node_with_template(
            layout_node,
            tab_template,
            vec![], // no floating_panes in swap tiled node
            should_mark_external_children_index,
            &tab_template_kdl_node,
        )?;
        Ok(layout.2)
    }
    fn populate_one_swap_floating_layout(
        &self,
        layout_node: &KdlNode,
    ) -> Result<Vec<FloatingPaneLayout>, ConfigError> {
        let mut floating_panes = vec![];
        self.assert_valid_tab_properties(layout_node)?;
        self.populate_floating_pane_children(layout_node, &mut floating_panes)?;
        Ok(floating_panes)
    }
    fn populate_one_swap_floating_layout_with_template(
        &self,
        layout_node: &KdlNode,
        tab_template: TiledPaneLayout,
        tab_template_floating_panes: Vec<FloatingPaneLayout>,
        tab_template_kdl_node: KdlNode,
    ) -> Result<Vec<FloatingPaneLayout>, ConfigError> {
        let should_mark_external_children_index = false;
        let layout = self.parse_tab_node_with_template(
            layout_node,
            tab_template,
            tab_template_floating_panes,
            should_mark_external_children_index,
            &tab_template_kdl_node,
        )?;
        Ok(layout.3)
    }
    fn layout_with_tabs(
        &self,
        tabs: Vec<(Option<String>, TiledPaneLayout, Vec<FloatingPaneLayout>)>,
        focused_tab_index: Option<usize>,
        swap_tiled_layouts: Vec<SwapTiledLayout>,
        swap_floating_layouts: Vec<SwapFloatingLayout>,
    ) -> Result<Layout, ConfigError> {
        let template = if let Some(new_tab_template) = &self.new_tab_template {
            Some(new_tab_template.clone())
        } else {
            let default_tab_tiled_panes_template = self
                .default_template()?
                .unwrap_or_else(|| TiledPaneLayout::default());
            Some((default_tab_tiled_panes_template, vec![]))
        };

        Ok(Layout {
            tabs,
            template,
            focused_tab_index,
            swap_tiled_layouts,
            swap_floating_layouts,
            ..Default::default()
        })
    }
    fn layout_with_one_tab(
        &self,
        panes: Vec<TiledPaneLayout>,
        floating_panes: Vec<FloatingPaneLayout>,
        swap_tiled_layouts: Vec<SwapTiledLayout>,
        swap_floating_layouts: Vec<SwapFloatingLayout>,
    ) -> Result<Layout, ConfigError> {
        let main_tab_layout = TiledPaneLayout {
            children: panes,
            ..Default::default()
        };
        let default_template = self.default_template()?;
        let tabs = if default_template.is_none() && self.new_tab_template.is_none() {
            // in this case, the layout will be created as the default template and we don't need
            // to explicitly place it in the first tab
            vec![]
        } else {
            vec![(None, main_tab_layout.clone(), floating_panes.clone())]
        };
        let template = default_template
            .map(|tiled_panes_template| (tiled_panes_template, floating_panes.clone()))
            .or_else(|| self.new_tab_template.clone())
            .unwrap_or_else(|| (main_tab_layout.clone(), floating_panes.clone()));
        // create a layout with one tab that has these child panes
        Ok(Layout {
            tabs,
            template: Some(template),
            swap_tiled_layouts,
            swap_floating_layouts,
            ..Default::default()
        })
    }
    fn layout_with_one_pane(
        &self,
        child_floating_panes: Vec<FloatingPaneLayout>,
        swap_tiled_layouts: Vec<SwapTiledLayout>,
        swap_floating_layouts: Vec<SwapFloatingLayout>,
    ) -> Result<Layout, ConfigError> {
        let template = if let Some(new_tab_template) = &self.new_tab_template {
            Some(new_tab_template.clone())
        } else {
            let default_tab_tiled_panes_template = self
                .default_template()?
                .unwrap_or_else(|| TiledPaneLayout::default());
            Some((default_tab_tiled_panes_template, child_floating_panes))
        };
        Ok(Layout {
            template,
            swap_tiled_layouts,
            swap_floating_layouts,
            ..Default::default()
        })
    }
    fn populate_layout_child(
        &mut self,
        child: &KdlNode,
        child_tabs: &mut Vec<(
            bool,
            Option<String>,
            TiledPaneLayout,
            Vec<FloatingPaneLayout>,
        )>,
        child_panes: &mut Vec<TiledPaneLayout>,
        child_floating_panes: &mut Vec<FloatingPaneLayout>,
    ) -> Result<(), ConfigError> {
        let child_name = kdl_name!(child);
        if (child_name == "pane" || child_name == "floating_panes") && !child_tabs.is_empty() {
            return Err(ConfigError::new_layout_kdl_error(
                "Cannot have both tabs and panes in the same node".into(),
                child.span().offset(),
                child.span().len(),
            ));
        }
        if child_name == "pane" {
            let is_part_of_stack = false;
            let mut pane_node = self.parse_pane_node(child, is_part_of_stack)?;
            if let Some(global_cwd) = &self.global_cwd {
                pane_node.add_cwd_to_layout(&global_cwd);
            }
            child_panes.push(pane_node);
        } else if child_name == "floating_panes" {
            self.populate_floating_pane_children(child, child_floating_panes)?;
        } else if child_name == "tab" {
            if !child_panes.is_empty() || !child_floating_panes.is_empty() {
                return Err(ConfigError::new_layout_kdl_error(
                    "Cannot have both tabs and panes in the same node".into(),
                    child.span().offset(),
                    child.span().len(),
                ));
            }
            match &self.default_tab_template {
                Some((
                    default_tab_template,
                    default_tab_template_floating_panes,
                    default_tab_template_kdl_node,
                )) => {
                    let default_tab_template = default_tab_template.clone();
                    let should_mark_external_children_index = false;
                    child_tabs.push(self.parse_tab_node_with_template(
                        child,
                        default_tab_template,
                        default_tab_template_floating_panes.clone(),
                        should_mark_external_children_index,
                        default_tab_template_kdl_node,
                    )?);
                },
                None => {
                    child_tabs.push(self.parse_tab_node(child)?);
                },
            }
        } else if let Some((tab_template, tab_template_floating_panes, tab_template_kdl_node)) =
            self.tab_templates.get(child_name).cloned()
        {
            if !child_panes.is_empty() {
                return Err(ConfigError::new_layout_kdl_error(
                    "Cannot have both tabs and panes in the same node".into(),
                    child.span().offset(),
                    child.span().len(),
                ));
            }
            let should_mark_external_children_index = false;
            child_tabs.push(self.parse_tab_node_with_template(
                child,
                tab_template,
                tab_template_floating_panes,
                should_mark_external_children_index,
                &tab_template_kdl_node,
            )?);
        } else if let Some((pane_template, pane_template_kdl_node)) =
            self.pane_templates.get(child_name).cloned()
        {
            if !child_tabs.is_empty() {
                return Err(ConfigError::new_layout_kdl_error(
                    "Cannot have both tabs and panes in the same node".into(),
                    child.span().offset(),
                    child.span().len(),
                ));
            }
            let should_mark_external_children_index = false;
            let mut pane_template = self.parse_pane_node_with_template(
                child,
                pane_template,
                should_mark_external_children_index,
                &pane_template_kdl_node,
            )?;
            if let Some(cwd_prefix) = &self.cwd_prefix(None)? {
                pane_template.add_cwd_to_layout(&cwd_prefix);
            }
            child_panes.push(pane_template);
        } else if !self.is_a_reserved_word(child_name) {
            return Err(ConfigError::new_layout_kdl_error(
                format!("Unknown layout node: '{}'", child_name),
                child.span().offset(),
                child.span().len(),
            ));
        }
        Ok(())
    }
    fn populate_floating_pane_children(
        &self,
        child: &KdlNode,
        child_floating_panes: &mut Vec<FloatingPaneLayout>,
    ) -> Result<(), ConfigError> {
        if let Some(children) = kdl_children_nodes!(child) {
            for child in children {
                if kdl_name!(child) == "pane" {
                    let mut pane_node = self.parse_floating_pane_node(child)?;
                    if let Some(global_cwd) = &self.global_cwd {
                        pane_node.add_cwd_to_layout(&global_cwd);
                    }
                    child_floating_panes.push(pane_node);
                } else if let Some((pane_template, pane_template_kdl_node)) =
                    self.pane_templates.get(kdl_name!(child)).cloned()
                {
                    let pane_node = self.parse_floating_pane_node_with_template(
                        child,
                        pane_template,
                        &pane_template_kdl_node,
                    )?;
                    child_floating_panes.push(pane_node);
                } else {
                    return Err(ConfigError::new_layout_kdl_error(
                        format!(
                            "floating_panes can only contain pane nodes, found: {}",
                            kdl_name!(child)
                        ),
                        child.span().offset(),
                        child.span().len(),
                    ));
                }
            }
        };
        Ok(())
    }
    pub fn parse_external_swap_layouts(
        &mut self,
        raw_swap_layouts: &str,
        mut existing_layout: Layout,
    ) -> Result<Layout, ConfigError> {
        let kdl_swap_layout: KdlDocument = raw_swap_layouts.parse()?;
        let mut swap_tiled_layouts = vec![];
        let mut swap_floating_layouts = vec![];

        for node in kdl_swap_layout.nodes() {
            let node_name = kdl_name!(node);
            if node_name == "swap_floating_layout"
                || node_name == "swap_tiled_layout"
                || node_name == "tab_template"
                || node_name == "pane_template"
            {
                continue;
            } else if node_name == "layout" {
                return Err(ConfigError::new_layout_kdl_error(
                    "Swap layouts should not have their own layout node".into(),
                    node.span().offset(),
                    node.span().len(),
                ))?;
            } else if self.is_a_reserved_word(node_name) {
                return Err(ConfigError::new_layout_kdl_error(
                    format!(
                        "Swap layouts should not contain bare nodes of type: {}",
                        node_name
                    ),
                    node.span().offset(),
                    node.span().len(),
                ))?;
            }
        }

        self.populate_pane_templates(kdl_swap_layout.nodes(), &kdl_swap_layout)?;
        self.populate_tab_templates(kdl_swap_layout.nodes())?;
        self.populate_swap_tiled_layouts(kdl_swap_layout.nodes(), &mut swap_tiled_layouts)?;
        self.populate_swap_floating_layouts(kdl_swap_layout.nodes(), &mut swap_floating_layouts)?;

        existing_layout
            .swap_tiled_layouts
            .append(&mut swap_tiled_layouts);
        existing_layout
            .swap_floating_layouts
            .append(&mut swap_floating_layouts);
        Ok(existing_layout)
    }
    pub fn parse(&mut self) -> Result<Layout, ConfigError> {
        let kdl_layout: KdlDocument = self.raw_layout.parse()?;
        let layout_node = kdl_layout
            .nodes()
            .iter()
            .find(|n| kdl_name!(n) == "layout")
            .ok_or(ConfigError::new_layout_kdl_error(
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
        let mut non_layout_nodes_in_root = kdl_layout
            .nodes()
            .iter()
            .filter(|n| kdl_name!(n) != "layout" && self.is_a_reserved_word(kdl_name!(n)));
        if let Some(first_non_layout_node) = non_layout_nodes_in_root.next() {
            return Err(ConfigError::new_layout_kdl_error(
                "This node should be inside the main \"layout\" node".into(),
                first_non_layout_node.span().offset(),
                first_non_layout_node.span().len(),
            ));
        }
        if has_multiple_layout_nodes {
            return Err(ConfigError::new_layout_kdl_error(
                "Only one layout node per file allowed".into(),
                kdl_layout.span().offset(),
                kdl_layout.span().len(),
            ));
        }
        let mut child_tabs = vec![];
        let mut child_panes = vec![];
        let mut child_floating_panes = vec![];
        let mut swap_tiled_layouts = vec![];
        let mut swap_floating_layouts = vec![];
        if let Some(children) = kdl_children_nodes!(layout_node) {
            self.populate_global_cwd(layout_node)?;
            self.populate_pane_templates(children, &kdl_layout)?;
            self.populate_tab_templates(children)?;
            self.populate_swap_tiled_layouts(children, &mut swap_tiled_layouts)?;
            self.populate_swap_floating_layouts(children, &mut swap_floating_layouts)?;
            for child in children {
                self.populate_layout_child(
                    child,
                    &mut child_tabs,
                    &mut child_panes,
                    &mut child_floating_panes,
                )?;
            }
        }
        if !child_tabs.is_empty() {
            let has_more_than_one_focused_tab = child_tabs
                .iter()
                .filter(|(is_focused, _, _, _)| *is_focused)
                .count()
                > 1;
            if has_more_than_one_focused_tab {
                return Err(ConfigError::new_layout_kdl_error(
                    "Only one tab can be focused".into(),
                    kdl_layout.span().offset(),
                    kdl_layout.span().len(),
                ));
            }
            let focused_tab_index = child_tabs
                .iter()
                .position(|(is_focused, _, _, _)| *is_focused);
            let child_tabs: Vec<(Option<String>, TiledPaneLayout, Vec<FloatingPaneLayout>)> =
                child_tabs
                    .drain(..)
                    .map(
                        |(_is_focused, tab_name, pane_layout, floating_panes_layout)| {
                            (tab_name, pane_layout, floating_panes_layout)
                        },
                    )
                    .collect();
            self.layout_with_tabs(
                child_tabs,
                focused_tab_index,
                swap_tiled_layouts,
                swap_floating_layouts,
            )
        } else if !child_panes.is_empty() {
            self.layout_with_one_tab(
                child_panes,
                child_floating_panes,
                swap_tiled_layouts,
                swap_floating_layouts,
            )
        } else {
            self.layout_with_one_pane(
                child_floating_panes,
                swap_tiled_layouts,
                swap_floating_layouts,
            )
        }
    }
}
