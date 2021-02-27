mod line;
mod tab;

use zellij_tile::*;

use crate::line::tab_line;
use crate::tab::tab_style;

#[derive(Debug)]
pub struct LinePart {
    part: String,
    len: usize,
}

#[derive(Default)]
struct State {
    active_tab_index: usize,
    num_tabs: usize,
    tabs: Vec<TabData>,
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
        let mut all_tabs: Vec<LinePart> = vec![];
        let mut active_tab_index = 0;
        for t in &self.tabs {
            let tab = tab_style(t.name.clone(), t.active, t.position);
            all_tabs.push(tab);
            if t.active {
                active_tab_index = t.position;
            }
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
}
