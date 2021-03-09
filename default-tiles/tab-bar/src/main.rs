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

#[derive(PartialEq)]
enum BarMode {
    Normal,
    Rename,
}

impl Default for BarMode {
    fn default() -> Self {
        BarMode::Normal
    }
}

#[derive(Default)]
struct State {
    active_tab_index: usize,
    num_tabs: usize,
    tabs: Vec<TabData>,
    mode: BarMode,
    new_name: String,
}

static ARROW_SEPARATOR: &str = "";

register_tile!(State);

impl ZellijTile for State {
    fn load(&mut self) {
        set_selectable(false);
        set_invisible_borders(true);
        set_max_height(1);
        self.active_tab_index = 0;
        self.num_tabs = 0;
        self.mode = BarMode::Normal;
        self.new_name = String::new();
    }

    fn draw(&mut self, _rows: usize, cols: usize) {
        if self.tabs.is_empty() {
            return;
        }
        let mut all_tabs: Vec<LinePart> = vec![];
        let mut active_tab_index = 0;
        for t in self.tabs.iter_mut() {
            let mut tabname = t.name.clone();
            if t.active && self.mode == BarMode::Rename {
                if self.new_name.is_empty() {
                    tabname = String::from("Enter name...");
                } else {
                    tabname = self.new_name.clone();
                }
                active_tab_index = t.position;
            } else if t.active {
                active_tab_index = t.position;
            }
            let tab = tab_style(tabname, t.active, t.position);
            all_tabs.push(tab);
        }
        let tab_line = tab_line(all_tabs, active_tab_index, cols);
        let mut s = String::new();
        for bar_part in tab_line {
            s = format!("{}{}", s, bar_part.part);
        }
        println!("{}\u{1b}[40m\u{1b}[0K", s);
    }

    fn update_tabs(&mut self) {
        self.tabs = get_tabs();
    }

    fn handle_tab_rename_keypress(&mut self, key: Key) {
        self.mode = BarMode::Rename;
        match key {
            Key::Char('\n') | Key::Esc => {
                self.mode = BarMode::Normal;
                self.new_name.clear();
            }
            Key::Char(c) => self.new_name = format!("{}{}", self.new_name, c),
            _ => {}
        }
    }
}
