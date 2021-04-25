use super::terminal_character::{TerminalCharacter, EMPTY_TERMINAL_CHARACTER, CharacterStyles};
use crate::utils::logging::debug_log_to_file;

#[derive(Debug)]
pub struct OutputBuffer {
    width: usize,
    height: usize,
    pub chars: Vec<TerminalCharacter>,
}

impl OutputBuffer {
    pub fn new(width: usize, height: usize) -> OutputBuffer {
        OutputBuffer {
            width,
            height,
            chars: Vec::with_capacity(width * height),
        }
    }

    pub fn new_empty(width: usize, height: usize) -> OutputBuffer {
        OutputBuffer {
            width,
            height,
            chars: vec![
                EMPTY_TERMINAL_CHARACTER;
                width * height
            ],
        }
    }

    /// Takes another buffer and returns a string with the commands to put the difference into STDOUT
    pub fn diff(&self, other: &OutputBuffer, (x_offset, y_offset): (usize, usize)) -> String {
        if other.width != self.width || other.height != self.height {
            // TODO: Clean rerender on a resize
        }
        self.chars
            .iter()
            .zip(other.chars.iter())
            .enumerate()
            .fold(
                (String::new(), false, CharacterStyles::new()),
                |(mut output, was_previous_match, mut styles), (index, (old_char, new_char))| {
                    if old_char != new_char {
                        if was_previous_match {
                            let y = index / self.width + y_offset;
                            let x = index % self.width + x_offset;
                            styles.clear();
                            // debug_log_to_file(format!("({}, {})", x, y));
                            output.push_str(&format!("\u{1b}[{};{}H\u{1b}[m", y + 1, x + 1));
                        }
                        let diff_styles = styles.update_and_return_diff(&new_char.styles);
                        if let Some(diff) = diff_styles {
                            output.push_str(diff.to_string().as_str());
                        }
                        output.push(new_char.character);
                    }
                    (output, old_char == new_char, styles)
                },
            )
            .0
    }
}
