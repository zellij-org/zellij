use crate::output::{CharacterChunk, KittyImageChunk, SixelImageChunk};
use crate::panes::PaneId;
use crate::tab::Pane;
use crate::ui::pane_boundaries_frame::FrameParams;
use crate::ClientId;
use std::time::Instant;
use zellij_utils::data::{InputMode, PaletteColor, PaneContents};
use zellij_utils::errors::prelude::*;
use zellij_utils::input::layout::Run;
use zellij_utils::pane_size::{Offset, PaneGeom};

pub struct FailingRenderPane {
    pub inner: Box<dyn Pane>,
}

impl Pane for FailingRenderPane {
    fn x(&self) -> usize {
        self.inner.x()
    }
    fn y(&self) -> usize {
        self.inner.y()
    }
    fn rows(&self) -> usize {
        self.inner.rows()
    }
    fn cols(&self) -> usize {
        self.inner.cols()
    }
    fn get_content_x(&self) -> usize {
        self.inner.get_content_x()
    }
    fn get_content_y(&self) -> usize {
        self.inner.get_content_y()
    }
    fn get_content_columns(&self) -> usize {
        self.inner.get_content_columns()
    }
    fn get_content_rows(&self) -> usize {
        self.inner.get_content_rows()
    }
    fn reset_size_and_position_override(&mut self) {
        self.inner.reset_size_and_position_override()
    }
    fn set_geom(&mut self, position_and_size: PaneGeom) {
        self.inner.set_geom(position_and_size)
    }
    fn set_geom_override(&mut self, pane_geom: PaneGeom) {
        self.inner.set_geom_override(pane_geom)
    }
    fn cursor_coordinates(&self, _client_id: Option<ClientId>) -> Option<(usize, usize, bool)> {
        self.inner.cursor_coordinates(_client_id)
    }
    fn position_and_size(&self) -> PaneGeom {
        self.inner.position_and_size()
    }
    fn current_geom(&self) -> PaneGeom {
        self.inner.current_geom()
    }
    fn geom_override(&self) -> Option<PaneGeom> {
        self.inner.geom_override()
    }
    fn should_render(&self) -> bool {
        self.inner.should_render()
    }
    fn set_should_render(&mut self, should_render: bool) {
        self.inner.set_should_render(should_render)
    }
    fn selectable(&self) -> bool {
        self.inner.selectable()
    }
    fn set_selectable(&mut self, selectable: bool) {
        self.inner.set_selectable(selectable)
    }
    fn render(
        &mut self,
        client_id: Option<ClientId>,
    ) -> Result<
        Option<(
            Vec<CharacterChunk>,
            Option<String>,
            Vec<SixelImageChunk>,
            Vec<KittyImageChunk>,
        )>,
    > {
        self.inner.render(client_id)
    }
    fn render_frame(
        &mut self,
        _client_id: ClientId,
        _frame_params: FrameParams,
        _input_mode: InputMode,
    ) -> Result<Option<(Vec<CharacterChunk>, Option<String>)>> {
        Err(anyhow!("failing render"))
    }
    fn render_fake_cursor(
        &mut self,
        cursor_color: PaletteColor,
        text_color: PaletteColor,
    ) -> Option<String> {
        self.inner.render_fake_cursor(cursor_color, text_color)
    }
    fn render_terminal_title(&mut self, _input_mode: InputMode) -> String {
        self.inner.render_terminal_title(_input_mode)
    }
    fn update_name(&mut self, name: &str) {
        self.inner.update_name(name)
    }
    fn pid(&self) -> PaneId {
        self.inner.pid()
    }
    fn reduce_height(&mut self, percent: f64) {
        self.inner.reduce_height(percent)
    }
    fn increase_height(&mut self, percent: f64) {
        self.inner.increase_height(percent)
    }
    fn reduce_width(&mut self, percent: f64) {
        self.inner.reduce_width(percent)
    }
    fn increase_width(&mut self, percent: f64) {
        self.inner.increase_width(percent)
    }
    fn push_down(&mut self, count: usize) {
        self.inner.push_down(count)
    }
    fn push_right(&mut self, count: usize) {
        self.inner.push_right(count)
    }
    fn pull_left(&mut self, count: usize) {
        self.inner.pull_left(count)
    }
    fn pull_up(&mut self, count: usize) {
        self.inner.pull_up(count)
    }
    fn clear_screen(&mut self) {
        self.inner.clear_screen()
    }
    fn scroll_up(&mut self, count: usize, client_id: ClientId) {
        self.inner.scroll_up(count, client_id)
    }
    fn scroll_down(&mut self, count: usize, client_id: ClientId) {
        self.inner.scroll_down(count, client_id)
    }
    fn scroll_left(&mut self, count: usize, client_id: ClientId) {
        self.inner.scroll_left(count, client_id)
    }
    fn scroll_right(&mut self, count: usize, client_id: ClientId) {
        self.inner.scroll_right(count, client_id)
    }
    fn clear_scroll(&mut self) {
        self.inner.clear_scroll()
    }
    fn is_scrolled(&self) -> bool {
        self.inner.is_scrolled()
    }
    fn active_at(&self) -> Instant {
        self.inner.active_at()
    }
    fn set_active_at(&mut self, instant: Instant) {
        self.inner.set_active_at(instant)
    }
    fn set_frame(&mut self, frame: bool) {
        self.inner.set_frame(frame)
    }
    fn set_content_offset(&mut self, offset: Offset) {
        self.inner.set_content_offset(offset)
    }
    fn store_pane_name(&mut self) {
        self.inner.store_pane_name()
    }
    fn load_pane_name(&mut self) {
        self.inner.load_pane_name()
    }
    fn set_borderless(&mut self, borderless: bool) {
        self.inner.set_borderless(borderless)
    }
    fn borderless(&self) -> bool {
        self.inner.borderless()
    }
    fn set_exclude_from_sync(&mut self, exclude_from_sync: bool) {
        self.inner.set_exclude_from_sync(exclude_from_sync)
    }
    fn exclude_from_sync(&self) -> bool {
        self.inner.exclude_from_sync()
    }
    fn add_red_pane_frame_color_override(&mut self, _error_text: Option<String>) {
        self.inner.add_red_pane_frame_color_override(_error_text)
    }
    fn clear_pane_frame_color_override(&mut self, _client_id: Option<ClientId>) {
        self.inner.clear_pane_frame_color_override(_client_id)
    }
    fn frame_color_override(&self) -> Option<PaletteColor> {
        self.inner.frame_color_override()
    }
    fn invoked_with(&self) -> &Option<Run> {
        self.inner.invoked_with()
    }
    fn set_title(&mut self, title: String) {
        self.inner.set_title(title)
    }
    fn current_title(&self) -> String {
        self.inner.current_title()
    }
    fn custom_title(&self) -> Option<String> {
        self.inner.custom_title()
    }
    fn pane_contents(
        &self,
        client_id: Option<ClientId>,
        _get_full_scrollback: bool,
        _max_scrollback_lines: Option<usize>,
    ) -> PaneContents {
        self.inner
            .pane_contents(client_id, _get_full_scrollback, _max_scrollback_lines)
    }
}

pub struct FocusReportingPane {
    pub inner: Box<dyn Pane>,
}

impl Pane for FocusReportingPane {
    fn x(&self) -> usize {
        self.inner.x()
    }
    fn y(&self) -> usize {
        self.inner.y()
    }
    fn rows(&self) -> usize {
        self.inner.rows()
    }
    fn cols(&self) -> usize {
        self.inner.cols()
    }
    fn get_content_x(&self) -> usize {
        self.inner.get_content_x()
    }
    fn get_content_y(&self) -> usize {
        self.inner.get_content_y()
    }
    fn get_content_columns(&self) -> usize {
        self.inner.get_content_columns()
    }
    fn get_content_rows(&self) -> usize {
        self.inner.get_content_rows()
    }
    fn reset_size_and_position_override(&mut self) {
        self.inner.reset_size_and_position_override()
    }
    fn set_geom(&mut self, position_and_size: PaneGeom) {
        self.inner.set_geom(position_and_size)
    }
    fn set_geom_override(&mut self, pane_geom: PaneGeom) {
        self.inner.set_geom_override(pane_geom)
    }
    fn cursor_coordinates(&self, _client_id: Option<ClientId>) -> Option<(usize, usize, bool)> {
        self.inner.cursor_coordinates(_client_id)
    }
    fn position_and_size(&self) -> PaneGeom {
        self.inner.position_and_size()
    }
    fn current_geom(&self) -> PaneGeom {
        self.inner.current_geom()
    }
    fn geom_override(&self) -> Option<PaneGeom> {
        self.inner.geom_override()
    }
    fn should_render(&self) -> bool {
        self.inner.should_render()
    }
    fn set_should_render(&mut self, should_render: bool) {
        self.inner.set_should_render(should_render)
    }
    fn selectable(&self) -> bool {
        self.inner.selectable()
    }
    fn set_selectable(&mut self, selectable: bool) {
        self.inner.set_selectable(selectable)
    }
    fn render(
        &mut self,
        client_id: Option<ClientId>,
    ) -> Result<
        Option<(
            Vec<CharacterChunk>,
            Option<String>,
            Vec<SixelImageChunk>,
            Vec<KittyImageChunk>,
        )>,
    > {
        self.inner.render(client_id)
    }
    fn render_frame(
        &mut self,
        client_id: ClientId,
        frame_params: FrameParams,
        input_mode: InputMode,
    ) -> Result<Option<(Vec<CharacterChunk>, Option<String>)>> {
        self.inner.render_frame(client_id, frame_params, input_mode)
    }
    fn render_fake_cursor(
        &mut self,
        cursor_color: PaletteColor,
        text_color: PaletteColor,
    ) -> Option<String> {
        self.inner.render_fake_cursor(cursor_color, text_color)
    }
    fn render_terminal_title(&mut self, _input_mode: InputMode) -> String {
        self.inner.render_terminal_title(_input_mode)
    }
    fn update_name(&mut self, name: &str) {
        self.inner.update_name(name)
    }
    fn pid(&self) -> PaneId {
        self.inner.pid()
    }
    fn reduce_height(&mut self, percent: f64) {
        self.inner.reduce_height(percent)
    }
    fn increase_height(&mut self, percent: f64) {
        self.inner.increase_height(percent)
    }
    fn reduce_width(&mut self, percent: f64) {
        self.inner.reduce_width(percent)
    }
    fn increase_width(&mut self, percent: f64) {
        self.inner.increase_width(percent)
    }
    fn push_down(&mut self, count: usize) {
        self.inner.push_down(count)
    }
    fn push_right(&mut self, count: usize) {
        self.inner.push_right(count)
    }
    fn pull_left(&mut self, count: usize) {
        self.inner.pull_left(count)
    }
    fn pull_up(&mut self, count: usize) {
        self.inner.pull_up(count)
    }
    fn clear_screen(&mut self) {
        self.inner.clear_screen()
    }
    fn scroll_up(&mut self, count: usize, client_id: ClientId) {
        self.inner.scroll_up(count, client_id)
    }
    fn scroll_down(&mut self, count: usize, client_id: ClientId) {
        self.inner.scroll_down(count, client_id)
    }
    fn scroll_left(&mut self, count: usize, client_id: ClientId) {
        self.inner.scroll_left(count, client_id)
    }
    fn scroll_right(&mut self, count: usize, client_id: ClientId) {
        self.inner.scroll_right(count, client_id)
    }
    fn clear_scroll(&mut self) {
        self.inner.clear_scroll()
    }
    fn is_scrolled(&self) -> bool {
        self.inner.is_scrolled()
    }
    fn active_at(&self) -> Instant {
        self.inner.active_at()
    }
    fn set_active_at(&mut self, instant: Instant) {
        self.inner.set_active_at(instant)
    }
    fn set_frame(&mut self, frame: bool) {
        self.inner.set_frame(frame)
    }
    fn set_content_offset(&mut self, offset: Offset) {
        self.inner.set_content_offset(offset)
    }
    fn store_pane_name(&mut self) {
        self.inner.store_pane_name()
    }
    fn load_pane_name(&mut self) {
        self.inner.load_pane_name()
    }
    fn set_borderless(&mut self, borderless: bool) {
        self.inner.set_borderless(borderless)
    }
    fn borderless(&self) -> bool {
        self.inner.borderless()
    }
    fn set_exclude_from_sync(&mut self, exclude_from_sync: bool) {
        self.inner.set_exclude_from_sync(exclude_from_sync)
    }
    fn exclude_from_sync(&self) -> bool {
        self.inner.exclude_from_sync()
    }
    fn add_red_pane_frame_color_override(&mut self, _error_text: Option<String>) {
        self.inner.add_red_pane_frame_color_override(_error_text)
    }
    fn clear_pane_frame_color_override(&mut self, _client_id: Option<ClientId>) {
        self.inner.clear_pane_frame_color_override(_client_id)
    }
    fn frame_color_override(&self) -> Option<PaletteColor> {
        self.inner.frame_color_override()
    }
    fn invoked_with(&self) -> &Option<Run> {
        self.inner.invoked_with()
    }
    fn set_title(&mut self, title: String) {
        self.inner.set_title(title)
    }
    fn current_title(&self) -> String {
        self.inner.current_title()
    }
    fn custom_title(&self) -> Option<String> {
        self.inner.custom_title()
    }
    fn pane_contents(
        &self,
        client_id: Option<ClientId>,
        _get_full_scrollback: bool,
        _max_scrollback_lines: Option<usize>,
    ) -> PaneContents {
        self.inner
            .pane_contents(client_id, _get_full_scrollback, _max_scrollback_lines)
    }
    fn focus_event(&self) -> Option<String> {
        Some("focus-in".to_owned())
    }
    fn unfocus_event(&self) -> Option<String> {
        Some("focus-out".to_owned())
    }
}
