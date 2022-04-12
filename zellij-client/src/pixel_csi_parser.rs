//! Main input logic.
use zellij_utils::{
    pane_size::SizeInPixels,
};

use zellij_utils::{
    ipc::PixelDimensions,
    regex::Regex,
    lazy_static::lazy_static,
};

use zellij_tile::data::Key;

pub struct PixelCsiParser {
    expected_pixel_csi_instructions: usize,
    current_buffer: Vec<(Key, Vec<u8>)>,
}

impl PixelCsiParser {
    pub fn new() -> Self {
        PixelCsiParser {
            expected_pixel_csi_instructions: 0,
            current_buffer: vec![],
        }
    }
    pub fn increment_expected_csi_instructions(&mut self, by: usize) {
        self.expected_pixel_csi_instructions += by;
    }
    pub fn decrement_expected_csi_instructions(&mut self, by: usize) {
        self.expected_pixel_csi_instructions = self.expected_pixel_csi_instructions.saturating_sub(by);
    }
    pub fn expected_instructions(&self) -> usize {
        self.expected_pixel_csi_instructions
    }
    pub fn parse(&mut self, key: Key, raw_bytes: Vec<u8>) -> Option<PixelDimensionsOrKeys> {
        if let Key::Char('t') = key {
            self.current_buffer.push((key, raw_bytes));
            match PixelDimensionsOrKeys::pixel_dimensions_from_keys(&self.current_buffer) {
                Ok(pixel_instruction) => {
                    self.decrement_expected_csi_instructions(1);
                    self.current_buffer.clear();
                    Some(pixel_instruction)
                },
                Err(_) => {
                    self.expected_pixel_csi_instructions = 0;
                    Some(PixelDimensionsOrKeys::Keys(self.current_buffer.drain(..).collect()))
                }
            }
        } else if self.key_is_valid(key) {
            self.current_buffer.push((key, raw_bytes));
            None
        } else {
            self.current_buffer.push((key, raw_bytes));
            self.expected_pixel_csi_instructions = 0;
            Some(PixelDimensionsOrKeys::Keys(self.current_buffer.drain(..).collect()))
        }
    }
    fn key_is_valid(&self, key: Key) -> bool {
        match key {
            Key::Esc => {
                // this is a UX improvement
                // in case the user's terminal doesn't support one or more of these signals,
                // if they spam ESC they need to be able to get back to normal mode and not "us
                // waiting for pixel instructions" mode
                if self.current_buffer.iter().find(|(key, _)| *key == Key::Esc).is_none() {
                    true
                } else {
                    false
                }
            }
            Key::Char(';') | Key::Char('[') => true,
            Key::Char(c) => {
                if let '0'..='9' = c {
                    true
                } else {
                    false
                }
            }
            _ => false
        }
    }
}

#[derive(Debug)]
pub enum PixelDimensionsOrKeys { // TODO: rename to PixelDimensionsOrKeys
    PixelDimensions(PixelDimensions),
    Keys(Vec<(Key, Vec<u8>)>),
}

impl PixelDimensionsOrKeys {
    pub fn pixel_dimensions_from_keys(keys: &Vec<(Key, Vec<u8>)>) -> Result<Self, &'static str> {
        lazy_static! {
            static ref RE: Regex = Regex::new(r"^\u{1b}\[(\d+);(\d+);(\d+)t$").unwrap();
        }
        let key_sequence: Vec<Option<char>> = keys.iter().map(|(key, _)| {
            match key {
                Key::Char(c) => Some(*c),
                Key::Esc => Some('\u{1b}'),
                _ => None,
            }
        }).collect();
        if key_sequence.iter().all(|k| k.is_some()) {
            let key_string: String = key_sequence.iter().map(|k| k.unwrap()).collect();
            let captures = RE.captures_iter(&key_string).next().ok_or("invalid_instruction")?;
            let csi_index = captures[1].parse::<usize>();
            let first_field = captures[2].parse::<usize>();
            let second_field = captures[3].parse::<usize>();
            if csi_index.is_err() || first_field.is_err() || second_field.is_err() {
                return Err("invalid_instruction");
            }
            match csi_index {
                Ok(4) => {
                    // text area size
                    Ok(PixelDimensionsOrKeys::PixelDimensions(PixelDimensions {
                        character_cell_size: None,
                        text_area_size: Some(SizeInPixels {
                            height: first_field.unwrap(),
                            width: second_field.unwrap(),
                        })
                    }))
                },
                Ok(6) => {
                    // character cell size
                    Ok(PixelDimensionsOrKeys::PixelDimensions(PixelDimensions {
                        character_cell_size: Some(SizeInPixels {
                            height: first_field.unwrap(),
                            width: second_field.unwrap(),
                        }),
                        text_area_size: None,
                    }))
                },
                _ => {
                    Err("invalid sequence")
                }
            }
        } else {
            Err("invalid sequence")
        }
    }
}
