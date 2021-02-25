use colored::*;
use zellij_tile::*;

#[derive(Default)]
struct State {
    active_tab_index: usize,
    num_tabs: usize,
}

register_tile!(State);

impl ZellijTile for State {
    fn init(&mut self) {
        set_selectable(false);
        set_invisible_borders(true);
        set_max_height(1);
        self.active_tab_index = 0;
        self.num_tabs = 0;
    }

    fn draw(&mut self, _rows: usize, _cols: usize) {
        let mut s = String::new();
        let active_tab = self.active_tab_index + 1;
        for i in 1..=self.num_tabs {
            let tab;
            if i == active_tab {
                tab = format!("*{} ", i).black().bold().on_magenta();
            } else {
                tab = format!("-{} ", i).white();
            }
            s = format!("{}{}", s, tab);
        }
        println!("Tabs: {}\u{1b}[40m\u{1b}[0K", s);
    }

    fn update_tabs(&mut self, active_tab_index: usize, num_tabs: usize) {
        self.active_tab_index = active_tab_index;
        self.num_tabs = num_tabs;
    }
}
