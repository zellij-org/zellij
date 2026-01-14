use crate::{panes::tiled_panes::StackedPanes, panes::PaneId, tab::Pane};
use insta::assert_snapshot;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use zellij_utils::errors::prelude::*;
use zellij_utils::input::layout::Run;
use zellij_utils::pane_size::Offset;
use zellij_utils::pane_size::{Dimension, PaneGeom};

use crate::ui::pane_boundaries_frame::FrameParams;
use crate::{
    output::{CharacterChunk, SixelImageChunk},
    pty::VteBytes,
    ClientId,
};
use std::time::Instant;
use zellij_utils::data::{InputMode, PaletteColor, PaneContents};

macro_rules! mock_pane {
    ($pane_id:expr, $dimension:expr, $inner:expr, $x:expr, $y:expr, $logical_position:expr, $mock_panes:expr) => {
        let mut mock_pane_rows = $dimension;
        mock_pane_rows.set_inner($inner);
        let mut mock_pane: Box<dyn Pane> = Box::new(MockPane::new(PaneGeom {
            x: $x,
            y: $y,
            rows: mock_pane_rows,
            cols: Dimension::percent(100.0),
            logical_position: $logical_position,
            ..Default::default()
        }));
        $mock_panes.insert($pane_id, &mut mock_pane);
    };
}

macro_rules! mock_pane_with_cols {
    ($pane_id:expr, $rows_dimension:expr, $rows_inner:expr, $cols_dimension:expr, $cols_inner:expr, $x:expr, $y:expr, $logical_position:expr, $mock_panes:expr) => {
        let mut mock_pane_rows = $rows_dimension;
        mock_pane_rows.set_inner($rows_inner);
        let mut mock_pane_cols = $cols_dimension;
        mock_pane_cols.set_inner($cols_inner);
        let mut mock_pane: Box<dyn Pane> = Box::new(MockPane::new(PaneGeom {
            x: $x,
            y: $y,
            rows: mock_pane_rows,
            cols: mock_pane_cols,
            logical_position: $logical_position,
            ..Default::default()
        }));
        $mock_panes.insert($pane_id, &mut mock_pane);
    };
}
macro_rules! mock_stacked_pane {
    ($pane_id:expr, $dimension:expr, $inner:expr, $x:expr, $y:expr, $logical_position:expr, $mock_panes:expr) => {
        let mut mock_pane_rows = $dimension;
        mock_pane_rows.set_inner($inner);
        let mut mock_pane: Box<dyn Pane> = Box::new(MockPane::new(PaneGeom {
            x: $x,
            y: $y,
            rows: mock_pane_rows,
            cols: Dimension::percent(100.0),
            logical_position: $logical_position,
            stacked: Some(0),
            ..Default::default()
        }));
        $mock_panes.insert($pane_id, &mut mock_pane);
    };
}

macro_rules! mock_stacked_pane_with_id {
    ($pane_id:expr, $dimension:expr, $inner:expr, $x:expr, $y:expr, $logical_position:expr, $mock_panes:expr, $stack_id:expr) => {
        let mut mock_pane_rows = $dimension;
        mock_pane_rows.set_inner($inner);
        let mut mock_pane: Box<dyn Pane> = Box::new(MockPane::new(PaneGeom {
            x: $x,
            y: $y,
            rows: mock_pane_rows,
            cols: Dimension::percent(100.0),
            logical_position: $logical_position,
            stacked: Some($stack_id),
            ..Default::default()
        }));
        $mock_panes.insert($pane_id, &mut mock_pane);
    };
}

macro_rules! mock_stacked_pane_with_cols_and_id {
    ($pane_id:expr, $rows_dimension:expr, $rows_inner:expr, $cols_dimension:expr, $cols_inner:expr, $x:expr, $y:expr, $logical_position:expr, $mock_panes:expr, $stack_id:expr) => {
        let mut mock_pane_rows = $rows_dimension;
        mock_pane_rows.set_inner($rows_inner);
        let mut mock_pane_cols = $cols_dimension;
        mock_pane_cols.set_inner($cols_inner);
        let mut mock_pane: Box<dyn Pane> = Box::new(MockPane::new(PaneGeom {
            x: $x,
            y: $y,
            rows: mock_pane_rows,
            cols: mock_pane_cols,
            logical_position: $logical_position,
            stacked: Some($stack_id),
            ..Default::default()
        }));
        $mock_panes.insert($pane_id, &mut mock_pane);
    };
}

#[test]
fn combine_vertically_aligned_panes_to_stack() {
    let mut mock_panes: HashMap<PaneId, &mut Box<dyn Pane>> = HashMap::new();

    mock_pane!(
        PaneId::Terminal(1),
        Dimension::percent(50.0),
        50,
        0,
        0,
        Some(1),
        mock_panes
    );
    mock_pane!(
        PaneId::Terminal(2),
        Dimension::percent(50.0),
        50,
        0,
        50,
        Some(2),
        mock_panes
    );

    let mock_panes = Rc::new(RefCell::new(mock_panes));
    let pane_id_above = PaneId::Terminal(1);
    let pane_id_below = PaneId::Terminal(2);

    StackedPanes::new(mock_panes.clone())
        .combine_vertically_aligned_panes_to_stack(&pane_id_above, vec![pane_id_below])
        .unwrap();
    let mut pane_geoms_after: Vec<PaneGeom> = mock_panes
        .borrow()
        .values()
        .map(|p| p.current_geom())
        .collect();
    pane_geoms_after.sort_by(|a, b| a.logical_position.cmp(&b.logical_position));
    assert_snapshot!(format!("{:#?}", pane_geoms_after));
}

#[test]
fn combine_vertically_aligned_panes_to_stack_when_lower_pane_is_stacked() {
    let mut mock_panes: HashMap<PaneId, &mut Box<dyn Pane>> = HashMap::new();

    mock_pane!(
        PaneId::Terminal(1),
        Dimension::percent(33.3),
        33,
        0,
        0,
        Some(1),
        mock_panes
    );
    mock_pane!(
        PaneId::Terminal(2),
        Dimension::percent(33.3),
        33,
        0,
        33,
        Some(2),
        mock_panes
    );
    mock_stacked_pane!(
        PaneId::Terminal(3),
        Dimension::fixed(1),
        1,
        0,
        66,
        Some(3),
        mock_panes
    );
    mock_stacked_pane!(
        PaneId::Terminal(4),
        Dimension::percent(33.3),
        33,
        0,
        67,
        Some(4),
        mock_panes
    );

    let mock_panes = Rc::new(RefCell::new(mock_panes));
    let pane_id_above = PaneId::Terminal(2);
    let pane_id_below = PaneId::Terminal(4);

    StackedPanes::new(mock_panes.clone())
        .combine_vertically_aligned_panes_to_stack(&pane_id_above, vec![pane_id_below])
        .unwrap();
    let mut pane_geoms_after: Vec<PaneGeom> = mock_panes
        .borrow()
        .values()
        .map(|p| p.current_geom())
        .collect();
    pane_geoms_after.sort_by(|a, b| a.logical_position.cmp(&b.logical_position));
    assert_snapshot!(format!("{:#?}", pane_geoms_after));
}

#[test]
fn combine_vertically_aligned_panes_to_stack_when_lower_pane_is_stacked_and_flexible_pane_is_on_top_of_stack(
) {
    let mut mock_panes: HashMap<PaneId, &mut Box<dyn Pane>> = HashMap::new();

    mock_pane!(
        PaneId::Terminal(1),
        Dimension::percent(33.3),
        33,
        0,
        0,
        Some(1),
        mock_panes
    );
    mock_pane!(
        PaneId::Terminal(2),
        Dimension::percent(33.3),
        33,
        0,
        33,
        Some(2),
        mock_panes
    );
    mock_stacked_pane!(
        PaneId::Terminal(3),
        Dimension::percent(33.3),
        33,
        0,
        66,
        Some(3),
        mock_panes
    );
    mock_stacked_pane!(
        PaneId::Terminal(4),
        Dimension::fixed(1),
        1,
        0,
        99,
        Some(4),
        mock_panes
    );

    let mock_panes = Rc::new(RefCell::new(mock_panes));
    let pane_id_above = PaneId::Terminal(2);
    let pane_id_below = PaneId::Terminal(3);

    StackedPanes::new(mock_panes.clone())
        .combine_vertically_aligned_panes_to_stack(&pane_id_above, vec![pane_id_below])
        .unwrap();
    let mut pane_geoms_after: Vec<PaneGeom> = mock_panes
        .borrow()
        .values()
        .map(|p| p.current_geom())
        .collect();
    pane_geoms_after.sort_by(|a, b| a.logical_position.cmp(&b.logical_position));
    assert_snapshot!(format!("{:#?}", pane_geoms_after));
}

#[test]
fn combine_vertically_aligned_panes_to_stack_when_lower_pane_is_stacked_and_flexible_pane_is_mid_stack(
) {
    let mut mock_panes: HashMap<PaneId, &mut Box<dyn Pane>> = HashMap::new();

    mock_pane!(
        PaneId::Terminal(1),
        Dimension::percent(33.3),
        33,
        0,
        0,
        Some(1),
        mock_panes
    );
    mock_pane!(
        PaneId::Terminal(2),
        Dimension::percent(33.3),
        33,
        0,
        33,
        Some(2),
        mock_panes
    );
    mock_stacked_pane!(
        PaneId::Terminal(3),
        Dimension::fixed(1),
        1,
        0,
        66,
        Some(3),
        mock_panes
    );
    mock_stacked_pane!(
        PaneId::Terminal(4),
        Dimension::percent(33.3),
        32,
        0,
        67,
        Some(4),
        mock_panes
    );
    mock_stacked_pane!(
        PaneId::Terminal(5),
        Dimension::fixed(1),
        1,
        0,
        99,
        Some(5),
        mock_panes
    );

    let mock_panes = Rc::new(RefCell::new(mock_panes));
    let pane_id_above = PaneId::Terminal(2);
    let pane_id_below = PaneId::Terminal(4);

    StackedPanes::new(mock_panes.clone())
        .combine_vertically_aligned_panes_to_stack(&pane_id_above, vec![pane_id_below])
        .unwrap();
    let mut pane_geoms_after: Vec<PaneGeom> = mock_panes
        .borrow()
        .values()
        .map(|p| p.current_geom())
        .collect();
    pane_geoms_after.sort_by(|a, b| a.logical_position.cmp(&b.logical_position));
    assert_snapshot!(format!("{:#?}", pane_geoms_after));
}

#[test]
fn combine_vertically_aligned_panes_to_stack_when_both_are_stacked() {
    let mut mock_panes: HashMap<PaneId, &mut Box<dyn Pane>> = HashMap::new();

    mock_stacked_pane_with_id!(
        PaneId::Terminal(1),
        Dimension::percent(50.0),
        49,
        0,
        0,
        Some(1),
        mock_panes,
        0
    );
    mock_stacked_pane_with_id!(
        PaneId::Terminal(2),
        Dimension::fixed(1),
        1,
        0,
        49,
        Some(2),
        mock_panes,
        0
    );
    mock_stacked_pane_with_id!(
        PaneId::Terminal(3),
        Dimension::fixed(1),
        1,
        0,
        50,
        Some(3),
        mock_panes,
        1
    );
    mock_stacked_pane_with_id!(
        PaneId::Terminal(4),
        Dimension::percent(50.0),
        48,
        0,
        51,
        Some(4),
        mock_panes,
        1
    );
    mock_stacked_pane_with_id!(
        PaneId::Terminal(5),
        Dimension::fixed(1),
        1,
        0,
        99,
        Some(5),
        mock_panes,
        1
    );

    let mock_panes = Rc::new(RefCell::new(mock_panes));
    let pane_id_above = PaneId::Terminal(2);
    let pane_id_below = PaneId::Terminal(4);

    StackedPanes::new(mock_panes.clone())
        .combine_vertically_aligned_panes_to_stack(&pane_id_above, vec![pane_id_below])
        .unwrap();
    let mut pane_geoms_after: Vec<PaneGeom> = mock_panes
        .borrow()
        .values()
        .map(|p| p.current_geom())
        .collect();
    pane_geoms_after.sort_by(|a, b| a.logical_position.cmp(&b.logical_position));
    assert_snapshot!(format!("{:#?}", pane_geoms_after));
}

#[test]
fn combine_vertically_aligned_panes_to_stack_with_multiple_non_stacked_neighbors() {
    let mut mock_panes: HashMap<PaneId, &mut Box<dyn Pane>> = HashMap::new();

    mock_pane_with_cols!(
        PaneId::Terminal(1),
        Dimension::percent(50.0),
        50,
        Dimension::percent(50.0),
        50,
        0,
        0,
        Some(1),
        mock_panes
    );
    mock_pane_with_cols!(
        PaneId::Terminal(2),
        Dimension::percent(50.0),
        50,
        Dimension::percent(50.0),
        50,
        50,
        0,
        Some(2),
        mock_panes
    );

    mock_stacked_pane_with_id!(
        PaneId::Terminal(3),
        Dimension::fixed(1),
        1,
        0,
        50,
        Some(3),
        mock_panes,
        1
    );
    mock_stacked_pane_with_id!(
        PaneId::Terminal(4),
        Dimension::percent(50.0),
        48,
        0,
        51,
        Some(4),
        mock_panes,
        1
    );
    mock_stacked_pane_with_id!(
        PaneId::Terminal(5),
        Dimension::fixed(1),
        1,
        0,
        99,
        Some(5),
        mock_panes,
        1
    );

    let mock_panes = Rc::new(RefCell::new(mock_panes));
    let root_pane_id = PaneId::Terminal(4);
    let pane_ids_above = vec![PaneId::Terminal(1), PaneId::Terminal(2)];

    StackedPanes::new(mock_panes.clone())
        .combine_vertically_aligned_panes_to_stack(&root_pane_id, pane_ids_above)
        .unwrap();
    let mut pane_geoms_after: Vec<PaneGeom> = mock_panes
        .borrow()
        .values()
        .map(|p| p.current_geom())
        .collect();
    pane_geoms_after.sort_by(|a, b| a.logical_position.cmp(&b.logical_position));
    assert_snapshot!(format!("{:#?}", pane_geoms_after));
}

#[test]
fn combine_vertically_aligned_panes_to_stack_with_multiple_stacked_neighbors() {
    let mut mock_panes: HashMap<PaneId, &mut Box<dyn Pane>> = HashMap::new();

    mock_pane_with_cols!(
        PaneId::Terminal(1),
        Dimension::percent(50.0),
        50,
        Dimension::percent(50.0),
        50,
        0,
        0,
        Some(1),
        mock_panes
    );
    mock_stacked_pane_with_cols_and_id!(
        PaneId::Terminal(2),
        Dimension::percent(50.0),
        49,
        Dimension::percent(50.0),
        50,
        50,
        0,
        Some(2),
        mock_panes,
        2
    );
    mock_stacked_pane_with_cols_and_id!(
        PaneId::Terminal(3),
        Dimension::fixed(1),
        1,
        Dimension::percent(50.0),
        50,
        50,
        49,
        Some(3),
        mock_panes,
        2
    );

    mock_stacked_pane_with_id!(
        PaneId::Terminal(4),
        Dimension::fixed(1),
        1,
        0,
        50,
        Some(4),
        mock_panes,
        1
    );
    mock_stacked_pane_with_id!(
        PaneId::Terminal(5),
        Dimension::percent(50.0),
        48,
        0,
        51,
        Some(5),
        mock_panes,
        1
    );
    mock_stacked_pane_with_id!(
        PaneId::Terminal(6),
        Dimension::fixed(1),
        1,
        0,
        99,
        Some(6),
        mock_panes,
        1
    );

    let mock_panes = Rc::new(RefCell::new(mock_panes));
    let root_pane_id = PaneId::Terminal(5);
    let pane_ids_above = vec![PaneId::Terminal(1), PaneId::Terminal(2)];

    StackedPanes::new(mock_panes.clone())
        .combine_vertically_aligned_panes_to_stack(&root_pane_id, pane_ids_above)
        .unwrap();
    let mut pane_geoms_after: Vec<PaneGeom> = mock_panes
        .borrow()
        .values()
        .map(|p| p.current_geom())
        .collect();
    pane_geoms_after.sort_by(|a, b| a.logical_position.cmp(&b.logical_position));
    assert_snapshot!(format!("{:#?}", pane_geoms_after));
}

#[test]
fn combine_horizontally_aligned_panes_to_stack() {
    let mut mock_panes: HashMap<PaneId, &mut Box<dyn Pane>> = HashMap::new();

    mock_pane_with_cols!(
        PaneId::Terminal(1),
        Dimension::percent(100.0),
        100,
        Dimension::percent(50.0),
        50,
        0,
        0,
        Some(1),
        mock_panes
    );
    mock_pane_with_cols!(
        PaneId::Terminal(2),
        Dimension::percent(50.0),
        50,
        Dimension::percent(50.0),
        50,
        50,
        0,
        Some(2),
        mock_panes
    );
    mock_pane_with_cols!(
        PaneId::Terminal(3),
        Dimension::percent(50.0),
        50,
        Dimension::percent(50.0),
        50,
        50,
        50,
        Some(3),
        mock_panes
    );

    let mock_panes = Rc::new(RefCell::new(mock_panes));
    let pane_id_of_main_stack = PaneId::Terminal(1);
    let neighboring_pane_ids = vec![PaneId::Terminal(2), PaneId::Terminal(3)];

    StackedPanes::new(mock_panes.clone())
        .combine_horizontally_aligned_panes_to_stack(&pane_id_of_main_stack, neighboring_pane_ids)
        .unwrap();
    let mut pane_geoms_after: Vec<PaneGeom> = mock_panes
        .borrow()
        .values()
        .map(|p| p.current_geom())
        .collect();
    pane_geoms_after.sort_by(|a, b| a.logical_position.cmp(&b.logical_position));
    assert_snapshot!(format!("{:#?}", pane_geoms_after));
}

#[test]
fn combine_horizontally_aligned_panes_to_stack_when_left_pane_is_stacked() {
    let mut mock_panes: HashMap<PaneId, &mut Box<dyn Pane>> = HashMap::new();

    mock_stacked_pane_with_cols_and_id!(
        PaneId::Terminal(1),
        Dimension::percent(100.0),
        98,
        Dimension::percent(50.0),
        50,
        0,
        0,
        Some(1),
        mock_panes,
        0
    );
    mock_stacked_pane_with_cols_and_id!(
        PaneId::Terminal(2),
        Dimension::fixed(1),
        1,
        Dimension::percent(50.0),
        50,
        0,
        98,
        Some(2),
        mock_panes,
        0
    );
    mock_stacked_pane_with_cols_and_id!(
        PaneId::Terminal(3),
        Dimension::fixed(1),
        1,
        Dimension::percent(50.0),
        50,
        0,
        99,
        Some(3),
        mock_panes,
        0
    );

    mock_pane_with_cols!(
        PaneId::Terminal(4),
        Dimension::percent(50.0),
        50,
        Dimension::percent(50.0),
        50,
        50,
        0,
        Some(4),
        mock_panes
    );
    mock_pane_with_cols!(
        PaneId::Terminal(5),
        Dimension::percent(50.0),
        50,
        Dimension::percent(50.0),
        50,
        50,
        50,
        Some(5),
        mock_panes
    );

    let mock_panes = Rc::new(RefCell::new(mock_panes));
    let pane_id_of_main_stack = PaneId::Terminal(1);
    let neighboring_pane_ids = vec![PaneId::Terminal(4), PaneId::Terminal(5)];

    StackedPanes::new(mock_panes.clone())
        .combine_horizontally_aligned_panes_to_stack(&pane_id_of_main_stack, neighboring_pane_ids)
        .unwrap();
    let mut pane_geoms_after: Vec<PaneGeom> = mock_panes
        .borrow()
        .values()
        .map(|p| p.current_geom())
        .collect();
    pane_geoms_after.sort_by(|a, b| a.logical_position.cmp(&b.logical_position));
    assert_snapshot!(format!("{:#?}", pane_geoms_after));
}

#[test]
fn combine_horizontally_aligned_panes_to_stack_when_right_pane_is_stacked() {
    let mut mock_panes: HashMap<PaneId, &mut Box<dyn Pane>> = HashMap::new();

    mock_stacked_pane_with_cols_and_id!(
        PaneId::Terminal(1),
        Dimension::percent(100.0),
        98,
        Dimension::percent(50.0),
        50,
        50,
        0,
        Some(1),
        mock_panes,
        0
    );
    mock_stacked_pane_with_cols_and_id!(
        PaneId::Terminal(2),
        Dimension::fixed(1),
        1,
        Dimension::percent(50.0),
        50,
        50,
        98,
        Some(2),
        mock_panes,
        0
    );
    mock_stacked_pane_with_cols_and_id!(
        PaneId::Terminal(3),
        Dimension::fixed(1),
        1,
        Dimension::percent(50.0),
        50,
        50,
        99,
        Some(3),
        mock_panes,
        0
    );

    mock_pane_with_cols!(
        PaneId::Terminal(4),
        Dimension::percent(50.0),
        50,
        Dimension::percent(50.0),
        50,
        0,
        0,
        Some(4),
        mock_panes
    );
    mock_pane_with_cols!(
        PaneId::Terminal(5),
        Dimension::percent(50.0),
        50,
        Dimension::percent(50.0),
        50,
        0,
        50,
        Some(5),
        mock_panes
    );

    let mock_panes = Rc::new(RefCell::new(mock_panes));
    let pane_id_of_main_stack = PaneId::Terminal(1);
    let neighboring_pane_ids = vec![PaneId::Terminal(4), PaneId::Terminal(5)];

    StackedPanes::new(mock_panes.clone())
        .combine_horizontally_aligned_panes_to_stack(&pane_id_of_main_stack, neighboring_pane_ids)
        .unwrap();
    let mut pane_geoms_after: Vec<PaneGeom> = mock_panes
        .borrow()
        .values()
        .map(|p| p.current_geom())
        .collect();
    pane_geoms_after.sort_by(|a, b| a.logical_position.cmp(&b.logical_position));
    assert_snapshot!(format!("{:#?}", pane_geoms_after));
}

#[test]
fn break_pane_out_of_stack_top() {
    let mut mock_panes: HashMap<PaneId, &mut Box<dyn Pane>> = HashMap::new();

    mock_pane!(
        PaneId::Terminal(1),
        Dimension::percent(33.3),
        33,
        0,
        0,
        Some(1),
        mock_panes
    );
    mock_pane!(
        PaneId::Terminal(2),
        Dimension::percent(33.3),
        33,
        0,
        33,
        Some(2),
        mock_panes
    );
    mock_stacked_pane!(
        PaneId::Terminal(3),
        Dimension::percent(33.3),
        1,
        0,
        66,
        Some(3),
        mock_panes
    );
    mock_stacked_pane!(
        PaneId::Terminal(4),
        Dimension::fixed(1),
        32,
        0,
        67,
        Some(4),
        mock_panes
    );
    mock_stacked_pane!(
        PaneId::Terminal(5),
        Dimension::fixed(1),
        1,
        0,
        99,
        Some(5),
        mock_panes
    );

    let mock_panes = Rc::new(RefCell::new(mock_panes));
    let focused_pane = PaneId::Terminal(3);

    // here the bottom pane should be broken out because the focused pane is the top one and should
    // remain in the stack
    StackedPanes::new(mock_panes.clone())
        .break_pane_out_of_stack(&focused_pane)
        .unwrap();
    let mut pane_geoms_after: Vec<PaneGeom> = mock_panes
        .borrow()
        .values()
        .map(|p| p.current_geom())
        .collect();
    pane_geoms_after.sort_by(|a, b| a.logical_position.cmp(&b.logical_position));
    assert_snapshot!(format!("{:#?}", pane_geoms_after));
}

#[test]
fn break_pane_out_of_stack_middle() {
    let mut mock_panes: HashMap<PaneId, &mut Box<dyn Pane>> = HashMap::new();

    mock_pane!(
        PaneId::Terminal(1),
        Dimension::percent(33.3),
        33,
        0,
        0,
        Some(1),
        mock_panes
    );
    mock_pane!(
        PaneId::Terminal(2),
        Dimension::percent(33.3),
        33,
        0,
        33,
        Some(2),
        mock_panes
    );
    mock_stacked_pane!(
        PaneId::Terminal(3),
        Dimension::fixed(1),
        1,
        0,
        66,
        Some(3),
        mock_panes
    );
    mock_stacked_pane!(
        PaneId::Terminal(4),
        Dimension::percent(33.3),
        32,
        0,
        67,
        Some(4),
        mock_panes
    );
    mock_stacked_pane!(
        PaneId::Terminal(5),
        Dimension::fixed(1),
        1,
        0,
        99,
        Some(5),
        mock_panes
    );

    let mock_panes = Rc::new(RefCell::new(mock_panes));
    let focused_pane = PaneId::Terminal(4);

    // here the bottom pane should be broken out (default behavior)
    StackedPanes::new(mock_panes.clone())
        .break_pane_out_of_stack(&focused_pane)
        .unwrap();
    let mut pane_geoms_after: Vec<PaneGeom> = mock_panes
        .borrow()
        .values()
        .map(|p| p.current_geom())
        .collect();
    pane_geoms_after.sort_by(|a, b| a.logical_position.cmp(&b.logical_position));
    assert_snapshot!(format!("{:#?}", pane_geoms_after));
}

#[test]
fn break_pane_out_of_stack_bottom() {
    let mut mock_panes: HashMap<PaneId, &mut Box<dyn Pane>> = HashMap::new();

    mock_pane!(
        PaneId::Terminal(1),
        Dimension::percent(33.3),
        33,
        0,
        0,
        Some(1),
        mock_panes
    );
    mock_pane!(
        PaneId::Terminal(2),
        Dimension::percent(33.3),
        33,
        0,
        33,
        Some(2),
        mock_panes
    );
    mock_stacked_pane!(
        PaneId::Terminal(3),
        Dimension::fixed(1),
        32,
        0,
        66,
        Some(3),
        mock_panes
    );
    mock_stacked_pane!(
        PaneId::Terminal(4),
        Dimension::fixed(1),
        1,
        0,
        98,
        Some(4),
        mock_panes
    );
    mock_stacked_pane!(
        PaneId::Terminal(5),
        Dimension::percent(33.3),
        1,
        0,
        99,
        Some(5),
        mock_panes
    );

    let mock_panes = Rc::new(RefCell::new(mock_panes));
    let focused_pane = PaneId::Terminal(5);

    // here the top pane should be broken out, because the focused pane is the bottom one and it
    // should remain in the stack
    StackedPanes::new(mock_panes.clone())
        .break_pane_out_of_stack(&focused_pane)
        .unwrap();
    let mut pane_geoms_after: Vec<PaneGeom> = mock_panes
        .borrow()
        .values()
        .map(|p| p.current_geom())
        .collect();
    pane_geoms_after.sort_by(|a, b| a.logical_position.cmp(&b.logical_position));
    assert_snapshot!(format!("{:#?}", pane_geoms_after));
}

#[test]
fn break_next_to_last_pane_out_of_stack() {
    let mut mock_panes: HashMap<PaneId, &mut Box<dyn Pane>> = HashMap::new();

    mock_pane!(
        PaneId::Terminal(1),
        Dimension::percent(33.3),
        33,
        0,
        0,
        Some(1),
        mock_panes
    );
    mock_pane!(
        PaneId::Terminal(2),
        Dimension::percent(33.3),
        33,
        0,
        33,
        Some(2),
        mock_panes
    );
    mock_pane!(
        PaneId::Terminal(3),
        Dimension::percent(22.1),
        22,
        0,
        66,
        Some(3),
        mock_panes
    );
    mock_stacked_pane!(
        PaneId::Terminal(4),
        Dimension::percent(11.2),
        11,
        0,
        88,
        Some(4),
        mock_panes
    );
    mock_stacked_pane!(
        PaneId::Terminal(5),
        Dimension::fixed(1),
        1,
        0,
        99,
        Some(5),
        mock_panes
    );

    let mock_panes = Rc::new(RefCell::new(mock_panes));
    let focused_pane = PaneId::Terminal(4);

    StackedPanes::new(mock_panes.clone())
        .break_pane_out_of_stack(&focused_pane)
        .unwrap();
    let mut pane_geoms_after: Vec<PaneGeom> = mock_panes
        .borrow()
        .values()
        .map(|p| p.current_geom())
        .collect();
    pane_geoms_after.sort_by(|a, b| a.logical_position.cmp(&b.logical_position));
    assert_snapshot!(format!("{:#?}", pane_geoms_after));
}

struct MockPane {
    pane_geom: PaneGeom,
}

impl MockPane {
    pub fn new(pane_geom: PaneGeom) -> Self {
        MockPane { pane_geom }
    }
}

impl Pane for MockPane {
    fn x(&self) -> usize {
        unimplemented!()
    }
    fn y(&self) -> usize {
        unimplemented!()
    }
    fn rows(&self) -> usize {
        unimplemented!()
    }
    fn cols(&self) -> usize {
        unimplemented!()
    }
    fn get_content_x(&self) -> usize {
        unimplemented!()
    }
    fn get_content_y(&self) -> usize {
        unimplemented!()
    }
    fn get_content_columns(&self) -> usize {
        unimplemented!()
    }
    fn get_content_rows(&self) -> usize {
        unimplemented!()
    }
    fn reset_size_and_position_override(&mut self) {
        unimplemented!()
    }
    fn set_geom(&mut self, position_and_size: PaneGeom) {
        self.pane_geom = position_and_size;
    }
    fn set_geom_override(&mut self, _pane_geom: PaneGeom) {
        unimplemented!()
    }
    fn handle_pty_bytes(&mut self, _bytes: VteBytes) {
        unimplemented!()
    }
    fn handle_plugin_bytes(&mut self, _client_id: ClientId, _bytes: VteBytes) {
        unimplemented!()
    }
    fn cursor_coordinates(&self, _client_id: Option<ClientId>) -> Option<(usize, usize)> {
        unimplemented!()
    }

    fn position_and_size(&self) -> PaneGeom {
        self.pane_geom.clone()
    }
    fn current_geom(&self) -> PaneGeom {
        self.pane_geom.clone()
    }

    fn geom_override(&self) -> Option<PaneGeom> {
        unimplemented!()
    }
    fn should_render(&self) -> bool {
        unimplemented!()
    }
    fn set_should_render(&mut self, _should_render: bool) {
        unimplemented!()
    }
    fn set_should_render_boundaries(&mut self, _should_render: bool) {
        unimplemented!()
    }
    fn selectable(&self) -> bool {
        unimplemented!()
    }
    fn set_selectable(&mut self, _selectable: bool) {
        unimplemented!()
    }

    fn render(
        &mut self,
        _client_id: Option<ClientId>,
    ) -> Result<Option<(Vec<CharacterChunk>, Option<String>, Vec<SixelImageChunk>)>> {
        unimplemented!()
    }
    fn render_frame(
        &mut self,
        _client_id: ClientId,
        _frame_params: FrameParams,
        _input_mode: InputMode,
    ) -> Result<Option<(Vec<CharacterChunk>, Option<String>)>> {
        unimplemented!()
    }
    fn render_fake_cursor(
        &mut self,
        _cursor_color: PaletteColor,
        _text_color: PaletteColor,
    ) -> Option<String> {
        unimplemented!()
    }
    fn render_terminal_title(&mut self, _input_mode: InputMode) -> String {
        unimplemented!()
    }
    fn update_name(&mut self, _name: &str) {
        unimplemented!()
    }
    fn pid(&self) -> PaneId {
        unimplemented!()
    }
    fn reduce_height(&mut self, _percent: f64) {
        unimplemented!()
    }
    fn increase_height(&mut self, _percent: f64) {
        unimplemented!()
    }
    fn reduce_width(&mut self, _percent: f64) {
        unimplemented!()
    }
    fn increase_width(&mut self, _percent: f64) {
        unimplemented!()
    }
    fn push_down(&mut self, _count: usize) {
        unimplemented!()
    }
    fn push_right(&mut self, _count: usize) {
        unimplemented!()
    }
    fn pull_left(&mut self, _count: usize) {
        unimplemented!()
    }
    fn pull_up(&mut self, _count: usize) {
        unimplemented!()
    }
    fn clear_screen(&mut self) {
        unimplemented!()
    }
    fn scroll_up(&mut self, _count: usize, _client_id: ClientId) {
        unimplemented!()
    }
    fn scroll_down(&mut self, _count: usize, _client_id: ClientId) {
        unimplemented!()
    }
    fn clear_scroll(&mut self) {
        unimplemented!()
    }
    fn is_scrolled(&self) -> bool {
        unimplemented!()
    }
    fn active_at(&self) -> Instant {
        unimplemented!()
    }
    fn set_active_at(&mut self, _instant: Instant) {
        unimplemented!()
    }
    fn set_frame(&mut self, _frame: bool) {
        unimplemented!()
    }
    fn set_content_offset(&mut self, _offset: Offset) {
        unimplemented!()
    }
    fn store_pane_name(&mut self) {
        unimplemented!()
    }
    fn load_pane_name(&mut self) {
        unimplemented!()
    }
    fn set_borderless(&mut self, _borderless: bool) {
        unimplemented!()
    }
    fn borderless(&self) -> bool {
        unimplemented!()
    }
    fn set_exclude_from_sync(&mut self, _exclude_from_sync: bool) {
        unimplemented!()
    }
    fn exclude_from_sync(&self) -> bool {
        unimplemented!()
    }

    fn add_red_pane_frame_color_override(&mut self, _error_text: Option<String>) {
        unimplemented!()
    }
    fn clear_pane_frame_color_override(&mut self, _client_id: Option<ClientId>) {
        unimplemented!()
    }
    fn frame_color_override(&self) -> Option<PaletteColor> {
        unimplemented!()
    }
    fn invoked_with(&self) -> &Option<Run> {
        unimplemented!()
    }
    fn set_title(&mut self, _title: String) {
        unimplemented!()
    }
    fn current_title(&self) -> String {
        unimplemented!()
    }
    fn custom_title(&self) -> Option<String> {
        unimplemented!()
    }
    fn pane_contents(
        &self,
        _client_id: Option<ClientId>,
        _get_full_scrollback: bool,
    ) -> PaneContents {
        unimplemented!()
    }
}
