use crate::golden::{self, Digest};

#[derive(Debug, PartialEq, Clone)]
enum Event {
    Print(char),
    Execute(u8),
    Hook(Vec<Vec<u16>>, usize, Vec<u8>, bool, char),
    Put(u8),
    Unhook,
    Osc(Vec<Vec<u8>>, bool),
    Csi(Vec<Vec<u16>>, usize, Vec<u8>, bool, char),
    Esc(Vec<u8>, bool, u8),
}

#[derive(Default)]
struct Ours(Vec<Event>);

fn groups_of(parameters: &crate::vt_parser::Parameters) -> Vec<Vec<u16>> {
    parameters.iter().map(|group| group.to_vec()).collect()
}

impl crate::vt_parser::Handler for Ours {
    fn print(&mut self, c: char) {
        self.0.push(Event::Print(c));
    }
    fn execute(&mut self, b: u8) {
        self.0.push(Event::Execute(b));
    }
    fn hook(&mut self, sequence: &crate::vt_parser::Sequence) {
        self.0.push(Event::Hook(
            groups_of(sequence.parameters()),
            sequence.parameters().value_count(),
            sequence.intermediates().to_vec(),
            sequence.overflowed(),
            sequence.final_char(),
        ));
    }
    fn put(&mut self, b: u8) {
        self.0.push(Event::Put(b));
    }
    fn unhook(&mut self) {
        self.0.push(Event::Unhook);
    }
    fn osc_end(&mut self, p: &[&[u8]], bell: bool) {
        self.0
            .push(Event::Osc(p.iter().map(|x| x.to_vec()).collect(), bell));
    }
    fn csi_dispatch(&mut self, sequence: &crate::vt_parser::Sequence) {
        self.0.push(Event::Csi(
            groups_of(sequence.parameters()),
            sequence.parameters().value_count(),
            sequence.intermediates().to_vec(),
            sequence.overflowed(),
            sequence.final_char(),
        ));
    }
    fn esc_dispatch(&mut self, sequence: &crate::vt_parser::Sequence) {
        self.0.push(Event::Esc(
            sequence.intermediates().to_vec(),
            sequence.overflowed(),
            sequence.final_byte(),
        ));
    }
}

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

fn random_stream(rng: &mut Rng, len: usize) -> Vec<u8> {
    let pieces: &[&[u8]] = &[
        b"\x1b",
        b"[",
        b"]",
        b"P",
        b"X",
        b"^",
        b"_",
        b"\\",
        b";",
        b":",
        b"0",
        b"1",
        b"9",
        b"65535",
        b"99999",
        b"?",
        b"<",
        b"=",
        b">",
        b" ",
        b"!",
        b"\"",
        b"#",
        b"(",
        b"m",
        b"H",
        b"q",
        b"a",
        b"z",
        b"~",
        b"\x07",
        b"\x18",
        b"\x1a",
        b"\x7f",
        b"\x00",
        b"\n",
        b"\r",
        b"\x08",
        b"\x9b",
        b"\x9c",
        b"\x80",
        b"\xc2\x9b",
        b"\xc2\x85",
        b"\xc3\xa9",
        b"\xe6\xbc\xa2",
        b"\xf0\x9f\x91\x8d",
        b"\xe6\xbc",
        b"\xf0\x9f",
        b"\xff",
        b"\xed\xa0\x80",
        b"hello",
        b"https://x.y",
        b"\x1b[38;2;1;2;3m",
        b"\x1b]8;;http://a\x07",
        b"\x1bP1$qm\x1b\\",
        b"\x1b[4:3m",
        b"\x1b[?1049h",
    ];
    let mut out = Vec::new();
    while out.len() < len {
        out.extend_from_slice(pieces[rng.below(pieces.len())]);
    }
    out
}

fn feed_with(mut parser: crate::vt_parser::Parser, bytes: &[u8], splits: &[usize]) -> Vec<Event> {
    let mut ours = Ours::default();
    let mut at = 0;
    for &end in splits.iter().chain(std::iter::once(&bytes.len())) {
        parser.feed(&mut ours, &bytes[at..end]);
        at = end;
    }
    ours.0
}

fn feed_ours(bytes: &[u8], splits: &[usize]) -> Vec<Event> {
    feed_with(crate::vt_parser::Parser::new(), bytes, splits)
}

fn add_events(digest: &mut Digest, events: &[Event]) {
    digest.number(events.len() as u64);
    for event in events {
        match event {
            Event::Print(character) => {
                digest.bytes(&[1]);
                digest.number(*character as u64);
            },
            Event::Execute(byte) => digest.bytes(&[2, *byte]),
            Event::Hook(groups, count, intermediates, overflowed, action)
            | Event::Csi(groups, count, intermediates, overflowed, action) => {
                digest.bytes(&[if matches!(event, Event::Hook(..)) {
                    3
                } else {
                    7
                }]);
                digest.number(groups.len() as u64);
                for group in groups {
                    digest.number(group.len() as u64);
                    group.iter().for_each(|value| digest.number(*value as u64));
                }
                digest.number(*count as u64);
                digest.number(intermediates.len() as u64);
                digest.bytes(intermediates);
                digest.bytes(&[*overflowed as u8]);
                digest.number(*action as u64);
            },
            Event::Put(byte) => digest.bytes(&[4, *byte]),
            Event::Unhook => digest.bytes(&[5]),
            Event::Osc(fields, bell) => {
                digest.bytes(&[6]);
                digest.number(fields.len() as u64);
                for field in fields {
                    digest.number(field.len() as u64);
                    digest.bytes(field);
                }
                digest.bytes(&[*bell as u8]);
            },
            Event::Esc(intermediates, overflowed, byte) => {
                digest.bytes(&[8]);
                digest.number(intermediates.len() as u64);
                digest.bytes(intermediates);
                digest.bytes(&[*overflowed as u8, *byte]);
            },
        }
    }
}

fn digest_of(events: &[Event]) -> String {
    let mut digest = Digest::new();
    add_events(&mut digest, events);
    digest.hex()
}

fn check_self_consistent(bytes: &[u8], rng: &mut Rng, label: &str) -> Vec<Event> {
    let whole = feed_ours(bytes, &[]);
    let mut anywhere = vec![];
    let mut at = 0;
    while at < bytes.len() {
        at += 1 + rng.below(8);
        if at < bytes.len() {
            anywhere.push(at);
        }
    }
    assert_eq!(
        feed_ours(bytes, &anywhere),
        whole,
        "{}: splitting the input changed the result",
        label
    );
    assert_eq!(
        feed_with(
            crate::vt_parser::Parser::without_shortcuts(),
            bytes,
            &anywhere
        ),
        whole,
        "{}: the parser without shortcuts disagrees",
        label
    );
    whole
}

#[test]
fn random_streams_match_their_golden() {
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    let mut lines = vec![];
    let mut digest = Digest::new();
    for case in 0..10000 {
        let len = 1 + rng.below(400);
        let bytes = random_stream(&mut rng, len);
        let mut split = Rng(case as u64 * 7919 + 1);
        let events = check_self_consistent(
            &bytes,
            &mut split,
            &format!("random case {} {:?}", case, bytes),
        );
        add_events(&mut digest, &events);
        if case % 500 == 499 {
            lines.push(format!("cases {}..={} {}", case - 499, case, digest.hex()));
            digest = Digest::new();
        }
    }
    golden::check("vt_parser_random_streams", &lines);
}

#[test]
fn every_single_byte_after_each_state_matches_its_golden() {
    let prefixes: &[&[u8]] = &[
        b"",
        b"\x1b",
        b"\x1b ",
        b"\x1b[",
        b"\x1b[1",
        b"\x1b[1 ",
        b"\x1b[<",
        b"\x1b[1<",
        b"\x1bP",
        b"\x1bP1",
        b"\x1bP ",
        b"\x1bP1<",
        b"\x1bPq",
        b"\x1b]",
        b"\x1b]0;a",
        b"\x1bX",
        b"\xe6",
        b"\xe6\xbc",
        b"\xf0\x9f\x91",
    ];
    let mut rng = Rng(42);
    let mut lines = vec![];
    for prefix in prefixes {
        let mut digest = Digest::new();
        for byte in 0..=255u8 {
            let mut bytes = prefix.to_vec();
            bytes.push(byte);
            bytes.extend_from_slice(b"Z\x1b\\x");
            let events =
                check_self_consistent(&bytes, &mut rng, &format!("{:?} + {:#04x}", prefix, byte));
            add_events(&mut digest, &events);
        }
        lines.push(format!("{} {}", golden::hex(prefix), digest.hex()));
    }
    golden::check("vt_parser_single_bytes", &lines);
}

#[test]
fn limits_match_their_golden() {
    let mut rng = Rng(7);
    let many_params = format!(
        "\x1b[{}m",
        (0..40).map(|i| i.to_string()).collect::<Vec<_>>().join(";")
    );
    let many_sub = format!(
        "\x1b[{}m",
        (0..40).map(|i| i.to_string()).collect::<Vec<_>>().join(":")
    );
    let many_osc = format!(
        "\x1b]{}\x07",
        (0..30).map(|i| i.to_string()).collect::<Vec<_>>().join(";")
    );
    let long_osc = format!("\x1b]52;c;{}\x1b\\", "A".repeat(100_000));
    let mut lines = vec![];
    for (name, text) in [
        ("many parameters", many_params),
        ("many subparameters", many_sub),
        ("many osc fields", many_osc),
        ("long osc", long_osc),
        ("many intermediates", "\x1b[  !p\x1b(((B".to_owned()),
    ] {
        let events = check_self_consistent(text.as_bytes(), &mut rng, &text[..text.len().min(60)]);
        lines.push(format!("{} {}", name, digest_of(&events)));
    }
    golden::check("vt_parser_limits", &lines);
}

#[test]
fn recorded_terminal_sessions_match_their_golden() {
    let mut rng = Rng(99);
    let mut lines = vec![];
    for (name, bytes) in recorded_sessions() {
        let events = check_self_consistent(&bytes, &mut rng, &name);
        lines.push(format!("{} {}", name, digest_of(&events)));
    }
    assert!(lines.len() > 50);
    golden::check("vt_parser_recorded_sessions", &lines);
}

fn recorded_sessions() -> Vec<(String, Vec<u8>)> {
    let mut sessions: Vec<(String, Vec<u8>)> = std::fs::read_dir("../src/tests/fixtures")
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.is_file())
        .map(|path| {
            (
                path.file_name().unwrap().to_string_lossy().into_owned(),
                std::fs::read(&path).unwrap(),
            )
        })
        .collect();
    sessions.sort();
    sessions
}

struct StopOurs(Ours, usize);
impl crate::vt_parser::Handler for StopOurs {
    fn print(&mut self, c: char) {
        crate::vt_parser::Handler::print(&mut self.0, c)
    }
    fn execute(&mut self, b: u8) {
        crate::vt_parser::Handler::execute(&mut self.0, b)
    }
    fn csi_dispatch(&mut self, sequence: &crate::vt_parser::Sequence) {
        crate::vt_parser::Handler::csi_dispatch(&mut self.0, sequence)
    }
    fn osc_end(&mut self, p: &[&[u8]], bell: bool) {
        crate::vt_parser::Handler::osc_end(&mut self.0, p, bell)
    }
    fn esc_dispatch(&mut self, sequence: &crate::vt_parser::Sequence) {
        crate::vt_parser::Handler::esc_dispatch(&mut self.0, sequence)
    }
    fn paused(&self) -> bool {
        self.0
             .0
            .iter()
            .filter(|e| matches!(e, Event::Csi(..) | Event::Osc(..) | Event::Esc(..)))
            .count()
            >= self.1
    }
}
#[test]
fn stopping_early_matches_its_golden() {
    let mut rng = Rng(1234);
    let mut lines = vec![];
    let mut digest = Digest::new();
    for case in 0..5000 {
        let len = 1 + rng.below(300);
        let bytes = random_stream(&mut rng, len);
        let stop_after = rng.below(6);
        let mut ours = StopOurs(Ours::default(), stop_after);
        let ours_used = crate::vt_parser::Parser::new().feed_until_paused(&mut ours, &bytes);
        let mut simple = StopOurs(Ours::default(), stop_after);
        let simple_used =
            crate::vt_parser::Parser::without_shortcuts().feed_until_paused(&mut simple, &bytes);
        assert_eq!(
            (simple_used, &simple.0 .0),
            (ours_used, &ours.0 .0),
            "case {} {:?}",
            case,
            bytes
        );
        digest.number(ours_used as u64);
        add_events(&mut digest, &ours.0 .0);
        if case % 500 == 499 {
            lines.push(format!("cases {}..={} {}", case - 499, case, digest.hex()));
            digest = Digest::new();
        }
    }
    golden::check("vt_parser_stopping_early", &lines);
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
struct Situation {
    state: u8,
    intermediates: usize,
    ignoring: bool,
    values: usize,
    open_group: bool,
    number: u8,
    osc_fields: usize,
    osc_bytes: bool,
}

fn magnitude(number: u16) -> u8 {
    match number {
        0 => 0,
        u16::MAX => 6,
        number => number.ilog10() as u8 + 1,
    }
}

fn situation_of(parser: &super::Parser) -> Option<Situation> {
    use super::State;
    if parser.carry_len != 0 {
        return None;
    }
    let mut situation = Situation {
        state: parser.state as u8,
        ..Situation::default()
    };
    match parser.state {
        State::Escape | State::EscapeIntermediate => {
            situation.intermediates = parser.collected_count;
            situation.ignoring = parser.overflowed;
        },
        State::CsiEntry
        | State::CsiParam
        | State::CsiIntermediate
        | State::DcsEntry
        | State::DcsParam
        | State::DcsIntermediate => {
            situation.intermediates = parser.collected_count;
            situation.ignoring = parser.overflowed;
            situation.values = parser.parameters.value_total;
            situation.open_group = parser.parameters.open_group > 0;
            situation.number = magnitude(parser.number);
        },
        State::OscString => {
            situation.osc_fields = parser
                .osc
                .iter()
                .filter(|&&byte| byte == b';')
                .count()
                .min(super::OSC_FIELD_LIMIT);
            situation.osc_bytes = parser.osc.iter().any(|&byte| byte != b';');
        },
        _ => {},
    }
    Some(situation)
}

#[test]
fn every_byte_in_every_reachable_situation_matches_its_golden() {
    struct Node {
        path: Vec<u8>,
        parser: super::Parser,
        simple: super::Parser,
        events: Vec<Event>,
    }
    let probes: [&[u8]; 2] = [b"m\x07\x1b\\x", b"1;2:3m\x07\x1b\\x"];
    let mut seen = std::collections::HashSet::new();
    let start = super::Parser::new();
    seen.insert(situation_of(&start).unwrap());
    let mut queue = std::collections::VecDeque::from([Node {
        path: vec![],
        parser: start,
        simple: super::Parser::without_shortcuts(),
        events: vec![],
    }]);
    let mut edges = 0;
    let mut lines = vec![];
    while let Some(node) = queue.pop_front() {
        let mut digest = Digest::new();
        for byte in 0..=255u8 {
            for probe in probes {
                let mut input = vec![byte];
                input.extend_from_slice(probe);
                let mut parser = node.parser.clone();
                let mut ours = Ours(node.events.clone());
                parser.feed(&mut ours, &input);
                let mut simple = Ours(node.events.clone());
                node.simple.clone().feed(&mut simple, &input);
                assert_eq!(
                    simple.0, ours.0,
                    "without shortcuts, after {:?} then {:#04x}",
                    node.path, byte
                );
                add_events(&mut digest, &ours.0[node.events.len()..]);
                edges += 1;
            }
            let mut parser = node.parser.clone();
            let mut ours = Ours(node.events.clone());
            parser.feed(&mut ours, &[byte]);
            if let Some(situation) = situation_of(&parser) {
                if seen.insert(situation) {
                    let mut path = node.path.clone();
                    path.push(byte);
                    let mut simple = node.simple.clone();
                    simple.feed(&mut Ours::default(), &[byte]);
                    queue.push_back(Node {
                        path,
                        parser,
                        simple,
                        events: ours.0,
                    });
                }
            }
        }
        lines.push(format!("{} {}", golden::hex(&node.path), digest.hex()));
    }
    assert!(seen.len() > 1000, "only {} situations reached", seen.len());
    assert!(edges > 500_000);
    assert!(seen
        .iter()
        .any(|s| s.values == super::PARAMETER_LIMIT && s.ignoring));
    assert!(seen
        .iter()
        .any(|s| s.values == super::PARAMETER_LIMIT && s.open_group));
    assert!(seen.iter().any(|s| s.number == 6 && s.values == 0));
    assert!(seen
        .iter()
        .any(|s| s.intermediates == super::COLLECT_LIMIT && s.ignoring));
    assert!(seen.iter().any(|s| s.osc_fields == super::OSC_FIELD_LIMIT));
    golden::check("vt_parser_situations", &lines);
}

fn utf8_sequences() -> Vec<Vec<u8>> {
    let mut sequences = vec![];
    for lead in 0x80..=0xffu8 {
        for second in 0..=255u8 {
            sequences.push(vec![lead, second]);
        }
    }
    let second_range = |lead: u8| match lead {
        0xe0 => (0xa0, 0xbf),
        0xed => (0x80, 0x9f),
        0xf0 => (0x90, 0xbf),
        0xf4 => (0x80, 0x8f),
        _ => (0x80, 0xbf),
    };
    let edges_of = |(low, high): (u8, u8)| [low, low + 1, low + (high - low) / 2, high - 1, high];
    for lead in 0xe0..=0xf4u8 {
        for second in edges_of(second_range(lead)) {
            for third in 0..=255u8 {
                sequences.push(vec![lead, second, third]);
            }
        }
    }
    for lead in 0xf0..=0xf4u8 {
        for second in edges_of(second_range(lead)) {
            for third in edges_of((0x80, 0xbf)) {
                for fourth in 0..=255u8 {
                    sequences.push(vec![lead, second, third, fourth]);
                }
            }
        }
    }
    sequences
}

#[test]
fn every_utf8_lead_and_continuation_matches_its_golden_whole_and_split() {
    let mut digests: std::collections::BTreeMap<u8, Digest> = Default::default();
    for sequence in utf8_sequences() {
        for after in [&b"b"[..], &b"\x1b[1mb"[..]] {
            let mut bytes = b"a".to_vec();
            bytes.extend_from_slice(&sequence);
            bytes.extend_from_slice(after);
            let whole = feed_ours(&bytes, &[]);
            assert_eq!(
                feed_with(crate::vt_parser::Parser::without_shortcuts(), &bytes, &[]),
                whole,
                "{:x?} without shortcuts",
                bytes
            );
            add_events(
                digests.entry(sequence[0]).or_insert_with(Digest::new),
                &whole,
            );
            for split in 1..bytes.len() {
                assert_eq!(
                    feed_ours(&bytes, &[split]),
                    whole,
                    "{:x?} split at {}",
                    bytes,
                    split
                );
            }
        }
    }
    let lines: Vec<String> = digests
        .iter()
        .map(|(lead, digest)| format!("{:02x} {}", lead, digest.hex()))
        .collect();
    golden::check("vt_parser_utf8", &lines);
}

#[test]
fn every_short_colour_sequence_matches_its_golden() {
    let alphabet = [b'0', b'1', b'9', b';', b':'];
    let mut rng = Rng(5);
    let mut lines = vec![];
    let mut current: Vec<Vec<u8>> = vec![vec![]];
    for length in 0..=6 {
        let mut digest = Digest::new();
        let mut next = vec![];
        for parameters in &current {
            let mut bytes = b"a\x1b[".to_vec();
            bytes.extend_from_slice(parameters);
            bytes.extend_from_slice(b"mb\x1b[");
            bytes.extend_from_slice(parameters);
            bytes.extend_from_slice(b"m");
            let events = check_self_consistent(
                &bytes,
                &mut rng,
                &format!("{:?}", String::from_utf8_lossy(&bytes)),
            );
            add_events(&mut digest, &events);
            for symbol in alphabet {
                let mut longer = parameters.clone();
                longer.push(symbol);
                next.push(longer);
            }
        }
        lines.push(format!("length {} {}", length, digest.hex()));
        current = next;
    }
    for length in 26..=36 {
        for unit in [
            &b"1;"[..],
            b":",
            b";",
            b"9",
            b"38;2;255;255;255;",
            b"65535:",
        ] {
            let parameters: Vec<u8> = unit.iter().copied().cycle().take(length).collect();
            let mut digest = Digest::new();
            for tail in [&b"m"[..], b"mx", b"8m", b";m", b"?m", b" m"] {
                let mut bytes = b"a\x1b[".to_vec();
                bytes.extend_from_slice(&parameters);
                bytes.extend_from_slice(tail);
                let events = check_self_consistent(
                    &bytes,
                    &mut rng,
                    &format!("{:?}", String::from_utf8_lossy(&bytes)),
                );
                add_events(&mut digest, &events);
            }
            lines.push(format!(
                "{} x{} {}",
                golden::hex(unit),
                length,
                digest.hex()
            ));
        }
    }
    golden::check("vt_parser_colour_sequences", &lines);
}

fn expect(bytes: &[u8], expected: &[Event]) {
    let label = String::from_utf8_lossy(bytes).into_owned();
    assert_eq!(feed_ours(bytes, &[]), expected, "{:?}", label);
    for split in 1..bytes.len() {
        assert_eq!(
            feed_ours(bytes, &[split]),
            expected,
            "{:?} split at {}",
            label,
            split
        );
    }
    assert_eq!(
        feed_with(crate::vt_parser::Parser::without_shortcuts(), bytes, &[]),
        expected,
        "{:?} without shortcuts",
        label
    );
}

fn text(characters: &str) -> Vec<Event> {
    characters.chars().map(Event::Print).collect()
}

fn csi(groups: &[&[u16]], intermediates: &[u8], overflowed: bool, action: char) -> Event {
    Event::Csi(
        groups.iter().map(|group| group.to_vec()).collect(),
        groups.iter().map(|group| group.len()).sum(),
        intermediates.to_vec(),
        overflowed,
        action,
    )
}

fn hook(groups: &[&[u16]], intermediates: &[u8], overflowed: bool, action: char) -> Event {
    Event::Hook(
        groups.iter().map(|group| group.to_vec()).collect(),
        groups.iter().map(|group| group.len()).sum(),
        intermediates.to_vec(),
        overflowed,
        action,
    )
}

fn esc(intermediates: &[u8], overflowed: bool, byte: u8) -> Event {
    Event::Esc(intermediates.to_vec(), overflowed, byte)
}

fn osc(fields: &[&[u8]], bell: bool) -> Event {
    Event::Osc(fields.iter().map(|field| field.to_vec()).collect(), bell)
}

const REPLACEMENT: Event = Event::Print(char::REPLACEMENT_CHARACTER);

#[test]
fn a_csi_sequence_without_parameters_reports_a_single_zero() {
    expect(b"\x1b[m", &[csi(&[&[0]], &[], false, 'm')]);
}

#[test]
fn empty_csi_parameters_are_zero() {
    expect(b"\x1b[;5;H", &[csi(&[&[0], &[5], &[0]], &[], false, 'H')]);
}

#[test]
fn colons_group_subparameters_and_semicolons_separate_groups() {
    expect(
        b"\x1b[1;38:2:1:2:3;4m",
        &[csi(&[&[1], &[38, 2, 1, 2, 3], &[4]], &[], false, 'm')],
    );
    expect(b"\x1b[4:3m", &[csi(&[&[4, 3]], &[], false, 'm')]);
}

#[test]
fn parameter_values_saturate_at_65535() {
    expect(
        b"\x1b[99999;65536m",
        &[csi(&[&[65535], &[65535]], &[], false, 'm')],
    );
}

#[test]
fn a_thirty_third_parameter_marks_the_sequence_overflowed() {
    let numbers: Vec<String> = (0..=32).map(|number| number.to_string()).collect();
    let bytes = format!("\x1b[{}m", numbers.join(";"));
    let groups: Vec<[u16; 1]> = (0..32).map(|number| [number]).collect();
    let groups: Vec<&[u16]> = groups.iter().map(|group| &group[..]).collect();
    expect(bytes.as_bytes(), &[csi(&groups, &[], true, 'm')]);
}

#[test]
fn overflowing_inside_a_subparameter_group_keeps_the_group() {
    let numbers: Vec<String> = (0..=32).map(|number| number.to_string()).collect();
    let bytes = format!("\x1b[{}m", numbers.join(":"));
    let values: Vec<u16> = (0..32).collect();
    expect(bytes.as_bytes(), &[csi(&[&values], &[], true, 'm')]);
}

#[test]
fn private_markers_are_reported_as_intermediates() {
    expect(b"\x1b[?1049h", &[csi(&[&[1049]], b"?", false, 'h')]);
    expect(b"\x1b[>4;1m", &[csi(&[&[4], &[1]], b">", false, 'm')]);
}

#[test]
fn a_third_intermediate_byte_marks_the_sequence_overflowed() {
    expect(b"\x1b[  !p", &[csi(&[&[0]], b"  ", true, 'p')]);
    expect(b"\x1b(((B", &[esc(b"((", true, b'B')]);
}

#[test]
fn a_private_marker_after_parameters_ignores_the_sequence() {
    expect(b"\x1b[1?hX", &text("X"));
}

#[test]
fn a_parameter_after_an_intermediate_ignores_the_sequence() {
    expect(b"\x1b[ 1pX", &text("X"));
}

#[test]
fn controls_inside_a_csi_sequence_run_without_ending_it() {
    expect(
        b"\x1b[1\n2H",
        &[Event::Execute(b'\n'), csi(&[&[12]], &[], false, 'H')],
    );
}

#[test]
fn delete_inside_a_csi_sequence_is_ignored() {
    expect(b"\x1b[1\x7f2m", &[csi(&[&[12]], &[], false, 'm')]);
}

#[test]
fn cancel_and_substitute_abort_a_sequence_and_run() {
    expect(b"\x1b[12\x18A", &[Event::Execute(0x18), Event::Print('A')]);
    expect(b"\x1b[12\x1aA", &[Event::Execute(0x1a), Event::Print('A')]);
    expect(b"\x1b(\x18A", &[Event::Execute(0x18), Event::Print('A')]);
}

#[test]
fn escape_restarts_any_sequence() {
    expect(b"\x1b[12\x1b[3m", &[csi(&[&[3]], &[], false, 'm')]);
    expect(b"\x1b\x1b7", &[esc(b"", false, b'7')]);
}

#[test]
fn escape_sequences_report_their_intermediates() {
    expect(b"\x1b7", &[esc(b"", false, b'7')]);
    expect(b"\x1b(B", &[esc(b"(", false, b'B')]);
    expect(b"\x1b\n7", &[Event::Execute(b'\n'), esc(b"", false, b'7')]);
}

#[test]
fn an_osc_string_is_split_into_fields_at_semicolons() {
    expect(b"\x1b]0;title\x07", &[osc(&[b"0", b"title"], true)]);
    expect(b"\x1b]\x07", &[osc(&[b""], true)]);
    expect(
        b"\x1b]2;x\x1b\\",
        &[osc(&[b"2", b"x"], false), esc(b"", false, b'\\')],
    );
}

#[test]
fn an_osc_string_keeps_at_most_sixteen_fields() {
    let numbers: Vec<String> = (0..20).map(|number| number.to_string()).collect();
    let bytes = format!("\x1b]{}\x07", numbers.join(";"));
    let fields: Vec<&[u8]> = numbers[..16].iter().map(|field| field.as_bytes()).collect();
    expect(bytes.as_bytes(), &[osc(&fields, true)]);
}

#[test]
fn a_payload_with_explosive_field_count_passes_a_bounded_prefix() {
    let payload = vec![b';'; 100_000];
    let seen = super::with_osc_fields(&payload, |fields| fields.len());
    assert_eq!(seen, super::MAX_OSC_FIELDS);
}

#[test]
fn oversized_osc_payload_is_dropped_and_parser_recovers() {
    let mut bytes = b"\x1b]0;".to_vec();
    bytes.extend(std::iter::repeat(b'x').take(super::MAX_OSC_PAYLOAD_BYTES + 1));
    bytes.push(0x07);
    bytes.extend_from_slice(b"ok");

    let mut parser = crate::vt_parser::Parser::new();
    let mut ours = Ours::default();
    for chunk in bytes.chunks(4096) {
        parser.feed(&mut ours, chunk);
    }
    assert_eq!(ours.0, text("ok"));
    assert!(parser.osc_capacity() <= super::RETAINED_OSC_CAPACITY);
}

thread_local! {
    static WARNINGS: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) };
}

struct WarningCapture;

impl log::Log for WarningCapture {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= log::Level::Warn
    }
    fn log(&self, record: &log::Record) {
        if self.enabled(record.metadata()) {
            WARNINGS.with(|warnings| warnings.borrow_mut().push(record.args().to_string()));
        }
    }
    fn flush(&self) {}
}

#[test]
fn an_oversized_osc_payload_logs_one_warning_without_the_payload() {
    static CAPTURE: WarningCapture = WarningCapture;
    let _ = log::set_logger(&CAPTURE);
    log::set_max_level(log::LevelFilter::Warn);
    WARNINGS.with(|warnings| warnings.borrow_mut().clear());

    let mut bytes = b"\x1b]52;c;".to_vec();
    bytes.extend(std::iter::repeat(b'Q').take(super::MAX_OSC_PAYLOAD_BYTES + 1));
    bytes.push(0x07);

    let mut parser = crate::vt_parser::Parser::new();
    let mut ours = Ours::default();
    for chunk in bytes.chunks(4096) {
        parser.feed(&mut ours, chunk);
    }
    assert!(ours.0.is_empty());
    let warnings = WARNINGS.with(|warnings| warnings.borrow().clone());
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(warnings[0].contains("OSC 52"), "{}", warnings[0]);
    assert!(
        warnings[0].contains(&super::MAX_OSC_PAYLOAD_BYTES.to_string()),
        "{}",
        warnings[0]
    );
    assert!(!warnings[0].contains("QQQ"), "{}", warnings[0]);
}

#[test]
fn a_large_osc_payload_within_the_limit_is_dispatched_and_released() {
    // e.g. an OSC 52 clipboard copy of a few hundred KiB of text
    let payload = vec![b'Q'; 1024 * 1024];
    let mut bytes = b"\x1b]52;c;".to_vec();
    bytes.extend_from_slice(&payload);
    bytes.push(0x07);

    let mut parser = crate::vt_parser::Parser::new();
    let mut ours = Ours::default();
    for chunk in bytes.chunks(4096) {
        parser.feed(&mut ours, chunk);
    }
    assert_eq!(
        ours.0,
        vec![osc(
            &[b"52".as_slice(), b"c".as_slice(), payload.as_slice()],
            true
        )]
    );
    assert!(parser.osc_capacity() <= super::RETAINED_OSC_CAPACITY);
}

#[test]
fn an_osc_string_drops_controls_and_keeps_other_bytes() {
    expect(
        b"\x1b]0;a\x01b\xc3\xa9\x7f\x07",
        &[osc(&[b"0", b"ab\xc3\xa9\x7f"], true)],
    );
}

#[test]
fn cancelling_an_osc_string_still_dispatches_it() {
    expect(
        b"\x1b]0;x\x18",
        &[osc(&[b"0", b"x"], false), Event::Execute(0x18)],
    );
}

#[test]
fn discarding_an_unfinished_osc_does_not_let_the_next_bytes_finish_it() {
    let mut parser = crate::vt_parser::Parser::new();
    let mut ours = Ours::default();
    parser.feed(&mut ours, b"\x1b]2;unfinished-title");
    parser.discard_unfinished();
    parser.feed(&mut ours, b"\x07VISIBLE");
    assert!(
        ours.0.iter().all(|event| !matches!(event, Event::Osc(..))),
        "the abandoned title was dispatched: {:?}",
        ours.0
    );
    let printed: String = ours
        .0
        .iter()
        .filter_map(|event| match event {
            Event::Print(character) => Some(*character),
            _ => None,
        })
        .collect();
    assert_eq!(printed, "VISIBLE");
    assert!(ours.0.contains(&Event::Execute(0x07)));
}

#[test]
fn discarding_an_unfinished_dcs_does_not_put_the_following_bytes() {
    let mut parser = crate::vt_parser::Parser::new();
    let mut ours = Ours::default();
    parser.feed(&mut ours, b"\x1bPqpartial");
    parser.discard_unfinished();
    ours.0.clear();
    parser.feed(&mut ours, b"more\x1b\\");
    assert!(
        ours.0.iter().all(|event| !matches!(event, Event::Put(_))),
        "bytes after the gap were still DCS payload: {:?}",
        ours.0
    );
    let printed: String = ours
        .0
        .iter()
        .filter_map(|event| match event {
            Event::Print(character) => Some(*character),
            _ => None,
        })
        .collect();
    assert_eq!(printed, "more");
}

#[test]
fn a_dcs_string_is_hooked_passed_through_and_unhooked() {
    expect(
        b"\x1bP1$qm\x1b\\",
        &[
            hook(&[&[1]], b"$", false, 'q'),
            Event::Put(b'm'),
            Event::Unhook,
            esc(b"", false, b'\\'),
        ],
    );
}

#[test]
fn byte_0x9c_ends_a_dcs_string() {
    expect(
        b"\x1bPqab\x9cX",
        &[
            hook(&[&[0]], b"", false, 'q'),
            Event::Put(b'a'),
            Event::Put(b'b'),
            Event::Unhook,
            Event::Print('X'),
        ],
    );
}

#[test]
fn a_dcs_string_passes_controls_and_drops_delete_and_high_bytes() {
    expect(
        b"\x1bPq\x01\x7f\x80\x1b\\",
        &[
            hook(&[&[0]], b"", false, 'q'),
            Event::Put(0x01),
            Event::Unhook,
            esc(b"", false, b'\\'),
        ],
    );
}

#[test]
fn controls_inside_a_dcs_header_are_ignored() {
    expect(
        b"\x1bP\x011q\x1b\\",
        &[
            hook(&[&[1]], b"", false, 'q'),
            Event::Unhook,
            esc(b"", false, b'\\'),
        ],
    );
}

#[test]
fn sos_pm_and_apc_strings_are_ignored() {
    for introducer in [b'X', b'^', b'_'] {
        let bytes = [&[0x1b, introducer][..], b"abc\x07\x1b\\X"].concat();
        expect(&bytes, &[esc(b"", false, b'\\'), Event::Print('X')]);
    }
}

#[test]
fn plain_controls_run_and_delete_prints() {
    expect(
        b"a\x07b\x7fc",
        &[
            Event::Print('a'),
            Event::Execute(0x07),
            Event::Print('b'),
            Event::Print('\x7f'),
            Event::Print('c'),
        ],
    );
}

#[test]
fn c1_controls_encoded_as_utf8_run_instead_of_printing() {
    expect(
        "a\u{85}b".as_bytes(),
        &[Event::Print('a'), Event::Execute(0x85), Event::Print('b')],
    );
    expect(
        "\u{9b}1m".as_bytes(),
        &[Event::Execute(0x9b), Event::Print('1'), Event::Print('m')],
    );
}

#[test]
fn invalid_utf8_runs_lone_c1_bytes_and_replaces_everything_else() {
    expect(b"\x80", &[Event::Execute(0x80)]);
    expect(b"\xff", &[REPLACEMENT]);
    expect(b"\xe6\xbca", &[REPLACEMENT, Event::Print('a')]);
    expect(b"\xc0\xaf", &[REPLACEMENT, REPLACEMENT]);
    expect(
        b"\xed\xa0\x80",
        &[REPLACEMENT, REPLACEMENT, Event::Execute(0x80)],
    );
}

#[test]
fn an_incomplete_character_before_escape_prints_one_replacement() {
    expect(
        b"\xe6\xbc\x1b[1m",
        &[REPLACEMENT, csi(&[&[1]], &[], false, 'm')],
    );
}

#[test]
fn an_incomplete_character_at_the_end_waits_for_more_input() {
    expect(b"a\xe6\xbc", &text("a"));
    let mut parser = crate::vt_parser::Parser::new();
    let mut ours = Ours::default();
    parser.feed(&mut ours, b"a\xe6\xbc");
    parser.feed(&mut ours, b"\xa2b");
    assert_eq!(ours.0, text("a漢b"));
}

#[test]
fn feeding_until_paused_stops_right_after_the_pausing_sequence() {
    let mut ours = StopOurs(Ours::default(), 1);
    let used = crate::vt_parser::Parser::new().feed_until_paused(&mut ours, b"\x1b[1m\x1b[2mX");
    assert_eq!(used, 4);
    assert_eq!(ours.0 .0, vec![csi(&[&[1]], &[], false, 'm')]);
}
