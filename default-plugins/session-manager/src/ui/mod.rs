pub mod components;
pub mod welcome_screen;
use zellij_tile::prelude::*;

use crate::session_list::{SelectedIndex, SessionList};
use components::{
    build_pane_ui_line, build_session_ui_line, build_tab_ui_line, minimize_lines, Colors,
    LineToRender,
};

macro_rules! render_assets {
    ($assets:expr, $line_count_to_remove:expr, $selected_index:expr, $to_render_until_selected: expr, $to_render_after_selected:expr, $has_deeper_selected_assets:expr, $max_cols:expr, $colors:expr) => {{
        let (start_index, anchor_asset_index, end_index, line_count_to_remove) =
            minimize_lines($assets.len(), $line_count_to_remove, $selected_index);
        let mut truncated_result_count_above = start_index;
        let mut truncated_result_count_below = $assets.len().saturating_sub(end_index);
        let mut current_index = 1;
        if let Some(assets_to_render_before_selected) = $assets.get(start_index..anchor_asset_index)
        {
            for asset in assets_to_render_before_selected {
                let mut asset: LineToRender =
                    asset.as_line_to_render(current_index, $max_cols, $colors);
                asset.add_truncated_results(truncated_result_count_above);
                truncated_result_count_above = 0;
                current_index += 1;
                $to_render_until_selected.push(asset);
            }
        }
        if let Some(selected_asset) = $assets.get(anchor_asset_index) {
            if $selected_index.is_some() && !$has_deeper_selected_assets {
                let mut selected_asset: LineToRender =
                    selected_asset.as_line_to_render(current_index, $max_cols, $colors);
                selected_asset.make_selected(true);
                selected_asset.add_truncated_results(truncated_result_count_above);
                if anchor_asset_index + 1 >= end_index {
                    // no more results below, let's add the more indication if we need to
                    selected_asset.add_truncated_results(truncated_result_count_below);
                }
                current_index += 1;
                $to_render_until_selected.push(selected_asset);
            } else {
                $to_render_until_selected.push(selected_asset.as_line_to_render(
                    current_index,
                    $max_cols,
                    $colors,
                ));
                current_index += 1;
            }
        }
        if let Some(assets_to_render_after_selected) =
            $assets.get(anchor_asset_index + 1..end_index)
        {
            for asset in assets_to_render_after_selected.iter().rev() {
                let mut asset: LineToRender =
                    asset.as_line_to_render(current_index, $max_cols, $colors);
                asset.add_truncated_results(truncated_result_count_below);
                truncated_result_count_below = 0;
                current_index += 1;
                $to_render_after_selected.insert(0, asset.into());
            }
        }
        line_count_to_remove
    }};
}

impl SessionList {
    pub fn render(&self, max_rows: usize, max_cols: usize, colors: Colors) -> Vec<LineToRender> {
        if self.is_searching {
            self.render_search_results(max_rows, max_cols)
        } else {
            self.render_list(max_rows, max_cols, colors)
        }
    }
    fn render_search_results(&self, max_rows: usize, max_cols: usize) -> Vec<LineToRender> {
        let mut lines_to_render = vec![];
        for (i, result) in self.search_results.iter().enumerate() {
            if lines_to_render.len() + result.lines_to_render() <= max_rows {
                let mut result_lines = result.render(max_cols);
                if Some(i) == self.selected_search_index {
                    let mut render_arrows = true;
                    for line_to_render in result_lines.iter_mut() {
                        line_to_render.make_selected_as_search(render_arrows);
                        render_arrows = false; // only render arrows on the first search result
                    }
                }
                lines_to_render.append(&mut result_lines);
            } else {
                break;
            }
        }
        lines_to_render
    }
    fn render_list(&self, max_rows: usize, max_cols: usize, colors: Colors) -> Vec<LineToRender> {
        let mut lines_to_render_until_selected = vec![];
        let mut lines_to_render_after_selected = vec![];
        let total_lines_to_render = self.total_lines_to_render();
        let line_count_to_remove = total_lines_to_render.saturating_sub(max_rows);
        let line_count_to_remove = self.render_sessions(
            &mut lines_to_render_until_selected,
            &mut lines_to_render_after_selected,
            line_count_to_remove,
            max_cols,
            colors,
        );
        let line_count_to_remove = self.render_tabs(
            &mut lines_to_render_until_selected,
            &mut lines_to_render_after_selected,
            line_count_to_remove,
            max_cols,
            colors,
        );
        self.render_panes(
            &mut lines_to_render_until_selected,
            &mut lines_to_render_after_selected,
            line_count_to_remove,
            max_cols,
            colors,
        );
        let mut lines_to_render = lines_to_render_until_selected;
        lines_to_render.append(&mut lines_to_render_after_selected);
        lines_to_render
    }
    fn render_sessions(
        &self,
        to_render_until_selected: &mut Vec<LineToRender>,
        to_render_after_selected: &mut Vec<LineToRender>,
        line_count_to_remove: usize,
        max_cols: usize,
        colors: Colors,
    ) -> usize {
        render_assets!(
            self.session_ui_infos,
            line_count_to_remove,
            self.selected_index.0,
            to_render_until_selected,
            to_render_after_selected,
            self.selected_index.1.is_some(),
            max_cols,
            colors
        )
    }
    fn render_tabs(
        &self,
        to_render_until_selected: &mut Vec<LineToRender>,
        to_render_after_selected: &mut Vec<LineToRender>,
        line_count_to_remove: usize,
        max_cols: usize,
        colors: Colors,
    ) -> usize {
        if self.selected_index.1.is_none() {
            return line_count_to_remove;
        }
        if let Some(tabs_in_session) = self
            .selected_index
            .0
            .and_then(|i| self.session_ui_infos.get(i))
            .map(|s| &s.tabs)
        {
            render_assets!(
                tabs_in_session,
                line_count_to_remove,
                self.selected_index.1,
                to_render_until_selected,
                to_render_after_selected,
                self.selected_index.2.is_some(),
                max_cols,
                colors
            )
        } else {
            line_count_to_remove
        }
    }
    fn render_panes(
        &self,
        to_render_until_selected: &mut Vec<LineToRender>,
        to_render_after_selected: &mut Vec<LineToRender>,
        line_count_to_remove: usize,
        max_cols: usize,
        colors: Colors,
    ) -> usize {
        if self.selected_index.2.is_none() {
            return line_count_to_remove;
        }
        if let Some(panes_in_session) = self
            .selected_index
            .0
            .and_then(|i| self.session_ui_infos.get(i))
            .map(|s| &s.tabs)
            .and_then(|tabs| {
                self.selected_index
                    .1
                    .and_then(|i| tabs.get(i))
                    .map(|t| &t.panes)
            })
        {
            render_assets!(
                panes_in_session,
                line_count_to_remove,
                self.selected_index.2,
                to_render_until_selected,
                to_render_after_selected,
                false,
                max_cols,
                colors
            )
        } else {
            line_count_to_remove
        }
    }
    fn total_lines_to_render(&self) -> usize {
        self.session_ui_infos
            .iter()
            .enumerate()
            .fold(0, |acc, (index, s)| {
                if self.selected_index.session_index_is_selected(index) {
                    acc + s.line_count(&self.selected_index)
                } else {
                    acc + 1
                }
            })
    }
}

#[derive(Debug, Clone)]
pub struct SessionUiInfo {
    pub name: String,
    pub tabs: Vec<TabUiInfo>,
    pub connected_users: usize,
    pub is_current_session: bool,
}

impl SessionUiInfo {
    pub fn from_session_info(session_info: &SessionInfo) -> Self {
        SessionUiInfo {
            name: session_info.name.clone(),
            tabs: session_info
                .tabs
                .iter()
                .map(|t| TabUiInfo::new(t, &session_info.panes))
                .collect(),
            connected_users: session_info.connected_clients,
            is_current_session: session_info.is_current_session,
        }
    }
    pub fn line_count(&self, selected_index: &SelectedIndex) -> usize {
        let mut line_count = 1; // self
        if selected_index.tabs_are_visible() {
            match selected_index
                .selected_tab_index()
                .and_then(|i| self.tabs.get(i))
                .map(|t| t.line_count(&selected_index))
            {
                Some(line_count_of_selected_tab) => {
                    // we add the line count in the selected tab minus 1 because we will account
                    // for the selected tab line itself in self.tabs.len() below
                    line_count += line_count_of_selected_tab.saturating_sub(1);
                    line_count += self.tabs.len();
                },
                None => {
                    line_count += self.tabs.len();
                },
            }
        }
        line_count
    }
    fn as_line_to_render(
        &self,
        _session_index: u8,
        mut max_cols: usize,
        colors: Colors,
    ) -> LineToRender {
        let mut line_to_render = LineToRender::new(colors);
        let ui_spans = build_session_ui_line(&self, colors);
        for span in ui_spans {
            span.render(None, &mut line_to_render, &mut max_cols);
        }
        line_to_render
    }
}

#[derive(Debug, Clone)]
pub struct TabUiInfo {
    pub name: String,
    pub panes: Vec<PaneUiInfo>,
    pub position: usize,
}

impl TabUiInfo {
    pub fn new(tab_info: &TabInfo, pane_manifest: &PaneManifest) -> Self {
        let panes = pane_manifest
            .panes
            .get(&tab_info.position)
            .map(|p| {
                p.iter()
                    .filter_map(|pane_info| {
                        if pane_info.is_selectable {
                            Some(PaneUiInfo {
                                name: pane_info.title.clone(),
                                exit_code: pane_info.exit_status.clone(),
                                pane_id: pane_info.id,
                                is_plugin: pane_info.is_plugin,
                            })
                        } else {
                            None
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();
        TabUiInfo {
            name: tab_info.name.clone(),
            panes,
            position: tab_info.position,
        }
    }
    pub fn line_count(&self, selected_index: &SelectedIndex) -> usize {
        let mut line_count = 1; // self
        if selected_index.panes_are_visible() {
            line_count += self.panes.len()
        }
        line_count
    }
    fn as_line_to_render(
        &self,
        _session_index: u8,
        mut max_cols: usize,
        colors: Colors,
    ) -> LineToRender {
        let mut line_to_render = LineToRender::new(colors);
        let ui_spans = build_tab_ui_line(&self, colors);
        for span in ui_spans {
            span.render(None, &mut line_to_render, &mut max_cols);
        }
        line_to_render
    }
}

#[derive(Debug, Clone)]
pub struct PaneUiInfo {
    pub name: String,
    pub exit_code: Option<i32>,
    pub pane_id: u32,
    pub is_plugin: bool,
}

impl PaneUiInfo {
    fn as_line_to_render(
        &self,
        _session_index: u8,
        mut max_cols: usize,
        colors: Colors,
    ) -> LineToRender {
        let mut line_to_render = LineToRender::new(colors);
        let ui_spans = build_pane_ui_line(&self, colors);
        for span in ui_spans {
            span.render(None, &mut line_to_render, &mut max_cols);
        }
        line_to_render
    }
}
