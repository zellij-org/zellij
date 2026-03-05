use crate::vendored::termwiz::cell::{Cell, CellAttributes};
use crate::vendored::termwiz::color::ColorAttribute;
use crate::vendored::termwiz::image::ImageCell;
use crate::vendored::termwiz::surface::line::CellRef;
use finl_unicode::grapheme_clusters::Graphemes;
use ordered_float::NotNan;
#[cfg(feature = "use_serde")]
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::cmp::min;
use wezterm_dynamic::{FromDynamic, ToDynamic};

pub mod change;
pub mod line;

pub use self::change::{Change, Image, LineAttribute, TextureCoordinate};
pub use self::line::Line;

/// Position holds 0-based positioning information, where
/// Absolute(0) is the start of the line or column,
/// Relative(0) is the current position in the line or
/// column and EndRelative(0) is the end position in the
/// line or column.
#[cfg_attr(feature = "use_serde", derive(Serialize, Deserialize))]
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum Position {
    /// Negative values move up, positive values down, 0 means no change
    Relative(isize),
    /// Relative to the start of the line or top of the screen
    Absolute(usize),
    /// Relative to the end of line or bottom of screen
    EndRelative(usize),
}

#[cfg_attr(feature = "use_serde", derive(Serialize, Deserialize))]
#[derive(Debug, Clone, Hash, Copy, PartialEq, Eq, FromDynamic, ToDynamic)]
pub enum CursorVisibility {
    Hidden,
    Visible,
}

impl Default for CursorVisibility {
    fn default() -> CursorVisibility {
        CursorVisibility::Visible
    }
}

#[cfg_attr(feature = "use_serde", derive(Serialize, Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, FromDynamic, ToDynamic)]
pub enum CursorShape {
    Default,
    BlinkingBlock,
    SteadyBlock,
    BlinkingUnderline,
    SteadyUnderline,
    BlinkingBar,
    SteadyBar,
}

impl Default for CursorShape {
    fn default() -> CursorShape {
        CursorShape::Default
    }
}

impl CursorShape {
    pub fn is_blinking(self) -> bool {
        matches!(
            self,
            Self::BlinkingBlock | Self::BlinkingUnderline | Self::BlinkingBar
        )
    }
}

/// SequenceNo indicates a logical position within a stream of changes.
/// The sequence is only meaningful within a given `Surface` instance.
pub type SequenceNo = usize;
pub const SEQ_ZERO: SequenceNo = 0;

/// The `Surface` type represents the contents of a terminal screen.
/// It is not directly connected to a terminal device.
/// It consists of a buffer and a log of changes.  You can accumulate
/// updates to the screen by adding instances of the `Change` enum
/// that describe the updates.
///
/// When ready to render the `Surface` to a `Terminal`, you can use
/// the `get_changes` method to return an optimized stream of `Change`s
/// since the last render and then pass it to an instance of `Renderer`.
///
/// `Surface`s can also be composited together; this is useful when
/// building up a UI with layers or widgets: each widget can be its
/// own `Surface` instance and have its content maintained independently
/// from the other widgets on the screen and can then be copied into
/// the target `Surface` buffer for rendering.
///
/// To support more efficient updates in the composite use case, a
/// `draw_from_screen` method is available; the intent is to have one
/// `Surface` be hold the data that was last rendered, and a second `Surface`
/// of the same size that is repeatedly redrawn from the composite
/// of the widgets.  `draw_from_screen` is used to extract the smallest
/// difference between the updated screen and apply those changes to
/// the render target, and then use `get_changes` to render those without
/// repainting the world on each update.
#[derive(Default, Clone)]
pub struct Surface {
    width: usize,
    height: usize,
    lines: Vec<Line>,
    attributes: CellAttributes,
    xpos: usize,
    ypos: usize,
    seqno: SequenceNo,
    changes: Vec<Change>,
    cursor_shape: Option<CursorShape>,
    cursor_visibility: CursorVisibility,
    cursor_color: ColorAttribute,
    title: String,
}

#[derive(Default)]
struct DiffState {
    changes: Vec<Change>,
    /// Keep track of the cursor position that the change stream
    /// selects for updates so that we can avoid emitting redundant
    /// position changes.
    cursor: Option<(usize, usize)>,
    /// Similarly, we keep track of the cell attributes that we have
    /// activated for change stream to avoid over-emitting.
    /// Tracking the cursor and attributes in this way helps to coalesce
    /// lines of text into simpler strings.
    attr: Option<CellAttributes>,
}

impl DiffState {
    #[inline]
    fn diff_cells(&mut self, col_num: usize, row_num: usize, cell: CellRef, other_cell: CellRef) {
        if cell.same_contents(&other_cell) {
            return;
        }

        self.set_cell(col_num, row_num, other_cell);
    }

    #[inline]
    fn set_cell(&mut self, col_num: usize, row_num: usize, other_cell: CellRef) {
        self.cursor = match self.cursor.take() {
            Some((cursor_row, cursor_col)) if cursor_row == row_num && cursor_col == col_num => {
                // It is on the current column, so we don't need
                // to explicitly move it.  Move the cursor by the
                // width of the text we're about to add.
                Some((row_num, col_num + other_cell.width()))
            },
            _ => {
                // Need to explicitly move the cursor
                self.changes.push(Change::CursorPosition {
                    y: Position::Absolute(row_num),
                    x: Position::Absolute(col_num),
                });
                // and update the position for next time
                Some((row_num, col_num + other_cell.width()))
            },
        };

        // we could get fancy and try to minimize the update traffic
        // by computing a series of AttributeChange values here.
        // For now, let's just record the new value
        self.attr = match self.attr.take() {
            Some(ref attr) if attr == other_cell.attrs() => {
                // Active attributes match, so we don't need
                // to emit a change for them
                Some(attr.clone())
            },
            _ => {
                // Attributes are different
                self.changes
                    .push(Change::AllAttributes(other_cell.attrs().clone()));
                Some(other_cell.attrs().clone())
            },
        };
        // A little bit of bloat in the code to avoid runs of single
        // character Text entries; just append to the string.
        let result_len = self.changes.len();
        if result_len > 0 && self.changes[result_len - 1].is_text() {
            if let Some(Change::Text(ref mut prefix)) = self.changes.get_mut(result_len - 1) {
                prefix.push_str(other_cell.str());
            }
        } else {
            self.changes
                .push(Change::Text(other_cell.str().to_string()));
        }
    }
}

impl Surface {
    /// Create a new Surface with the specified width and height.
    pub fn new(width: usize, height: usize) -> Self {
        let mut scr = Surface {
            width,
            height,
            ..Default::default()
        };
        scr.resize(width, height);
        scr
    }

    /// Returns the (width, height) of the surface
    pub fn dimensions(&self) -> (usize, usize) {
        (self.width, self.height)
    }

    pub fn cursor_position(&self) -> (usize, usize) {
        (self.xpos, self.ypos)
    }

    pub fn cursor_shape(&self) -> Option<CursorShape> {
        self.cursor_shape
    }

    pub fn cursor_visibility(&self) -> CursorVisibility {
        self.cursor_visibility
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    /// Resize the Surface to the specified width and height.
    /// If the width and/or height are smaller than previously, the rows and/or
    /// columns are truncated.  If the width and/or height are larger than
    /// previously then an appropriate number of cells are added to the
    /// buffer and filled with default attributes.
    /// The resize event invalidates the change stream, discarding it and
    /// causing a subsequent `get_changes` call to yield a full repaint.
    /// If the cursor position would be outside the bounds of the newly resized
    /// screen, it will be moved to be within the new bounds.
    pub fn resize(&mut self, width: usize, height: usize) {
        // We need to invalidate the change stream prior to this
        // event, so we nominally generate an entry for the resize
        // here.  Since rendering a resize doesn't make sense, we
        // don't record a Change entry.  Instead what we do is
        // increment the sequence number and then flush the whole
        // stream.  The next call to get_changes() will perform a
        // full repaint, and that is what we want.
        // We only do this if we have any changes buffered.
        if !self.changes.is_empty() {
            self.seqno += 1;
            self.changes.clear();
        }

        self.lines
            .resize(height, Line::with_width(width, self.seqno));
        for line in &mut self.lines {
            line.resize(width, self.seqno);
        }
        self.width = width;
        self.height = height;

        // Ensure that the cursor position is well-defined
        self.xpos = compute_position_change(self.xpos, &Position::Relative(0), self.width);
        self.ypos = compute_position_change(self.ypos, &Position::Relative(0), self.height);
    }

    /// Efficiently apply a series of changes
    /// Returns the sequence number at the end of the change.
    pub fn add_changes(&mut self, mut changes: Vec<Change>) -> SequenceNo {
        let seq = self.seqno.saturating_sub(1) + changes.len();

        for change in &changes {
            self.apply_change(&change);
        }

        self.seqno += changes.len();
        self.changes.append(&mut changes);

        seq
    }

    /// Apply a change and return the sequence number at the end of the change.
    pub fn add_change<C: Into<Change>>(&mut self, change: C) -> SequenceNo {
        let seq = self.seqno;
        self.seqno += 1;
        let change = change.into();
        self.apply_change(&change);
        self.changes.push(change);
        seq
    }

    fn apply_change(&mut self, change: &Change) {
        match change {
            Change::AllAttributes(attr) => self.attributes = attr.clone(),
            Change::Text(text) => self.print_text(text),
            Change::Attribute(change) => self.attributes.apply_change(change),
            Change::CursorPosition { x, y } => self.set_cursor_pos(x, y),
            Change::ClearScreen(color) => self.clear_screen(*color),
            Change::ClearToEndOfLine(color) => self.clear_eol(*color),
            Change::ClearToEndOfScreen(color) => self.clear_eos(*color),
            Change::CursorColor(color) => self.cursor_color = *color,
            Change::CursorShape(shape) => self.cursor_shape = Some(*shape),
            Change::CursorVisibility(visibility) => self.cursor_visibility = *visibility,
            Change::Image(image) => self.add_image(image),
            Change::Title(text) => self.title = text.to_owned(),
            Change::ScrollRegionUp {
                first_row,
                region_size,
                scroll_count,
            } => self.scroll_region_up(*first_row, *region_size, *scroll_count),
            Change::ScrollRegionDown {
                first_row,
                region_size,
                scroll_count,
            } => self.scroll_region_down(*first_row, *region_size, *scroll_count),
            Change::LineAttribute(attr) => self.line_attribute(attr),
        }
    }

    fn add_image(&mut self, image: &Image) {
        let xsize = (image.bottom_right.x - image.top_left.x) / image.width as f32;
        let ysize = (image.bottom_right.y - image.top_left.y) / image.height as f32;

        if self.ypos + image.height > self.height {
            let scroll = (self.ypos + image.height) - self.height;
            for _ in 0..scroll {
                self.scroll_screen_up();
            }
            self.ypos -= scroll;
        }

        let mut ypos = NotNan::new(0.0).unwrap();
        for y in 0..image.height {
            let mut xpos = NotNan::new(0.0).unwrap();
            for x in 0..image.width {
                self.lines[self.ypos + y].set_cell(
                    self.xpos + x,
                    Cell::new(
                        ' ',
                        self.attributes
                            .clone()
                            .set_image(Box::new(ImageCell::new(
                                TextureCoordinate::new(
                                    image.top_left.x + xpos,
                                    image.top_left.y + ypos,
                                ),
                                TextureCoordinate::new(
                                    image.top_left.x + xpos + xsize,
                                    image.top_left.y + ypos + ysize,
                                ),
                                image.image.clone(),
                            )))
                            .clone(),
                    ),
                    self.seqno,
                );

                xpos += xsize;
            }
            ypos += ysize;
        }

        self.xpos += image.width;
    }

    fn clear_screen(&mut self, color: ColorAttribute) {
        self.attributes = CellAttributes::default().set_background(color).clone();
        let cleared = Cell::new(' ', self.attributes.clone());
        for line in &mut self.lines {
            line.fill_range(0..self.width, &cleared, self.seqno);
        }
        self.xpos = 0;
        self.ypos = 0;
    }

    fn clear_eos(&mut self, color: ColorAttribute) {
        self.attributes = CellAttributes::default().set_background(color).clone();
        let cleared = Cell::new(' ', self.attributes.clone());
        self.lines[self.ypos].fill_range(self.xpos..self.width, &cleared, self.seqno);
        for line in &mut self.lines.iter_mut().skip(self.ypos + 1) {
            line.fill_range(0..self.width, &cleared, self.seqno);
        }
    }

    fn clear_eol(&mut self, color: ColorAttribute) {
        self.attributes = CellAttributes::default().set_background(color).clone();
        let cleared = Cell::new(' ', self.attributes.clone());
        self.lines[self.ypos].fill_range(self.xpos..self.width, &cleared, self.seqno);
    }

    fn scroll_screen_up(&mut self) {
        self.lines.remove(0);
        self.lines.push(Line::with_width(self.width, self.seqno));
    }

    fn scroll_region_up(&mut self, start: usize, size: usize, count: usize) {
        // Replace the first lines with empty lines
        for index in start..start + min(count, size) {
            self.lines[index] = Line::with_width(self.width, self.seqno);
        }
        // Rotate the remaining lines up the surface.
        if 0 < count && count < size {
            self.lines[start..start + size].rotate_left(count);
        }
    }

    fn scroll_region_down(&mut self, start: usize, size: usize, count: usize) {
        // Replace the last lines with empty lines
        for index in start + size - min(count, size)..start + size {
            self.lines[index] = Line::with_width(self.width, self.seqno);
        }
        // Rotate the remaining lines down the surface.
        if 0 < count && count < size {
            self.lines[start..start + size].rotate_right(count);
        }
    }

    fn line_attribute(&mut self, attr: &LineAttribute) {
        let line = &mut self.lines[self.ypos];
        match attr {
            LineAttribute::DoubleHeightTopHalfLine => line.set_double_height_top(self.seqno),
            LineAttribute::DoubleHeightBottomHalfLine => line.set_double_height_bottom(self.seqno),
            LineAttribute::DoubleWidthLine => line.set_double_width(self.seqno),
            LineAttribute::SingleWidthLine => line.set_single_width(self.seqno),
        }
    }

    fn print_text(&mut self, text: &str) {
        for g in Graphemes::new(text) {
            if g == "\r\n" {
                self.xpos = 0;
                let new_y = self.ypos + 1;
                if new_y >= self.height {
                    self.scroll_screen_up();
                } else {
                    self.ypos = new_y;
                }
                continue;
            }

            if g == "\r" {
                self.xpos = 0;
                continue;
            }

            if g == "\n" {
                let new_y = self.ypos + 1;
                if new_y >= self.height {
                    self.scroll_screen_up();
                } else {
                    self.ypos = new_y;
                }
                continue;
            }

            if self.xpos >= self.width {
                let new_y = self.ypos + 1;
                if new_y >= self.height {
                    self.scroll_screen_up();
                } else {
                    self.ypos = new_y;
                }
                self.xpos = 0;
            }

            let cell = Cell::new_grapheme(g, self.attributes.clone(), None);
            // the max(1) here is to ensure that we advance to the next cell
            // position for zero-width graphemes.  We want to make sure that
            // they occupy a cell so that we can re-emit them when we output them.
            // If we didn't do this, then we'd effectively filter them out from
            // the model, which seems like a lossy design choice.
            let width = cell.width().max(1);

            self.lines[self.ypos].set_cell(self.xpos, cell, self.seqno);

            // Increment the position now; we'll defer processing
            // wrapping until the next printed character, otherwise
            // we'll eagerly scroll when we reach the right margin.
            self.xpos += width;
        }
    }

    fn set_cursor_pos(&mut self, x: &Position, y: &Position) {
        self.xpos = compute_position_change(self.xpos, x, self.width);
        self.ypos = compute_position_change(self.ypos, y, self.height);
    }

    /// Returns the entire contents of the screen as a string.
    /// Only the character data is returned.  The end of each line is
    /// returned as a \n character.
    /// This function exists primarily for testing purposes.
    pub fn screen_chars_to_string(&self) -> String {
        let mut s = String::new();

        for line in &self.lines {
            for cell in line.visible_cells() {
                s.push_str(cell.str());
            }
            s.push('\n');
        }

        s
    }

    /// Returns the cell data for the screen.
    /// This is intended to be used for testing purposes.
    pub fn screen_cells(&mut self) -> Vec<&mut [Cell]> {
        let mut lines = Vec::new();
        for line in &mut self.lines {
            lines.push(line.cells_mut());
        }
        lines
    }

    pub fn screen_lines(&self) -> Vec<Cow<Line>> {
        self.lines.iter().map(|line| Cow::Borrowed(line)).collect()
    }

    /// Returns a stream of changes suitable to update the screen
    /// to match the model.  The input `seq` argument should be 0
    /// on the first call, or in any situation where the screen
    /// contents may have been invalidated, otherwise it should
    /// be set to the `SequenceNo` returned by the most recent call
    /// to `get_changes`.
    /// `get_changes` will use a heuristic to decide on the lower
    /// cost approach to updating the screen and return some sequence
    /// of `Change` entries that will update the display accordingly.
    /// The worst case is that this function will fabricate a sequence
    /// of Change entries to paint the screen from scratch.
    pub fn get_changes(&self, seq: SequenceNo) -> (SequenceNo, Cow<[Change]>) {
        // Do we have continuity in the sequence numbering?
        let first = self.seqno.saturating_sub(self.changes.len());
        if seq == 0 || first > seq || self.seqno == 0 {
            // No, we have folded away some data, we'll need a full paint
            return (self.seqno, Cow::Owned(self.repaint_all()));
        }

        // Approximate cost to render the change screen
        let delta_cost = self.seqno - seq;
        // Approximate cost to repaint from scratch
        let full_cost = self.estimate_full_paint_cost();

        if delta_cost > full_cost {
            (self.seqno, Cow::Owned(self.repaint_all()))
        } else {
            (self.seqno, Cow::Borrowed(&self.changes[seq - first..]))
        }
    }

    pub fn has_changes(&self, seq: SequenceNo) -> bool {
        self.seqno != seq
    }

    pub fn current_seqno(&self) -> SequenceNo {
        self.seqno
    }

    /// After having called `get_changes` and processed the resultant
    /// change stream, the caller can then pass the returned `SequenceNo`
    /// value to this call to prune the list of changes and free up
    /// resources from the change log.
    pub fn flush_changes_older_than(&mut self, seq: SequenceNo) {
        let first = self.seqno.saturating_sub(self.changes.len());
        let idx = seq.saturating_sub(first);
        if idx > self.changes.len() {
            return;
        }
        self.changes = self.changes.split_off(idx);
    }

    /// Without allocating resources, estimate how many Change entries
    /// we would produce in repaint_all for the current state.
    fn estimate_full_paint_cost(&self) -> usize {
        // assume 1 per cell with 20% overhead for attribute changes
        3 + (((self.width * self.height) as f64) * 1.2) as usize
    }

    fn repaint_all(&self) -> Vec<Change> {
        let mut result = vec![
            // Home the cursor and clear the screen to defaults.  Hide the
            // cursor while we're repainting.
            Change::CursorVisibility(CursorVisibility::Hidden),
            Change::ClearScreen(Default::default()),
        ];

        if !self.title.is_empty() {
            result.push(Change::Title(self.title.to_owned()));
        }

        let mut attr = CellAttributes::default();

        let crlf = Change::CursorPosition {
            x: Position::Absolute(0),
            y: Position::Relative(1),
        };

        // Walk backwards through the lines; the goal is to determine
        // if the screen ends with a number of clear lines that we
        // can coalesce together as a ClearToEndOfScreen op.
        // We track the index (from the end) of the last matching
        // run, together with the color of that run.
        let mut trailing_color = None;
        let mut trailing_idx = None;

        for (idx, line) in self.lines.iter().rev().enumerate() {
            let changes = line.changes(&attr);
            if changes.is_empty() {
                // The line recorded no changes; this means that the line
                // consists of spaces and the default background color
                match trailing_color {
                    Some(other) if other != Default::default() => {
                        // Color doesn't match up, so we have to stop
                        // looking for the ClearToEndOfScreen run here
                        break;
                    },
                    // Color does match
                    Some(_) => continue,
                    // we don't have a run, we should start one
                    None => {
                        trailing_color = Some(Default::default());
                        trailing_idx = Some(idx);
                        continue;
                    },
                }
            } else {
                let last_change = changes.len() - 1;
                match (&changes[last_change], trailing_color) {
                    (&Change::ClearToEndOfLine(ref color), None) => {
                        trailing_color = Some(*color);
                        trailing_idx = Some(idx);
                    },
                    (&Change::ClearToEndOfLine(ref color), Some(other)) => {
                        if other == *color {
                            trailing_idx = Some(idx);
                            continue;
                        } else {
                            break;
                        }
                    },
                    _ => break,
                }
            }
        }

        for (idx, line) in self.lines.iter().enumerate() {
            match trailing_idx {
                Some(t) if self.height - t == idx => {
                    let color =
                        trailing_color.expect("didn't set trailing_color along with trailing_idx");

                    // The first in the sequence of the ClearToEndOfLine may
                    // be batched up here; let's remove it if that is the case.
                    let last_result = result.len() - 1;
                    match result[last_result] {
                        Change::ClearToEndOfLine(col) if col == color => {
                            result.remove(last_result);
                        },
                        _ => {},
                    }

                    result.push(Change::ClearToEndOfScreen(color));
                    break;
                },
                _ => {},
            }

            let mut changes = line.changes(&attr);

            if idx != 0 {
                // We emit a relative move at the end of each
                // line with the theory that this will translate
                // to a short \r\n sequence rather than the longer
                // absolute cursor positioning sequence
                result.push(crlf.clone());
            }

            result.append(&mut changes);
            if let Some(c) = line.visible_cells().last() {
                attr = c.attrs().clone();
            }
        }

        // Remove any trailing sequence of cursor movements, as we're
        // going to just finish up with an absolute move anyway.
        loop {
            let result_len = result.len();
            if result_len == 0 {
                break;
            }
            match result[result_len - 1] {
                Change::CursorPosition { .. } => {
                    result.remove(result_len - 1);
                },
                _ => break,
            }
        }

        // Place the cursor at its intended position, but only if we moved the
        // cursor.  We don't explicitly track movement but can infer it from the
        // size of the results: results will have an initial ClearScreen entry
        // that homes the cursor and a CursorShape entry that hides the cursor.
        // If the screen is otherwise blank there will be no further entries
        // and we don't need to emit cursor movement.  However, in the
        // optimization passes above, we may have removed some number of
        // movement entries, so let's be sure to check the cursor position to
        // make sure that we don't fail to emit movement.

        let moved_cursor = result.len() != 2;
        if moved_cursor || self.xpos != 0 || self.ypos != 0 {
            result.push(Change::CursorPosition {
                x: Position::Absolute(self.xpos),
                y: Position::Absolute(self.ypos),
            });
        }

        // Set the intended cursor shape.  We hid the cursor at the start
        // of the repaint, so no need to hide it again.
        if self.cursor_visibility != CursorVisibility::Hidden {
            result.push(Change::CursorVisibility(CursorVisibility::Visible));
            if let Some(shape) = self.cursor_shape {
                result.push(Change::CursorShape(shape));
            }
        }

        result
    }

    /// Computes the change stream required to make the region within `self`
    /// at coordinates `x`, `y` and size `width`, `height` look like the
    /// same sized region within `other` at coordinates `other_x`, `other_y`.
    ///
    /// `other` and `self` may be the same, causing regions within the same
    /// `Surface` to be differenced; this is used by the `copy_region` method.
    ///
    /// The returned list of `Change`s can be passed to the `add_changes` method
    /// to make the region within self match the region within other.
    #[allow(clippy::too_many_arguments)]
    pub fn diff_region(
        &self,
        x: usize,
        y: usize,
        width: usize,
        height: usize,
        other: &Surface,
        other_x: usize,
        other_y: usize,
    ) -> Vec<Change> {
        let mut diff_state = DiffState::default();

        for ((row_num, line), other_line) in self
            .lines
            .iter()
            .enumerate()
            .skip(y)
            .take_while(|(row_num, _)| *row_num < y + height)
            .zip(other.lines.iter().skip(other_y))
        {
            diff_line(
                &mut diff_state,
                line,
                row_num,
                other_line,
                x,
                width,
                other_x,
            );
        }

        diff_state.changes
    }

    pub fn diff_lines(&self, other_lines: Vec<&Line>) -> Vec<Change> {
        let mut diff_state = DiffState::default();
        for ((row_num, line), other_line) in self.lines.iter().enumerate().zip(other_lines.iter()) {
            diff_line(&mut diff_state, line, row_num, other_line, 0, line.len(), 0);
        }
        diff_state.changes
    }

    pub fn diff_against_numbered_line(&self, row_num: usize, other_line: &Line) -> Vec<Change> {
        let mut diff_state = DiffState::default();
        if let Some(line) = self.lines.get(row_num) {
            diff_line(&mut diff_state, line, row_num, other_line, 0, line.len(), 0);
        }
        diff_state.changes
    }

    /// Computes the change stream required to make `self` have the same
    /// screen contents as `other`.
    pub fn diff_screens(&self, other: &Surface) -> Vec<Change> {
        self.diff_region(0, 0, self.width, self.height, other, 0, 0)
    }

    /// Draw the contents of `other` into self at the specified coordinates.
    /// The required updates are recorded as Change entries as well as stored
    /// in the screen line/cell data.
    /// Saves the cursor position and attributes that were in effect prior to
    /// calling `draw_from_screen` and restores them after applying the changes
    /// from the other surface.
    pub fn draw_from_screen(&mut self, other: &Surface, x: usize, y: usize) -> SequenceNo {
        let attrs = self.attributes.clone();
        let cursor = (self.xpos, self.ypos);
        let changes = self.diff_region(x, y, other.width, other.height, other, 0, 0);
        let seq = self.add_changes(changes);
        self.xpos = cursor.0;
        self.ypos = cursor.1;
        self.attributes = attrs;
        seq
    }

    /// Copy the contents of the specified region to the same sized
    /// region elsewhere in the screen display.
    /// The regions may overlap.
    /// # Panics
    /// The destination region must be the same size as the source
    /// (which is implied by the function parameters) and must fit
    /// within the width and height of the Surface or this operation
    /// will panic.
    pub fn copy_region(
        &mut self,
        src_x: usize,
        src_y: usize,
        width: usize,
        height: usize,
        dest_x: usize,
        dest_y: usize,
    ) -> SequenceNo {
        let changes = self.diff_region(dest_x, dest_y, width, height, self, src_x, src_y);
        self.add_changes(changes)
    }
}

/// Populate `diff_state` with changes to replace contents of `line` in range [x,x+width)
/// with the contents of `other_line` in range [other_x,other_x+width).
fn diff_line(
    diff_state: &mut DiffState,
    line: &Line,
    row_num: usize,
    other_line: &Line,
    x: usize,
    width: usize,
    other_x: usize,
) {
    let mut cells = line
        .visible_cells()
        .skip_while(|cell| cell.cell_index() < x)
        .take_while(|cell| cell.cell_index() < x + width)
        .peekable();
    let other_cells = other_line
        .visible_cells()
        .skip_while(|cell| cell.cell_index() < other_x)
        .take_while(|cell| cell.cell_index() < other_x + width);

    for other_cell in other_cells {
        let rel_x = other_cell.cell_index() - other_x;
        let mut comparison_cell = None;

        // Advance the `cells` iterator to try to find the visible cell in `line` in the equivalent
        // position to `other_cell`. If there is no visible cell in equivalent position, advance
        // one past and wait for next iteration.
        while let Some(cell) = cells.peek() {
            let cell_rel_x = cell.cell_index() - x;

            if cell_rel_x == rel_x {
                comparison_cell = Some(*cell);
                break;
            } else if cell_rel_x > rel_x {
                break;
            }

            cells.next();
        }

        // If we find a cell in the equivalent position, diff against it. If not, we know
        // there is a multi-cell grapheme in `line` that partially overlaps `other_cell`,
        // so we have to overwrite anyway.
        if let Some(comparison_cell) = comparison_cell {
            diff_state.diff_cells(x + rel_x, row_num, comparison_cell, other_cell);
        } else {
            diff_state.set_cell(x + rel_x, row_num, other_cell);
        }
    }
}

/// Applies a Position update to either the x or y position.
/// The value is clamped to be in the range: 0..limit
fn compute_position_change(current: usize, pos: &Position, limit: usize) -> usize {
    use self::Position::*;
    match pos {
        Relative(delta) => {
            if *delta >= 0 {
                min(
                    current.saturating_add(*delta as usize),
                    limit.saturating_sub(1),
                )
            } else {
                current.saturating_sub((*delta).abs() as usize)
            }
        },
        Absolute(abs) => min(*abs, limit.saturating_sub(1)),
        EndRelative(delta) => limit.saturating_sub(*delta),
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::vendored::termwiz::cell::{AttributeChange, Intensity};
    use crate::vendored::termwiz::color::AnsiColor;
    use crate::vendored::termwiz::image::ImageData;
    use std::sync::Arc;

    // The \x20's look a little awkward, but we can't use a plain
    // space in the first chararcter of a multi-line continuation;
    // it gets eaten up and ignored.

    #[test]
    fn basic_print() {
        let mut s = Surface::new(4, 3);
        assert_eq!(
            s.screen_chars_to_string(),
            "\x20\x20\x20\x20\n\
             \x20\x20\x20\x20\n\
             \x20\x20\x20\x20\n"
        );

        s.add_change("w00t");
        assert_eq!(
            s.screen_chars_to_string(),
            "w00t\n\
             \x20\x20\x20\x20\n\
             \x20\x20\x20\x20\n"
        );

        s.add_change("foo");
        assert_eq!(
            s.screen_chars_to_string(),
            "w00t\n\
             foo\x20\n\
             \x20\x20\x20\x20\n"
        );

        s.add_change("baar");
        assert_eq!(
            s.screen_chars_to_string(),
            "w00t\n\
             foob\n\
             aar\x20\n"
        );

        s.add_change("baz");
        assert_eq!(
            s.screen_chars_to_string(),
            "foob\n\
             aarb\n\
             az\x20\x20\n"
        );
    }

    #[test]
    fn newline() {
        let mut s = Surface::new(4, 4);
        s.add_change("bloo\rwat\n hey\r\nho");
        assert_eq!(
            s.screen_chars_to_string(),
            "wato\n\
             \x20\x20\x20\x20\n\
             hey \n\
             ho  \n"
        );
    }

    #[test]
    fn clear_screen() {
        let mut s = Surface::new(2, 2);
        s.add_change("hello");
        assert_eq!(s.xpos, 1);
        assert_eq!(s.ypos, 1);
        s.add_change(Change::ClearScreen(Default::default()));
        assert_eq!(s.xpos, 0);
        assert_eq!(s.ypos, 0);
        assert_eq!(s.screen_chars_to_string(), "  \n  \n");
    }

    #[test]
    fn clear_eol() {
        let mut s = Surface::new(3, 3);
        s.add_change("helwowfoo");
        s.add_change(Change::ClearToEndOfLine(Default::default()));
        assert_eq!(s.screen_chars_to_string(), "hel\nwow\nfoo\n");
        s.add_change(Change::CursorPosition {
            x: Position::Absolute(0),
            y: Position::Absolute(0),
        });
        s.add_change(Change::ClearToEndOfLine(Default::default()));
        assert_eq!(s.screen_chars_to_string(), "   \nwow\nfoo\n");
        s.add_change(Change::CursorPosition {
            x: Position::Absolute(1),
            y: Position::Absolute(1),
        });
        s.add_change(Change::ClearToEndOfLine(Default::default()));
        assert_eq!(s.screen_chars_to_string(), "   \nw\nfoo\n");
    }

    #[test]
    fn clear_eos() {
        let mut s = Surface::new(3, 3);
        s.add_change("helwowfoo");
        s.add_change(Change::ClearToEndOfScreen(Default::default()));
        assert_eq!(s.screen_chars_to_string(), "hel\nwow\nfoo\n");
        s.add_change(Change::CursorPosition {
            x: Position::Absolute(1),
            y: Position::Absolute(1),
        });
        s.add_change(Change::ClearToEndOfScreen(Default::default()));
        assert_eq!(s.screen_chars_to_string(), "hel\nw\n   \n");

        let (_seq, changes) = s.get_changes(0);
        assert_eq!(
            &[
                Change::CursorVisibility(CursorVisibility::Hidden),
                Change::ClearScreen(Default::default()),
                Change::Text("hel".into()),
                Change::CursorPosition {
                    x: Position::Absolute(0),
                    y: Position::Relative(1),
                },
                Change::Text("w".into()),
                Change::CursorPosition {
                    x: Position::Absolute(1),
                    y: Position::Absolute(1),
                },
                Change::CursorVisibility(CursorVisibility::Visible),
            ],
            &*changes
        );
    }

    #[test]
    fn clear_eos_back_color() {
        let mut s = Surface::new(3, 3);
        s.add_change(Change::ClearScreen(AnsiColor::Red.into()));
        s.add_change("helwowfoo");
        assert_eq!(s.screen_chars_to_string(), "hel\nwow\nfoo\n");
        s.add_change(Change::CursorPosition {
            x: Position::Absolute(1),
            y: Position::Absolute(1),
        });
        s.add_change(Change::ClearToEndOfScreen(AnsiColor::Red.into()));
        assert_eq!(s.screen_chars_to_string(), "hel\nw  \n   \n");

        let (_seq, changes) = s.get_changes(0);
        assert_eq!(
            &[
                Change::CursorVisibility(CursorVisibility::Hidden),
                Change::ClearScreen(Default::default()),
                Change::AllAttributes(
                    CellAttributes::default()
                        .set_background(AnsiColor::Red)
                        .clone()
                ),
                Change::Text("hel".into()),
                Change::CursorPosition {
                    x: Position::Absolute(0),
                    y: Position::Relative(1),
                },
                Change::Text("w".into()),
                Change::ClearToEndOfScreen(AnsiColor::Red.into()),
                Change::CursorPosition {
                    x: Position::Absolute(1),
                    y: Position::Absolute(1),
                },
                Change::CursorVisibility(CursorVisibility::Visible),
            ],
            &*changes
        );
    }

    #[test]
    fn clear_eol_opt() {
        let mut s = Surface::new(3, 3);
        s.add_change(Change::Attribute(AttributeChange::Background(
            AnsiColor::Red.into(),
        )));
        s.add_change("111   333");
        let (_seq, changes) = s.get_changes(0);
        assert_eq!(
            &[
                Change::CursorVisibility(CursorVisibility::Hidden),
                Change::ClearScreen(Default::default()),
                Change::AllAttributes(
                    CellAttributes::default()
                        .set_background(AnsiColor::Red)
                        .clone()
                ),
                Change::Text("111".into()),
                Change::CursorPosition {
                    x: Position::Absolute(0),
                    y: Position::Relative(1),
                },
                Change::ClearToEndOfLine(AnsiColor::Red.into()),
                Change::CursorPosition {
                    x: Position::Absolute(0),
                    y: Position::Relative(1),
                },
                Change::Text("333".into()),
                Change::CursorPosition {
                    x: Position::Absolute(3),
                    y: Position::Absolute(2),
                },
                Change::CursorVisibility(CursorVisibility::Visible),
            ],
            &*changes
        );
    }

    #[test]
    fn clear_and_move_cursor() {
        let mut s = Surface::new(4, 3);
        s.add_change(Change::CursorPosition {
            x: Position::Absolute(3),
            y: Position::Absolute(2),
        });
        let (_seq, changes) = s.get_changes(0);
        assert_eq!(
            &[
                Change::CursorVisibility(CursorVisibility::Hidden),
                Change::ClearScreen(Default::default()),
                Change::CursorPosition {
                    x: Position::Absolute(3),
                    y: Position::Absolute(2),
                },
                Change::CursorVisibility(CursorVisibility::Visible),
            ],
            &*changes
        );
    }

    #[test]
    fn cursor_movement() {
        let mut s = Surface::new(4, 3);
        s.add_change(Change::CursorPosition {
            x: Position::Absolute(3),
            y: Position::Absolute(2),
        });
        s.add_change("X");
        assert_eq!(
            s.screen_chars_to_string(),
            "\x20\x20\x20\x20\n\
             \x20\x20\x20\x20\n\
             \x20\x20\x20X\n"
        );

        s.add_change(Change::CursorPosition {
            x: Position::Relative(-2),
            y: Position::Relative(-1),
        });
        s.add_change("-");
        assert_eq!(
            s.screen_chars_to_string(),
            "\x20\x20\x20\x20\n\
             \x20\x20-\x20\n\
             \x20\x20\x20X\n"
        );

        s.add_change(Change::CursorPosition {
            x: Position::Relative(1),
            y: Position::Relative(-1),
        });
        s.add_change("-");
        assert_eq!(
            s.screen_chars_to_string(),
            "\x20\x20\x20-\n\
             \x20\x20-\x20\n\
             \x20\x20\x20X\n"
        );
    }

    #[test]
    fn attribute_setting() {
        use crate::vendored::termwiz::cell::Intensity;

        let mut s = Surface::new(3, 1);
        s.add_change("n");
        s.add_change(AttributeChange::Intensity(Intensity::Bold));
        s.add_change("b");

        let mut bold = CellAttributes::default();
        bold.set_intensity(Intensity::Bold);

        assert_eq!(
            s.screen_cells(),
            [[
                Cell::new('n', CellAttributes::default()),
                Cell::new('b', bold),
                Cell::default(),
            ]]
        );
    }

    #[test]
    fn empty_changes() {
        let s = Surface::new(4, 3);

        let empty = &[
            Change::CursorVisibility(CursorVisibility::Hidden),
            Change::ClearScreen(Default::default()),
            Change::CursorVisibility(CursorVisibility::Visible),
        ];

        let (seq, changes) = s.get_changes(0);
        assert_eq!(seq, 0);
        assert_eq!(empty, &*changes);

        // Using an invalid sequence number should get us the full
        // repaint also.
        let (seq, changes) = s.get_changes(1);
        assert_eq!(seq, 0);
        assert_eq!(empty, &*changes);
    }

    #[test]
    fn add_changes_empty() {
        let mut s = Surface::new(2, 2);
        let last_seq = s.add_change("foo");
        assert_eq!(0, last_seq);
        assert_eq!(last_seq, s.add_changes(vec![]));
        assert_eq!(last_seq + 1, s.add_changes(vec![Change::Text("a".into())]));
    }

    #[test]
    fn resize_delta_flush() {
        let mut s = Surface::new(4, 3);
        s.add_change("a");
        let (seq, _) = s.get_changes(0);
        s.resize(2, 2);

        let full = &[
            Change::CursorVisibility(CursorVisibility::Hidden),
            Change::ClearScreen(Default::default()),
            Change::Text("a".to_string()),
            Change::CursorPosition {
                x: Position::Absolute(1),
                y: Position::Absolute(0),
            },
            Change::CursorVisibility(CursorVisibility::Visible),
        ];

        let (_seq, changes) = s.get_changes(seq);
        // The resize causes get_changes to return a full repaint
        assert_eq!(full, &*changes);
    }

    #[test]
    fn dont_lose_first_char_on_attr_change() {
        let mut s = Surface::new(2, 2);
        s.add_change(Change::Attribute(AttributeChange::Foreground(
            AnsiColor::Maroon.into(),
        )));
        s.add_change("ab");
        let (_seq, changes) = s.get_changes(0);
        assert_eq!(
            &[
                Change::CursorVisibility(CursorVisibility::Hidden),
                Change::ClearScreen(Default::default()),
                Change::AllAttributes(
                    CellAttributes::default()
                        .set_foreground(AnsiColor::Maroon)
                        .clone()
                ),
                Change::Text("ab".into()),
                Change::CursorPosition {
                    x: Position::Absolute(2),
                    y: Position::Absolute(0),
                },
                Change::CursorVisibility(CursorVisibility::Visible),
            ],
            &*changes
        );
    }

    #[test]
    fn resize_cursor_position() {
        let mut s = Surface::new(4, 4);

        s.add_change(" a");
        s.add_change(Change::CursorPosition {
            x: Position::Absolute(3),
            y: Position::Absolute(3),
        });

        assert_eq!(s.xpos, 3);
        assert_eq!(s.ypos, 3);
        s.resize(2, 2);
        assert_eq!(s.xpos, 1);
        assert_eq!(s.ypos, 1);

        let full = &[
            Change::CursorVisibility(CursorVisibility::Hidden),
            Change::ClearScreen(Default::default()),
            Change::Text(" a".to_string()),
            Change::CursorPosition {
                x: Position::Absolute(1),
                y: Position::Absolute(1),
            },
            Change::CursorVisibility(CursorVisibility::Visible),
        ];

        let (_seq, changes) = s.get_changes(0);
        assert_eq!(full, &*changes);
    }

    #[test]
    fn delta_change() {
        let mut s = Surface::new(4, 3);
        // flushing nothing should be a NOP
        s.flush_changes_older_than(0);

        // check that using an invalid index doesn't panic
        s.flush_changes_older_than(1);

        let initial = &[
            Change::CursorVisibility(CursorVisibility::Hidden),
            Change::ClearScreen(Default::default()),
            Change::Text("a".to_string()),
            Change::CursorPosition {
                x: Position::Absolute(1),
                y: Position::Absolute(0),
            },
            Change::CursorVisibility(CursorVisibility::Visible),
        ];

        let seq_pos = {
            let next_seq = s.add_change("a");
            let (seq, changes) = s.get_changes(0);
            assert_eq!(seq, next_seq + 1);
            assert_eq!(initial, &*changes);
            seq
        };

        let seq_pos = {
            let next_seq = s.add_change("b");
            let (seq, changes) = s.get_changes(seq_pos);
            assert_eq!(seq, next_seq + 1);
            assert_eq!(&[Change::Text("b".to_string())], &*changes);
            seq
        };

        // prep some deltas for the loop to test below
        {
            s.add_change(Change::Attribute(AttributeChange::Intensity(
                Intensity::Bold,
            )));
            s.add_change("c");
            s.add_change(Change::Attribute(AttributeChange::Intensity(
                Intensity::Normal,
            )));
            s.add_change("d");
        }

        // Do this three times to ennsure that the behavior is consistent
        // across multiple flush calls
        for _ in 0..3 {
            {
                let (_seq, changes) = s.get_changes(seq_pos);

                assert_eq!(
                    &[
                        Change::Attribute(AttributeChange::Intensity(Intensity::Bold)),
                        Change::Text("c".to_string()),
                        Change::Attribute(AttributeChange::Intensity(Intensity::Normal)),
                        Change::Text("d".to_string()),
                    ],
                    &*changes
                );
            }

            // Flush the changes so that the next iteration is run on a pruned
            // set of changes.  It should not change the outcome of the body
            // of the loop.
            s.flush_changes_older_than(seq_pos);
        }
    }

    #[test]
    fn diff_screens() {
        let mut s = Surface::new(4, 3);
        s.add_change("w00t");
        s.add_change("foo");
        s.add_change("baar");
        s.add_change("baz");
        assert_eq!(
            s.screen_chars_to_string(),
            "foob\n\
             aarb\n\
             az  \n"
        );

        let s2 = Surface::new(2, 2);

        {
            // We want to sample the top left corner
            let changes = s2.diff_region(0, 0, 2, 2, &s, 0, 0);
            assert_eq!(
                vec![
                    Change::CursorPosition {
                        x: Position::Absolute(0),
                        y: Position::Absolute(0),
                    },
                    Change::AllAttributes(CellAttributes::default()),
                    Change::Text("fo".into()),
                    Change::CursorPosition {
                        x: Position::Absolute(0),
                        y: Position::Absolute(1),
                    },
                    Change::Text("aa".into()),
                ],
                changes
            );
        }

        // Throw in some attribute changes too
        s.add_change(Change::CursorPosition {
            x: Position::Absolute(1),
            y: Position::Absolute(1),
        });
        s.add_change(Change::Attribute(AttributeChange::Intensity(
            Intensity::Bold,
        )));
        s.add_change("XO");

        {
            let changes = s2.diff_region(0, 0, 2, 2, &s, 1, 1);
            assert_eq!(
                vec![
                    Change::CursorPosition {
                        x: Position::Absolute(0),
                        y: Position::Absolute(0),
                    },
                    Change::AllAttributes(
                        CellAttributes::default()
                            .set_intensity(Intensity::Bold)
                            .clone(),
                    ),
                    Change::Text("XO".into()),
                    Change::CursorPosition {
                        x: Position::Absolute(0),
                        y: Position::Absolute(1),
                    },
                    Change::AllAttributes(CellAttributes::default()),
                    Change::Text("z".into()),
                    /* There's no change for the final character
                     * position because it is a space in both regions. */
                ],
                changes
            );
        }
    }

    #[test]
    fn draw_screens() {
        let mut s = Surface::new(4, 4);

        let mut s1 = Surface::new(2, 2);
        s1.add_change("1234");

        let mut s2 = Surface::new(2, 2);
        s2.add_change("XYZA");

        s.draw_from_screen(&s1, 0, 0);
        s.draw_from_screen(&s2, 2, 2);

        assert_eq!(
            s.screen_chars_to_string(),
            "12  \n\
             34  \n\
             \x20\x20XY\n\
             \x20\x20ZA\n"
        );
    }

    #[test]
    fn draw_colored_region() {
        let mut dest = Surface::new(4, 4);
        dest.add_change("A");
        let mut src = Surface::new(2, 2);
        src.add_change(Change::ClearScreen(AnsiColor::Blue.into()));
        dest.draw_from_screen(&src, 2, 2);

        assert_eq!(
            dest.screen_chars_to_string(),
            "A   \n\
             \x20   \n\
             \x20   \n\
             \x20   \n"
        );

        let blue_space = Cell::new(
            ' ',
            CellAttributes::default()
                .set_background(AnsiColor::Blue)
                .clone(),
        );

        assert_eq!(
            dest.screen_cells(),
            [
                [
                    Cell::new('A', CellAttributes::default()),
                    Cell::default(),
                    Cell::default(),
                    Cell::default(),
                ],
                [
                    Cell::default(),
                    Cell::default(),
                    Cell::default(),
                    Cell::default(),
                ],
                [
                    Cell::default(),
                    Cell::default(),
                    blue_space.clone(),
                    blue_space.clone(),
                ],
                [
                    Cell::default(),
                    Cell::default(),
                    blue_space.clone(),
                    blue_space.clone(),
                ]
            ]
        );

        assert_eq!(dest.xpos, 1);
        assert_eq!(dest.ypos, 0);
        assert_eq!(dest.attributes, Default::default());
        dest.add_change("B");

        assert_eq!(
            dest.screen_chars_to_string(),
            "AB  \n\
             \x20   \n\
             \x20   \n\
             \x20   \n"
        );
    }

    #[test]
    fn copy_region() {
        let mut s = Surface::new(4, 3);
        s.add_change("w00t");
        s.add_change("foo");
        s.add_change("baar");
        s.add_change("baz");
        assert_eq!(
            s.screen_chars_to_string(),
            "foob\n\
             aarb\n\
             az  \n"
        );

        // Copy top left to bottom left
        s.copy_region(0, 0, 2, 2, 2, 1);
        assert_eq!(
            s.screen_chars_to_string(),
            "foob\n\
             aafo\n\
             azaa\n"
        );
    }

    #[test]
    fn double_width() {
        let mut s = Surface::new(4, 1);
        s.add_change("🤷12");
        assert_eq!(s.screen_chars_to_string(), "🤷12\n");
        s.add_change(Change::CursorPosition {
            x: Position::Absolute(1),
            y: Position::Absolute(0),
        });
        s.add_change("a🤷");
        assert_eq!(s.screen_chars_to_string(), " a🤷\n");
        s.add_change(Change::CursorPosition {
            x: Position::Absolute(2),
            y: Position::Absolute(0),
        });
        s.add_change("x");
        assert_eq!(s.screen_chars_to_string(), " ax \n");
    }

    #[test]
    fn draw_double_width() {
        let mut s = Surface::new(4, 1);
        s.add_change("か a");
        assert_eq!(s.screen_chars_to_string(), "か a\n");

        let mut s2 = Surface::new(4, 1);
        s2.draw_from_screen(&s, 0, 0);
        // Verify no issue when the second visible cells on both sides
        // are identical (' 's) but they are at different cell indices.
        assert_eq!(s2.screen_chars_to_string(), "か a\n");

        let s3 = Surface::new(4, 1);
        s2.draw_from_screen(&s3, 0, 0);
        // Verify same but in other direction
        assert_eq!(s2.screen_chars_to_string(), "    \n");

        let mut s4 = Surface::new(4, 1);
        s4.add_change("abcd");
        s.draw_from_screen(&s4, 0, 0);
        // Verify that all overlapping cells are updated when cell widths
        // differ on each side.
        assert_eq!(s.screen_chars_to_string(), "abcd\n");
    }

    #[test]
    fn diff_cursor_double_width() {
        let mut s = Surface::new(3, 1);
        s.add_change("かa");

        let s2 = Surface::new(3, 1);
        let changes = s2.diff_region(0, 0, 3, 1, &s, 0, 0);

        assert_eq!(
            changes
                .iter()
                .filter(|change| matches!(change, Change::CursorPosition { .. }))
                .count(),
            1
        );
    }

    #[test]
    fn zero_width() {
        let mut s = Surface::new(4, 1);
        // https://en.wikipedia.org/wiki/Zero-width_space
        s.add_change("A\u{200b}B");
        assert_eq!(s.screen_chars_to_string(), "A\u{200b}B \n");
    }

    #[test]
    fn images() {
        // a dummy image blob with nonsense content
        let data = Arc::new(ImageData::with_raw_data(vec![]));
        let mut s = Surface::new(2, 2);
        s.add_change(Change::Image(Image {
            top_left: TextureCoordinate::new_f32(0.0, 0.0),
            bottom_right: TextureCoordinate::new_f32(1.0, 1.0),
            image: data.clone(),
            width: 4,
            height: 2,
        }));

        // We're checking that we slice the image up and assign the correct
        // texture coordinates for each cell.  The width and height are
        // different from each other to help ensure that the right terms
        // are used by add_image() function.
        assert_eq!(
            s.screen_cells(),
            [
                [
                    Cell::new(
                        ' ',
                        CellAttributes::default()
                            .set_image(Box::new(ImageCell::new(
                                TextureCoordinate::new_f32(0.0, 0.0),
                                TextureCoordinate::new_f32(0.25, 0.5),
                                data.clone()
                            )))
                            .clone()
                    ),
                    Cell::new(
                        ' ',
                        CellAttributes::default()
                            .set_image(Box::new(ImageCell::new(
                                TextureCoordinate::new_f32(0.25, 0.0),
                                TextureCoordinate::new_f32(0.5, 0.5),
                                data.clone()
                            )))
                            .clone()
                    ),
                    Cell::new(
                        ' ',
                        CellAttributes::default()
                            .set_image(Box::new(ImageCell::new(
                                TextureCoordinate::new_f32(0.5, 0.0),
                                TextureCoordinate::new_f32(0.75, 0.5),
                                data.clone()
                            )))
                            .clone()
                    ),
                    Cell::new(
                        ' ',
                        CellAttributes::default()
                            .set_image(Box::new(ImageCell::new(
                                TextureCoordinate::new_f32(0.75, 0.0),
                                TextureCoordinate::new_f32(1.0, 0.5),
                                data.clone()
                            )))
                            .clone()
                    ),
                ],
                [
                    Cell::new(
                        ' ',
                        CellAttributes::default()
                            .set_image(Box::new(ImageCell::new(
                                TextureCoordinate::new_f32(0.0, 0.5),
                                TextureCoordinate::new_f32(0.25, 1.0),
                                data.clone()
                            )))
                            .clone()
                    ),
                    Cell::new(
                        ' ',
                        CellAttributes::default()
                            .set_image(Box::new(ImageCell::new(
                                TextureCoordinate::new_f32(0.25, 0.5),
                                TextureCoordinate::new_f32(0.5, 1.0),
                                data.clone()
                            )))
                            .clone()
                    ),
                    Cell::new(
                        ' ',
                        CellAttributes::default()
                            .set_image(Box::new(ImageCell::new(
                                TextureCoordinate::new_f32(0.5, 0.5),
                                TextureCoordinate::new_f32(0.75, 1.0),
                                data.clone()
                            )))
                            .clone()
                    ),
                    Cell::new(
                        ' ',
                        CellAttributes::default()
                            .set_image(Box::new(ImageCell::new(
                                TextureCoordinate::new_f32(0.75, 0.5),
                                TextureCoordinate::new_f32(1.0, 1.0),
                                data.clone()
                            )))
                            .clone()
                    ),
                ],
            ]
        );

        // Check that starting at not the texture origin coordinates
        // gives reasonable values in the resultant cell
        let mut other = Surface::new(1, 1);
        other.add_change(Change::Image(Image {
            top_left: TextureCoordinate::new_f32(0.25, 0.3),
            bottom_right: TextureCoordinate::new_f32(0.75, 0.8),
            image: data.clone(),
            width: 1,
            height: 1,
        }));
        assert_eq!(
            other.screen_cells(),
            [[Cell::new(
                ' ',
                CellAttributes::default()
                    .set_image(Box::new(ImageCell::new(
                        TextureCoordinate::new_f32(0.25, 0.3),
                        TextureCoordinate::new_f32(0.75, 0.8),
                        data.clone()
                    )))
                    .clone()
            ),]]
        );
    }
}
