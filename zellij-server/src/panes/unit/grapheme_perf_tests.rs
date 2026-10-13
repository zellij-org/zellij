use super::super::Grid;
use crate::output::{Output, StructuredClientState};
use crate::panes::cluster_table;
use crate::panes::grid::SixelImageStore;
use crate::panes::kitty_graphics::KittyImageStore;
use crate::panes::link_handler::LinkHandler;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::time::{Duration, Instant};
use zellij_utils::data::{Palette, Style};
use zellij_utils::pane_size::{Size, SizeInPixels};

const ROWS: usize = 50;
const COLS: usize = 200;
const TARGET_BYTES: usize = 50 * 1024 * 1024;

fn new_grid() -> Grid {
    Grid::new(
        ROWS,
        COLS,
        Rc::new(RefCell::new(Palette::default())),
        Rc::new(RefCell::new(HashMap::new())),
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(None)),
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

fn new_output(structured: bool) -> Output {
    let structured_render_clients = Rc::new(RefCell::new(HashMap::new()));
    if structured {
        let mut clients = structured_render_clients.borrow_mut();
        let state = clients
            .entry(1)
            .or_insert_with(StructuredClientState::default);
        state.enabled = true;
        state.reset_viewport(Size {
            rows: ROWS,
            cols: COLS,
        });
    }
    Output::new(
        Rc::new(RefCell::new(SixelImageStore::default())),
        Rc::new(RefCell::new(Some(SizeInPixels {
            height: 20,
            width: 10,
        }))),
        true,
        true,
        Rc::new(RefCell::new(KittyImageStore::default())),
        Rc::new(RefCell::new(HashMap::new())),
        Rc::new(RefCell::new(HashMap::new())),
        Rc::new(RefCell::new(HashMap::new())),
        Rc::new(RefCell::new(HashMap::new())),
        structured_render_clients,
    )
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

fn ascii_block(rng: &mut Rng) -> String {
    let words = [
        "lorem", "ipsum", "dolor", "sit", "amet", "consectetur", "adipiscing", "elit",
    ];
    let mut out = String::new();
    for line in 0..40 {
        out.push_str(&format!("\u{1b}[3{}m", line % 8));
        let mut cols = 0;
        while cols < 150 {
            let word = words[rng.below(words.len())];
            out.push_str(word);
            out.push(' ');
            cols += word.len() + 1;
        }
        out.push_str("\u{1b}[0m\r\n");
    }
    out
}

fn cjk_block(rng: &mut Rng) -> String {
    let mut out = String::new();
    for _ in 0..40 {
        for _ in 0..80 {
            out.push(char::from_u32(0x4E00 + rng.below(0x5000) as u32).unwrap());
        }
        out.push_str("\r\n");
    }
    out
}

fn emoji_block(rng: &mut Rng) -> String {
    let samples = [
        "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}",
        "\u{1F44D}\u{1F3FD}",
        "\u{1F1E9}\u{1F1EA}",
        "\u{1F3F3}\u{FE0F}\u{200D}\u{1F308}",
        "\u{26A0}\u{FE0F}",
        "\u{1F600}",
        "1\u{FE0F}\u{20E3}",
    ];
    let mut out = String::new();
    for _ in 0..40 {
        for _ in 0..25 {
            out.push_str(samples[rng.below(samples.len())]);
            out.push(' ');
        }
        out.push_str("\r\n");
    }
    out
}

fn vietnamese_block(rng: &mut Rng) -> String {
    let words = [
        "Vie\u{0302}\u{0323}t",
        "Nam",
        "ti\u{0301}e\u{0302}\u{0301}ng",
        "nga\u{0300}y",
        "ho\u{0323}c",
        "tru\u{031B}o\u{031B}\u{0300}ng",
    ];
    let mut out = String::new();
    for _ in 0..40 {
        let mut cols = 0;
        while cols < 150 {
            let word = words[rng.below(words.len())];
            out.push_str(word);
            out.push(' ');
            cols += 8;
        }
        out.push_str("\r\n");
    }
    out
}

fn zalgo_block(rng: &mut Rng) -> String {
    let mut out = String::new();
    for _ in 0..40 {
        for _ in 0..150 {
            out.push((b'a' + rng.below(26) as u8) as char);
            for _ in 0..(1 + rng.below(4)) {
                out.push(char::from_u32(0x0300 + rng.below(0x70) as u32).unwrap());
            }
        }
        out.push_str("\r\n");
    }
    out
}

fn feed(grid: &mut Grid, block: &str) -> (Duration, usize) {
    let mut parser = vte::Parser::new();
    let bytes = block.as_bytes();
    let target_bytes = std::env::var("GRAPHEME_PERF_MB")
        .ok()
        .and_then(|mb| mb.parse::<usize>().ok())
        .map(|mb| mb * 1024 * 1024)
        .unwrap_or(TARGET_BYTES);
    let rounds = target_bytes / bytes.len() + 1;
    let mut peak = 0;
    let start = Instant::now();
    for round in 0..rounds {
        parser.advance(grid, bytes);
        if round % 64 == 0 {
            peak = peak.max(cluster_table::bytes());
        }
    }
    let elapsed = start.elapsed();
    (elapsed, peak.max(cluster_table::bytes()))
}

fn serialize_screen(grid: &mut Grid, structured: bool) -> Duration {
    let mut total = Duration::ZERO;
    for _ in 0..200 {
        grid.output_buffer.update_all_lines();
        let (chunks, _, _) = grid.read_changes(0, 0);
        assert_eq!(chunks.len(), ROWS);
        let mut output = new_output(structured);
        let mut clients = HashSet::new();
        clients.insert(1);
        output.add_clients(&clients, Rc::new(RefCell::new(LinkHandler::new())), None);
        let start = Instant::now();
        output
            .add_character_chunks_to_client(1, chunks, None)
            .unwrap();
        let payload = output.serialize().unwrap();
        total += start.elapsed();
        assert!(!payload.is_empty());
    }
    total
}

#[test]
#[ignore]
fn grapheme_cluster_throughput() {
    let cases: [(&str, fn(&mut Rng) -> String); 5] = [
        ("ascii_sgr", ascii_block),
        ("cjk", cjk_block),
        ("emoji", emoji_block),
        ("vietnamese_decomposed", vietnamese_block),
        ("random_marks", zalgo_block),
    ];
    let only = std::env::var("GRAPHEME_PERF_CASE").ok();
    for (name, make) in cases {
        if only.as_deref().map(|case| case != name).unwrap_or(false) {
            continue;
        }
        let mut rng = Rng(0x9E3779B97F4A7C15);
        let block = make(&mut rng);
        let mut grid = new_grid();
        let (feed_time, peak_table_bytes) = feed(&mut grid, &block);
        let ansi_time = serialize_screen(&mut grid, false);
        let frame_time = serialize_screen(&mut grid, true);
        let stats = table_stats_line(&mut grid, peak_table_bytes);
        println!(
            "PERF {name}: feed={:.3}s ansi_serialize_x200={:.3}s frame_serialize_x200={:.3}s {stats}",
            feed_time.as_secs_f64(),
            ansi_time.as_secs_f64(),
            frame_time.as_secs_f64(),
        );
    }
}

fn table_stats_line(grid: &mut Grid, peak_table_bytes: usize) -> String {
    let mut parser = vte::Parser::new();
    parser.advance(grid, b"\x1b[2J\x1b[3Jx");
    let after_clear = cluster_table::bytes();
    assert!(peak_table_bytes <= cluster_table::DEFAULT_BYTE_LIMIT);
    format!(
        "peak_table_bytes={} table_bytes_after_clear={}",
        peak_table_bytes, after_clear
    )
}
