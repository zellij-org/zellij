//! Soak test: drive a `TerminalPane` (interceptor, parser and grid) with
//! hostile, deterministic output and check that nothing panics, retained
//! state stays inside its bounds, and CPU grows roughly linearly with input.
//!
//! Every case is derived from a seed through a local splitmix64 generator,
//! so a failure reproduces from `(seed, byte budget, resize flag)` alone.
//! The ignored `soak_heavy` test runs many more seeds with bigger budgets.

use super::super::TerminalPane;
use crate::panes::grid::{
    MAX_NESTED_FRAME_DCS_BYTES, MAX_TITLE_BYTES, MAX_TITLE_STACK_SIZE, MAX_UI_COMPONENT_DCS_BYTES,
    MAX_XTGETTCAP_DCS_BYTES,
};
use crate::panes::kitty_graphics::KittyImageStore;
use crate::panes::sixel::SixelImageStore;
use crate::panes::{cluster_table, LinkHandler};
use crate::tab::Pane;
use base64::engine::general_purpose::STANDARD as BASE64_ENCODER;
use base64::engine::Engine as _;
use std::cell::RefCell;
use std::collections::HashMap;
use std::panic::{catch_unwind, resume_unwind, AssertUnwindSafe};
use std::rc::Rc;
use std::time::{Duration, Instant};
use zellij_utils::consts::SCROLL_BUFFER_SIZE;
use zellij_utils::{
    data::{Palette, Style},
    pane_size::{PaneGeom, SizeInPixels},
};

const INITIAL_ROWS: usize = 24;
const INITIAL_COLUMNS: usize = 80;
/// Quota of the kitty image store under test. Small, so that eviction is
/// exercised by a few megabytes of images.
const KITTY_QUOTA_BYTES: usize = 4 * 1024 * 1024;
/// The parser holds OSC payloads of up to 4 MiB and releases buffers that
/// grew past 64 KiB when the string ends. A `Vec` that is filled to 4 MiB
/// may have rounded its capacity up to the next power of two.
const OSC_CAPACITY_WHILE_OPEN: usize = 8 * 1024 * 1024;
const OSC_CAPACITY_AT_REST: usize = 64 * 1024;
/// The kitty interceptor stops buffering at 1 MiB.
const KITTY_CAPTURE_LIMIT: usize = 1_048_576;
/// Bytes of notification ids a pane may track: the 256 tracked ids at the
/// 4 KiB that a notification payload is held to.
const NOTIFICATION_ID_BYTES_LIMIT: usize = 256 * 4096;

// ---------------------------------------------------------------------------
// deterministic random numbers
// ---------------------------------------------------------------------------

struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Rng(seed)
    }
    fn next(&mut self) -> u64 {
        // splitmix64
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    /// Uniform in `0..n`; `n` must be non-zero.
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
    fn range(&mut self, low: usize, high_inclusive: usize) -> usize {
        low + self.below(high_inclusive - low + 1)
    }
    fn chance(&mut self, one_in: usize) -> bool {
        self.below(one_in) == 0
    }
    fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len())]
    }
}

// ---------------------------------------------------------------------------
// generators
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    PlainText,
    RandomBytes,
    Csi,
    ModeSwitch,
    Osc,
    Dcs,
    Apc,
    Hyperlinks,
    Titles,
    Clipboard,
    Notification,
    Unterminated,
    Kitty,
    Sixel,
    WideAndCombining,
    ScrollRegion,
    AlternateScreen,
    EraseAndRepeat,
}

const ALL_KINDS: &[Kind] = &[
    Kind::PlainText,
    Kind::RandomBytes,
    Kind::Csi,
    Kind::ModeSwitch,
    Kind::Osc,
    Kind::Dcs,
    Kind::Apc,
    Kind::Hyperlinks,
    Kind::Titles,
    Kind::Clipboard,
    Kind::Notification,
    Kind::Unterminated,
    Kind::Kitty,
    Kind::Sixel,
    Kind::WideAndCombining,
    Kind::ScrollRegion,
    Kind::AlternateScreen,
    Kind::EraseAndRepeat,
];

struct Gen {
    rng: Rng,
    next_link: usize,
}

impl Gen {
    fn new(seed: u64) -> Self {
        Gen {
            rng: Rng::new(seed),
            next_link: 0,
        }
    }

    /// A numeric parameter, biased towards the values that break parsers.
    fn number(&mut self) -> String {
        match self.rng.below(14) {
            0 => "0".into(),
            1 => "1".into(),
            2 => "2".into(),
            3 => "65535".into(),
            4 => "65536".into(),
            5 => "4294967295".into(),
            6 => "4294967296".into(),
            7 => "18446744073709551615".into(),
            8 => String::new(),
            9 => "-1".into(),
            10 => {
                let digits = self.rng.range(20, 3000);
                (0..digits)
                    .map(|_| (b'0' + self.rng.below(10) as u8) as char)
                    .collect()
            },
            _ => self.rng.below(300).to_string(),
        }
    }

    /// A sixel raster attribute: the sizes that overflow, and the sizes that
    /// are merely large. Declared heights from a few thousand pixels up are
    /// kept out to keep the run short (see
    /// `a_tall_sixel_raster_is_rejected_cheaply`).
    fn raster_number(&mut self) -> String {
        match self.rng.below(8) {
            0 => "4294967295".into(),
            1 => "4294967296".into(),
            2 => "18446744073709551615".into(),
            3 => "1000".into(),
            4 => "1500".into(),
            5 => "0".into(),
            _ => self.rng.below(300).to_string(),
        }
    }

    /// Pick a kind of output, offering the kinds that cost a lot of CPU per
    /// byte less often so that a byte budget still buys variety.
    fn pick_kind(&mut self) -> Kind {
        loop {
            let kind = *self.rng.pick(ALL_KINDS);
            let costly = matches!(
                kind,
                Kind::Csi | Kind::Sixel | Kind::ScrollRegion | Kind::EraseAndRepeat
            );
            if !costly || self.rng.chance(4) {
                return kind;
            }
        }
    }

    fn params(&mut self, max: usize) -> String {
        let count = self.rng.range(0, max);
        let mut out = String::new();
        for i in 0..count {
            if i > 0 {
                out.push(if self.rng.chance(8) { ':' } else { ';' });
            }
            out.push_str(&self.number());
        }
        out
    }

    fn printable(&mut self, out: &mut Vec<u8>, len: usize) {
        out.extend((0..len).map(|_| b' ' + self.rng.below(95) as u8));
    }

    fn word(&mut self) -> String {
        let len = self.rng.range(1, 12);
        (0..len)
            .map(|_| (b'a' + self.rng.below(26) as u8) as char)
            .collect()
    }

    fn terminator(&mut self, out: &mut Vec<u8>) {
        if self.rng.chance(2) {
            out.push(0x07);
        } else {
            out.extend_from_slice(b"\x1b\\");
        }
    }

    fn emit(&mut self, kind: Kind, out: &mut Vec<u8>) {
        match kind {
            Kind::PlainText => self.plain_text(out),
            Kind::RandomBytes => {
                let len = self.rng.range(1, 4096);
                out.extend((0..len).map(|_| self.rng.next() as u8));
            },
            Kind::Csi => self.csi(out),
            Kind::ModeSwitch => self.mode_switch(out),
            Kind::Osc => self.osc(out),
            Kind::Dcs => self.dcs(out),
            Kind::Apc => self.apc(out),
            Kind::Hyperlinks => self.hyperlinks(out),
            Kind::Titles => self.titles(out),
            Kind::Clipboard => self.clipboard(out),
            Kind::Notification => self.notification(out),
            Kind::Unterminated => self.unterminated(out),
            Kind::Kitty => self.kitty(out),
            Kind::Sixel => self.sixel(out),
            Kind::WideAndCombining => self.wide_and_combining(out),
            Kind::ScrollRegion => self.scroll_region(out),
            Kind::AlternateScreen => self.alternate_screen(out),
            Kind::EraseAndRepeat => self.erase_and_repeat(out),
        }
    }

    fn plain_text(&mut self, out: &mut Vec<u8>) {
        let lines = self.rng.range(1, 400);
        for _ in 0..lines {
            let len = self.rng.range(0, 120);
            self.printable(out, len);
            out.extend_from_slice(b"\r\n");
        }
    }

    fn csi(&mut self, out: &mut Vec<u8>) {
        const FINALS: &[u8] = b"@ABCDEFGHIJKLMPSTXZ`abcdefghilmnpqrstuvxyz}~";
        for _ in 0..self.rng.range(1, 24) {
            out.extend_from_slice(b"\x1b[");
            if self.rng.chance(6) {
                out.push(*self.rng.pick(b"?><=!"));
            }
            let params = self.params(8);
            out.extend_from_slice(params.as_bytes());
            if self.rng.chance(10) {
                out.push(*self.rng.pick(b" $'!\""));
            }
            if self.rng.chance(12) {
                out.push(0x40 + self.rng.below(0x3f) as u8);
            } else {
                out.push(*self.rng.pick(FINALS));
            }
            if self.rng.chance(3) {
                let len = self.rng.range(1, 16);
                self.printable(out, len);
            }
        }
    }

    fn mode_switch(&mut self, out: &mut Vec<u8>) {
        const MODES: &[usize] = &[
            1, 3, 4, 6, 7, 20, 25, 47, 69, 1000, 1002, 1003, 1004, 1005, 1006, 1015, 1047, 1048,
            1049, 2004, 2026, 2027, 2031, 7727,
        ];
        for _ in 0..self.rng.range(1, 8) {
            let mode = if self.rng.chance(6) {
                self.number()
            } else {
                self.rng.pick(MODES).to_string()
            };
            let private = if self.rng.chance(5) { "" } else { "?" };
            let action = if self.rng.chance(2) { 'h' } else { 'l' };
            out.extend_from_slice(format!("\x1b[{private}{mode}{action}").as_bytes());
        }
    }

    fn osc(&mut self, out: &mut Vec<u8>) {
        const NUMBERS: &[usize] = &[
            0, 1, 2, 4, 7, 8, 9, 10, 11, 12, 52, 99, 104, 110, 111, 112, 133, 633, 777, 1337, 7501,
        ];
        for _ in 0..self.rng.range(1, 8) {
            let number = if self.rng.chance(6) {
                self.number()
            } else {
                self.rng.pick(NUMBERS).to_string()
            };
            out.extend_from_slice(format!("\x1b]{number}").as_bytes());
            for _ in 0..self.rng.range(0, 5) {
                out.push(b';');
                if self.rng.chance(4) {
                    out.extend_from_slice(self.number().as_bytes());
                } else {
                    let len = self.rng.range(0, 60);
                    // keep the separators and terminators out of the text
                    let mut text = Vec::new();
                    self.printable(&mut text, len);
                    text.retain(|byte| *byte != b';');
                    out.extend(text);
                }
            }
            if self.rng.chance(3) {
                out.extend_from_slice(b";?");
            }
            self.terminator(out);
        }
    }

    fn dcs(&mut self, out: &mut Vec<u8>) {
        for _ in 0..self.rng.range(1, 4) {
            out.extend_from_slice(b"\x1bP");
            let params = self.params(4);
            out.extend_from_slice(params.as_bytes());
            if self.rng.chance(3) {
                out.push(*self.rng.pick(b"+$!"));
            }
            out.push(*self.rng.pick(b"qzntp|{"));
            let len = if self.rng.chance(8) {
                self.rng.range(1000, 200_000)
            } else {
                self.rng.range(0, 200)
            };
            self.printable(out, len);
            out.extend_from_slice(b"\x1b\\");
        }
    }

    fn apc(&mut self, out: &mut Vec<u8>) {
        for _ in 0..self.rng.range(1, 4) {
            out.extend_from_slice(b"\x1b_");
            if self.rng.chance(3) {
                out.push(b'G');
            } else {
                out.push(*self.rng.pick(b"abcxyz0;"));
            }
            let len = if self.rng.chance(8) {
                self.rng.range(1000, 100_000)
            } else {
                self.rng.range(0, 100)
            };
            self.printable(out, len);
            self.terminator(out);
        }
    }

    fn hyperlinks(&mut self, out: &mut Vec<u8>) {
        for _ in 0..self.rng.range(1, 40) {
            self.next_link += 1;
            let id = self.next_link;
            let tail = if self.rng.chance(10) {
                // an occasional target near or past the per-link limit
                "x".repeat(self.rng.range(4000, 20_000))
            } else {
                self.word()
            };
            let params = match self.rng.below(4) {
                0 => String::new(),
                1 => format!("id=link{id}"),
                2 => format!("id={}", "i".repeat(self.rng.range(200, 400))),
                _ => format!("foo=bar:id=g{}", id % 7),
            };
            out.extend_from_slice(
                format!("\x1b]8;{params};https://example.test/{id}/{tail}\x1b\\").as_bytes(),
            );
            out.extend_from_slice(self.word().as_bytes());
            out.extend_from_slice(b"\x1b]8;;\x1b\\ ");
        }
        out.extend_from_slice(b"\r\n");
    }

    fn titles(&mut self, out: &mut Vec<u8>) {
        for _ in 0..self.rng.range(1, 60) {
            match self.rng.below(5) {
                0 | 1 => {
                    let osc = *self.rng.pick(&[0, 2]);
                    let len = if self.rng.chance(10) {
                        self.rng.range(4096, 120_000)
                    } else {
                        self.rng.range(0, 80)
                    };
                    let mut title = Vec::new();
                    self.printable(&mut title, len);
                    title.retain(|byte| *byte != b';');
                    out.extend_from_slice(format!("\x1b]{osc};").as_bytes());
                    out.extend(title);
                    self.terminator(out);
                },
                2 => {
                    // a multi-byte title cut by the length limit
                    out.extend_from_slice(b"\x1b]2;");
                    for _ in 0..self.rng.range(1000, 3000) {
                        out.extend_from_slice("\u{4f60}\u{597d}\u{1f600}".as_bytes());
                    }
                    self.terminator(out);
                },
                3 => {
                    let which = self.rng.pick(&["", ";0", ";1", ";2", ";99999"]);
                    out.extend_from_slice(format!("\x1b[22{which}t").as_bytes());
                },
                _ => {
                    let which = self.rng.pick(&["", ";0", ";1", ";2", ";99999"]);
                    out.extend_from_slice(format!("\x1b[23{which}t").as_bytes());
                },
            }
        }
    }

    fn clipboard(&mut self, out: &mut Vec<u8>) {
        for _ in 0..self.rng.range(1, 6) {
            let selection = *self.rng.pick(&["c", "p", "s", "", "cp", "0", "?"]);
            let payload = match self.rng.below(5) {
                0 => "?".to_string(),
                1 => String::new(),
                2 => {
                    let mut raw = Vec::new();
                    let len = self.rng.range(0, 200_000);
                    self.printable(&mut raw, len);
                    BASE64_ENCODER.encode(raw)
                },
                3 => {
                    // not base64
                    let mut raw = Vec::new();
                    let len = self.rng.range(1, 4000);
                    self.printable(&mut raw, len);
                    raw.retain(|byte| *byte != b';');
                    String::from_utf8(raw).unwrap()
                },
                _ => BASE64_ENCODER.encode(self.word()),
            };
            out.extend_from_slice(format!("\x1b]52;{selection};{payload}").as_bytes());
            self.terminator(out);
        }
    }

    fn notification(&mut self, out: &mut Vec<u8>) {
        for _ in 0..self.rng.range(1, 20) {
            let id = if self.rng.chance(6) {
                "n".repeat(self.rng.range(1000, 30_000))
            } else {
                self.rng.below(500).to_string()
            };
            let kind = *self.rng.pick(&["title", "body", "close", "?", "alive"]);
            let done = *self.rng.pick(&["0", "1"]);
            let action = if self.rng.chance(4) {
                ":a=report,focus"
            } else {
                ""
            };
            out.extend_from_slice(
                format!("\x1b]99;i={id}:p={kind}:d={done}{action};{}", self.word()).as_bytes(),
            );
            self.terminator(out);
        }
    }

    /// A string that starts and never ends, or ends only after a long while.
    fn unterminated(&mut self, out: &mut Vec<u8>) {
        let introducer: &[u8] = match self.rng.below(8) {
            0 => b"\x1b]0;",
            1 => b"\x1b]52;c;",
            2 => b"\x1b]8;;https://example.test/",
            3 => b"\x1bPq",
            4 => b"\x1bPz",
            5 => b"\x1b_G",
            6 => b"\x1b_",
            _ => b"\x1bP+q",
        };
        out.extend_from_slice(introducer);
        let len = match self.rng.below(4) {
            0 => self.rng.range(100, 5000),
            1 => self.rng.range(5000, 100_000),
            2 => self.rng.range(100_000, 600_000),
            _ => self.rng.range(1_000_000, 1_600_000),
        };
        let start = out.len();
        self.printable(out, len.min(4096));
        let block: Vec<u8> = out[start..].to_vec();
        while out.len() - start < len {
            let take = block.len().min(len - (out.len() - start));
            out.extend_from_slice(&block[..take]);
        }
        match self.rng.below(5) {
            0 => out.push(0x18),
            1 => out.extend_from_slice(b"\x1b\\"),
            2 => out.push(0x07),
            // left open: whatever comes next is swallowed into the string
            _ => {},
        }
    }

    fn kitty(&mut self, out: &mut Vec<u8>) {
        for _ in 0..self.rng.range(1, 5) {
            let id = self.rng.range(0, 40);
            match self.rng.below(7) {
                0 | 1 => {
                    let big = self.rng.chance(12);
                    let (width, height) = if big {
                        (self.rng.range(200, 600), self.rng.range(200, 600))
                    } else {
                        (self.rng.range(1, 48), self.rng.range(1, 48))
                    };
                    let mut pixels = vec![0u8; width * height * 4];
                    for chunk in pixels.chunks_mut(8) {
                        let word = self.rng.next().to_le_bytes();
                        chunk.copy_from_slice(&word[..chunk.len()]);
                    }
                    let encoded = BASE64_ENCODER.encode(&pixels);
                    let pieces: Vec<&[u8]> = encoded.as_bytes().chunks(4096).collect();
                    let placement = if self.rng.chance(2) {
                        format!(",c={},r={}", self.rng.range(0, 90), self.rng.range(0, 40))
                    } else {
                        String::new()
                    };
                    for (index, piece) in pieces.iter().enumerate() {
                        let more = if index + 1 < pieces.len() { 1 } else { 0 };
                        if index == 0 {
                            out.extend_from_slice(
                                format!(
                                    "\x1b_Ga=T,f=32,s={width},v={height},i={id},q=2{placement},m={more};"
                                )
                                .as_bytes(),
                            );
                        } else {
                            out.extend_from_slice(format!("\x1b_Gm={more};").as_bytes());
                        }
                        out.extend_from_slice(piece);
                        out.extend_from_slice(b"\x1b\\");
                    }
                },
                2 => {
                    let keys = format!(
                        "a=p,i={id},p={},c={},r={},x={},y={},z={},X={},Y={},C={},q=2",
                        self.number(),
                        self.number(),
                        self.number(),
                        self.number(),
                        self.number(),
                        self.number(),
                        self.number(),
                        self.number(),
                        self.rng.below(2),
                    );
                    out.extend_from_slice(format!("\x1b_G{keys}\x1b\\").as_bytes());
                },
                3 => {
                    let what = *self.rng.pick(&[
                        "a", "A", "i", "I", "c", "C", "p", "P", "q", "Q", "x", "X", "y", "Y", "z",
                        "Z", "n", "N",
                    ]);
                    out.extend_from_slice(
                        format!(
                            "\x1b_Ga=d,d={what},i={id},p={},x={},y={},z={},q=2\x1b\\",
                            self.number(),
                            self.number(),
                            self.number(),
                            self.number()
                        )
                        .as_bytes(),
                    );
                },
                4 => {
                    out.extend_from_slice(
                        format!("\x1b_Ga=q,i={},s=1,v=1,f=24;AAAA\x1b\\", self.number()).as_bytes(),
                    );
                },
                5 => {
                    // declared dimensions far beyond the payload
                    out.extend_from_slice(
                        format!(
                            "\x1b_Ga=T,f=32,s={},v={},i={id},q=2;AAAA\x1b\\",
                            self.number(),
                            self.number()
                        )
                        .as_bytes(),
                    );
                },
                _ => {
                    out.extend_from_slice(b"\x1b_G");
                    for _ in 0..self.rng.range(0, 10) {
                        out.push(*self.rng.pick(b"asvfimtcrxyzpqdCSOXYM"));
                        out.push(b'=');
                        out.extend_from_slice(self.number().as_bytes());
                        out.push(b',');
                    }
                    out.push(b';');
                    let len = self.rng.range(0, 3000);
                    self.printable(out, len);
                    out.extend_from_slice(b"\x1b\\");
                },
            }
        }
    }

    fn sixel(&mut self, out: &mut Vec<u8>) {
        for _ in 0..self.rng.range(1, 3) {
            out.extend_from_slice(b"\x1bP");
            out.extend_from_slice(self.params(3).as_bytes());
            out.push(b'q');
            // raster attributes, often huge
            out.extend_from_slice(
                format!(
                    "\"{};{};{};{}",
                    self.raster_number(),
                    self.raster_number(),
                    self.raster_number(),
                    self.raster_number()
                )
                .as_bytes(),
            );
            let pieces = if self.rng.chance(4) {
                self.rng.range(200, 4000)
            } else {
                self.rng.range(1, 150)
            };
            for _ in 0..pieces {
                match self.rng.below(8) {
                    0 => out.extend_from_slice(
                        format!(
                            "#{};2;{};{};{}",
                            self.rng.below(300),
                            self.rng.below(120),
                            self.rng.below(120),
                            self.rng.below(120)
                        )
                        .as_bytes(),
                    ),
                    1 => out.extend_from_slice(format!("#{}", self.number()).as_bytes()),
                    2 | 3 => {
                        let count = self.number();
                        out.extend_from_slice(format!("!{count}").as_bytes());
                        out.push(b'?' + self.rng.below(64) as u8);
                    },
                    4 => out.push(b'$'),
                    5 => out.push(b'-'),
                    _ => {
                        for _ in 0..self.rng.range(1, 30) {
                            out.push(b'?' + self.rng.below(64) as u8);
                        }
                    },
                }
            }
            if !self.rng.chance(10) {
                out.extend_from_slice(b"\x1b\\");
            }
        }
    }

    fn wide_and_combining(&mut self, out: &mut Vec<u8>) {
        const SAMPLES: &[&str] = &[
            "\u{4f60}\u{597d}",
            "\u{1f600}",
            "\u{1f468}\u{200d}\u{1f469}\u{200d}\u{1f467}",
            "\u{1f1fa}\u{1f1f8}",
            "e\u{301}",
            "a\u{300}\u{301}\u{302}\u{303}",
            "\u{0915}\u{094d}\u{0937}",
            "\u{0627}\u{0644}\u{0639}\u{0631}\u{0628}\u{064a}\u{0629}",
            "\u{200b}",
            "\u{fe0f}",
            "\u{ff21}\u{ff22}",
            "\u{1f3fb}",
        ];
        for _ in 0..self.rng.range(1, 60) {
            match self.rng.below(5) {
                0 => {
                    // one base character with many marks
                    out.push(b'a' + self.rng.below(26) as u8);
                    for _ in 0..self.rng.range(1, 400) {
                        let mark = 0x0300 + self.rng.below(0x70) as u32;
                        out.extend_from_slice(char::from_u32(mark).unwrap().to_string().as_bytes());
                    }
                },
                1 => {
                    // distinct combinations, to fill the cluster table
                    out.push(b'a' + self.rng.below(26) as u8);
                    for _ in 0..self.rng.range(2, 6) {
                        let mark = 0x0300 + self.rng.below(0x70) as u32;
                        out.extend_from_slice(char::from_u32(mark).unwrap().to_string().as_bytes());
                    }
                },
                _ => {
                    for _ in 0..self.rng.range(1, 80) {
                        out.extend_from_slice(self.rng.pick(SAMPLES).as_bytes());
                    }
                },
            }
            if self.rng.chance(6) {
                out.extend_from_slice(b"\r\n");
            }
        }
    }

    fn scroll_region(&mut self, out: &mut Vec<u8>) {
        for _ in 0..self.rng.range(1, 6) {
            match self.rng.below(5) {
                0 => out.extend_from_slice(b"\x1b[r"),
                1 => out.extend_from_slice(
                    format!("\x1b[{};{}r", self.number(), self.number()).as_bytes(),
                ),
                2 => out.extend_from_slice(
                    format!("\x1b[{};{}r", self.rng.range(1, 30), self.rng.range(1, 30)).as_bytes(),
                ),
                3 => out.extend_from_slice(
                    format!(
                        "\x1b[{}{}",
                        self.number(),
                        self.rng.pick(&['S', 'T', 'L', 'M'])
                    )
                    .as_bytes(),
                ),
                _ => {
                    out.push(0x1b);
                    out.push(*self.rng.pick(b"MDE78"));
                },
            }
            for _ in 0..self.rng.range(0, 60) {
                out.extend_from_slice(b"line of text\r\n");
            }
        }
    }

    fn alternate_screen(&mut self, out: &mut Vec<u8>) {
        for _ in 0..self.rng.range(1, 6) {
            let mode = *self.rng.pick(&["47", "1047", "1049"]);
            let action = if self.rng.chance(2) { 'h' } else { 'l' };
            out.extend_from_slice(format!("\x1b[?{mode}{action}").as_bytes());
            for _ in 0..self.rng.range(0, 80) {
                out.extend_from_slice(b"alternate screen text\r\n");
            }
        }
    }

    fn erase_and_repeat(&mut self, out: &mut Vec<u8>) {
        for _ in 0..self.rng.range(1, 12) {
            let count = self.number();
            let finals = *self
                .rng
                .pick(&['J', 'K', '@', 'P', 'b', 'X', 'L', 'M', 'C', 'D', 'G']);
            let mode = if matches!(finals, 'J' | 'K') {
                format!("{}", self.rng.below(4))
            } else {
                count
            };
            let text = self.word();
            out.extend_from_slice(text.as_bytes());
            out.extend_from_slice(format!("\x1b[{mode}{finals}").as_bytes());
        }
    }
}

// ---------------------------------------------------------------------------
// the harness
// ---------------------------------------------------------------------------

struct Soak {
    pane: TerminalPane,
    links: Rc<RefCell<LinkHandler>>,
    sixel_store: Rc<RefCell<SixelImageStore>>,
    kitty_store: Rc<RefCell<KittyImageStore>>,
    rows: usize,
    columns: usize,
    fed: usize,
}

impl Soak {
    fn new() -> Self {
        let mut geom = PaneGeom::default();
        geom.cols.set_inner(INITIAL_COLUMNS);
        geom.rows.set_inner(INITIAL_ROWS);
        let sixel_store = Rc::new(RefCell::new(SixelImageStore::default()));
        let kitty_store = Rc::new(RefCell::new(KittyImageStore::with_quota(KITTY_QUOTA_BYTES)));
        let pane = TerminalPane::new(
            1,
            geom,
            Style::default(),
            0,
            String::new(),
            Rc::new(RefCell::new(Some(SizeInPixels {
                width: 8,
                height: 16,
            }))),
            sixel_store.clone(),
            kitty_store.clone(),
            Rc::new(RefCell::new(Palette::default())),
            Rc::new(RefCell::new(HashMap::new())),
            None,
            None,
            false,
            true,
            true,
            true,
            false,
            None,
        );
        let links = pane.grid.link_handler.clone();
        Soak {
            pane,
            links,
            sixel_store,
            kitty_store,
            rows: INITIAL_ROWS,
            columns: INITIAL_COLUMNS,
            fed: 0,
        }
    }

    /// Hand bytes to the pane as the tab does: drain everything the pane
    /// queued for other threads, and when a host query paused the pane,
    /// answer it at once and replay the bytes held back.
    fn feed(&mut self, bytes: &[u8]) {
        self.fed += bytes.len();
        self.pane.handle_pty_bytes(bytes.to_vec());
        for _ in 0..100_000 {
            self.pane.drain_messages_to_pty();
            self.pane.drain_nested_session_messages();
            self.pane.drain_clipboard_update();
            self.pane.drain_desktop_notifications();
            self.pane.drain_osc7_cwd();
            self.pane.drain_prompt_return();
            self.pane.drain_pane_signals();
            if self.pane.drain_forwarded_queries().is_empty() {
                return;
            }
            self.pane.arm_forward_pause();
            self.pane.clear_forward_pause();
            let held = self.pane.drain_pending_pty_input();
            if held.is_empty() {
                return;
            }
            self.pane.handle_pty_bytes(held);
        }
        panic!("the pane kept pausing for host queries without making progress");
    }

    fn resize(&mut self, rows: usize, columns: usize) {
        let mut geom = PaneGeom::default();
        geom.cols.set_inner(columns);
        geom.rows.set_inner(rows);
        self.pane.set_geom(geom);
        self.rows = rows;
        self.columns = columns;
    }

    /// Everything that must hold between any two chunks of output.
    fn check_bounds(&mut self) {
        let scrollback_limit = *SCROLL_BUFFER_SIZE.get().unwrap();
        let state = self.pane.grid.retained_state();
        let at = format!("after {} bytes: {state:?}", self.fed);

        // screen geometry
        assert_eq!(self.pane.grid.height, self.rows, "grid height, {at}");
        assert_eq!(self.pane.grid.width, self.columns, "grid width, {at}");
        let viewport_limit = self.rows;
        assert!(
            state.viewport <= viewport_limit,
            "viewport has {} rows for a pane of {}, {at}",
            state.viewport,
            self.rows
        );
        assert!(
            state.widest_viewport_row <= self.columns,
            "a viewport row is {} columns wide in a pane of {}, {at}",
            state.widest_viewport_row,
            self.columns
        );

        // scrollback
        assert!(
            state.lines_above <= scrollback_limit,
            "scrollback holds {} rows (limit {scrollback_limit}), {at}",
            state.lines_above
        );
        assert!(
            state.lines_below <= scrollback_limit,
            "lines below hold {} rows (limit {scrollback_limit}), {at}",
            state.lines_below
        );
        assert!(
            state.saved_lines_above <= scrollback_limit,
            "the saved primary screen holds {} scrollback rows, {at}",
            state.saved_lines_above
        );
        assert!(
            state.saved_viewport <= state.viewport.max(self.rows.max(INITIAL_ROWS)) * 4 + 128,
            "the saved primary viewport holds {} rows, {at}",
            state.saved_viewport
        );

        // titles
        assert!(
            state.title_bytes <= MAX_TITLE_BYTES,
            "title is {} bytes, {at}",
            state.title_bytes
        );
        assert!(
            state.longest_stacked_title <= MAX_TITLE_BYTES,
            "a stacked title is {} bytes, {at}",
            state.longest_stacked_title
        );
        // The stack evicts before it pushes, so it settles one over its
        // nominal size.
        assert!(
            state.title_stack_depth <= MAX_TITLE_STACK_SIZE + 1,
            "title stack is {} deep, {at}",
            state.title_stack_depth
        );

        // DCS payloads assembled for the grid
        assert!(
            state.pending_ui_component_bytes <= MAX_UI_COMPONENT_DCS_BYTES,
            "{at}"
        );
        assert!(
            state.pending_nested_frame_bytes <= MAX_NESTED_FRAME_DCS_BYTES,
            "{at}"
        );
        assert!(
            state.pending_xtgettcap_bytes <= MAX_XTGETTCAP_DCS_BYTES,
            "{at}"
        );

        // OSC payload in the parser and APC capture in the interceptor
        let osc_capacity = self.pane.vte_parser.osc_capacity();
        assert!(
            osc_capacity <= OSC_CAPACITY_WHILE_OPEN,
            "parser OSC buffer has capacity {osc_capacity}, {at}"
        );
        let (capture_len, capture_capacity) =
            self.pane.kitty_interceptor.buffered_len_and_capacity();
        assert!(
            capture_len <= KITTY_CAPTURE_LIMIT && capture_capacity <= 2 * KITTY_CAPTURE_LIMIT,
            "kitty capture holds {capture_len} bytes (capacity {capture_capacity}), {at}"
        );
        assert_eq!(
            self.pane.pending_pty_input.len(),
            0,
            "bytes still held back for a host reply, {at}"
        );

        // notification bookkeeping
        assert!(
            state.tracked_notification_id_bytes <= NOTIFICATION_ID_BYTES_LIMIT,
            "tracked notification ids hold {} bytes, {at}",
            state.tracked_notification_id_bytes
        );

        // hyperlinks
        let (link_bytes, link_cap) = self.links.borrow().retained_bytes_and_cap();
        assert!(
            link_bytes <= link_cap,
            "hyperlink handler retains {link_bytes} bytes (cap {link_cap}), {at}"
        );

        // grapheme clusters
        assert!(
            cluster_table::bytes() <= cluster_table::DEFAULT_BYTE_LIMIT,
            "cluster table holds {} bytes, {at}",
            cluster_table::bytes()
        );

        // kitty images
        let kitty_limit = KITTY_QUOTA_BYTES;
        let kitty_bytes = self.kitty_store.borrow().total_bytes();
        assert!(
            kitty_bytes <= kitty_limit,
            "kitty store holds {kitty_bytes} bytes (quota {KITTY_QUOTA_BYTES}), {at}"
        );

        // sixel images: every one needs at least a cell to sit in
        let sixel_images = self.sixel_store.borrow().image_count();
        let cells_that_can_hold_an_image = state.lines_above
            + state.lines_below
            + state.viewport.max(self.rows)
            + state.saved_lines_above
            + state.saved_viewport;
        assert!(
            sixel_images <= cells_that_can_hold_an_image,
            "sixel store holds {sixel_images} images for {cells_that_can_hold_an_image} rows, {at}"
        );
    }

    /// End every string in progress, then check that the buffers that grew
    /// during the case have been released.
    fn settle_and_check(&mut self) {
        self.feed(b"\x18\x1b\\\x07\x18\x1b\\\x18");
        self.check_bounds();
        let state = self.pane.grid.retained_state();
        assert_eq!(
            (
                state.pending_ui_component_bytes,
                state.pending_nested_frame_bytes,
                state.pending_xtgettcap_bytes
            ),
            (0, 0, 0),
            "DCS payloads still pending after the strings were cancelled"
        );
        let osc_capacity = self.pane.vte_parser.osc_capacity();
        assert!(
            osc_capacity <= OSC_CAPACITY_AT_REST,
            "parser keeps an OSC buffer of capacity {osc_capacity} at rest"
        );
    }
}

/// A random pane size. Panes are at least two columns wide: a wide character
/// reflowed into a single column overflows it (see
/// `a_wide_character_reflowed_into_a_one_column_pane_does_not_overflow_it`).
fn random_size(gen: &mut Gen) -> (usize, usize) {
    match gen.rng.below(10) {
        0 => (1, gen.rng.range(2, 400)),
        1 => (gen.rng.range(1, 80), 2),
        2 => (2, 2),
        3 => (gen.rng.range(100, 300), gen.rng.range(100, 500)),
        _ => (gen.rng.range(2, 70), gen.rng.range(2, 250)),
    }
}

/// One step of a case: output, or something the user does to the pane.
#[derive(Clone, Debug)]
enum Op {
    Feed(Vec<u8>),
    Resize(usize, usize),
    ScrollUp(usize),
    ScrollDown(usize),
    ClearScroll,
}

/// Cut a batch of generated output into chunks of random size, as the pty
/// reader would, and occasionally resize or scroll between two chunks.
fn push_chunked(gen: &mut Gen, batch: &[u8], with_resizes: bool, ops: &mut Vec<Op>) {
    let mut rest = batch;
    while !rest.is_empty() {
        let take = match gen.rng.below(4) {
            // a few bytes at a time: every parser state gets split
            0 => gen.rng.range(1, 3),
            1 => gen.rng.range(1, 200),
            2 => gen.rng.range(1, 8192),
            _ => rest.len(),
        }
        .min(rest.len());
        let (chunk, tail) = rest.split_at(take);
        ops.push(Op::Feed(chunk.to_vec()));
        rest = tail;
        if with_resizes && gen.rng.chance(40) {
            let (rows, columns) = random_size(gen);
            ops.push(Op::Resize(rows, columns));
        }
        if gen.rng.chance(60) {
            let lines = gen.rng.range(1, 5000);
            ops.push(if gen.rng.chance(2) {
                Op::ScrollUp(lines)
            } else {
                Op::ScrollDown(lines)
            });
            if gen.rng.chance(3) {
                ops.push(Op::ClearScroll);
            }
        }
    }
}

/// `budget` bytes of every kind of hostile output mixed together.
fn plan_mixed(seed: u64, budget: usize, with_resizes: bool) -> Vec<Op> {
    let mut gen = Gen::new(seed);
    let mut ops = Vec::new();
    let mut batch = Vec::new();
    let mut fed = 0;
    while fed < budget {
        batch.clear();
        for _ in 0..gen.rng.range(1, 12) {
            let kind = gen.pick_kind();
            gen.emit(kind, &mut batch);
        }
        fed += batch.len();
        push_chunked(&mut gen, &batch, with_resizes, &mut ops);
    }
    ops
}

/// `budget` bytes of a single kind of output on a pane that keeps its size.
fn plan_single_kind(kind: Kind, seed: u64, budget: usize) -> Vec<Op> {
    let mut gen = Gen::new(seed);
    let mut ops = Vec::new();
    let mut batch = Vec::new();
    let mut fed = 0;
    while fed < budget {
        batch.clear();
        for _ in 0..8 {
            gen.emit(kind, &mut batch);
        }
        fed += batch.len();
        for chunk in batch.chunks(1 + gen.rng.below(16_384)) {
            ops.push(Op::Feed(chunk.to_vec()));
        }
    }
    ops
}

/// Run `ops` on a fresh pane and call `after_each` after every one.
fn replay(ops: &[Op], after_each: &mut dyn FnMut(&mut Soak)) -> Soak {
    let mut soak = Soak::new();
    for op in ops {
        match op {
            Op::Feed(bytes) => soak.feed(bytes),
            Op::Resize(rows, columns) => soak.resize(*rows, *columns),
            Op::ScrollUp(lines) => soak.pane.scroll_up(*lines, 1),
            Op::ScrollDown(lines) => soak.pane.scroll_down(*lines, 1),
            Op::ClearScroll => soak.pane.clear_scroll(),
        }
        after_each(&mut soak);
    }
    soak
}

/// Run a plan, checking the bounds after every step and, at the end, after
/// every string in progress has been cancelled. A failure prints what is
/// needed to rerun it.
fn run_plan(label: &str, ops: &[Op]) {
    let outcome = catch_unwind(AssertUnwindSafe(|| {
        let mut soak = replay(ops, &mut |soak| soak.check_bounds());
        soak.settle_and_check();
        drop(soak);
        assert_eq!(
            (cluster_table::live_entries(), cluster_table::bytes()),
            (0, 0),
            "grapheme clusters leaked after the pane was dropped"
        );
    }));
    if let Err(panic) = outcome {
        eprintln!("hostile output soak failed: {label}");
        resume_unwind(panic);
    }
}

fn run_case(seed: u64, budget: usize, with_resizes: bool) {
    run_plan(
        &format!("mixed seed={seed} budget={budget} resizes={with_resizes}"),
        &plan_mixed(seed, budget, with_resizes),
    );
}

/// How many bytes of each kind a default run feeds. What a byte costs
/// differs by orders of magnitude: a repeat or erase count of 65535 makes a
/// ten-byte sequence touch 65535 cells.
fn single_kind_budget(kind: Kind) -> usize {
    match kind {
        Kind::Unterminated => 3_000_000,
        Kind::Kitty => 300_000,
        Kind::Csi => 20_000,
        Kind::Sixel => 30_000,
        Kind::ScrollRegion => 15_000,
        Kind::EraseAndRepeat => 8_000,
        _ => 150_000,
    }
}

// ---------------------------------------------------------------------------
// tests
// ---------------------------------------------------------------------------

#[test]
fn hostile_output_keeps_retained_state_bounded() {
    for seed in 0..5 {
        run_case(seed, 50_000, seed % 3 != 0);
    }
}

/// Many more seeds and bigger budgets. Run by hand:
/// `cargo test -p zellij-server --lib hostile_output_heavy -- --ignored --nocapture`
/// `ZELLIJ_SOAK_SEEDS` sets the number of seeds (default 40).
#[test]
#[ignore = "long-running soak; run manually"]
fn hostile_output_heavy() {
    let seeds: u64 = std::env::var("ZELLIJ_SOAK_SEEDS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(40);
    for seed in 1_000..1_000 + seeds {
        run_case(seed, 1_500_000, seed % 4 != 0);
    }
}

/// Each kind of output on its own, on a pane with a fixed size: a kind that
/// is only dangerous alone cannot hide behind the others.
#[test]
fn every_kind_of_hostile_output_alone_keeps_state_bounded() {
    for (index, kind) in ALL_KINDS.iter().enumerate() {
        let seed = 7_000 + index as u64;
        run_plan(
            &format!("kind={kind:?} seed={seed}"),
            &plan_single_kind(*kind, seed, single_kind_budget(*kind)),
        );
    }
}

/// A string that never ends must stop being buffered at its limit, however
/// much more arrives, and must leave nothing behind once cancelled.
#[test]
fn strings_longer_than_their_limits_are_discarded_not_buffered() {
    const MIB: usize = 1024 * 1024;
    let cases: &[(&str, &[u8], usize)] = &[
        ("OSC title", b"\x1b]0;", 4 * MIB + MIB / 2),
        ("OSC clipboard", b"\x1b]52;c;", 4 * MIB + MIB / 2),
        (
            "OSC hyperlink",
            b"\x1b]8;;https://example.test/",
            4 * MIB + MIB / 2,
        ),
        ("DCS UI component", b"\x1bPz", 4 * MIB + MIB / 2),
        ("DCS nested frame", b"\x1bP26661n", MIB + MIB / 4),
        ("DCS XTGETTCAP", b"\x1bP+q", 100_000),
        ("APC kitty", b"\x1b_G", MIB + MIB / 4),
        ("APC other", b"\x1b_x", 4 * MIB + MIB / 2),
        ("PM", b"\x1b^x", 4 * MIB + MIB / 2),
        ("SOS", b"\x1bXx", 4 * MIB + MIB / 2),
    ];
    for (name, introducer, length) in cases {
        let outcome = catch_unwind(AssertUnwindSafe(|| {
            let mut soak = Soak::new();
            soak.feed(introducer);
            let block = vec![b'x'; 64 * 1024];
            let mut fed = 0;
            while fed < *length {
                soak.feed(&block);
                soak.check_bounds();
                fed += block.len();
            }
            assert!(
                soak.pane.grid.title.is_none(),
                "an over-long title was applied"
            );
            // the string ends, and ordinary output works again
            soak.feed(b"\x1b\\\x07\x1b[2J\x1b[Hafter");
            soak.settle_and_check();
            let text: String = soak.pane.read_buffer_as_lines()[0]
                .iter()
                .map(|cell| cell.character)
                .collect();
            assert!(
                text.starts_with("after"),
                "output after the string: {text:?}"
            );
        }));
        if let Err(panic) = outcome {
            eprintln!("string limit failed: {name}");
            resume_unwind(panic);
        }
    }
}

/// Scrollback fills to its limit and stops there, through floods of text,
/// narrow and wide panes, and wide characters that reflow.
#[test]
fn scrollback_fills_to_its_limit_and_stops_there() {
    let mut soak = Soak::new();
    let limit = *SCROLL_BUFFER_SIZE.get().unwrap();
    let sizes = [
        (24, 80),
        (30, 100),
        (10, 20),
        (24, 7),
        (50, 40),
        (3, 300),
        (24, 80),
    ];
    let mut line = 0;
    for (round, (rows, columns)) in sizes.iter().enumerate() {
        soak.resize(*rows, *columns);
        let mut text = Vec::new();
        for _ in 0..limit / 2 + 500 {
            line += 1;
            if round % 2 == 0 {
                text.extend_from_slice(format!("flood line {line}\r\n").as_bytes());
            } else {
                text.extend_from_slice(
                    format!("\u{4f60}\u{597d} wide \u{1f600} e\u{301} {line}\r\n").as_bytes(),
                );
            }
            if text.len() > 64 * 1024 {
                soak.feed(&text);
                soak.check_bounds();
                text.clear();
            }
        }
        soak.feed(&text);
        soak.check_bounds();
    }
    soak.resize(24, 80);
    let mut text = Vec::new();
    for _ in 0..limit + limit / 2 {
        line += 1;
        text.extend_from_slice(format!("flood line {line}\r\n").as_bytes());
    }
    soak.feed(&text);
    soak.check_bounds();
    assert_eq!(
        soak.pane.grid.retained_state().lines_above,
        limit,
        "scrollback is full at a settled size"
    );
}

/// Titles: the stack stops growing at its depth and titles at their length.
#[test]
fn title_stack_depth_and_title_length_are_capped() {
    let mut soak = Soak::new();
    let title = "t".repeat(MAX_TITLE_BYTES * 3);
    for round in 0..(MAX_TITLE_STACK_SIZE + 500) {
        soak.feed(format!("\x1b]2;{round}{title}\x07\x1b[22;0t").as_bytes());
        if round % 100 == 0 {
            soak.check_bounds();
        }
    }
    soak.check_bounds();
    let state = soak.pane.grid.retained_state();
    assert!(state.title_stack_depth >= MAX_TITLE_STACK_SIZE);
    for _ in 0..(MAX_TITLE_STACK_SIZE + 500) {
        soak.feed(b"\x1b[23;0t");
    }
    soak.check_bounds();
}

/// Hyperlink targets are retained up to a byte cap, and past it new targets
/// are left as text.
#[test]
fn hyperlink_targets_stop_being_retained_at_the_byte_cap() {
    let mut soak = Soak::new();
    let tail = "p".repeat(8000);
    let mut text = Vec::new();
    for id in 0..2600 {
        text.extend_from_slice(
            format!("\x1b]8;id=l{id};https://example.test/{id}/{tail}\x1b\\x\x1b]8;;\x1b\\\r\n")
                .as_bytes(),
        );
        if text.len() > 512 * 1024 {
            soak.feed(&text);
            soak.check_bounds();
            text.clear();
        }
    }
    soak.feed(&text);
    soak.check_bounds();
    let (retained, cap) = soak.links.borrow().retained_bytes_and_cap();
    assert!(
        retained > cap / 2,
        "only {retained} of {cap} bytes were reached; the test no longer fills the cap"
    );
}

/// Kitty images: enough image data to overflow the store several times over.
#[test]
fn kitty_images_stay_within_the_store_quota() {
    let mut soak = Soak::new();
    let mut gen = Gen::new(99);
    for id in 1..=14 {
        let (width, height) = (512, 512);
        let mut pixels = vec![0u8; width * height * 4];
        for chunk in pixels.chunks_mut(8) {
            let word = gen.rng.next().to_le_bytes();
            chunk.copy_from_slice(&word[..chunk.len()]);
        }
        let encoded = BASE64_ENCODER.encode(&pixels);
        let pieces: Vec<&[u8]> = encoded.as_bytes().chunks(4096).collect();
        let mut bytes = Vec::new();
        for (index, piece) in pieces.iter().enumerate() {
            let more = (index + 1 < pieces.len()) as u8;
            if index == 0 {
                bytes.extend_from_slice(
                    format!("\x1b_Ga=T,f=32,s={width},v={height},i={id},q=2,c=10,r=5,m={more};")
                        .as_bytes(),
                );
            } else {
                bytes.extend_from_slice(format!("\x1b_Gm={more};").as_bytes());
            }
            bytes.extend_from_slice(piece);
            bytes.extend_from_slice(b"\x1b\\");
        }
        soak.feed(&bytes);
        soak.check_bounds();
    }
    assert!(
        soak.kitty_store.borrow().total_bytes() > 0,
        "no image was kept; the test no longer exercises the store"
    );
}

/// Time the feeding of about `target_bytes` of `kind`. With `cycle`, the
/// input repeats a fixed set of sixteen items, so that its cost per byte
/// does not depend on how many unlucky items a prefix happens to contain.
fn time_to_feed(kind: Kind, seed: u64, target_bytes: usize, cycle: bool) -> (usize, Duration) {
    let mut gen = Gen::new(seed);
    let mut input = Vec::with_capacity(target_bytes + 1_000_000);
    if cycle {
        let items: Vec<Vec<u8>> = (0..16)
            .map(|_| {
                let mut item = Vec::new();
                gen.emit(kind, &mut item);
                item
            })
            .collect();
        let mut next = 0;
        while input.len() < target_bytes {
            input.extend_from_slice(&items[next % items.len()]);
            next += 1;
        }
    } else {
        while input.len() < target_bytes {
            gen.emit(kind, &mut input);
        }
    }
    // end whatever string the input left open, outside the timed part
    input.extend_from_slice(b"\x18\x1b\\\x18");
    let mut soak = Soak::new();
    let started = Instant::now();
    for chunk in input.chunks(16 * 1024) {
        soak.feed(chunk);
    }
    (input.len(), started.elapsed())
}

/// Four times the input must cost about four times the CPU. Quadratic
/// behaviour makes it sixteen times; the threshold sits well between, and a
/// slow run is repeated before it is held against the code. The inputs are
/// the same seeds every run, so only the machine's load varies.
#[test]
fn cpu_grows_linearly_with_hostile_input() {
    // (kind, bytes of the small input, whether the cost of one byte varies
    // so much between items that the input has to repeat a fixed set);
    // the large input is four times the small one
    let kinds: &[(Kind, usize, bool)] = &[
        (Kind::PlainText, 300_000, false),
        (Kind::RandomBytes, 150_000, false),
        (Kind::Csi, 8_000, true),
        (Kind::Hyperlinks, 200_000, false),
        (Kind::Titles, 200_000, false),
        (Kind::Clipboard, 200_000, false),
        (Kind::Notification, 200_000, false),
        (Kind::Unterminated, 600_000, false),
        (Kind::Kitty, 300_000, false),
        (Kind::Sixel, 15_000, false),
        (Kind::WideAndCombining, 150_000, false),
        (Kind::ScrollRegion, 8_000, true),
        (Kind::EraseAndRepeat, 3_000, true),
    ];
    const FACTOR: usize = 4;
    let mut slow = Vec::new();
    for (index, (kind, small_target, cycle)) in kinds.iter().enumerate() {
        let seed = 31_000 + index as u64;
        let mut verdict = None;
        for _attempt in 0..3 {
            let (small_bytes, small) = time_to_feed(*kind, seed, *small_target, *cycle);
            let (large_bytes, large) = time_to_feed(*kind, seed, small_target * FACTOR, *cycle);
            let per_byte_growth = (large.as_secs_f64() / large_bytes as f64)
                / (small.as_secs_f64().max(0.005) / small_bytes as f64);
            verdict = Some((per_byte_growth, small, large));
            // linear is 1.0; quadratic would be about 4.0
            if per_byte_growth < 2.5 {
                verdict = None;
                break;
            }
        }
        if let Some((growth, small, large)) = verdict {
            slow.push(format!(
                "{kind:?}: {small_target} bytes took {small:?}, {} bytes took {large:?} (cost per byte grew {growth:.1}x)",
                small_target * FACTOR
            ));
        }
    }
    assert!(slow.is_empty(), "superlinear CPU: {slow:#?}");
}

// ---------------------------------------------------------------------------
// bugs found by the soak, kept as regressions (fixed by fix/pane-output-bounds
// and fix/graphics-cache-bounds)
// ---------------------------------------------------------------------------

/// Was a bug: a scroll region whose top lies below the last row (`CSI 42;96 r` on
/// a 24-row pane), followed by a scroll (`CSI S`), adds rows to the
/// viewport: it ends up with as many rows as the region's top margin, 42 in
/// this case, instead of 24, until the next resize. The growth is bounded
/// by the 16-bit region parameters, so memory is not at risk, but the
/// viewport no longer matches the pane. Found by the mixed soak, seed 7020
/// with `Kind::Csi`; the default runs check only the 16-bit bound.
#[test]
fn scroll_region_below_the_screen_keeps_the_viewport_at_the_pane_height() {
    let mut soak = Soak::new();
    soak.feed(b"\x1b[42;96r\x1b[S");
    soak.check_bounds();
}

/// Was a bug: a double-width character that is reflowed into a one-column pane
/// stays two columns wide, so the row is wider than the pane. Found by the
/// mixed soak, seed 4 (before panes of one column were left out of
/// `random_size`).
#[test]
fn a_wide_character_reflowed_into_a_one_column_pane_does_not_overflow_it() {
    let mut soak = Soak::new();
    soak.feed("\u{ff13}".as_bytes());
    soak.resize(1, 1);
    soak.check_bounds();
}

/// Was a bug: the CPU cost of a sixel image is quadratic in the height declared
/// by its raster attributes, and is paid before any image data arrives.
/// The 16 bytes below, a raster attribute of height 65536 and nothing
/// else, take over 30 s in a debug build; a declared height of 32000 takes
/// about 2 s and 16000 about 0.5 s, so doubling the height quadruples the
/// time. Found by the CPU-growth test (`Kind::Sixel`, seed 31009: one
/// 18027-byte item took 32 s while the other 30 took 2 s in total) and
/// reduced by delta debugging to the sequence below. The sixel generator
/// declares heights of at most 1500, and values that overflow, which are
/// rejected quickly.
#[test]
fn a_tall_sixel_raster_is_rejected_cheaply() {
    let mut soak = Soak::new();
    let started = Instant::now();
    soak.feed(b"\x1bPq\"1;2;2;65536!");
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "16 bytes of sixel took {:?}",
        started.elapsed()
    );
}

/// Was a bug: every placement of an image at a size other than its own adds a
/// scaled copy of the image to the store (`add_scaled_variant`), and the
/// copies are charged to the quota but never refused or evicted while the
/// image is placed, nor dropped when a placement goes away. One 2x2 pixel
/// image (16 bytes) placed at 1x1, 2x2, ... 60x60 cells holds the store at
/// well over its 4 MiB quota after about 30 placements, and a program can
/// go on for as long as it likes: the size of a copy is limited only by the
/// 16-bit cell counts. Found by the single-kind soak, `Kind::Kitty`, seed
/// 7012 (store at 4.5 MB of 4 MiB after 75 KB of output) and by
/// `kitty_images_stay_within_the_store_quota`, and reduced by hand.
#[test]
fn scaled_kitty_placements_stay_within_the_store_quota() {
    let mut soak = Soak::new();
    let pixels = BASE64_ENCODER.encode([0u8; 16]);
    soak.feed(format!("\x1b_Ga=t,f=32,s=2,v=2,i=1,q=2;{pixels}\x1b\\").as_bytes());
    for cells in 1..=60 {
        soak.feed(format!("\x1b_Ga=p,i=1,c={cells},r={cells},q=2\x1b\\").as_bytes());
        soak.check_bounds();
    }
}

/// A pane that is shrunk while it shows the alternate screen must shrink the
/// alternate screen's viewport with it. Found by the heavy soak, seed 1002: a
/// one-row pane held a 24-row viewport.
#[test]
fn shrinking_a_pane_on_the_alternate_screen_shrinks_its_viewport() {
    let mut soak = Soak::new();
    soak.feed(b"\x1b[?1049h");
    soak.feed("row\r\n".repeat(INITIAL_ROWS).as_bytes());
    soak.resize(1, 40);
    soak.check_bounds();
    soak.feed(b"\x1b[?1049l");
    soak.check_bounds();
    soak.resize(INITIAL_ROWS, INITIAL_COLUMNS);
    soak.check_bounds();
}
