//! The in-plugin "+ New Session" name-entry prompt. Esc / Enter and the
//! [Cancel] / [Accept] tap targets are equivalent.

use unicode_width::UnicodeWidthStr;
use zellij_tile::prelude::*;

use crate::click::{ClickAction, ClickRegion};
use crate::frame::Frame;
use crate::screens::ActiveScreen;

const H_PAD: usize = 1;
const RESERVED_INPUT_CHARS: usize = 20;
const ELLIPSIS: &str = "\u{2026}";
const DEFAULT_BUTTON_GAP: usize = 6;
const BLOCK_ROWS: usize = 5;
const TITLE: &str = "New Session";
const INPUT_LABEL: &str = "Name: ";
const CANCEL_LABEL: &str = "[Cancel]";
const ACCEPT_LABEL: &str = "[Accept]";

#[derive(Default)]
pub struct NewSessionPromptScreen {
    pub pending_session_name: String,
    /// Sticky count of leading characters hidden behind the `…`
    /// indicator when the name outgrows the input row.
    pub new_session_view_offset: usize,
    /// High-water mark of the content width: the box grows to fit the
    /// typed name and never shrinks while backspacing.
    pub new_session_content_w: usize,
}

impl NewSessionPromptScreen {
    fn reset(&mut self) {
        self.pending_session_name.clear();
        self.new_session_view_offset = 0;
        self.new_session_content_w = 0;
    }

    pub fn open(&mut self, active: &mut ActiveScreen) -> bool {
        self.reset();
        *active = ActiveScreen::NewSessionPrompt;
        true
    }

    pub fn cancel(&mut self, active: &mut ActiveScreen) -> bool {
        self.reset();
        *active = ActiveScreen::Sessions;
        true
    }

    /// An empty name lets the host auto-name the session.
    pub fn accept(&mut self, active: &mut ActiveScreen) -> bool {
        let name = std::mem::take(&mut self.pending_session_name);
        let arg = if name.is_empty() {
            None
        } else {
            Some(name.as_str())
        };
        switch_session(arg);
        self.new_session_view_offset = 0;
        self.new_session_content_w = 0;
        *active = ActiveScreen::Viewport;
        true
    }

    /// Every key is consumed so a typo never leaks to the pane beneath.
    pub fn handle_key(&mut self, active: &mut ActiveScreen, key: KeyWithModifier) -> bool {
        match key.bare_key {
            BareKey::Esc => {
                self.cancel(active);
            },
            BareKey::Enter => {
                self.accept(active);
            },
            BareKey::Backspace => {
                self.pending_session_name.pop();
            },
            BareKey::Char(c) => {
                self.pending_session_name.push(c);
            },
            _ => {},
        }
        true
    }

    pub fn render(
        &mut self,
        frame: &mut Frame,
        row_start: usize,
        row_end: usize,
        cols: usize,
    ) {
        let body_height = row_end.saturating_sub(row_start);
        if body_height == 0 || cols == 0 {
            return;
        }

        let input = self.visible_input_row(cols);
        let layout = self.layout(&input, cols, row_start, row_end, body_height);
        draw_title(&layout);
        draw_input(&input, &layout);
        draw_buttons(frame, &layout);
    }

    /// The `"Name: …xyz_"` row, scrolled so the cursor stays visible.
    /// Updates the sticky `new_session_view_offset`.
    fn visible_input_row(&mut self, cols: usize) -> String {
        let max_input_total_w = cols.saturating_sub(2 * H_PAD);
        // Reserve one cell for the trailing cursor underscore.
        let max_chars_no_ellipsis = max_input_total_w
            .saturating_sub(INPUT_LABEL.len())
            .saturating_sub(1);
        let max_chars_with_ellipsis =
            max_chars_no_ellipsis.saturating_sub(UnicodeWidthStr::width(ELLIPSIS));

        let buffer_chars = self.pending_session_name.chars().count();
        let view_offset = if buffer_chars > max_chars_no_ellipsis {
            let min_offset = buffer_chars.saturating_sub(max_chars_with_ellipsis);
            self.new_session_view_offset.max(min_offset).min(buffer_chars)
        } else {
            0
        };
        self.new_session_view_offset = view_offset;

        let visible: String = self
            .pending_session_name
            .chars()
            .skip(view_offset)
            .collect();
        if view_offset > 0 {
            format!("{}{}{}_", INPUT_LABEL, ELLIPSIS, visible)
        } else {
            format!("{}{}_", INPUT_LABEL, visible)
        }
    }

    fn layout(
        &mut self,
        input: &str,
        cols: usize,
        row_start: usize,
        row_end: usize,
        body_height: usize,
    ) -> PromptLayout {
        let default_buttons_w =
            UnicodeWidthStr::width(CANCEL_LABEL) + DEFAULT_BUTTON_GAP + UnicodeWidthStr::width(ACCEPT_LABEL);
        let default_input_w = INPUT_LABEL.len() + RESERVED_INPUT_CHARS + 1;
        let default_content_w = UnicodeWidthStr::width(TITLE)
            .max(default_input_w)
            .max(default_buttons_w);

        let target_content_w = default_content_w.max(UnicodeWidthStr::width(input));
        let content_w = self.new_session_content_w.max(target_content_w);
        self.new_session_content_w = content_w;

        let box_w = (content_w + 2 * H_PAD).min(cols);
        let content_x = cols.saturating_sub(box_w) / 2 + H_PAD;

        let top = if body_height >= BLOCK_ROWS {
            row_start + (body_height - BLOCK_ROWS) / 2
        } else {
            row_start
        };

        PromptLayout {
            content_x,
            content_w: box_w.saturating_sub(2 * H_PAD),
            row_title: top,
            row_input: top + 2,
            row_buttons: top + 4,
            row_end,
        }
    }
}

struct PromptLayout {
    content_x: usize,
    content_w: usize,
    row_title: usize,
    row_input: usize,
    row_buttons: usize,
    row_end: usize,
}

impl PromptLayout {
    fn centered_x(&self, label_w: usize) -> usize {
        self.content_x + self.content_w.saturating_sub(label_w) / 2
    }

    fn right_x(&self, label_w: usize) -> usize {
        self.content_x + self.content_w.saturating_sub(label_w)
    }
}

fn draw_title(layout: &PromptLayout) {
    if layout.row_title < layout.row_end {
        let x = layout.centered_x(UnicodeWidthStr::width(TITLE));
        print_text_with_coordinates(Text::new(TITLE).color_range(3, ..), x, layout.row_title, None, None);
    }
}

fn draw_input(input: &str, layout: &PromptLayout) {
    if layout.row_input < layout.row_end {
        print_text_with_coordinates(Text::new(input), layout.content_x, layout.row_input, None, None);
    }
}

fn draw_buttons(frame: &mut Frame, layout: &PromptLayout) {
    if layout.row_buttons >= layout.row_end {
        return;
    }
    let cancel_w = UnicodeWidthStr::width(CANCEL_LABEL);
    let accept_w = UnicodeWidthStr::width(ACCEPT_LABEL);
    let cancel_x = layout.content_x;
    let accept_x = layout.right_x(accept_w);
    let gap = " ".repeat(accept_x.saturating_sub(cancel_x + cancel_w));
    let buttons = format!("{}{}{}", CANCEL_LABEL, gap, ACCEPT_LABEL);
    let buttons_text = Text::new(&buttons)
        .error_color_substring(CANCEL_LABEL)
        .success_color_substring(ACCEPT_LABEL);
    print_text_with_coordinates(buttons_text, cancel_x, layout.row_buttons, None, None);

    frame.click_regions.push(ClickRegion::tight(
        layout.row_buttons,
        cancel_x,
        cancel_x + cancel_w,
        ClickAction::CancelNewSessionPrompt,
    ));
    frame.click_regions.push(ClickRegion::tight(
        layout.row_buttons,
        accept_x,
        accept_x + accept_w,
        ClickAction::AcceptNewSessionPrompt,
    ));
}
