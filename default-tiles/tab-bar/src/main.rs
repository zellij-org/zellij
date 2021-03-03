mod tab;
mod line;

use zellij_tile::*;

use crate::tab::nameless_tab;
use crate::line::tab_line;

#[derive(Debug)]
pub struct LinePart {
    part: String,
    len: usize,
}

#[derive(Default)]
struct State {
    active_tab_index: usize,
    num_tabs: usize,
}

static ARROW_SEPARATOR: &str = "";

register_tile!(State);

impl ZellijTile for State {
    fn init(&mut self) {
        set_selectable(false);
        set_invisible_borders(true);
        set_max_height(1);
        self.active_tab_index = 0;
        self.num_tabs = 0;
    }

    fn draw(&mut self, _rows: usize, cols: usize) {
        if self.num_tabs == 0 {
            return;
        }
        let mut all_tabs: Vec<LinePart> = vec![];
        for i in 0..self.num_tabs {
            let tab = nameless_tab(i, i == self.active_tab_index);
            all_tabs.push(tab);
        }

        let tab_line = tab_line(all_tabs, self.active_tab_index, cols);

        let mut s = String::new();
        for bar_part in tab_line {
            s = format!("{}{}", s, bar_part.part);
        }
        println!("{}\u{1b}[40m\u{1b}[0K", s);
    }

    fn update_tabs(&mut self, active_tab_index: usize, num_tabs: usize) {
        self.active_tab_index = active_tab_index;
        self.num_tabs = num_tabs;
    }
}
