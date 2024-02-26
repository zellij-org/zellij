use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::{
    input::layout::PluginUserConfiguration,
    input::layout::{
        FloatingPaneLayout, Layout, PercentOrFixed, Run, RunPluginOrAlias, SplitDirection,
        SplitSize, SwapFloatingLayout, SwapTiledLayout, TiledPaneLayout,
    },
    pane_size::{Constraint, PaneGeom},
};

const INDENT: &str = "    ";
const DOUBLE_INDENT: &str = "        ";
const TRIPLE_INDENT: &str = "            ";

fn indent(s: &str, prefix: &str) -> String {
    let mut result = String::new();
    for line in s.lines() {
        if line.chars().any(|c| !c.is_whitespace()) {
            result.push_str(prefix);
            result.push_str(line);
        }
        result.push('\n');
    }
    result
}

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
    let mut kdl_string = String::from("layout {\n");
    let mut pane_contents = BTreeMap::new();
    stringify_global_cwd(&global_layout_manifest.global_cwd, &mut kdl_string);
    if let Err(e) = stringify_multiple_tabs(
        global_layout_manifest.tabs,
        &mut pane_contents,
        &mut kdl_string,
    ) {
        return Err(e);
    }
    stringify_new_tab_template(
        global_layout_manifest.default_layout.template,
        &mut pane_contents,
        &mut kdl_string,
    );
    stringify_swap_tiled_layouts(
        global_layout_manifest.default_layout.swap_tiled_layouts,
        &mut pane_contents,
        &mut kdl_string,
    );
    stringify_swap_floating_layouts(
        global_layout_manifest.default_layout.swap_floating_layouts,
        &mut pane_contents,
        &mut kdl_string,
    );
    kdl_string.push_str("}");
    Ok((kdl_string, pane_contents))
}

fn stringify_tab(
    tab_name: String,
    is_focused: bool,
    hide_floating_panes: bool,
    tiled_panes: &Vec<PaneLayoutManifest>,
    floating_panes: &Vec<PaneLayoutManifest>,
    pane_contents: &mut BTreeMap<String, String>,
) -> Option<String> {
    let mut kdl_string = String::new();
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
            let mut tab_attributes = vec![format!("name=\"{}\"", tab_name,)];
            if is_focused {
                tab_attributes.push(format!("focus=true"));
            }
            if hide_floating_panes {
                tab_attributes.push(format!("hide_floating_panes=true"));
            }
            kdl_string.push_str(&kdl_string_from_tab(
                &tiled_panes,
                &floating_panes_layout,
                tab_attributes,
                None,
                pane_contents,
            ));
            Some(kdl_string)
        },
        None => {
            return None;
        },
    }
}

/// Redundant with `geoms_to_kdl_tab`
fn kdl_string_from_tab(
    tiled_panes: &Vec<TiledPaneLayout>,
    floating_panes: &Vec<FloatingPaneLayout>,
    node_attributes: Vec<String>,
    node_name: Option<String>,
    pane_contents: &mut BTreeMap<String, String>,
) -> String {
    let mut kdl_string = if node_attributes.is_empty() {
        format!("{} {{\n", node_name.unwrap_or_else(|| "tab".to_owned()))
    } else {
        format!(
            "{} {} {{\n",
            node_name.unwrap_or_else(|| "tab".to_owned()),
            node_attributes.join(" ")
        )
    };
    for tiled_pane_layout in tiled_panes {
        let ignore_size = false;
        let sub_kdl_string =
            kdl_string_from_tiled_pane(&tiled_pane_layout, ignore_size, pane_contents);
        kdl_string.push_str(&indent(&sub_kdl_string, INDENT));
    }
    if !floating_panes.is_empty() {
        kdl_string.push_str(&indent("floating_panes {\n", INDENT));
        for floating_pane_layout in floating_panes {
            let sub_kdl_string =
                kdl_string_from_floating_pane(&floating_pane_layout, pane_contents);
            kdl_string.push_str(&indent(&sub_kdl_string, DOUBLE_INDENT));
        }
        kdl_string.push_str(&indent("}\n", INDENT));
    }
    kdl_string.push_str("}\n");
    kdl_string
}

/// Pane declaration and recursion
fn kdl_string_from_tiled_pane(
    layout: &TiledPaneLayout,
    ignore_size: bool,
    pane_contents: &mut BTreeMap<String, String>,
) -> String {
    let (command, args) = extract_command_and_args(&layout.run);
    let (plugin, plugin_config) = extract_plugin_and_config(&layout.run);
    let (edit, _line_number) = extract_edit_and_line_number(&layout.run);
    let cwd = layout.run.as_ref().and_then(|r| r.get_cwd());
    let has_children = layout.external_children_index.is_some() || !layout.children.is_empty();
    let mut kdl_string = stringify_pane_title_and_attributes(
        &command,
        &edit,
        &layout.name,
        cwd,
        layout.focus,
        &layout.pane_initial_contents,
        pane_contents,
        has_children,
    );

    stringify_tiled_layout_attributes(&layout, ignore_size, &mut kdl_string);
    let has_child_attributes = !layout.children.is_empty()
        || layout.external_children_index.is_some()
        || !args.is_empty()
        || plugin.is_some()
        || command.is_some();
    if has_child_attributes {
        kdl_string.push_str(" {\n");
        stringify_args(args, &mut kdl_string);
        stringify_start_suspended(&command, &mut kdl_string);
        stringify_plugin(plugin, plugin_config, &mut kdl_string);
        if layout.children.is_empty() && layout.external_children_index.is_some() {
            kdl_string.push_str(&indent(&"children\n", INDENT));
        }
        for (i, pane) in layout.children.iter().enumerate() {
            if Some(i) == layout.external_children_index {
                kdl_string.push_str(&indent(&"children\n", INDENT));
            } else {
                let ignore_size = layout.children_are_stacked;
                let sub_kdl_string = kdl_string_from_tiled_pane(&pane, ignore_size, pane_contents);
                kdl_string.push_str(&indent(&sub_kdl_string, INDENT));
            }
        }
        kdl_string.push_str("}\n");
    } else {
        kdl_string.push_str("\n");
    }
    kdl_string
}

fn extract_command_and_args(layout_run: &Option<Run>) -> (Option<String>, Vec<String>) {
    match layout_run {
        Some(Run::Command(run_command)) => (
            Some(run_command.command.display().to_string()),
            run_command.args.clone(),
        ),
        _ => (None, vec![]),
    }
}
fn extract_plugin_and_config(
    layout_run: &Option<Run>,
) -> (Option<String>, Option<PluginUserConfiguration>) {
    match &layout_run {
        Some(Run::Plugin(run_plugin_or_alias)) => match run_plugin_or_alias {
            RunPluginOrAlias::RunPlugin(run_plugin) => (
                Some(run_plugin.location.display()),
                Some(run_plugin.configuration.clone()),
            ),
            RunPluginOrAlias::Alias(plugin_alias) => {
                let name = plugin_alias.name.clone();
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
fn extract_edit_and_line_number(layout_run: &Option<Run>) -> (Option<String>, Option<usize>) {
    match &layout_run {
        // TODO: line number in layouts?
        Some(Run::EditFile(path, line_number, _cwd)) => {
            (Some(path.display().to_string()), line_number.clone())
        },
        _ => (None, None),
    }
}

fn stringify_pane_title_and_attributes(
    command: &Option<String>,
    edit: &Option<String>,
    name: &Option<String>,
    cwd: Option<PathBuf>,
    focus: Option<bool>,
    initial_pane_contents: &Option<String>,
    pane_contents: &mut BTreeMap<String, String>,
    has_children: bool,
) -> String {
    let mut kdl_string = match (&command, &edit) {
        (Some(command), _) => format!("pane command=\"{}\"", command),
        (None, Some(edit)) => format!("pane edit=\"{}\"", edit),
        (None, None) => format!("pane"),
    };
    if let Some(name) = name {
        kdl_string.push_str(&format!(" name=\"{}\"", name));
    }
    if let Some(cwd) = cwd {
        let path = cwd.display().to_string();
        if !path.is_empty() && !has_children {
            kdl_string.push_str(&format!(" cwd=\"{}\"", path));
        }
    }
    if focus.unwrap_or(false) {
        kdl_string.push_str(&" focus=true");
    }
    if let Some(initial_pane_contents) = initial_pane_contents.as_ref() {
        if command.is_none() && edit.is_none() {
            let file_name = format!("initial_contents_{}", pane_contents.keys().len() + 1);
            kdl_string.push_str(&format!(" contents_file=\"{}\"", file_name));
            pane_contents.insert(file_name.to_string(), initial_pane_contents.clone());
        }
    }
    kdl_string
}

fn stringify_args(args: Vec<String>, kdl_string: &mut String) {
    if !args.is_empty() {
        let args = args
            .iter()
            .map(|a| format!("\"{}\"", a))
            .collect::<Vec<_>>()
            .join(" ");
        kdl_string.push_str(&indent(&format!("args {}\n", args), INDENT));
    }
}

fn stringify_plugin(
    plugin: Option<String>,
    plugin_config: Option<PluginUserConfiguration>,
    kdl_string: &mut String,
) {
    if let Some(plugin) = plugin {
        if let Some(plugin_config) =
            plugin_config.and_then(|p| if p.inner().is_empty() { None } else { Some(p) })
        {
            kdl_string.push_str(&indent(
                &format!("plugin location=\"{}\" {{\n", plugin),
                INDENT,
            ));
            for (config_key, config_value) in plugin_config.inner() {
                kdl_string.push_str(&indent(
                    &format!("{} \"{}\"\n", config_key, config_value),
                    INDENT,
                ));
            }
            kdl_string.push_str(&indent("}\n", INDENT));
        } else {
            kdl_string.push_str(&indent(
                &format!("plugin location=\"{}\"\n", plugin),
                INDENT,
            ));
        }
    }
}

fn stringify_tiled_layout_attributes(
    layout: &TiledPaneLayout,
    ignore_size: bool,
    kdl_string: &mut String,
) {
    if !ignore_size {
        match layout.split_size {
            Some(SplitSize::Fixed(size)) => kdl_string.push_str(&format!(" size={size}")),
            Some(SplitSize::Percent(size)) => kdl_string.push_str(&format!(" size=\"{size}%\"")),
            None => (),
        };
    }
    if layout.borderless {
        kdl_string.push_str(&" borderless=true");
    }
    if layout.children_are_stacked {
        kdl_string.push_str(&" stacked=true");
    }
    if layout.is_expanded_in_stack {
        kdl_string.push_str(&" expanded=true");
    }
    if layout.children_split_direction != SplitDirection::default() {
        let direction = match layout.children_split_direction {
            SplitDirection::Horizontal => "horizontal",
            SplitDirection::Vertical => "vertical",
        };
        kdl_string.push_str(&format!(" split_direction=\"{direction}\""));
    }
}

fn stringify_floating_layout_attributes(layout: &FloatingPaneLayout, kdl_string: &mut String) {
    match layout.height {
        Some(PercentOrFixed::Fixed(fixed_height)) => {
            kdl_string.push_str(&indent(&format!("height {}\n", fixed_height), INDENT));
        },
        Some(PercentOrFixed::Percent(percent)) => {
            kdl_string.push_str(&indent(&format!("height \"{}%\"\n", percent), INDENT));
        },
        None => {},
    }
    match layout.width {
        Some(PercentOrFixed::Fixed(fixed_width)) => {
            kdl_string.push_str(&indent(&format!("width {}\n", fixed_width), INDENT));
        },
        Some(PercentOrFixed::Percent(percent)) => {
            kdl_string.push_str(&indent(&format!("width \"{}%\"\n", percent), INDENT));
        },
        None => {},
    }
    match layout.x {
        Some(PercentOrFixed::Fixed(fixed_x)) => {
            kdl_string.push_str(&indent(&format!("x {}\n", fixed_x), INDENT));
        },
        Some(PercentOrFixed::Percent(percent)) => {
            kdl_string.push_str(&indent(&format!("x \"{}%\"\n", percent), INDENT));
        },
        None => {},
    }
    match layout.y {
        Some(PercentOrFixed::Fixed(fixed_y)) => {
            kdl_string.push_str(&indent(&format!("y {}\n", fixed_y), INDENT));
        },
        Some(PercentOrFixed::Percent(percent)) => {
            kdl_string.push_str(&indent(&format!("y \"{}%\"\n", percent), INDENT));
        },
        None => {},
    }
}

fn stringify_start_suspended(command: &Option<String>, kdl_string: &mut String) {
    if command.is_some() {
        kdl_string.push_str(&indent(&"start_suspended true\n", INDENT));
    }
}

fn stringify_global_cwd(global_cwd: &Option<PathBuf>, kdl_string: &mut String) {
    if let Some(global_cwd) = global_cwd {
        kdl_string.push_str(&indent(
            &format!("cwd \"{}\"\n", global_cwd.display()),
            INDENT,
        ));
    }
}

fn stringify_new_tab_template(
    new_tab_template: Option<(TiledPaneLayout, Vec<FloatingPaneLayout>)>,
    pane_contents: &mut BTreeMap<String, String>,
    kdl_string: &mut String,
) {
    if let Some((tiled_panes, floating_panes)) = new_tab_template {
        let tiled_panes = if &tiled_panes.children_split_direction != &SplitDirection::default() {
            vec![tiled_panes]
        } else {
            tiled_panes.children
        };
        kdl_string.push_str(&indent(
            &kdl_string_from_tab(
                &tiled_panes,
                &floating_panes,
                vec![],
                Some(String::from("new_tab_template")),
                pane_contents,
            ),
            INDENT,
        ));
    }
}

fn stringify_swap_tiled_layouts(
    swap_tiled_layouts: Vec<SwapTiledLayout>,
    pane_contents: &mut BTreeMap<String, String>,
    kdl_string: &mut String,
) {
    for swap_tiled_layout in swap_tiled_layouts {
        let swap_tiled_layout_name = swap_tiled_layout.1;
        match &swap_tiled_layout_name {
            Some(name) => kdl_string.push_str(&indent(
                &format!("swap_tiled_layout name=\"{}\" {{\n", name),
                INDENT,
            )),
            None => kdl_string.push_str(&indent("swap_tiled_layout {\n", INDENT)),
        };
        for (layout_constraint, tiled_panes_layout) in swap_tiled_layout.0 {
            let tiled_panes_layout =
                if &tiled_panes_layout.children_split_direction != &SplitDirection::default() {
                    vec![tiled_panes_layout]
                } else {
                    tiled_panes_layout.children
                };
            kdl_string.push_str(&indent(
                &kdl_string_from_tab(
                    &tiled_panes_layout,
                    &vec![],
                    vec![layout_constraint.to_string()],
                    None,
                    pane_contents,
                ),
                DOUBLE_INDENT,
            ));
        }
        kdl_string.push_str(&indent("}", INDENT));
    }
}

fn stringify_swap_floating_layouts(
    swap_floating_layouts: Vec<SwapFloatingLayout>,
    pane_contents: &mut BTreeMap<String, String>,
    kdl_string: &mut String,
) {
    for swap_floating_layout in swap_floating_layouts {
        let swap_floating_layout_name = swap_floating_layout.1;
        match &swap_floating_layout_name {
            Some(name) => kdl_string.push_str(&indent(
                &format!("swap_floating_layout name=\"{}\" {{\n", name),
                INDENT,
            )),
            None => kdl_string.push_str(&indent("swap_floating_layout {\n", INDENT)),
        };
        for (layout_constraint, floating_panes_layout) in swap_floating_layout.0 {
            let has_floating_panes = !floating_panes_layout.is_empty();
            if has_floating_panes {
                kdl_string.push_str(&indent(
                    &format!("floating_panes {} {{\n", layout_constraint),
                    DOUBLE_INDENT,
                ));
            } else {
                kdl_string.push_str(&indent(
                    &format!("floating_panes {}\n", layout_constraint),
                    DOUBLE_INDENT,
                ));
            }
            for floating_pane_layout in floating_panes_layout {
                let sub_kdl_string =
                    kdl_string_from_floating_pane(&floating_pane_layout, pane_contents);
                kdl_string.push_str(&indent(&sub_kdl_string, TRIPLE_INDENT));
            }
            if has_floating_panes {
                kdl_string.push_str(&indent("}\n", DOUBLE_INDENT));
            }
        }
        kdl_string.push_str(&indent("}", INDENT));
    }
}

fn stringify_multiple_tabs(
    tabs: Vec<(String, TabLayoutManifest)>,
    pane_contents: &mut BTreeMap<String, String>,
    kdl_string: &mut String,
) -> Result<(), &'static str> {
    for (tab_name, tab_layout_manifest) in tabs {
        let tiled_panes = tab_layout_manifest.tiled_panes;
        let floating_panes = tab_layout_manifest.floating_panes;
        let hide_floating_panes = tab_layout_manifest.hide_floating_panes;
        let stringified = stringify_tab(
            tab_name.clone(),
            tab_layout_manifest.is_focused,
            hide_floating_panes,
            &tiled_panes,
            &floating_panes,
            pane_contents,
        );
        match stringified {
            Some(stringified) => {
                kdl_string.push_str(&indent(&stringified, INDENT));
            },
            None => {
                return Err("Failed to stringify tab");
            },
        }
    }
    Ok(())
}

fn kdl_string_from_floating_pane(
    layout: &FloatingPaneLayout,
    pane_contents: &mut BTreeMap<String, String>,
) -> String {
    let (command, args) = extract_command_and_args(&layout.run);
    let (plugin, plugin_config) = extract_plugin_and_config(&layout.run);
    let (edit, _line_number) = extract_edit_and_line_number(&layout.run);
    let cwd = layout.run.as_ref().and_then(|r| r.get_cwd());
    let has_children = false;
    let mut kdl_string = stringify_pane_title_and_attributes(
        &command,
        &edit,
        &layout.name,
        cwd,
        layout.focus,
        &layout.pane_initial_contents,
        pane_contents,
        has_children,
    );
    kdl_string.push_str(" {\n");
    stringify_start_suspended(&command, &mut kdl_string);
    stringify_floating_layout_attributes(&layout, &mut kdl_string);
    stringify_args(args, &mut kdl_string);
    stringify_plugin(plugin, plugin_config, &mut kdl_string);
    kdl_string.push_str("}\n");
    kdl_string
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
                g.geom.is_stacked && g.geom.rows.inner > 1,
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
            return Some(tiled_pane_layout_from_manifest(
                geoms.iter().next(),
                split_size,
            ))
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
            .all(|c| c.iter().all(|c| c.geom.is_stacked));
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
                run,
                focus: Some(m.is_focused),
                already_running: false,
                pane_initial_contents: m.pane_contents.clone(),
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
    // geoms: &Vec<(PaneGeom, Option<Vec<String>>)>,
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
    for y in sorted_geoms.iter().map(|g| g.geom.y) {
        if splits.contains(&y) {
            continue;
        }
        if sorted_geoms
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
    // geoms: &Vec<(PaneGeom, Option<Vec<String>>)>,
    geoms: &Vec<PaneLayoutManifest>,
    split_direction: &SplitDirection,
    (v_min, v_max): (usize, usize),
) -> Option<Constraint> {
    match split_direction {
        SplitDirection::Horizontal => get_domain_row_constraint(&geoms, (v_min, v_max)),
        SplitDirection::Vertical => get_domain_col_constraint(&geoms, (v_min, v_max)),
    }
}

// fn get_domain_col_constraint(geoms: &Vec<(PaneGeom, Option<Vec<String>>)>, (x_min, x_max): (usize, usize)) -> Constraint {
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

// fn get_domain_row_constraint(geoms: &Vec<(PaneGeom, Option<Vec<String>>)>, (y_min, y_max): (usize, usize)) -> Constraint {
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
        // let kdl = kdl_string_from_panegeoms(&geoms);
        let kdl = serialize_session_layout(global_layout_manifest).unwrap();
        expect![[r#"layout {
    tab name="Tab #1" {
        pane size=1
        pane
        pane size=2
    }
}"#]]
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
        expect![[r#"layout {
    tab name="Tab #1" {
        pane
        pane size=20 split_direction="vertical" {
            pane size=50
            pane
        }
    }
}"#]]
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
        expect![[r#"layout {
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
}"#]]
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
        expect![[r#"layout {
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
}"#]]
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
        expect![[r#"layout {
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
}"#]]
        .assert_eq(&kdl.0);
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
            is_stacked: data["is_stacked"].to_string().parse().unwrap(),
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
