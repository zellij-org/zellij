mod line;
mod tab;

use std::cmp::{max, min};
use std::convert::TryInto;

use zellij_tile::prelude::*;

use crate::line::tab_line;
use crate::tab::tab_style;

#[derive(Debug, Default)]
pub struct LinePart {
    part: String,
    len: usize,
}

#[derive(Default)]
struct State {
    tabs: Vec<TabInfo>,
    active_tab_idx: usize,
    mode_info: ModeInfo,
    mouse_click_pos: usize,
    should_render: bool,
}

static ARROW_SEPARATOR: &str = "";

register_plugin!(State);

impl ZellijPlugin for State {
    fn load(&mut self) {
        set_selectable(false);
        subscribe(&[
            EventType::TabUpdate,
            EventType::ModeUpdate,
            EventType::Mouse,
        ]);
    }

    fn update(&mut self, event: Event) {
        match event {
            Event::ModeUpdate(mode_info) => self.mode_info = mode_info,
            Event::TabUpdate(tabs) => {
                // tabs are indexed starting from 1 so we need to add 1
                self.active_tab_idx = tabs.iter().position(|t| t.active).unwrap() + 1;
                self.tabs = tabs;
            }
            Event::Mouse(me) => match me {
                Mouse::LeftClick(_, col) => {
                    self.mouse_click_pos = col;
                    self.should_render = true;
                }
                Mouse::ScrollUp(_) => {
                    switch_tab_to(min(self.active_tab_idx + 1, self.tabs.len()) as u32);
                }
                Mouse::ScrollDown(_) => {
                    switch_tab_to(max(self.active_tab_idx.saturating_sub(1), 1) as u32);
                }
                _ => {}
            },
            _ => unimplemented!(), // FIXME: This should be unreachable, but this could be cleaner
        }
    }

    fn render(&mut self, _rows: usize, cols: usize) {
        if self.tabs.is_empty() {
            return;
        }
        let mut all_tabs: Vec<LinePart> = vec![];
        let mut active_tab_index = 0;
        for t in &mut self.tabs {
            let mut tabname = t.name.clone();
            if t.active && self.mode_info.mode == InputMode::RenameTab {
                if tabname.is_empty() {
                    tabname = String::from("Enter name...");
                }
                active_tab_index = t.position;
            } else if t.active {
                active_tab_index = t.position;
            }
            let tab = tab_style(
                tabname,
                t.active,
                t.is_sync_panes_active,
                self.mode_info.palette,
                self.mode_info.capabilities,
                t.other_focused_clients.as_slice(),
            );
            all_tabs.push(tab);
        }
        let tab_line = tab_line(
            self.mode_info.session_name.as_deref(),
            all_tabs,
            active_tab_index,
            cols.saturating_sub(1),
            self.mode_info.palette,
            self.mode_info.capabilities,
        );
        let mut s = String::new();
        let mut len_cnt = 0;
        for (idx, bar_part) in tab_line.iter().enumerate() {
            s = format!("{}{}", s, &bar_part.part);

            if self.should_render
                && self.mouse_click_pos > len_cnt
                && self.mouse_click_pos <= len_cnt + bar_part.len
                && idx > 2
            {
                // First three elements of tab_line are "Zellij", session name and empty thing, hence the idx > 2 condition.
                // Tabs are indexed starting from 1, therefore we need subtract 2 below.
                switch_tab_to(TryInto::<u32>::try_into(idx).unwrap() - 2);
            }
            len_cnt += bar_part.len;
        }
        match self.mode_info.palette.gray {
            PaletteColor::Rgb((r, g, b)) => {
                println!("{}\u{1b}[48;2;{};{};{}m\u{1b}[0K", s, r, g, b);
            }
            PaletteColor::EightBit(color) => {
                println!("{}\u{1b}[48;5;{}m\u{1b}[0K", s, color);
            }
        }
        self.should_render = false;
    }
}
