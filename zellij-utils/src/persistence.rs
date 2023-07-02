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
//!         let kdl_string = geoms_to_kdl_tab(&geoms);
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

const INDENT: &str = "    ";

pub fn tabs_to_kdl(tabs: &[(String, Vec<PaneGeom>)]) -> String {
    let tab_n = tabs.len();
    let mut kdl_layout = format!("layout {{\n");

    for (name, panes) in tabs {
        // log::info!("PANES in tab:{panes:?}");
        let mut kdl_tab = geoms_to_kdl_tab(&name, &panes, tab_n);
        kdl_tab.push_str("\n")
    }

    kdl_layout.push_str("}");

    kdl_layout
}

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

pub fn panegeom_from_hashmap(map: &HashMap<String, usize>) -> PaneGeom {
    PaneGeom {
        x: map["x"] as usize,
        y: map["y"] as usize,
        rows: Dimension::fixed(map["rows"] as usize),
        cols: Dimension::fixed(map["cols"] as usize),
        is_stacked: false,
    }
}

/// Tab declaration
fn geoms_to_kdl_tab(name: &str, geoms: &[PaneGeom], tab_n: usize) -> String {
    let layout = layout_from_geoms(geoms, None);
    // log::info!("TiledLayout: {layout:?}");
    let tab = if &layout.children_split_direction != &SplitDirection::default() {
        vec![layout]
    } else {
        layout.children
    };

    // skip tab decl if the tab is the only one
    let mut kdl_string = match tab_n {
        1 => format!(""),
        _ => format!("tab name=\"{name}\" {{\n"),
    };

    let indent_level = 1;
    for layout in tab {
        kdl_string.push_str(&kdl_string_from_layout(&layout, indent_level));
    }

    // skip tab closing } if the tab is the only one
    match tab_n {
        1 => {},
        _ => kdl_string.push_str("}"),
    };

    kdl_string
}

/// Pane declaration and recursion
fn kdl_string_from_layout(layout: &TiledPaneLayout, indent_level: usize) -> String {
    let mut kdl_string = String::from(&INDENT.repeat(indent_level));
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
            kdl_string.push_str(&kdl_string_from_layout(&pane, indent_level + 1));
        }
        kdl_string.push_str(&INDENT.repeat(indent_level));
        kdl_string.push_str("}\n");
    }
    kdl_string
}

/// Tab-level parsing
fn layout_from_geoms(geoms: &[PaneGeom], split_size: Option<SplitSize>) -> TiledPaneLayout {
    let (children_split_direction, splits) = match splits(&geoms) {
        Some(x) => x,
        None => {
            return TiledPaneLayout {
                split_size,
                ..Default::default()
            }
        },
    };
    log::info!("SPLITS: {splits:?}");

    let mut remaining_geoms = geoms.to_owned();
    let children = match splits {
        splits if splits.len() == 1 => {
            eprintln!("HERE");
            vec![layout_from_subgeoms(
                &mut remaining_geoms,
                children_split_direction,
                (0, splits[0]),
            )]
        },
        _ => {
            eprintln!("OR HERE");

            (1..splits.len())
                .into_iter()
                .map(|i| {
                    let (v_min, v_max) = (splits[i - 1], splits[i]);
                    layout_from_subgeoms(
                        &mut remaining_geoms,
                        children_split_direction,
                        (v_min, v_max),
                    )
                })
                .collect()
        },
    };
    TiledPaneLayout {
        children_split_direction,
        split_size,
        children,
        ..Default::default()
    }
}

fn layout_from_subgeoms(
    remaining_geoms: &mut Vec<PaneGeom>,
    split_direction: SplitDirection,
    (v_min, v_max): (usize, usize),
) -> TiledPaneLayout {
    let subgeoms: Vec<PaneGeom>;
    (subgeoms, *remaining_geoms) = match split_direction {
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
    layout_from_geoms(&subgeoms, Some(subsplit_size))
}

fn x_lims(geoms: &[PaneGeom]) -> Option<(usize, usize)> {
    let x_min = geoms.iter().map(|g| g.x).min();
    let x_max = geoms.iter().map(|g| g.x + g.rows.as_usize()).max();
    match (x_min, x_max) {
        (Some(x_min), Some(x_max)) => Some((x_min, x_max)),
        _ => None,
    }
}

fn y_lims(geoms: &[PaneGeom]) -> Option<(usize, usize)> {
    let y_min = geoms.iter().map(|g| g.y).min();
    let y_max = geoms.iter().map(|g| g.y + g.rows.as_usize()).max();
    match (y_min, y_max) {
        (Some(y_min), Some(y_max)) => Some((y_min, y_max)),
        _ => None,
    }
}

fn splits(geoms: &[PaneGeom]) -> Option<(SplitDirection, Vec<usize>)> {
    log::info!("len: {}", geoms.len());
    if geoms.len() == 1 {
        return None;
    }
    let (x_lims, y_lims) = match (x_lims(&geoms), y_lims(&geoms)) {
        (Some(x_lims), Some(y_lims)) => (x_lims, y_lims),
        _ => return None,
    };
    let mut direction = SplitDirection::default();
    let mut splits = match direction {
        SplitDirection::Vertical => vertical_splits(&geoms, x_lims, y_lims),
        SplitDirection::Horizontal => horizontal_splits(&geoms, x_lims, y_lims),
    };
    log::info!("initial splits: {splits:?}, direction: {direction:?}");
    if splits.len() <= 2 {
        direction = !direction;
        splits = match direction {
            SplitDirection::Vertical => vertical_splits(&geoms, x_lims, y_lims),
            SplitDirection::Horizontal => horizontal_splits(&geoms, x_lims, y_lims),
        };
    }
    log::info!("second step splits: {splits:?}");
    if splits.len() <= 2 {
        None
    } else {
        Some((direction, splits))
    }
}

fn vertical_splits(
    geoms: &[PaneGeom],
    x_lims: (usize, usize),
    y_lims: (usize, usize),
) -> Vec<usize> {
    log::info!("VERTICAL:{}", geoms.len());
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

fn horizontal_splits(
    geoms: &[PaneGeom],
    x_lims: (usize, usize),
    y_lims: (usize, usize),
) -> Vec<usize> {
    log::info!("HORIZONTAL:{}", geoms.len());
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
        constraint: crate::pane_size::Constraint::Fixed(5),
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
        let kdl = geoms_to_kdl_tab("test", &geoms, 2);
        expect![[r#"
            "tab name=\"test\" {\n    pane size=50 \n    pane size=50 split_direction=\"vertical\" {\n        pane size=50 \n        pane size=50 \n    }\n}"
        "#]].assert_debug_eq(&kdl);

        let geoms = parse_geoms_from_json(LAYOUT[1]);
        let kdl = geoms_to_kdl_tab("test", &geoms, 1);
        expect![[r#"
            ""
        "#]]
        .assert_debug_eq(&kdl);

        let geoms = parse_geoms_from_json(LAYOUT[2]);
        let kdl = geoms_to_kdl_tab("test", &geoms, 1);
        expect![[r#"
            "    pane split_direction=\"vertical\" {\n        pane size=60 \n        pane size=40 \n    }\n"
        "#]].assert_debug_eq(&kdl);

        let geoms = parse_geoms_from_json(LAYOUT[3]);
        let kdl = geoms_to_kdl_tab("test", &geoms, 1);
        expect![[r#"
            ""
        "#]]
        .assert_debug_eq(&kdl);

        let geoms = parse_geoms_from_json(LAYOUT[4]);
        let kdl = geoms_to_kdl_tab("test", &geoms, 1);
        expect![[r#"
            "    pane split_direction=\"vertical\" {\n        pane size=40 \n        pane size=40 {\n            pane size=20 \n            pane size=20 split_direction=\"vertical\" {\n                pane size=20 \n                pane size=20 \n            }\n            pane size=20 \n        }\n    }\n"
        "#]].assert_debug_eq(&kdl);

        let geoms = parse_geoms_from_json(LAYOUT[5]);
        let kdl = geoms_to_kdl_tab("test", &geoms, 1);
        expect![[r#"
            "    pane split_direction=\"vertical\" {\n        pane size=60 \n        pane size=60 \n    }\n"
        "#]].assert_debug_eq(&kdl);
    }
}
