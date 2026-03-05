//! The `LineEditor` struct provides line editing facilities similar
//! to those in the unix shell.
//!
//! ```no_run
//! use zellij_utils::vendored::termwiz::lineedit::{line_editor_terminal, NopLineEditorHost, LineEditor};
//!
//! fn main() -> zellij_utils::vendored::termwiz::Result<()> {
//!     let mut terminal = line_editor_terminal()?;
//!     let mut editor = LineEditor::new(&mut terminal);
//!     let mut host = NopLineEditorHost::default();
//!
//!     let line = editor.read_line(&mut host)?;
//!     println!("read line: {:?}", line);
//!
//!     Ok(())
//! }
//! ```
//!
//! ## Key Bindings
//!
//! The following key bindings are supported:
//!
//! Keystroke     | Action
//! ---------     | ------
//! Ctrl-A, Home  | Move cursor to the beginning of the line
//! Ctrl-E, End   | Move cursor to the end of the line
//! Ctrl-B, Left  | Move cursor one grapheme to the left
//! Ctrl-C        | Cancel the line editor
//! Ctrl-D        | Cancel the line editor with an End-of-File result
//! Ctrl-F, Right | Move cursor one grapheme to the right
//! Ctrl-H, Backspace | Delete the grapheme to the left of the cursor
//! Delete        | Delete the grapheme to the right of the cursor
//! Ctrl-J, Ctrl-M, Enter | Finish line editing and accept the current line
//! Ctrl-K        | Delete from cursor to end of line
//! Ctrl-L        | Move the cursor to the top left, clear screen and repaint
//! Ctrl-R        | Incremental history search mode
//! Ctrl-W        | Delete word leading up to cursor
//! Alt-b, Alt-Left | Move the cursor backwards one word
//! Alt-f, Alt-Right | Move the cursor forwards one word
use crate::vendored::termwiz::caps::{Capabilities, ProbeHints};
use crate::vendored::termwiz::input::{InputEvent, KeyCode, KeyEvent, Modifiers};
use crate::vendored::termwiz::surface::change::ChangeSequence;
use crate::vendored::termwiz::surface::{Change, Position};
use crate::vendored::termwiz::terminal::{new_terminal, Terminal};
use crate::vendored::termwiz::Result;
use crate::{vendored_termwiz_bail as bail, vendored_termwiz_ensure as ensure};

mod actions;
mod buffer;
mod history;
mod host;
pub use actions::{Action, Movement, RepeatCount};
pub use buffer::LineEditBuffer;
pub use history::*;
pub use host::*;

/// The `LineEditor` struct provides line editing facilities similar
/// to those in the unix shell.
/// ```no_run
/// use zellij_utils::vendored::termwiz::lineedit::{line_editor_terminal, NopLineEditorHost, LineEditor};
///
/// fn main() -> zellij_utils::vendored::termwiz::Result<()> {
///     let mut terminal = line_editor_terminal()?;
///     let mut editor = LineEditor::new(&mut terminal);
///     let mut host = NopLineEditorHost::default();
///
///     let line = editor.read_line(&mut host)?;
///     println!("read line: {:?}", line);
///
///     Ok(())
/// }
/// ```
pub struct LineEditor<'term> {
    terminal: &'term mut dyn Terminal,
    prompt: String,
    line: LineEditBuffer,

    history_pos: Option<usize>,
    bottom_line: Option<String>,

    completion: Option<CompletionState>,

    move_to_editor_start: Option<Change>,
    move_to_editor_end: Option<Change>,

    state: EditorState,
}

#[derive(Clone, Eq, PartialEq, Debug)]
enum EditorState {
    Inactive,
    Editing,
    Cancelled,
    Accepted,
    Searching {
        style: SearchStyle,
        direction: SearchDirection,
        matching_line: String,
        cursor: usize,
    },
}

struct CompletionState {
    candidates: Vec<CompletionCandidate>,
    index: usize,
    original_line: String,
    original_cursor: usize,
}

impl CompletionState {
    fn next(&mut self) {
        self.index += 1;
        if self.index >= self.candidates.len() {
            self.index = 0;
        }
    }

    fn current(&self) -> (usize, String) {
        let mut line = self.original_line.clone();
        let candidate = &self.candidates[self.index];
        line.replace_range(candidate.range.clone(), &candidate.text);

        // To figure the new cursor position do a little math:
        // "he<TAB>" when the completion is "hello" will set the completion
        // candidate to replace "he" with "hello", so the difference in the
        // lengths of these two is how far the cursor needs to move.
        let range_len = candidate.range.end - candidate.range.start;
        let new_cursor = self.original_cursor + candidate.text.len() - range_len;

        (new_cursor, line)
    }
}

impl<'term> LineEditor<'term> {
    /// Create a new line editor.
    /// In most cases, you'll want to use the `line_editor` function,
    /// because it creates a `Terminal` instance with the recommended
    /// settings, but if you need to decompose that for some reason,
    /// this snippet shows the recommended way to create a line
    /// editor:
    ///
    /// ```no_run
    /// use zellij_utils::vendored::termwiz::caps::{Capabilities, ProbeHints};
    /// use zellij_utils::vendored::termwiz::terminal::new_terminal;
    /// use zellij_utils::vendored::termwiz::Error;
    /// // Disable mouse input in the line editor
    /// let hints = ProbeHints::new_from_env()
    ///     .mouse_reporting(Some(false));
    /// let caps = Capabilities::new_with_hints(hints)?;
    /// let terminal = new_terminal(caps)?;
    /// # Ok::<(), Error>(())
    /// ```
    pub fn new(terminal: &'term mut dyn Terminal) -> Self {
        Self {
            terminal,
            prompt: "> ".to_owned(),
            line: LineEditBuffer::default(),
            history_pos: None,
            bottom_line: None,
            completion: None,
            move_to_editor_start: None,
            move_to_editor_end: None,
            state: EditorState::Inactive,
        }
    }

    fn render(&mut self, host: &mut dyn LineEditorHost) -> Result<()> {
        let screen_size = self.terminal.get_screen_size()?;

        let mut changes = ChangeSequence::new(screen_size.rows, screen_size.cols);

        changes.add(Change::ClearToEndOfScreen(Default::default()));
        changes.add(Change::AllAttributes(Default::default()));
        for ele in host.render_prompt(&self.prompt) {
            changes.add(ele);
        }
        changes.add(Change::AllAttributes(Default::default()));

        // If we're searching, the input area shows the match rather than the input,
        // and the cursor moves to the first matching character
        let (line_to_display, cursor) = match &self.state {
            EditorState::Searching {
                matching_line,
                cursor,
                ..
            } => (matching_line.as_str(), *cursor),
            _ => (self.line.get_line(), self.line.get_cursor()),
        };

        let cursor_position_after_printing_prompt = changes.current_cursor_position();

        let (elements, cursor_x_pos) = host.highlight_line(line_to_display, cursor);

        // Calculate what the cursor position would be after printing X columns
        // of text from the specified location.
        // Returns (x, y) of the resultant cursor position.
        fn compute_cursor_after_printing_x_columns(
            cursor_x: usize,
            cursor_y: isize,
            delta: usize,
            screen_cols: usize,
        ) -> (usize, isize) {
            let y = (cursor_x + delta) / screen_cols;
            let x = (cursor_x + delta) % screen_cols;

            let row = cursor_y + y as isize;
            let col = x.max(0) as usize;

            (col, row)
        }
        let cursor_position = compute_cursor_after_printing_x_columns(
            cursor_position_after_printing_prompt.0,
            cursor_position_after_printing_prompt.1,
            cursor_x_pos,
            screen_size.cols,
        );

        for ele in elements {
            changes.add(ele);
        }

        let cursor_after_line_render = changes.current_cursor_position();
        if cursor_after_line_render.0 == screen_size.cols {
            // If the cursor position remains in the first column
            // then the renderer may still consider itself to be on
            // the prior line; force out an additional character to force
            // it to apply wrapping/flush.
            changes.add(" ");
        }

        if let EditorState::Editing = &self.state {
            let preview_elements = host.render_preview(line_to_display);
            if !preview_elements.is_empty() {
                // Preview starts from a new line.
                changes.add("\r\n");
                // Do not be affected by attributes set by highlight_line.
                changes.add(Change::AllAttributes(Default::default()));
                for ele in preview_elements {
                    changes.add(ele);
                }
            }
        }

        if let EditorState::Searching {
            style, direction, ..
        } = &self.state
        {
            // We want to draw the search state below the input area
            let label = match (style, direction) {
                (SearchStyle::Substring, SearchDirection::Backwards) => "bck-i-search",
                (SearchStyle::Substring, SearchDirection::Forwards) => "fwd-i-search",
            };
            // Do not be affected by attributes set by previous lines.
            changes.add(Change::AllAttributes(Default::default()));
            // We position the actual cursor on the matching portion of
            // the text in the line editing area, but since the input
            // is drawn here, we render an `_` to indicate where the input
            // position really is.
            changes.add(format!("\r\n{}: {}_", label, self.line.get_line()));
        }

        // Add some debugging status at the bottom
        /*
        changes.add(format!(
            "\r\n{:?} {:?}",
            cursor_position,
            (changes.cursor_x, changes.cursor_y)
        ));
        */

        let render_height = changes.render_height();

        changes.move_to(cursor_position);

        let mut changes = changes.consume();
        if let Some(start) = self.move_to_editor_start.take() {
            changes.insert(0, start);
        }
        self.terminal.render(&changes)?;

        self.move_to_editor_start.replace(Change::CursorPosition {
            x: Position::Absolute(0),
            y: Position::Relative(-1 * cursor_position.1),
        });

        self.move_to_editor_end.replace(Change::CursorPosition {
            x: Position::Absolute(0),
            y: Position::Relative(1 + render_height as isize - cursor_position.1),
        });

        Ok(())
    }

    pub fn set_prompt(&mut self, prompt: &str) {
        self.prompt = prompt.to_owned();
    }

    /// Enter line editing mode.
    /// Control is not returned to the caller until a line has been
    /// accepted, or until an error is detected.
    /// Returns Ok(None) if the editor was cancelled eg: via CTRL-C.
    pub fn read_line(&mut self, host: &mut dyn LineEditorHost) -> Result<Option<String>> {
        self.read_line_with_optional_initial_value(host, None)
    }

    pub fn read_line_with_optional_initial_value(
        &mut self,
        host: &mut dyn LineEditorHost,
        initial_value: Option<&str>,
    ) -> Result<Option<String>> {
        ensure!(
            self.state == EditorState::Inactive,
            "recursive call to read_line!"
        );

        // Clear out the last render info so that we don't over-compensate
        // on the first call to render().
        self.move_to_editor_end.take();
        self.move_to_editor_start.take();

        self.terminal.set_raw_mode()?;
        self.state = EditorState::Editing;
        let res = self.read_line_impl(host, initial_value);
        self.state = EditorState::Inactive;

        if let Some(move_end) = self.move_to_editor_end.take() {
            self.terminal
                .render(&[move_end, Change::ClearToEndOfScreen(Default::default())])?;
        }

        self.terminal.flush()?;
        self.terminal.set_cooked_mode()?;
        res
    }

    fn resolve_action(
        &mut self,
        event: &InputEvent,
        host: &mut dyn LineEditorHost,
    ) -> Option<Action> {
        if let Some(action) = host.resolve_action(event, self) {
            return Some(action);
        }

        match event {
            InputEvent::Key(KeyEvent {
                key: KeyCode::Char('C'),
                modifiers: Modifiers::CTRL,
            }) => Some(Action::Cancel),

            InputEvent::Key(KeyEvent {
                key: KeyCode::Tab,
                modifiers: Modifiers::NONE,
            }) => Some(Action::Complete),

            InputEvent::Key(KeyEvent {
                key: KeyCode::Char('D'),
                modifiers: Modifiers::CTRL,
            }) => Some(Action::EndOfFile),

            InputEvent::Key(KeyEvent {
                key: KeyCode::Char('J'),
                modifiers: Modifiers::CTRL,
            })
            | InputEvent::Key(KeyEvent {
                key: KeyCode::Char('M'),
                modifiers: Modifiers::CTRL,
            })
            | InputEvent::Key(KeyEvent {
                key: KeyCode::Enter,
                modifiers: Modifiers::NONE,
            }) => Some(Action::AcceptLine),
            InputEvent::Key(KeyEvent {
                key: KeyCode::Char('H'),
                modifiers: Modifiers::CTRL,
            })
            | InputEvent::Key(KeyEvent {
                key: KeyCode::Backspace,
                modifiers: Modifiers::NONE,
            }) => Some(Action::Kill(Movement::BackwardChar(1))),
            InputEvent::Key(KeyEvent {
                key: KeyCode::Delete,
                modifiers: Modifiers::NONE,
            }) => Some(Action::KillAndMove(
                Movement::ForwardChar(1),
                Movement::None,
            )),

            InputEvent::Key(KeyEvent {
                key: KeyCode::Char('P'),
                modifiers: Modifiers::CTRL,
            })
            | InputEvent::Key(KeyEvent {
                key: KeyCode::UpArrow,
                modifiers: Modifiers::NONE,
            })
            | InputEvent::Key(KeyEvent {
                key: KeyCode::ApplicationUpArrow,
                modifiers: Modifiers::NONE,
            }) => Some(Action::HistoryPrevious),

            InputEvent::Key(KeyEvent {
                key: KeyCode::Char('N'),
                modifiers: Modifiers::CTRL,
            })
            | InputEvent::Key(KeyEvent {
                key: KeyCode::DownArrow,
                modifiers: Modifiers::NONE,
            })
            | InputEvent::Key(KeyEvent {
                key: KeyCode::ApplicationDownArrow,
                modifiers: Modifiers::NONE,
            }) => Some(Action::HistoryNext),

            InputEvent::Key(KeyEvent {
                key: KeyCode::Char('B'),
                modifiers: Modifiers::CTRL,
            })
            | InputEvent::Key(KeyEvent {
                key: KeyCode::ApplicationLeftArrow,
                modifiers: Modifiers::NONE,
            })
            | InputEvent::Key(KeyEvent {
                key: KeyCode::LeftArrow,
                modifiers: Modifiers::NONE,
            }) => Some(Action::Move(Movement::BackwardChar(1))),

            InputEvent::Key(KeyEvent {
                key: KeyCode::Char('W'),
                modifiers: Modifiers::CTRL,
            }) => Some(Action::Kill(Movement::BackwardWord(1))),

            InputEvent::Key(KeyEvent {
                key: KeyCode::Char('b'),
                modifiers: Modifiers::ALT,
            })
            | InputEvent::Key(KeyEvent {
                key: KeyCode::LeftArrow,
                modifiers: Modifiers::ALT,
            })
            | InputEvent::Key(KeyEvent {
                key: KeyCode::ApplicationLeftArrow,
                modifiers: Modifiers::ALT,
            }) => Some(Action::Move(Movement::BackwardWord(1))),

            InputEvent::Key(KeyEvent {
                key: KeyCode::Char('f'),
                modifiers: Modifiers::ALT,
            })
            | InputEvent::Key(KeyEvent {
                key: KeyCode::RightArrow,
                modifiers: Modifiers::ALT,
            })
            | InputEvent::Key(KeyEvent {
                key: KeyCode::ApplicationRightArrow,
                modifiers: Modifiers::ALT,
            }) => Some(Action::Move(Movement::ForwardWord(1))),

            InputEvent::Key(KeyEvent {
                key: KeyCode::Char('A'),
                modifiers: Modifiers::CTRL,
            })
            | InputEvent::Key(KeyEvent {
                key: KeyCode::Home,
                modifiers: Modifiers::NONE,
            }) => Some(Action::Move(Movement::StartOfLine)),
            InputEvent::Key(KeyEvent {
                key: KeyCode::Char('E'),
                modifiers: Modifiers::CTRL,
            })
            | InputEvent::Key(KeyEvent {
                key: KeyCode::End,
                modifiers: Modifiers::NONE,
            }) => Some(Action::Move(Movement::EndOfLine)),
            InputEvent::Key(KeyEvent {
                key: KeyCode::Char('F'),
                modifiers: Modifiers::CTRL,
            })
            | InputEvent::Key(KeyEvent {
                key: KeyCode::RightArrow,
                modifiers: Modifiers::NONE,
            })
            | InputEvent::Key(KeyEvent {
                key: KeyCode::ApplicationRightArrow,
                modifiers: Modifiers::NONE,
            }) => Some(Action::Move(Movement::ForwardChar(1))),
            InputEvent::Key(KeyEvent {
                key: KeyCode::Char(c),
                modifiers: Modifiers::SHIFT,
            })
            | InputEvent::Key(KeyEvent {
                key: KeyCode::Char(c),
                modifiers: Modifiers::NONE,
            }) => Some(Action::InsertChar(1, *c)),
            InputEvent::Paste(text) => Some(Action::InsertText(1, text.clone())),
            InputEvent::Key(KeyEvent {
                key: KeyCode::Char('L'),
                modifiers: Modifiers::CTRL,
            }) => Some(Action::Repaint),
            InputEvent::Key(KeyEvent {
                key: KeyCode::Char('K'),
                modifiers: Modifiers::CTRL,
            }) => Some(Action::Kill(Movement::EndOfLine)),

            InputEvent::Key(KeyEvent {
                key: KeyCode::Char('R'),
                modifiers: Modifiers::CTRL,
            }) => Some(Action::HistoryIncSearchBackwards),

            // This is the common binding for forwards, but it is usually
            // masked by the stty stop setting
            InputEvent::Key(KeyEvent {
                key: KeyCode::Char('S'),
                modifiers: Modifiers::CTRL,
            }) => Some(Action::HistoryIncSearchForwards),

            _ => None,
        }
    }

    fn kill_text(&mut self, kill_movement: Movement, move_movement: Movement) {
        self.clear_completion();
        self.line.kill_text(kill_movement, move_movement);
    }

    fn clear_completion(&mut self) {
        self.completion = None;
    }

    fn cancel_search_state(&mut self) {
        if let EditorState::Searching {
            matching_line,
            cursor,
            ..
        } = &self.state
        {
            self.line.set_line_and_cursor(matching_line, *cursor);
            self.state = EditorState::Editing;
        }
    }

    /// Returns the current line and cursor position.
    /// You don't normally need to call this unless you are defining
    /// a custom editor operation on the line buffer contents.
    /// The cursor position is the byte index into the line UTF-8 bytes.
    pub fn get_line_and_cursor(&mut self) -> (&str, usize) {
        (self.line.get_line(), self.line.get_cursor())
    }

    /// Sets the current line and cursor position.
    /// You don't normally need to call this unless you are defining
    /// a custom editor operation on the line buffer contents.
    /// The cursor position is the byte index into the line UTF-8 bytes.
    /// Panics: the cursor must be the first byte in a UTF-8 code point
    /// sequence or the end of the provided line.
    pub fn set_line_and_cursor(&mut self, line: &str, cursor: usize) {
        self.line.set_line_and_cursor(line, cursor);
    }

    /// Call this after changing modifying the line buffer.
    /// If the editor is in search mode this will update the search
    /// results, otherwise it will be a NOP.
    fn reapply_search_pattern(&mut self, host: &mut dyn LineEditorHost) {
        if let EditorState::Searching {
            style,
            direction,
            matching_line,
            cursor,
        } = &self.state
        {
            // We always start again from the bottom
            self.history_pos.take();

            let history_pos = match host.history().last() {
                Some(p) => p,
                None => {
                    // TODO: there's no way we can match anything.
                    // Generate a failed match result?
                    return;
                },
            };

            let last_matching_line;
            let last_cursor;

            if let Some(result) =
                host.history()
                    .search(history_pos, *style, *direction, self.line.get_line())
            {
                self.history_pos.replace(result.idx);
                last_matching_line = result.line.to_string();
                last_cursor = result.cursor;
            } else {
                last_matching_line = matching_line.clone();
                last_cursor = *cursor;
            }

            self.state = EditorState::Searching {
                style: *style,
                direction: *direction,
                matching_line: last_matching_line,
                cursor: last_cursor,
            };
        }
    }

    fn trigger_search(
        &mut self,
        style: SearchStyle,
        direction: SearchDirection,
        host: &mut dyn LineEditorHost,
    ) {
        self.clear_completion();

        if let EditorState::Searching { .. } = &self.state {
            // Already searching
        } else {
            // Not yet searching, so we start a new search
            // with an empty pattern
            self.line.clear();
            self.history_pos.take();
        }

        let history_pos = match self.history_pos {
            Some(p) => match direction.next(p) {
                Some(p) => p,
                None => return,
            },
            None => match host.history().last() {
                Some(p) => p,
                None => {
                    // TODO: there's no way we can match anything.
                    // Generate a failed match result?
                    return;
                },
            },
        };

        let search_result =
            host.history()
                .search(history_pos, style, direction, self.line.get_line());

        let last_matching_line;
        let last_cursor;

        if let Some(result) = search_result {
            self.history_pos.replace(result.idx);
            last_matching_line = result.line.to_string();
            last_cursor = result.cursor;
        } else if let EditorState::Searching {
            matching_line,
            cursor,
            ..
        } = &self.state
        {
            last_matching_line = matching_line.clone();
            last_cursor = *cursor;
        } else {
            last_matching_line = String::new();
            last_cursor = 0;
        }

        self.state = EditorState::Searching {
            style,
            direction,
            matching_line: last_matching_line,
            cursor: last_cursor,
        };
    }

    /// Applies the effect of the specified action to the line editor.
    /// You don't normally need to call this unless you are defining
    /// custom key mapping or custom actions in your embedding application.
    pub fn apply_action(&mut self, host: &mut dyn LineEditorHost, action: Action) -> Result<()> {
        // When searching, reinterpret history next/prev as repeated
        // search actions in the appropriate direction
        let action = match (action, &self.state) {
            (
                Action::HistoryPrevious,
                EditorState::Searching {
                    style: SearchStyle::Substring,
                    ..
                },
            ) => Action::HistoryIncSearchBackwards,
            (
                Action::HistoryNext,
                EditorState::Searching {
                    style: SearchStyle::Substring,
                    ..
                },
            ) => Action::HistoryIncSearchForwards,
            (action, _) => action,
        };

        match action {
            Action::Cancel => self.state = EditorState::Cancelled,
            Action::NoAction => {},
            Action::AcceptLine => {
                // Make sure that hitting Enter for a line that
                // shows in the incremental search causes that
                // line to be accepted, rather than the search pattern!
                self.cancel_search_state();

                self.state = EditorState::Accepted;
            },
            Action::EndOfFile => {
                return Err(
                    std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "End Of File").into(),
                )
            },
            Action::Kill(movement) => {
                self.kill_text(movement, movement);
                self.reapply_search_pattern(host);
            },
            Action::KillAndMove(kill_movement, move_movement) => {
                self.kill_text(kill_movement, move_movement);
                self.reapply_search_pattern(host);
            },

            Action::Move(movement) => {
                self.clear_completion();
                self.cancel_search_state();
                self.line.exec_movement(movement);
            },

            Action::InsertChar(rep, c) => {
                self.clear_completion();
                for _ in 0..rep {
                    self.line.insert_char(c);
                }
                self.reapply_search_pattern(host);
            },
            Action::InsertText(rep, text) => {
                self.clear_completion();
                for _ in 0..rep {
                    self.line.insert_text(&text);
                }
                self.reapply_search_pattern(host);
            },
            Action::Repaint => {
                self.terminal
                    .render(&[Change::ClearScreen(Default::default())])?;
            },
            Action::HistoryPrevious => {
                self.clear_completion();
                self.cancel_search_state();

                if let Some(cur_pos) = self.history_pos.as_ref() {
                    let prior_idx = cur_pos.saturating_sub(1);
                    if let Some(prior) = host.history().get(prior_idx) {
                        self.history_pos = Some(prior_idx);
                        self.line.set_line_and_cursor(&prior, prior.len());
                    }
                } else if let Some(last) = host.history().last() {
                    self.bottom_line = Some(self.line.get_line().to_string());
                    self.history_pos = Some(last);
                    let line = host
                        .history()
                        .get(last)
                        .expect("History::last and History::get to be consistent");
                    self.line.set_line_and_cursor(&line, line.len())
                }
            },
            Action::HistoryNext => {
                self.clear_completion();
                self.cancel_search_state();

                if let Some(cur_pos) = self.history_pos.as_ref() {
                    let next_idx = cur_pos.saturating_add(1);
                    if let Some(next) = host.history().get(next_idx) {
                        self.history_pos = Some(next_idx);
                        self.line.set_line_and_cursor(&next, next.len());
                    } else if let Some(bottom) = self.bottom_line.take() {
                        self.line.set_line_and_cursor(&bottom, bottom.len());
                    } else {
                        self.line.clear();
                    }
                }
            },

            Action::HistoryIncSearchBackwards => {
                self.trigger_search(SearchStyle::Substring, SearchDirection::Backwards, host);
            },
            Action::HistoryIncSearchForwards => {
                self.trigger_search(SearchStyle::Substring, SearchDirection::Forwards, host);
            },

            Action::Complete => {
                self.cancel_search_state();

                if self.completion.is_none() {
                    let candidates = host.complete(self.line.get_line(), self.line.get_cursor());
                    if !candidates.is_empty() {
                        let state = CompletionState {
                            candidates,
                            index: 0,
                            original_line: self.line.get_line().to_string(),
                            original_cursor: self.line.get_cursor(),
                        };

                        let (cursor, line) = state.current();
                        self.line.set_line_and_cursor(&line, cursor);

                        // If there is only a single completion then don't
                        // leave us in a state where we just cycle on the
                        // same completion over and over.
                        if state.candidates.len() > 1 {
                            self.completion = Some(state);
                        }
                    }
                } else if let Some(state) = self.completion.as_mut() {
                    state.next();
                    let (cursor, line) = state.current();
                    self.line.set_line_and_cursor(&line, cursor);
                }
            },
        }

        Ok(())
    }

    fn read_line_impl(
        &mut self,
        host: &mut dyn LineEditorHost,
        initial_value: Option<&str>,
    ) -> Result<Option<String>> {
        self.line.clear();
        if let Some(value) = initial_value {
            self.line.set_line_and_cursor(value, value.len());
        }
        self.history_pos = None;
        self.bottom_line = None;
        self.clear_completion();

        self.render(host)?;
        while let Some(event) = self.terminal.poll_input(None)? {
            if let Some(action) = self.resolve_action(&event, host) {
                self.apply_action(host, action)?;
                // Editor state might have changed. Re-render to clear
                // preview or highlight lines differently.
                self.render(host)?;
                match self.state {
                    EditorState::Searching { .. } | EditorState::Editing => {},
                    EditorState::Cancelled => return Ok(None),
                    EditorState::Accepted => return Ok(Some(self.line.get_line().to_string())),
                    EditorState::Inactive => bail!("editor is inactive during read line!?"),
                }
            } else {
                self.render(host)?;
            }
        }
        Ok(Some(self.line.get_line().to_string()))
    }
}

/// Create a `Terminal` with the recommended settings for use with
/// a `LineEditor`.
pub fn line_editor_terminal() -> Result<impl Terminal> {
    let hints = ProbeHints::new_from_env().mouse_reporting(Some(false));
    let caps = Capabilities::new_with_hints(hints)?;
    new_terminal(caps)
}
