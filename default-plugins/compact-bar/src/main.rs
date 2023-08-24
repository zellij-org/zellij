mod line;
mod tab;

use std::cmp::{max, min};
use std::collections::BTreeMap;
use std::convert::TryInto;

use tab::get_tab_to_focus;
use zellij_tile::prelude::*;

use crate::line::tab_line;
use crate::tab::tab_style;

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
}

static ARROW_SEPARATOR: &str = "";

register_plugin!(State);

impl ZellijPlugin for State {
    fn load(&mut self, _configuration: BTreeMap<String, String>) {
        set_selectable(false);
        subscribe(&[
            EventType::TabUpdate,
            EventType::ModeUpdate,
            EventType::Mouse,
        ]);
    }

    fn update(&mut self, event: Event) -> bool {
        let mut should_render = false;
        match event {
            Event::ModeUpdate(mode_info) => {
                if self.mode_info != mode_info {
                    should_render = true;
                }
                self.mode_info = mode_info
            },
            Event::TabUpdate(tabs) => {
                if let Some(active_tab_index) = tabs.iter().position(|t| t.active) {
                    // tabs are indexed starting from 1 so we need to add 1
                    let active_tab_idx = active_tab_index + 1;
                    if self.active_tab_idx != active_tab_idx || self.tabs != tabs {
                        should_render = true;
                    }
                    self.active_tab_idx = active_tab_idx;
                    self.tabs = tabs;
                } else {
                    eprintln!("Could not find active tab.");
                }
            },
            Event::Mouse(me) => match me {
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
            },
            _ => {
                eprintln!("Got unrecognized event: {:?}", event);
            },
        };
        should_render
    }

    fn render(&mut self, _rows: usize, cols: usize) {
        if self.tabs.is_empty() {
            return;
        }
        let mut all_tabs: Vec<LinePart> = vec![];
        let mut active_tab_index = 0;
        let mut active_swap_layout_name = None;
        let mut is_swap_layout_dirty = false;
        let mut is_alternate_tab = false;
        for t in &mut self.tabs {
            let mut tabname = t.name.clone();
            if t.active && self.mode_info.mode == InputMode::RenameTab {
                if tabname.is_empty() {
                    tabname = String::from("Enter name...");
                }
                active_tab_index = t.position;
            } else if t.active {
                active_tab_index = t.position;
                is_swap_layout_dirty = t.is_swap_layout_dirty;
                active_swap_layout_name = t.active_swap_layout_name.clone();
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
        self.tab_line = tab_line(
            self.mode_info.session_name.as_deref(),
            all_tabs,
            active_tab_index,
            cols.saturating_sub(1),
            self.mode_info.style.colors,
            self.mode_info.capabilities,
            self.mode_info.style.hide_session_name,
            self.mode_info.mode,
            &active_swap_layout_name,
            is_swap_layout_dirty,
        );
        let output = self
            .tab_line
            .iter()
            .fold(String::new(), |output, part| output + &part.part);
        let background = match self.mode_info.style.colors.theme_hue {
            ThemeHue::Dark => self.mode_info.style.colors.black,
            ThemeHue::Light => self.mode_info.style.colors.white,
        };
        match background {
            PaletteColor::Rgb((r, g, b)) => {
                print!("{}\u{1b}[48;2;{};{};{}m\u{1b}[0K", output, r, g, b);
            },
            PaletteColor::EightBit(color) => {
                print!("{}\u{1b}[48;5;{}m\u{1b}[0K", output, color);
            },
        }
    }
}
