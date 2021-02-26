use colored::*;
use zellij_tile::*;

struct LinePart {
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

    fn draw(&mut self, _rows: usize, _cols: usize) {
        let mut s = String::new();
        let active_tab = self.active_tab_index + 1;
        // TODO:
        // * loop through tabs, find the length of each tab string and add to it the length of the
        // arrows and padding
        // * register this length in a global place
        // * if the length is greater than cols + calculated MORE_MESSAGE length:
        //   - if we have already reached the active_tab, break the loop and render all the tabs
        //   - if not, remove the first tab to be rendered and continue
        for i in 1..=self.num_tabs {
            let tab;
            if i == active_tab {
                let right_separator = if i == self.num_tabs {
                    format!("{}", ARROW_SEPARATOR.magenta().on_black())
                } else {
                    format!("{}{}", ARROW_SEPARATOR.magenta().on_black(), ARROW_SEPARATOR.black().on_green())
                };
                tab = format!(" Tab #{} {}", i, right_separator).black().bold().on_magenta();
            } else {
                let right_separator = if i == self.num_tabs {
                    format!("{}", ARROW_SEPARATOR.green().on_black())
                } else if i + 1 == active_tab {
                    format!("{}{}", ARROW_SEPARATOR.green().on_black(), ARROW_SEPARATOR.black().on_magenta())
                } else {
                    format!("{}{}", ARROW_SEPARATOR.green().on_black(), ARROW_SEPARATOR.black().on_green())
                };
                tab = format!(" Tab #{} {}", i, right_separator).black().bold().on_green();
            }
            s = format!("{}{}", s, tab);
        }
        println!("{}\u{1b}[40m\u{1b}[0K", s);
    }

    fn update_tabs(&mut self, active_tab_index: usize, num_tabs: usize) {
        self.active_tab_index = active_tab_index;
        self.num_tabs = num_tabs;
    }
}
