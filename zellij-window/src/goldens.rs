use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

pub fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures")
}

pub fn goldens_dir() -> PathBuf {
    fixtures_dir().join("goldens")
}

pub fn golden_path(fixture: &Path) -> PathBuf {
    goldens_dir().join(format!("{}.dump", stem_of(fixture)))
}

pub fn png_golden_path(fixture: &Path) -> PathBuf {
    goldens_dir().join(format!("{}.png", stem_of(fixture)))
}

pub fn stream_goldens_dir() -> PathBuf {
    goldens_dir().join("stream")
}

pub fn stream_golden_path(stream: &Path, extension: &str) -> PathBuf {
    stream_goldens_dir().join(format!(
        "{}.{}",
        stream
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "unnamed".to_owned()),
        extension
    ))
}

fn stem_of(fixture: &Path) -> String {
    fixture
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_else(|| "unnamed".to_owned())
}

pub fn corpus() -> Result<Vec<PathBuf>> {
    let dir = fixtures_dir();
    let mut fixtures: Vec<PathBuf> = std::fs::read_dir(&dir)
        .with_context(|| format!("failed to read {:?}", dir))?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "jsonl"))
        .collect();
    fixtures.sort();
    Ok(fixtures)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::atlas::GlyphCache;
    use crate::differential;
    use crate::font::{FontStack, DEFAULT_FONT_SIZE};
    use crate::headless::Headless;
    use crate::image_io;
    use crate::image_io::Image;
    use crate::replay;
    use crate::scene;
    use zellij_utils::ipc::ServerToClientMsg;

    const LIGATING_GOLDENS: &[(&str, &[&str])] = &[
        ("blink", &["..."]),
        ("cursor-shapes", &["..."]),
        ("glyph-torture", &["..."]),
        ("kitty-graphics", &["..."]),
        ("kitty-pane-open", &["..."]),
        ("osc-signals", &["..."]),
        ("panes-and-tabs", &["..."]),
        ("scroll-regions", &["..."]),
        ("sixel-graphics", &["..."]),
        ("sixel-pane-open", &["..."]),
        ("smoke", &["..."]),
        ("styling", &["..."]),
        ("wide-chars", &["..."]),
        ("stream/fish_paste_multiline", &["\\\\\\\\"]),
        ("stream/git_diff_scrollup", &["*", "::"]),
        ("stream/grid_copy_wrapped", &["!?"]),
        ("stream/htop", &["|||||", "||||||", "|||||||", "|||||||||||||||||||||||||||||||||||||"]),
        ("stream/htop_right_scrolling", &["//", "|||||||", "|||||||||", "||||||||||", "|||||||||||||||||||||||||||||||||||||"]),
        ("stream/htop_scrolling", &["//", "|||||||||||||||||||||||||||||||||||||", "||||||||||||||||||||||||||||||||||||||||||"]),
        ("stream/ncmpcpp-wide-chars", &["!!", "=================>", "==========>"]),
        ("stream/vim_overwrite", &["!!!"]),
        ("stream/vttest1-0", &["*", "*++++++++++++++++++++++++++++++++++++++++++++++++++++++++++++++++++++++++++++++*", "++++++++++++++++++++++++++++++++++++++++++++++++++++++++++++++++++++++++++++"]),
        ("stream/vttest1-1", &["*", "*++++++++++++++++++++++++++++++++++++++++++++++++++++++++++++++++++++++++++++++++++++++++++++++++++++++++++++++++++++++*", "++++++++++++++++++++++++++++++++++++++++++++++++++++++++++++++++++++++++++++++++++++++++++++++++++++++++++++++++++++++"]),
        ("stream/vttest2-6", &[".."]),
        ("stream/vttest2-7", &[".."]),
        ("stream/vttest2-8", &[".."]),
        ("stream/vttest2-9", &[".."]),
        ("stream/vttest3-0", &["*", "<=>"]),
        ("stream/vttest8-1", &["..."]),
    ];

    fn should_update() -> bool {
        std::env::var_os("UPDATE_GOLDENS").is_some()
    }

    #[test]
    fn the_fixture_corpus_is_recorded_as_render_frames() {
        let mut unusable = Vec::new();
        for path in corpus().unwrap() {
            let fixture = crate::fixture::read(&path).unwrap();
            for message in &fixture.messages {
                match &message.msg {
                    ServerToClientMsg::Render { .. } => {
                        unusable.push(format!("{:?} carries an ANSI render payload", path));
                        break;
                    },
                    ServerToClientMsg::RenderFrame { frame } => {
                        if let Err(e) = zellij_utils::structured_render::decode(frame) {
                            unusable.push(format!(
                                "{:?} carries a frame this build rejects: {}",
                                path, e
                            ));
                            break;
                        }
                    },
                    _ => {},
                }
            }
        }
        assert!(
            unusable.is_empty(),
            "the fixture corpus is not readable by this build; re-record it with \
             fixtures/capture.sh: {}",
            unusable.join(", ")
        );
    }

    fn shaping_cache() -> GlyphCache {
        GlyphCache::new(
            FontStack::build(&crate::font::FontOptions {
                family: None,
                size: DEFAULT_FONT_SIZE,
                system_fonts: false,
                ligatures: true,
            })
            .unwrap(),
        )
    }

    fn ligating_goldens() -> Vec<(String, Vec<String>)> {
        let mut cache = shaping_cache();
        let mut found = Vec::new();

        for fixture in corpus().unwrap() {
            let outcome = replay::replay_file(&fixture).unwrap();
            let sequences = scene::ligating_sequences(&outcome.state, &mut cache);
            if !sequences.is_empty() {
                found.push((stem_of(&fixture), sequences));
            }
        }
        for stream in differential::stream_corpus().unwrap() {
            let state = stream_state(&stream);
            let sequences = scene::ligating_sequences(&state, &mut cache);
            if !sequences.is_empty() {
                found.push((
                    format!("stream/{}", stream.file_name().unwrap().to_string_lossy()),
                    sequences,
                ));
            }
        }
        found
    }

    fn ink_run_across(rendered: &Image, boundary: u32, from: u32, to: u32) -> u32 {
        let mut longest = 0;
        for y in 0..rendered.height {
            let mut run = 0;
            let mut best = 0;
            let mut crosses = false;
            for x in from..to {
                let pixel = rendered.pixel(x, y);
                if pixel[0] as u32 + pixel[1] as u32 + pixel[2] as u32 > 120 {
                    run += 1;
                    if x == boundary || x + 1 == boundary {
                        crosses = true;
                    }
                    if crosses {
                        best = best.max(run);
                    }
                } else {
                    run = 0;
                    crosses = false;
                }
            }
            longest = longest.max(best);
        }
        longest
    }

    fn arrow_rendered(gpu: &mut crate::headless::GpuLease, ligatures: bool) -> Image {
        let mut cache = GlyphCache::new(
            FontStack::build(&crate::font::FontOptions {
                family: None,
                size: DEFAULT_FONT_SIZE,
                system_fonts: false,
                ligatures,
            })
            .unwrap(),
        );
        let state = crate::screen_buffer::painter::Painter::state(1, 4, |painter| {
            painter.text(0, 0, "a->b")
        });
        let built = scene::build(&state, &mut cache);
        gpu.get().render(&built, cache.atlases()).unwrap()
    }

    #[test]
    fn an_arrow_is_continuous_ink_across_its_two_cells_only_when_ligatures_are_on() {
        let Some(mut gpu) = Headless::exclusive() else {
            return;
        };
        let ligated = arrow_rendered(&mut gpu, true);
        let plain = arrow_rendered(&mut gpu, false);

        let ligated_stem = ink_run_across(&ligated, 16, 8, 24);
        let plain_stem = ink_run_across(&plain, 16, 8, 24);
        assert!(
            ligated_stem >= 12,
            "the arrow did not span its two cells: {} px of stem",
            ligated_stem
        );
        assert!(
            plain_stem < ligated_stem,
            "the unshaped hyphen already spanned the cell boundary: {} px",
            plain_stem
        );
    }

    #[test]
    fn with_the_option_off_no_fixture_ever_reaches_the_shaper() {
        let mut cache = GlyphCache::new(
            FontStack::build(&crate::font::FontOptions {
                family: None,
                size: DEFAULT_FONT_SIZE,
                system_fonts: false,
                ligatures: false,
            })
            .unwrap(),
        );
        for fixture in corpus().unwrap() {
            let outcome = replay::replay_file(&fixture).unwrap();
            let _ = scene::build(&outcome.state, &mut cache);
        }
        for stream in differential::stream_corpus().unwrap() {
            let state = stream_state(&stream);
            let _ = scene::build(&state, &mut cache);
        }
        assert_eq!(
            cache.shaped_runs(),
            0,
            "the shaper was consulted with ligatures off"
        );
    }

    #[test]
    #[ignore]
    fn print_the_ligating_goldens_as_a_table() {
        for (name, sequences) in ligating_goldens() {
            let listed: Vec<String> = sequences
                .iter()
                .map(|sequence| format!("{:?}", sequence))
                .collect();
            println!("        (\"{}\", &[{}]),", name, listed.join(", "));
        }
    }

    #[test]
    fn exactly_the_recorded_goldens_carry_a_ligating_sequence() {
        let found = ligating_goldens();
        let printed: Vec<String> = found
            .iter()
            .map(|(name, sequences)| format!("{} {}", name, sequences.join(" ")))
            .collect();
        let expected: Vec<String> = LIGATING_GOLDENS
            .iter()
            .map(|(name, sequences)| format!("{} {}", name, sequences.join(" ")))
            .collect();
        assert_eq!(
            printed, expected,
            "the set of goldens whose text ligates changed; a golden that renders \
             differently without a sequence named here is a defect"
        );
    }

    #[test]
    fn the_corpus_is_not_empty() {
        assert!(
            !corpus().unwrap().is_empty(),
            "no fixtures found in {:?}",
            fixtures_dir()
        );
    }

    #[test]
    fn every_fixture_replays_to_its_golden_dump() {
        let mut failures = Vec::new();

        for fixture in corpus().unwrap() {
            let outcome = replay::replay_file(&fixture)
                .unwrap_or_else(|e| panic!("failed to replay {:?}: {:?}", fixture, e));
            let dumped = outcome.state.dump();
            let golden = golden_path(&fixture);

            if should_update() {
                std::fs::create_dir_all(goldens_dir()).unwrap();
                std::fs::write(&golden, &dumped).unwrap();
                continue;
            }

            let expected = match std::fs::read_to_string(&golden) {
                Ok(expected) => expected,
                Err(_) => {
                    failures.push(format!(
                        "{:?} has no golden at {:?}; regenerate with UPDATE_GOLDENS=1",
                        fixture, golden
                    ));
                    continue;
                },
            };

            if expected != dumped {
                failures.push(format!(
                    "{:?} diverges from {:?} at {}; regenerate with UPDATE_GOLDENS=1",
                    fixture,
                    golden,
                    first_difference(&expected, &dumped)
                ));
            }
        }

        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

    #[test]
    fn a_warm_atlas_renders_the_same_pixels_as_a_cold_one() {
        let Some(mut gpu) = Headless::exclusive() else {
            return;
        };
        let corpus = corpus().unwrap();
        let subject = corpus
            .iter()
            .find(|path| path.file_name().unwrap() == "smoke.jsonl")
            .expect("the corpus carries smoke.jsonl");
        let outcome = replay::replay_file(subject).unwrap();
        let headless = gpu.get();

        let mut cold = GlyphCache::new(FontStack::embedded(DEFAULT_FONT_SIZE).unwrap());
        let built = scene::build(&outcome.state, &mut cold);
        let from_cold = headless.render(&built, cold.atlases()).unwrap();

        let mut warm = GlyphCache::new(FontStack::embedded(DEFAULT_FONT_SIZE).unwrap());
        for fixture in corpus.iter().filter(|path| *path != subject) {
            let other = replay::replay_file(fixture).unwrap();
            let _ = scene::build(&other.state, &mut warm);
        }
        let built = scene::build(&outcome.state, &mut warm);
        let from_warm = headless.render(&built, warm.atlases()).unwrap();

        assert_eq!(
            from_cold.pixels, from_warm.pixels,
            "a glyph must not change appearance because of what else the atlas holds; \
             the PNG goldens are rendered warm and would otherwise be unreproducible alone"
        );
    }

    #[test]
    fn a_flushed_atlas_renders_the_same_pixels_as_a_cold_one() {
        let Some(mut gpu) = Headless::exclusive() else {
            return;
        };
        let corpus = corpus().unwrap();
        let subject = corpus
            .iter()
            .find(|path| path.file_name().unwrap() == "smoke.jsonl")
            .expect("the corpus carries smoke.jsonl");
        let outcome = replay::replay_file(subject).unwrap();
        let headless = gpu.get();

        let mut cold = GlyphCache::new(FontStack::embedded(DEFAULT_FONT_SIZE).unwrap());
        let built = scene::build(&outcome.state, &mut cold);
        let from_cold = headless.render(&built, cold.atlases()).unwrap();

        let mut flushed = GlyphCache::new(FontStack::embedded(DEFAULT_FONT_SIZE).unwrap());
        for fixture in corpus.iter().filter(|path| *path != subject) {
            let other = replay::replay_file(fixture).unwrap();
            let _ = scene::build(&other.state, &mut flushed);
        }
        flushed.flush();
        assert_eq!(flushed.entries(), 0);
        let built = scene::build(&outcome.state, &mut flushed);
        let after_flush = headless.render(&built, flushed.atlases()).unwrap();

        assert_eq!(
            from_cold.pixels, after_flush.pixels,
            "a flushed cache must re-rasterize the screen exactly as a cold one would"
        );
    }

    #[test]
    fn every_fixture_renders_to_its_golden_png() {
        let Some(mut gpu) = Headless::exclusive() else {
            return;
        };
        let mut cache = GlyphCache::new(FontStack::embedded(DEFAULT_FONT_SIZE).unwrap());
        let headless = gpu.get();
        let mut failures = Vec::new();

        for fixture in corpus().unwrap() {
            let outcome = replay::replay_file(&fixture)
                .unwrap_or_else(|e| panic!("failed to replay {:?}: {:?}", fixture, e));
            let built = scene::build(&outcome.state, &mut cache);
            let rendered = headless
                .render(&built, cache.atlases())
                .unwrap_or_else(|e| panic!("failed to render {:?}: {:?}", fixture, e));
            let golden = png_golden_path(&fixture);

            if should_update() {
                image_io::write(&golden, &rendered).unwrap();
                continue;
            }

            let expected = match image_io::read(&golden) {
                Ok(expected) => expected,
                Err(_) => {
                    failures.push(format!(
                        "{:?} has no golden at {:?}; regenerate with UPDATE_GOLDENS=1",
                        fixture, golden
                    ));
                    continue;
                },
            };

            if let Some(difference) = compare(&expected, &rendered) {
                failures.push(format!(
                    "{:?} diverges from {:?} at {}; regenerate with UPDATE_GOLDENS=1",
                    fixture, golden, difference
                ));
            }
        }

        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

    #[test]
    fn every_application_stream_replays_to_its_golden_dump() {
        let mut failures = Vec::new();
        if should_update() {
            std::fs::create_dir_all(stream_goldens_dir()).unwrap();
        }

        for stream in differential::stream_corpus().unwrap() {
            let dumped = stream_state(&stream).dump();
            let golden = stream_golden_path(&stream, "dump");

            if should_update() {
                std::fs::write(&golden, &dumped).unwrap();
                continue;
            }

            match std::fs::read_to_string(&golden) {
                Ok(expected) if expected == dumped => {},
                Ok(expected) => failures.push(format!(
                    "{:?} diverges from {:?} at {}; regenerate with UPDATE_GOLDENS=1",
                    stream,
                    golden,
                    first_difference(&expected, &dumped)
                )),
                Err(_) => failures.push(format!(
                    "{:?} has no golden at {:?}; regenerate with UPDATE_GOLDENS=1",
                    stream, golden
                )),
            }
        }

        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

    #[test]
    fn every_application_stream_renders_to_its_golden_png() {
        let Some(mut gpu) = Headless::exclusive() else {
            return;
        };
        let mut cache = GlyphCache::new(FontStack::embedded(DEFAULT_FONT_SIZE).unwrap());
        let headless = gpu.get();
        let mut failures = Vec::new();
        if should_update() {
            std::fs::create_dir_all(stream_goldens_dir()).unwrap();
        }

        for stream in differential::stream_corpus().unwrap() {
            let state = stream_state(&stream);
            let built = scene::build(&state, &mut cache);
            let rendered = headless
                .render(&built, cache.atlases())
                .unwrap_or_else(|e| panic!("failed to render {:?}: {:?}", stream, e));
            let golden = stream_golden_path(&stream, "png");

            if should_update() {
                image_io::write(&golden, &rendered).unwrap();
                continue;
            }

            match image_io::read(&golden) {
                Ok(expected) => {
                    if let Some(difference) = compare(&expected, &rendered) {
                        failures.push(format!(
                            "{:?} diverges from {:?} at {}; regenerate with UPDATE_GOLDENS=1",
                            stream, golden, difference
                        ));
                    }
                },
                Err(_) => failures.push(format!(
                    "{:?} has no golden at {:?}; regenerate with UPDATE_GOLDENS=1",
                    stream, golden
                )),
            }
        }

        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

    fn stream_state(stream: &Path) -> crate::terminal::TerminalState {
        let bytes = std::fs::read(stream).expect("a readable stream");
        let dialects = crate::equivalence::dialects_of(
            &bytes,
            differential::STREAM_ROWS,
            differential::STREAM_COLS,
            4096,
        );
        crate::equivalence::state_of(
            &dialects.frames,
            differential::STREAM_ROWS,
            differential::STREAM_COLS,
        )
    }

    #[test]
    fn the_sixel_fixture_decodes_to_the_colors_its_driver_emitted() {
        let fixture = fixtures_dir().join("sixel-graphics.jsonl");
        let outcome = replay::replay_file(&fixture).unwrap();
        let chunks: Vec<_> = outcome.state.sixels().chunks().collect();

        let mut widths: Vec<u32> = chunks.iter().map(|chunk| chunk.width).collect();
        widths.sort_unstable();
        assert_eq!(
            widths,
            vec![24, 32, 32, 40],
            "the recorded sixel geometry changed"
        );
        let rgb = chunks
            .iter()
            .find(|chunk| chunk.width == 32 && chunk.height == 36)
            .expect("the rgb image is missing");
        let bands = [
            [255u8, 0, 0, 255],
            [0, 255, 0, 255],
            [0, 0, 255, 255],
            [255, 255, 0, 255],
        ];
        for expected in bands {
            assert!(
                rgb.image
                    .pixels
                    .chunks_exact(4)
                    .any(|pixel| pixel == expected),
                "the rgb image carries no {:?} band",
                expected
            );
        }
    }

    const HIDPI_FIXTURE: &str = "glyph-torture";
    const HIDPI_SCALE: f32 = 2.0;

    #[test]
    fn the_torture_fixture_renders_at_a_doubled_font_size() {
        let Some(mut gpu) = Headless::exclusive() else {
            return;
        };
        let fixture = fixtures_dir().join(format!("{}.jsonl", HIDPI_FIXTURE));
        let golden = goldens_dir().join(format!("{}@{}x.png", HIDPI_FIXTURE, HIDPI_SCALE as u32));

        let mut cache =
            GlyphCache::new(FontStack::embedded(DEFAULT_FONT_SIZE * HIDPI_SCALE).unwrap());
        let outcome = replay::replay_file(&fixture)
            .unwrap_or_else(|e| panic!("failed to replay {:?}: {:?}", fixture, e));
        let built = scene::build(&outcome.state, &mut cache);
        let rendered = gpu.get().render(&built, cache.atlases()).unwrap();

        if should_update() {
            image_io::write(&golden, &rendered).unwrap();
            return;
        }

        let expected = image_io::read(&golden).unwrap_or_else(|_| {
            panic!(
                "{:?} has no golden; regenerate with UPDATE_GOLDENS=1",
                golden
            )
        });
        let single = image_io::read(&png_golden_path(&fixture)).unwrap();
        assert_eq!(
            (rendered.width, rendered.height),
            (single.width * 2, single.height * 2),
            "doubling the font size did not double the frame"
        );
        if let Some(difference) = compare(&expected, &rendered) {
            panic!(
                "{:?} diverges from {:?} at {}; regenerate with UPDATE_GOLDENS=1",
                fixture, golden, difference
            );
        }
    }

    const CHANNEL_TOLERANCE: u8 = 2;

    fn exceeds_tolerance(expected: &[u8], actual: &[u8]) -> bool {
        const BLOCK: usize = 256;
        expected
            .chunks(BLOCK)
            .zip(actual.chunks(BLOCK))
            .any(|(expected, actual)| {
                expected != actual
                    && expected
                        .iter()
                        .zip(actual)
                        .any(|(a, b)| a.abs_diff(*b) > CHANNEL_TOLERANCE)
            })
    }

    fn compare(expected: &Image, actual: &Image) -> Option<String> {
        if expected.width != actual.width || expected.height != actual.height {
            return Some(format!(
                "size: golden {}x{} vs render {}x{}",
                expected.width, expected.height, actual.width, actual.height
            ));
        }

        if !exceeds_tolerance(&expected.pixels, &actual.pixels) {
            return None;
        }

        let mut first_beyond_tolerance = None;
        let mut beyond_tolerance = 0usize;
        let mut within_tolerance = 0usize;
        let mut worst = 0u8;

        for y in 0..expected.height {
            for x in 0..expected.width {
                let (golden, rendered) = (expected.pixel(x, y), actual.pixel(x, y));
                let deviation = golden
                    .iter()
                    .zip(rendered.iter())
                    .map(|(a, b)| a.abs_diff(*b))
                    .max()
                    .unwrap_or(0);
                worst = worst.max(deviation);

                if deviation > CHANNEL_TOLERANCE {
                    beyond_tolerance += 1;
                    first_beyond_tolerance.get_or_insert((x, y, golden, rendered));
                } else if deviation > 0 {
                    within_tolerance += 1;
                }
            }
        }

        let (x, y, golden, rendered) = first_beyond_tolerance?;
        Some(format!(
            "pixel ({}, {}): golden {:?} vs render {:?}; {} pixels exceed the {} ULP tolerance \
             ({} within it), worst channel deviation {}",
            x, y, golden, rendered, beyond_tolerance, CHANNEL_TOLERANCE, within_tolerance, worst
        ))
    }

    #[test]
    fn the_comparison_absorbs_one_ulp_per_blend_a_pixel_takes_but_no_more() {
        let image = |value: u8| Image {
            width: 1,
            height: 1,
            pixels: vec![value, 0, 0, 255],
        };
        assert!(compare(&image(40), &image(41)).is_none());
        assert!(compare(&image(40), &image(42)).is_none());
        assert!(compare(&image(40), &image(43)).is_some());
    }

    #[test]
    fn a_size_change_is_reported_rather_than_compared() {
        let golden = Image {
            width: 1,
            height: 1,
            pixels: vec![0, 0, 0, 255],
        };
        let rendered = Image {
            width: 2,
            height: 1,
            pixels: vec![0, 0, 0, 255, 0, 0, 0, 255],
        };
        assert!(
            compare(&golden, &rendered).is_some_and(|difference| difference.starts_with("size:"))
        );
    }

    fn first_difference(expected: &str, actual: &str) -> String {
        for (index, (expected_line, actual_line)) in
            expected.lines().zip(actual.lines()).enumerate()
        {
            if expected_line != actual_line {
                return format!(
                    "line {}:\n  golden: {}\n  replay: {}",
                    index + 1,
                    expected_line,
                    actual_line
                );
            }
        }
        format!(
            "line count: golden {} vs replay {}",
            expected.lines().count(),
            actual.lines().count()
        )
    }

    mod retained_path {
        use super::*;
        use crate::color::Paints;
        use crate::retained::RetainedScene;
        use crate::scene::{BlinkPhase, CursorOptions};
        use crate::terminal::TerminalState;

        fn frames_of(fixture: &Path) -> (usize, usize, (u32, u32), Vec<Vec<u8>>) {
            let fixture = crate::fixture::read(fixture).expect("a readable fixture");
            let mut frames = Vec::new();
            for message in &fixture.messages {
                match &message.msg {
                    ServerToClientMsg::RenderFrame { frame } => frames.push(frame.clone()),
                    ServerToClientMsg::Exit { .. } => break,
                    _ => {},
                }
            }
            (
                fixture.header.rows,
                fixture.header.cols,
                (
                    fixture.header.cell_width as u32,
                    fixture.header.cell_height as u32,
                ),
                frames,
            )
        }

        fn refreshed(retained: &mut RetainedScene, state: &TerminalState, cache: &mut GlyphCache) {
            retained.refresh(
                state,
                cache,
                BlinkPhase::On,
                &Paints::default(),
                CursorOptions::default(),
                None,
                None,
            );
        }

        fn whole(state: &TerminalState, cache: &mut GlyphCache) -> RetainedScene {
            let mut retained = RetainedScene::new();
            refreshed(&mut retained, state, cache);
            retained
        }

        fn fingerprint(image: &Image) -> u64 {
            let mut hash = 0xcbf2_9ce4_8422_2325u64;
            for byte in &image.pixels {
                hash ^= *byte as u64;
                hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
            }
            hash
        }

        #[test]
        fn every_fixture_renders_to_its_golden_png_through_the_retained_renderer() {
            let Some(mut gpu) = Headless::exclusive() else {
                return;
            };
            let mut cache = GlyphCache::new(FontStack::embedded(DEFAULT_FONT_SIZE).unwrap());
            let headless = gpu.get();
            let mut failures = Vec::new();

            for fixture in corpus().unwrap() {
                let outcome = replay::replay_file(&fixture).unwrap();
                let retained = whole(&outcome.state, &mut cache);
                let rendered = headless
                    .render_retained(&retained, cache.atlases())
                    .unwrap_or_else(|e| panic!("failed to render {:?}: {:?}", fixture, e));
                let golden = png_golden_path(&fixture);
                let expected = image_io::read(&golden)
                    .unwrap_or_else(|e| panic!("{:?} has no golden: {:?}", fixture, e));
                if let Some(difference) = compare(&expected, &rendered) {
                    failures.push(format!(
                        "{:?} diverges from {:?} at {} through the retained renderer",
                        fixture, golden, difference
                    ));
                }
            }

            assert!(failures.is_empty(), "{}", failures.join("\n"));
        }

        #[test]
        fn every_application_stream_renders_to_its_golden_png_through_the_retained_renderer() {
            let Some(mut gpu) = Headless::exclusive() else {
                return;
            };
            let mut cache = GlyphCache::new(FontStack::embedded(DEFAULT_FONT_SIZE).unwrap());
            let headless = gpu.get();
            let mut failures = Vec::new();

            for stream in differential::stream_corpus().unwrap() {
                let state = stream_state(&stream);
                let retained = whole(&state, &mut cache);
                let rendered = headless
                    .render_retained(&retained, cache.atlases())
                    .unwrap_or_else(|e| panic!("failed to render {:?}: {:?}", stream, e));
                let golden = stream_golden_path(&stream, "png");
                let expected = image_io::read(&golden)
                    .unwrap_or_else(|e| panic!("{:?} has no golden: {:?}", stream, e));
                if let Some(difference) = compare(&expected, &rendered) {
                    failures.push(format!(
                        "{:?} diverges from {:?} at {} through the retained renderer",
                        stream, golden, difference
                    ));
                }
            }

            assert!(failures.is_empty(), "{}", failures.join("\n"));
        }

        fn retained_walk(
            headless: &mut Headless,
            rows: usize,
            cols: usize,
            cell: (u32, u32),
            frames: &[Vec<u8>],
        ) -> Vec<u64> {
            let mut cache = GlyphCache::new(FontStack::embedded(DEFAULT_FONT_SIZE).unwrap());
            let mut state = TerminalState::new(rows, cols);
            state.set_cell_size(cell.0, cell.1);
            let mut retained = RetainedScene::new();
            let mut prints = Vec::with_capacity(frames.len());
            for frame in frames {
                let Ok(applied) = state.apply_frame(frame) else {
                    continue;
                };
                retained.mark(&applied.damage);
                refreshed(&mut retained, &state, &mut cache);
                let image = headless
                    .render_retained(&retained, cache.atlases())
                    .unwrap();
                prints.push(fingerprint(&image));
            }
            prints
        }

        fn rebuilt_walk(
            headless: &mut Headless,
            rows: usize,
            cols: usize,
            cell: (u32, u32),
            frames: &[Vec<u8>],
        ) -> Vec<u64> {
            let mut cache = GlyphCache::new(FontStack::embedded(DEFAULT_FONT_SIZE).unwrap());
            let mut state = TerminalState::new(rows, cols);
            state.set_cell_size(cell.0, cell.1);
            let mut prints = Vec::with_capacity(frames.len());
            for frame in frames {
                let Ok(_) = state.apply_frame(frame) else {
                    continue;
                };
                let built = scene::build(&state, &mut cache);
                let image = headless.render(&built, cache.atlases()).unwrap();
                prints.push(fingerprint(&image));
            }
            prints
        }

        #[test]
        fn every_fixture_draws_the_same_pixels_one_frame_at_a_time_as_from_scratch() {
            let Some(mut gpu) = Headless::exclusive() else {
                return;
            };
            let headless = gpu.get();
            let mut failures = Vec::new();

            for fixture in corpus().unwrap() {
                let (rows, cols, cell, frames) = frames_of(&fixture);
                let retained = retained_walk(headless, rows, cols, cell, &frames);
                let rebuilt = rebuilt_walk(headless, rows, cols, cell, &frames);
                for (index, (held, built)) in retained.iter().zip(rebuilt.iter()).enumerate() {
                    if held != built {
                        failures.push(format!(
                            "{:?} frame {} draws different pixels when the scene is retained",
                            fixture, index
                        ));
                    }
                }
            }

            assert!(failures.is_empty(), "{}", failures.join("\n"));
        }

        #[test]
        fn every_application_stream_draws_the_same_pixels_one_frame_at_a_time_as_from_scratch() {
            let Some(mut gpu) = Headless::exclusive() else {
                return;
            };
            let headless = gpu.get();
            let cell = (
                crate::reference::CELL_WIDTH as u32,
                crate::reference::CELL_HEIGHT as u32,
            );
            let mut failures = Vec::new();

            for stream in differential::stream_corpus().unwrap() {
                let bytes = std::fs::read(&stream).expect("a readable stream");
                let frames = crate::equivalence::dialects_of(
                    &bytes,
                    differential::STREAM_ROWS,
                    differential::STREAM_COLS,
                    4096,
                )
                .frames;
                let retained = retained_walk(
                    headless,
                    differential::STREAM_ROWS,
                    differential::STREAM_COLS,
                    cell,
                    &frames,
                );
                let rebuilt = rebuilt_walk(
                    headless,
                    differential::STREAM_ROWS,
                    differential::STREAM_COLS,
                    cell,
                    &frames,
                );
                for (index, (held, built)) in retained.iter().zip(rebuilt.iter()).enumerate() {
                    if held != built {
                        failures.push(format!(
                            "{:?} frame {} draws different pixels when the scene is retained",
                            stream, index
                        ));
                    }
                }
            }

            assert!(failures.is_empty(), "{}", failures.join("\n"));
        }
    }
}
