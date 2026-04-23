//! Continuous, termwiz-driven parser for host-terminal replies arriving on
//! stdin. Replaces the previous 500-ms window + on-disk cache with a
//! long-lived parser that lives for the whole client session.
//!
//! This parser routes stdin bytes through a private `termwiz::InputParser`,
//! classifies OSC / CSI-report events into `HostReply` variants, and lets
//! all other bytes (keyboard input) pass through as a residue byte sequence
//! that the caller feeds to the normal keyboard parser.

use lazy_static::lazy_static;
use regex::Regex;
use serde::{Deserialize, Serialize};
use zellij_utils::{
    ipc::PixelDimensions,
    pane_size::SizeInPixels,
    vendored::termwiz::input::{InputEvent, InputParser},
};

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

/// A classified host-terminal reply received on stdin.
///
/// The variants track the reply types Zellij consumes for its own
/// synchronous render hot-path (pixel dims, bg/fg, palette registers,
/// sync-output support) plus a generic `ForwardedReply` that carries raw
/// bytes accumulated inside a forwarding window. These classified replies
/// are emitted **in addition** to any forwarded-reply accumulation; this
/// double-dispatch invariant keeps the server's cached view of the host
/// state in sync even while a forwarded query is in flight.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HostReply {
    PixelDimensions(PixelDimensions),
    BackgroundColor(String),
    ForegroundColor(String),
    ColorRegisters(Vec<(usize, String)>),
    SynchronizedOutput(Option<SyncOutput>),
    /// Raw bytes of replies observed during an active forwarding window,
    /// closed by the barrier (Primary-DA) reply. The bytes are delivered
    /// verbatim to the pane's pty on the server side.
    ForwardedReply {
        token: u32,
        raw_bytes: Vec<u8>,
    },
}

/// Retained alias for the pre-refactor type name used by other modules in
/// the client. New code should prefer `HostReply`; the alias keeps the
/// existing `InputInstruction::AnsiStdinInstructions(Vec<...>)` plumbing
/// stable during the migration.
pub type AnsiStdinInstruction = HostReply;

impl HostReply {
    /// Classify an OSC payload (the bytes between the `ESC ]` prefix and
    /// the ST/BEL terminator) into a known `HostReply`, if possible.
    pub fn from_osc_payload(payload: &[u8]) -> Option<HostReply> {
        lazy_static! {
            // OSC 10 (foreground) / OSC 11 (background) answer form:
            //   OSC 10 ; <color> ST        e.g. "10;rgb:ffff/ffff/ffff"
            //   OSC 11 ; <color> ST
            static ref FG_RE: Regex = Regex::new(r"^10;(.*)$").unwrap();
            static ref BG_RE: Regex = Regex::new(r"^11;(.*)$").unwrap();
            // OSC 4 ; N ; <color> — palette-register answer.
            static ref COLOR_REGISTER_RE: Regex = Regex::new(r"^4;(\d+);(.*)$").unwrap();
            // Alacritty quirk: sometimes drops the leading "4;" in the reply.
            static ref ALTERNATIVE_COLOR_REGISTER_RE: Regex = Regex::new(r"^(\d+);(.*)$").unwrap();
        }
        let s = std::str::from_utf8(payload).ok()?;
        if let Some(caps) = BG_RE.captures(s) {
            return Some(HostReply::BackgroundColor(caps[1].to_string()));
        }
        if let Some(caps) = FG_RE.captures(s) {
            return Some(HostReply::ForegroundColor(caps[1].to_string()));
        }
        if let Some(caps) = COLOR_REGISTER_RE.captures(s) {
            let index: usize = caps[1].parse().ok()?;
            let color = caps[2].to_string();
            return Some(HostReply::ColorRegisters(vec![(index, color)]));
        }
        if let Some(caps) = ALTERNATIVE_COLOR_REGISTER_RE.captures(s) {
            let index: usize = caps[1].parse().ok()?;
            let color = caps[2].to_string();
            return Some(HostReply::ColorRegisters(vec![(index, color)]));
        }
        None
    }

    /// Classify a CSI-based report (the full raw sequence including the
    /// leading `ESC [`) into a `HostReply`, if possible.
    ///
    /// Recognised final bytes: `t` (pixel-dims reply, `CSI 4/6 ; H ; W t`),
    /// `y` (DECRPM reply to `CSI ?2026$p` — sync-output support
    /// advertisement).
    pub fn from_csi_report(raw: &[u8]) -> Option<HostReply> {
        let s = std::str::from_utf8(raw).ok()?;
        lazy_static! {
            // <ESC>[4;H;Wt or <ESC>[6;H;Wt
            static ref PIX_RE: Regex = Regex::new(r"^\u{1b}\[(\d+);(\d+);(\d+)t$").unwrap();
            // <ESC>[?2026;Ny — DECRPM reply for sync-output (VT mode 2026)
            static ref SYNC_RE: Regex = Regex::new(r"^\u{1b}\[\?2026;([0-4])\$y$").unwrap();
        }
        if let Some(caps) = PIX_RE.captures(s) {
            let which: usize = caps[1].parse().ok()?;
            let first: usize = caps[2].parse().ok()?;
            let second: usize = caps[3].parse().ok()?;
            return match which {
                4 => Some(HostReply::PixelDimensions(PixelDimensions {
                    character_cell_size: None,
                    text_area_size: Some(SizeInPixels {
                        height: first,
                        width: second,
                    }),
                })),
                6 => Some(HostReply::PixelDimensions(PixelDimensions {
                    character_cell_size: Some(SizeInPixels {
                        height: first,
                        width: second,
                    }),
                    text_area_size: None,
                })),
                _ => None,
            };
        }
        if let Some(caps) = SYNC_RE.captures(s) {
            let code: usize = caps[1].parse().ok()?;
            return match code {
                1 | 2 | 3 => Some(HostReply::SynchronizedOutput(Some(SyncOutput::CSI))),
                _ => Some(HostReply::SynchronizedOutput(None)),
            };
        }
        None
    }
}

/// The "slot" tracking state for a single forwarded query currently in
/// flight to the host terminal. The parser accumulates raw reply bytes
/// into `reply_bytes` until it sees a Primary-DA (`c`) reply, which acts
/// as the serializing barrier.
#[derive(Debug, Clone)]
pub struct ForwardSlot {
    pub token: u32,
    pub reply_bytes: Vec<u8>,
    pub opened_at: std::time::Instant,
}

/// Return value of `feed()`.
#[derive(Debug, Clone, Default)]
pub struct ParseOutput {
    /// Classified host replies (zero or more).
    pub replies: Vec<HostReply>,
    /// A completed forwarded reply (Primary-DA barrier seen), ready to be
    /// sent to the server. At most one per feed call; more than one in a
    /// single feed would indicate the host emitted two barriers, in which
    /// case only the first is honored.
    pub completed_forward: Option<(u32, Vec<u8>)>,
    /// Residue bytes that were not classified as host replies. These are
    /// the bytes the caller should feed to the keyboard parser.
    pub residue: Vec<u8>,
}

/// Continuous host-reply parser. Lives for the whole client session.
pub struct HostReplyParser {
    inner: InputParser,
    /// Active forwarding slot: `Some` while a forwarded query is in
    /// flight, `None` otherwise.
    active_forward: Option<ForwardSlot>,
}

impl std::fmt::Debug for HostReplyParser {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HostReplyParser")
            .field("active_forward", &self.active_forward)
            .finish()
    }
}

impl HostReplyParser {
    pub fn new() -> Self {
        HostReplyParser {
            inner: InputParser::new(),
            active_forward: None,
        }
    }

    /// Open a forwarding window for `token`. Subsequent reply events that
    /// arrive before the Primary-DA barrier will be accumulated into the
    /// slot's `reply_bytes`, in addition to being dispatched as normal
    /// classified `HostReply` events.
    pub fn open_forward(&mut self, token: u32) {
        self.active_forward = Some(ForwardSlot {
            token,
            reply_bytes: Vec::new(),
            opened_at: std::time::Instant::now(),
        });
    }

    /// Close an active forwarding window without a barrier (timeout path).
    /// Returns the accumulated reply bytes and the token, if any.
    pub fn close_forward_on_timeout(&mut self, token: u32) -> Option<(u32, Vec<u8>)> {
        match &self.active_forward {
            Some(slot) if slot.token == token => {
                let slot = self.active_forward.take().unwrap();
                Some((slot.token, slot.reply_bytes))
            },
            _ => None,
        }
    }

    pub fn active_forward_token(&self) -> Option<u32> {
        self.active_forward.as_ref().map(|s| s.token)
    }

    pub fn active_forward_opened_at(&self) -> Option<std::time::Instant> {
        self.active_forward.as_ref().map(|s| s.opened_at)
    }

    /// Consume a chunk of raw stdin bytes. Returns classified host replies
    /// (to be dispatched to the server's cached-state consumers), at most
    /// one completed forwarded reply (barrier closed the window), and the
    /// residue bytes that were not part of any classified sequence — these
    /// are the bytes the caller should feed to the keyboard parser.
    pub fn feed(&mut self, bytes: &[u8]) -> ParseOutput {
        let mut out = ParseOutput::default();
        // Collect events first (borrow-splits the InputParser across the
        // callback and the post-processing mutations).
        let mut events = Vec::new();
        let mut residue = Vec::new();
        self.inner.parse(
            bytes,
            |event| {
                events.push(event);
            },
            true, // maybe_more — typical stream usage
        );
        for event in events {
            match event {
                InputEvent::OperatingSystemCommand(payload) => {
                    // Classified OSC reply — double-dispatch.
                    if let Some(reply) = HostReply::from_osc_payload(&payload) {
                        out.replies.push(reply);
                    }
                    if let Some(slot) = self.active_forward.as_mut() {
                        // Re-serialize so the pane's pty sees a legal OSC.
                        // Terminators vary by host; ST (ESC \) is always safe.
                        slot.reply_bytes.extend_from_slice(b"\x1b]");
                        slot.reply_bytes.extend_from_slice(&payload);
                        slot.reply_bytes.extend_from_slice(b"\x1b\\");
                    }
                },
                InputEvent::DeviceControlReply {
                    params,
                    final_byte,
                    raw,
                    ..
                } => {
                    match final_byte {
                        b'c' => {
                            // Primary-DA — the barrier. Close the slot and
                            // emit the completed forwarded reply if active.
                            if let Some(slot) = self.active_forward.take() {
                                out.completed_forward = Some((slot.token, slot.reply_bytes));
                            }
                            // Primary-DA is NOT double-dispatched — it has
                            // no cached-state counterpart.
                        },
                        _ => {
                            if let Some(reply) = HostReply::from_csi_report(&raw) {
                                out.replies.push(reply);
                            }
                            if let Some(slot) = self.active_forward.as_mut() {
                                slot.reply_bytes.extend_from_slice(&raw);
                            }
                            // Suppress unused-variable warning for params.
                            let _ = params;
                        },
                    }
                },
                // Everything else is keyboard / mouse / paste / wake input;
                // we need those bytes to reach the keyboard parser. We can
                // not reconstruct the exact bytes from a parsed event here,
                // so we rely on the caller's own second pass through the
                // keyboard parser: the residue is the concatenation of all
                // input bytes that are NOT part of a classified reply. To
                // produce that residue deterministically, we re-scan the
                // buffer a second time below.
                _ => {},
            }
        }
        // Produce the residue: replay the input through a scratch parser
        // that strips out OSC payloads and whitelisted CSI reports. All
        // other bytes pass through unchanged.
        residue.extend(Self::strip_replies(bytes));
        out.residue = residue;
        out
    }

    /// Walk `bytes` and drop any OSC/whitelisted-CSI sequences, returning
    /// the remaining bytes verbatim (keyboard residue). This is a
    /// byte-level scrubber — it does not produce events, only bytes.
    fn strip_replies(bytes: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(bytes.len());
        let mut i = 0;
        while i < bytes.len() {
            let rest = &bytes[i..];
            // OSC: ESC ] ... (BEL | ESC \)
            if rest.len() >= 2 && rest[0] == 0x1b && rest[1] == b']' {
                if let Some(len) = osc_len(rest) {
                    i += len;
                    continue;
                }
            }
            // Whitelisted CSI report: ESC [ <params>* <intermediates>* <final>
            if rest.len() >= 2 && rest[0] == 0x1b && rest[1] == b'[' {
                if let Some(len) = csi_report_len(rest) {
                    i += len;
                    continue;
                }
            }
            out.push(bytes[i]);
            i += 1;
        }
        out
    }
}

fn osc_len(buf: &[u8]) -> Option<usize> {
    if buf.get(0) != Some(&0x1b) || buf.get(1) != Some(&b']') {
        return None;
    }
    let mut i = 2;
    while i < buf.len() {
        match buf[i] {
            0x07 => return Some(i + 1),
            0x1b if buf.get(i + 1) == Some(&b'\\') => return Some(i + 2),
            0x1b => return None, // bare ESC inside OSC — malformed
            _ => i += 1,
        }
    }
    None
}

fn csi_report_len(buf: &[u8]) -> Option<usize> {
    if buf.get(0) != Some(&0x1b) || buf.get(1) != Some(&b'[') {
        return None;
    }
    let mut i = 2;
    let max = buf.len().min(256);
    while i < max {
        let b = buf[i];
        match b {
            0x30..=0x3F | 0x20..=0x2F => i += 1,
            b't' | b'y' | b'c' | b'n' => return Some(i + 1),
            0x40..=0x7E => return None, // non-whitelisted final
            _ => return None,
        }
    }
    None
}

#[cfg(test)]
#[path = "stdin_ansi_parser_tests.rs"]
mod tests;
