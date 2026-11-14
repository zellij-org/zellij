use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::{Duration, Instant};

use zellij_server::panes::grid::Grid;
use zellij_server::panes::kitty_graphics::KittyImageStore;
use zellij_server::panes::link_handler::LinkHandler;
use zellij_server::panes::sixel::SixelImageStore;
use zellij_server::vt_parser::Parser;
use zellij_utils::data::{Palette, Style};
use zellij_utils::pane_size::SizeInPixels;

const CHUNK: usize = 64 * 1024;
const RENDER_INTERVAL: Duration = Duration::from_millis(10);
const PRINTABLE: &[u8] =
    b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ  `~!@#$%^&*()_+-=[]{}\\|;:'\",<.>/?";
const TESTS: &[&str] = &[
    "ascii",
    "unicode",
    "csi",
    "unique_unicode",
    "long_escape_codes",
];

const USAGE: &str = "\
Usage: vt_throughput [options] [TEST...]

Feeds data straight into Zellij's terminal parser and screen grid, without a
pty, a window or other threads, and prints how long it took.

The built-in tests send the same kinds of data as kitty's benchmark
(kitten __benchmark__): ascii, unicode, csi, unique_unicode, long_escape_codes.
Default: ascii unicode csi.

Options:
  --repetitions N   how often each test sends its data (default 100)
  --rounds N        time every test N times and report the fastest (default 3)
  --rows N          screen rows (default 48)
  --cols N          screen columns (default 191)
  --scrollback      use the main screen, keeping scrollback (like --with-scrollback)
  --render          build a screen update every 10 ms, as Zellij does while
                    drawing is allowed (like --render)
  --show            after each test, print the screen as the grid holds it
                    after the last data was sent, with its colors; lines that
                    wrapped are printed joined; runs each test once
  --file PATH       time this file instead, with every line feed sent as a
                    carriage return and line feed, as a terminal driver does
                    (repeatable)
";

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, limit: usize) -> usize {
        (self.next() % limit as u64) as usize
    }
    fn bytes_from(&mut self, count: usize, alphabet: &[u8]) -> Vec<u8> {
        (0..count)
            .map(|_| alphabet[self.below(alphabet.len())])
            .collect()
    }
}

fn ascii(rng: &mut Rng) -> Vec<u8> {
    let mut alphabet = PRINTABLE.to_vec();
    alphabet.extend_from_slice(b"\n\t");
    rng.bytes_from(1024 * 2048 + 13, &alphabet)
}

fn unicode(rng: &mut Rng) -> Vec<u8> {
    let mut line = String::new();
    for _ in 0..360 {
        let code = 0x4e00 + rng.below(0x9fff - 0x4e00) as u32;
        line.push(char::from_u32(code).unwrap_or('?'));
        if rng.below(12) == 0 {
            line.push(['：', '，', '。', '；', '「', '」', '？', '！', '、'][rng.below(9)]);
        }
    }
    line.push_str("‘’“”‹›«» 😀😛😇😈😉😍😎😮👍👎 —–§¶†‡©®™ →⇒•·°±−×÷¼½¾");
    line.push_str("…µ¢£€¿¡¨´¸ˆ˜ ÀÁÂÃÄÅÆÇÈÉÊË ÌÍÎÏÐÑÒÓÔÕÖØ ŒŠÙÚÛÜÝŸÞßàáâãäåæçèéêëìí");
    line.push_str("îïðñòóôõöøœš ùúûüýÿþªºαΩ∞ e\u{301}a\u{308}o\u{302}u\u{300}\u{323}n\u{303}");
    line.push_str("\n\t");
    line.repeat(1024).into_bytes()
}

fn csi(rng: &mut Rng) -> Vec<u8> {
    let mut alphabet = PRINTABLE.to_vec();
    alphabet.extend_from_slice(b"\n\t");
    let mut out = Vec::new();
    while out.len() < 1024 * 1024 + 17 {
        let roll = rng.below(100);
        match roll {
            0..=9 => {
                let count = rng.below(72) + 1;
                out.extend(rng.bytes_from(count, &alphabet));
            },
            10..=29 => out.extend_from_slice(b"\x1b[m\x1b[?1h\x1b[H"),
            30..=39 => out.extend_from_slice(b"\x1b[1;2;3;4:3;31m"),
            40..=49 => out.extend_from_slice(b"\x1b[38:5:24;48:2:125:136:147m"),
            50..=59 => out.extend_from_slice(b"\x1b[58;5;44;2m"),
            60..=79 => out.extend_from_slice(b"\x1b[m\x1b[10A\x1b[3E\x1b[2K"),
            _ => out.extend_from_slice(b"\x1b[39m\x1b[10`a\x1b[100b\x1b[?1l"),
        }
    }
    out.extend_from_slice(b"\x1b[m");
    out
}

fn unique_unicode(_rng: &mut Rng) -> Vec<u8> {
    let mut text = String::new();
    for index in 0..256 * 1024u32 {
        let mut value = index;
        text.push('a');
        for _ in 0..3 {
            text.push(char::from_u32(0x300 + value % 0x70).unwrap_or('a'));
            value /= 0x70;
        }
    }
    text.into_bytes()
}

fn long_escape_codes(rng: &mut Rng) -> Vec<u8> {
    let payload = rng.bytes_from(8024, PRINTABLE);
    let mut one = b"\x1b]6;".to_vec();
    one.extend_from_slice(&payload);
    one.push(0x07);
    one.repeat(1024)
}

fn generate(name: &str) -> Option<Vec<u8>> {
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    Some(match name {
        "ascii" => ascii(&mut rng),
        "unicode" => unicode(&mut rng),
        "csi" => csi(&mut rng),
        "unique_unicode" => unique_unicode(&mut rng),
        "long_escape_codes" => long_escape_codes(&mut rng),
        _ => return None,
    })
}

fn with_carriage_returns(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len() + bytes.len() / 32);
    for &byte in bytes {
        if byte == b'\n' {
            out.push(b'\r');
        }
        out.push(byte);
    }
    out
}

struct Settings {
    repetitions: usize,
    rounds: usize,
    rows: usize,
    cols: usize,
    scrollback: bool,
    render: bool,
    show: bool,
}

fn new_grid(settings: &Settings) -> Grid {
    Grid::new(
        settings.rows,
        settings.cols,
        Rc::new(RefCell::new(Palette::default())),
        Rc::new(RefCell::new(HashMap::new())),
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(Some(SizeInPixels {
            width: 8,
            height: 16,
        }))),
        Rc::new(RefCell::new(SixelImageStore::default())),
        Rc::new(RefCell::new(KittyImageStore::default())),
        Style::default(),
        false,
        true,
        true,
        true,
        false,
    )
}

struct Feeder {
    parser: Parser,
    grid: Grid,
    style: Style,
    render: bool,
    last_render: Instant,
    renders: usize,
}

impl Feeder {
    fn feed(&mut self, bytes: &[u8]) {
        for chunk in bytes.chunks(CHUNK) {
            self.parser.feed(&mut self.grid, chunk);
            self.grid.pending_messages_to_pty.clear();
            self.grid.pending_forwarded_queries.clear();
            if self.render && self.last_render.elapsed() >= RENDER_INTERVAL {
                if let Ok(Some(_)) = self.grid.render(0, 0, &self.style) {
                    self.renders += 1;
                }
                self.last_render = Instant::now();
            }
        }
    }
}

fn run_once(data: &[u8], settings: &Settings, kitten_framing: bool) -> (Duration, usize, usize) {
    let mut feeder = Feeder {
        parser: Parser::new(),
        grid: new_grid(settings),
        style: Style::default(),
        render: settings.render,
        last_render: Instant::now(),
        renders: 0,
    };
    let pause = !settings.render && kitten_framing;
    let mut start = Vec::new();
    if !settings.scrollback {
        start.extend_from_slice(b"\x1b[?1049h");
    }
    if pause {
        start.extend_from_slice(b"\x1b[?2026h");
    }
    let mut between = b"\x1b[m\x1b[H\x1b[2JRunning\r\n".to_vec();
    if pause {
        between.extend_from_slice(b"\x1b[?2026l\x1b[?2026h");
    }
    let mut sent = 0;
    let began = Instant::now();
    feeder.feed(&start);
    for repetition in 0..settings.repetitions {
        feeder.feed(data);
        sent += data.len();
        if kitten_framing && repetition + 1 < settings.repetitions {
            feeder.feed(&between);
        }
    }
    feeder.feed(b"\x1b[?2026l\x1b[6n\x1b[5n");
    let elapsed = began.elapsed();
    if settings.show {
        println!("{}", "-".repeat(settings.cols));
        print!("{}", feeder.grid.dump_screen_with_ansi(false));
        println!("\x1b[m");
        println!("{}", "-".repeat(settings.cols));
    }
    (elapsed, sent, feeder.renders)
}

fn report(name: &str, data: &[u8], settings: &Settings, kitten_framing: bool) {
    let mut best: Option<(Duration, usize, usize)> = None;
    let rounds = if settings.show { 1 } else { settings.rounds };
    for _ in 0..rounds {
        let result = run_once(data, settings, kitten_framing);
        if best.map_or(true, |(time, _, _)| result.0 < time) {
            best = Some(result);
        }
    }
    if let Some((time, sent, renders)) = best {
        let seconds = time.as_secs_f64();
        let mut line = format!(
            "{:<20} {:>9.1} ms {:>8.1} MB/s",
            name,
            seconds * 1000.0,
            sent as f64 / 1e6 / seconds
        );
        if settings.render {
            line.push_str(&format!("  {} screen updates", renders));
        }
        println!("{}", line);
    }
}

fn number(value: Option<String>, option: &str) -> usize {
    match value.and_then(|value| value.parse().ok()) {
        Some(number) => number,
        None => {
            eprintln!("{} needs a number\n\n{}", option, USAGE);
            std::process::exit(1);
        },
    }
}

fn main() {
    let mut settings = Settings {
        repetitions: 100,
        rounds: 3,
        rows: 48,
        cols: 191,
        scrollback: false,
        render: false,
        show: false,
    };
    let mut tests = Vec::new();
    let mut files = Vec::new();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repetitions" => settings.repetitions = number(args.next(), &arg),
            "--rounds" => settings.rounds = number(args.next(), &arg).max(1),
            "--rows" => settings.rows = number(args.next(), &arg).max(1),
            "--cols" => settings.cols = number(args.next(), &arg).max(1),
            "--scrollback" => settings.scrollback = true,
            "--render" => settings.render = true,
            "--show" => settings.show = true,
            "--file" => match args.next() {
                Some(path) => files.push(path),
                None => {
                    eprintln!("--file needs a path\n\n{}", USAGE);
                    std::process::exit(1);
                },
            },
            "-h" | "--help" => {
                print!("{}", USAGE);
                return;
            },
            name if TESTS.contains(&name) => tests.push(name.to_owned()),
            other => {
                eprintln!("unknown option or test: {}\n\n{}", other, USAGE);
                std::process::exit(1);
            },
        }
    }
    if tests.is_empty() && files.is_empty() {
        tests = vec!["ascii".into(), "unicode".into(), "csi".into()];
    }
    println!(
        "{} x {}, {} repetitions, best of {} rounds, {}{}",
        settings.rows,
        settings.cols,
        settings.repetitions,
        if settings.show { 1 } else { settings.rounds },
        if settings.scrollback {
            "main screen with scrollback"
        } else {
            "alternate screen"
        },
        if settings.render {
            ", screen updates every 10 ms"
        } else {
            ", drawing paused"
        }
    );
    for name in &tests {
        if let Some(data) = generate(name) {
            report(name, &data, &settings, true);
        }
    }
    for path in &files {
        match std::fs::read(path) {
            Ok(bytes) => {
                let name = std::path::Path::new(path)
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_else(|| path.clone());
                report(&name, &with_carriage_returns(&bytes), &settings, false);
            },
            Err(error) => eprintln!("could not read {}: {}", path, error),
        }
    }
}
