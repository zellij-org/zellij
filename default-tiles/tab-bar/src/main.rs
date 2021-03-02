use colored::*;
use zellij_tile::*;

#[derive(Default)]
struct State {
    tabs: Vec<TabInfo>,
}

register_tile!(State);

impl ZellijTile for State {
    fn load(&mut self) {
        set_selectable(false);
        set_invisible_borders(true);
        set_max_height(1);
    }

    fn update(&mut self, _dt: f64) {
        let tabs = get_tab_info();
        if self.tabs != tabs {
            self.tabs = tabs;
            request_rerender();
        }
    }

    fn draw(&mut self, _rows: usize, _cols: usize) {
        let tabs: Vec<_> = self
            .tabs
            .iter()
            .map(|tab| {
                if tab.active {
                    format!("*{} ", tab.position)
                        .black()
                        .bold()
                        .on_magenta()
                        .to_string()
                } else {
                    format!("-{} ", tab.position).white().to_string()
                }
            })
            .collect();
        println!("Tabs: {}\u{1b}[40m\u{1b}[0K", tabs.concat());
    }
}
