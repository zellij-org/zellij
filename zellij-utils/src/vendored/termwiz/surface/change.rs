use crate::vendored::termwiz::cell::{unicode_column_width, AttributeChange, CellAttributes};
use crate::vendored::termwiz::color::ColorAttribute;
pub use crate::vendored::termwiz::image::{ImageData, TextureCoordinate};
use crate::vendored::termwiz::surface::{CursorShape, CursorVisibility, Position};
use finl_unicode::grapheme_clusters::Graphemes;
#[cfg(feature = "use_serde")]
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[cfg_attr(feature = "use_serde", derive(Serialize, Deserialize))]
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum LineAttribute {
    DoubleHeightTopHalfLine,
    DoubleHeightBottomHalfLine,
    DoubleWidthLine,
    SingleWidthLine,
}

/// `Change` describes an update operation to be applied to a `Surface`.
/// Changes to the active attributes (color, style), moving the cursor
/// and outputting text are examples of some of the values.
#[cfg_attr(feature = "use_serde", derive(Serialize, Deserialize))]
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum Change {
    /// Change a single attribute
    Attribute(AttributeChange),
    /// Change all possible attributes to the given set of values
    AllAttributes(CellAttributes),
    /// Add printable text.
    /// Control characters are rendered inert by transforming them
    /// to space.  CR and LF characters are interpreted by moving
    /// the cursor position.  CR moves the cursor to the start of
    /// the line and LF moves the cursor down to the next line.
    /// You typically want to use both together when sending in
    /// a line break.
    Text(String),
    /// Clear the screen to the specified color.
    /// Implicitly clears all attributes prior to clearing the screen.
    /// Moves the cursor to the home position (top left).
    ClearScreen(ColorAttribute),
    /// Clear from the current cursor X position to the rightmost
    /// edge of the screen.  The background color is set to the
    /// provided color.  The cursor position remains unchanged.
    ClearToEndOfLine(ColorAttribute),
    /// Clear from the current cursor X position to the rightmost
    /// edge of the screen on the current line.  Clear all of the
    /// lines below the current cursor Y position.  The background
    /// color is set ot the provided color.  The cursor position
    /// remains unchanged.
    ClearToEndOfScreen(ColorAttribute),
    /// Move the cursor to the specified `Position`.
    CursorPosition { x: Position, y: Position },
    /// Change the cursor color.
    CursorColor(ColorAttribute),
    /// Change the cursor shape
    CursorShape(CursorShape),
    /// Change the cursor visibility
    CursorVisibility(CursorVisibility),
    /// Place an image at the current cursor position.
    /// The image defines the dimensions in cells.
    /// TODO: check iterm rendering behavior when the image is larger than the width of the screen.
    /// If the image is taller than the remaining space at the bottom
    /// of the screen, the screen will scroll up.
    /// The cursor Y position is unchanged by rendering the Image.
    /// The cursor X position will be incremented by `Image::width` cells.
    Image(Image),
    /// Scroll the `region_size` lines starting at `first_row` upwards
    /// by `scroll_count` lines.  The `scroll_count` lines at the top of
    /// the region are overwritten.  The `scroll_count` lines at the
    /// bottom of the region will become blank.
    ///
    /// After a region is scrolled, the cursor position is undefined,
    /// and the terminal's scroll region is set to the range specified.
    /// To restore scrolling behaviour to the full terminal window, an
    /// additional `Change::ScrollRegionUp { first_row: 0, region_size:
    /// height, scroll_count: 0 }`, where `height` is the height of the
    /// terminal, should be emitted.
    ScrollRegionUp {
        first_row: usize,
        region_size: usize,
        scroll_count: usize,
    },
    /// Scroll the `region_size` lines starting at `first_row` downwards
    /// by `scroll_count` lines.  The `scroll_count` lines at the bottom
    /// the region are overwritten.  The `scroll_count` lines at the top
    /// of the region will become blank.
    ///
    /// After a region is scrolled, the cursor position is undefined,
    /// and the terminal's scroll region is set to the range specified.
    /// To restore scrolling behaviour to the full terminal window, an
    /// additional `Change::ScrollRegionDown { first_row: 0,
    /// region_size: height, scroll_count: 0 }`, where `height` is the
    /// height of the terminal, should be emitted.
    ScrollRegionDown {
        first_row: usize,
        region_size: usize,
        scroll_count: usize,
    },
    /// Change the title of the window in which the surface will be
    /// rendered.
    Title(String),

    /// Adjust the current line attributes, such as double height or width
    LineAttribute(LineAttribute),
}

impl Change {
    pub fn is_text(&self) -> bool {
        matches!(self, Change::Text(_))
    }

    pub fn text(&self) -> &str {
        match self {
            Change::Text(text) => text,
            _ => panic!("you must use Change::is_text() to guard calls to Change::text()"),
        }
    }
}

impl<S: Into<String>> From<S> for Change {
    fn from(s: S) -> Self {
        Change::Text(s.into())
    }
}

impl From<AttributeChange> for Change {
    fn from(c: AttributeChange) -> Self {
        Change::Attribute(c)
    }
}

impl From<LineAttribute> for Change {
    fn from(attr: LineAttribute) -> Self {
        Change::LineAttribute(attr)
    }
}

/// Keeps track of a run of changes and allows reasoning about the cursor
/// position and the extent of the screen that the sequence will affect.
/// This is useful for example when implementing something like a LineEditor
/// where you don't want to take control over the entire surface but do want
/// to be able to emit a dynamically sized output relative to the cursor
/// position at the time that the editor is invoked.
pub struct ChangeSequence {
    changes: Vec<Change>,
    screen_rows: usize,
    screen_cols: usize,
    pub(crate) cursor_x: usize,
    pub(crate) cursor_y: isize,
    render_y_max: isize,
    render_y_min: isize,
}

impl ChangeSequence {
    pub fn new(rows: usize, cols: usize) -> Self {
        Self {
            changes: vec![],
            screen_rows: rows,
            screen_cols: cols,
            cursor_x: 0,
            cursor_y: 0,
            render_y_max: 0,
            render_y_min: 0,
        }
    }

    pub fn consume(self) -> Vec<Change> {
        self.changes
    }

    /// Returns the cursor position, (x, y).
    pub fn current_cursor_position(&self) -> (usize, isize) {
        (self.cursor_x, self.cursor_y)
    }

    pub fn move_to(&mut self, (cursor_x, cursor_y): (usize, isize)) {
        self.add(Change::CursorPosition {
            x: Position::Relative(cursor_x as isize - self.cursor_x as isize),
            y: Position::Relative(cursor_y - self.cursor_y),
        });
    }

    /// Returns the total number of rows affected
    pub fn render_height(&self) -> usize {
        (self.render_y_max - self.render_y_min).max(0).abs() as usize
    }

    fn update_render_height(&mut self) {
        self.render_y_max = self.render_y_max.max(self.cursor_y);
        self.render_y_min = self.render_y_min.min(self.cursor_y);
    }

    pub fn add_changes(&mut self, changes: Vec<Change>) {
        for change in changes {
            self.add(change);
        }
    }

    pub fn add<C: Into<Change>>(&mut self, change: C) {
        let change = change.into();
        match &change {
            Change::AllAttributes(_)
            | Change::Attribute(_)
            | Change::CursorColor(_)
            | Change::CursorShape(_)
            | Change::CursorVisibility(_)
            | Change::ClearToEndOfLine(_)
            | Change::Title(_)
            | Change::LineAttribute(_)
            | Change::ClearToEndOfScreen(_) => {},
            Change::Text(t) => {
                for g in Graphemes::new(t.as_str()) {
                    if self.cursor_x == self.screen_cols {
                        self.cursor_y += 1;
                        self.cursor_x = 0;
                    }
                    if g == "\n" {
                        self.cursor_y += 1;
                    } else if g == "\r" {
                        self.cursor_x = 0;
                    } else if g == "\r\n" {
                        self.cursor_y += 1;
                        self.cursor_x = 0;
                    } else {
                        let len = unicode_column_width(g, None);
                        self.cursor_x += len;
                    }
                }
                self.update_render_height();
            },
            Change::Image(im) => {
                self.cursor_x += im.width;
                self.render_y_max = self.render_y_max.max(self.cursor_y + im.height as isize);
            },
            Change::ClearScreen(_) => {
                self.cursor_x = 0;
                self.cursor_y = 0;
            },
            Change::CursorPosition { x, y } => {
                self.cursor_x = match x {
                    Position::Relative(x) => {
                        ((self.cursor_x as isize + x) % self.screen_cols as isize) as usize
                    },
                    Position::Absolute(x) => x % self.screen_cols,
                    Position::EndRelative(x) => (self.screen_cols - x) % self.screen_cols,
                };

                self.cursor_y = match y {
                    Position::Relative(y) => {
                        (self.cursor_y as isize + y) % self.screen_rows as isize
                    },
                    Position::Absolute(y) => (y % self.screen_rows) as isize,
                    Position::EndRelative(y) => {
                        ((self.screen_rows - y) % self.screen_rows) as isize
                    },
                };
                self.update_render_height();
            },
            Change::ScrollRegionUp { .. } | Change::ScrollRegionDown { .. } => {
                // The resultant cursor position is undefined by
                // the renderer!
                // We just pick something.
                self.cursor_x = 0;
                self.cursor_y = 0;
            },
        }

        self.changes.push(change);
    }
}

/// The `Image` `Change` needs to support adding an image that spans multiple
/// rows and columns, as well as model the content for just one of those cells.
/// For instance, if some of the cells inside an image are replaced by textual
/// content, and the screen is scrolled, computing the diff change stream needs
/// to be able to express that a single cell holds a slice from a larger image.
/// The `Image` struct expresses its dimensions in cells and references a region
/// in the shared source image data using texture coordinates.
/// A 4x3 cell image would set `width=3`, `height=3`, `top_left=(0,0)`, `bottom_right=(1,1)`.
/// The top left cell from that image, if it were to be included in a diff,
/// would be recorded as `width=1`, `height=1`, `top_left=(0,0)`, `bottom_right=(1/4,1/3)`.
#[cfg_attr(feature = "use_serde", derive(Serialize, Deserialize))]
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Image {
    /// measured in cells
    pub width: usize,
    /// measure in cells
    pub height: usize,
    /// Texture coordinate for the top left of this image block.
    /// (0,0) is the top left of the ImageData. (1, 1) is
    /// the bottom right.
    pub top_left: TextureCoordinate,
    /// Texture coordinates for the bottom right of this image block.
    pub bottom_right: TextureCoordinate,
    /// the image data
    pub image: Arc<ImageData>,
}
