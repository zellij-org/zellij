//! # Persistence module
//! !WIP! This module is holding the logic for all persistence sessions need
//!
//! # Examples
//! ```rust,no_run
//! fn main() {
//! }
//! ```
//!
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;

use crate::{
    input::layout::{SplitDirection, SplitSize, TiledPaneLayout, Run},
    input::command::RunCommand,
    pane_size::{Constraint, Dimension, PaneGeom},
};


const INDENT: &str = "    ";

/// Copied from textwrap::indent
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
    pub tabs: Vec<(String, TabLayoutManifest)>
}

#[derive(Default, Debug, Clone)]
pub struct TabLayoutManifest {
    pub tiled_panes: Vec<PaneLayoutManifest>,
    pub floating_panes: Vec<PaneLayoutManifest>,
    pub suppressed_panes: Vec<PaneLayoutManifest>,
}

#[derive(Default, Debug, Clone)]
pub struct PaneLayoutManifest {
    pub geom: PaneGeom,
    pub run: Option<Run>,
}

// pub fn tabs_to_kdl(tab_names_to_tiled_panes: &Vec<(String, Vec<(PaneGeom, Option<Vec<String>>)>)>) -> String {
pub fn tabs_to_kdl(global_layout_manifest: GlobalLayoutManifest) -> String {
    let mut kdl_string = String::from("layout {\n");
    for (tab_name, tab_layout_manifest) in global_layout_manifest.tabs {
        let tiled_panes = tab_layout_manifest.tiled_panes;
        kdl_string.push_str(&indent(&stringify_tab(tab_name.clone(), &tiled_panes), INDENT));
    }
    kdl_string.push_str("}");
    kdl_string
}

// pub fn stringify_tab(tab_name: String, tiled_panes: &Vec<(PaneGeom, Option<Vec<String>>)>) -> String {
    // Option<String> is an optional pane command
pub fn stringify_tab(tab_name: String, tiled_panes: &Vec<PaneLayoutManifest>) -> String {
    let mut kdl_string = String::new();
    let layout = get_layout_from_panegeoms(tiled_panes, None);
    let tab = if &layout.children_split_direction != &SplitDirection::default() {
        vec![layout]
    } else {
        layout.children
    };
    kdl_string.push_str(&kdl_string_from_tab(&tab, tab_name));
    kdl_string
}

// pub fn kdl_string_from_panegeoms(geoms: &Vec<(PaneGeom, Option<Vec<String>>)>) -> String {
pub fn kdl_string_from_panegeoms(geoms: &Vec<PaneLayoutManifest>) -> String {
    // Option<String> is an optional pane command
    let mut kdl_string = String::from("layout {\n");
    let layout = get_layout_from_panegeoms(&geoms, None);
    let tab = if &layout.children_split_direction != &SplitDirection::default() {
        vec![layout]
    } else {
        layout.children
    };
    kdl_string.push_str(&indent(&kdl_string_from_tab(&tab, String::new()), INDENT));
    kdl_string.push_str("}");
    kdl_string
}

///
/// Expects this input
///
///  r#"{ "x": 0, "y": 1, "rows": { "constraint": "Percent(100.0)", "inner": 43 }, "cols": { "constraint": "Percent(100.0)", "inner": 211 }, "is_stacked": false }"#,
///
fn parse_panegeom_from_json(data_str: &str) -> PaneGeom {
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

// /// Tab declaration
// fn geoms_to_kdl_tab(name: &str, geoms: &[PaneGeom], tab_n: usize) -> String {
//     let layout = get_layout_from_panegeoms(geoms, None);
//     // log::info!("TiledLayout: {layout:?}");
//     let tab = if &layout.children_split_direction != &SplitDirection::default() {
//         vec![layout]
//     } else {
//         layout.children
//     };

//     // skip tab decl if the tab is the only one
//     let mut kdl_string = match tab_n {
//         1 => format!(""),
//         _ => format!("tab name=\"{name}\" {{\n"),
//     };

//     for layout in tab {
//         kdl_string.push_str(&kdl_string_from_layout(&layout));
//     }

//     // skip tab closing } if the tab is the only one
//     match tab_n {
//         1 => {},
//         _ => kdl_string.push_str("}"),
//     };

//     kdl_string
// }

/// Redundant with `geoms_to_kdl_tab`
fn kdl_string_from_tab(tab: &Vec<TiledPaneLayout>, tab_name: String) -> String {
    let mut kdl_string = if tab_name.is_empty() {
        format!("tab {{\n")
    } else {
        format!("tab name=\"{}\" {{\n", tab_name)
    };
    for tiled_pane_layout in tab {
        let sub_kdl_string = kdl_string_from_layout(&tiled_pane_layout);
        kdl_string.push_str(&indent(&sub_kdl_string, INDENT));
    }
    kdl_string.push_str("}\n");
    kdl_string
}

/// Pane declaration and recursion
fn kdl_string_from_layout(layout: &TiledPaneLayout) -> String {
    let (command, args) = match &layout.run {
        Some(Run::Command(run_command)) => {
            (Some(run_command.command.display()), run_command.args.clone())
        },
        _ => (None, vec![])
    };
    let mut kdl_string = match command {
        Some(command) => format!("pane command=\"{}\"", command),
        None => format!("pane")
    };

    match layout.split_size {
        Some(SplitSize::Fixed(size)) => kdl_string.push_str(&format!(" size={size}")),
        Some(SplitSize::Percent(size)) => kdl_string.push_str(&format!(" size=\"{size}%\"")),
        None => (),
    };
    if layout.children_split_direction != SplitDirection::default() {
        let direction = match layout.children_split_direction {
            SplitDirection::Horizontal => "horizontal",
            SplitDirection::Vertical => "vertical",
        };
        kdl_string.push_str(&format!(" split_direction=\"{direction}\""));
    }
    if layout.children.is_empty() && args.is_empty() {
        kdl_string.push_str("\n");
    } else if !args.is_empty() {
        kdl_string.push_str(" {\n");
        let args = args.iter().map(|a| format!("\"{}\"", a)).collect::<Vec<_>>().join(" ");
        kdl_string.push_str(&indent(&format!("args {}\n", args), INDENT));
        kdl_string.push_str("}\n");
    } else {
        kdl_string.push_str(" {\n");
        for pane in &layout.children {
            let sub_kdl_string = kdl_string_from_layout(&pane);
            kdl_string.push_str(&indent(&sub_kdl_string, INDENT));
        }
        kdl_string.push_str("}\n");
    }
    kdl_string
}

/// Tab-level parsing
fn get_layout_from_panegeoms(
    // geoms: &Vec<(PaneGeom, Option<Vec<String>>)>,
    geoms: &Vec<PaneLayoutManifest>,
    split_size: Option<SplitSize>,
) -> TiledPaneLayout {
    let (children_split_direction, splits) = match get_splits(&geoms) {
        Some(x) => x,
        None => {
            let run = geoms.iter().next().and_then(|g| g.run.clone());
//                 Some(pane_layout_manifest) => pane_layout_manifest.run
//                     let mut command_line = command.iter();
//                     if let Some(command_name) = command_line.next() {
//                         let mut run_command = RunCommand::new(PathBuf::from(command_name));
//                         run_command.args = command_line.map(|c| c.to_owned()).collect();
//                         Some(Run::Command(run_command))
//                     } else {
//                         None
//                     }
//                 },
//                 _ => None,
            // };
            return TiledPaneLayout {
                split_size,
                run,
                ..Default::default()
            }
        },
    };
    let mut children = Vec::new();
    let mut remaining_geoms = geoms.clone();
    let mut new_geoms = Vec::new();
    let mut new_constraints = Vec::new();
    for i in 1..splits.len() {
        let (v_min, v_max) = (splits[i - 1], splits[i]);
        // let subgeoms: Vec<(PaneGeom, Option<Vec<String>>)>;
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
        let constraint =
            get_domain_constraint(&subgeoms, &children_split_direction, (v_min, v_max));
        new_geoms.push(subgeoms);
        new_constraints.push(constraint);
    }
    let new_split_sizes = get_split_sizes(&new_constraints);
    for (subgeoms, subsplit_size) in new_geoms.iter().zip(new_split_sizes) {
        children.push(get_layout_from_panegeoms(&subgeoms, subsplit_size));
    }
    TiledPaneLayout {
        children_split_direction,
        split_size,
        children,
        ..Default::default()
    }
}

// fn get_x_lims(geoms: &Vec<(PaneGeom, Option<Vec<String>>)>) -> Option<(usize, usize)> {
fn get_x_lims(geoms: &Vec<PaneLayoutManifest>) -> Option<(usize, usize)> {
    match (
        geoms.iter().map(|g| g.geom.x).min(),
        geoms.iter().map(|g| g.geom.x + g.geom.cols.as_usize()).max(),
    ) {
        (Some(x_min), Some(x_max)) => Some((x_min, x_max)),
        _ => None,
    }
}

// fn get_y_lims(geoms: &Vec<(PaneGeom, Option<Vec<String>>)>) -> Option<(usize, usize)> {
fn get_y_lims(geoms: &Vec<PaneLayoutManifest>) -> Option<(usize, usize)> {
    match (
        geoms.iter().map(|g| g.geom.y).min(),
        geoms.iter().map(|g| g.geom.y + g.geom.rows.as_usize()).max(),
    ) {
        (Some(y_min), Some(y_max)) => Some((y_min, y_max)),
        _ => None,
    }
}

/// Returns the `SplitDirection` as well as the values, on the axis
/// perpendicular the `SplitDirection`, for which there is a split spanning
/// the max_cols or max_rows of the domain. The values are ordered
/// increasingly and contains the boundaries of the domain.
// fn get_splits(geoms: &Vec<(PaneGeom, Option<Vec<String>>)>) -> Option<(SplitDirection, Vec<usize>)> {
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
    // geoms: &Vec<(PaneGeom, Option<Vec<String>>)>,
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
) -> Constraint {
    match split_direction {
        SplitDirection::Horizontal => get_domain_row_constraint(&geoms, (v_min, v_max)),
        SplitDirection::Vertical => get_domain_col_constraint(&geoms, (v_min, v_max)),
    }
}

// fn get_domain_col_constraint(geoms: &Vec<(PaneGeom, Option<Vec<String>>)>, (x_min, x_max): (usize, usize)) -> Constraint {
fn get_domain_col_constraint(geoms: &Vec<PaneLayoutManifest>, (x_min, x_max): (usize, usize)) -> Constraint {
    let mut percent = 0.0;
    let mut x = x_min;
    while x != x_max {
        // we only look at one (ie the last) geom that has value `x` for `g.x`
        let geom = geoms.iter().filter(|g| g.geom.x == x).last().unwrap();
        if let Some(size) = geom.geom.cols.as_percent() {
            percent += size;
        }
        x += geom.geom.cols.as_usize();
    }
    if percent == 0.0 {
        Constraint::Fixed(x_max - x_min)
    } else {
        Constraint::Percent(percent)
    }
}

// fn get_domain_row_constraint(geoms: &Vec<(PaneGeom, Option<Vec<String>>)>, (y_min, y_max): (usize, usize)) -> Constraint {
fn get_domain_row_constraint(geoms: &Vec<PaneLayoutManifest>, (y_min, y_max): (usize, usize)) -> Constraint {
    let mut percent = 0.0;
    let mut y = y_min;
    while y != y_max {
        // we only look at one (ie the last) geom that has value `y` for `g.y`
        let geom = geoms.iter().filter(|g| g.geom.y == y).last().unwrap();
        if let Some(size) = geom.geom.rows.as_percent() {
            percent += size;
        }
        y += geom.geom.rows.as_usize();
    }
    if percent == 0.0 {
        Constraint::Fixed(y_max - y_min)
    } else {
        Constraint::Percent(percent)
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
    use expect_test::expect;
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
            .collect();
        let kdl = kdl_string_from_panegeoms(&geoms);
        expect![[r#"layout {
    tab {
        pane size=1
        pane
        pane size=2
    }
}"#]]
        .assert_debug_eq(&kdl);

        let geoms = PANEGEOMS_JSON[1]
            .iter()
            .map(|pg| parse_panegeom_from_json(pg))
            .collect();
        let kdl = kdl_string_from_panegeoms(&geoms);
        expect![[r#"layout {
    tab {
        pane
        pane size=20 split_direction="vertical" {
            pane size=50
            pane
        }
    }
}"#]]
        .assert_debug_eq(&kdl);

        let geoms = PANEGEOMS_JSON[2]
            .iter()
            .map(|pg| parse_panegeom_from_json(pg))
            .collect();
        let kdl = kdl_string_from_panegeoms(&geoms);
        expect![[r#"layout {
    tab {
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
        .assert_debug_eq(&kdl);

        let geoms = PANEGEOMS_JSON[3]
            .iter()
            .map(|pg| parse_panegeom_from_json(pg))
            .collect();
        let kdl = kdl_string_from_panegeoms(&geoms);
        expect![[r#"layout {
    tab {
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
        .assert_debug_eq(&kdl);

        let geoms = PANEGEOMS_JSON[4]
            .iter()
            .map(|pg| parse_panegeom_from_json(pg))
            .collect();
        let kdl = kdl_string_from_panegeoms(&geoms);
        expect![[r#"layout {
    tab {
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
        .assert_debug_eq(&kdl);
    }
}
