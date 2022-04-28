use zellij_utils::pane_size::SizeInPixels;

use zellij_utils::{ipc::PixelDimensions, lazy_static::lazy_static, regex::Regex};

use zellij_tile::data::{CharOrArrow, Key};

pub struct StdinAnsiParser {
    expected_ansi_instructions: usize,
    current_buffer: Vec<(Key, Vec<u8>)>,
}

impl StdinAnsiParser {
    pub fn new() -> Self {
        StdinAnsiParser {
            expected_ansi_instructions: 0,
            current_buffer: vec![],
        }
    }
    pub fn increment_expected_ansi_instructions(&mut self, to: usize) {
        self.expected_ansi_instructions = to;
    }
    pub fn decrement_expected_ansi_instructions(&mut self, by: usize) {
        self.expected_ansi_instructions = self.expected_ansi_instructions.saturating_sub(by);
    }
    pub fn expected_instructions(&self) -> usize {
        self.expected_ansi_instructions
    }
    pub fn parse(&mut self, key: Key, raw_bytes: Vec<u8>) -> Option<AnsiStdinInstructionOrKeys> {
        if let Key::Char('t') = key {
            self.current_buffer.push((key, raw_bytes));
            match AnsiStdinInstructionOrKeys::pixel_dimensions_from_keys(&self.current_buffer) {
                Ok(pixel_instruction) => {
                    self.decrement_expected_ansi_instructions(1);
                    self.current_buffer.clear();
                    Some(pixel_instruction)
                }
                Err(_) => {
                    self.expected_ansi_instructions = 0;
                    Some(AnsiStdinInstructionOrKeys::Keys(
                        self.current_buffer.drain(..).collect(),
                    ))
                }
            }
        } else if let Key::Alt(CharOrArrow::Char('\\')) = key {
            match AnsiStdinInstructionOrKeys::color_sequence_from_keys(&self.current_buffer) {
                Ok(color_instruction) => {
                    self.decrement_expected_ansi_instructions(1);
                    self.current_buffer.clear();
                    Some(color_instruction)
                }
                Err(_) => {
                    self.expected_ansi_instructions = 0;
                    Some(AnsiStdinInstructionOrKeys::Keys(
                        self.current_buffer.drain(..).collect(),
                    ))
                }
            }
        } else if self.key_is_valid(key) {
            self.current_buffer.push((key, raw_bytes));
            None
        } else {
            self.current_buffer.push((key, raw_bytes));
            self.expected_ansi_instructions = 0;
            Some(AnsiStdinInstructionOrKeys::Keys(
                self.current_buffer.drain(..).collect(),
            ))
        }
    }
    fn key_is_valid(&self, key: Key) -> bool {
        if self.current_buffer.is_empty()
            && (key != Key::Esc && key != Key::Alt(CharOrArrow::Char(']')))
        {
            // the first key of a sequence is always Esc, but termwiz interprets esc + ] as Alt+]
            return false;
        }
        match key {
            Key::Esc => {
                // this is a UX improvement
                // in case the user's terminal doesn't support one or more of these signals,
                // if they spam ESC they need to be able to get back to normal mode and not "us
                // waiting for ansi instructions" mode
                !self
                    .current_buffer
                    .iter().any(|(key, _)| *key == Key::Esc)
            }
            Key::Char(';')
            | Key::Char('[')
            | Key::Char(']')
            | Key::Char('r')
            | Key::Char('g')
            | Key::Char('b')
            | Key::Char('\\')
            | Key::Char(':')
            | Key::Char('/') => true,
            Key::Alt(CharOrArrow::Char(']')) => true,
            Key::Alt(CharOrArrow::Char('\\')) => true,
            Key::Char(c) => {
                if let '0'..='9' | 'a'..='f' = c {
                    true
                } else {
                    false
                }
            }
            _ => false,
        }
    }
}

#[derive(Debug)]
pub enum AnsiStdinInstructionOrKeys {
    PixelDimensions(PixelDimensions),
    BackgroundColor(String),
    ForegroundColor(String),
    Keys(Vec<(Key, Vec<u8>)>),
}

impl AnsiStdinInstructionOrKeys {
    pub fn pixel_dimensions_from_keys(keys: &Vec<(Key, Vec<u8>)>) -> Result<Self, &'static str> {
        lazy_static! {
            static ref RE: Regex = Regex::new(r"^\u{1b}\[(\d+);(\d+);(\d+)t$").unwrap();
        }
        let key_sequence: Vec<Option<char>> = keys
            .iter()
            .map(|(key, _)| match key {
                Key::Char(c) => Some(*c),
                Key::Esc => Some('\u{1b}'),
                _ => None,
            })
            .collect();
        if key_sequence.iter().all(|k| k.is_some()) {
            let key_string: String = key_sequence.iter().map(|k| k.unwrap()).collect();
            let captures = RE
                .captures_iter(&key_string)
                .next()
                .ok_or("invalid_instruction")?;
            let csi_index = captures[1].parse::<usize>();
            let first_field = captures[2].parse::<usize>();
            let second_field = captures[3].parse::<usize>();
            if csi_index.is_err() || first_field.is_err() || second_field.is_err() {
                return Err("invalid_instruction");
            }
            match csi_index {
                Ok(4) => {
                    // text area size
                    Ok(AnsiStdinInstructionOrKeys::PixelDimensions(
                        PixelDimensions {
                            character_cell_size: None,
                            text_area_size: Some(SizeInPixels {
                                height: first_field.unwrap(),
                                width: second_field.unwrap(),
                            }),
                        },
                    ))
                }
                Ok(6) => {
                    // character cell size
                    Ok(AnsiStdinInstructionOrKeys::PixelDimensions(
                        PixelDimensions {
                            character_cell_size: Some(SizeInPixels {
                                height: first_field.unwrap(),
                                width: second_field.unwrap(),
                            }),
                            text_area_size: None,
                        },
                    ))
                }
                _ => Err("invalid sequence"),
            }
        } else {
            Err("invalid sequence")
        }
    }
    pub fn color_sequence_from_keys(keys: &Vec<(Key, Vec<u8>)>) -> Result<Self, &'static str> {
        lazy_static! {
            static ref BACKGROUND_RE: Regex = Regex::new(r"11;(.*)$").unwrap();
        }
        lazy_static! {
            static ref FOREGROUND_RE: Regex = Regex::new(r"10;(.*)$").unwrap();
        }
        let key_string = keys.iter().fold(String::new(), |mut acc, (key, _)| {
            match key {
                Key::Char(c) => acc.push(*c),
                _ => {}
            };
            acc
        });
        if let Some(captures) = BACKGROUND_RE.captures_iter(&key_string).next() {
            let background_query_response = captures[1].parse::<String>();
            Ok(AnsiStdinInstructionOrKeys::BackgroundColor(
                background_query_response.unwrap(),
            ))
        } else if let Some(captures) = FOREGROUND_RE.captures_iter(&key_string).next() {
            let foreground_query_response = captures[1].parse::<String>();
            Ok(AnsiStdinInstructionOrKeys::ForegroundColor(
                foreground_query_response.unwrap(),
            ))
        } else {
            Err("invalid_instruction")
        }
        //         let background_query_response = captures[1].parse::<String>();
        //         if background_query_response.is_err() {
        //             return Err("invalid_instruction");
        //         }
        //         Ok(AnsiStdinInstructionOrKeys::BackgroundColor(background_query_response.unwrap()))
        //         let captures = RE
        //             .captures_iter(&key_string)
        //             .next()
        //             .ok_or("invalid_instruction")?;
        //         let background_query_response = captures[1].parse::<String>();
        //         if background_query_response.is_err() {
        //             return Err("invalid_instruction");
        //         }
        //         Ok(AnsiStdinInstructionOrKeys::BackgroundColor(background_query_response.unwrap()))
    }
}
