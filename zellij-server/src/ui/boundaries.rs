use zellij_utils::pane_size::{Offset, Viewport};

use crate::output::CharacterChunk;
use crate::panes::terminal_character::{TerminalCharacter, EMPTY_TERMINAL_CHARACTER, RESET_STYLES};
use crate::tab::Pane;
use crate::ui::border_glyphs::remap_light_glyph;
use ansi_term::Colour::{Fixed, RGB};
use std::collections::HashMap;
use zellij_utils::errors::prelude::*;
use zellij_utils::{
    data::{BorderStyle, LineStyle, PaletteColor},
    shared::colors,
};

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
    line_style: Option<LineStyle>,
}

impl BoundarySymbol {
    pub fn new(boundary_type: BoundaryType) -> Self {
        BoundarySymbol {
            boundary_type,
            invisible: false,
            color: Some((PaletteColor::EightBit(colors::GRAY), 0)),
            line_style: Some(LineStyle::Single),
        }
    }
    pub fn color(&mut self, color: Option<(PaletteColor, usize)>) -> Self {
        self.color = color;
        *self
    }
    pub fn line_style(&mut self, line_style: LineStyle) -> Self {
        self.line_style = Some(line_style);
        *self
    }
    pub fn glyph(&self, fallback_line_style: LineStyle) -> BoundaryType {
        remap_light_glyph(
            self.boundary_type,
            self.line_style.unwrap_or(fallback_line_style),
        )
    }
    pub fn as_terminal_character(
        &self,
        fallback_line_style: LineStyle,
    ) -> Result<TerminalCharacter> {
        let tc = if self.invisible {
            EMPTY_TERMINAL_CHARACTER
        } else {
            let glyph = self.glyph(fallback_line_style);
            let character = glyph
                .chars()
                .next()
                .context("no boundary symbols defined")
                .with_context(|| {
                    format!(
                        "failed to convert boundary symbol {} into terminal character",
                        glyph
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
        let glyph = self.glyph(LineStyle::Single);
        match self.invisible {
            true => write!(f, " "),
            false => match self.color {
                Some(color) => match color.0 {
                    PaletteColor::Rgb((r, g, b)) => write!(f, "{}", RGB(r, g, b).paint(glyph)),
                    PaletteColor::EightBit(color) => write!(f, "{}", Fixed(color).paint(glyph)),
                },
                None => write!(f, "{}", glyph),
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
    let line_style = match (current_symbol.line_style, next_symbol.line_style) {
        (Some(current_line_style), Some(next_line_style)) => {
            if current_line_style == next_line_style {
                Some(current_line_style)
            } else {
                None
            }
        },
        _ => None,
    };
    match (current_symbol.boundary_type, next_symbol.boundary_type) {
        (CROSS, _) | (_, CROSS) => {
            // (┼, *) or (*, ┼) => Some(┼)
            let boundary_type = CROSS;
            Some(BoundarySymbol {
                boundary_type,
                invisible,
                color,
                line_style,
            })
        },
        (TOP_RIGHT, TOP_RIGHT) => {
            // (┐, ┐) => Some(┐)
            let boundary_type = TOP_RIGHT;
            Some(BoundarySymbol {
                boundary_type,
                invisible,
                color,
                line_style,
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
                line_style,
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
                line_style,
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
                line_style,
            })
        },
        (HORIZONTAL, HORIZONTAL) => {
            // (─, ─) => Some(─)
            let boundary_type = HORIZONTAL;
            Some(BoundarySymbol {
                boundary_type,
                invisible,
                color,
                line_style,
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
                line_style,
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
                line_style,
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
                line_style,
            })
        },
        (VERTICAL, VERTICAL) => {
            // (│, │) => Some(│)
            let boundary_type = VERTICAL;
            Some(BoundarySymbol {
                boundary_type,
                invisible,
                color,
                line_style,
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
                line_style,
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
                line_style,
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
                line_style,
            })
        },
        (TOP_LEFT, TOP_LEFT) => {
            // (┌, ┌) => Some(┌)
            let boundary_type = TOP_LEFT;
            Some(BoundarySymbol {
                boundary_type,
                invisible,
                color,
                line_style,
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
                line_style,
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
                line_style,
            })
        },
        (TOP_LEFT, HORIZONTAL_DOWN) => {
            // (┌, ┬) => Some(┬)
            let boundary_type = HORIZONTAL_DOWN;
            Some(BoundarySymbol {
                boundary_type,
                invisible,
                color,
                line_style,
            })
        },
        (BOTTOM_RIGHT, BOTTOM_RIGHT) => {
            // (┘, ┘) => Some(┘)
            let boundary_type = BOTTOM_RIGHT;
            Some(BoundarySymbol {
                boundary_type,
                invisible,
                color,
                line_style,
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
                line_style,
            })
        },
        (BOTTOM_RIGHT, VERTICAL_LEFT) => {
            // (┘, ┤) => Some(┤)
            let boundary_type = VERTICAL_LEFT;
            Some(BoundarySymbol {
                boundary_type,
                invisible,
                color,
                line_style,
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
                line_style,
            })
        },
        (BOTTOM_LEFT, BOTTOM_LEFT) => {
            // (└, └) => Some(└)
            let boundary_type = BOTTOM_LEFT;
            Some(BoundarySymbol {
                boundary_type,
                invisible,
                color,
                line_style,
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
                line_style,
            })
        },
        (BOTTOM_LEFT, VERTICAL_RIGHT) => {
            // (└, ├) => Some(├)
            let boundary_type = VERTICAL_RIGHT;
            Some(BoundarySymbol {
                boundary_type,
                invisible,
                color,
                line_style,
            })
        },
        (BOTTOM_LEFT, HORIZONTAL_UP) => {
            // (└, ┴) => Some(┴)
            let boundary_type = HORIZONTAL_UP;
            Some(BoundarySymbol {
                boundary_type,
                invisible,
                color,
                line_style,
            })
        },
        (VERTICAL_LEFT, VERTICAL_LEFT) => {
            // (┤, ┤) => Some(┤)
            let boundary_type = VERTICAL_LEFT;
            Some(BoundarySymbol {
                boundary_type,
                invisible,
                color,
                line_style,
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
                line_style,
            })
        },
        (VERTICAL_RIGHT, VERTICAL_RIGHT) => {
            // (├, ├) => Some(├)
            let boundary_type = VERTICAL_RIGHT;
            Some(BoundarySymbol {
                boundary_type,
                invisible,
                color,
                line_style,
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
                line_style,
            })
        },
        (HORIZONTAL_DOWN, HORIZONTAL_DOWN) => {
            // (┬, ┬) => Some(┬)
            let boundary_type = HORIZONTAL_DOWN;
            Some(BoundarySymbol {
                boundary_type,
                invisible,
                color,
                line_style,
            })
        },
        (HORIZONTAL_DOWN, HORIZONTAL_UP) => {
            // (┬, ┴) => Some(┼)
            let boundary_type = CROSS;
            Some(BoundarySymbol {
                boundary_type,
                invisible,
                color,
                line_style,
            })
        },
        (HORIZONTAL_UP, HORIZONTAL_UP) => {
            // (┴, ┴) => Some(┴)
            let boundary_type = HORIZONTAL_UP;
            Some(BoundarySymbol {
                boundary_type,
                invisible,
                color,
                line_style,
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
    fallback_line_style: LineStyle,
    pub boundary_characters: HashMap<Coordinates, BoundarySymbol>,
}

#[allow(clippy::if_same_then_else)]
impl Boundaries {
    pub fn new(viewport: Viewport, fallback_line_style: LineStyle) -> Self {
        Boundaries {
            viewport,
            fallback_line_style,
            boundary_characters: HashMap::new(),
        }
    }
    pub fn add_rect(
        &mut self,
        rect: &dyn Pane,
        color: Option<(PaletteColor, usize)>, // (color, color_precedence)
        border_style: BorderStyle,
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
                        BoundarySymbol::new(boundary_type::VERTICAL_RIGHT)
                            .color(color)
                            .line_style(border_style.left)
                    } else {
                        BoundarySymbol::new(boundary_type::TOP_LEFT)
                            .color(color)
                            .line_style(border_style.left)
                    }
                } else if row == first_row_coordinates && pane_is_stacked {
                    BoundarySymbol::new(boundary_type::TOP_LEFT)
                        .color(color)
                        .line_style(border_style.left)
                } else if row == last_row_coordinates - 1
                    && row != self.viewport.y + self.viewport.rows - 1
                    && content_offset.bottom > 0
                {
                    BoundarySymbol::new(boundary_type::BOTTOM_LEFT)
                        .color(color)
                        .line_style(border_style.left)
                } else {
                    BoundarySymbol::new(boundary_type::VERTICAL)
                        .color(color)
                        .line_style(border_style.left)
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
                    BoundarySymbol::new(boundary_type::TOP_LEFT)
                        .color(color)
                        .line_style(border_style.top)
                } else if col == last_col_coordinates - 1 && col != self.viewport.cols - 1 {
                    BoundarySymbol::new(boundary_type::TOP_RIGHT)
                        .color(color)
                        .line_style(border_style.top)
                } else {
                    BoundarySymbol::new(boundary_type::HORIZONTAL)
                        .color(color)
                        .line_style(border_style.top)
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
                    BoundarySymbol::new(boundary_type::VERTICAL_LEFT)
                        .color(color)
                        .line_style(border_style.right)
                } else if row == first_row_coordinates && row != self.viewport.y {
                    if pane_is_stacked {
                        BoundarySymbol::new(boundary_type::VERTICAL_LEFT)
                            .color(color)
                            .line_style(border_style.right)
                    } else {
                        BoundarySymbol::new(boundary_type::TOP_RIGHT)
                            .color(color)
                            .line_style(border_style.right)
                    }
                } else if row == last_row_coordinates - 1
                    && row != self.viewport.y + self.viewport.rows - 1
                    && content_offset.bottom > 0
                {
                    BoundarySymbol::new(boundary_type::BOTTOM_RIGHT)
                        .color(color)
                        .line_style(border_style.right)
                } else {
                    BoundarySymbol::new(boundary_type::VERTICAL)
                        .color(color)
                        .line_style(border_style.right)
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
                    BoundarySymbol::new(boundary_type::BOTTOM_LEFT)
                        .color(color)
                        .line_style(border_style.bottom)
                } else if col == last_col_coordinates - 1 && col != self.viewport.cols - 1 {
                    BoundarySymbol::new(boundary_type::BOTTOM_RIGHT)
                        .color(color)
                        .line_style(border_style.bottom)
                } else {
                    BoundarySymbol::new(boundary_type::HORIZONTAL)
                        .color(color)
                        .line_style(border_style.bottom)
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
            let glyph = boundary_character.glyph(self.fallback_line_style);
            let already_on_screen = existing_boundaries_on_screen
                .and_then(|e| e.boundary_characters.get(coordinates))
                .map(|e| {
                    e.color == boundary_character.color
                        && e.invisible == boundary_character.invisible
                        && e.glyph(
                            existing_boundaries_on_screen
                                .map(|b| b.fallback_line_style)
                                .unwrap_or(LineStyle::Single),
                        ) == glyph
                })
                .unwrap_or(false);
            if already_on_screen {
                continue;
            }
            character_chunks.push(CharacterChunk::new(
                vec![boundary_character
                    .as_terminal_character(self.fallback_line_style)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn symbol(boundary_type: BoundaryType, line_style: LineStyle) -> BoundarySymbol {
        BoundarySymbol::new(boundary_type).line_style(line_style)
    }

    #[test]
    fn matching_line_styles_are_kept_through_a_junction() {
        let combined = combine_symbols(
            symbol(boundary_type::HORIZONTAL, LineStyle::Double),
            symbol(boundary_type::VERTICAL, LineStyle::Double),
        )
        .unwrap();
        assert_eq!(combined.glyph(LineStyle::Single), "╬");
    }

    #[test]
    fn conflicting_line_styles_fall_back_to_the_ambient_style() {
        let combined = combine_symbols(
            symbol(boundary_type::HORIZONTAL, LineStyle::Double),
            symbol(boundary_type::VERTICAL, LineStyle::Heavy),
        )
        .unwrap();
        assert_eq!(combined.glyph(LineStyle::Single), "┼");
        assert_eq!(combined.glyph(LineStyle::Double), "╬");
    }

    #[test]
    fn a_style_without_a_glyph_for_the_junction_falls_back_to_single() {
        let combined = combine_symbols(
            symbol(boundary_type::HORIZONTAL, LineStyle::Dashed),
            symbol(boundary_type::VERTICAL, LineStyle::Dashed),
        )
        .unwrap();
        assert_eq!(combined.glyph(LineStyle::Single), "┼");
        assert_eq!(
            symbol(boundary_type::HORIZONTAL, LineStyle::Dashed).glyph(LineStyle::Single),
            "┄"
        );
    }

    #[test]
    fn heavy_junctions_are_kept() {
        let combined = combine_symbols(
            symbol(boundary_type::HORIZONTAL, LineStyle::Heavy),
            symbol(boundary_type::TOP_LEFT, LineStyle::Heavy),
        )
        .unwrap();
        assert_eq!(combined.glyph(LineStyle::Single), "┳");
    }
}
