use std::str;

const PARAMETER_LIMIT: usize = 32;
const COLLECT_LIMIT: usize = 2;
const OSC_FIELD_LIMIT: usize = 16;
const FAST_SGR_LIMIT: usize = 31;
const ESC: u8 = 0x1b;

pub struct Sequence<'a> {
    parameters: &'a Parameters,
    intermediates: &'a [u8],
    overflowed: bool,
    final_byte: u8,
}

impl<'a> Sequence<'a> {
    pub fn new(
        parameters: &'a Parameters,
        intermediates: &'a [u8],
        overflowed: bool,
        final_byte: u8,
    ) -> Self {
        Sequence {
            parameters,
            intermediates,
            overflowed,
            final_byte,
        }
    }

    pub fn parameters(&self) -> &'a Parameters {
        self.parameters
    }

    pub fn intermediates(&self) -> &'a [u8] {
        self.intermediates
    }

    pub fn overflowed(&self) -> bool {
        self.overflowed
    }

    pub fn final_byte(&self) -> u8 {
        self.final_byte
    }

    pub fn final_char(&self) -> char {
        self.final_byte as char
    }
}

pub trait Handler {
    fn print(&mut self, _character: char) {}

    fn print_str(&mut self, text: &str) {
        text.chars().for_each(|character| self.print(character));
    }

    fn print_ascii(&mut self, text: &str) {
        self.print_str(text);
    }

    fn sgr(&mut self, parameter_bytes: &[u8]) {
        let parameters = Parameters::from_parameter_bytes(parameter_bytes);
        self.csi_dispatch(&Sequence::new(&parameters, &[], false, b'm'));
    }

    fn execute(&mut self, _control: u8) {}

    fn csi_dispatch(&mut self, _sequence: &Sequence) {}

    fn esc_dispatch(&mut self, _sequence: &Sequence) {}

    fn hook(&mut self, _sequence: &Sequence) {}

    fn put(&mut self, _byte: u8) {}

    fn unhook(&mut self) {}

    fn osc_end(&mut self, _fields: &[&[u8]], _ended_by_bell: bool) {}

    fn paused(&self) -> bool {
        false
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Parameters {
    values: [u16; PARAMETER_LIMIT],
    group_lengths: [u8; PARAMETER_LIMIT],
    value_total: usize,
    group_total: usize,
    open_group: u8,
}

impl Parameters {
    pub fn from_parameter_bytes(bytes: &[u8]) -> Parameters {
        let mut parameters = Parameters::default();
        let mut number: u16 = 0;
        for &byte in bytes {
            if parameters.at_limit() {
                continue;
            }
            match byte {
                b'0'..=b'9' => number = accumulate(number, byte),
                b':' => {
                    parameters.continue_group(number);
                    number = 0;
                },
                b';' => {
                    parameters.end_group(number);
                    number = 0;
                },
                _ => {},
            }
        }
        parameters.finish(number);
        parameters
    }

    pub fn value_count(&self) -> usize {
        self.value_total
    }

    pub fn is_empty(&self) -> bool {
        self.value_total == 0
    }

    #[inline]
    pub fn iter(&self) -> ParameterGroups<'_> {
        ParameterGroups {
            values: &self.values[..self.value_total],
            lengths: &self.group_lengths[..self.group_total],
        }
    }

    pub(crate) fn at_limit(&self) -> bool {
        self.value_total == PARAMETER_LIMIT
    }

    pub(crate) fn reset(&mut self) {
        self.value_total = 0;
        self.group_total = 0;
        self.open_group = 0;
    }

    #[inline(always)]
    pub(crate) fn end_group(&mut self, value: u16) {
        self.continue_group(value);
        self.seal_group();
    }

    #[inline(always)]
    pub(crate) fn seal_group(&mut self) {
        if self.open_group > 0 {
            self.group_lengths[self.group_total] = self.open_group;
            self.group_total += 1;
            self.open_group = 0;
        }
    }

    #[inline(always)]
    pub(crate) fn continue_group(&mut self, value: u16) {
        self.values[self.value_total] = value;
        self.value_total += 1;
        self.open_group += 1;
    }

    #[inline(always)]
    fn finish(&mut self, number: u16) {
        if self.at_limit() {
            self.seal_group();
        } else {
            self.end_group(number);
        }
    }
}

impl<'a> IntoIterator for &'a Parameters {
    type IntoIter = ParameterGroups<'a>;
    type Item = &'a [u16];

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl std::ops::Index<usize> for Parameters {
    type Output = [u16];

    fn index(&self, group: usize) -> &[u16] {
        self.iter()
            .nth(group)
            .expect("parameter group out of range")
    }
}

pub struct ParameterGroups<'a> {
    values: &'a [u16],
    lengths: &'a [u8],
}

impl<'a> Iterator for ParameterGroups<'a> {
    type Item = &'a [u16];

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        let (&length, lengths) = self.lengths.split_first()?;
        let (group, values) = self.values.split_at(length as usize);
        self.lengths = lengths;
        self.values = values;
        Some(group)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.lengths.len(), Some(self.lengths.len()))
    }
}

#[inline(always)]
fn accumulate(number: u16, digit: u8) -> u16 {
    number
        .saturating_mul(10)
        .saturating_add((digit - b'0') as u16)
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum State {
    #[default]
    Ground,
    Escape,
    EscapeIntermediate,
    CsiEntry,
    CsiParam,
    CsiIntermediate,
    CsiIgnore,
    DcsEntry,
    DcsParam,
    DcsIntermediate,
    DcsPassthrough,
    DcsIgnore,
    OscString,
    IgnoredString,
}

#[derive(Clone, Default)]
pub struct Parser {
    state: State,
    parameters: Parameters,
    number: u16,
    collected: [u8; COLLECT_LIMIT],
    collected_count: usize,
    overflowed: bool,
    osc: Vec<u8>,
    carry: [u8; 3],
    carry_len: usize,
    without_shortcuts: bool,
}

impl Parser {
    pub fn new() -> Self {
        Self::default()
    }

    #[cfg(test)]
    pub(crate) fn without_shortcuts() -> Self {
        Parser {
            without_shortcuts: true,
            ..Parser::default()
        }
    }

    pub fn feed<H: Handler>(&mut self, handler: &mut H, bytes: &[u8]) {
        self.run(handler, bytes, false);
    }

    pub fn feed_until_paused<H: Handler>(&mut self, handler: &mut H, bytes: &[u8]) -> usize {
        self.run(handler, bytes, true)
    }

    fn run<H: Handler>(&mut self, handler: &mut H, bytes: &[u8], stop: bool) -> usize {
        let scan = !self.without_shortcuts;
        let mut at = match self.carry_len {
            0 => 0,
            _ => self.resume_character(handler, bytes, scan),
        };
        while at < bytes.len() && !(stop && handler.paused()) {
            let rest = &bytes[at..];
            at += match self.state {
                State::Ground => self.ground(handler, rest, scan, stop),
                State::CsiEntry | State::CsiParam if scan => self.csi_numbers(handler, rest),
                _ => {
                    self.step(handler, rest[0]);
                    1
                },
            };
        }
        at
    }

    fn resume_character<H: Handler>(&mut self, handler: &mut H, bytes: &[u8], scan: bool) -> usize {
        let lead = self.carry[0];
        let width = utf8_width(lead);
        let mut unit = [0u8; 4];
        let mut filled = self.carry_len;
        unit[..filled].copy_from_slice(&self.carry[..filled]);
        let mut taken = 0;
        while filled < width {
            let Some(&byte) = bytes.get(taken) else {
                self.carry[..filled].copy_from_slice(&unit[..filled]);
                self.carry_len = filled;
                return taken;
            };
            if !continues_utf8(lead, filled, byte) {
                self.carry_len = 0;
                handler.print(char::REPLACEMENT_CHARACTER);
                return taken;
            }
            unit[filled] = byte;
            filled += 1;
            taken += 1;
        }
        self.carry_len = 0;
        match str::from_utf8(&unit[..width]) {
            Ok(character) => dispatch_text(handler, character, scan),
            Err(_) => handler.print(char::REPLACEMENT_CHARACTER),
        }
        taken
    }

    #[inline(always)]
    fn csi_numbers<H: Handler>(&mut self, handler: &mut H, bytes: &[u8]) -> usize {
        let mut used = 0;
        while let Some(&byte) = bytes.get(used) {
            used += 1;
            match byte {
                b'0'..=b'9' => self.digit(byte),
                b':' => self.subparameter(),
                b';' => self.parameter(),
                0x40..=0x7e => {
                    self.csi_dispatch(handler, byte);
                    return used;
                },
                _ => {
                    used -= 1;
                    break;
                },
            }
            self.state = State::CsiParam;
        }
        if used > 0 {
            return used;
        }
        self.step(handler, bytes[0]);
        1
    }

    #[inline(always)]
    fn ground<H: Handler>(
        &mut self,
        handler: &mut H,
        bytes: &[u8],
        scan: bool,
        stop: bool,
    ) -> usize {
        let text_end = memchr::memchr(ESC, bytes).unwrap_or(bytes.len());
        let mut remaining = &bytes[..text_end];
        if scan && remaining.is_ascii() {
            dispatch_ascii(handler, unsafe { str::from_utf8_unchecked(remaining) });
            remaining = &[];
        }
        while !remaining.is_empty() {
            let failure = str::from_utf8(remaining).err();
            let valid_len = failure.map_or(remaining.len(), |failure| failure.valid_up_to());
            dispatch_text(
                handler,
                unsafe { str::from_utf8_unchecked(&remaining[..valid_len]) },
                scan,
            );
            let broken = &remaining[valid_len..];
            remaining = match failure.and_then(|failure| failure.error_len()) {
                Some(broken_len) => {
                    report_invalid(handler, broken[0], broken_len);
                    &broken[broken_len..]
                },
                None if broken.is_empty() => broken,
                None if text_end < bytes.len() => {
                    handler.print(char::REPLACEMENT_CHARACTER);
                    &[]
                },
                None => {
                    self.carry[..broken.len()].copy_from_slice(broken);
                    self.carry_len = broken.len();
                    &[]
                },
            };
            if stop && !remaining.is_empty() && handler.paused() {
                return text_end - remaining.len();
            }
        }
        match bytes.get(text_end..) {
            Some(escape) if !escape.is_empty() => {
                text_end + self.at_escape(handler, escape, scan, stop)
            },
            _ => text_end,
        }
    }

    #[inline(always)]
    fn at_escape<H: Handler>(
        &mut self,
        handler: &mut H,
        bytes: &[u8],
        scan: bool,
        stop: bool,
    ) -> usize {
        if scan && !(stop && handler.paused()) {
            if let Some(length) = fast_sgr_length(bytes) {
                handler.sgr(&bytes[2..2 + length]);
                return length + 3;
            }
        }
        self.enter_escape();
        1
    }

    #[inline(always)]
    fn enter_escape(&mut self) {
        self.state = State::Escape;
        self.parameters.reset();
        self.number = 0;
        self.collected_count = 0;
        self.overflowed = false;
    }

    #[inline(always)]
    fn collect(&mut self, byte: u8) {
        match self.collected.get_mut(self.collected_count) {
            Some(slot) => {
                *slot = byte;
                self.collected_count += 1;
            },
            None => self.overflowed = true,
        }
    }

    #[inline(always)]
    fn digit(&mut self, byte: u8) {
        match self.parameters.at_limit() {
            true => self.overflowed = true,
            false => self.number = accumulate(self.number, byte),
        }
    }

    #[inline(always)]
    fn parameter(&mut self) {
        match self.parameters.at_limit() {
            true => self.overflowed = true,
            false => self.parameters.end_group(std::mem::take(&mut self.number)),
        }
    }

    #[inline(always)]
    fn subparameter(&mut self) {
        match self.parameters.at_limit() {
            true => self.overflowed = true,
            false => self
                .parameters
                .continue_group(std::mem::take(&mut self.number)),
        }
    }

    #[inline(always)]
    fn sequence_end(&mut self, byte: u8) -> Sequence<'_> {
        if self.parameters.at_limit() {
            self.overflowed = true;
        }
        self.parameters.finish(self.number);
        Sequence::new(
            &self.parameters,
            &self.collected[..self.collected_count],
            self.overflowed,
            byte,
        )
    }

    #[inline(always)]
    fn csi_dispatch<H: Handler>(&mut self, handler: &mut H, byte: u8) {
        handler.csi_dispatch(&self.sequence_end(byte));
        self.state = State::Ground;
    }

    #[inline(always)]
    fn hook<H: Handler>(&mut self, handler: &mut H, byte: u8) {
        handler.hook(&self.sequence_end(byte));
        self.state = State::DcsPassthrough;
    }

    #[inline(always)]
    fn esc_dispatch<H: Handler>(&mut self, handler: &mut H, byte: u8) {
        handler.esc_dispatch(&Sequence::new(
            &self.parameters,
            &self.collected[..self.collected_count],
            self.overflowed,
            byte,
        ));
        self.state = State::Ground;
    }

    fn osc_start(&mut self) {
        self.osc.clear();
        self.state = State::OscString;
    }

    fn osc_finish<H: Handler>(&self, handler: &mut H, ended_by_bell: bool) {
        let mut fields: [&[u8]; OSC_FIELD_LIMIT] = [&[]; OSC_FIELD_LIMIT];
        let mut count = 0;
        for (slot, field) in fields.iter_mut().zip(self.osc.split(|&byte| byte == b';')) {
            *slot = field;
            count += 1;
        }
        handler.osc_end(&fields[..count], ended_by_bell);
    }

    #[inline(always)]
    fn step<H: Handler>(&mut self, handler: &mut H, byte: u8) {
        match self.state {
            State::OscString => return self.osc_byte(handler, byte),
            State::DcsPassthrough => return self.passthrough_byte(handler, byte),
            _ => {},
        }
        match byte {
            0x18 | 0x1a => {
                handler.execute(byte);
                self.state = State::Ground;
                return;
            },
            ESC => {
                self.enter_escape();
                return;
            },
            _ => {},
        }
        let c0 = matches!(byte, 0x00..=0x17 | 0x19 | 0x1c..=0x1f);
        match self.state {
            State::Ground => unreachable!(),
            State::Escape => match byte {
                _ if c0 => handler.execute(byte),
                0x20..=0x2f => {
                    self.collect(byte);
                    self.state = State::EscapeIntermediate;
                },
                b'[' => self.state = State::CsiEntry,
                b']' => self.osc_start(),
                b'P' => self.state = State::DcsEntry,
                b'X' | b'^' | b'_' => self.state = State::IgnoredString,
                0x30..=0x7e => self.esc_dispatch(handler, byte),
                _ => {},
            },
            State::EscapeIntermediate => match byte {
                _ if c0 => handler.execute(byte),
                0x20..=0x2f => self.collect(byte),
                0x30..=0x7e => self.esc_dispatch(handler, byte),
                _ => {},
            },
            State::CsiEntry => match byte {
                _ if c0 => handler.execute(byte),
                0x20..=0x2f => {
                    self.collect(byte);
                    self.state = State::CsiIntermediate;
                },
                b'0'..=b'9' => {
                    self.digit(byte);
                    self.state = State::CsiParam;
                },
                b':' => {
                    self.subparameter();
                    self.state = State::CsiParam;
                },
                b';' => {
                    self.parameter();
                    self.state = State::CsiParam;
                },
                0x3c..=0x3f => {
                    self.collect(byte);
                    self.state = State::CsiParam;
                },
                0x40..=0x7e => self.csi_dispatch(handler, byte),
                _ => {},
            },
            State::CsiParam => match byte {
                _ if c0 => handler.execute(byte),
                0x20..=0x2f => {
                    self.collect(byte);
                    self.state = State::CsiIntermediate;
                },
                b'0'..=b'9' => self.digit(byte),
                b':' => self.subparameter(),
                b';' => self.parameter(),
                0x3c..=0x3f => self.state = State::CsiIgnore,
                0x40..=0x7e => self.csi_dispatch(handler, byte),
                _ => {},
            },
            State::CsiIntermediate => match byte {
                _ if c0 => handler.execute(byte),
                0x20..=0x2f => self.collect(byte),
                0x30..=0x3f => self.state = State::CsiIgnore,
                0x40..=0x7e => self.csi_dispatch(handler, byte),
                _ => {},
            },
            State::CsiIgnore => match byte {
                _ if c0 => handler.execute(byte),
                0x40..=0x7e => self.state = State::Ground,
                _ => {},
            },
            State::DcsEntry => match byte {
                0x20..=0x2f => {
                    self.collect(byte);
                    self.state = State::DcsIntermediate;
                },
                b'0'..=b'9' => {
                    self.digit(byte);
                    self.state = State::DcsParam;
                },
                b':' => {
                    self.subparameter();
                    self.state = State::DcsParam;
                },
                b';' => {
                    self.parameter();
                    self.state = State::DcsParam;
                },
                0x3c..=0x3f => {
                    self.collect(byte);
                    self.state = State::DcsParam;
                },
                0x40..=0x7e => self.hook(handler, byte),
                _ => {},
            },
            State::DcsParam => match byte {
                0x20..=0x2f => {
                    self.collect(byte);
                    self.state = State::DcsIntermediate;
                },
                b'0'..=b'9' => self.digit(byte),
                b':' => self.subparameter(),
                b';' => self.parameter(),
                0x3c..=0x3f => self.state = State::DcsIgnore,
                0x40..=0x7e => self.hook(handler, byte),
                _ => {},
            },
            State::DcsIntermediate => match byte {
                0x20..=0x2f => self.collect(byte),
                0x30..=0x3f => self.state = State::DcsIgnore,
                0x40..=0x7e => self.hook(handler, byte),
                _ => {},
            },
            State::DcsIgnore | State::IgnoredString => {},
            State::OscString | State::DcsPassthrough => unreachable!(),
        }
    }

    #[inline(always)]
    fn osc_byte<H: Handler>(&mut self, handler: &mut H, byte: u8) {
        match byte {
            0x07 => {
                self.osc_finish(handler, true);
                self.state = State::Ground;
            },
            0x18 | 0x1a => {
                self.osc_finish(handler, false);
                handler.execute(byte);
                self.state = State::Ground;
            },
            ESC => {
                self.osc_finish(handler, false);
                self.enter_escape();
            },
            0x00..=0x1f => {},
            _ => self.osc.push(byte),
        }
    }

    #[inline(always)]
    fn passthrough_byte<H: Handler>(&mut self, handler: &mut H, byte: u8) {
        match byte {
            0x18 | 0x1a => {
                handler.unhook();
                handler.execute(byte);
                self.state = State::Ground;
            },
            ESC => {
                handler.unhook();
                self.enter_escape();
            },
            0x9c => {
                handler.unhook();
                self.state = State::Ground;
            },
            0x7f => {},
            0x00..=0x7e => handler.put(byte),
            _ => {},
        }
    }
}

fn utf8_width(lead: u8) -> usize {
    match lead {
        0xc2..=0xdf => 2,
        0xe0..=0xef => 3,
        0xf0..=0xf4 => 4,
        _ => 1,
    }
}

fn continues_utf8(lead: u8, position: usize, byte: u8) -> bool {
    let (low, high) = match (position, lead) {
        (1, 0xe0) => (0xa0, 0xbf),
        (1, 0xed) => (0x80, 0x9f),
        (1, 0xf0) => (0x90, 0xbf),
        (1, 0xf4) => (0x80, 0x8f),
        _ => (0x80, 0xbf),
    };
    (low..=high).contains(&byte)
}

fn report_invalid<H: Handler>(handler: &mut H, first: u8, broken_len: usize) {
    match (broken_len, first) {
        (1, 0x00..=0x9f) => handler.execute(first),
        _ => handler.print(char::REPLACEMENT_CHARACTER),
    }
}

#[inline(always)]
fn fast_sgr_length(bytes: &[u8]) -> Option<usize> {
    if bytes.get(1) != Some(&b'[') {
        return None;
    }
    let window = &bytes[2..bytes.len().min(2 + FAST_SGR_LIMIT + 1)];
    window
        .iter()
        .position(|byte| !matches!(byte, b'0'..=b'9' | b';' | b':'))
        .filter(|&end| window[end] == b'm')
}

#[inline(always)]
fn read_word(bytes: &[u8], at: usize) -> u64 {
    let chunk: &[u8; 8] = bytes[at..at + 8].try_into().unwrap_or(&[0; 8]);
    u64::from_le_bytes(*chunk)
}

const ONES: u64 = 0x0101_0101_0101_0101;
const HIGHS: u64 = 0x8080_8080_8080_8080;

#[inline(always)]
fn below_space_lanes(word: u64) -> u64 {
    word.wrapping_sub(ONES * 0x20) & !word
}

#[inline(always)]
fn matching_lanes(word: u64, byte: u8) -> u64 {
    let matched = word ^ (ONES * byte as u64);
    matched.wrapping_sub(ONES) & !matched
}

#[inline(always)]
fn dispatch_ascii<H: Handler>(handler: &mut H, text: &str) {
    let bytes = text.as_bytes();
    let mut run_start = 0;
    let mut at = 0;
    while at < bytes.len() {
        if at + 8 <= bytes.len() {
            let word = read_word(bytes, at);
            if (below_space_lanes(word) | matching_lanes(word, 0x7f)) & HIGHS == 0 {
                at += 8;
                continue;
            }
        }
        let byte = bytes[at];
        if byte < 0x20 || byte == 0x7f {
            if run_start < at {
                handler.print_ascii(&text[run_start..at]);
            }
            match byte {
                0x7f => handler.print_str(&text[at..at + 1]),
                _ => handler.execute(byte),
            }
            run_start = at + 1;
        }
        at += 1;
    }
    if run_start < bytes.len() {
        handler.print_ascii(&text[run_start..]);
    }
}

#[inline(always)]
fn dispatch_text<H: Handler>(handler: &mut H, text: &str, scan: bool) {
    let bytes = text.as_bytes();
    let mut run_start = 0;
    let mut at = 0;
    while at < bytes.len() {
        if scan && at + 8 <= bytes.len() {
            let word = read_word(bytes, at);
            if (below_space_lanes(word) | matching_lanes(word, 0xc2)) & HIGHS == 0 {
                at += 8;
                continue;
            }
        }
        let control = match bytes[at] {
            byte if byte < 0x20 => Some((byte, 1)),
            0xc2 => match bytes.get(at + 1) {
                Some(&next @ 0x80..=0x9f) => Some((next, 2)),
                _ => None,
            },
            _ => None,
        };
        match control {
            Some((code, length)) => {
                if run_start < at {
                    handler.print_str(&text[run_start..at]);
                }
                handler.execute(code);
                at += length;
                run_start = at;
            },
            None => at += 1,
        }
    }
    if run_start < bytes.len() {
        handler.print_str(&text[run_start..]);
    }
}

#[cfg(test)]
#[path = "./unit/vt_parser_tests.rs"]
mod vt_parser_tests;
