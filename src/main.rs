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
    }

    fn draw(&mut self, _rows: usize, cols: usize) {
        let mut width = 0;
        let more_msg = ", <?> More";
        self.lines = vec![String::new()];
        for item in get_help() {
            if width + item.len() > cols - more_msg.len() {
                self.lines.last_mut().unwrap().push_str(more_msg);
                width = item.len();
                self.lines.push(item);
            } else {
                let line = self.lines.last_mut().unwrap();
                if !line.is_empty() {
                    line.push_str(", ");
                }
                line.push_str(&item);
                width += item.len() + 2;
            }
        }
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
        if self.lines.len() > 1 {
            if let Key::Char('?') = key {
                self.page += 1;
                self.page %= self.lines.len();
            }
        }
    }
}
