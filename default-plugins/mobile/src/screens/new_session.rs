//! The in-plugin "+ New Session" name-entry prompt. Owns its text
//! buffer and the truncation/box-width bookkeeping the renderer needs to
//! keep the cursor visible and the box stable across keystrokes. Esc /
//! Enter and the [Cancel] / [Accept] tap targets are equivalent.

use unicode_width::UnicodeWidthStr;
use zellij_tile::prelude::*;

use crate::click::{ClickAction, ClickRegion};
use crate::frame::Frame;
use crate::screens::ActiveScreen;

/// New-session prompt state.
#[derive(Default)]
pub struct NewSessionPromptScreen {
    /// In-progress text buffer for the name-entry overlay. Empty while
    /// the prompt is not open. Reset on open and after a successful
    /// Enter submit (the buffer is `mem::take`-n into `switch_session`).
    pub pending_session_name: String,
    /// Sticky scroll offset into `pending_session_name` for the input
    /// row. Counts characters hidden behind the leading `…` indicator
    /// when the typed name is too long to fit on one row.
    pub new_session_view_offset: usize,
    /// High-water-mark of the prompt's content area width. The box never
    /// *shrinks* during a single prompt session — it grows to fit the
    /// typed name and then stays at that size while the user backspaces.
    pub new_session_content_w: usize,
}

impl NewSessionPromptScreen {
    /// Reset the prompt's buffer + box anchors. Shared by every entry /
    /// exit path so a previously-cancelled attempt never leaks back in.
    fn reset(&mut self) {
        self.pending_session_name.clear();
        self.new_session_view_offset = 0;
        self.new_session_content_w = 0;
    }

    /// Open the prompt from the Sessions selector. No host call — the
    /// actual `switch_session` happens in the Enter / Accept paths.
    pub fn open(&mut self, active: &mut ActiveScreen) -> bool {
        self.reset();
        *active = ActiveScreen::NewSessionPrompt;
        true
    }

    /// [Cancel] / Esc: discard the buffer and return to the Sessions
    /// selector (the screen the user was on when they opened the
    /// prompt).
    pub fn cancel(&mut self, active: &mut ActiveScreen) -> bool {
        self.reset();
        *active = ActiveScreen::Sessions;
        true
    }

    /// [Accept] / Enter: hand the buffer to `switch_session` (empty →
    /// host auto-name) and close the prompt. The mobile plugin dismounts
    /// as the host swaps the client into the new session.
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

    /// Capture keys for the name buffer. Every key is consumed so a typo
    /// never leaks to the embedded pane beneath the prompt. Sticky
    /// ctrl/alt state is intentionally left untouched.
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
            // Every other key (arrows, function keys, Tab, …) is
            // swallowed silently — the prompt is intentionally minimal.
            _ => {},
        }
        true
    }

    /// In-plugin name-entry overlay for "+ New Session". Drawn
    /// vertically centered within `[row_start, row_end)`.
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

        let title = "New Session";

        let cancel_label = "[Cancel]";
        let accept_label = "[Accept]";

        const H_PAD: usize = 1;
        const RESERVED_INPUT_CHARS: usize = 20;
        const ELLIPSIS: &str = "\u{2026}";

        let title_w = UnicodeWidthStr::width(title);
        let cancel_w = UnicodeWidthStr::width(cancel_label);
        let accept_w = UnicodeWidthStr::width(accept_label);
        let input_label_w = "Name: ".len();

        let buffer_chars = self.pending_session_name.chars().count();
        let max_input_total_w = cols.saturating_sub(2 * H_PAD);
        let max_chars_no_ellipsis = max_input_total_w
            .saturating_sub(input_label_w)
            .saturating_sub(1);
        let ellipsis_w = UnicodeWidthStr::width(ELLIPSIS);
        let max_chars_with_ellipsis = max_chars_no_ellipsis.saturating_sub(ellipsis_w);

        let view_offset = if buffer_chars > max_chars_no_ellipsis {
            let min_offset = buffer_chars.saturating_sub(max_chars_with_ellipsis);
            self.new_session_view_offset
                .max(min_offset)
                .min(buffer_chars)
        } else {
            0
        };
        self.new_session_view_offset = view_offset;

        let visible_buffer: String = self
            .pending_session_name
            .chars()
            .skip(view_offset)
            .collect();
        let input = if view_offset > 0 {
            format!("Name: {}{}_", ELLIPSIS, visible_buffer)
        } else {
            format!("Name: {}_", visible_buffer)
        };
        let visible_input_w = UnicodeWidthStr::width(input.as_str());

        const DEFAULT_BUTTON_GAP: usize = 6;
        let default_buttons_w = cancel_w + DEFAULT_BUTTON_GAP + accept_w;
        let default_input_w = input_label_w + RESERVED_INPUT_CHARS + 1;
        let default_content_w = title_w.max(default_input_w).max(default_buttons_w);

        let target_content_w = default_content_w.max(visible_input_w);
        let content_w = self.new_session_content_w.max(target_content_w);
        self.new_session_content_w = content_w;
        let box_w = (content_w + 2 * H_PAD).min(cols);
        let box_x = cols.saturating_sub(box_w) / 2;
        let content_x = box_x + H_PAD;
        let content_w_effective = box_w.saturating_sub(2 * H_PAD);

        const BLOCK_ROWS: usize = 5;
        let top = if body_height >= BLOCK_ROWS {
            row_start + (body_height - BLOCK_ROWS) / 2
        } else {
            row_start
        };

        let row_title = top;
        let row_input = top + 2;
        let row_buttons = top + 4;

        if row_title < row_end {
            let title_x = content_x + content_w_effective.saturating_sub(title_w) / 2;
            print_text_with_coordinates(
                Text::new(title).color_range(3, ..),
                title_x,
                row_title,
                None,
                None,
            );
        }

        if row_input < row_end {
            print_text_with_coordinates(Text::new(&input), content_x, row_input, None, None);
        }

        if row_buttons < row_end {
            let cancel_x = content_x;
            let accept_x = content_x + content_w_effective.saturating_sub(accept_w);
            let gap_w = accept_x.saturating_sub(cancel_x + cancel_w);
            let gap: String = " ".repeat(gap_w);
            let buttons = format!("{}{}{}", cancel_label, gap, accept_label);
            let buttons_text = Text::new(&buttons)
                .error_color_substring(cancel_label)
                .success_color_substring(accept_label);
            print_text_with_coordinates(buttons_text, cancel_x, row_buttons, None, None);

            frame.click_regions.push(ClickRegion::tight(
                row_buttons,
                cancel_x,
                cancel_x + cancel_w,
                ClickAction::CancelNewSessionPrompt,
            ));
            frame.click_regions.push(ClickRegion::tight(
                row_buttons,
                accept_x,
                accept_x + accept_w,
                ClickAction::AcceptNewSessionPrompt,
            ));
        }
    }
}
