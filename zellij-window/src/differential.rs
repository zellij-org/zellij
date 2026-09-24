use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use anyhow::{Context, Result};

use crate::fixture;
use crate::projection::{compare, of_window, Divergence, Projection};
use crate::reference::{Emitter, Reference, CELL_HEIGHT, CELL_WIDTH};
use crate::vte_terminal::VteTerminal;

pub const STREAM_ROWS: usize = 40;
pub const STREAM_COLS: usize = 120;
const EMIT_CHUNK: usize = 4096;
const SKIPPED_SUFFIXES: [&str; 4] = [".sh", ".kdl", ".toml", ".json"];

pub fn streams_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("src")
        .join("tests")
        .join("fixtures")
}

pub fn stream_corpus() -> Result<Vec<PathBuf>> {
    let dir = streams_dir();
    let mut streams = Vec::new();
    for entry in fs::read_dir(&dir).with_context(|| format!("reading {}", dir.display()))? {
        let path = entry?.path();
        if !path.is_file() {
            continue;
        }
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_owned();
        if SKIPPED_SUFFIXES.iter().any(|suffix| name.ends_with(suffix)) {
            continue;
        }
        streams.push(path);
    }
    streams.sort();
    Ok(streams)
}

pub fn frames_for(path: &Path) -> Arc<Vec<String>> {
    static CACHE: OnceLock<Mutex<HashMap<PathBuf, Arc<Vec<String>>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));

    if let Some(frames) = cache
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(path)
        .cloned()
    {
        return frames;
    }

    let bytes = fs::read(path).unwrap_or_else(|e| panic!("failed to read {:?}: {:?}", path, e));
    let frames = Arc::new(frames_of(&bytes, STREAM_ROWS, STREAM_COLS, EMIT_CHUNK));
    cache
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(path.to_owned(), Arc::clone(&frames));
    frames
}

pub fn frames_of(bytes: &[u8], rows: usize, cols: usize, chunk: usize) -> Vec<String> {
    let mut emitter = Emitter::new(rows, cols);
    let mut frames = Vec::new();
    for piece in bytes.chunks(chunk.max(1)) {
        emitter.advance(piece);
        if let Some(frame) = emitter.frame() {
            frames.push(frame);
        }
    }
    if let Some(frame) = emitter.frame() {
        frames.push(frame);
    }
    frames
}

pub fn window_of(frames: &[String], rows: usize, cols: usize) -> VteTerminal {
    let mut state = VteTerminal::new(rows, cols);
    for frame in frames {
        state.apply(frame);
    }
    state
}

pub fn reference_of(frames: &[String], rows: usize, cols: usize) -> Reference {
    let mut reference = Reference::new(rows, cols);
    for frame in frames {
        reference.advance(frame.as_bytes());
    }
    reference
}

pub fn window_projection_for(path: &Path) -> Arc<Projection> {
    static CACHE: OnceLock<Mutex<HashMap<PathBuf, Arc<Projection>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));

    if let Some(projection) = cache
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(path)
        .cloned()
    {
        return projection;
    }

    let frames = frames_for(path);
    let projection = Arc::new(of_window(&window_of(&frames, STREAM_ROWS, STREAM_COLS)));
    cache
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(path.to_owned(), Arc::clone(&projection));
    projection
}

pub fn reference_projection_for(path: &Path) -> Arc<Projection> {
    static CACHE: OnceLock<Mutex<HashMap<PathBuf, Arc<Projection>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));

    if let Some(projection) = cache
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(path)
        .cloned()
    {
        return projection;
    }

    let frames = frames_for(path);
    let projection = Arc::new(reference_of(&frames, STREAM_ROWS, STREAM_COLS).project());
    cache
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(path.to_owned(), Arc::clone(&projection));
    projection
}

pub fn report(what: &str, divergences: &[Divergence]) -> String {
    let mut lines = vec![format!("{}: {} divergences", what, divergences.len())];
    for divergence in divergences.iter().take(12) {
        lines.push(divergence.describe());
    }
    if divergences.len() > 12 {
        lines.push(format!("... and {} more", divergences.len() - 12));
    }
    lines.join("\n")
}

pub fn disagreement(reference: &Projection, state: &VteTerminal) -> Vec<Divergence> {
    compare(reference, &of_window(state))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::goldens::{self, fixtures_dir};

    fn recorded_frames(path: &Path) -> Result<(usize, usize, Vec<Vec<u8>>)> {
        let fixture = fixture::read(path)?;
        let mut frames = Vec::new();
        for message in &fixture.messages {
            match &message.msg {
                zellij_utils::ipc::ServerToClientMsg::RenderFrame { frame } => {
                    frames.push(frame.clone())
                },
                zellij_utils::ipc::ServerToClientMsg::Exit { .. } => break,
                _ => {},
            }
        }
        Ok((fixture.header.rows, fixture.header.cols, frames))
    }

    fn recorded_state(path: &Path) -> crate::terminal::TerminalState {
        let (rows, cols, frames) = recorded_frames(path).expect("a readable fixture");
        crate::equivalence::state_of(&frames, rows, cols)
    }

    #[test]
    fn the_stream_corpus_is_not_empty() {
        let corpus = stream_corpus().expect("a readable stream corpus");
        assert!(
            corpus.len() > 50,
            "expected the in-tree application streams, found {}",
            corpus.len()
        );
        assert!(corpus.iter().any(|path| path
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("vttest")));
    }

    #[test]
    fn every_application_stream_agrees_with_the_reference() {
        let mut failures = Vec::new();
        for path in stream_corpus().expect("a readable stream corpus") {
            let divergences = compare(
                &reference_projection_for(&path),
                &window_projection_for(&path),
            );
            if !divergences.is_empty() {
                failures.push(report(
                    path.file_name().unwrap().to_str().unwrap(),
                    &divergences,
                ));
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n\n"));
    }

    #[test]
    fn every_application_stream_round_trips_through_the_wire() {
        let mut failures = Vec::new();
        for path in stream_corpus().expect("a readable stream corpus") {
            let bytes = fs::read(&path).expect("a readable stream");
            let mut emitter = Emitter::new(STREAM_ROWS, STREAM_COLS);
            let mut frames = Vec::new();
            for piece in bytes.chunks(EMIT_CHUNK) {
                emitter.advance(piece);
                if let Some(frame) = emitter.frame() {
                    frames.push(frame);
                }
            }
            if let Some(frame) = emitter.frame() {
                frames.push(frame);
            }
            let divergences = compare(&emitter.project(), &reference_projection_for(&path));
            if !divergences.is_empty() {
                failures.push(report(
                    path.file_name().unwrap().to_str().unwrap(),
                    &divergences,
                ));
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n\n"));
    }

    #[test]
    fn every_recorded_fixture_applies_frame_by_frame() {
        let mut failures = Vec::new();
        for path in goldens::corpus().expect("a readable fixture corpus") {
            let (rows, cols, frames) = recorded_frames(&path).expect("a readable fixture");
            let name = path.file_name().unwrap().to_str().unwrap().to_owned();
            if frames.is_empty() {
                failures.push(format!("{} carries no render frames", name));
                continue;
            }
            let mut state = crate::terminal::TerminalState::new(rows, cols);
            state.set_cell_size(CELL_WIDTH as u32, CELL_HEIGHT as u32);
            let mut previous: Option<u64> = None;
            for (index, frame) in frames.iter().enumerate() {
                match state.apply_frame(frame) {
                    Ok(applied) => {
                        if let Some(previous) = previous {
                            if applied.seq <= previous {
                                failures.push(format!(
                                    "{} frame {} carries sequence {} after {}",
                                    name, index, applied.seq, previous
                                ));
                            }
                        }
                        previous = Some(applied.seq);
                    },
                    Err(e) => failures.push(format!("{} frame {}: {}", name, index, e)),
                }
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

    #[test]
    fn a_payload_split_at_any_byte_boundary_agrees_with_the_reference() {
        let bytes = fs::read(streams_dir().join("vttest2-2")).expect("a readable stream");
        let frames = frames_of(&bytes, STREAM_ROWS, STREAM_COLS, EMIT_CHUNK);
        let whole = window_of(&frames, STREAM_ROWS, STREAM_COLS);
        let reference = reference_of(&frames, STREAM_ROWS, STREAM_COLS);
        let expected = reference.project();
        assert!(disagreement(&expected, &whole).is_empty());

        for split in [1usize, 2, 3, 7, 64, 511, 4096] {
            let mut state = VteTerminal::new(STREAM_ROWS, STREAM_COLS);
            for frame in &frames {
                let mut rest = frame.as_str();
                while !rest.is_empty() {
                    let mut at = split.min(rest.len());
                    while !rest.is_char_boundary(at) {
                        at += 1;
                    }
                    let (head, tail) = rest.split_at(at);
                    state.apply(head);
                    rest = tail;
                }
            }
            let divergences = disagreement(&expected, &state);
            assert!(
                divergences.is_empty(),
                "{}",
                report(&format!("split at {}", split), &divergences)
            );
        }
    }

    #[test]
    fn an_unreportable_reference_cursor_leaves_the_window_at_the_last_column() {
        let mut checked = 0;
        for path in stream_corpus().expect("a readable stream corpus") {
            let frames = frames_for(&path);
            if reference_projection_for(&path).cursor.position.is_some() {
                continue;
            }
            checked += 1;
            let state = window_of(&frames, STREAM_ROWS, STREAM_COLS);
            let cursor = of_window(&state).cursor;
            assert_eq!(
                cursor.position.map(|(col, _)| col),
                Some(STREAM_COLS - 1),
                "{} left the window cursor at {:?}",
                path.display(),
                cursor.position
            );
            assert_eq!(cursor.visible, Some(false), "{}", path.display());
        }
        assert!(checked > 0, "no stream exercised the unreportable cursor");
    }

    fn census(projections: impl Iterator<Item = Projection>, seen: &mut BTreeSet<String>) {
        for projection in projections {
            if projection.cursor.visible == Some(false) {
                seen.insert("cursor hidden".to_owned());
            }
            if !matches!(projection.cursor.position, Some((0, 0)) | None) {
                seen.insert("cursor placed".to_owned());
            }
            for row in &projection.cells {
                for cell in row {
                    for attr in &cell.attrs {
                        seen.insert(format!("attr {}", attr));
                    }
                    for (axis, color) in [("fg", &cell.fg), ("bg", &cell.bg)] {
                        if color == crate::projection::DEFAULT_COLOR {
                            continue;
                        }
                        let kind = if color.starts_with('#') {
                            "rgb"
                        } else if color.starts_with("idx") {
                            "indexed"
                        } else {
                            "named"
                        };
                        seen.insert(format!("{} {}", axis, kind));
                    }
                    if cell.underline.is_some() {
                        seen.insert("underline color".to_owned());
                    }
                    if cell.link.is_some() {
                        seen.insert("link".to_owned());
                    }
                    if cell.occupancy != crate::projection::Occupancy::Single {
                        seen.insert(format!("occupancy {:?}", cell.occupancy));
                    }
                    if cell.text.chars().count() > 1 {
                        seen.insert("combining mark".to_owned());
                    }
                }
            }
        }
    }

    fn axes_exercised_by_every_lane() -> BTreeSet<String> {
        let mut seen = BTreeSet::new();
        census(
            stream_corpus()
                .unwrap()
                .into_iter()
                .map(|path| (*window_projection_for(&path)).clone()),
            &mut seen,
        );
        census(
            goldens::corpus().unwrap().into_iter().map(|path| {
                crate::equivalence::project_screen_buffer(recorded_state(&path).screen())
            }),
            &mut seen,
        );
        census(
            crate::adversarial::every_stream(STREAM_ROWS, STREAM_COLS)
                .into_iter()
                .map(|(_, stream)| {
                    let mut state = VteTerminal::new(STREAM_ROWS, STREAM_COLS);
                    state.apply(&stream);
                    of_window(&state)
                }),
            &mut seen,
        );
        seen
    }

    const NARROWER_GEOMETRIES: [(usize, usize); 3] = [(24, 80), (30, 60), (12, 40)];

    #[test]
    fn every_application_stream_agrees_across_every_model_at_three_narrower_geometries_against_each_other_and_no_golden(
    ) {
        let mut failures = Vec::new();
        for (rows, cols) in NARROWER_GEOMETRIES {
            for path in stream_corpus().expect("a readable stream corpus") {
                let bytes = fs::read(&path).expect("a readable stream");
                let name = format!(
                    "{} at {}x{}",
                    path.file_name().unwrap().to_str().unwrap(),
                    rows,
                    cols
                );
                let dialects = crate::equivalence::dialects_of(&bytes, rows, cols, EMIT_CHUNK);
                let grid = reference_of(&dialects.ansi, rows, cols).project();
                let wire = crate::equivalence::project_screen_buffer(
                    &crate::equivalence::buffer_of(&dialects.frames, rows, cols),
                );
                let alacritty = of_window(&window_of(&dialects.ansi, rows, cols));

                let against_the_wire = compare(&grid, &wire);
                if !against_the_wire.is_empty() {
                    failures.push(report(&format!("{} (wire)", name), &against_the_wire));
                }
                let against_alacritty = compare(&grid, &alacritty);
                if !against_alacritty.is_empty() {
                    failures.push(report(&format!("{} (alacritty)", name), &against_alacritty));
                }
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n\n"));
    }

    #[test]
    fn no_stream_wraps_a_line_in_the_window_at_any_geometry() {
        for (rows, cols) in NARROWER_GEOMETRIES {
            for path in stream_corpus().expect("a readable stream corpus") {
                let bytes = fs::read(&path).expect("a readable stream");
                let dialects = crate::equivalence::dialects_of(&bytes, rows, cols, EMIT_CHUNK);
                let wrapped = of_window(&window_of(&dialects.ansi, rows, cols)).wrapped_rows;
                assert!(
                    wrapped.is_empty(),
                    "{} wrapped rows {:?} at {}x{}; the wire positions every chunk absolutely and \
                     must never rely on the client's autowrap, at any size",
                    path.display(),
                    wrapped,
                    rows,
                    cols
                );
            }
        }
    }

    struct Reflow {
        emitter: Emitter,
        ansi: Reference,
        alacritty: VteTerminal,
        buffer: crate::screen_buffer::ScreenBuffer,
    }

    impl Reflow {
        fn new() -> Self {
            Reflow {
                emitter: Emitter::new(STREAM_ROWS, STREAM_COLS),
                ansi: Reference::new(STREAM_ROWS, STREAM_COLS),
                alacritty: VteTerminal::new(STREAM_ROWS, STREAM_COLS),
                buffer: crate::screen_buffer::ScreenBuffer::new(STREAM_ROWS, STREAM_COLS),
            }
        }

        fn drain(&mut self) {
            while let Some((ansi, frame)) = self.emitter.dialects() {
                let (ansi, frame) = match (ansi, frame) {
                    (
                        zellij_server::output::RenderPayload::Ansi(ansi),
                        zellij_server::output::RenderPayload::Frame(frame),
                    ) => (ansi, frame),
                    (ansi, frame) => {
                        panic!("the dialects came out swapped: {:?} {:?}", ansi, frame)
                    },
                };
                self.ansi.advance(ansi.as_bytes());
                self.alacritty.apply(&ansi);
                let view = zellij_utils::structured_render::decode(&frame)
                    .expect("the server emitted a frame its own decoder rejects");
                self.buffer
                    .apply(&view)
                    .expect("the server emitted a frame the screen buffer rejects");
            }
        }

        fn feed(&mut self, bytes: &[u8]) {
            for piece in bytes.chunks(EMIT_CHUNK) {
                self.emitter.advance(piece);
                self.drain();
            }
            self.drain();
        }

        fn resize(&mut self, rows: usize, cols: usize) {
            self.emitter.resize(rows, cols);
            self.ansi.resize(rows, cols);
            self.alacritty.resize(rows, cols);
            self.drain();
        }

        fn divergences(&self) -> Vec<(&'static str, Vec<Divergence>)> {
            let ansi = self.ansi.project();
            let wire = crate::equivalence::project_screen_buffer(&self.buffer);
            let alacritty = of_window(&self.alacritty);
            vec![
                ("wire", compare(&ansi, &wire)),
                ("alacritty", compare(&ansi, &alacritty)),
            ]
        }
    }

    #[test]
    fn every_application_stream_reflowed_to_a_narrower_geometry_carries_the_same_screen_on_both_dialects_the_grids_own_projection_excluded(
    ) {
        let mut failures = Vec::new();
        for (rows, cols) in NARROWER_GEOMETRIES {
            for path in stream_corpus().expect("a readable stream corpus") {
                let bytes = fs::read(&path).expect("a readable stream");
                let name = path.file_name().unwrap().to_str().unwrap().to_owned();
                let arms: &[(&str, usize)] = if (rows, cols) == NARROWER_GEOMETRIES[1] {
                    &[("after the stream", usize::MAX), ("mid stream", 0)]
                } else {
                    &[("after the stream", usize::MAX)]
                };
                for (when, split) in arms.iter().copied() {
                    let resize_after = if split == 0 { bytes.len() / 2 } else { split };
                    let mut reflow = Reflow::new();
                    let (before, after) = bytes.split_at(resize_after.min(bytes.len()));
                    reflow.feed(before);
                    reflow.resize(rows, cols);
                    reflow.feed(after);
                    for (model, divergences) in reflow.divergences() {
                        if !divergences.is_empty() {
                            failures.push(report(
                                &format!(
                                    "{} resized to {}x{} {} ({})",
                                    name, rows, cols, when, model
                                ),
                                &divergences,
                            ));
                        }
                    }
                }
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n\n"));
    }

    const SCREEN_CLEAR: &[u8] = b"\x1b[2J";

    fn clears_the_screen(bytes: &[u8]) -> bool {
        bytes
            .windows(SCREEN_CLEAR.len())
            .any(|window| window == SCREEN_CLEAR)
    }

    struct Scrolled {
        rows_above_the_viewport: usize,
        grid: Projection,
    }

    fn page_up_the_grid(emitter: &mut Emitter, rows: usize) -> Vec<Scrolled> {
        let mut pages = Vec::new();
        let mut rows_above_the_viewport = 0;
        loop {
            let stepped = emitter.scroll_up(rows);
            if stepped == 0 {
                return pages;
            }
            rows_above_the_viewport += stepped;
            pages.push(Scrolled {
                rows_above_the_viewport,
                grid: emitter.project_scrolled(),
            });
        }
    }

    #[test]
    fn every_application_stream_without_a_screen_clear_agrees_with_alacritty_on_its_whole_scrollback_page_by_page_blink_and_cursor_excluded(
    ) {
        let mut failures = Vec::new();
        let mut compared = 0;
        for path in stream_corpus().expect("a readable stream corpus") {
            let bytes = fs::read(&path).expect("a readable stream");
            if clears_the_screen(&bytes) {
                continue;
            }
            let name = path.file_name().unwrap().to_str().unwrap().to_owned();
            let mut emitter = Emitter::new(STREAM_ROWS, STREAM_COLS);
            emitter.advance(&bytes);
            let mut alacritty = VteTerminal::new(STREAM_ROWS, STREAM_COLS);
            alacritty.apply_bytes(&bytes);

            let stored = emitter.scrollback_len();
            if stored != alacritty.history_len() {
                failures.push(format!(
                    "{} scrolled {} rows out of the grid and {} out of alacritty, with no screen \
                     clear to explain the difference",
                    name,
                    stored,
                    alacritty.history_len()
                ));
                continue;
            }
            if stored == 0 {
                continue;
            }
            compared += 1;
            for page in page_up_the_grid(&mut emitter, STREAM_ROWS) {
                let divergences = compare(
                    &crate::projection::of_window_scrolled(
                        &alacritty,
                        page.rows_above_the_viewport,
                    ),
                    &page.grid,
                );
                if !divergences.is_empty() {
                    failures.push(report(
                        &format!("{} scrolled up {} rows", name, page.rows_above_the_viewport),
                        &divergences,
                    ));
                    break;
                }
            }
        }
        assert!(
            compared >= 5,
            "only {} streams left a scrollback to compare; the comparison has gone vacuous",
            compared
        );
        assert!(failures.is_empty(), "{}", failures.join("\n\n"));
    }

    #[test]
    fn only_a_screen_clear_makes_the_two_models_disagree_on_how_much_scrolled_out() {
        let mut unexplained = Vec::new();
        for path in stream_corpus().expect("a readable stream corpus") {
            let bytes = fs::read(&path).expect("a readable stream");
            let mut emitter = Emitter::new(STREAM_ROWS, STREAM_COLS);
            emitter.advance(&bytes);
            let mut alacritty = VteTerminal::new(STREAM_ROWS, STREAM_COLS);
            alacritty.apply_bytes(&bytes);
            let grid = emitter.scrollback_len();
            let alacritty = alacritty.history_len();
            let name = path.file_name().unwrap().to_str().unwrap().to_owned();
            match (clears_the_screen(&bytes), grid == alacritty) {
                (true, false) => {
                    if grid < alacritty {
                        unexplained.push(format!(
                            "{} kept {} rows against alacritty's {}; a screen clear can only make \
                             the grid's scrollback the longer of the two",
                            name, grid, alacritty
                        ));
                    }
                },
                (false, true) | (true, true) => {},
                (false, false) => unexplained.push(format!(
                    "{} kept {} rows against alacritty's {} without ever clearing the screen",
                    name, grid, alacritty
                )),
            }
        }
        assert!(unexplained.is_empty(), "{}", unexplained.join("\n"));
    }

    #[test]
    fn every_application_stream_delivers_its_whole_scrollback_through_the_wire_page_by_page() {
        let mut failures = Vec::new();
        let mut compared = 0;
        for path in stream_corpus().expect("a readable stream corpus") {
            let bytes = fs::read(&path).expect("a readable stream");
            let name = path.file_name().unwrap().to_str().unwrap().to_owned();
            let mut emitter = Emitter::new(STREAM_ROWS, STREAM_COLS);
            let mut buffer = crate::screen_buffer::ScreenBuffer::new(STREAM_ROWS, STREAM_COLS);
            for piece in bytes.chunks(EMIT_CHUNK) {
                emitter.advance(piece);
                apply_frames(&mut emitter, &mut buffer);
            }
            apply_frames(&mut emitter, &mut buffer);
            if emitter.scrollback_len() == 0 {
                continue;
            }
            compared += 1;
            let mut rows_above_the_viewport = 0;
            loop {
                let stepped = emitter.scroll_up(STREAM_ROWS);
                if stepped == 0 {
                    break;
                }
                rows_above_the_viewport += stepped;
                apply_frames(&mut emitter, &mut buffer);
                let divergences = compare(
                    &emitter.project(),
                    &crate::equivalence::project_screen_buffer(&buffer),
                );
                if !divergences.is_empty() {
                    failures.push(report(
                        &format!("{} scrolled up {} rows", name, rows_above_the_viewport),
                        &divergences,
                    ));
                    break;
                }
            }
        }
        assert!(
            compared >= 20,
            "only {} streams left a scrollback to deliver; the comparison has gone vacuous",
            compared
        );
        assert!(failures.is_empty(), "{}", failures.join("\n\n"));
    }

    fn apply_frames(emitter: &mut Emitter, buffer: &mut crate::screen_buffer::ScreenBuffer) {
        while let Some((_, frame)) = emitter.dialects() {
            let frame = match frame {
                zellij_server::output::RenderPayload::Frame(frame) => frame,
                other => panic!("the structured dialect came out as {:?}", other),
            };
            let view = zellij_utils::structured_render::decode(&frame)
                .expect("the server emitted a frame its own decoder rejects");
            buffer
                .apply(&view)
                .expect("the server emitted a frame the screen buffer rejects");
        }
    }

    fn evicting_stream(lines: usize) -> Vec<u8> {
        let mut stream = Vec::new();
        for line in 0..lines {
            stream.extend_from_slice(format!("line-{:05}", line).as_bytes());
            if line % 7 == 0 {
                stream.extend_from_slice(&vec![b'w'; STREAM_COLS + 20]);
            }
            stream.extend_from_slice(b"\r\n");
        }
        stream
    }

    #[test]
    fn a_stream_that_overflows_the_scrollback_cap_evicts_to_the_same_rows_as_alacritty() {
        let stream = evicting_stream(10_600);
        let mut emitter = Emitter::new(STREAM_ROWS, STREAM_COLS);
        emitter.advance(&stream);
        let mut alacritty = VteTerminal::new(STREAM_ROWS, STREAM_COLS);
        alacritty.apply_bytes(&stream);

        let stored = emitter.scrollback_len();
        assert_eq!(
            stored,
            zellij_utils::consts::DEFAULT_SCROLL_BUFFER_SIZE,
            "the stream was meant to overflow the scrollback cap and evict"
        );
        assert_eq!(
            stored,
            alacritty.history_len(),
            "the two models evicted a different number of rows"
        );

        for page in page_up_the_grid(&mut emitter, STREAM_ROWS) {
            let divergences = compare(
                &crate::projection::of_window_scrolled(&alacritty, page.rows_above_the_viewport),
                &page.grid,
            );
            assert!(
                divergences.is_empty(),
                "{}",
                report(
                    &format!(
                        "evicted scrollback {} rows up",
                        page.rows_above_the_viewport
                    ),
                    &divergences
                )
            );
        }
    }

    fn style_surface(projections: impl Iterator<Item = Projection>) -> (BTreeSet<String>, usize) {
        let mut attributes = BTreeSet::new();
        let mut colors = BTreeSet::new();
        for projection in projections {
            for row in &projection.cells {
                for cell in row {
                    for attr in &cell.attrs {
                        attributes.insert((*attr).to_owned());
                    }
                    if cell.fg != crate::projection::DEFAULT_COLOR {
                        colors.insert(format!("fg {}", cell.fg));
                    }
                    if cell.bg != crate::projection::DEFAULT_COLOR {
                        colors.insert(format!("bg {}", cell.bg));
                    }
                    if cell.occupancy != crate::projection::Occupancy::Single {
                        attributes.insert(format!("{:?}", cell.occupancy));
                    }
                    if cell.link.is_some() {
                        attributes.insert("link".to_owned());
                    }
                }
            }
        }
        (attributes, colors.len())
    }

    #[test]
    fn the_corpora_exercise_the_styles_they_are_relied_on_to_exercise() {
        let streams = stream_corpus()
            .expect("a readable stream corpus")
            .into_iter()
            .map(|path| (*window_projection_for(&path)).clone());
        let (stream_attrs, stream_colors) = style_surface(streams);

        let fixtures = goldens::corpus()
            .expect("a readable fixture corpus")
            .into_iter()
            .map(|path| crate::equivalence::project_screen_buffer(recorded_state(&path).screen()));
        let (fixture_attrs, fixture_colors) = style_surface(fixtures);

        for expected in ["bold", "inverse", "WideHead", "WideTail"] {
            assert!(
                stream_attrs.contains(expected),
                "the application streams no longer exercise {}: {:?}",
                expected,
                stream_attrs
            );
        }
        assert!(
            stream_colors >= 40,
            "the application streams exercise only {} colors",
            stream_colors
        );

        for expected in [
            "bold",
            "dim",
            "italic",
            "inverse",
            "strike",
            "underline",
            "undercurl",
            "hidden",
        ] {
            assert!(
                fixture_attrs.contains(expected),
                "the recorded fixtures no longer exercise {}: {:?}",
                expected,
                fixture_attrs
            );
        }
        assert!(
            fixture_colors >= 20,
            "the recorded fixtures exercise only {} colors",
            fixture_colors
        );
    }

    #[test]
    fn the_recorded_blink_capture_reaches_the_window_as_blinking_cells() {
        let state = recorded_state(&fixtures_dir().join("blink.jsonl"));
        assert!(state.has_blinking_cells());

        let projection = crate::equivalence::project_screen_buffer(state.screen());
        for (label, slow, fast) in [
            ("SLOW-BLINK", true, false),
            ("FAST-BLINK", false, true),
            ("BOTH", true, true),
            ("CLEARED", false, false),
            ("BLINK-BOLD-RED", true, false),
            ("BLINK-UNDERLINE", false, true),
            ("plain", false, false),
        ] {
            for cell in cells_of(&projection, label) {
                assert_eq!(
                    cell.attrs.contains("blink-slow"),
                    slow,
                    "{:?} in {}",
                    cell.describe(),
                    label
                );
                assert_eq!(
                    cell.attrs.contains("blink-fast"),
                    fast,
                    "{:?} in {}",
                    cell.describe(),
                    label
                );
            }
        }
    }

    fn cells_of<'a>(projection: &'a Projection, label: &str) -> Vec<&'a crate::projection::Cell> {
        for row in &projection.cells {
            let text: String = row.iter().map(|cell| cell.text.as_str()).collect();
            if let Some(at) = text.find(label) {
                let start = text[..at].chars().count();
                return row[start..start + label.chars().count()].iter().collect();
            }
        }
        panic!("{} is not on the screen", label);
    }

    #[test]
    fn the_recorded_signal_capture_reaches_the_window_as_title_bell_and_notification() {
        let path = fixtures_dir().join("osc-signals.jsonl");
        let (rows, cols, frames) = recorded_frames(&path).expect("a readable fixture");
        let mut state = crate::terminal::TerminalState::new(rows, cols);
        state.set_cell_size(CELL_WIDTH as u32, CELL_HEIGHT as u32);

        let mut title = None;
        let mut bells = 0;
        let mut notifications = Vec::new();
        for frame in &frames {
            state.apply_frame(frame).expect("a usable frame");
            if let Some(seen) = state.take_title() {
                title = Some(seen);
            }
            bells += state.take_bells();
            notifications.extend(state.take_notifications());
        }

        assert_eq!(
            title,
            Some(Some("window-capture-osc-signals | osc-signals".to_owned()))
        );
        assert!(bells > 0, "no bell survived the wire");
        assert_eq!(
            notifications
                .iter()
                .map(|notification| notification.body.as_str())
                .collect::<Vec<_>>(),
            vec!["osc9 notification body"],
            "the recording's OSC 99 declares d=0 and never sends its closing chunk, so it is \
             still being assembled, exactly as kitty would leave it"
        );

        let projection = crate::equivalence::project_screen_buffer(state.screen());
        let linked: Vec<&str> = projection
            .cells
            .iter()
            .flatten()
            .filter_map(|cell| cell.link.as_deref())
            .collect();
        assert_eq!(
            linked,
            vec!["https://example.com/one"; 9],
            "the capture carries the driver's OSC 8 link on the nine cells of the word it wraps"
        );
    }

    #[test]
    fn a_hyperlink_round_trips_through_every_model() {
        const URI: &str = "https://example.com/one";
        let stream = format!(
            "\u{1b}]8;id=1;{}\u{1b}\\linked\u{1b}]8;;\u{1b}\\ plain",
            URI
        );

        let dialects = crate::equivalence::dialects_of(
            stream.as_bytes(),
            STREAM_ROWS,
            STREAM_COLS,
            EMIT_CHUNK,
        );
        let grid = reference_of(&dialects.ansi, STREAM_ROWS, STREAM_COLS).project();
        let wire = crate::equivalence::project_screen_buffer(&crate::equivalence::buffer_of(
            &dialects.frames,
            STREAM_ROWS,
            STREAM_COLS,
        ));
        let mut alacritty = VteTerminal::new(STREAM_ROWS, STREAM_COLS);
        for frame in &dialects.ansi {
            alacritty.apply(frame);
        }
        let alacritty = of_window(&alacritty);

        for (name, projection) in [("grid", &grid), ("wire", &wire), ("alacritty", &alacritty)] {
            let linked: Vec<&str> = projection.cells[0]
                .iter()
                .filter_map(|cell| cell.link.as_deref())
                .collect();
            assert_eq!(
                linked,
                vec![URI; 6],
                "{} did not carry the link on exactly the six linked cells",
                name
            );
            let text: String = projection.cells[0]
                .iter()
                .take(12)
                .map(|cell| cell.text.as_str())
                .collect();
            assert_eq!(text, "linked plain", "{} lost the text", name);
        }

        assert!(compare(&grid, &wire).is_empty());
        assert!(compare(&grid, &alacritty).is_empty());
    }

    #[test]
    fn every_attribute_survives_a_round_trip_through_the_wire() {
        let mut failures = Vec::new();

        for (name, stream) in crate::adversarial::application_attribute_streams() {
            let mut emitter = Emitter::new(STREAM_ROWS, STREAM_COLS);
            emitter.advance(stream.as_bytes());
            let frames = emitter.frame().into_iter().collect::<Vec<_>>();
            let reference = reference_of(&frames, STREAM_ROWS, STREAM_COLS);
            let divergences = compare(&emitter.project(), &reference.project());

            if !divergences.is_empty() {
                failures.push(report(&name, &divergences));
            }

            let state = window_of(&frames, STREAM_ROWS, STREAM_COLS);
            let against_window = disagreement(&reference.project(), &state);
            if !against_window.is_empty() {
                failures.push(report(&format!("{} (window)", name), &against_window));
            }
        }

        assert!(failures.is_empty(), "{}", failures.join("\n\n"));
    }

    #[test]
    fn every_axis_the_wire_can_carry_is_exercised_by_some_corpus() {
        let exercised = axes_exercised_by_every_lane();
        let expected = [
            "attr blink-fast",
            "attr blink-slow",
            "attr bold",
            "attr dashed-underline",
            "attr dim",
            "attr dotted-underline",
            "attr double-underline",
            "attr hidden",
            "attr inverse",
            "attr italic",
            "attr strike",
            "attr undercurl",
            "attr underline",
            "bg indexed",
            "bg named",
            "bg rgb",
            "cursor hidden",
            "cursor placed",
            "fg indexed",
            "fg named",
            "fg rgb",
            "link",
            "occupancy WideHead",
            "occupancy WideTail",
            "underline color",
        ];
        for axis in expected {
            assert!(
                exercised.contains(axis),
                "no corpus exercises {}; the comparison of that axis is vacuous",
                axis
            );
        }

        for unreachable in ["combining mark", "occupancy LeadingSpacer"] {
            assert!(
                !exercised.contains(unreachable),
                "{} is now reachable on this wire; it was recorded as impossible and its pinning \
                 test should become a comparison",
                unreachable
            );
        }
    }

    #[test]
    fn no_stream_ever_wraps_a_line_in_the_window() {
        for path in stream_corpus().expect("a readable stream corpus") {
            let wrapped = window_projection_for(&path).wrapped_rows.clone();
            assert!(
                wrapped.is_empty(),
                "{} wrapped rows {:?}; the wire positions every chunk absolutely and must never \
                 rely on the client's autowrap",
                path.display(),
                wrapped
            );
        }
    }

    mod retention {
        use super::*;
        use crate::atlas::GlyphCache;
        use crate::color::Paints;
        use crate::font::{FontStack, DEFAULT_FONT_SIZE};
        use crate::links::{self, LinkRun};
        use crate::retained::{self, Damage, RetainedScene};
        use crate::scene::{self, BlinkPhase, CursorOptions, Scene};
        use crate::screen_buffer::CursorShape;
        use crate::terminal::TerminalState;
        use zellij_utils::structured_render::{
            CursorState, FrameBuilder, GeometryRecord, LinkEntry, LinkRecord, PaneRect, WireCell,
            WireColor, ATTR_BOLD, ATTR_DIM, ATTR_FAST_BLINK, ATTR_HIDDEN, ATTR_ITALIC,
            ATTR_REVERSE, ATTR_SLOW_BLINK, ATTR_STRIKE, CURSOR_BLINKING, CURSOR_SHAPE_BEAM,
            CURSOR_SHAPE_BLOCK, CURSOR_SHAPE_DEFAULT, CURSOR_SHAPE_UNDERLINE, PANE_FOCUSED,
            PANE_FRAMED,
        };

        struct Rng(u64);

        impl Rng {
            fn next(&mut self) -> u64 {
                self.0 ^= self.0 << 13;
                self.0 ^= self.0 >> 7;
                self.0 ^= self.0 << 17;
                self.0
            }

            fn below(&mut self, bound: usize) -> usize {
                (self.next() % bound.max(1) as u64) as usize
            }

            fn chance(&mut self, one_in: usize) -> bool {
                self.below(one_in) == 0
            }

            fn pick<T: Copy>(&mut self, from: &[T]) -> T {
                from[self.below(from.len())]
            }
        }

        struct Overlay {
            phase: BlinkPhase,
            cursor: CursorOptions,
            hover: Option<LinkRun>,
        }

        impl Overlay {
            fn new() -> Self {
                Self {
                    phase: BlinkPhase::On,
                    cursor: CursorOptions::default(),
                    hover: None,
                }
            }

            fn disturb(
                &mut self,
                state: &TerminalState,
                retained: &mut RetainedScene,
                step: usize,
            ) {
                match step % 5 {
                    0 => {},
                    1 => {
                        self.phase = match self.phase {
                            BlinkPhase::On => BlinkPhase::Off,
                            BlinkPhase::Off => BlinkPhase::On,
                        };
                        retained.mark(&retained::blink_damage(state, self.cursor));
                    },
                    2 => self.hover_over(state, retained, first_link(state)),
                    3 => self.hover_over(state, retained, None),
                    _ => {
                        self.cursor = match self.cursor.shape {
                            None => CursorOptions {
                                shape: Some(CursorShape::Beam),
                                blink: Some(true),
                            },
                            Some(_) => CursorOptions::default(),
                        };
                        retained.mark_everything();
                    },
                }
            }

            fn hover_over(
                &mut self,
                _state: &TerminalState,
                retained: &mut RetainedScene,
                hover: Option<LinkRun>,
            ) {
                if hover == self.hover {
                    return;
                }
                let mut rows = retained::hover_rows(self.hover.as_ref());
                rows.extend(retained::hover_rows(hover.as_ref()));
                self.hover = hover;
                retained.mark(&Damage::Rows(rows));
            }

            fn refresh(
                &self,
                state: &TerminalState,
                cache: &mut GlyphCache,
                retained: &mut RetainedScene,
            ) {
                retained.refresh(
                    state,
                    cache,
                    self.phase,
                    &Paints::default(),
                    self.cursor,
                    self.hover.as_ref(),
                    None,
                );
            }

            fn rebuild(&self, state: &TerminalState, cache: &mut GlyphCache) -> Scene {
                scene::build_at(
                    state,
                    cache,
                    self.phase,
                    &Paints::default(),
                    self.cursor,
                    self.hover.as_ref(),
                    None,
                )
            }
        }

        fn first_link(state: &TerminalState) -> Option<LinkRun> {
            let size = state.size();
            for row in 0..size.rows {
                for col in 0..size.cols {
                    if let Some(run) = links::run_at(state.screen(), row, col) {
                        return Some(run);
                    }
                }
            }
            None
        }

        fn cache() -> GlyphCache {
            GlyphCache::new(FontStack::embedded(DEFAULT_FONT_SIZE).expect("no embedded font"))
        }

        fn walk(name: &str, rows: usize, cols: usize, frames: &[Vec<u8>]) -> Vec<String> {
            let mut state = TerminalState::new(rows, cols);
            state.set_cell_size(CELL_WIDTH as u32, CELL_HEIGHT as u32);
            let mut cache = cache();
            let mut retained = RetainedScene::new();
            let mut overlay = Overlay::new();
            let mut failures = Vec::new();

            for (index, frame) in frames.iter().enumerate() {
                match state.apply_frame(frame) {
                    Ok(applied) => retained.mark(&applied.damage),
                    Err(e) => {
                        failures.push(format!("{} frame {}: {}", name, index, e));
                        continue;
                    },
                }
                overlay.disturb(&state, &mut retained, index);
                overlay.refresh(&state, &mut cache, &mut retained);
                let retained_scene = retained.flatten();
                let rebuilt = overlay.rebuild(&state, &mut cache);
                if retained_scene != rebuilt {
                    failures.push(format!(
                        "{} frame {}: {}",
                        name,
                        index,
                        divergence(&retained_scene, &rebuilt)
                    ));
                    break;
                }
            }
            failures
        }

        fn divergence(retained: &Scene, rebuilt: &Scene) -> String {
            if (retained.width, retained.height, retained.clear)
                != (rebuilt.width, rebuilt.height, rebuilt.clear)
            {
                return format!(
                    "{}x{} clear {:?} against {}x{} clear {:?}",
                    retained.width,
                    retained.height,
                    retained.clear,
                    rebuilt.width,
                    rebuilt.height,
                    rebuilt.clear
                );
            }
            if retained.rects != rebuilt.rects {
                return count("rects", retained.rects.len(), rebuilt.rects.len());
            }
            if retained.glyphs != rebuilt.glyphs {
                return count("glyphs", retained.glyphs.len(), rebuilt.glyphs.len());
            }
            if retained.color_glyphs != rebuilt.color_glyphs {
                return count(
                    "color glyphs",
                    retained.color_glyphs.len(),
                    rebuilt.color_glyphs.len(),
                );
            }
            "the images diverge".to_owned()
        }

        fn count(what: &str, retained: usize, rebuilt: usize) -> String {
            format!(
                "the {} diverge, {} retained against {} rebuilt",
                what, retained, rebuilt
            )
        }

        #[test]
        fn every_recorded_fixture_retains_what_a_rebuild_would_produce() {
            let mut failures = Vec::new();
            for path in goldens::corpus().expect("a readable fixture corpus") {
                let (rows, cols, frames) = recorded_frames(&path).expect("a readable fixture");
                let name = path.file_name().unwrap().to_str().unwrap().to_owned();
                failures.extend(walk(&name, rows, cols, &frames));
            }
            assert!(failures.is_empty(), "{}", failures.join("\n"));
        }

        #[test]
        fn every_application_stream_retains_what_a_rebuild_would_produce() {
            let mut failures = Vec::new();
            for path in stream_corpus().expect("a readable stream corpus") {
                let bytes = fs::read(&path).expect("a readable stream");
                let dialects =
                    crate::equivalence::dialects_of(&bytes, STREAM_ROWS, STREAM_COLS, EMIT_CHUNK);
                let name = path.file_name().unwrap().to_str().unwrap().to_owned();
                failures.extend(walk(&name, STREAM_ROWS, STREAM_COLS, &dialects.frames));
            }
            assert!(failures.is_empty(), "{}", failures.join("\n"));
        }

        const GLYPHS: [char; 24] = [
            'a',
            'b',
            'Z',
            '0',
            ' ',
            '=',
            '>',
            '<',
            '-',
            '|',
            '/',
            ':',
            '.',
            '!',
            '?',
            '+',
            '*',
            '#',
            'é',
            'Ω',
            '\u{2500}',
            '\u{1f600}',
            'й',
            '\u{0}',
        ];
        const ATTRS: [u16; 8] = [
            ATTR_BOLD,
            ATTR_ITALIC,
            ATTR_DIM,
            ATTR_REVERSE,
            ATTR_HIDDEN,
            ATTR_STRIKE,
            ATTR_SLOW_BLINK,
            ATTR_FAST_BLINK,
        ];
        const SHAPES: [u8; 4] = [
            CURSOR_SHAPE_BLOCK,
            CURSOR_SHAPE_UNDERLINE,
            CURSOR_SHAPE_BEAM,
            CURSOR_SHAPE_DEFAULT,
        ];

        fn random_cell(rng: &mut Rng) -> WireCell {
            let mut attrs = 0u16;
            for _ in 0..rng.below(3) {
                attrs |= rng.pick(&ATTRS);
            }
            attrs |= (rng.below(6) as u16) << 8;
            WireCell {
                ch: rng.pick(&GLYPHS) as u32,
                fg: random_color(rng),
                bg: random_color(rng),
                underline_color: random_color(rng),
                attrs,
                width: 1,
                link: rng.below(3) as u8,
            }
        }

        fn random_color(rng: &mut Rng) -> u32 {
            match rng.below(4) {
                0 => WireColor::Default.pack(),
                1 => WireColor::Named(rng.below(16) as u8).pack(),
                2 => WireColor::Indexed(rng.below(256) as u8).pack(),
                _ => WireColor::Rgb(
                    rng.below(256) as u8,
                    rng.below(256) as u8,
                    rng.below(256) as u8,
                )
                .pack(),
            }
        }

        fn random_row(rng: &mut Rng, cols: usize) -> (u16, Vec<WireCell>) {
            let x0 = rng.below(cols);
            let len = 1 + rng.below(cols - x0);
            let mut cells = Vec::with_capacity(len);
            while cells.len() < len {
                if cells.len() + 1 < len && rng.chance(6) {
                    let mut head = random_cell(rng);
                    head.ch = '\u{4e2d}' as u32;
                    head.width = 2;
                    let spacer = WireCell {
                        ch: ' ' as u32,
                        width: 1,
                        ..head
                    };
                    cells.push(head);
                    cells.push(spacer);
                    continue;
                }
                cells.push(random_cell(rng));
            }
            (x0 as u16, cells)
        }

        fn random_frame(rng: &mut Rng, seq: u64, rows: &mut usize, cols: &mut usize) -> Vec<u8> {
            let reshape = rng.chance(12);
            if reshape {
                *rows = 2 + rng.below(6);
                *cols = 4 + rng.below(12);
            }
            let full = reshape || rng.chance(6);
            let mut builder = FrameBuilder::new(*cols as u16, *rows as u16, seq);
            builder.set_full_repaint(full);
            if !full && rng.chance(10) {
                builder.set_clear(true);
            }
            for _ in 0..1 + rng.below(4) {
                let row = rng.below(*rows);
                let (x0, cells) = random_row(rng, *cols);
                builder.push_row(row as u16, x0, &cells);
            }
            if rng.chance(5) {
                builder.push_links(&LinkRecord {
                    entries: vec![
                        LinkEntry {
                            id: 1,
                            uri: "https://example.invalid/one".to_owned(),
                        },
                        LinkEntry {
                            id: 2,
                            uri: "https://example.invalid/two".to_owned(),
                        },
                    ],
                });
            }
            if rng.chance(7) {
                builder.push_geometry(&GeometryRecord {
                    panes: vec![PaneRect {
                        x: 0,
                        y: 0,
                        cols: *cols as u16,
                        rows: (*rows as u16).saturating_sub(rng.below(2) as u16),
                        top: 1,
                        bottom: 0,
                        left: 1,
                        right: 0,
                        flags: PANE_FRAMED | PANE_FOCUSED,
                    }],
                });
            }
            let shape = rng.pick(&SHAPES);
            builder.set_cursor(CursorState {
                x: rng.below(*cols) as u16,
                y: rng.below(*rows) as u16,
                shape: if rng.chance(2) {
                    shape | CURSOR_BLINKING
                } else {
                    shape
                },
                visible: !rng.chance(4),
            });
            builder.finish()
        }

        #[test]
        fn random_delta_sequences_retain_what_a_rebuild_would_produce() {
            let mut failures = Vec::new();
            for seed in 1..=24u64 {
                let mut rng = Rng(seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1);
                let (mut rows, mut cols) = (4 + rng.below(6), 8 + rng.below(12));
                let mut frames = vec![{
                    let mut builder = FrameBuilder::new(cols as u16, rows as u16, 0);
                    builder.set_full_repaint(true);
                    builder.finish()
                }];
                for seq in 1..=60u64 {
                    frames.push(random_frame(&mut rng, seq, &mut rows, &mut cols));
                }
                failures.extend(walk(&format!("seed {}", seed), rows, cols, &frames));
            }
            assert!(failures.is_empty(), "{}", failures.join("\n"));
        }
    }
}
