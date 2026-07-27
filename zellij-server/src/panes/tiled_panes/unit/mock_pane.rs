#![allow(dead_code)]

use super::pane_resizer::PaneResizer;
use crate::panes::PaneId;
use crate::tab::Pane;
use crate::ui::pane_boundaries_frame::FrameParams;
use crate::{
    output::{CharacterChunk, SixelImageChunk},
    pty::VteBytes,
    ClientId,
};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Instant;
use zellij_utils::data::{InputMode, PaletteColor, PaneContents};
use zellij_utils::errors::prelude::*;
use zellij_utils::input::layout::Run as LayoutRun;
use zellij_utils::input::layout::SplitDirection;
use zellij_utils::pane_size::Offset;
use zellij_utils::pane_size::{Dimension, PaneGeom};

#[derive(Clone, Copy, Debug)]
pub struct PaneSpec {
    pub id: PaneId,
    pub x: usize,
    pub y: usize,
    pub cols: Dimension,
    pub rows: Dimension,
    pub stacked: Option<usize>,
    pub logical_position: Option<usize>,
    pub geom_override: Option<PaneGeom>,
}

impl PaneSpec {
    pub fn geom(&self) -> PaneGeom {
        PaneGeom {
            x: self.x,
            y: self.y,
            rows: self.rows,
            cols: self.cols,
            stacked: self.stacked,
            is_pinned: false,
            logical_position: self.logical_position,
        }
    }
}

pub fn dim_pct(percent: f64, inner: usize) -> Dimension {
    let mut dimension = Dimension::percent(percent);
    dimension.set_inner(inner);
    dimension
}

pub fn dim_fixed(size: usize) -> Dimension {
    Dimension::fixed(size)
}

pub fn spec(id: u32, x: usize, y: usize, cols: Dimension, rows: Dimension) -> PaneSpec {
    PaneSpec {
        id: PaneId::Terminal(id),
        x,
        y,
        cols,
        rows,
        stacked: None,
        logical_position: None,
        geom_override: None,
    }
}

pub fn spec_stacked(
    id: u32,
    x: usize,
    y: usize,
    cols: Dimension,
    rows: Dimension,
    stack_id: usize,
) -> PaneSpec {
    PaneSpec {
        id: PaneId::Terminal(id),
        x,
        y,
        cols,
        rows,
        stacked: Some(stack_id),
        logical_position: None,
        geom_override: None,
    }
}

pub fn with_override(base: PaneSpec, geom_override: PaneGeom) -> PaneSpec {
    PaneSpec {
        geom_override: Some(geom_override),
        ..base
    }
}

pub struct MockPane {
    pid: PaneId,
    geom: PaneGeom,
    geom_override: Option<PaneGeom>,
}

impl MockPane {
    pub fn from_spec(spec: PaneSpec) -> Self {
        MockPane {
            pid: spec.id,
            geom: spec.geom(),
            geom_override: spec.geom_override,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct PaneSnapshot {
    pub id: PaneId,
    pub geom: PaneGeom,
    pub current: PaneGeom,
    pub geom_override: Option<PaneGeom>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    Ok,
    PaneSizeUnchanged,
    OtherErr(String),
}

#[derive(Clone, Debug)]
pub struct Run {
    pub outcome: Outcome,
    pub panes: Vec<PaneSnapshot>,
}

pub fn run_layout(specs: &[PaneSpec], direction: SplitDirection, space: usize) -> Run {
    let mut owned: Vec<Box<dyn Pane>> = specs
        .iter()
        .map(|s| Box::new(MockPane::from_spec(*s)) as Box<dyn Pane>)
        .collect();

    let mut map: HashMap<PaneId, &mut Box<dyn Pane>> = HashMap::new();
    for (s, b) in specs.iter().zip(owned.iter_mut()) {
        map.insert(s.id, b);
    }
    let map = Rc::new(RefCell::new(map));

    let result = PaneResizer::new(map.clone()).layout(direction, space);

    let outcome = match result {
        Ok(()) => Outcome::Ok,
        Err(e) => match e.downcast_ref::<ZellijError>() {
            Some(ZellijError::PaneSizeUnchanged) => Outcome::PaneSizeUnchanged,
            _ => Outcome::OtherErr(format!("{:#}", e)),
        },
    };

    let mut panes: Vec<PaneSnapshot> = map
        .borrow()
        .iter()
        .map(|(id, p)| PaneSnapshot {
            id: *id,
            geom: p.position_and_size(),
            current: p.current_geom(),
            geom_override: p.geom_override(),
        })
        .collect();
    panes.sort_by_key(|s| s.id);

    Run { outcome, panes }
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
        self.geom = position_and_size;
    }
    fn set_geom_override(&mut self, pane_geom: PaneGeom) {
        self.geom_override = Some(pane_geom);
    }
    fn handle_pty_bytes(&mut self, _bytes: VteBytes) {
        unimplemented!()
    }
    fn handle_plugin_bytes(&mut self, _client_id: ClientId, _bytes: VteBytes) {
        unimplemented!()
    }
    fn cursor_coordinates(&self, _client_id: Option<ClientId>) -> Option<(usize, usize, bool)> {
        unimplemented!()
    }
    fn position_and_size(&self) -> PaneGeom {
        self.geom
    }
    fn current_geom(&self) -> PaneGeom {
        self.geom_override.unwrap_or(self.geom)
    }
    fn geom_override(&self) -> Option<PaneGeom> {
        self.geom_override
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
        self.pid
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
    fn invoked_with(&self) -> &Option<LayoutRun> {
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
        _max_scrollback_lines: Option<usize>,
    ) -> PaneContents {
        unimplemented!()
    }
}
