//! # Persistence module
//! !WIP! This module is holding the logic for all persistence sessions need
//!
//! # Examples
//! ```rust,no_run
//! fn main() {
//!     // Set test data
//!     let vec_string_geoms = vec![
//!         r#"[ {"x": 0, "y": 0, "cols": 100, "rows": 50}, {"x": 0, "y": 50, "rows": 50, "cols": 50}, {"x": 50, "y": 50, "rows": 50, "cols": 50} ]"#,
//!         r#"[{"x": 0, "y": 0, "cols": 80, "rows": 30}, {"x": 0, "y": 30, "rows": 30, "cols": 30}, {"x": 30, "y": 30, "rows": 30, "cols": 50}]"#,
//!         r#"[{"x": 0, "y": 0, "cols": 60, "rows": 40}, {"x": 60, "y": 0, "rows": 40, "cols": 20}, {"x": 0, "y": 40, "rows": 20, "cols": 60}, {"x": 60, "y": 40, "rows": 20, "cols": 20}]"#,
//!         r#"[{"x": 0, "y": 0, "cols": 40, "rows": 20}, {"x": 40, "y": 0, "rows": 20, "cols": 40}, {"x": 0, "y": 20, "rows": 20, "cols": 25}, {"x": 25, "y": 20, "rows": 20, "cols": 30}, {"x": 55, "y": 20, "rows": 20, "cols": 25}, {"x": 0, "y": 40, "rows": 20, "cols": 40}, {"x": 40, "y": 40, "rows": 20, "cols": 40}]"#,
//!         r#"[{"x": 0, "y": 0, "cols": 40, "rows": 30}, {"x": 0, "y": 30, "cols": 40, "rows": 30}, {"x": 40, "y": 0, "cols": 40, "rows":20}, {"x": 40, "y": 20, "cols": 20, "rows": 20}, {"x": 60, "y": 20, "cols": 20, "rows": 20}, {"x": 40, "y": 40, "cols": 40, "rows": 20}]"#,
//!         r#"[{"x": 0, "y": 0, "cols": 30, "rows": 20}, {"x": 0, "y": 20, "cols": 30, "rows": 20}, {"x": 0, "y": 40, "cols": 30, "rows": 10}, {"x": 30, "y": 0, "cols": 30, "rows": 50}, {"x": 0, "y": 50, "cols": 60, "rows": 10}, {"x": 60, "y": 0, "cols": 20, "rows": 60}]"#,
//!     ];
//!     let vec_hashmap_geoms: Vec<Vec<HashMap<String, usize>>> = vec_string_geoms
//!         .iter()
//!         .map(|s| serde_json::from_str(s).unwrap())
//!         .collect();
//!     let vec_geoms: Vec<Vec<PaneGeom>> = vec_hashmap_geoms
//!         .iter()
//!         .map(|hms| {
//!             hms.iter()
//!                 .map(|hm| panegeom_from_hashmap(&hm))
//!                 .collect()
//!         })
//!         .collect();
//!
//!     for (i, geoms) in vec_geoms.iter().enumerate() {
//!         let kdl_string = geoms_to_kdl(&geoms);
//!         println!("========== {i} ==========");
//!         println!("{kdl_string}\n");
//!     }
//! }
//! ```
//!
use crate::{
    input::layout::{SplitDirection, SplitSize, TiledPaneLayout},
    pane_size::{Dimension, PaneGeom},
};
use std::collections::HashMap;

///
/// Expects this input
///
/// r#"[ {"x": 0, "y": 0, "cols": 100, "rows": 50}, {"x": 0, "y": 50, "rows": 50, "cols": 50}, {"x": 50, "y": 50, "rows": 50, "cols": 50} ]"#
///
pub fn parse_geoms_from_json(geoms: &str) -> Vec<PaneGeom> {
    let vec_hashmap_geoms: Vec<HashMap<String, usize>> = serde_json::from_str(geoms).unwrap();
    vec_hashmap_geoms
        .iter()
        .map(panegeom_from_hashmap)
        .collect()
}

pub fn panegeom_from_hashmap(hm: &HashMap<String, usize>) -> PaneGeom {
    PaneGeom {
        x: hm["x"] as usize,
        y: hm["y"] as usize,
        rows: Dimension::fixed(hm["rows"] as usize),
        cols: Dimension::fixed(hm["cols"] as usize),
        is_stacked: false,
    }
}

pub fn geoms_to_kdl(tab_name: &str, geoms: &[PaneGeom]) -> String {
    let layout = get_layout_from_geoms(geoms, None);
    let tab = if &layout.children_split_direction != &SplitDirection::default() {
        vec![layout]
    } else {
        layout.children
    };
    geoms_to_kdl_tab(tab_name, &tab)
}

fn geoms_to_kdl_tab(name: &str, tab: &[TiledPaneLayout]) -> String {
    let mut kdl_string = format!("tab name=\"{name}\"{{\n");
    let indent = "    ";
    let indent_level = 1;
    for layout in tab {
        kdl_string.push_str(&kdl_string_from_layout(&layout, indent, indent_level));
    }
    kdl_string.push_str("}");
    kdl_string
}

fn kdl_string_from_layout(layout: &TiledPaneLayout, indent: &str, indent_level: usize) -> String {
    let mut kdl_string = String::from(&indent.repeat(indent_level));
    kdl_string.push_str("pane ");
    match layout.split_size {
        Some(SplitSize::Fixed(size)) => kdl_string.push_str(&format!("size={size} ")),
        Some(SplitSize::Percent(size)) => kdl_string.push_str(&format!("size={size}% ")),
        None => (),
    };
    if layout.children_split_direction != SplitDirection::default() {
        let direction = match layout.children_split_direction {
            SplitDirection::Horizontal => "horizontal",
            SplitDirection::Vertical => "vertical",
        };
        kdl_string.push_str(&format!("split_direction=\"{direction}\" "));
    }
    if layout.children.is_empty() {
        kdl_string.push_str("\n");
    } else {
        kdl_string.push_str("{\n");
        for pane in &layout.children {
            kdl_string.push_str(&kdl_string_from_layout(&pane, indent, indent_level + 1));
        }
        kdl_string.push_str(&indent.repeat(indent_level));
        kdl_string.push_str("}\n");
    }
    kdl_string
}

fn get_layout_from_geoms(geoms: &[PaneGeom], split_size: Option<SplitSize>) -> TiledPaneLayout {
    let (children_split_direction, splits) = match get_splits(&geoms) {
        Some(x) => x,
        None => {
            return TiledPaneLayout {
                split_size,
                ..Default::default()
            }
        },
    };
    let mut children = Vec::new();
    let mut remaining_geoms = geoms.to_owned();
    for i in 1..splits.len() {
        let (v_min, v_max) = (splits[i - 1], splits[i]);
        let subgeoms: Vec<PaneGeom>;
        (subgeoms, remaining_geoms) = match children_split_direction {
            SplitDirection::Horizontal => remaining_geoms
                .clone()
                .into_iter()
                .partition(|g| g.y + g.rows.as_usize() <= v_max),
            SplitDirection::Vertical => remaining_geoms
                .clone()
                .into_iter()
                .partition(|g| g.x + g.cols.as_usize() <= v_max),
        };
        let subsplit_size = SplitSize::Fixed(v_max - v_min);
        children.push(get_layout_from_geoms(&subgeoms, Some(subsplit_size)));
    }
    TiledPaneLayout {
        children_split_direction,
        split_size,
        children,
        ..Default::default()
    }
}

fn get_x_lims(geoms: &[PaneGeom]) -> Option<(usize, usize)> {
    let x_min = geoms.iter().map(|g| g.x).min();
    let x_max = geoms.iter().map(|g| g.x + g.rows.as_usize()).max();
    match (x_min, x_max) {
        (Some(x_min), Some(x_max)) => Some((x_min, x_max)),
        _ => None,
    }
}

fn get_y_lims(geoms: &[PaneGeom]) -> Option<(usize, usize)> {
    let y_min = geoms.iter().map(|g| g.y).min();
    let y_max = geoms.iter().map(|g| g.y + g.rows.as_usize()).max();
    match (y_min, y_max) {
        (Some(y_min), Some(y_max)) => Some((y_min, y_max)),
        _ => None,
    }
}

fn get_splits(geoms: &[PaneGeom]) -> Option<(SplitDirection, Vec<usize>)> {
    if geoms.len() == 1 {
        return None;
    }
    let (x_lims, y_lims) = match (get_x_lims(&geoms), get_y_lims(&geoms)) {
        (Some(x_lims), Some(y_lims)) => (x_lims, y_lims),
        _ => return None,
    };
    let mut direction = SplitDirection::default();
    let mut splits = match direction {
        SplitDirection::Vertical => get_vertical_splits(&geoms, x_lims, y_lims),
        SplitDirection::Horizontal => get_horizontal_splits(&geoms, x_lims, y_lims),
    };
    if splits.len() <= 2 {
        direction = !direction;
        splits = match direction {
            SplitDirection::Vertical => get_vertical_splits(&geoms, x_lims, y_lims),
            SplitDirection::Horizontal => get_horizontal_splits(&geoms, x_lims, y_lims),
        };
    }
    if splits.len() <= 2 {
        None
    } else {
        Some((direction, splits))
    }
}

fn get_vertical_splits(
    geoms: &[PaneGeom],
    x_lims: (usize, usize),
    y_lims: (usize, usize),
) -> Vec<usize> {
    let ((_, x_max), (y_min, y_max)) = (x_lims, y_lims);
    let height = y_max - y_min;
    let mut splits = Vec::new();
    for x in geoms.iter().map(|g| g.x) {
        if splits.contains(&x) {
            continue;
        }
        if geoms
            .iter()
            .filter(|g| g.x == x)
            .map(|g| g.rows.as_usize())
            .sum::<usize>()
            == height
        {
            splits.push(x);
        };
    }
    splits.push(x_max);
    splits
}

fn get_horizontal_splits(
    geoms: &[PaneGeom],
    x_lims: (usize, usize),
    y_lims: (usize, usize),
) -> Vec<usize> {
    let ((x_min, x_max), (_, y_max)) = (x_lims, y_lims);
    let width = x_max - x_min;
    let mut splits = Vec::new();
    for y in geoms.iter().map(|g| g.y) {
        if splits.contains(&y) {
            continue;
        }
        if geoms
            .iter()
            .filter(|g| g.y == y)
            .map(|g| g.cols.as_usize())
            .sum::<usize>()
            == width
        {
            splits.push(y);
        };
    }
    splits.push(y_max);
    splits
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    const LAYOUT: &[&str] = &[
        r#"[ {"x": 0, "y": 0, "cols": 100, "rows": 50}, {"x": 0, "y": 50, "rows": 50, "cols": 50}, {"x": 50, "y": 50, "rows": 50, "cols": 50} ]"#,
        r#"[{"x": 0, "y": 0, "cols": 80, "rows": 30}, {"x": 0, "y": 30, "rows": 30, "cols": 30}, {"x": 30, "y": 30, "rows": 30, "cols": 50}]"#,
        r#"[{"x": 0, "y": 0, "cols": 60, "rows": 40}, {"x": 60, "y": 0, "rows": 40, "cols": 20}, {"x": 0, "y": 40, "rows": 20, "cols": 60}, {"x": 60, "y": 40, "rows": 20, "cols": 20}]"#,
        r#"[{"x": 0, "y": 0, "cols": 40, "rows": 20}, {"x": 40, "y": 0, "rows": 20, "cols": 40}, {"x": 0, "y": 20, "rows": 20, "cols": 25}, {"x": 25, "y": 20, "rows": 20, "cols": 30}, {"x": 55, "y": 20, "rows": 20, "cols": 25}, {"x": 0, "y": 40, "rows": 20, "cols": 40}, {"x": 40, "y": 40, "rows": 20, "cols": 40}]"#,
        r#"[{"x": 0, "y": 0, "cols": 40, "rows": 30}, {"x": 0, "y": 30, "cols": 40, "rows": 30}, {"x": 40, "y": 0, "cols": 40, "rows":20}, {"x": 40, "y": 20, "cols": 20, "rows": 20}, {"x": 60, "y": 20, "cols": 20, "rows": 20}, {"x": 40, "y": 40, "cols": 40, "rows": 20}]"#,
        r#"[{"x": 0, "y": 0, "cols": 30, "rows": 20}, {"x": 0, "y": 20, "cols": 30, "rows": 20}, {"x": 0, "y": 40, "cols": 30, "rows": 10}, {"x": 30, "y": 0, "cols": 30, "rows": 50}, {"x": 0, "y": 50, "cols": 60, "rows": 10}, {"x": 60, "y": 0, "cols": 20, "rows": 60}]"#,
    ];
    const DIM: Dimension = Dimension {
        constraint: crate::pane_size::Constraint::Fixed(0),
        inner: 0,
    };
    const PANE_GEOM: PaneGeom = PaneGeom {
        x: 0,
        y: 0,
        rows: DIM,
        cols: DIM,
        is_stacked: false,
    };

    #[test]
    fn geoms() {
        let geoms = parse_geoms_from_json(LAYOUT[0]);
        let kdl = geoms_to_kdl("test", &geoms);
        expect![[r#"
            "tab name=\"test\"{\n    pane size=50 \n    pane size=50 split_direction=\"vertical\" {\n        pane size=50 \n        pane size=50 \n    }\n}"
        "#]].assert_debug_eq(&kdl);

        let geoms = parse_geoms_from_json(LAYOUT[1]);
        let kdl = geoms_to_kdl("test", &geoms);
        expect![[r#"
            "tab name=\"test\"{\n}"
        "#]].assert_debug_eq(&kdl);

        let geoms = parse_geoms_from_json(LAYOUT[2]);
        let kdl = geoms_to_kdl("test", &geoms);
        expect![[r#"
            "tab name=\"test\"{\n    pane split_direction=\"vertical\" {\n        pane size=60 \n        pane size=40 \n    }\n}"
        "#]].assert_debug_eq(&kdl);

        let geoms = parse_geoms_from_json(LAYOUT[3]);
        let kdl = geoms_to_kdl("test", &geoms);
        expect![[r#"
            "tab name=\"test\"{\n}"
        "#]].assert_debug_eq(&kdl);

        let geoms = parse_geoms_from_json(LAYOUT[4]);
        let kdl = geoms_to_kdl("test", &geoms);
        expect![[r#"
            "tab name=\"test\"{\n    pane split_direction=\"vertical\" {\n        pane size=40 \n        pane size=40 {\n            pane size=20 \n            pane size=20 split_direction=\"vertical\" {\n                pane size=20 \n                pane size=20 \n            }\n            pane size=20 \n        }\n    }\n}"
        "#]].assert_debug_eq(&kdl);

        let geoms = parse_geoms_from_json(LAYOUT[5]);
        let kdl = geoms_to_kdl("test", &geoms);
        expect![[r#"
            "tab name=\"test\"{\n    pane split_direction=\"vertical\" {\n        pane size=60 \n        pane size=60 \n    }\n}"
        "#]].assert_debug_eq(&kdl);
    }
}
