use zellij_tile::prelude::*;

#[derive(Debug, Clone, Copy)]
pub struct Frame {
    pub x: usize,
    pub y: usize,
    pub width: usize,
    pub height: usize,
    pub offset: usize,
    pub pane_cols: usize,
}

impl Frame {
    pub fn screen_y(&self, row: usize, height: usize) -> Option<usize> {
        if row >= self.offset && row + height.max(1) <= self.offset + self.height {
            Some(self.y + row - self.offset)
        } else {
            None
        }
    }
    pub fn bottom(&self) -> usize {
        self.y + self.height
    }
    pub fn text(&self, row: usize, column: usize, text: Text) {
        if column >= self.width {
            return;
        }
        if let Some(y) = self.screen_y(row, 1) {
            print_text_with_coordinates(text, self.x + column, y, Some(self.width - column), None);
        }
    }
    pub fn heading(&self, row: usize, title: &str) {
        self.text(row, 0, Text::new(title).color_all(2));
    }
    pub fn caption(&self, row: usize, column: usize, caption: &str) {
        self.text(row, column, Text::new(caption).color_all(1));
    }
    pub fn note(&self, row: usize, column: usize, note: &str) {
        self.text(row, column, Text::new(note));
    }
}

pub struct Flow {
    width: usize,
    gap: usize,
    column: usize,
    row: usize,
    line_height: usize,
}

impl Flow {
    pub fn new(start_row: usize, width: usize, gap: usize) -> Self {
        Flow {
            width,
            gap,
            column: 0,
            row: start_row,
            line_height: 0,
        }
    }
    pub fn place(&mut self, width: usize, height: usize) -> (usize, usize) {
        if self.column > 0 && self.column + width > self.width {
            self.row += self.line_height + 1;
            self.column = 0;
            self.line_height = 0;
        }
        let position = (self.column, self.row);
        self.column += width + self.gap;
        self.line_height = self.line_height.max(height);
        position
    }
    pub fn end_row(&self) -> usize {
        self.row + self.line_height
    }
}
