use colored::*;
use mosaic_tile::*;

#[derive(Default)]
struct State(usize, String);

register_tile!(State);

impl MosaicTile for State {
    fn init(&mut self) {
        set_selectable(false);
    }

    fn draw(&mut self, _rows: usize, cols: usize) {
        let line = format!("{}: {}", self.0, self.1);
        println!("{:cols$}", line.reversed(), cols = cols);
    }

    fn handle_global_key(&mut self, key: Key) {
        self.0 += 1;
        self.1 = format!("{:?}", key);
    }
}
