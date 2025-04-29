use zellij_utils::pane_size::{Offset, Viewport};

use crate::output::CharacterChunk;
use crate::panes::terminal_character::{TerminalCharacter, EMPTY_TERMINAL_CHARACTER, RESET_STYLES};
use crate::tab::Pane;
use ansi_term::Colour::{Fixed, RGB};
use std::collections::HashMap;
use zellij_utils::errors::prelude::*;
use zellij_utils::{data::PaletteColor, shared::colors};

use std::fmt::{Display, Error, Formatter};
pub mod boundary_type {
    pub const TOP_RIGHT: &str = "┐";
    pub const TOP_RIGHT_ROUND: &str = "╮";
    pub const VERTICAL: &str = "│";
    pub const HORIZONTAL: &str = "─";
    pub const TOP_LEFT: &str = "┌";
    pub const TOP_LEFT_ROUND: &str = "╭";
    pub const BOTTOM_RIGHT: &str = "┘";
    pub const BOTTOM_RIGHT_ROUND: &str = "╯";
    pub const BOTTOM_LEFT: &str = "└";
    pub const BOTTOM_LEFT_ROUND: &str = "╰";
    pub const VERTICAL_LEFT: &str = "┤";
    pub const VERTICAL_RIGHT: &str = "├";
    pub const HORIZONTAL_DOWN: &str = "┬";
    pub const HORIZONTAL_UP: &str = "┴";
    pub const CROSS: &str = "┼";
}

pub type BoundaryType = &'static str; // easy way to refer to boundary_type above

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BoundarySymbol {
    boundary_type: BoundaryType,
    invisible: bool,
    color: Option<(PaletteColor, usize)>, // (color, color_precedence)
}

impl BoundarySymbol {
    pub fn new(boundary_type: BoundaryType) -> Self {
        BoundarySymbol {
            boundary_type,
            invisible: false,
            color: Some((PaletteColor::EightBit(colors::GRAY), 0)),
        }
    }
    pub fn color(&mut self, color: Option<(PaletteColor, usize)>) -> Self {
        self.color = color;
        *self
    }
    pub fn as_terminal_character(&self) -> Result<TerminalCharacter> {
        let tc = if self.invisible {
            EMPTY_TERMINAL_CHARACTER
        } else {
            let character = self
                .boundary_type
                .chars()
                .next()
                .context("no boundary symbols defined")
                .with_context(|| {
                    format!(
                        "failed to convert boundary symbol {} into terminal character",
                        self.boundary_type
                    )
                })?;
            TerminalCharacter::new_singlewidth_styled(
                character,
                RESET_STYLES
                    .foreground(self.color.map(|palette_color| palette_color.0.into()))
                    .into(),
            )
        };
        Ok(tc)
    }
}

impl Display for BoundarySymbol {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        match self.invisible {
            true => write!(f, " "),
            false => match self.color {
                Some(color) => match color.0 {
                    PaletteColor::Rgb((r, g, b)) => {
                        write!(f, "{}", RGB(r, g, b).paint(self.boundary_type))
                    },
                    PaletteColor::EightBit(color) => {
                        write!(f, "{}", Fixed(color).paint(self.boundary_type))
                    },
                },
                None => write!(f, "{}", self.boundary_type),
            },
        }
    }
}

fn combine_symbols(
    current_symbol: BoundarySymbol,
    next_symbol: BoundarySymbol,
) -> Option<BoundarySymbol> {
    use boundary_type::*;
    let invisible = current_symbol.invisible || next_symbol.invisible;
    let color = match (current_symbol.color, next_symbol.color) {
        (Some(current_symbol_color), Some(next_symbol_color)) => {
            let ret = if current_symbol_color.1 >= next_symbol_color.1 {
                Some(current_symbol_color)
            } else {
                Some(next_symbol_color)
            };
            ret
        },
        _ => current_symbol.color.or(next_symbol.color),
    };
    match (current_symbol.boundary_type, next_symbol.boundary_type) {
        (CROSS, _) | (_, CROSS) => {
            // (┼, *) or (*, ┼) => Some(┼)
            let boundary_type = CROSS;
            Some(BoundarySymbol {
                boundary_type,
                invisible,
                color,
            })
        },
        (TOP_RIGHT, TOP_RIGHT) => {
            // (┐, ┐) => Some(┐)
            let boundary_type = TOP_RIGHT;
            Some(BoundarySymbol {
                boundary_type,
                invisible,
                color,
            })
        },
        (TOP_RIGHT, VERTICAL) | (TOP_RIGHT, BOTTOM_RIGHT) | (TOP_RIGHT, VERTICAL_LEFT) => {
            // (┐, │) => Some(┤)
            // (┐, ┘) => Some(┤)
            // (─, ┤) => Some(┤)
            let boundary_type = VERTICAL_LEFT;
            Some(BoundarySymbol {
                boundary_type,
                invisible,
                color,
            })
        },
        (TOP_RIGHT, HORIZONTAL) | (TOP_RIGHT, TOP_LEFT) | (TOP_RIGHT, HORIZONTAL_DOWN) => {
            // (┐, ─) => Some(┬)
            // (┐, ┌) => Some(┬)
            // (┐, ┬) => Some(┬)
            let boundary_type = HORIZONTAL_DOWN;
            Some(BoundarySymbol {
                boundary_type,
                invisible,
                color,
            })
        },
        (TOP_RIGHT, BOTTOM_LEFT) | (TOP_RIGHT, VERTICAL_RIGHT) | (TOP_RIGHT, HORIZONTAL_UP) => {
            // (┐, └) => Some(┼)
            // (┐, ├) => Some(┼)
            // (┐, ┴) => Some(┼)
            let boundary_type = CROSS;
            Some(BoundarySymbol {
                boundary_type,
                invisible,
                color,
            })
        },
        (HORIZONTAL, HORIZONTAL) => {
            // (─, ─) => Some(─)
            let boundary_type = HORIZONTAL;
            Some(BoundarySymbol {
                boundary_type,
                invisible,
                color,
            })
        },
        (HORIZONTAL, VERTICAL) | (HORIZONTAL, VERTICAL_LEFT) | (HORIZONTAL, VERTICAL_RIGHT) => {
            // (─, │) => Some(┼)
            // (─, ┤) => Some(┼)
            // (─, ├) => Some(┼)
            let boundary_type = CROSS;
            Some(BoundarySymbol {
                boundary_type,
                invisible,
                color,
            })
        },
        (HORIZONTAL, TOP_LEFT) | (HORIZONTAL, HORIZONTAL_DOWN) => {
            // (─, ┌) => Some(┬)
            // (─, ┬) => Some(┬)
            let boundary_type = HORIZONTAL_DOWN;
            Some(BoundarySymbol {
                boundary_type,
                invisible,
                color,
            })
        },
        (HORIZONTAL, BOTTOM_RIGHT) | (HORIZONTAL, BOTTOM_LEFT) | (HORIZONTAL, HORIZONTAL_UP) => {
            // (─, ┘) => Some(┴)
            // (─, └) => Some(┴)
            // (─, ┴) => Some(┴)
            let boundary_type = HORIZONTAL_UP;
            Some(BoundarySymbol {
                boundary_type,
                invisible,
                color,
            })
        },
        (VERTICAL, VERTICAL) => {
            // (│, │) => Some(│)
            let boundary_type = VERTICAL;
            Some(BoundarySymbol {
                boundary_type,
                invisible,
                color,
            })
        },
        (VERTICAL, TOP_LEFT) | (VERTICAL, BOTTOM_LEFT) | (VERTICAL, VERTICAL_RIGHT) => {
            // (│, ┌) => Some(├)
            // (│, └) => Some(├)
            // (│, ├) => Some(├)
            let boundary_type = VERTICAL_RIGHT;
            Some(BoundarySymbol {
                boundary_type,
                invisible,
                color,
            })
        },
        (VERTICAL, BOTTOM_RIGHT) | (VERTICAL, VERTICAL_LEFT) => {
            // (│, ┘) => Some(┤)
            // (│, ┤) => Some(┤)
            let boundary_type = VERTICAL_LEFT;
            Some(BoundarySymbol {
                boundary_type,
                invisible,
                color,
            })
        },
        (VERTICAL, HORIZONTAL_DOWN) | (VERTICAL, HORIZONTAL_UP) => {
            // (│, ┬) => Some(┼)
            // (│, ┴) => Some(┼)
            let boundary_type = CROSS;
            Some(BoundarySymbol {
                boundary_type,
                invisible,
                color,
            })
        },
        (TOP_LEFT, TOP_LEFT) => {
            // (┌, ┌) => Some(┌)
            let boundary_type = TOP_LEFT;
            Some(BoundarySymbol {
                boundary_type,
                invisible,
                color,
            })
        },
        (TOP_LEFT, BOTTOM_RIGHT) | (TOP_LEFT, VERTICAL_LEFT) | (TOP_LEFT, HORIZONTAL_UP) => {
            // (┌, ┘) => Some(┼)
            // (┌, ┤) => Some(┼)
            // (┌, ┴) => Some(┼)
            let boundary_type = CROSS;
            Some(BoundarySymbol {
                boundary_type,
                invisible,
                color,
            })
        },
        (TOP_LEFT, BOTTOM_LEFT) | (TOP_LEFT, VERTICAL_RIGHT) => {
            // (┌, └) => Some(├)
            // (┌, ├) => Some(├)
            let boundary_type = VERTICAL_RIGHT;
            Some(BoundarySymbol {
                boundary_type,
                invisible,
                color,
            })
        },
        (TOP_LEFT, HORIZONTAL_DOWN) => {
            // (┌, ┬) => Some(┬)
            let boundary_type = HORIZONTAL_DOWN;
            Some(BoundarySymbol {
                boundary_type,
                invisible,
                color,
            })
        },
        (BOTTOM_RIGHT, BOTTOM_RIGHT) => {
            // (┘, ┘) => Some(┘)
            let boundary_type = BOTTOM_RIGHT;
            Some(BoundarySymbol {
                boundary_type,
                invisible,
                color,
            })
        },
        (BOTTOM_RIGHT, BOTTOM_LEFT) | (BOTTOM_RIGHT, HORIZONTAL_UP) => {
            // (┘, └) => Some(┴)
            // (┘, ┴) => Some(┴)
            let boundary_type = HORIZONTAL_UP;
            Some(BoundarySymbol {
                boundary_type,
                invisible,
                color,
            })
        },
        (BOTTOM_RIGHT, VERTICAL_LEFT) => {
            // (┘, ┤) => Some(┤)
            let boundary_type = VERTICAL_LEFT;
            Some(BoundarySymbol {
                boundary_type,
                invisible,
                color,
            })
        },
        (BOTTOM_RIGHT, VERTICAL_RIGHT) | (BOTTOM_RIGHT, HORIZONTAL_DOWN) => {
            // (┘, ├) => Some(┼)
            // (┘, ┬) => Some(┼)
            let boundary_type = CROSS;
            Some(BoundarySymbol {
                boundary_type,
                invisible,
                color,
            })
        },
        (BOTTOM_LEFT, BOTTOM_LEFT) => {
            // (└, └) => Some(└)
            let boundary_type = BOTTOM_LEFT;
            Some(BoundarySymbol {
                boundary_type,
                invisible,
                color,
            })
        },
        (BOTTOM_LEFT, VERTICAL_LEFT) | (BOTTOM_LEFT, HORIZONTAL_DOWN) => {
            // (└, ┤) => Some(┼)
            // (└, ┬) => Some(┼)
            let boundary_type = CROSS;
            Some(BoundarySymbol {
                boundary_type,
                invisible,
                color,
            })
        },
        (BOTTOM_LEFT, VERTICAL_RIGHT) => {
            // (└, ├) => Some(├)
            let boundary_type = VERTICAL_RIGHT;
            Some(BoundarySymbol {
                boundary_type,
                invisible,
                color,
            })
        },
        (BOTTOM_LEFT, HORIZONTAL_UP) => {
            // (└, ┴) => Some(┴)
            let boundary_type = HORIZONTAL_UP;
            Some(BoundarySymbol {
                boundary_type,
                invisible,
                color,
            })
        },
        (VERTICAL_LEFT, VERTICAL_LEFT) => {
            // (┤, ┤) => Some(┤)
            let boundary_type = VERTICAL_LEFT;
            Some(BoundarySymbol {
                boundary_type,
                invisible,
                color,
            })
        },
        (VERTICAL_LEFT, VERTICAL_RIGHT)
        | (VERTICAL_LEFT, HORIZONTAL_DOWN)
        | (VERTICAL_LEFT, HORIZONTAL_UP) => {
            // (┤, ├) => Some(┼)
            // (┤, ┬) => Some(┼)
            // (┤, ┴) => Some(┼)
            let boundary_type = CROSS;
            Some(BoundarySymbol {
                boundary_type,
                invisible,
                color,
            })
        },
        (VERTICAL_RIGHT, VERTICAL_RIGHT) => {
            // (├, ├) => Some(├)
            let boundary_type = VERTICAL_RIGHT;
            Some(BoundarySymbol {
                boundary_type,
                invisible,
                color,
            })
        },
        (VERTICAL_RIGHT, HORIZONTAL_DOWN) | (VERTICAL_RIGHT, HORIZONTAL_UP) => {
            // (├, ┬) => Some(┼)
            // (├, ┴) => Some(┼)
            let boundary_type = CROSS;
            Some(BoundarySymbol {
                boundary_type,
                invisible,
                color,
            })
        },
        (HORIZONTAL_DOWN, HORIZONTAL_DOWN) => {
            // (┬, ┬) => Some(┬)
            let boundary_type = HORIZONTAL_DOWN;
            Some(BoundarySymbol {
                boundary_type,
                invisible,
                color,
            })
        },
        (HORIZONTAL_DOWN, HORIZONTAL_UP) => {
            // (┬, ┴) => Some(┼)
            let boundary_type = CROSS;
            Some(BoundarySymbol {
                boundary_type,
                invisible,
                color,
            })
        },
        (HORIZONTAL_UP, HORIZONTAL_UP) => {
            // (┴, ┴) => Some(┴)
            let boundary_type = HORIZONTAL_UP;
            Some(BoundarySymbol {
                boundary_type,
                invisible,
                color,
            })
        },
        (_, _) => combine_symbols(next_symbol, current_symbol),
    }
}

#[derive(PartialEq, Eq, Hash, Debug)]
pub struct Coordinates {
    x: usize,
    y: usize,
}

impl Coordinates {
    pub fn new(x: usize, y: usize) -> Self {
        Coordinates { x, y }
    }
}

pub struct Boundaries {
    viewport: Viewport,
    pub boundary_characters: HashMap<Coordinates, BoundarySymbol>,
}

#[allow(clippy::if_same_then_else)]
impl Boundaries {
    pub fn new(viewport: Viewport) -> Self {
        Boundaries {
            viewport,
            boundary_characters: HashMap::new(),
        }
    }
    pub fn add_rect(
        &mut self,
        rect: &dyn Pane,
        color: Option<(PaletteColor, usize)>, // (color, color_precedence)
        pane_is_on_top_of_stack: bool,
        pane_is_on_bottom_of_stack: bool,
        pane_is_stacked_under: bool,
    ) {
        let pane_is_stacked = rect.current_geom().is_stacked();
        let should_skip_top_boundary = pane_is_stacked && !pane_is_on_top_of_stack;
        let should_skip_bottom_boundary = pane_is_stacked && !pane_is_on_bottom_of_stack;
        let content_offset = rect.get_content_offset();
        if !self.is_fully_inside_screen(rect) {
            return;
        }
        if rect.x() > self.viewport.x {
            // left boundary
            let boundary_x_coords = rect.x() - 1;
            let first_row_coordinates =
                self.rect_right_boundary_row_start(rect, pane_is_stacked_under, content_offset);
            let last_row_coordinates = self.rect_right_boundary_row_end(rect);
            for row in first_row_coordinates..last_row_coordinates {
                let coordinates = Coordinates::new(boundary_x_coords, row);
                let symbol_to_add = if row == first_row_coordinates && row != self.viewport.y {
                    if pane_is_stacked {
                        BoundarySymbol::new(boundary_type::VERTICAL_RIGHT).color(color)
                    } else {
                        BoundarySymbol::new(boundary_type::TOP_LEFT).color(color)
                    }
                } else if row == first_row_coordinates && pane_is_stacked {
                    BoundarySymbol::new(boundary_type::TOP_LEFT).color(color)
                } else if row == last_row_coordinates - 1
                    && row != self.viewport.y + self.viewport.rows - 1
                    && content_offset.bottom > 0
                {
                    BoundarySymbol::new(boundary_type::BOTTOM_LEFT).color(color)
                } else {
                    BoundarySymbol::new(boundary_type::VERTICAL).color(color)
                };
                let next_symbol = self
                    .boundary_characters
                    .remove(&coordinates)
                    .and_then(|current_symbol| combine_symbols(current_symbol, symbol_to_add))
                    .unwrap_or(symbol_to_add);
                self.boundary_characters.insert(coordinates, next_symbol);
            }
        }
        if rect.y() > self.viewport.y && !should_skip_top_boundary {
            // top boundary
            let boundary_y_coords = rect.y() - 1;
            let first_col_coordinates = self.rect_bottom_boundary_col_start(rect);
            let last_col_coordinates = self.rect_bottom_boundary_col_end(rect);
            for col in first_col_coordinates..last_col_coordinates {
                let coordinates = Coordinates::new(col, boundary_y_coords);
                let symbol_to_add = if col == first_col_coordinates && col != self.viewport.x {
                    BoundarySymbol::new(boundary_type::TOP_LEFT).color(color)
                } else if col == last_col_coordinates - 1 && col != self.viewport.cols - 1 {
                    BoundarySymbol::new(boundary_type::TOP_RIGHT).color(color)
                } else {
                    BoundarySymbol::new(boundary_type::HORIZONTAL).color(color)
                };
                let next_symbol = self
                    .boundary_characters
                    .remove(&coordinates)
                    .and_then(|current_symbol| combine_symbols(current_symbol, symbol_to_add))
                    .unwrap_or(symbol_to_add);
                self.boundary_characters.insert(coordinates, next_symbol);
            }
        }
        if self.rect_right_boundary_is_before_screen_edge(rect) {
            // right boundary
            let boundary_x_coords = rect.right_boundary_x_coords() - 1;
            let first_row_coordinates =
                self.rect_right_boundary_row_start(rect, pane_is_stacked_under, content_offset);
            let last_row_coordinates = self.rect_right_boundary_row_end(rect);
            for row in first_row_coordinates..last_row_coordinates {
                let coordinates = Coordinates::new(boundary_x_coords, row);
                let symbol_to_add = if row == first_row_coordinates && pane_is_stacked {
                    BoundarySymbol::new(boundary_type::VERTICAL_LEFT).color(color)
                } else if row == first_row_coordinates && row != self.viewport.y {
                    if pane_is_stacked {
                        BoundarySymbol::new(boundary_type::VERTICAL_LEFT).color(color)
                    } else {
                        BoundarySymbol::new(boundary_type::TOP_RIGHT).color(color)
                    }
                } else if row == last_row_coordinates - 1
                    && row != self.viewport.y + self.viewport.rows - 1
                    && content_offset.bottom > 0
                {
                    BoundarySymbol::new(boundary_type::BOTTOM_RIGHT).color(color)
                } else {
                    BoundarySymbol::new(boundary_type::VERTICAL).color(color)
                };
                let next_symbol = self
                    .boundary_characters
                    .remove(&coordinates)
                    .and_then(|current_symbol| combine_symbols(current_symbol, symbol_to_add))
                    .unwrap_or(symbol_to_add);
                self.boundary_characters.insert(coordinates, next_symbol);
            }
        }
        if self.rect_bottom_boundary_is_before_screen_edge(rect) && !should_skip_bottom_boundary {
            // bottom boundary
            let boundary_y_coords = rect.bottom_boundary_y_coords() - 1;
            let first_col_coordinates = self.rect_bottom_boundary_col_start(rect);
            let last_col_coordinates = self.rect_bottom_boundary_col_end(rect);
            for col in first_col_coordinates..last_col_coordinates {
                let coordinates = Coordinates::new(col, boundary_y_coords);
                let symbol_to_add = if col == first_col_coordinates && col != self.viewport.x {
                    BoundarySymbol::new(boundary_type::BOTTOM_LEFT).color(color)
                } else if col == last_col_coordinates - 1 && col != self.viewport.cols - 1 {
                    BoundarySymbol::new(boundary_type::BOTTOM_RIGHT).color(color)
                } else {
                    BoundarySymbol::new(boundary_type::HORIZONTAL).color(color)
                };
                let next_symbol = self
                    .boundary_characters
                    .remove(&coordinates)
                    .and_then(|current_symbol| combine_symbols(current_symbol, symbol_to_add))
                    .unwrap_or(symbol_to_add);
                self.boundary_characters.insert(coordinates, next_symbol);
            }
        }
    }
    pub fn render(
        &self,
        existing_boundaries_on_screen: Option<&Boundaries>,
    ) -> Result<Vec<CharacterChunk>> {
        let mut character_chunks = vec![];
        for (coordinates, boundary_character) in &self.boundary_characters {
            let already_on_screen = existing_boundaries_on_screen
                .and_then(|e| e.boundary_characters.get(coordinates))
                .map(|e| e == boundary_character)
                .unwrap_or(false);
            if already_on_screen {
                continue;
            }
            character_chunks.push(CharacterChunk::new(
                vec![boundary_character
                    .as_terminal_character()
                    .context("failed to render as terminal character")?],
                coordinates.x,
                coordinates.y,
            ));
        }
        Ok(character_chunks)
    }
    fn rect_right_boundary_is_before_screen_edge(&self, rect: &dyn Pane) -> bool {
        rect.x() + rect.cols() < self.viewport.cols
    }
    fn rect_bottom_boundary_is_before_screen_edge(&self, rect: &dyn Pane) -> bool {
        rect.y() + rect.rows() < self.viewport.y + self.viewport.rows
    }
    fn rect_right_boundary_row_start(
        &self,
        rect: &dyn Pane,
        pane_is_stacked_under: bool,
        content_offset: Offset,
    ) -> usize {
        let pane_is_stacked = rect.current_geom().is_stacked();
        let horizontal_frame_offset = if pane_is_stacked_under {
            // these panes - panes that are in a stack below the flexible pane - need to have their
            // content offset taken into account when rendering them (i.e. they are rendered
            // one line above their actual y coordinates, since they are only 1 line)
            // as opposed to panes that are in a stack above the flexible pane who do not because
            // they are rendered in place (the content offset of the stack is "absorbed" by the
            // flexible pane below them)
            content_offset.bottom
        } else if pane_is_stacked {
            0
        } else {
            1
        };
        if rect.y() > self.viewport.y {
            rect.y().saturating_sub(horizontal_frame_offset)
        } else {
            self.viewport.y
        }
    }
    fn rect_right_boundary_row_end(&self, rect: &dyn Pane) -> usize {
        rect.y() + rect.rows()
    }
    fn rect_bottom_boundary_col_start(&self, rect: &dyn Pane) -> usize {
        if rect.x() == 0 {
            0
        } else {
            rect.x() - 1
        }
    }
    fn rect_bottom_boundary_col_end(&self, rect: &dyn Pane) -> usize {
        rect.x() + rect.cols()
    }
    fn is_fully_inside_screen(&self, rect: &dyn Pane) -> bool {
        rect.x() >= self.viewport.x
            && rect.x() + rect.cols() <= self.viewport.x + self.viewport.cols
            && rect.y() >= self.viewport.y
            && rect.y() + rect.rows() <= self.viewport.y + self.viewport.rows
    }
}
