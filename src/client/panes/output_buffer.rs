use crate::panes::terminal_character::TerminalCharacter;

use super::CharacterStyles;

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
            // chars: vec![TerminalCharacter { character: ' ', styles: CharacterStyles::new() }; width * height],
        }
    }

    pub fn new_filled(width: usize, height: usize, c: char) -> OutputBuffer {
        OutputBuffer {
            width,
            height,
            chars: vec![TerminalCharacter { character: c, styles: CharacterStyles::new() }; width * height],
        }
    }

    /// Takes another buffer and returns a string with the commands to put the difference into STDOUT
    pub fn diff(&self, other: OutputBuffer, (xOffset, yOffset): (usize, usize)) -> String {
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
                    let diff_styles = styles.update_and_return_diff(&new_char.styles);
                    if old_char != new_char {
                        if !was_previous_match {
                            let y = index / self.width + yOffset;
                            let x = index % self.width + xOffset;
                            output.push_str(&format!("\u{1b}[{};{}H\u{1b}[m", y + 1, x + 1));
                            // goto row/col and reset styles
                        }
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
