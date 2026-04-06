use zellij_tile::prelude::*;

pub struct MultiLineErrorMessage {
    message: Vec<String>,
}

impl MultiLineErrorMessage {
    pub fn new(message: Vec<String>) -> Self {
        Self { message }
    }

    pub fn render(&self, x: usize, y: usize, max_rows: usize) {
        let title = Text::new("Error").error_color_all();
        print_text_with_coordinates(title, x, y, None, None);

        let mut current_y = y + 2;
        for line in self.message.iter().take(max_rows) {
            let text = Text::new(line).error_color_all();
            print_text_with_coordinates(text, x, current_y, None, None);
            current_y += 1;
        }

        let help = Text::new("Press any key to continue");
        print_text_with_coordinates(help, x, current_y + 2, None, None);
    }
}
