use std::collections::BTreeMap;
use std::path::Path;
use std::rc::Rc;

use base64::alphabet::STANDARD as STANDARD_ALPHABET;
use base64::engine::general_purpose::{GeneralPurpose, GeneralPurposeConfig};
use base64::engine::{DecodePaddingMode, Engine as _};

const BASE64: GeneralPurpose = GeneralPurpose::new(
    &STANDARD_ALPHABET,
    GeneralPurposeConfig::new().with_decode_padding_mode(DecodePaddingMode::Indifferent),
);

const MAX_CSI_PARAMS: usize = 32;
const MAX_FORWARDED: usize = 2;
const MAX_OSC_LEN: usize = 8192;
const MAX_IMAGE_BYTES: usize = 104_857_600;
const SHM_DIR: &str = "/dev/shm";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FilterState {
    Ground,
    Esc,
    EscBracket,
    OscForward,
    OscForwardEsc,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OscSignal {
    Notification(NotificationChunk),
    Title(Option<String>),
    Clipboard(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notification {
    pub title: Option<String>,
    pub body: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotificationChunk {
    pub id: Option<String>,
    pub title: bool,
    pub text: String,
    pub done: bool,
}

fn notification_chunk(osc: &[u8]) -> Option<NotificationChunk> {
    if let Some(rest) = osc.strip_prefix(b"9;") {
        return Some(NotificationChunk {
            id: None,
            title: false,
            text: String::from_utf8(rest.to_vec()).ok()?,
            done: true,
        });
    }
    let rest = osc.strip_prefix(b"99;")?;
    let separator = rest.iter().position(|byte| *byte == b';')?;
    let metadata = std::str::from_utf8(&rest[..separator]).ok()?;
    let mut chunk = NotificationChunk {
        id: Some(String::new()),
        title: true,
        text: String::new(),
        done: true,
    };
    let mut encoded = false;
    for pair in metadata.split(':') {
        match pair.split_once('=') {
            Some(("i", value)) => chunk.id = Some(value.to_owned()),
            Some(("p", "title")) => chunk.title = true,
            Some(("p", "body")) => chunk.title = false,
            Some(("p", _)) => return None,
            Some(("d", value)) => chunk.done = value != "0",
            Some(("e", value)) => encoded = value == "1",
            _ => {},
        }
    }
    let payload = &rest[separator + 1..];
    let payload = match encoded {
        true => BASE64.decode(payload).ok()?,
        false => payload.to_vec(),
    };
    chunk.text = String::from_utf8(payload).ok()?;
    Some(chunk)
}

fn title_body(osc: &[u8]) -> Option<&[u8]> {
    for prefix in [b"0;".as_slice(), b"1;".as_slice(), b"2;".as_slice()] {
        if let Some(rest) = osc.strip_prefix(prefix) {
            return Some(rest);
        }
    }
    None
}

fn clipboard_body(osc: &[u8]) -> Option<String> {
    let rest = osc.strip_prefix(b"52;")?;
    let separator = rest.iter().position(|byte| *byte == b';')?;
    let encoded = &rest[separator + 1..];
    if encoded == b"?" {
        return None;
    }
    let decoded = BASE64.decode(encoded).ok()?;
    String::from_utf8(decoded).ok()
}

fn osc_signal(osc: &[u8]) -> Option<OscSignal> {
    if osc.starts_with(b"9;") || osc.starts_with(b"99;") {
        return notification_chunk(osc).map(OscSignal::Notification);
    }
    if let Some(body) = title_body(osc) {
        return match String::from_utf8(body.to_vec()) {
            Ok(title) if title.is_empty() => Some(OscSignal::Title(None)),
            Ok(title) => Some(OscSignal::Title(Some(title))),
            Err(_) => None,
        };
    }
    clipboard_body(osc).map(OscSignal::Clipboard)
}

pub struct Forwarded {
    bytes: [u8; MAX_FORWARDED],
    len: usize,
}

impl Forwarded {
    fn of(bytes: &[u8]) -> Self {
        let mut buffer = [0u8; MAX_FORWARDED];
        buffer[..bytes.len()].copy_from_slice(bytes);
        Self {
            bytes: buffer,
            len: bytes.len(),
        }
    }

    pub fn as_slice(&self) -> &[u8] {
        &self.bytes[..self.len]
    }
}

pub struct Step {
    pub bytes: Forwarded,
    pub kind: Kind,
}

impl Step {
    fn forward(bytes: &[u8]) -> Self {
        Self {
            bytes: Forwarded::of(bytes),
            kind: Kind::Forward,
        }
    }

    fn boundary(bytes: &[u8]) -> Self {
        Self {
            bytes: Forwarded::of(bytes),
            kind: Kind::Boundary,
        }
    }

    fn cleared(bytes: &[u8]) -> Self {
        Self {
            bytes: Forwarded::of(bytes),
            kind: Kind::Cleared,
        }
    }

    fn signal(bytes: &[u8], signal: OscSignal) -> Self {
        Self {
            bytes: Forwarded::of(bytes),
            kind: Kind::Signal(signal),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind {
    Forward,
    Cleared,
    Boundary,
    Signal(OscSignal),
}

pub struct ApcFilter {
    state: FilterState,
    params: Vec<u8>,
    osc: Vec<u8>,
}

impl ApcFilter {
    pub fn new() -> Self {
        Self {
            state: FilterState::Ground,
            params: Vec::new(),
            osc: Vec::new(),
        }
    }

    pub fn advance(&mut self, byte: u8) -> Step {
        match self.state {
            FilterState::Ground => match byte {
                0x1b => {
                    self.state = FilterState::Esc;
                    Step::forward(&[])
                },
                0x00..=0x1f | 0x7f => Step::boundary(&[byte]),
                other => Step::forward(&[other]),
            },
            FilterState::Esc => match byte {
                0x1b => Step::forward(&[0x1b]),
                b'[' => {
                    self.state = FilterState::EscBracket;
                    self.params.clear();
                    Step::forward(&[0x1b, b'['])
                },
                b']' => {
                    self.state = FilterState::OscForward;
                    self.osc.clear();
                    Step::forward(&[0x1b, b']'])
                },
                other => {
                    self.state = FilterState::Ground;
                    Step::boundary(&[0x1b, other])
                },
            },
            FilterState::EscBracket => self.control_sequence(byte),
            FilterState::OscForward => match byte {
                0x1b => {
                    self.state = FilterState::OscForwardEsc;
                    Step::forward(&[])
                },
                0x07 | 0x9c => {
                    self.state = FilterState::Ground;
                    self.finish_osc(&[byte])
                },
                other => {
                    self.record_osc(other);
                    Step::forward(&[other])
                },
            },
            FilterState::OscForwardEsc => match byte {
                0x1b => Step::forward(&[0x1b]),
                b'\\' => {
                    self.state = FilterState::Ground;
                    self.finish_osc(&[0x1b, b'\\'])
                },
                other => {
                    self.state = FilterState::OscForward;
                    self.record_osc(0x1b);
                    self.record_osc(other);
                    Step::forward(&[0x1b, other])
                },
            },
        }
    }

    #[cfg(test)]
    pub fn params(&self) -> &[u8] {
        &self.params
    }

    fn record_osc(&mut self, byte: u8) {
        if self.osc.len() < MAX_OSC_LEN {
            self.osc.push(byte);
        }
    }

    fn finish_osc(&mut self, terminator: &[u8]) -> Step {
        let body = std::mem::take(&mut self.osc);
        match osc_signal(&body) {
            Some(signal) => Step::signal(terminator, signal),
            None => Step::boundary(terminator),
        }
    }

    fn control_sequence(&mut self, byte: u8) -> Step {
        match byte {
            0x1b => {
                self.state = FilterState::Esc;
                Step::forward(&[])
            },
            0x20..=0x3f => {
                if self.params.len() < MAX_CSI_PARAMS {
                    self.params.push(byte);
                }
                Step::forward(&[byte])
            },
            0x40..=0x7e => {
                self.state = FilterState::Ground;
                match byte {
                    b'J' if self.erases_the_display() => Step::cleared(&[byte]),
                    _ => Step::boundary(&[byte]),
                }
            },
            _ => Step::forward(&[byte]),
        }
    }

    fn erases_the_display(&self) -> bool {
        matches!(self.params.as_slice(), b"2" | b"3")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

impl Image {
    #[cfg(test)]
    pub fn digest(&self) -> u64 {
        let mut hash = 0xcbf2_9ce4_8422_2325u64;
        for byte in &self.pixels {
            hash ^= *byte as u64;
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
        hash
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Placement {
    pub image_id: u32,
    pub placement_id: u32,
    pub cell_x: usize,
    pub cell_y: usize,
    pub offset_x: u32,
    pub offset_y: u32,
    pub source_x: u32,
    pub source_y: u32,
    pub source_width: u32,
    pub source_height: u32,
    pub z: i32,
}

struct Resident {
    image: Rc<Image>,
    revision: u64,
}

pub struct Graphics {
    images: BTreeMap<u32, Resident>,
    placements: BTreeMap<(u32, u32), Placement>,
    revisions: u64,
    stamp: u64,
}

impl Graphics {
    pub fn new() -> Self {
        Self {
            images: BTreeMap::new(),
            placements: BTreeMap::new(),
            revisions: 0,
            stamp: 0,
        }
    }

    pub fn stamp(&self) -> u64 {
        self.stamp
    }

    pub fn image(&self, id: u32) -> Option<&Rc<Image>> {
        self.images.get(&id).map(|resident| &resident.image)
    }

    pub fn revision(&self, id: u32) -> Option<u64> {
        self.images.get(&id).map(|resident| resident.revision)
    }

    pub fn resident_ids(&self) -> Vec<u32> {
        self.images.keys().copied().collect()
    }

    pub fn placements(&self) -> impl Iterator<Item = &Placement> {
        self.placements.values()
    }

    pub fn clear(&mut self) {
        self.stamp += 1;
        self.images.clear();
        self.placements.clear();
    }

    pub fn insert_image(&mut self, id: u32, image: Image) {
        self.revisions += 1;
        self.stamp += 1;
        self.images.insert(
            id,
            Resident {
                image: Rc::new(image),
                revision: self.revisions,
            },
        );
    }

    pub fn insert_placement(&mut self, placement: Placement) {
        if !self.images.contains_key(&placement.image_id) {
            return;
        }
        self.stamp += 1;
        self.placements
            .insert((placement.image_id, placement.placement_id), placement);
    }

    pub fn remove_image(&mut self, id: u32) {
        self.stamp += 1;
        self.images.remove(&id);
        self.placements.retain(|key, _| key.0 != id);
    }

    pub fn remove_placement(&mut self, image_id: u32, placement_id: u32) {
        self.stamp += 1;
        self.placements.remove(&(image_id, placement_id));
    }
}

pub fn read_shared_media(path: &str, expected: usize) -> Option<Vec<u8>> {
    if expected > MAX_IMAGE_BYTES {
        eprintln!(
            "zellij-window: refusing a {} byte image from {:?}",
            expected, path
        );
        return None;
    }
    let path = Path::new(path);
    if !path.starts_with(SHM_DIR) && !path.starts_with(std::env::temp_dir()) {
        eprintln!(
            "zellij-window: refusing to read graphics media from {:?}",
            path
        );
        return None;
    }
    match std::fs::read(path) {
        Ok(bytes) if bytes.len() >= expected => Some(bytes[..expected].to_vec()),
        Ok(bytes) => {
            eprintln!(
                "zellij-window: {:?} carries {} bytes, not the {} its record named",
                path,
                bytes.len(),
                expected
            );
            None
        },
        Err(e) => {
            eprintln!("zellij-window: failed to read {:?}: {}", path, e);
            None
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn observed(stream: &[u8]) -> (Vec<u8>, usize) {
        let mut filter = ApcFilter::new();
        let mut text = Vec::new();
        let mut clears = 0usize;
        for byte in stream {
            let step = filter.advance(*byte);
            text.extend_from_slice(step.bytes.as_slice());
            if step.kind == Kind::Cleared {
                clears += 1;
            }
        }
        (text, clears)
    }

    #[test]
    fn an_erase_in_display_is_reported_without_being_consumed() {
        let (text, clears) = observed(b"a\x1b[m\x1b[2Jb");
        assert_eq!(text, b"a\x1b[m\x1b[2Jb");
        assert_eq!(clears, 1);
    }

    #[test]
    fn the_scrollback_erase_is_reported_too() {
        assert_eq!(observed(b"\x1b[3J").1, 1);
    }

    #[test]
    fn a_partial_erase_or_another_final_byte_is_not_a_display_clear() {
        for stream in [
            &b"\x1b[J"[..],
            &b"\x1b[0J"[..],
            &b"\x1b[1J"[..],
            &b"\x1b[2K"[..],
            &b"\x1b[23J"[..],
            &b"\x1b[?2J"[..],
            &b"\x1b[2;3H"[..],
        ] {
            let (text, clears) = observed(stream);
            assert_eq!(text, stream, "{:?}", stream);
            assert_eq!(clears, 0, "{:?}", stream);
        }
    }

    #[test]
    fn an_aborted_control_sequence_does_not_carry_its_parameters_forward() {
        let (text, clears) = observed(b"\x1b[2\x1b[J");
        assert_eq!(text, b"\x1b[2\x1b[J");
        assert_eq!(clears, 0);
    }

    #[test]
    fn a_control_sequence_longer_than_the_parameter_budget_is_still_forwarded() {
        let mut stream = b"\x1b[".to_vec();
        stream.extend(std::iter::repeat(b'1').take(MAX_CSI_PARAMS * 2));
        stream.extend_from_slice(b"J");
        let (text, clears) = observed(&stream);
        assert_eq!(text, stream);
        assert_eq!(clears, 0);
    }

    #[test]
    fn plain_text_passes_through_untouched() {
        let (text, _) = observed(b"hello\x1b[31mworld");
        assert_eq!(text, b"hello\x1b[31mworld");
    }

    #[test]
    fn an_escape_that_is_not_an_apc_is_forwarded_whole() {
        let (text, _) = observed(b"\x1b[2J\x1b]52;c;QQ\x1b\\");
        assert_eq!(text, b"\x1b[2J\x1b]52;c;QQ\x1b\\");
    }

    #[test]
    fn an_apc_or_a_device_control_string_is_forwarded_byte_for_byte() {
        for stream in [
            &b"a\x1b_Ga=t,i=1;QUJD\x1b\\b"[..],
            &b"\x1b_Xpayload\x1b\\"[..],
            &b"a\x1bP0;1;0q#0;2;100;0;0@\x1b\\b"[..],
            &b"\x1bPq~~~\x9cz"[..],
            &b"\x1bP+q544e\x1b\\"[..],
        ] {
            let (text, clears) = observed(stream);
            assert_eq!(
                text, stream,
                "nothing captures graphics any more; {:?} must reach the parser whole",
                stream
            );
            assert_eq!(clears, 0, "{:?}", stream);
        }
    }

    #[test]
    fn a_repeated_escape_is_not_lost() {
        let (text, _) = observed(b"\x1b\x1bA");
        assert_eq!(text, b"\x1b\x1bA");
    }

    #[test]
    fn a_control_byte_inside_a_control_sequence_is_forwarded_and_does_not_break_recognition() {
        let (text, clears) = observed(b"\x1b[2\x07J");
        assert_eq!(text, b"\x1b[2\x07J");
        assert_eq!(clears, 1);
    }

    fn steps(stream: &[u8]) -> Vec<&'static str> {
        let mut filter = ApcFilter::new();
        stream
            .iter()
            .map(|byte| match filter.advance(*byte).kind {
                Kind::Forward => "forward",
                Kind::Cleared => "cleared",
                Kind::Boundary => "boundary",
                Kind::Signal(_) => "signal",
            })
            .collect()
    }

    #[test]
    fn every_control_sequence_closes_a_run_and_printable_text_does_not() {
        assert_eq!(steps(b"ab"), vec!["forward", "forward"]);
        assert_eq!(*steps(b"\x1b[31m").last().unwrap(), "boundary");
        assert_eq!(*steps(b"\x1b[s").last().unwrap(), "boundary");
        assert_eq!(*steps(b"\x1b[u").last().unwrap(), "boundary");
        assert_eq!(*steps(b"\x1b[?25l").last().unwrap(), "boundary");
        assert_eq!(*steps(b"\x1b[2;3H").last().unwrap(), "boundary");
        assert_eq!(*steps(b"\x1b7").last().unwrap(), "boundary");
        assert_eq!(*steps(b"\x1b8").last().unwrap(), "boundary");
        assert_eq!(*steps(b"\x1bM").last().unwrap(), "boundary");
        assert_eq!(*steps(b"\r").last().unwrap(), "boundary");
        assert_eq!(*steps(b"\n").last().unwrap(), "boundary");
        assert_eq!(*steps(b"\t").last().unwrap(), "boundary");
        assert_eq!(*steps(b"\x7f").last().unwrap(), "boundary");
        assert_eq!(*steps(b"\x1b[2J").last().unwrap(), "cleared");
    }

    #[test]
    fn a_boundary_carries_its_bytes_so_the_parser_still_sees_them() {
        let (text, _) = observed(b"a\x1b[31mb\rc\x1b7d");
        assert_eq!(text, b"a\x1b[31mb\rc\x1b7d");
    }

    #[test]
    fn the_digest_separates_images_that_differ_by_one_byte() {
        let one = Image {
            width: 1,
            height: 1,
            pixels: vec![1, 2, 3, 4],
        };
        let other = Image {
            width: 1,
            height: 1,
            pixels: vec![1, 2, 3, 5],
        };
        assert_eq!(one.digest(), one.clone().digest());
        assert_ne!(one.digest(), other.digest());
    }
}
