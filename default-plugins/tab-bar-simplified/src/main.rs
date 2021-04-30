mod line;
mod tab;

use zellij_tile::prelude::*;

use crate::line::tab_line;
use crate::tab::tab_style;

#[derive(Debug)]
pub struct LinePart {
    part: String,
    len: usize,
}

#[derive(Default)]
struct State {
    tabs: Vec<TabInfo>,
    mode: InputMode,
}

static ARROW_SEPARATOR: &str = "|";

pub mod colors {
    use ansi_term::Colour::{self, Fixed};
    pub const WHITE: Colour = Fixed(255);
    pub const BLACK: Colour = Fixed(16);
    pub const GREEN: Colour = Fixed(154);
    pub const ORANGE: Colour = Fixed(166);
    pub const GRAY: Colour = Fixed(238);
    pub const BRIGHT_GRAY: Colour = Fixed(245);
    pub const RED: Colour = Fixed(88);
}

register_plugin!(State);

impl ZellijPlugin for State {
    fn load(&mut self) {
        set_selectable(false);
        set_invisible_borders(true);
        set_max_height(1);
        subscribe(&[EventType::TabUpdate, EventType::ModeUpdate]);
    }

    fn update(&mut self, event: Event) {
        match event {
            Event::ModeUpdate(mode_info) => self.mode = mode_info.mode,
            Event::TabUpdate(tabs) => self.tabs = tabs,
            _ => unimplemented!(), // FIXME: This should be unreachable, but this could be cleaner
        }
    }

    fn render(&mut self, _rows: usize, cols: usize) {
        if self.tabs.is_empty() {
            return;
        }
        let mut all_tabs: Vec<LinePart> = vec![];
        let mut active_tab_index = 0;
        for t in self.tabs.iter_mut() {
            let mut tabname = t.name.clone();
            if t.active && self.mode == InputMode::RenameTab {
                if tabname.is_empty() {
                    tabname = String::from("Enter name...");
                }
                active_tab_index = t.position;
            } else if t.active {
                active_tab_index = t.position;
            }
            let tab = tab_style(tabname, t.active, t.position, t.is_sync_panes_active);
            all_tabs.push(tab);
        }
        let tab_line = tab_line(all_tabs, active_tab_index, cols);
        let mut s = String::new();
        for bar_part in tab_line {
            s = format!("{}{}", s, bar_part.part);
        }
        println!("{}\u{1b}[48;5;238m\u{1b}[0K", s);
    }
}
