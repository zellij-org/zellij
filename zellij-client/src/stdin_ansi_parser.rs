use std::time::{Duration, Instant};

const STARTUP_PARSE_DEADLINE_MS: u64 = 500;
use lazy_static::lazy_static;
use regex::Regex;
use zellij_utils::{
    consts::ZELLIJ_STDIN_CACHE_FILE, ipc::PixelDimensions, pane_size::SizeInPixels,
};

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};

/// Describe the terminal implementation of synchronised output
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum SyncOutput {
    DCS,
    CSI,
}

impl SyncOutput {
    pub fn start_seq(&self) -> &'static [u8] {
        static CSI_BSU_SEQ: &'static [u8] = "\u{1b}[?2026h".as_bytes();
        static DCS_BSU_SEQ: &'static [u8] = "\u{1b}P=1s\u{1b}".as_bytes();
        match self {
            SyncOutput::DCS => DCS_BSU_SEQ,
            SyncOutput::CSI => CSI_BSU_SEQ,
        }
    }

    pub fn end_seq(&self) -> &'static [u8] {
        static CSI_ESU_SEQ: &'static [u8] = "\u{1b}[?2026l".as_bytes();
        static DCS_ESU_SEQ: &'static [u8] = "\u{1b}P=2s\u{1b}".as_bytes();
        match self {
            SyncOutput::DCS => DCS_ESU_SEQ,
            SyncOutput::CSI => CSI_ESU_SEQ,
        }
    }
}

#[derive(Debug)]
pub struct StdinAnsiParser {
    raw_buffer: Vec<u8>,
    pending_color_sequences: Vec<(usize, String)>,
    pending_events: Vec<AnsiStdinInstruction>,
    parse_deadline: Option<Instant>,
}

impl StdinAnsiParser {
    pub fn new() -> Self {
        StdinAnsiParser {
            raw_buffer: vec![],
            pending_color_sequences: vec![],
            pending_events: vec![],
            parse_deadline: None,
        }
    }
    pub fn terminal_emulator_query_string(&mut self) -> String {
        // note that this assumes the String will be sent to the terminal emulator and so starts a
        // deadline timeout (self.parse_deadline)

        // <ESC>[14t => get text area size in pixels,
        // <ESC>[16t => get character cell size in pixels
        // <ESC>]11;?<ESC>\ => get background color
        // <ESC>]10;?<ESC>\ => get foreground color
        // <ESC>[?2026$p => get synchronised output mode
        let mut query_string = String::from(
            "\u{1b}[14t\u{1b}[16t\u{1b}]11;?\u{1b}\u{5c}\u{1b}]10;?\u{1b}\u{5c}\u{1b}[?2026$p",
        );

        // query colors
        // eg. <ESC>]4;5;?<ESC>\ => query color register number 5
        for i in 0..256 {
            query_string.push_str(&format!("\u{1b}]4;{};?\u{1b}\u{5c}", i));
        }
        self.parse_deadline =
            Some(Instant::now() + Duration::from_millis(STARTUP_PARSE_DEADLINE_MS));
        query_string
    }
    fn drain_pending_events(&mut self) -> Vec<AnsiStdinInstruction> {
        let mut events = vec![];
        events.append(&mut self.pending_events);
        if let Some(color_registers) =
            AnsiStdinInstruction::color_registers_from_bytes(&mut self.pending_color_sequences)
        {
            events.push(color_registers);
        }
        events
    }
    pub fn should_parse(&self) -> bool {
        if let Some(parse_deadline) = self.parse_deadline {
            if parse_deadline >= Instant::now() {
                return true;
            }
        }
        false
    }
    pub fn startup_query_duration(&self) -> u64 {
        STARTUP_PARSE_DEADLINE_MS
    }
    pub fn parse(&mut self, mut raw_bytes: Vec<u8>) -> Vec<AnsiStdinInstruction> {
        for byte in raw_bytes.drain(..) {
            self.parse_byte(byte);
        }
        self.drain_pending_events()
    }
    pub fn read_cache(&self) -> Option<Vec<AnsiStdinInstruction>> {
        match OpenOptions::new()
            .read(true)
            .open(ZELLIJ_STDIN_CACHE_FILE.as_path())
        {
            Ok(mut file) => {
                let mut json_cache = String::new();
                file.read_to_string(&mut json_cache).ok()?;
                let instructions =
                    serde_json::from_str::<Vec<AnsiStdinInstruction>>(&json_cache).ok()?;
                if instructions.is_empty() {
                    None
                } else {
                    Some(instructions)
                }
            },
            Err(e) => {
                log::error!("Failed to open STDIN cache file: {:?}", e);
                None
            },
        }
    }
    pub fn write_cache(&self, events: Vec<AnsiStdinInstruction>) {
        if let Ok(serialized_events) = serde_json::to_string(&events) {
            if let Ok(mut file) = File::create(ZELLIJ_STDIN_CACHE_FILE.as_path()) {
                let _ = file.write_all(serialized_events.as_bytes());
            }
        };
    }
    fn parse_byte(&mut self, byte: u8) {
        if byte == b't' {
            self.raw_buffer.push(byte);
            match AnsiStdinInstruction::pixel_dimensions_from_bytes(&self.raw_buffer) {
                Ok(ansi_sequence) => {
                    self.pending_events.push(ansi_sequence);
                    self.raw_buffer.clear();
                },
                Err(_) => {
                    self.raw_buffer.clear();
                },
            }
        } else if byte == b'\\' {
            self.raw_buffer.push(byte);
            if let Ok(ansi_sequence) = AnsiStdinInstruction::bg_or_fg_from_bytes(&self.raw_buffer) {
                self.pending_events.push(ansi_sequence);
                self.raw_buffer.clear();
            } else if let Ok((color_register, color_sequence)) =
                color_sequence_from_bytes(&self.raw_buffer)
            {
                self.raw_buffer.clear();
                self.pending_color_sequences
                    .push((color_register, color_sequence));
            } else {
                self.raw_buffer.clear();
            }
        } else if byte == b'y' {
            self.raw_buffer.push(byte);
            if let Some(ansi_sequence) =
                AnsiStdinInstruction::synchronized_output_from_bytes(&self.raw_buffer)
            {
                self.pending_events.push(ansi_sequence);
                self.raw_buffer.clear();
            }
        } else {
            self.raw_buffer.push(byte);
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AnsiStdinInstruction {
    PixelDimensions(PixelDimensions),
    BackgroundColor(String),
    ForegroundColor(String),
    ColorRegisters(Vec<(usize, String)>),
    SynchronizedOutput(Option<SyncOutput>),
}

impl AnsiStdinInstruction {
    pub fn pixel_dimensions_from_bytes(bytes: &[u8]) -> Result<Self, &'static str> {
        // eg. <ESC>[4;21;8t
        lazy_static! {
            static ref RE: Regex = Regex::new(r"^\u{1b}\[(\d+);(\d+);(\d+)t$").unwrap();
        }
        let key_string = String::from_utf8_lossy(bytes); // TODO: handle error
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
                Ok(AnsiStdinInstruction::PixelDimensions(PixelDimensions {
                    character_cell_size: None,
                    text_area_size: Some(SizeInPixels {
                        height: first_field.unwrap(),
                        width: second_field.unwrap(),
                    }),
                }))
            },
            Ok(6) => {
                // character cell size
                Ok(AnsiStdinInstruction::PixelDimensions(PixelDimensions {
                    character_cell_size: Some(SizeInPixels {
                        height: first_field.unwrap(),
                        width: second_field.unwrap(),
                    }),
                    text_area_size: None,
                }))
            },
            _ => Err("invalid sequence"),
        }
    }
    pub fn bg_or_fg_from_bytes(bytes: &[u8]) -> Result<Self, &'static str> {
        // eg. <ESC>]11;rgb:0000/0000/0000\
        lazy_static! {
            static ref BACKGROUND_RE: Regex = Regex::new(r"\]11;(.*)\u{1b}\\$").unwrap();
        }
        // eg. <ESC>]10;rgb:ffff/ffff/ffff\
        lazy_static! {
            static ref FOREGROUND_RE: Regex = Regex::new(r"\]10;(.*)\u{1b}\\$").unwrap();
        }
        let key_string = String::from_utf8_lossy(bytes);
        if let Some(captures) = BACKGROUND_RE.captures_iter(&key_string).next() {
            let background_query_response = captures[1].parse::<String>();
            match background_query_response {
                Ok(background_query_response) => Ok(AnsiStdinInstruction::BackgroundColor(
                    background_query_response,
                )),
                _ => Err("invalid_instruction"),
            }
        } else if let Some(captures) = FOREGROUND_RE.captures_iter(&key_string).next() {
            let foreground_query_response = captures[1].parse::<String>();
            match foreground_query_response {
                Ok(foreground_query_response) => Ok(AnsiStdinInstruction::ForegroundColor(
                    foreground_query_response,
                )),
                _ => Err("invalid_instruction"),
            }
        } else {
            Err("invalid_instruction")
        }
    }
    pub fn color_registers_from_bytes(color_sequences: &mut Vec<(usize, String)>) -> Option<Self> {
        if color_sequences.is_empty() {
            return None;
        }
        let mut registers = vec![];
        for (color_register, color_sequence) in color_sequences.drain(..) {
            registers.push((color_register, color_sequence));
        }
        Some(AnsiStdinInstruction::ColorRegisters(registers))
    }

    pub fn synchronized_output_from_bytes(bytes: &[u8]) -> Option<Self> {
        lazy_static! {
            static ref RE: Regex = Regex::new(r"^\u{1b}\[\?2026;([0|1|2|3|4])\$y$").unwrap();
        }
        let key_string = String::from_utf8_lossy(bytes);
        if let Some(captures) = RE.captures_iter(&key_string).next() {
            match captures[1].parse::<usize>().ok()? {
                1 | 2 => Some(AnsiStdinInstruction::SynchronizedOutput(Some(
                    SyncOutput::CSI,
                ))),
                0 | 4 => Some(AnsiStdinInstruction::SynchronizedOutput(None)),
                _ => None,
            }
        } else {
            None
        }
    }
}

fn color_sequence_from_bytes(bytes: &[u8]) -> Result<(usize, String), &'static str> {
    lazy_static! {
        static ref COLOR_REGISTER_RE: Regex = Regex::new(r"\]4;(.*);(.*)\u{1b}\\$").unwrap();
    }
    lazy_static! {
        // this form is used by eg. Alacritty, where the leading 4 is dropped in the response
        static ref ALTERNATIVE_COLOR_REGISTER_RE: Regex = Regex::new(r"\](.*);(.*)\u{1b}\\$").unwrap();
    }
    let key_string = String::from_utf8_lossy(bytes);
    if let Some(captures) = COLOR_REGISTER_RE.captures_iter(&key_string).next() {
        let color_register_response = captures[1].parse::<usize>();
        let color_response = captures[2].parse::<String>();
        match (color_register_response, color_response) {
            (Ok(crr), Ok(cr)) => Ok((crr, cr)),
            _ => Err("invalid_instruction"),
        }
    } else if let Some(captures) = ALTERNATIVE_COLOR_REGISTER_RE
        .captures_iter(&key_string)
        .next()
    {
        let color_register_response = captures[1].parse::<usize>();
        let color_response = captures[2].parse::<String>();
        match (color_register_response, color_response) {
            (Ok(crr), Ok(cr)) => Ok((crr, cr)),
            _ => Err("invalid_instruction"),
        }
    } else {
        Err("invalid_instruction")
    }
}
