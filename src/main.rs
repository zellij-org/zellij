use colored::*;
use mosaic_tile::*;

#[derive(Default)]
struct State {
    lines: Vec<String>,
    page: usize,
}

register_tile!(State);

impl MosaicTile for State {
    fn init(&mut self) {
        set_selectable(false);
        set_max_height(1);
    }

    fn draw(&mut self, _rows: usize, cols: usize) {
        let more_msg = ", <?> More";
        self.lines = vec![String::new()];
        for item in get_help() {
            let width = self.lines.last().unwrap().len();
            if width + item.len() + 2 > cols - more_msg.len() {
                self.lines.last_mut().unwrap().push_str(more_msg);
                self.lines.push(item);
            } else {
                let line = self.lines.last_mut().unwrap();
                if !line.is_empty() {
                    line.push_str(", ");
                }
                line.push_str(&item);
            }
        }
        self.page %= self.lines.len();
        let line = format!(
            "{}{}",
            self.lines[self.page],
            if self.page > 0 && self.lines.len() == self.page + 1 {
                ", <?> Back"
            } else {
                ""
            }
        );
        println!("{}", line.italic());
    }

    fn handle_global_key(&mut self, key: Key) {
        if let Key::Char('?') = key {
            self.page += 1;
        }
    }
}
