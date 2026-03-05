//! A Renderer for windows consoles

use crate::vendored::termwiz::caps::{Capabilities, ColorLevel};
use crate::vendored::termwiz::cell::{AttributeChange, CellAttributes, Underline};
use crate::vendored::termwiz::color::{AnsiColor, ColorAttribute};
use crate::vendored::termwiz::surface::{Change, Position};
use crate::vendored::termwiz::terminal::windows::ConsoleOutputHandle;
use crate::vendored::termwiz::Result;
use num_traits::FromPrimitive;
use std::io::Write;
use winapi::shared::minwindef::WORD;
use winapi::um::wincon::{
    BACKGROUND_BLUE, BACKGROUND_GREEN, BACKGROUND_INTENSITY, BACKGROUND_RED, CHAR_INFO,
    COMMON_LVB_REVERSE_VIDEO, COMMON_LVB_UNDERSCORE, FOREGROUND_BLUE, FOREGROUND_GREEN,
    FOREGROUND_INTENSITY, FOREGROUND_RED,
};

pub struct WindowsConsoleRenderer {
    pending_attr: CellAttributes,
    capabilities: Capabilities,
}

impl WindowsConsoleRenderer {
    pub fn new(capabilities: Capabilities) -> Self {
        Self {
            pending_attr: CellAttributes::default(),
            capabilities,
        }
    }
}

fn to_attr_word(capabilities: &Capabilities, attr: &CellAttributes) -> u16 {
    macro_rules! ansi_colors_impl {
        ($idx:expr, $default:ident,
                $red:ident, $green:ident, $blue:ident,
                $bright:ident, $( ($variant:ident, $bits:expr) ),*) =>{
            match FromPrimitive::from_u8($idx).unwrap_or(AnsiColor::$default) {
                $(
                    AnsiColor::$variant => $bits,
                )*
            }
        }
    }

    macro_rules! ansi_colors {
        ($idx:expr, $default:ident, $red:ident, $green:ident, $blue:ident, $bright:ident) => {
            ansi_colors_impl!(
                $idx,
                $default,
                $red,
                $green,
                $blue,
                $bright,
                (Black, 0),
                (Maroon, $red),
                (Green, $green),
                (Olive, $red | $green),
                (Navy, $blue),
                (Purple, $red | $blue),
                (Teal, $green | $blue),
                (Silver, $red | $green | $blue),
                (Grey, $bright),
                (Red, $bright | $red),
                (Lime, $bright | $green),
                (Yellow, $bright | $red | $green),
                (Blue, $bright | $blue),
                (Fuchsia, $bright | $red | $blue),
                (Aqua, $bright | $green | $blue),
                (White, $bright | $red | $green | $blue)
            )
        };
    }

    let reverse = if attr.reverse() {
        COMMON_LVB_REVERSE_VIDEO
    } else {
        0
    };
    let underline = if attr.underline() != Underline::None {
        COMMON_LVB_UNDERSCORE
    } else {
        0
    };

    if capabilities.color_level() == ColorLevel::MonoChrome {
        return reverse | underline;
    }

    let fg = match attr.foreground() {
        ColorAttribute::TrueColorWithDefaultFallback(_) | ColorAttribute::Default => {
            FOREGROUND_BLUE | FOREGROUND_RED | FOREGROUND_GREEN
        },

        ColorAttribute::TrueColorWithPaletteFallback(_, idx)
        | ColorAttribute::PaletteIndex(idx) => ansi_colors!(
            idx,
            White,
            FOREGROUND_RED,
            FOREGROUND_GREEN,
            FOREGROUND_BLUE,
            FOREGROUND_INTENSITY
        ),
    };

    let bg = match attr.background() {
        ColorAttribute::TrueColorWithDefaultFallback(_) | ColorAttribute::Default => 0,
        ColorAttribute::TrueColorWithPaletteFallback(_, idx)
        | ColorAttribute::PaletteIndex(idx) => ansi_colors!(
            idx,
            Black,
            BACKGROUND_RED,
            BACKGROUND_GREEN,
            BACKGROUND_BLUE,
            BACKGROUND_INTENSITY
        ),
    };

    bg | fg | reverse | underline
}

struct ScreenBuffer {
    buf: Vec<CHAR_INFO>,
    dirty: bool,
    rows: usize,
    cols: usize,
    cursor_x: usize,
    cursor_y: usize,
    pending_attr: WORD,
}

impl ScreenBuffer {
    fn cursor_idx(&self) -> usize {
        let idx = (self.cursor_y * self.cols) + self.cursor_x;
        assert!(
            idx < self.rows * self.cols,
            "idx={}, cursor:({},{}) rows={}, cols={}.",
            idx,
            self.cursor_x,
            self.cursor_y,
            self.rows,
            self.cols
        );
        idx
    }

    fn fill(&mut self, c: char, attr: WORD, x: usize, y: usize, num_elements: usize) -> usize {
        let idx = (y * self.cols) + x;
        let max = self.rows * self.cols;

        let end = (idx + num_elements).min(max);
        let c = c as u16;
        for cell in &mut self.buf[idx..end] {
            cell.Attributes = attr;
            unsafe {
                *cell.Char.UnicodeChar_mut() = c;
            }
        }
        self.dirty = true;
        end
    }

    fn do_cursor_y_scroll<B: ConsoleOutputHandle + Write>(&mut self, out: &mut B) -> Result<()> {
        if self.cursor_y >= self.rows {
            self.dirty = true;
            let lines_to_scroll = self.cursor_y.saturating_sub(self.rows) + 1;
            self.scroll(0, self.rows, -1 * lines_to_scroll as isize, out)?;
            self.dirty = true;
            self.cursor_y -= lines_to_scroll;
            assert!(self.cursor_y < self.rows);
        }
        Ok(())
    }

    fn set_cursor<B: ConsoleOutputHandle + Write>(
        &mut self,
        x: usize,
        y: usize,
        out: &mut B,
    ) -> Result<()> {
        self.cursor_x = x;
        self.cursor_y = y;

        self.do_cursor_y_scroll(out)?;

        // Make sure we mark dirty after we've scrolled!
        self.dirty = true;
        assert!(self.cursor_x < self.cols);
        assert!(self.cursor_y < self.rows);
        Ok(())
    }

    fn write_text<B: ConsoleOutputHandle + Write>(
        &mut self,
        t: &str,
        attr: WORD,
        out: &mut B,
    ) -> Result<()> {
        for c in t.chars() {
            match c {
                '\r' => {
                    self.cursor_x = 0;
                },
                '\n' => {
                    self.cursor_y += 1;
                    self.do_cursor_y_scroll(out)?;
                },
                c => {
                    if self.cursor_x == self.cols {
                        self.cursor_y += 1;
                        self.cursor_x = 0;
                    }
                    self.do_cursor_y_scroll(out)?;

                    let idx = self.cursor_idx();

                    let cell = &mut self.buf[idx];
                    cell.Attributes = attr;
                    unsafe {
                        *cell.Char.UnicodeChar_mut() = c as u16;
                    }
                    self.cursor_x += 1;
                    self.dirty = true;
                },
            }
        }
        Ok(())
    }

    fn flush<B: ConsoleOutputHandle + Write>(&mut self, out: &mut B) -> Result<()> {
        self.flush_screen(out)?;
        let info = out.get_buffer_info()?;
        out.set_cursor_position(
            self.cursor_x as i16,
            self.cursor_y as i16 + info.srWindow.Top,
        )?;
        out.set_attr(self.pending_attr)?;
        out.flush()?;
        Ok(())
    }

    fn flush_screen<B: ConsoleOutputHandle + Write>(&mut self, out: &mut B) -> Result<()> {
        if self.dirty {
            out.flush()?;
            out.set_buffer_contents(&self.buf)?;
            out.flush()?;
            self.dirty = false;
        }
        Ok(())
    }

    fn reread_buffer<B: ConsoleOutputHandle + Write>(&mut self, out: &mut B) -> Result<()> {
        self.buf = out.get_buffer_contents()?;
        self.dirty = false;
        Ok(())
    }

    fn scroll<B: ConsoleOutputHandle + Write>(
        &mut self,
        first_row: usize,
        region_size: usize,
        scroll_count: isize,
        out: &mut B,
    ) -> Result<()> {
        if region_size > 0 && scroll_count != 0 {
            self.flush_screen(out)?;
            let info = out.get_buffer_info()?;

            // Scroll the full width of the window, always.
            let left = 0;
            let right = info.dwSize.X - 1;

            // We're only doing vertical scrolling
            let dx = 0;
            let dy = scroll_count as i16;

            if first_row == 0 && region_size == self.rows {
                // We're scrolling the whole screen, so let it scroll
                // up into the scrollback
                out.set_viewport(
                    info.srWindow.Left,
                    info.srWindow.Top - dy,
                    info.srWindow.Right,
                    info.srWindow.Bottom - dy,
                )?;
            } else {
                // We're just scrolling a region within the window
                let top = info.srWindow.Top + first_row as i16;
                let bottom = top + region_size.saturating_sub(1) as i16;
                out.scroll_region(left, top, right, bottom, dx, dy, self.pending_attr)?;
            }

            self.reread_buffer(out)?;
        }
        Ok(())
    }
}

impl WindowsConsoleRenderer {
    pub fn render_to<B: ConsoleOutputHandle + Write>(
        &mut self,
        changes: &[Change],
        out: &mut B,
    ) -> Result<()> {
        out.flush()?;
        let info = out.get_buffer_info()?;

        let cols = info.dwSize.X as usize;
        let rows = 1 + info.srWindow.Bottom as usize - info.srWindow.Top as usize;

        let mut buffer = ScreenBuffer {
            buf: out.get_buffer_contents()?,
            cursor_x: info.dwCursorPosition.X as usize,
            cursor_y: (info.dwCursorPosition.Y as usize).saturating_sub(info.srWindow.Top as usize),
            dirty: false,
            rows,
            cols,
            pending_attr: to_attr_word(&self.capabilities, &CellAttributes::default()),
        };

        for change in changes {
            match change {
                Change::ClearScreen(color) => {
                    let attr = CellAttributes::default()
                        .set_background(color.clone())
                        .clone();

                    buffer.fill(
                        ' ',
                        to_attr_word(&self.capabilities, &attr),
                        0,
                        0,
                        cols * rows,
                    );
                    buffer.set_cursor(0, 0, out)?;
                },
                Change::ClearToEndOfLine(color) => {
                    let attr = CellAttributes::default()
                        .set_background(color.clone())
                        .clone();

                    buffer.fill(
                        ' ',
                        to_attr_word(&self.capabilities, &attr),
                        buffer.cursor_x,
                        buffer.cursor_y,
                        cols.saturating_sub(buffer.cursor_x),
                    );
                },
                Change::ClearToEndOfScreen(color) => {
                    let attr = CellAttributes::default()
                        .set_background(color.clone())
                        .clone();

                    buffer.fill(
                        ' ',
                        to_attr_word(&self.capabilities, &attr),
                        buffer.cursor_x,
                        buffer.cursor_y,
                        cols * rows,
                    );
                },
                Change::Text(text) => {
                    buffer.write_text(
                        &text,
                        to_attr_word(&self.capabilities, &self.pending_attr),
                        out,
                    )?;
                },
                Change::CursorPosition { x, y } => {
                    let x = match x {
                        Position::Absolute(x) => *x as usize,
                        Position::Relative(delta) => {
                            (buffer.cursor_x as isize).saturating_sub(-*delta) as usize
                        },
                        Position::EndRelative(delta) => cols.saturating_sub(*delta),
                    };

                    // For vertical cursor movement, we constrain the movement to
                    // the viewport.
                    let y = match y {
                        Position::Absolute(y) => *y as usize,
                        Position::Relative(delta) => {
                            (buffer.cursor_y as isize).saturating_sub(-*delta) as usize
                        },
                        Position::EndRelative(delta) => rows.saturating_sub(*delta),
                    };

                    buffer.set_cursor(x, y, out)?;
                },
                Change::Attribute(AttributeChange::Intensity(value)) => {
                    self.pending_attr.set_intensity(*value);
                },
                Change::Attribute(AttributeChange::Italic(value)) => {
                    self.pending_attr.set_italic(*value);
                },
                Change::Attribute(AttributeChange::Reverse(value)) => {
                    self.pending_attr.set_reverse(*value);
                },
                Change::Attribute(AttributeChange::StrikeThrough(value)) => {
                    self.pending_attr.set_strikethrough(*value);
                },
                Change::Attribute(AttributeChange::Blink(value)) => {
                    self.pending_attr.set_blink(*value);
                },
                Change::Attribute(AttributeChange::Invisible(value)) => {
                    self.pending_attr.set_invisible(*value);
                },
                Change::Attribute(AttributeChange::Underline(value)) => {
                    self.pending_attr.set_underline(*value);
                },
                Change::Attribute(AttributeChange::Foreground(col)) => {
                    self.pending_attr.set_foreground(*col);
                },
                Change::Attribute(AttributeChange::Background(col)) => {
                    self.pending_attr.set_background(*col);
                },
                Change::Attribute(AttributeChange::Hyperlink(link)) => {
                    self.pending_attr.set_hyperlink(link.clone());
                },
                Change::AllAttributes(all) => {
                    self.pending_attr = all.clone();
                },
                Change::CursorColor(_color) => {},
                Change::CursorShape(_shape) => {},
                Change::CursorVisibility(_visibility) => {},
                Change::Image(image) => {
                    // Images are not supported, so just blank out the cells and
                    // move the cursor to the right spot

                    for y in 0..image.height {
                        buffer.fill(
                            ' ',
                            0,
                            buffer.cursor_x,
                            y + buffer.cursor_y,
                            image.width as usize,
                        );
                    }
                    buffer.set_cursor(buffer.cursor_x + image.width, buffer.cursor_y, out)?;
                },
                Change::ScrollRegionUp {
                    first_row,
                    region_size,
                    scroll_count,
                } => {
                    buffer.scroll(*first_row, *region_size, -1 * *scroll_count as isize, out)?;
                },
                Change::ScrollRegionDown {
                    first_row,
                    region_size,
                    scroll_count,
                } => {
                    buffer.scroll(*first_row, *region_size, *scroll_count as isize, out)?;
                },
                Change::Title(_text) => {
                    // Don't actually render this for now.
                    // The primary purpose of Change::Title at the time of
                    // writing is to transfer tab titles across domains
                    // in the wezterm multiplexer model.  It's not clear
                    // that it would be a good idea to unilaterally output
                    // eg: a title change escape sequence here in the
                    // renderer because we might be composing multiple widgets
                    // together, each with its own title.
                },
                Change::LineAttribute(_) => {
                    // Ignore line attributes
                },
            }
        }

        buffer.flush(out)?;
        Ok(())
    }
}
