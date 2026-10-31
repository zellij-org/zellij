//! Parser for implementing virtual terminal emulators
//!
//! [`Parser`] is implemented according to [Paul Williams' ANSI parser state
//! machine]. The state machine doesn't assign meaning to the parsed data and is
//! thus not itself sufficient for writing a terminal emulator. Instead, it is
//! expected that an implementation of [`Perform`] is provided which does
//! something useful with the parsed data. The [`Parser`] handles the book
//! keeping, and the [`Perform`] gets to simply handle actions.
//!
//! # Examples
//!
//! For an example of using the [`Parser`] please see the examples folder. The
//! example included there simply logs all the actions [`Perform`] does. One
//! quick way to see it in action is to pipe `printf` into it
//!
//! ```sh
//! printf '\x1b[31mExample' | cargo run --example parselog
//! ```
//!
//! # Differences from original state machine description
//!
//! * UTF-8 Support for Input
//! * OSC Strings can be terminated by 0x07
//! * Only supports 7-bit codes
//!
//! [`Parser`]: struct.Parser.html
//! [`Perform`]: trait.Perform.html
//! [Paul Williams' ANSI parser state machine]: https://vt100.net/emu/dec_ansi_parser

use core::mem::MaybeUninit;
use core::str;

#[cfg(any())]
use arrayvec::ArrayVec;

pub use crate::vt_parser::{Handler as Perform, Parameters as Params, Sequence};

const MAX_INTERMEDIATES: usize = 2;
const MAX_OSC_PARAMS: usize = 16;
const MAX_OSC_RAW: usize = 1024;

/// Parser for raw _VTE_ protocol which delegates actions to a [`Perform`]
///
/// [`Perform`]: trait.Perform.html
///
/// Generic over the value for the size of the raw Operating System Command
/// buffer. Only used when the `std` feature is not enabled.
#[derive(Default)]
pub struct Parser<const OSC_RAW_BUF_SIZE: usize = MAX_OSC_RAW> {
    state: State,
    intermediates: [u8; MAX_INTERMEDIATES],
    intermediate_idx: usize,
    params: Params,
    param: u16,
    #[cfg(any())]
    osc_raw: ArrayVec<u8, OSC_RAW_BUF_SIZE>,
    #[cfg(all())]
    osc_raw: Vec<u8>,
    osc_params: [(usize, usize); MAX_OSC_PARAMS],
    osc_num_params: usize,
    ignoring: bool,
    partial_utf8: [u8; 4],
    partial_utf8_len: usize,
}

impl Parser {
    /// Create a new Parser
    pub fn new() -> Parser {
        Default::default()
    }
}

impl<const OSC_RAW_BUF_SIZE: usize> Parser<OSC_RAW_BUF_SIZE> {
    /// Create a new Parser with a custom size for the Operating System Command
    /// buffer.
    ///
    /// Call with a const-generic param on `Parser`, like:
    ///
    /// ```ignore
    /// let mut p = vte::Parser::<64>::new_with_size();
    /// ```
    #[cfg(any())]
    pub fn new_with_size() -> Parser<OSC_RAW_BUF_SIZE> {
        Default::default()
    }

    #[inline]
    pub fn advance<P: Perform>(&mut self, performer: &mut P, bytes: &[u8]) {
        let mut i = 0;

        // Handle partial codepoints from previous calls to `advance`.
        if self.partial_utf8_len != 0 {
            i += self.advance_partial_utf8(performer, bytes);
        }

        while i != bytes.len() {
            match self.state {
                State::Ground => i += self.advance_ground(performer, &bytes[i..]),
                _ => {
                    // Inlining it results in worse codegen.
                    let byte = bytes[i];
                    self.change_state(performer, byte);
                    i += 1;
                },
            }
        }
    }

    /// Partially advance the parser state.
    ///
    /// This is equivalent to [`Self::advance`], but stops when
    /// [`Perform::terminated`] is true after reading a byte.
    ///
    /// Returns the number of bytes read before termination.
    ///
    /// See [`Perform::advance`] for more details.
    #[inline]
    #[must_use = "Returned value should be used to processs the remaining bytes"]
    pub fn advance_until_terminated<P: Perform>(
        &mut self,
        performer: &mut P,
        bytes: &[u8],
    ) -> usize {
        let mut i = 0;

        // Handle partial codepoints from previous calls to `advance`.
        if self.partial_utf8_len != 0 {
            i += self.advance_partial_utf8(performer, bytes);
        }

        while i != bytes.len() && !performer.paused() {
            match self.state {
                State::Ground => i += self.advance_ground(performer, &bytes[i..]),
                State::CsiEntry | State::CsiParam => {
                    let start = i;
                    while i < bytes.len() {
                        let byte = bytes[i];
                        match byte {
                            0x30..=0x39 => self.action_paramnext(byte),
                            0x3A => self.action_subparam(),
                            0x3B => self.action_param(),
                            0x40..=0x7E => {
                                i += 1;
                                self.action_csi_dispatch(performer, byte);
                                break;
                            },
                            _ => break,
                        }
                        self.state = State::CsiParam;
                        i += 1;
                    }
                    if i == start {
                        let byte = bytes[i];
                        self.change_state(performer, byte);
                        i += 1;
                    }
                },
                _ => {
                    // Inlining it results in worse codegen.
                    let byte = bytes[i];
                    self.change_state(performer, byte);
                    i += 1;
                },
            }
        }

        i
    }

    #[inline(always)]
    fn change_state<P: Perform>(&mut self, performer: &mut P, byte: u8) {
        match self.state {
            State::CsiEntry => self.advance_csi_entry(performer, byte),
            State::CsiIgnore => self.advance_csi_ignore(performer, byte),
            State::CsiIntermediate => self.advance_csi_intermediate(performer, byte),
            State::CsiParam => self.advance_csi_param(performer, byte),
            State::DcsEntry => self.advance_dcs_entry(performer, byte),
            State::DcsIgnore => self.anywhere(performer, byte),
            State::DcsIntermediate => self.advance_dcs_intermediate(performer, byte),
            State::DcsParam => self.advance_dcs_param(performer, byte),
            State::DcsPassthrough => self.advance_dcs_passthrough(performer, byte),
            State::Escape => self.advance_esc(performer, byte),
            State::EscapeIntermediate => self.advance_esc_intermediate(performer, byte),
            State::OscString => self.advance_osc_string(performer, byte),
            State::SosPmApcString => self.anywhere(performer, byte),
            State::Ground => unreachable!(),
        }
    }

    #[inline(always)]
    fn advance_csi_entry<P: Perform>(&mut self, performer: &mut P, byte: u8) {
        match byte {
            0x00..=0x17 | 0x19 | 0x1C..=0x1F => performer.execute(byte),
            0x20..=0x2F => {
                self.action_collect(byte);
                self.state = State::CsiIntermediate
            },
            0x30..=0x39 => {
                self.action_paramnext(byte);
                self.state = State::CsiParam
            },
            0x3A => {
                self.action_subparam();
                self.state = State::CsiParam
            },
            0x3B => {
                self.action_param();
                self.state = State::CsiParam
            },
            0x3C..=0x3F => {
                self.action_collect(byte);
                self.state = State::CsiParam
            },
            0x40..=0x7E => self.action_csi_dispatch(performer, byte),
            _ => self.anywhere(performer, byte),
        }
    }

    #[inline(always)]
    fn advance_csi_ignore<P: Perform>(&mut self, performer: &mut P, byte: u8) {
        match byte {
            0x00..=0x17 | 0x19 | 0x1C..=0x1F => performer.execute(byte),
            0x20..=0x3F => (),
            0x40..=0x7E => self.state = State::Ground,
            0x7F => (),
            _ => self.anywhere(performer, byte),
        }
    }

    #[inline(always)]
    fn advance_csi_intermediate<P: Perform>(&mut self, performer: &mut P, byte: u8) {
        match byte {
            0x00..=0x17 | 0x19 | 0x1C..=0x1F => performer.execute(byte),
            0x20..=0x2F => self.action_collect(byte),
            0x30..=0x3F => self.state = State::CsiIgnore,
            0x40..=0x7E => self.action_csi_dispatch(performer, byte),
            _ => self.anywhere(performer, byte),
        }
    }

    #[inline(always)]
    fn advance_csi_param<P: Perform>(&mut self, performer: &mut P, byte: u8) {
        match byte {
            0x00..=0x17 | 0x19 | 0x1C..=0x1F => performer.execute(byte),
            0x20..=0x2F => {
                self.action_collect(byte);
                self.state = State::CsiIntermediate
            },
            0x30..=0x39 => self.action_paramnext(byte),
            0x3A => self.action_subparam(),
            0x3B => self.action_param(),
            0x3C..=0x3F => self.state = State::CsiIgnore,
            0x40..=0x7E => self.action_csi_dispatch(performer, byte),
            0x7F => (),
            _ => self.anywhere(performer, byte),
        }
    }

    #[inline(always)]
    fn advance_dcs_entry<P: Perform>(&mut self, performer: &mut P, byte: u8) {
        match byte {
            0x00..=0x17 | 0x19 | 0x1C..=0x1F => (),
            0x20..=0x2F => {
                self.action_collect(byte);
                self.state = State::DcsIntermediate
            },
            0x30..=0x39 => {
                self.action_paramnext(byte);
                self.state = State::DcsParam
            },
            0x3A => {
                self.action_subparam();
                self.state = State::DcsParam
            },
            0x3B => {
                self.action_param();
                self.state = State::DcsParam
            },
            0x3C..=0x3F => {
                self.action_collect(byte);
                self.state = State::DcsParam
            },
            0x40..=0x7E => self.action_hook(performer, byte),
            0x7F => (),
            _ => self.anywhere(performer, byte),
        }
    }

    #[inline(always)]
    fn advance_dcs_intermediate<P: Perform>(&mut self, performer: &mut P, byte: u8) {
        match byte {
            0x00..=0x17 | 0x19 | 0x1C..=0x1F => (),
            0x20..=0x2F => self.action_collect(byte),
            0x30..=0x3F => self.state = State::DcsIgnore,
            0x40..=0x7E => self.action_hook(performer, byte),
            0x7F => (),
            _ => self.anywhere(performer, byte),
        }
    }

    #[inline(always)]
    fn advance_dcs_param<P: Perform>(&mut self, performer: &mut P, byte: u8) {
        match byte {
            0x00..=0x17 | 0x19 | 0x1C..=0x1F => (),
            0x20..=0x2F => {
                self.action_collect(byte);
                self.state = State::DcsIntermediate
            },
            0x30..=0x39 => self.action_paramnext(byte),
            0x3A => self.action_subparam(),
            0x3B => self.action_param(),
            0x3C..=0x3F => self.state = State::DcsIgnore,
            0x40..=0x7E => self.action_hook(performer, byte),
            0x7F => (),
            _ => self.anywhere(performer, byte),
        }
    }

    #[inline(always)]
    fn advance_dcs_passthrough<P: Perform>(&mut self, performer: &mut P, byte: u8) {
        match byte {
            0x00..=0x17 | 0x19 | 0x1C..=0x7E => performer.put(byte),
            0x18 | 0x1A => {
                performer.unhook();
                performer.execute(byte);
                self.state = State::Ground
            },
            0x1B => {
                performer.unhook();
                self.reset_params();
                self.state = State::Escape
            },
            0x7F => (),
            0x9C => {
                performer.unhook();
                self.state = State::Ground
            },
            _ => (),
        }
    }

    #[inline(always)]
    fn advance_esc<P: Perform>(&mut self, performer: &mut P, byte: u8) {
        match byte {
            0x00..=0x17 | 0x19 | 0x1C..=0x1F => performer.execute(byte),
            0x20..=0x2F => {
                self.action_collect(byte);
                self.state = State::EscapeIntermediate
            },
            0x30..=0x4F => {
                performer.esc_dispatch(&Sequence::new(
                    &self.params,
                    &self.intermediates[..self.intermediate_idx],
                    self.ignoring,
                    byte,
                ));
                self.state = State::Ground
            },
            0x50 => {
                self.reset_params();
                self.state = State::DcsEntry
            },
            0x51..=0x57 => {
                performer.esc_dispatch(&Sequence::new(
                    &self.params,
                    &self.intermediates[..self.intermediate_idx],
                    self.ignoring,
                    byte,
                ));
                self.state = State::Ground
            },
            0x58 => self.state = State::SosPmApcString,
            0x59..=0x5A => {
                performer.esc_dispatch(&Sequence::new(
                    &self.params,
                    &self.intermediates[..self.intermediate_idx],
                    self.ignoring,
                    byte,
                ));
                self.state = State::Ground
            },
            0x5B => {
                self.reset_params();
                self.state = State::CsiEntry
            },
            0x5C => {
                performer.esc_dispatch(&Sequence::new(
                    &self.params,
                    &self.intermediates[..self.intermediate_idx],
                    self.ignoring,
                    byte,
                ));
                self.state = State::Ground
            },
            0x5D => {
                self.osc_raw.clear();
                self.osc_num_params = 0;
                self.state = State::OscString
            },
            0x5E..=0x5F => self.state = State::SosPmApcString,
            0x60..=0x7E => {
                performer.esc_dispatch(&Sequence::new(
                    &self.params,
                    &self.intermediates[..self.intermediate_idx],
                    self.ignoring,
                    byte,
                ));
                self.state = State::Ground
            },
            // Anywhere.
            0x18 | 0x1A => {
                performer.execute(byte);
                self.state = State::Ground
            },
            0x1B => (),
            _ => (),
        }
    }

    #[inline(always)]
    fn advance_esc_intermediate<P: Perform>(&mut self, performer: &mut P, byte: u8) {
        match byte {
            0x00..=0x17 | 0x19 | 0x1C..=0x1F => performer.execute(byte),
            0x20..=0x2F => self.action_collect(byte),
            0x30..=0x7E => {
                performer.esc_dispatch(&Sequence::new(
                    &self.params,
                    &self.intermediates[..self.intermediate_idx],
                    self.ignoring,
                    byte,
                ));
                self.state = State::Ground
            },
            0x7F => (),
            _ => self.anywhere(performer, byte),
        }
    }

    #[inline(always)]
    fn advance_osc_string<P: Perform>(&mut self, performer: &mut P, byte: u8) {
        match byte {
            0x00..=0x06 | 0x08..=0x17 | 0x19 | 0x1C..=0x1F => (),
            0x07 => {
                self.osc_end(performer, byte);
                self.state = State::Ground
            },
            0x18 | 0x1A => {
                self.osc_end(performer, byte);
                performer.execute(byte);
                self.state = State::Ground
            },
            0x1B => {
                self.osc_end(performer, byte);
                self.reset_params();
                self.state = State::Escape
            },
            0x3B => {
                #[cfg(any())]
                {
                    if self.osc_raw.is_full() {
                        return;
                    }
                }
                self.action_osc_put_param()
            },
            _ => self.action_osc_put(byte),
        }
    }

    #[inline(always)]
    fn anywhere<P: Perform>(&mut self, performer: &mut P, byte: u8) {
        match byte {
            0x18 | 0x1A => {
                performer.execute(byte);
                self.state = State::Ground
            },
            0x1B => {
                self.reset_params();
                self.state = State::Escape
            },
            _ => (),
        }
    }

    #[inline]
    fn action_csi_dispatch<P: Perform>(&mut self, performer: &mut P, byte: u8) {
        if self.params.at_limit() {
            self.ignoring = true;
            self.params.seal_group();
        } else {
            self.params.end_group(self.param);
        }
        performer.csi_dispatch(&Sequence::new(
            &self.params,
            &self.intermediates[..self.intermediate_idx],
            self.ignoring,
            byte,
        ));

        self.state = State::Ground
    }

    #[inline]
    fn action_hook<P: Perform>(&mut self, performer: &mut P, byte: u8) {
        if self.params.at_limit() {
            self.ignoring = true;
            self.params.seal_group();
        } else {
            self.params.end_group(self.param);
        }
        performer.hook(&Sequence::new(
            &self.params,
            &self.intermediates[..self.intermediate_idx],
            self.ignoring,
            byte,
        ));
        self.state = State::DcsPassthrough;
    }

    #[inline]
    fn action_collect(&mut self, byte: u8) {
        if self.intermediate_idx == MAX_INTERMEDIATES {
            self.ignoring = true;
        } else {
            self.intermediates[self.intermediate_idx] = byte;
            self.intermediate_idx += 1;
        }
    }

    /// Advance to the next subparameter.
    #[inline]
    fn action_subparam(&mut self) {
        if self.params.at_limit() {
            self.ignoring = true;
        } else {
            self.params.continue_group(self.param);
            self.param = 0;
        }
    }

    /// Advance to the next parameter.
    #[inline]
    fn action_param(&mut self) {
        if self.params.at_limit() {
            self.ignoring = true;
        } else {
            self.params.end_group(self.param);
            self.param = 0;
        }
    }

    /// Advance inside the parameter without terminating it.
    #[inline]
    fn action_paramnext(&mut self, byte: u8) {
        if self.params.at_limit() {
            self.ignoring = true;
        } else {
            // Continue collecting bytes into param.
            self.param = self.param.saturating_mul(10);
            self.param = self.param.saturating_add((byte - b'0') as u16);
        }
    }

    /// Add OSC param separator.
    #[inline]
    fn action_osc_put_param(&mut self) {
        let idx = self.osc_raw.len();

        let param_idx = self.osc_num_params;
        match param_idx {
            // First param is special - 0 to current byte index.
            0 => self.osc_params[param_idx] = (0, idx),

            // Only process up to MAX_OSC_PARAMS.
            MAX_OSC_PARAMS => return,

            // All other params depend on previous indexing.
            _ => {
                let prev = self.osc_params[param_idx - 1];
                let begin = prev.1;
                self.osc_params[param_idx] = (begin, idx);
            },
        }

        self.osc_num_params += 1;
    }

    #[inline(always)]
    fn action_osc_put(&mut self, byte: u8) {
        #[cfg(any())]
        {
            if self.osc_raw.is_full() {
                return;
            }
        }
        self.osc_raw.push(byte);
    }

    fn osc_end<P: Perform>(&mut self, performer: &mut P, byte: u8) {
        self.action_osc_put_param();
        self.osc_dispatch(performer, byte);
        self.osc_raw.clear();
        self.osc_num_params = 0;
    }

    /// Reset escape sequence parameters and intermediates.
    #[inline]
    fn reset_params(&mut self) {
        self.intermediate_idx = 0;
        self.ignoring = false;
        self.param = 0;

        self.params.reset();
    }

    /// Separate method for osc_dispatch that borrows self as read-only
    ///
    /// The aliasing is needed here for multiple slices into self.osc_raw
    #[inline]
    fn osc_dispatch<P: Perform>(&self, performer: &mut P, byte: u8) {
        let mut slices: [MaybeUninit<&[u8]>; MAX_OSC_PARAMS] =
            unsafe { MaybeUninit::uninit().assume_init() };

        for (i, slice) in slices.iter_mut().enumerate().take(self.osc_num_params) {
            let indices = self.osc_params[i];
            *slice = MaybeUninit::new(&self.osc_raw[indices.0..indices.1]);
        }

        unsafe {
            let num_params = self.osc_num_params;
            let params = &slices[..num_params] as *const [MaybeUninit<&[u8]>] as *const [&[u8]];
            performer.osc_end(&*params, byte == 0x07);
        }
    }

    /// Advance the parser state from ground.
    ///
    /// The ground state is handled separately since it can only be left using
    /// the escape character (`\x1b`). This allows more efficient parsing by
    /// using SIMD search with [`memchr`].
    #[inline]
    fn advance_ground<P: Perform>(&mut self, performer: &mut P, bytes: &[u8]) -> usize {
        // Find the next escape character.
        let num_bytes = bytes.len();
        let plain_chars = memchr::memchr(0x1B, bytes).unwrap_or(num_bytes);

        // If the next character is ESC, just process it and short-circuit.
        if plain_chars == 0 {
            self.state = State::Escape;
            self.reset_params();
            return 1;
        }

        let plain = &bytes[..plain_chars];
        if plain.is_ascii() {
            Self::ground_dispatch(performer, unsafe { str::from_utf8_unchecked(plain) });
            let mut processed = plain_chars;
            if processed < num_bytes {
                self.state = State::Escape;
                self.reset_params();
                processed += 1;
            }
            return processed;
        }
        match str::from_utf8(plain) {
            Ok(parsed) => {
                Self::ground_dispatch(performer, parsed);
                let mut processed = plain_chars;

                // If there's another character, it must be escape so process it directly.
                if processed < num_bytes {
                    self.state = State::Escape;
                    self.reset_params();
                    processed += 1;
                }

                processed
            },
            // Handle invalid and partial utf8.
            Err(err) => {
                // Dispatch all the valid bytes.
                let valid_bytes = err.valid_up_to();
                let parsed = unsafe { str::from_utf8_unchecked(&bytes[..valid_bytes]) };
                Self::ground_dispatch(performer, parsed);

                match err.error_len() {
                    Some(len) => {
                        // Execute C1 escapes or emit replacement character.
                        if len == 1 && bytes[valid_bytes] <= 0x9F {
                            performer.execute(bytes[valid_bytes]);
                        } else {
                            performer.print('�');
                        }

                        // Restart processing after the invalid bytes.
                        //
                        // While we could theoretically try to just re-parse
                        // `bytes[valid_bytes + len..plain_chars]`, it's easier
                        // to just skip it and invalid utf8 is pretty rare anyway.
                        valid_bytes + len
                    },
                    None => {
                        if plain_chars < num_bytes {
                            // Process bytes cut off by escape.
                            performer.print('�');
                            self.state = State::Escape;
                            self.reset_params();
                            plain_chars + 1
                        } else {
                            // Process bytes cut off by the buffer end.
                            let extra_bytes = num_bytes - valid_bytes;
                            let partial_len = self.partial_utf8_len + extra_bytes;
                            self.partial_utf8[self.partial_utf8_len..partial_len]
                                .copy_from_slice(&bytes[valid_bytes..valid_bytes + extra_bytes]);
                            self.partial_utf8_len = partial_len;
                            num_bytes
                        }
                    },
                }
            },
        }
    }

    /// Advance the parser while processing a partial utf8 codepoint.
    #[inline]
    fn advance_partial_utf8<P: Perform>(&mut self, performer: &mut P, bytes: &[u8]) -> usize {
        // Try to copy up to 3 more characters, to ensure the codepoint is complete.
        let old_bytes = self.partial_utf8_len;
        let to_copy = bytes.len().min(self.partial_utf8.len() - old_bytes);
        self.partial_utf8[old_bytes..old_bytes + to_copy].copy_from_slice(&bytes[..to_copy]);
        self.partial_utf8_len += to_copy;

        // Parse the unicode character.
        match str::from_utf8(&self.partial_utf8[..self.partial_utf8_len]) {
            // If the entire buffer is valid, use the first character and continue parsing.
            Ok(parsed) => {
                let c = unsafe { parsed.chars().next().unwrap_unchecked() };
                performer.print(c);

                self.partial_utf8_len = 0;
                c.len_utf8() - old_bytes
            },
            Err(err) => {
                let valid_bytes = err.valid_up_to();
                // If we have any valid bytes, that means we partially copied another
                // utf8 character into `partial_utf8`. Since we only care about the
                // first character, we just ignore the rest.
                if valid_bytes > 0 {
                    let c = unsafe {
                        let parsed = str::from_utf8_unchecked(&self.partial_utf8[..valid_bytes]);
                        parsed.chars().next().unwrap_unchecked()
                    };

                    performer.print(c);

                    self.partial_utf8_len = 0;
                    return valid_bytes - old_bytes;
                }

                match err.error_len() {
                    // If the partial character was also invalid, emit the replacement
                    // character.
                    Some(invalid_len) => {
                        performer.print('�');

                        self.partial_utf8_len = 0;
                        invalid_len - old_bytes
                    },
                    // If the character still isn't complete, wait for more data.
                    None => to_copy,
                }
            },
        }
    }

    /// Handle ground dispatch of print/execute for all characters in a string.
    #[inline]
    fn ground_dispatch<P: Perform>(performer: &mut P, text: &str) {
        let bytes = text.as_bytes();
        const ONES: u64 = 0x0101_0101_0101_0101;
        const HIGHS: u64 = 0x8080_8080_8080_8080;
        let mut start = 0;
        let mut i = 0;
        while i < bytes.len() {
            while i + 8 <= bytes.len() {
                let word = u64::from_le_bytes(bytes[i..i + 8].try_into().unwrap_or([0; 8]));
                let control = word.wrapping_sub(ONES * 0x20) & !word & HIGHS;
                let c2 = word ^ (ONES * 0xC2);
                let lead = c2.wrapping_sub(ONES) & !c2 & HIGHS;
                if control | lead != 0 {
                    break;
                }
                i += 8;
            }
            if i >= bytes.len() {
                break;
            }
            let byte = bytes[i];
            if byte < 0x20 {
                if start < i {
                    performer.print_str(unsafe { str::from_utf8_unchecked(&bytes[start..i]) });
                }
                performer.execute(byte);
                i += 1;
                start = i;
            } else if byte == 0xC2 && i + 1 < bytes.len() && (0x80..=0x9F).contains(&bytes[i + 1]) {
                if start < i {
                    performer.print_str(unsafe { str::from_utf8_unchecked(&bytes[start..i]) });
                }
                performer.execute(bytes[i + 1]);
                i += 2;
                start = i;
            } else {
                i += 1;
            }
        }
        if start < bytes.len() {
            performer.print_str(unsafe { str::from_utf8_unchecked(&bytes[start..]) });
        }
    }
}

#[derive(PartialEq, Eq, Debug, Default, Copy, Clone)]
enum State {
    CsiEntry,
    CsiIgnore,
    CsiIntermediate,
    CsiParam,
    DcsEntry,
    DcsIgnore,
    DcsIntermediate,
    DcsParam,
    DcsPassthrough,
    Escape,
    EscapeIntermediate,
    OscString,
    SosPmApcString,
    #[default]
    Ground,
}
