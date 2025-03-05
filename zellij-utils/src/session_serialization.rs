use kdl::{KdlDocument, KdlEntry, KdlNode, KdlValue};
use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;

use crate::{
    input::layout::PluginUserConfiguration,
    input::layout::{
        FloatingPaneLayout, Layout, LayoutConstraint, PercentOrFixed, Run, RunPluginOrAlias,
        SplitDirection, SplitSize, SwapFloatingLayout, SwapTiledLayout, TiledPaneLayout,
    },
    pane_size::{Constraint, PaneGeom},
};

#[derive(Default, Debug, Clone)]
pub struct GlobalLayoutManifest {
    pub global_cwd: Option<PathBuf>,
    pub default_shell: Option<PathBuf>,
    pub default_layout: Box<Layout>,
    pub tabs: Vec<(String, TabLayoutManifest)>,
}

#[derive(Default, Debug, Clone)]
pub struct TabLayoutManifest {
    pub tiled_panes: Vec<PaneLayoutManifest>,
    pub floating_panes: Vec<PaneLayoutManifest>,
    pub is_focused: bool,
    pub hide_floating_panes: bool,
}

#[derive(Default, Debug, Clone)]
pub struct PaneLayoutManifest {
    pub geom: PaneGeom,
    pub run: Option<Run>,
    pub cwd: Option<PathBuf>,
    pub is_borderless: bool,
    pub title: Option<String>,
    pub is_focused: bool,
    pub pane_contents: Option<String>,
}

pub fn serialize_session_layout(
    global_layout_manifest: GlobalLayoutManifest,
) -> Result<(String, BTreeMap<String, String>), &'static str> {
    // BTreeMap is the pane contents and their file names
    let mut document = KdlDocument::new();
    let mut pane_contents = BTreeMap::new();
    let mut layout_node = KdlNode::new("layout");
    let mut layout_node_children = KdlDocument::new();
    if let Some(global_cwd) = serialize_global_cwd(&global_layout_manifest.global_cwd) {
        layout_node_children.nodes_mut().push(global_cwd);
    }
    match serialize_multiple_tabs(global_layout_manifest.tabs, &mut pane_contents) {
        Ok(mut serialized_tabs) => {
            layout_node_children
                .nodes_mut()
                .append(&mut serialized_tabs);
        },
        Err(e) => {
            return Err(e);
        },
    }
    serialize_new_tab_template(
        global_layout_manifest.default_layout.template,
        &mut pane_contents,
        &mut layout_node_children,
    );
    serialize_swap_tiled_layouts(
        global_layout_manifest.default_layout.swap_tiled_layouts,
        &mut pane_contents,
        &mut layout_node_children,
    );
    serialize_swap_floating_layouts(
        global_layout_manifest.default_layout.swap_floating_layouts,
        &mut pane_contents,
        &mut layout_node_children,
    );

    layout_node.set_children(layout_node_children);
    document.nodes_mut().push(layout_node);
    Ok((document.to_string(), pane_contents))
}

fn serialize_tab(
    tab_name: String,
    is_focused: bool,
    hide_floating_panes: bool,
    tiled_panes: &Vec<PaneLayoutManifest>,
    floating_panes: &Vec<PaneLayoutManifest>,
    pane_contents: &mut BTreeMap<String, String>,
) -> Option<KdlNode> {
    let mut serialized_tab = KdlNode::new("tab");
    let mut serialized_tab_children = KdlDocument::new();
    match get_tiled_panes_layout_from_panegeoms(tiled_panes, None) {
        Some(tiled_panes_layout) => {
            let floating_panes_layout = get_floating_panes_layout_from_panegeoms(floating_panes);
            let tiled_panes = if &tiled_panes_layout.children_split_direction
                != &SplitDirection::default()
                || tiled_panes_layout.children_are_stacked
            {
                vec![tiled_panes_layout]
            } else {
                tiled_panes_layout.children
            };
            serialized_tab
                .entries_mut()
                .push(KdlEntry::new_prop("name", tab_name));
            if is_focused {
                serialized_tab
                    .entries_mut()
                    .push(KdlEntry::new_prop("focus", KdlValue::Bool(true)));
            }
            if hide_floating_panes {
                serialized_tab.entries_mut().push(KdlEntry::new_prop(
                    "hide_floating_panes",
                    KdlValue::Bool(true),
                ));
            }

            serialize_tiled_and_floating_panes(
                &tiled_panes,
                floating_panes_layout,
                pane_contents,
                &mut serialized_tab_children,
            );

            serialized_tab.set_children(serialized_tab_children);
            Some(serialized_tab)
        },
        None => {
            return None;
        },
    }
}

fn serialize_tiled_and_floating_panes(
    tiled_panes: &Vec<TiledPaneLayout>,
    floating_panes_layout: Vec<FloatingPaneLayout>,
    pane_contents: &mut BTreeMap<String, String>,
    serialized_tab_children: &mut KdlDocument,
) {
    for tiled_pane_layout in tiled_panes {
        let ignore_size = false;
        let tiled_pane_node = serialize_tiled_pane(tiled_pane_layout, ignore_size, pane_contents);
        serialized_tab_children.nodes_mut().push(tiled_pane_node);
    }
    if !floating_panes_layout.is_empty() {
        let mut floating_panes_node = KdlNode::new("floating_panes");
        let mut floating_panes_node_children = KdlDocument::new();
        for floating_pane in floating_panes_layout {
            let pane_node = serialize_floating_pane(&floating_pane, pane_contents);
            floating_panes_node_children.nodes_mut().push(pane_node);
        }
        floating_panes_node.set_children(floating_panes_node_children);
        serialized_tab_children
            .nodes_mut()
            .push(floating_panes_node);
    }
}

fn serialize_tiled_pane(
    layout: &TiledPaneLayout,
    ignore_size: bool,
    pane_contents: &mut BTreeMap<String, String>,
) -> KdlNode {
    let (command, args) = extract_command_and_args(&layout.run);
    let (plugin, plugin_config) = extract_plugin_and_config(&layout.run);
    let (edit, _line_number) = extract_edit_and_line_number(&layout.run);
    let cwd = layout.run.as_ref().and_then(|r| r.get_cwd());
    let has_children = layout.external_children_index.is_some() || !layout.children.is_empty();

    let mut tiled_pane_node = KdlNode::new("pane");
    serialize_pane_title_and_attributes(
        &command,
        &edit,
        &layout.name,
        cwd,
        layout.focus,
        &layout.pane_initial_contents,
        pane_contents,
        has_children,
        &mut tiled_pane_node,
    );

    serialize_tiled_layout_attributes(&layout, ignore_size, &mut tiled_pane_node);
    let has_child_attributes = !layout.children.is_empty()
        || layout.external_children_index.is_some()
        || !args.is_empty()
        || plugin.is_some()
        || command.is_some();
    if has_child_attributes {
        let mut tiled_pane_node_children = KdlDocument::new();
        serialize_args(args, &mut tiled_pane_node_children);
        serialize_start_suspended(&command, &mut tiled_pane_node_children);
        serialize_plugin(plugin, plugin_config, &mut tiled_pane_node_children);
        if layout.children.is_empty() && layout.external_children_index.is_some() {
            tiled_pane_node_children
                .nodes_mut()
                .push(KdlNode::new("children"));
        }
        for (i, pane) in layout.children.iter().enumerate() {
            if Some(i) == layout.external_children_index {
                tiled_pane_node_children
                    .nodes_mut()
                    .push(KdlNode::new("children"));
            } else {
                let ignore_size = layout.children_are_stacked;
                let child_pane_node = serialize_tiled_pane(&pane, ignore_size, pane_contents);
                tiled_pane_node_children.nodes_mut().push(child_pane_node);
            }
        }
        tiled_pane_node.set_children(tiled_pane_node_children);
    }
    tiled_pane_node
}

pub fn extract_command_and_args(layout_run: &Option<Run>) -> (Option<String>, Vec<String>) {
    match layout_run {
        Some(Run::Command(run_command)) => (
            Some(run_command.command.display().to_string()),
            run_command.args.clone(),
        ),
        _ => (None, vec![]),
    }
}
pub fn extract_plugin_and_config(
    layout_run: &Option<Run>,
) -> (Option<String>, Option<PluginUserConfiguration>) {
    match &layout_run {
        Some(Run::Plugin(run_plugin_or_alias)) => match run_plugin_or_alias {
            RunPluginOrAlias::RunPlugin(run_plugin) => (
                Some(run_plugin.location.display()),
                Some(run_plugin.configuration.clone()),
            ),
            RunPluginOrAlias::Alias(plugin_alias) => {
                // in this case, the aliases should already be populated by the RunPlugins they
                // translate to - if they are not, the alias either does not exist or this is some
                // sort of bug
                let name = plugin_alias
                    .run_plugin
                    .as_ref()
                    .map(|run_plugin| run_plugin.location.display().to_string())
                    .unwrap_or_else(|| plugin_alias.name.clone());
                let configuration = plugin_alias
                    .run_plugin
                    .as_ref()
                    .map(|run_plugin| run_plugin.configuration.clone());
                (Some(name), configuration)
            },
        },
        _ => (None, None),
    }
}
pub fn extract_edit_and_line_number(layout_run: &Option<Run>) -> (Option<String>, Option<usize>) {
    match &layout_run {
        // TODO: line number in layouts?
        Some(Run::EditFile(path, line_number, _cwd)) => {
            (Some(path.display().to_string()), line_number.clone())
        },
        _ => (None, None),
    }
}

fn serialize_pane_title_and_attributes(
    command: &Option<String>,
    edit: &Option<String>,
    name: &Option<String>,
    cwd: Option<PathBuf>,
    focus: Option<bool>,
    initial_pane_contents: &Option<String>,
    pane_contents: &mut BTreeMap<String, String>,
    has_children: bool,
    kdl_node: &mut KdlNode,
) {
    match (&command, &edit) {
        (Some(command), _) => kdl_node
            .entries_mut()
            .push(KdlEntry::new_prop("command", command.to_owned())),
        (None, Some(edit)) => kdl_node
            .entries_mut()
            .push(KdlEntry::new_prop("edit", edit.to_owned())),
        _ => {},
    };
    if let Some(name) = name {
        kdl_node
            .entries_mut()
            .push(KdlEntry::new_prop("name", name.to_owned()));
    }
    if let Some(cwd) = cwd {
        let path = cwd.display().to_string();
        if !path.is_empty() && !has_children {
            kdl_node
                .entries_mut()
                .push(KdlEntry::new_prop("cwd", path.to_owned()));
        }
    }
    if focus.unwrap_or(false) {
        kdl_node
            .entries_mut()
            .push(KdlEntry::new_prop("focus", KdlValue::Bool(true)));
    }
    if let Some(initial_pane_contents) = initial_pane_contents.as_ref() {
        if command.is_none() && edit.is_none() {
            let file_name = format!("initial_contents_{}", pane_contents.keys().len() + 1);
            kdl_node
                .entries_mut()
                .push(KdlEntry::new_prop("contents_file", file_name.clone()));

            pane_contents.insert(file_name, initial_pane_contents.clone());
        }
    }
}

fn serialize_args(args: Vec<String>, pane_node_children: &mut KdlDocument) {
    if !args.is_empty() {
        let mut args_node = KdlNode::new("args");
        for arg in &args {
            args_node.entries_mut().push(KdlEntry::new(arg.to_owned()));
        }
        pane_node_children.nodes_mut().push(args_node);
    }
}

fn serialize_plugin(
    plugin: Option<String>,
    plugin_config: Option<PluginUserConfiguration>,
    pane_node_children: &mut KdlDocument,
) {
    if let Some(plugin) = plugin {
        let mut plugin_node = KdlNode::new("plugin");
        plugin_node
            .entries_mut()
            .push(KdlEntry::new_prop("location", plugin.to_owned()));
        if let Some(plugin_config) =
            plugin_config.and_then(|p| if p.inner().is_empty() { None } else { Some(p) })
        {
            let mut plugin_node_children = KdlDocument::new();
            for (config_key, config_value) in plugin_config.inner() {
                let mut config_node = KdlNode::new(config_key.to_owned());
                config_node
                    .entries_mut()
                    .push(KdlEntry::new(config_value.to_owned()));
                plugin_node_children.nodes_mut().push(config_node);
            }
            plugin_node.set_children(plugin_node_children);
        }
        pane_node_children.nodes_mut().push(plugin_node);
    }
}

fn serialize_tiled_layout_attributes(
    layout: &TiledPaneLayout,
    ignore_size: bool,
    kdl_node: &mut KdlNode,
) {
    if !ignore_size {
        match layout.split_size {
            Some(SplitSize::Fixed(size)) => kdl_node
                .entries_mut()
                .push(KdlEntry::new_prop("size", KdlValue::Base10(size as i64))),
            Some(SplitSize::Percent(size)) => kdl_node
                .entries_mut()
                .push(KdlEntry::new_prop("size", format!("{size}%"))),
            None => (),
        };
    }
    if layout.borderless {
        kdl_node
            .entries_mut()
            .push(KdlEntry::new_prop("borderless", KdlValue::Bool(true)));
    }
    if layout.children_are_stacked {
        kdl_node
            .entries_mut()
            .push(KdlEntry::new_prop("stacked", KdlValue::Bool(true)));
    }
    if layout.is_expanded_in_stack {
        kdl_node
            .entries_mut()
            .push(KdlEntry::new_prop("expanded", KdlValue::Bool(true)));
    }
    if layout.children_split_direction != SplitDirection::default() {
        let direction = match layout.children_split_direction {
            SplitDirection::Horizontal => "horizontal",
            SplitDirection::Vertical => "vertical",
        };
        kdl_node
            .entries_mut()
            .push(KdlEntry::new_prop("split_direction", direction));
    }
}

fn serialize_floating_layout_attributes(
    layout: &FloatingPaneLayout,
    pane_node_children: &mut KdlDocument,
) {
    match layout.height {
        Some(PercentOrFixed::Fixed(fixed_height)) => {
            let mut node = KdlNode::new("height");
            node.entries_mut()
                .push(KdlEntry::new(KdlValue::Base10(fixed_height as i64)));
            pane_node_children.nodes_mut().push(node);
        },
        Some(PercentOrFixed::Percent(percent)) => {
            let mut node = KdlNode::new("height");
            node.entries_mut()
                .push(KdlEntry::new(format!("{}%", percent)));
            pane_node_children.nodes_mut().push(node);
        },
        None => {},
    }
    match layout.width {
        Some(PercentOrFixed::Fixed(fixed_width)) => {
            let mut node = KdlNode::new("width");
            node.entries_mut()
                .push(KdlEntry::new(KdlValue::Base10(fixed_width as i64)));
            pane_node_children.nodes_mut().push(node);
        },
        Some(PercentOrFixed::Percent(percent)) => {
            let mut node = KdlNode::new("width");
            node.entries_mut()
                .push(KdlEntry::new(format!("{}%", percent)));
            pane_node_children.nodes_mut().push(node);
        },
        None => {},
    }
    match layout.x {
        Some(PercentOrFixed::Fixed(fixed_x)) => {
            let mut node = KdlNode::new("x");
            node.entries_mut()
                .push(KdlEntry::new(KdlValue::Base10(fixed_x as i64)));
            pane_node_children.nodes_mut().push(node);
        },
        Some(PercentOrFixed::Percent(percent)) => {
            let mut node = KdlNode::new("x");
            node.entries_mut()
                .push(KdlEntry::new(format!("{}%", percent)));
            pane_node_children.nodes_mut().push(node);
        },
        None => {},
    }
    match layout.y {
        Some(PercentOrFixed::Fixed(fixed_y)) => {
            let mut node = KdlNode::new("y");
            node.entries_mut()
                .push(KdlEntry::new(KdlValue::Base10(fixed_y as i64)));
            pane_node_children.nodes_mut().push(node);
        },
        Some(PercentOrFixed::Percent(percent)) => {
            let mut node = KdlNode::new("y");
            node.entries_mut()
                .push(KdlEntry::new(format!("{}%", percent)));
            pane_node_children.nodes_mut().push(node);
        },
        None => {},
    }
    match layout.pinned {
        Some(true) => {
            let mut node = KdlNode::new("pinned");
            node.entries_mut().push(KdlEntry::new(KdlValue::Bool(true)));
            pane_node_children.nodes_mut().push(node);
        },
        _ => {},
    }
}

fn serialize_start_suspended(command: &Option<String>, pane_node_children: &mut KdlDocument) {
    if command.is_some() {
        let mut start_suspended_node = KdlNode::new("start_suspended");
        start_suspended_node
            .entries_mut()
            .push(KdlEntry::new(KdlValue::Bool(true)));
        pane_node_children.nodes_mut().push(start_suspended_node);
    }
}

fn serialize_global_cwd(global_cwd: &Option<PathBuf>) -> Option<KdlNode> {
    global_cwd.as_ref().map(|cwd| {
        let mut node = KdlNode::new("cwd");
        node.push(cwd.display().to_string());
        node
    })
}

fn serialize_new_tab_template(
    new_tab_template: Option<(TiledPaneLayout, Vec<FloatingPaneLayout>)>,
    pane_contents: &mut BTreeMap<String, String>,
    layout_children_node: &mut KdlDocument,
) {
    if let Some((tiled_panes, floating_panes)) = new_tab_template {
        let tiled_panes = if &tiled_panes.children_split_direction != &SplitDirection::default() {
            vec![tiled_panes]
        } else {
            tiled_panes.children
        };
        let mut new_tab_template_node = KdlNode::new("new_tab_template");
        let mut new_tab_template_children = KdlDocument::new();

        serialize_tiled_and_floating_panes(
            &tiled_panes,
            floating_panes,
            pane_contents,
            &mut new_tab_template_children,
        );
        new_tab_template_node.set_children(new_tab_template_children);
        layout_children_node.nodes_mut().push(new_tab_template_node);
    }
}

fn serialize_swap_tiled_layouts(
    swap_tiled_layouts: Vec<SwapTiledLayout>,
    pane_contents: &mut BTreeMap<String, String>,
    layout_node_children: &mut KdlDocument,
) {
    for swap_tiled_layout in swap_tiled_layouts {
        let mut swap_tiled_layout_node = KdlNode::new("swap_tiled_layout");
        let mut swap_tiled_layout_node_children = KdlDocument::new();
        let swap_tiled_layout_name = swap_tiled_layout.1;
        if let Some(name) = swap_tiled_layout_name {
            swap_tiled_layout_node
                .entries_mut()
                .push(KdlEntry::new_prop("name", name.to_owned()));
        }

        for (layout_constraint, tiled_panes_layout) in swap_tiled_layout.0 {
            let tiled_panes_layout =
                if &tiled_panes_layout.children_split_direction != &SplitDirection::default() {
                    vec![tiled_panes_layout]
                } else {
                    tiled_panes_layout.children
                };
            let mut layout_step_node = KdlNode::new("tab");
            let mut layout_step_node_children = KdlDocument::new();
            if let Some(layout_constraint_entry) = serialize_layout_constraint(layout_constraint) {
                layout_step_node.entries_mut().push(layout_constraint_entry);
            }

            serialize_tiled_and_floating_panes(
                &tiled_panes_layout,
                vec![],
                pane_contents,
                &mut layout_step_node_children,
            );
            layout_step_node.set_children(layout_step_node_children);
            swap_tiled_layout_node_children
                .nodes_mut()
                .push(layout_step_node);
        }
        swap_tiled_layout_node.set_children(swap_tiled_layout_node_children);
        layout_node_children
            .nodes_mut()
            .push(swap_tiled_layout_node);
    }
}

fn serialize_layout_constraint(layout_constraint: LayoutConstraint) -> Option<KdlEntry> {
    match layout_constraint {
        LayoutConstraint::MaxPanes(max_panes) => Some(KdlEntry::new_prop(
            "max_panes",
            KdlValue::Base10(max_panes as i64),
        )),
        LayoutConstraint::MinPanes(min_panes) => Some(KdlEntry::new_prop(
            "min_panes",
            KdlValue::Base10(min_panes as i64),
        )),
        LayoutConstraint::ExactPanes(exact_panes) => Some(KdlEntry::new_prop(
            "exact_panes",
            KdlValue::Base10(exact_panes as i64),
        )),
        LayoutConstraint::NoConstraint => None,
    }
}

fn serialize_swap_floating_layouts(
    swap_floating_layouts: Vec<SwapFloatingLayout>,
    pane_contents: &mut BTreeMap<String, String>,
    layout_children_node: &mut KdlDocument,
) {
    for swap_floating_layout in swap_floating_layouts {
        let mut swap_floating_layout_node = KdlNode::new("swap_floating_layout");
        let mut swap_floating_layout_node_children = KdlDocument::new();
        let swap_floating_layout_name = swap_floating_layout.1;
        if let Some(name) = swap_floating_layout_name {
            swap_floating_layout_node
                .entries_mut()
                .push(KdlEntry::new_prop("name", name.to_owned()));
        }

        for (layout_constraint, floating_panes_layout) in swap_floating_layout.0 {
            let mut layout_step_node = KdlNode::new("floating_panes");
            let mut layout_step_node_children = KdlDocument::new();
            if let Some(layout_constraint_entry) = serialize_layout_constraint(layout_constraint) {
                layout_step_node.entries_mut().push(layout_constraint_entry);
            }

            for floating_pane_layout in floating_panes_layout {
                let floating_pane_node =
                    serialize_floating_pane(&floating_pane_layout, pane_contents);
                layout_step_node_children
                    .nodes_mut()
                    .push(floating_pane_node);
            }
            layout_step_node.set_children(layout_step_node_children);
            swap_floating_layout_node_children
                .nodes_mut()
                .push(layout_step_node);
        }
        swap_floating_layout_node.set_children(swap_floating_layout_node_children);
        layout_children_node
            .nodes_mut()
            .push(swap_floating_layout_node);
    }
}

fn serialize_multiple_tabs(
    tabs: Vec<(String, TabLayoutManifest)>,
    pane_contents: &mut BTreeMap<String, String>,
) -> Result<Vec<KdlNode>, &'static str> {
    let mut serialized_tabs: Vec<KdlNode> = vec![];
    for (tab_name, tab_layout_manifest) in tabs {
        let tiled_panes = tab_layout_manifest.tiled_panes;
        let floating_panes = tab_layout_manifest.floating_panes;
        let hide_floating_panes = tab_layout_manifest.hide_floating_panes;
        let serialized = serialize_tab(
            tab_name.clone(),
            tab_layout_manifest.is_focused,
            hide_floating_panes,
            &tiled_panes,
            &floating_panes,
            pane_contents,
        );
        if let Some(serialized) = serialized {
            serialized_tabs.push(serialized);
        } else {
            return Err("Failed to serialize session state");
        }
    }
    Ok(serialized_tabs)
}

fn serialize_floating_pane(
    layout: &FloatingPaneLayout,
    pane_contents: &mut BTreeMap<String, String>,
) -> KdlNode {
    let mut floating_pane_node = KdlNode::new("pane");
    let mut floating_pane_node_children = KdlDocument::new();
    let (command, args) = extract_command_and_args(&layout.run);
    let (plugin, plugin_config) = extract_plugin_and_config(&layout.run);
    let (edit, _line_number) = extract_edit_and_line_number(&layout.run);
    let cwd = layout.run.as_ref().and_then(|r| r.get_cwd());
    let has_children = false;
    serialize_pane_title_and_attributes(
        &command,
        &edit,
        &layout.name,
        cwd,
        layout.focus,
        &layout.pane_initial_contents,
        pane_contents,
        has_children,
        &mut floating_pane_node,
    );
    serialize_start_suspended(&command, &mut floating_pane_node_children);
    serialize_floating_layout_attributes(&layout, &mut floating_pane_node_children);
    serialize_args(args, &mut floating_pane_node_children);
    serialize_plugin(plugin, plugin_config, &mut floating_pane_node_children);
    floating_pane_node.set_children(floating_pane_node_children);
    floating_pane_node
}

fn stack_layout_from_manifest(
    geoms: &Vec<PaneLayoutManifest>,
    split_size: Option<SplitSize>,
) -> Option<TiledPaneLayout> {
    let mut children_stacks: HashMap<usize, Vec<PaneLayoutManifest>> = HashMap::new();
    for p in geoms {
        if let Some(stack_id) = p.geom.stacked {
            children_stacks
                .entry(stack_id)
                .or_insert_with(Default::default)
                .push(p.clone());
        }
    }
    let mut stack_nodes = vec![];
    for (_stack_id, stacked_panes) in children_stacks.into_iter() {
        stack_nodes.push(TiledPaneLayout {
            split_size,
            children: stacked_panes
                .iter()
                .map(|p| tiled_pane_layout_from_manifest(Some(p), None))
                .collect(),
            children_are_stacked: true,
            ..Default::default()
        })
    }
    if stack_nodes.len() == 1 {
        // if there's only one stack, we return it without a wrapper
        stack_nodes.iter().next().cloned()
    } else {
        // here there is more than one stack, so we wrap it in a logical container node
        Some(TiledPaneLayout {
            split_size,
            children: stack_nodes,
            ..Default::default()
        })
    }
}

fn tiled_pane_layout_from_manifest(
    manifest: Option<&PaneLayoutManifest>,
    split_size: Option<SplitSize>,
) -> TiledPaneLayout {
    let (run, borderless, is_expanded_in_stack, name, focus, pane_initial_contents) = manifest
        .map(|g| {
            let mut run = g.run.clone();
            if let Some(cwd) = &g.cwd {
                if let Some(run) = run.as_mut() {
                    run.add_cwd(cwd);
                } else {
                    run = Some(Run::Cwd(cwd.clone()));
                }
            }
            (
                run,
                g.is_borderless,
                g.geom.is_stacked() && g.geom.rows.inner > 1,
                g.title.clone(),
                Some(g.is_focused),
                g.pane_contents.clone(),
            )
        })
        .unwrap_or((None, false, false, None, None, None));
    TiledPaneLayout {
        split_size,
        run,
        borderless,
        is_expanded_in_stack,
        name,
        focus,
        pane_initial_contents,
        ..Default::default()
    }
}

/// Tab-level parsing
fn get_tiled_panes_layout_from_panegeoms(
    geoms: &Vec<PaneLayoutManifest>,
    split_size: Option<SplitSize>,
) -> Option<TiledPaneLayout> {
    let (children_split_direction, splits) = match get_splits(&geoms) {
        Some(x) => x,
        None => {
            if geoms.len() > 1 {
                // this can only happen if all geoms belong to one or more stacks
                // since stack splits are discounted in the get_splits method
                return stack_layout_from_manifest(geoms, split_size);
            } else {
                return Some(tiled_pane_layout_from_manifest(
                    geoms.iter().next(),
                    split_size,
                ));
            }
        },
    };
    let mut children = Vec::new();
    let mut remaining_geoms = geoms.clone();
    let mut new_geoms = Vec::new();
    let mut new_constraints = Vec::new();
    for i in 1..splits.len() {
        let (v_min, v_max) = (splits[i - 1], splits[i]);
        let subgeoms: Vec<PaneLayoutManifest>;
        (subgeoms, remaining_geoms) = match children_split_direction {
            SplitDirection::Horizontal => remaining_geoms
                .clone()
                .into_iter()
                .partition(|g| g.geom.y + g.geom.rows.as_usize() <= v_max),
            SplitDirection::Vertical => remaining_geoms
                .clone()
                .into_iter()
                .partition(|g| g.geom.x + g.geom.cols.as_usize() <= v_max),
        };
        match get_domain_constraint(&subgeoms, &children_split_direction, (v_min, v_max)) {
            Some(constraint) => {
                new_geoms.push(subgeoms);
                new_constraints.push(constraint);
            },
            None => {
                return None;
            },
        }
    }

    if let Some(SplitSize::Fixed(fixed_size)) = split_size {
        if fixed_size == 1 && !new_geoms.is_empty() {
            // invalid state, likely an off-by-one error somewhere, we do not serialize
            log::error!("invalid state, not serializing");
            return None;
        }
    }

    let new_split_sizes = get_split_sizes(&new_constraints);

    for (subgeoms, subsplit_size) in new_geoms.iter().zip(new_split_sizes) {
        match get_tiled_panes_layout_from_panegeoms(&subgeoms, subsplit_size) {
            Some(child) => {
                children.push(child);
            },
            None => {
                return None;
            },
        }
    }
    let children_are_stacked = children_split_direction == SplitDirection::Horizontal
        && new_geoms
            .iter()
            .all(|c| c.iter().all(|c| c.geom.is_stacked()));
    Some(TiledPaneLayout {
        children_split_direction,
        split_size,
        children,
        children_are_stacked,
        ..Default::default()
    })
}

fn get_floating_panes_layout_from_panegeoms(
    manifests: &Vec<PaneLayoutManifest>,
) -> Vec<FloatingPaneLayout> {
    manifests
        .iter()
        .map(|m| {
            let mut run = m.run.clone();
            if let Some(cwd) = &m.cwd {
                run.as_mut().map(|r| r.add_cwd(cwd));
            }
            FloatingPaneLayout {
                name: m.title.clone(),
                height: Some(m.geom.rows.into()),
                width: Some(m.geom.cols.into()),
                x: Some(PercentOrFixed::Fixed(m.geom.x)),
                y: Some(PercentOrFixed::Fixed(m.geom.y)),
                pinned: Some(m.geom.is_pinned),
                run,
                focus: Some(m.is_focused),
                already_running: false,
                pane_initial_contents: m.pane_contents.clone(),
                logical_position: None,
            }
        })
        .collect()
}

fn get_x_lims(geoms: &Vec<PaneLayoutManifest>) -> Option<(usize, usize)> {
    match (
        geoms.iter().map(|g| g.geom.x).min(),
        geoms
            .iter()
            .map(|g| g.geom.x + g.geom.cols.as_usize())
            .max(),
    ) {
        (Some(x_min), Some(x_max)) => Some((x_min, x_max)),
        _ => None,
    }
}

fn get_y_lims(geoms: &Vec<PaneLayoutManifest>) -> Option<(usize, usize)> {
    match (
        geoms.iter().map(|g| g.geom.y).min(),
        geoms
            .iter()
            .map(|g| g.geom.y + g.geom.rows.as_usize())
            .max(),
    ) {
        (Some(y_min), Some(y_max)) => Some((y_min, y_max)),
        _ => None,
    }
}

/// Returns the `SplitDirection` as well as the values, on the axis
/// perpendicular the `SplitDirection`, for which there is a split spanning
/// the max_cols or max_rows of the domain. The values are ordered
/// increasingly and contains the boundaries of the domain.
fn get_splits(geoms: &Vec<PaneLayoutManifest>) -> Option<(SplitDirection, Vec<usize>)> {
    if geoms.len() == 1 {
        return None;
    }
    let (x_lims, y_lims) = match (get_x_lims(&geoms), get_y_lims(&geoms)) {
        (Some(x_lims), Some(y_lims)) => (x_lims, y_lims),
        _ => return None,
    };
    let mut direction = SplitDirection::default();
    let mut splits = match direction {
        SplitDirection::Vertical => get_col_splits(&geoms, &x_lims, &y_lims),
        SplitDirection::Horizontal => get_row_splits(&geoms, &x_lims, &y_lims),
    };
    if splits.len() <= 2 {
        // ie only the boundaries are present and no real split has been found
        direction = !direction;
        splits = match direction {
            SplitDirection::Vertical => get_col_splits(&geoms, &x_lims, &y_lims),
            SplitDirection::Horizontal => get_row_splits(&geoms, &x_lims, &y_lims),
        };
    }
    if splits.len() <= 2 {
        // ie no real split has been found in both directions
        None
    } else {
        Some((direction, splits))
    }
}

/// Returns a vector containing the abscisse (x) of the cols that split the
/// domain including the boundaries, ie the min and max abscisse values.
fn get_col_splits(
    geoms: &Vec<PaneLayoutManifest>,
    (_, x_max): &(usize, usize),
    (y_min, y_max): &(usize, usize),
) -> Vec<usize> {
    let max_rows = y_max - y_min;
    let mut splits = Vec::new();
    let mut sorted_geoms = geoms.clone();
    sorted_geoms.sort_by_key(|g| g.geom.x);
    for x in sorted_geoms.iter().map(|g| g.geom.x) {
        if splits.contains(&x) {
            continue;
        }
        if sorted_geoms
            .iter()
            .filter(|g| g.geom.x == x)
            .map(|g| g.geom.rows.as_usize())
            .sum::<usize>()
            == max_rows
        {
            splits.push(x);
        };
    }
    splits.push(*x_max); // Necessary as `g.x` is from the upper-left corner
    splits
}

/// Returns a vector containing the coordinate (y) of the rows that split the
/// domain including the boundaries, ie the min and max coordinate values.
fn get_row_splits(
    geoms: &Vec<PaneLayoutManifest>,
    (x_min, x_max): &(usize, usize),
    (_, y_max): &(usize, usize),
) -> Vec<usize> {
    let max_cols = x_max - x_min;
    let mut splits = Vec::new();
    let mut sorted_geoms = geoms.clone();
    sorted_geoms.sort_by_key(|g| g.geom.y);

    //  here we make sure the various panes in all the stacks aren't counted as splits, since
    //  stacked panes must always stay togethyer - we group them into one "geom" for the purposes
    //  of figuring out their splits
    let mut stack_geoms: HashMap<usize, Vec<PaneLayoutManifest>> = HashMap::new();
    let mut all_geoms = vec![];
    for pane_layout_manifest in sorted_geoms.drain(..) {
        if let Some(stack_id) = pane_layout_manifest.geom.stacked {
            stack_geoms
                .entry(stack_id)
                .or_insert_with(Default::default)
                .push(pane_layout_manifest)
        } else {
            all_geoms.push(pane_layout_manifest);
        }
    }
    for (_stack_id, mut geoms_in_stack) in stack_geoms.into_iter() {
        let mut geom_of_whole_stack = geoms_in_stack.remove(0);
        if let Some(last_geom) = geoms_in_stack.last() {
            geom_of_whole_stack
                .geom
                .rows
                .set_inner(last_geom.geom.y + last_geom.geom.rows.as_usize())
        }
        all_geoms.push(geom_of_whole_stack);
    }

    all_geoms.sort_by_key(|g| g.geom.y);

    for y in all_geoms.iter().map(|g| g.geom.y) {
        if splits.contains(&y) {
            continue;
        }
        if all_geoms
            .iter()
            .filter(|g| g.geom.y == y)
            .map(|g| g.geom.cols.as_usize())
            .sum::<usize>()
            == max_cols
        {
            splits.push(y);
        };
    }
    splits.push(*y_max); // Necessary as `g.y` is from the upper-left corner
    splits
}

/// Get the constraint of the domain considered, base on the rows or columns,
/// depending on the split direction provided.
fn get_domain_constraint(
    geoms: &Vec<PaneLayoutManifest>,
    split_direction: &SplitDirection,
    (v_min, v_max): (usize, usize),
) -> Option<Constraint> {
    match split_direction {
        SplitDirection::Horizontal => get_domain_row_constraint(&geoms, (v_min, v_max)),
        SplitDirection::Vertical => get_domain_col_constraint(&geoms, (v_min, v_max)),
    }
}

fn get_domain_col_constraint(
    geoms: &Vec<PaneLayoutManifest>,
    (x_min, x_max): (usize, usize),
) -> Option<Constraint> {
    let mut percent = 0.0;
    let mut x = x_min;
    while x != x_max {
        // we only look at one (ie the last) geom that has value `x` for `g.x`
        let geom = geoms.iter().filter(|g| g.geom.x == x).last();
        match geom {
            Some(geom) => {
                if let Some(size) = geom.geom.cols.as_percent() {
                    percent += size;
                }
                x += geom.geom.cols.as_usize();
            },
            None => {
                return None;
            },
        }
    }
    if percent == 0.0 {
        Some(Constraint::Fixed(x_max - x_min))
    } else {
        Some(Constraint::Percent(percent))
    }
}

fn get_domain_row_constraint(
    geoms: &Vec<PaneLayoutManifest>,
    (y_min, y_max): (usize, usize),
) -> Option<Constraint> {
    let mut percent = 0.0;
    let mut y = y_min;
    while y != y_max {
        // we only look at one (ie the last) geom that has value `y` for `g.y`
        let geom = geoms.iter().filter(|g| g.geom.y == y).last();
        match geom {
            Some(geom) => {
                if let Some(size) = geom.geom.rows.as_percent() {
                    percent += size;
                }
                y += geom.geom.rows.as_usize();
            },
            None => {
                return None;
            },
        }
    }
    if percent == 0.0 {
        Some(Constraint::Fixed(y_max - y_min))
    } else {
        Some(Constraint::Percent(percent))
    }
}

/// Returns split sizes for all the children of a `TiledPaneLayout` based on
/// their constraints.
fn get_split_sizes(constraints: &Vec<Constraint>) -> Vec<Option<SplitSize>> {
    let mut split_sizes = Vec::new();
    let max_percent = constraints
        .iter()
        .filter_map(|c| match c {
            Constraint::Percent(size) => Some(size),
            _ => None,
        })
        .sum::<f64>();
    for constraint in constraints {
        let split_size = match constraint {
            Constraint::Fixed(size) => Some(SplitSize::Fixed(*size)),
            Constraint::Percent(size) => {
                if size == &max_percent {
                    None
                } else {
                    Some(SplitSize::Percent((100.0 * size / max_percent) as usize))
                }
            },
        };
        split_sizes.push(split_size);
    }
    split_sizes
}

#[cfg(test)]
mod tests {

    use super::*;
    use crate::pane_size::Dimension;
    use expect_test::expect;
    use insta::assert_snapshot;
    use serde_json::Value;
    use std::collections::HashMap;
    const PANEGEOMS_JSON: &[&[&str]] = &[
        &[
            r#"{ "x": 0, "y": 1, "rows": { "constraint": "Percent(100.0)", "inner": 43 }, "cols": { "constraint": "Percent(100.0)", "inner": 211 }, "is_stacked": false }"#,
            r#"{ "x": 0, "y": 0, "rows": { "constraint": "Fixed(1)", "inner": 1 }, "cols": { "constraint": "Percent(100.0)", "inner": 211 }, "is_stacked": false }"#,
            r#"{ "x": 0, "y": 44, "rows": { "constraint": "Fixed(2)", "inner": 2 }, "cols": { "constraint": "Percent(100.0)", "inner": 211 }, "is_stacked": false }"#,
        ],
        &[
            r#"{ "x": 0, "y": 0, "rows": { "constraint": "Percent(100.0)", "inner": 26 }, "cols": { "constraint": "Percent(100.0)", "inner": 211 }, "is_stacked": false }"#,
            r#"{ "x": 0, "y": 26, "rows": { "constraint": "Fixed(20)", "inner": 20 }, "cols": { "constraint": "Fixed(50)", "inner": 50 }, "is_stacked": false }"#,
            r#"{ "x": 50, "y": 26, "rows": { "constraint": "Fixed(20)", "inner": 20 }, "cols": { "constraint": "Percent(100.0)", "inner": 161 }, "is_stacked": false }"#,
        ],
        &[
            r#"{ "x": 0, "y": 0, "rows": { "constraint": "Fixed(10)", "inner": 10 }, "cols": { "constraint": "Percent(50.0)", "inner": 106 }, "is_stacked": false }"#,
            r#"{ "x": 106, "y": 0, "rows": { "constraint": "Fixed(10)", "inner": 10 }, "cols": { "constraint": "Percent(50.0)", "inner": 105 }, "is_stacked": false }"#,
            r#"{ "x": 0, "y": 10, "rows": { "constraint": "Percent(100.0)", "inner": 26 }, "cols": { "constraint": "Fixed(40)", "inner": 40 }, "is_stacked": false }"#,
            r#"{ "x": 40, "y": 10, "rows": { "constraint": "Percent(100.0)", "inner": 26 }, "cols": { "constraint": "Percent(100.0)", "inner": 131 }, "is_stacked": false }"#,
            r#"{ "x": 171, "y": 10, "rows": { "constraint": "Percent(100.0)", "inner": 26 }, "cols": { "constraint": "Fixed(40)", "inner": 40 }, "is_stacked": false }"#,
            r#"{ "x": 0, "y": 36, "rows": { "constraint": "Fixed(10)", "inner": 10 }, "cols": { "constraint": "Percent(50.0)", "inner": 106 }, "is_stacked": false }"#,
            r#"{ "x": 106, "y": 36, "rows": { "constraint": "Fixed(10)", "inner": 10 }, "cols": { "constraint": "Percent(50.0)", "inner": 105 }, "is_stacked": false }"#,
        ],
        &[
            r#"{ "x": 0, "y": 0, "rows": { "constraint": "Percent(30.0)", "inner": 11 }, "cols": { "constraint": "Percent(35.0)", "inner": 74 }, "is_stacked": false }"#,
            r#"{ "x": 0, "y": 11, "rows": { "constraint": "Percent(30.0)", "inner": 11 }, "cols": { "constraint": "Percent(35.0)", "inner": 74 }, "is_stacked": false }"#,
            r#"{ "x": 0, "y": 22, "rows": { "constraint": "Percent(40.0)", "inner": 14 }, "cols": { "constraint": "Percent(35.0)", "inner": 74 }, "is_stacked": false }"#,
            r#"{ "x": 74, "y": 0, "rows": { "constraint": "Percent(100.0)", "inner": 36 }, "cols": { "constraint": "Percent(35.0)", "inner": 74 }, "is_stacked": false }"#,
            r#"{ "x": 0, "y": 36, "rows": { "constraint": "Fixed(10)", "inner": 10 }, "cols": { "constraint": "Percent(70.0)", "inner": 148 }, "is_stacked": false }"#,
            r#"{ "x": 148, "y": 0, "rows": { "constraint": "Percent(100.0)", "inner": 46 }, "cols": { "constraint": "Percent(30.0)", "inner": 63 }, "is_stacked": false }"#,
        ],
        &[
            r#"{ "x": 0, "y": 0, "rows": { "constraint": "Fixed(5)", "inner": 5 }, "cols": { "constraint": "Percent(100.0)", "inner": 211 }, "is_stacked": false }"#,
            r#"{ "x": 0, "y": 5, "rows": { "constraint": "Percent(100.0)", "inner": 36 }, "cols": { "constraint": "Fixed(20)", "inner": 20 }, "is_stacked": false }"#,
            r#"{ "x": 20, "y": 5, "rows": { "constraint": "Percent(100.0)", "inner": 36 }, "cols": { "constraint": "Percent(50.0)", "inner": 86 }, "is_stacked": false }"#,
            r#"{ "x": 106, "y": 5, "rows": { "constraint": "Percent(100.0)", "inner": 36 }, "cols": { "constraint": "Percent(50.0)", "inner": 85 }, "is_stacked": false }"#,
            r#"{ "x": 191, "y": 5, "rows": { "constraint": "Percent(100.0)", "inner": 36 }, "cols": { "constraint": "Fixed(20)", "inner": 20 }, "is_stacked": false }"#,
            r#"{ "x": 0, "y": 41, "rows": { "constraint": "Fixed(5)", "inner": 5 }, "cols": { "constraint": "Percent(100.0)", "inner": 211 }, "is_stacked": false }"#,
        ],
    ];

    #[test]
    fn geoms() {
        let geoms = PANEGEOMS_JSON[0]
            .iter()
            .map(|pg| parse_panegeom_from_json(pg))
            .map(|geom| PaneLayoutManifest {
                geom,
                ..Default::default()
            })
            .collect();
        let tab_layout_manifest = TabLayoutManifest {
            tiled_panes: geoms,
            ..Default::default()
        };
        let global_layout_manifest = GlobalLayoutManifest {
            tabs: vec![("Tab #1".to_owned(), tab_layout_manifest)],
            ..Default::default()
        };
        let kdl = serialize_session_layout(global_layout_manifest).unwrap();
        expect![[r#"
            layout {
                tab name="Tab #1" {
                    pane size=1
                    pane
                    pane size=2
                }
            }
        "#]]
        .assert_eq(&kdl.0);

        let geoms = PANEGEOMS_JSON[1]
            .iter()
            .map(|pg| parse_panegeom_from_json(pg))
            .map(|geom| PaneLayoutManifest {
                geom,
                ..Default::default()
            })
            .collect();
        let tab_layout_manifest = TabLayoutManifest {
            tiled_panes: geoms,
            ..Default::default()
        };
        let global_layout_manifest = GlobalLayoutManifest {
            tabs: vec![("Tab #1".to_owned(), tab_layout_manifest)],
            ..Default::default()
        };
        let kdl = serialize_session_layout(global_layout_manifest).unwrap();
        expect![[r#"
            layout {
                tab name="Tab #1" {
                    pane
                    pane size=20 split_direction="vertical" {
                        pane size=50
                        pane
                    }
                }
            }
        "#]]
        .assert_eq(&kdl.0);

        let geoms = PANEGEOMS_JSON[2]
            .iter()
            .map(|pg| parse_panegeom_from_json(pg))
            .map(|geom| PaneLayoutManifest {
                geom,
                ..Default::default()
            })
            .collect();
        let tab_layout_manifest = TabLayoutManifest {
            tiled_panes: geoms,
            ..Default::default()
        };
        let global_layout_manifest = GlobalLayoutManifest {
            tabs: vec![("Tab #1".to_owned(), tab_layout_manifest)],
            ..Default::default()
        };
        let kdl = serialize_session_layout(global_layout_manifest).unwrap();
        expect![[r#"
            layout {
                tab name="Tab #1" {
                    pane size=10 split_direction="vertical" {
                        pane size="50%"
                        pane size="50%"
                    }
                    pane split_direction="vertical" {
                        pane size=40
                        pane
                        pane size=40
                    }
                    pane size=10 split_direction="vertical" {
                        pane size="50%"
                        pane size="50%"
                    }
                }
            }
        "#]]
        .assert_eq(&kdl.0);

        let geoms = PANEGEOMS_JSON[3]
            .iter()
            .map(|pg| parse_panegeom_from_json(pg))
            .map(|geom| PaneLayoutManifest {
                geom,
                ..Default::default()
            })
            .collect();
        let tab_layout_manifest = TabLayoutManifest {
            tiled_panes: geoms,
            ..Default::default()
        };
        let global_layout_manifest = GlobalLayoutManifest {
            tabs: vec![("Tab #1".to_owned(), tab_layout_manifest)],
            ..Default::default()
        };
        let kdl = serialize_session_layout(global_layout_manifest).unwrap();
        expect![[r#"
            layout {
                tab name="Tab #1" {
                    pane split_direction="vertical" {
                        pane size="70%" {
                            pane split_direction="vertical" {
                                pane size="50%" {
                                    pane size="30%"
                                    pane size="30%"
                                    pane size="40%"
                                }
                                pane size="50%"
                            }
                            pane size=10
                        }
                        pane size="30%"
                    }
                }
            }
        "#]]
        .assert_eq(&kdl.0);

        let geoms = PANEGEOMS_JSON[4]
            .iter()
            .map(|pg| parse_panegeom_from_json(pg))
            .map(|geom| PaneLayoutManifest {
                geom,
                ..Default::default()
            })
            .collect();
        let tab_layout_manifest = TabLayoutManifest {
            tiled_panes: geoms,
            ..Default::default()
        };
        let global_layout_manifest = GlobalLayoutManifest {
            tabs: vec![("Tab #1".to_owned(), tab_layout_manifest)],
            ..Default::default()
        };
        let kdl = serialize_session_layout(global_layout_manifest).unwrap();
        expect![[r#"
            layout {
                tab name="Tab #1" {
                    pane size=5
                    pane split_direction="vertical" {
                        pane size=20
                        pane size="50%"
                        pane size="50%"
                        pane size=20
                    }
                    pane size=5
                }
            }
        "#]]
        .assert_eq(&kdl.0);
    }

    #[test]
    fn global_cwd() {
        let global_layout_manifest = GlobalLayoutManifest {
            global_cwd: Some(PathBuf::from("/path/to/m\"y/global cwd")),
            ..Default::default()
        };
        let kdl = serialize_session_layout(global_layout_manifest).unwrap();
        assert_snapshot!(kdl.0);
    }

    #[test]
    fn can_serialize_tab_name() {
        let global_layout_manifest = GlobalLayoutManifest {
            tabs: vec![("my \"tab \\name".to_owned(), TabLayoutManifest::default())],
            ..Default::default()
        };
        let kdl = serialize_session_layout(global_layout_manifest).unwrap();
        assert_snapshot!(kdl.0);
    }
    #[test]
    fn can_serialize_tab_focus() {
        let tab_layout_manifest = TabLayoutManifest {
            is_focused: true,
            ..Default::default()
        };
        let global_layout_manifest = GlobalLayoutManifest {
            tabs: vec![("Tab #1".to_owned(), tab_layout_manifest)],
            ..Default::default()
        };
        let kdl = serialize_session_layout(global_layout_manifest).unwrap();
        assert_snapshot!(kdl.0);
    }
    #[test]
    fn can_serialize_tab_hide_floating_panes() {
        let tab_layout_manifest = TabLayoutManifest {
            hide_floating_panes: true,
            ..Default::default()
        };
        let global_layout_manifest = GlobalLayoutManifest {
            tabs: vec![("Tab #1".to_owned(), tab_layout_manifest)],
            ..Default::default()
        };
        let kdl = serialize_session_layout(global_layout_manifest).unwrap();
        assert_snapshot!(kdl.0);
    }
    #[test]
    fn can_serialize_tab_with_tiled_panes() {
        use crate::input::command::RunCommand;
        use crate::input::layout::RunPlugin;
        let mut plugin_configuration = BTreeMap::new();
        plugin_configuration.insert("key 1\"\\".to_owned(), "val 1\"\\".to_owned());
        plugin_configuration.insert("key 2\"\\".to_owned(), "val 2\"\\".to_owned());
        let tab_layout_manifest = TabLayoutManifest {
            tiled_panes: vec![
                PaneLayoutManifest {
                    geom: PaneGeom {
                        x: 0,
                        y: 0,
                        rows: Dimension::fixed(10),
                        cols: Dimension::fixed(10),
                        stacked: None,
                        is_pinned: false,
                        logical_position: None,
                    },
                    ..Default::default()
                },
                PaneLayoutManifest {
                    run: Some(Run::Cwd(PathBuf::from("/tmp/\"my/cool cwd"))),
                    geom: PaneGeom {
                        x: 0,
                        y: 10,
                        rows: Dimension::fixed(10),
                        cols: Dimension::fixed(10),
                        stacked: None,
                        is_pinned: false,
                        logical_position: None,
                    },
                    ..Default::default()
                },
                PaneLayoutManifest {
                    run: Some(Run::EditFile(
                        PathBuf::from("/tmp/\"my/cool cwd/my-file"),
                        None,
                        None,
                    )),
                    geom: PaneGeom {
                        x: 0,
                        y: 20,
                        rows: Dimension::fixed(10),
                        cols: Dimension::fixed(10),
                        stacked: None,
                        is_pinned: false,
                        logical_position: None,
                    },
                    ..Default::default()
                },
                PaneLayoutManifest {
                    run: Some(Run::Command(RunCommand {
                        command: PathBuf::from("/tmp/\"my/cool cwd/command.sh"),
                        ..Default::default()
                    })),
                    geom: PaneGeom {
                        x: 0,
                        y: 30,
                        rows: Dimension::fixed(10),
                        cols: Dimension::fixed(10),
                        stacked: None,
                        is_pinned: false,
                        logical_position: None,
                    },
                    ..Default::default()
                },
                PaneLayoutManifest {
                    run: Some(Run::Command(RunCommand {
                        command: PathBuf::from("/tmp/\"my/cool cwd/command.sh"),
                        args: vec![
                            "--arg1".to_owned(),
                            "arg\"2".to_owned(),
                            "arg > \\3".to_owned(),
                        ],
                        ..Default::default()
                    })),
                    geom: PaneGeom {
                        x: 0,
                        y: 40,
                        rows: Dimension::fixed(10),
                        cols: Dimension::fixed(10),
                        stacked: None,
                        is_pinned: false,
                        logical_position: None,
                    },
                    ..Default::default()
                },
                PaneLayoutManifest {
                    run: Some(Run::Plugin(RunPluginOrAlias::RunPlugin(
                        RunPlugin::from_url("file:/tmp/\"my/cool cwd/plugin.wasm").unwrap(),
                    ))),
                    geom: PaneGeom {
                        x: 0,
                        y: 50,
                        rows: Dimension::fixed(10),
                        cols: Dimension::fixed(10),
                        stacked: None,
                        is_pinned: false,
                        logical_position: None,
                    },
                    ..Default::default()
                },
                PaneLayoutManifest {
                    run: Some(Run::Plugin(RunPluginOrAlias::RunPlugin(
                        RunPlugin::from_url("file:/tmp/\"my/cool cwd/plugin.wasm")
                            .unwrap()
                            .with_configuration(plugin_configuration),
                    ))),
                    geom: PaneGeom {
                        x: 0,
                        y: 60,
                        rows: Dimension::fixed(10),
                        cols: Dimension::fixed(10),
                        stacked: None,
                        is_pinned: false,
                        logical_position: None,
                    },
                    ..Default::default()
                },
                PaneLayoutManifest {
                    is_borderless: true,
                    geom: PaneGeom {
                        x: 0,
                        y: 70,
                        rows: Dimension::fixed(10),
                        cols: Dimension::fixed(10),
                        stacked: None,
                        is_pinned: false,
                        logical_position: None,
                    },
                    ..Default::default()
                },
                PaneLayoutManifest {
                    title: Some("my cool \\ \"pane_title\"".to_owned()),
                    is_focused: true,
                    pane_contents: Some("can has pane contents".to_owned()),
                    geom: PaneGeom {
                        x: 0,
                        y: 80,
                        rows: Dimension::fixed(10),
                        cols: Dimension::fixed(10),
                        stacked: None,
                        is_pinned: false,
                        logical_position: None,
                    },
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        let global_layout_manifest = GlobalLayoutManifest {
            tabs: vec![("Tab with \"tiled panes\"".to_owned(), tab_layout_manifest)],
            ..Default::default()
        };
        let kdl = serialize_session_layout(global_layout_manifest).unwrap();
        assert_snapshot!(kdl.0);
    }
    #[test]
    fn can_serialize_tab_with_floating_panes() {
        use crate::input::command::RunCommand;
        use crate::input::layout::RunPlugin;
        let mut plugin_configuration = BTreeMap::new();
        plugin_configuration.insert("key 1\"\\".to_owned(), "val 1\"\\".to_owned());
        plugin_configuration.insert("key 2\"\\".to_owned(), "val 2\"\\".to_owned());
        let tab_layout_manifest = TabLayoutManifest {
            floating_panes: vec![
                PaneLayoutManifest {
                    geom: PaneGeom {
                        x: 0,
                        y: 0,
                        rows: Dimension::fixed(10),
                        cols: Dimension::fixed(10),
                        stacked: None,
                        is_pinned: false,
                        logical_position: None,
                    },
                    ..Default::default()
                },
                PaneLayoutManifest {
                    run: Some(Run::Cwd(PathBuf::from("/tmp/\"my/cool cwd"))),
                    geom: PaneGeom {
                        x: 0,
                        y: 10,
                        rows: Dimension::fixed(10),
                        cols: Dimension::fixed(10),
                        stacked: None,
                        is_pinned: false,
                        logical_position: None,
                    },
                    ..Default::default()
                },
                PaneLayoutManifest {
                    run: Some(Run::EditFile(
                        PathBuf::from("/tmp/\"my/cool cwd/my-file"),
                        None,
                        None,
                    )),
                    geom: PaneGeom {
                        x: 0,
                        y: 20,
                        rows: Dimension::fixed(10),
                        cols: Dimension::fixed(10),
                        stacked: None,
                        is_pinned: false,
                        logical_position: None,
                    },
                    ..Default::default()
                },
                PaneLayoutManifest {
                    run: Some(Run::Command(RunCommand {
                        command: PathBuf::from("/tmp/\"my/cool cwd/command.sh"),
                        ..Default::default()
                    })),
                    geom: PaneGeom {
                        x: 0,
                        y: 30,
                        rows: Dimension::fixed(10),
                        cols: Dimension::fixed(10),
                        stacked: None,
                        is_pinned: false,
                        logical_position: None,
                    },
                    ..Default::default()
                },
                PaneLayoutManifest {
                    run: Some(Run::Command(RunCommand {
                        command: PathBuf::from("/tmp/\"my/cool cwd/command.sh"),
                        args: vec![
                            "--arg1".to_owned(),
                            "arg\"2".to_owned(),
                            "arg > \\3".to_owned(),
                        ],
                        ..Default::default()
                    })),
                    geom: PaneGeom {
                        x: 0,
                        y: 40,
                        rows: Dimension::fixed(10),
                        cols: Dimension::fixed(10),
                        stacked: None,
                        is_pinned: false,
                        logical_position: None,
                    },
                    ..Default::default()
                },
                PaneLayoutManifest {
                    run: Some(Run::Plugin(RunPluginOrAlias::RunPlugin(
                        RunPlugin::from_url("file:/tmp/\"my/cool cwd/plugin.wasm").unwrap(),
                    ))),
                    geom: PaneGeom {
                        x: 0,
                        y: 50,
                        rows: Dimension::fixed(10),
                        cols: Dimension::fixed(10),
                        stacked: None,
                        is_pinned: false,
                        logical_position: None,
                    },
                    ..Default::default()
                },
                PaneLayoutManifest {
                    run: Some(Run::Plugin(RunPluginOrAlias::RunPlugin(
                        RunPlugin::from_url("file:/tmp/\"my/cool cwd/plugin.wasm")
                            .unwrap()
                            .with_configuration(plugin_configuration),
                    ))),
                    geom: PaneGeom {
                        x: 0,
                        y: 60,
                        rows: Dimension::fixed(10),
                        cols: Dimension::fixed(10),
                        stacked: None,
                        is_pinned: false,
                        logical_position: None,
                    },
                    ..Default::default()
                },
                PaneLayoutManifest {
                    // note that in this case, `is_borderless` should be ignored because this is a
                    // floating pane
                    is_borderless: true,
                    geom: PaneGeom {
                        x: 0,
                        y: 70,
                        rows: Dimension::fixed(10),
                        cols: Dimension::fixed(10),
                        stacked: None,
                        is_pinned: false,
                        logical_position: None,
                    },
                    ..Default::default()
                },
                PaneLayoutManifest {
                    title: Some("my cool \\ \"pane_title\"".to_owned()),
                    is_focused: true,
                    pane_contents: Some("can has pane contents".to_owned()),
                    geom: PaneGeom {
                        x: 0,
                        y: 80,
                        rows: Dimension::fixed(10),
                        cols: Dimension::fixed(10),
                        stacked: None,
                        is_pinned: false,
                        logical_position: None,
                    },
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        let global_layout_manifest = GlobalLayoutManifest {
            tabs: vec![(
                "Tab with \"floating panes\"".to_owned(),
                tab_layout_manifest,
            )],
            ..Default::default()
        };
        let kdl = serialize_session_layout(global_layout_manifest).unwrap();
        assert_snapshot!(kdl.0);
    }
    #[test]
    fn can_serialize_tab_with_stacked_panes() {
        let tab_layout_manifest = TabLayoutManifest {
            tiled_panes: vec![
                PaneLayoutManifest {
                    geom: PaneGeom {
                        x: 0,
                        y: 0,
                        rows: Dimension::fixed(1),
                        cols: Dimension::fixed(10),
                        stacked: Some(0),
                        is_pinned: false,
                        logical_position: None,
                    },
                    ..Default::default()
                },
                PaneLayoutManifest {
                    geom: PaneGeom {
                        x: 0,
                        y: 1,
                        rows: Dimension::fixed(10),
                        cols: Dimension::fixed(10),
                        stacked: Some(0),
                        is_pinned: false,
                        logical_position: None,
                    },
                    ..Default::default()
                },
                PaneLayoutManifest {
                    geom: PaneGeom {
                        x: 0,
                        y: 11,
                        rows: Dimension::fixed(1),
                        cols: Dimension::fixed(10),
                        stacked: Some(0),
                        is_pinned: false,
                        logical_position: None,
                    },
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        let global_layout_manifest = GlobalLayoutManifest {
            tabs: vec![("Tab with \"stacked panes\"".to_owned(), tab_layout_manifest)],
            ..Default::default()
        };
        let kdl = serialize_session_layout(global_layout_manifest).unwrap();
        assert_snapshot!(kdl.0);
    }
    #[test]
    fn can_serialize_tab_with_multiple_stacked_panes_in_the_same_node() {
        let tab_layout_manifest = TabLayoutManifest {
            tiled_panes: vec![
                PaneLayoutManifest {
                    geom: PaneGeom {
                        x: 0,
                        y: 0,
                        rows: Dimension::fixed(1),
                        cols: Dimension::fixed(10),
                        stacked: Some(0),
                        is_pinned: false,
                        logical_position: None,
                    },
                    ..Default::default()
                },
                PaneLayoutManifest {
                    geom: PaneGeom {
                        x: 0,
                        y: 1,
                        rows: Dimension::fixed(10),
                        cols: Dimension::fixed(10),
                        stacked: Some(0),
                        is_pinned: false,
                        logical_position: None,
                    },
                    ..Default::default()
                },
                PaneLayoutManifest {
                    geom: PaneGeom {
                        x: 0,
                        y: 11,
                        rows: Dimension::fixed(1),
                        cols: Dimension::fixed(10),
                        stacked: Some(0),
                        is_pinned: false,
                        logical_position: None,
                    },
                    ..Default::default()
                },
                PaneLayoutManifest {
                    geom: PaneGeom {
                        x: 0,
                        y: 12,
                        rows: Dimension::fixed(10),
                        cols: Dimension::fixed(10),
                        stacked: None,
                        is_pinned: false,
                        logical_position: None,
                    },
                    ..Default::default()
                },
                PaneLayoutManifest {
                    geom: PaneGeom {
                        x: 0,
                        y: 22,
                        rows: Dimension::fixed(1),
                        cols: Dimension::fixed(10),
                        stacked: Some(1),
                        is_pinned: false,
                        logical_position: None,
                    },
                    ..Default::default()
                },
                PaneLayoutManifest {
                    geom: PaneGeom {
                        x: 0,
                        y: 23,
                        rows: Dimension::fixed(10),
                        cols: Dimension::fixed(10),
                        stacked: Some(1),
                        is_pinned: false,
                        logical_position: None,
                    },
                    ..Default::default()
                },
                PaneLayoutManifest {
                    geom: PaneGeom {
                        x: 0,
                        y: 33,
                        rows: Dimension::fixed(1),
                        cols: Dimension::fixed(10),
                        stacked: Some(1),
                        is_pinned: false,
                        logical_position: None,
                    },
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        let global_layout_manifest = GlobalLayoutManifest {
            tabs: vec![("Tab with \"stacked panes\"".to_owned(), tab_layout_manifest)],
            ..Default::default()
        };
        let kdl = serialize_session_layout(global_layout_manifest).unwrap();
        assert_snapshot!(kdl.0);
    }
    #[test]
    fn can_serialize_tab_with_multiple_stacks_next_to_eachother() {
        let tab_layout_manifest = TabLayoutManifest {
            tiled_panes: vec![
                PaneLayoutManifest {
                    geom: PaneGeom {
                        x: 0,
                        y: 0,
                        rows: Dimension::fixed(1),
                        cols: Dimension::fixed(10),
                        stacked: Some(0),
                        is_pinned: false,
                        logical_position: None,
                    },
                    ..Default::default()
                },
                PaneLayoutManifest {
                    geom: PaneGeom {
                        x: 0,
                        y: 1,
                        rows: Dimension::fixed(10),
                        cols: Dimension::fixed(10),
                        stacked: Some(0),
                        is_pinned: false,
                        logical_position: None,
                    },
                    ..Default::default()
                },
                PaneLayoutManifest {
                    geom: PaneGeom {
                        x: 0,
                        y: 11,
                        rows: Dimension::fixed(1),
                        cols: Dimension::fixed(10),
                        stacked: Some(0),
                        is_pinned: false,
                        logical_position: None,
                    },
                    ..Default::default()
                },
                PaneLayoutManifest {
                    geom: PaneGeom {
                        x: 0,
                        y: 12,
                        rows: Dimension::fixed(10),
                        cols: Dimension::fixed(10),
                        stacked: None,
                        is_pinned: false,
                        logical_position: None,
                    },
                    ..Default::default()
                },
                PaneLayoutManifest {
                    geom: PaneGeom {
                        x: 0,
                        y: 22,
                        rows: Dimension::fixed(1),
                        cols: Dimension::fixed(10),
                        stacked: Some(1),
                        is_pinned: false,
                        logical_position: None,
                    },
                    ..Default::default()
                },
                PaneLayoutManifest {
                    geom: PaneGeom {
                        x: 0,
                        y: 23,
                        rows: Dimension::fixed(10),
                        cols: Dimension::fixed(10),
                        stacked: Some(1),
                        is_pinned: false,
                        logical_position: None,
                    },
                    ..Default::default()
                },
                PaneLayoutManifest {
                    geom: PaneGeom {
                        x: 0,
                        y: 33,
                        rows: Dimension::fixed(1),
                        cols: Dimension::fixed(10),
                        stacked: Some(1),
                        is_pinned: false,
                        logical_position: None,
                    },
                    ..Default::default()
                },
                PaneLayoutManifest {
                    geom: PaneGeom {
                        x: 10,
                        y: 0,
                        rows: Dimension::fixed(1),
                        cols: Dimension::fixed(10),
                        stacked: Some(2),
                        is_pinned: false,
                        logical_position: None,
                    },
                    ..Default::default()
                },
                PaneLayoutManifest {
                    geom: PaneGeom {
                        x: 10,
                        y: 1,
                        rows: Dimension::fixed(10),
                        cols: Dimension::fixed(10),
                        stacked: Some(2),
                        is_pinned: false,
                        logical_position: None,
                    },
                    ..Default::default()
                },
                PaneLayoutManifest {
                    geom: PaneGeom {
                        x: 10,
                        y: 11,
                        rows: Dimension::fixed(1),
                        cols: Dimension::fixed(10),
                        stacked: Some(2),
                        is_pinned: false,
                        logical_position: None,
                    },
                    ..Default::default()
                },
                PaneLayoutManifest {
                    geom: PaneGeom {
                        x: 10,
                        y: 12,
                        rows: Dimension::fixed(10),
                        cols: Dimension::fixed(10),
                        stacked: None,
                        is_pinned: false,
                        logical_position: None,
                    },
                    ..Default::default()
                },
                PaneLayoutManifest {
                    geom: PaneGeom {
                        x: 10,
                        y: 22,
                        rows: Dimension::fixed(1),
                        cols: Dimension::fixed(10),
                        stacked: Some(3),
                        is_pinned: false,
                        logical_position: None,
                    },
                    ..Default::default()
                },
                PaneLayoutManifest {
                    geom: PaneGeom {
                        x: 10,
                        y: 23,
                        rows: Dimension::fixed(10),
                        cols: Dimension::fixed(10),
                        stacked: Some(3),
                        is_pinned: false,
                        logical_position: None,
                    },
                    ..Default::default()
                },
                PaneLayoutManifest {
                    geom: PaneGeom {
                        x: 10,
                        y: 33,
                        rows: Dimension::fixed(1),
                        cols: Dimension::fixed(10),
                        stacked: Some(3),
                        is_pinned: false,
                        logical_position: None,
                    },
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        let global_layout_manifest = GlobalLayoutManifest {
            tabs: vec![("Tab with \"stacked panes\"".to_owned(), tab_layout_manifest)],
            ..Default::default()
        };
        let kdl = serialize_session_layout(global_layout_manifest).unwrap();
        assert_snapshot!(kdl.0);
    }
    #[test]
    fn can_serialize_multiple_tabs() {
        let tab_1_layout_manifest = TabLayoutManifest {
            tiled_panes: vec![PaneLayoutManifest {
                geom: PaneGeom {
                    x: 0,
                    y: 0,
                    rows: Dimension::percent(100.0),
                    cols: Dimension::percent(100.0),
                    stacked: None,
                    is_pinned: false,
                    logical_position: None,
                },
                ..Default::default()
            }],
            ..Default::default()
        };
        let tab_2_layout_manifest = TabLayoutManifest {
            tiled_panes: vec![
                PaneLayoutManifest {
                    geom: PaneGeom {
                        x: 0,
                        y: 0,
                        rows: Dimension::fixed(10),
                        cols: Dimension::fixed(10),
                        stacked: None,
                        is_pinned: false,
                        logical_position: None,
                    },
                    ..Default::default()
                },
                PaneLayoutManifest {
                    geom: PaneGeom {
                        x: 10,
                        y: 0,
                        rows: Dimension::fixed(10),
                        cols: Dimension::fixed(10),
                        stacked: None,
                        is_pinned: false,
                        logical_position: None,
                    },
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        let global_layout_manifest = GlobalLayoutManifest {
            tabs: vec![
                ("First tab".to_owned(), tab_1_layout_manifest),
                ("Second tab".to_owned(), tab_2_layout_manifest),
            ],
            ..Default::default()
        };
        let kdl = serialize_session_layout(global_layout_manifest).unwrap();
        assert_snapshot!(kdl.0);
    }
    #[test]
    fn can_serialize_new_tab_template() {
        let tiled_panes_layout = TiledPaneLayout {
            children: vec![TiledPaneLayout::default(), TiledPaneLayout::default()],
            ..Default::default()
        };

        let floating_panes_layout = vec![
            FloatingPaneLayout::default(),
            FloatingPaneLayout::default(),
            FloatingPaneLayout::default(),
        ];
        let mut default_layout = Layout::default();
        default_layout.template = Some((tiled_panes_layout, floating_panes_layout));
        let default_layout = Box::new(default_layout);
        let global_layout_manifest = GlobalLayoutManifest {
            default_layout,
            ..Default::default()
        };
        let kdl = serialize_session_layout(global_layout_manifest).unwrap();
        assert_snapshot!(kdl.0);
    }
    #[test]
    fn can_serialize_swap_tiled_panes() {
        let tiled_panes_layout = TiledPaneLayout {
            children: vec![TiledPaneLayout::default(), TiledPaneLayout::default()],
            ..Default::default()
        };
        let mut default_layout = Layout::default();
        let mut swap_tiled_layout_1 = BTreeMap::new();
        let mut swap_tiled_layout_2 = BTreeMap::new();
        swap_tiled_layout_1.insert(LayoutConstraint::MaxPanes(1), tiled_panes_layout.clone());
        swap_tiled_layout_1.insert(LayoutConstraint::MinPanes(1), tiled_panes_layout.clone());
        swap_tiled_layout_1.insert(LayoutConstraint::ExactPanes(1), tiled_panes_layout.clone());
        swap_tiled_layout_1.insert(LayoutConstraint::NoConstraint, tiled_panes_layout.clone());
        swap_tiled_layout_2.insert(LayoutConstraint::MaxPanes(2), tiled_panes_layout.clone());
        swap_tiled_layout_2.insert(LayoutConstraint::MinPanes(2), tiled_panes_layout.clone());
        swap_tiled_layout_2.insert(LayoutConstraint::ExactPanes(2), tiled_panes_layout.clone());
        swap_tiled_layout_2.insert(LayoutConstraint::NoConstraint, tiled_panes_layout.clone());

        let swap_tiled_layouts = vec![
            (swap_tiled_layout_1, None),
            (swap_tiled_layout_2, Some("swap_tiled_layout_2".to_owned())),
        ];
        default_layout.swap_tiled_layouts = swap_tiled_layouts;
        let default_layout = Box::new(default_layout);
        let global_layout_manifest = GlobalLayoutManifest {
            default_layout,
            ..Default::default()
        };
        let kdl = serialize_session_layout(global_layout_manifest).unwrap();
        assert_snapshot!(kdl.0);
    }
    #[test]
    fn can_serialize_swap_floating_panes() {
        let floating_panes_layout = vec![
            FloatingPaneLayout::default(),
            FloatingPaneLayout::default(),
            FloatingPaneLayout::default(),
        ];
        let mut default_layout = Layout::default();
        let mut swap_floating_layout_1 = BTreeMap::new();
        let mut swap_floating_layout_2 = BTreeMap::new();
        swap_floating_layout_1.insert(LayoutConstraint::MaxPanes(1), floating_panes_layout.clone());
        swap_floating_layout_1.insert(LayoutConstraint::MinPanes(1), floating_panes_layout.clone());
        swap_floating_layout_1.insert(
            LayoutConstraint::ExactPanes(1),
            floating_panes_layout.clone(),
        );
        swap_floating_layout_1.insert(
            LayoutConstraint::NoConstraint,
            floating_panes_layout.clone(),
        );
        swap_floating_layout_2.insert(LayoutConstraint::MaxPanes(2), floating_panes_layout.clone());
        swap_floating_layout_2.insert(LayoutConstraint::MinPanes(2), floating_panes_layout.clone());
        swap_floating_layout_2.insert(
            LayoutConstraint::ExactPanes(2),
            floating_panes_layout.clone(),
        );
        swap_floating_layout_2.insert(
            LayoutConstraint::NoConstraint,
            floating_panes_layout.clone(),
        );

        let swap_floating_layouts = vec![
            (swap_floating_layout_1, None),
            (
                swap_floating_layout_2,
                Some("swap_floating_layout_2".to_owned()),
            ),
        ];
        default_layout.swap_floating_layouts = swap_floating_layouts;
        let default_layout = Box::new(default_layout);
        let global_layout_manifest = GlobalLayoutManifest {
            default_layout,
            ..Default::default()
        };
        let kdl = serialize_session_layout(global_layout_manifest).unwrap();
        assert_snapshot!(kdl.0);
    }

    // utility functions
    fn parse_panegeom_from_json(data_str: &str) -> PaneGeom {
        //
        // Expects this input
        //
        //  r#"{ "x": 0, "y": 1, "rows": { "constraint": "Percent(100.0)", "inner": 43 }, "cols": { "constraint": "Percent(100.0)", "inner": 211 }, "is_stacked": false }"#,
        //
        let data: HashMap<String, Value> = serde_json::from_str(data_str).unwrap();
        PaneGeom {
            x: data["x"].to_string().parse().unwrap(),
            y: data["y"].to_string().parse().unwrap(),
            rows: get_dim(&data["rows"]),
            cols: get_dim(&data["cols"]),
            stacked: None,
            is_pinned: false,
            logical_position: None,
        }
    }

    fn get_dim(dim_hm: &Value) -> Dimension {
        let constr_str = dim_hm["constraint"].to_string();
        let dim = if constr_str.contains("Fixed") {
            let value = &constr_str[7..constr_str.len() - 2];
            Dimension::fixed(value.parse().unwrap())
        } else if constr_str.contains("Percent") {
            let value = &constr_str[9..constr_str.len() - 2];
            let mut dim = Dimension::percent(value.parse().unwrap());
            dim.set_inner(dim_hm["inner"].to_string().parse().unwrap());
            dim
        } else {
            panic!("Constraint is nor a percent nor fixed");
        };
        dim
    }
}
