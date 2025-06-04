mod line;
mod tab;
mod action_types;
mod tooltip;
mod keybind_utils;
mod clipboard_utils;

use std::cmp::{max, min};
use std::collections::BTreeMap;
use std::convert::TryInto;

use tab::get_tab_to_focus;
use zellij_tile::prelude::*;

use crate::line::tab_line;
use crate::tab::tab_style;
use crate::tooltip::TooltipRenderer;
use crate::clipboard_utils::{text_copied_hint, system_clipboard_error};

#[derive(Debug, Default)]
pub struct LinePart {
    part: String,
    len: usize,
    tab_index: Option<usize>,
}

#[derive(Default)]
struct State {
    tabs: Vec<TabInfo>,
    active_tab_idx: usize,
    mode_info: ModeInfo,
    tab_line: Vec<LinePart>,
    text_copy_destination: Option<CopyDestination>,
    display_system_clipboard_failure: bool,
    is_tooltip: bool,
    config: BTreeMap<String, String>,
    display_area_rows: usize,
    display_area_cols: usize,
    own_plugin_id: Option<u32>,
}

static ARROW_SEPARATOR: &str = "";

register_plugin!(State);

impl ZellijPlugin for State {
    fn load(&mut self, configuration: BTreeMap<String, String>) {
        self.config = configuration.clone();
        self.is_tooltip = configuration
            .get("is_tooltip")
            .and_then(|v| v.parse().ok())
            .unwrap_or(false);
        
        set_selectable(false);
        subscribe(&[
            EventType::TabUpdate,
            EventType::ModeUpdate,
            EventType::Mouse,
            EventType::CopyToClipboard,
            EventType::InputReceived,
            EventType::SystemClipboardFailure,
        ]);
        self.own_plugin_id = Some(get_plugin_ids().plugin_id);
    }

    fn update(&mut self, event: Event) -> bool {
        let mut should_render = false;
        match event {
            Event::ModeUpdate(mode_info) => {
                should_render = self.handle_mode_update(mode_info);
            },
            Event::TabUpdate(tabs) => {
                should_render = self.handle_tab_update(tabs);
            },
            Event::Mouse(me) => {
                self.handle_mouse_event(me);
            },
            Event::CopyToClipboard(copy_destination) => {
                should_render = self.handle_clipboard_copy(copy_destination);
            },
            Event::SystemClipboardFailure => {
                should_render = self.handle_clipboard_failure();
            },
            Event::InputReceived => {
                should_render = self.handle_input_received();
            },
            _ => {
                eprintln!("Got unrecognized event: {:?}", event);
            },
        };
        should_render
    }

    fn render(&mut self, rows: usize, cols: usize) {
        if self.is_tooltip {
            let tooltip_renderer = TooltipRenderer::new(&self.mode_info);
            tooltip_renderer.render(rows, cols);
        } else {
            self.render_tab_line(cols);
        }
    }
}

impl State {
    fn handle_mode_update(&mut self, mode_info: ModeInfo) -> bool {
        let old_mode = self.mode_info.mode;
        let new_mode = mode_info.mode;
        let base_mode = mode_info.base_mode.unwrap_or(InputMode::Normal);
        
        let should_render = self.mode_info != mode_info;
        self.mode_info = mode_info;
        
        // Handle tooltip logic
        if !self.is_tooltip {
            // If not tooltip and mode changed from base mode to another mode, launch tooltip
            if old_mode == base_mode && new_mode != base_mode {
                self.launch_tooltip(new_mode);
            }
        } else {
            let modes_with_no_tooltip = vec![
                InputMode::Locked,
                InputMode::EnterSearch,
                InputMode::RenameTab,
                InputMode::RenamePane,
                InputMode::Prompt,
                InputMode::Tmux
            ];
            if new_mode == base_mode || modes_with_no_tooltip.contains(&new_mode) {
                close_self();
            } else if new_mode != old_mode {
                self.update_tooltip_for_mode_change(new_mode);
            }
        }
        
        should_render
    }

    fn handle_tab_update(&mut self, tabs: Vec<TabInfo>) -> bool {
        for tab in &tabs {
            if tab.active {
                self.display_area_rows = tab.display_area_rows;
                self.display_area_cols = tab.display_area_columns;
                break;
            }
        }
        
        if let Some(active_tab_index) = tabs.iter().position(|t| t.active) {
            // tabs are indexed starting from 1 so we need to add 1
            let active_tab_idx = active_tab_index + 1;
            let should_render = self.active_tab_idx != active_tab_idx || self.tabs != tabs;
            
            if self.is_tooltip && self.active_tab_idx != active_tab_idx {
                self.move_tooltip_to_new_tab(active_tab_idx);
            }
            
            self.active_tab_idx = active_tab_idx;
            self.tabs = tabs;
            should_render
        } else {
            eprintln!("Could not find active tab.");
            false
        }
    }

    fn handle_mouse_event(&mut self, mouse_event: Mouse) {
        // Only handle mouse events if not tooltip
        if self.is_tooltip {
            return;
        }

        match mouse_event {
            Mouse::LeftClick(_, col) => {
                let tab_to_focus = get_tab_to_focus(&self.tab_line, self.active_tab_idx, col);
                if let Some(idx) = tab_to_focus {
                    switch_tab_to(idx.try_into().unwrap());
                }
            },
            Mouse::ScrollUp(_) => {
                switch_tab_to(min(self.active_tab_idx + 1, self.tabs.len()) as u32);
            },
            Mouse::ScrollDown(_) => {
                switch_tab_to(max(self.active_tab_idx.saturating_sub(1), 1) as u32);
            },
            _ => {},
        }
    }

    fn handle_clipboard_copy(&mut self, copy_destination: CopyDestination) -> bool {
        // Only handle clipboard events if not tooltip
        if self.is_tooltip {
            return false;
        }

        let should_render = match self.text_copy_destination {
            Some(text_copy_destination) => text_copy_destination != copy_destination,
            None => true,
        };
        self.text_copy_destination = Some(copy_destination);
        should_render
    }

    fn handle_clipboard_failure(&mut self) -> bool {
        // Only handle clipboard failure if not tooltip
        if self.is_tooltip {
            return false;
        }
        
        self.display_system_clipboard_failure = true;
        true
    }

    fn handle_input_received(&mut self) -> bool {
        // Only handle input received if not tooltip
        if self.is_tooltip {
            return false;
        }

        let should_render = self.text_copy_destination.is_some() 
            || self.display_system_clipboard_failure;
        
        self.text_copy_destination = None;
        self.display_system_clipboard_failure = false;
        should_render
    }

    fn launch_tooltip(&self, new_mode: InputMode) {
        let mut tooltip_config = self.config.clone();
        tooltip_config.insert("is_tooltip".to_string(), "true".to_string());
        
        pipe_message_to_plugin(
            MessageToPlugin::new("launch_tooltip")
                .with_plugin_url("zellij:OWN_URL")
                .with_plugin_config(tooltip_config)
                .with_floating_pane_coordinates(self.calculate_tooltip_coordinates())
                .new_plugin_instance_should_have_pane_title(format!("{:?} mode keys", new_mode))
        );
    }

    fn update_tooltip_for_mode_change(&self, new_mode: InputMode) {
        if let Some(own_plugin_id) = self.own_plugin_id {
            let tooltip_coordinates = self.calculate_tooltip_coordinates();
            change_floating_panes_coordinates(vec![(
                PaneId::Plugin(own_plugin_id),
                tooltip_coordinates
            )]);
            rename_plugin_pane(own_plugin_id, format!("{:?} keys", new_mode));
        }
    }

    fn move_tooltip_to_new_tab(&self, new_tab_index: usize) {
        if let Some(own_plugin_id) = self.own_plugin_id {
            // Move the tooltip pane to the new active tab
            break_panes_to_tab_with_index(
                &[PaneId::Plugin(own_plugin_id)],
                new_tab_index - 1, // Convert from 1-based to 0-based indexing
                false, // Don't change focus to the new tab since user already switched
            );
        }
    }

    fn calculate_tooltip_coordinates(&self) -> FloatingPaneCoordinates {
        let current_mode = self.mode_info.mode;
        let tooltip_renderer = TooltipRenderer::new(&self.mode_info);
        let (tooltip_rows, tooltip_cols) = tooltip_renderer.calculate_dimensions(current_mode);
        
        let width = tooltip_cols + 4; // 2 for the borders, 2 for padding inside the pane
        let height = tooltip_rows + 2; // 2 for the borders
        let x_position = 2;
        let y_position = self.display_area_rows.saturating_sub(height + 2);
        
        FloatingPaneCoordinates::new(
            Some(format!("{}", x_position)),
            Some(format!("{}", y_position)),
            Some(format!("{}", width)),
            Some(format!("{}", height)),
            Some(true),
        ).unwrap_or_else(Default::default)
    }

    fn render_tab_line(&mut self, cols: usize) {
        if let Some(copy_destination) = self.text_copy_destination {
            self.render_clipboard_hint(copy_destination);
        } else if self.display_system_clipboard_failure {
            self.render_clipboard_error();
        } else {
            self.render_tabs(cols);
        }
    }

    fn render_clipboard_hint(&self, copy_destination: CopyDestination) {
        let hint = text_copied_hint(copy_destination).part;
        self.render_background_with_text(&hint);
    }

    fn render_clipboard_error(&self) {
        let hint = system_clipboard_error().part;
        self.render_background_with_text(&hint);
    }

    fn render_background_with_text(&self, text: &str) {
        let background = self.mode_info.style.colors.text_unselected.background;
        match background {
            PaletteColor::Rgb((r, g, b)) => {
                print!("{}\u{1b}[48;2;{};{};{}m\u{1b}[0K", text, r, g, b);
            },
            PaletteColor::EightBit(color) => {
                print!("{}\u{1b}[48;5;{}m\u{1b}[0K", text, color);
            },
        }
    }

    fn render_tabs(&mut self, cols: usize) {
        if self.tabs.is_empty() {
            return;
        }

        let tab_data = self.prepare_tab_data();
        self.tab_line = tab_line(
            self.mode_info.session_name.as_deref(),
            tab_data.tabs,
            tab_data.active_tab_index,
            cols.saturating_sub(1),
            self.mode_info.style.colors,
            self.mode_info.capabilities,
            self.mode_info.style.hide_session_name,
            self.mode_info.mode,
            &tab_data.active_swap_layout_name,
            tab_data.is_swap_layout_dirty,
        );

        let output = self
            .tab_line
            .iter()
            .fold(String::new(), |output, part| output + &part.part);
        
        self.render_background_with_text(&output);
    }

    fn prepare_tab_data(&mut self) -> TabRenderData {
        let mut all_tabs: Vec<LinePart> = vec![];
        let mut active_tab_index = 0;
        let mut active_swap_layout_name = None;
        let mut is_swap_layout_dirty = false;
        let mut is_alternate_tab = false;

        for t in &self.tabs {
            let tabname = self.get_tab_display_name(t);
            
            if t.active {
                active_tab_index = t.position;
                if self.mode_info.mode != InputMode::RenameTab {
                    is_swap_layout_dirty = t.is_swap_layout_dirty;
                    active_swap_layout_name = t.active_swap_layout_name.clone();
                }
            }

            let tab = tab_style(
                tabname,
                t,
                is_alternate_tab,
                self.mode_info.style.colors,
                self.mode_info.capabilities,
            );
            is_alternate_tab = !is_alternate_tab;
            all_tabs.push(tab);
        }

        TabRenderData {
            tabs: all_tabs,
            active_tab_index,
            active_swap_layout_name,
            is_swap_layout_dirty,
        }
    }

    fn get_tab_display_name(&self, tab: &TabInfo) -> String {
        let mut tabname = tab.name.clone();
        if tab.active && self.mode_info.mode == InputMode::RenameTab && tabname.is_empty() {
            tabname = String::from("Enter name...");
        }
        tabname
    }
}

struct TabRenderData {
    tabs: Vec<LinePart>,
    active_tab_index: usize,
    active_swap_layout_name: Option<String>,
    is_swap_layout_dirty: bool,
}
