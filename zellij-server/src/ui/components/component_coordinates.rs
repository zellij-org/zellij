use std::fmt::{self, Display, Formatter};

#[derive(Debug, Clone)]
pub struct Coordinates {
    pub x: usize,
    pub y: usize,
    pub width: Option<usize>,
    pub height: Option<usize>,
    /// columns and rows left in the pane from the origin on: what a component without an
    /// explicit width or height may use at most
    pub max_width: usize,
    pub max_height: usize,
}

impl Coordinates {
    pub fn stringify_with_y_offset(&self, y_offset: usize) -> String {
        format!("\u{1b}[{};{}H", self.y + y_offset + 1, self.x + 1)
    }
    /// The width, or the one the component would have by itself, but never past the pane
    pub fn width_or_else(&self, natural_width: impl FnOnce() -> usize) -> usize {
        self.width.unwrap_or_else(natural_width).min(self.max_width)
    }
    /// The height, or the one the component would have by itself, but never past the pane
    pub fn height_or_else(&self, natural_height: impl FnOnce() -> usize) -> usize {
        self.height
            .unwrap_or_else(natural_height)
            .min(self.max_height)
    }
}

impl Display for Coordinates {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "\u{1b}[{};{}H", self.y + 1, self.x + 1)
    }
}

pub fn is_too_wide(
    character_width: usize,
    current_width: usize,
    component_coordinates: &Option<Coordinates>,
) -> bool {
    if let Some(max_width) = component_coordinates.as_ref().and_then(|p| p.width) {
        if current_width + character_width > max_width {
            return true;
        }
    }
    false
}

pub fn is_too_high(current_height: usize, component_coordinates: &Option<Coordinates>) -> bool {
    if let Some(max_height) = component_coordinates.as_ref().and_then(|p| p.height) {
        if current_height > max_height {
            return true;
        }
    }
    false
}
