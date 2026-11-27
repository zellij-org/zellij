use super::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scene {
    pub width: u32,
    pub height: u32,
    pub clear: Srgb,
    pub rects: Vec<Rect>,
    pub glyphs: Vec<GlyphQuad>,
    pub color_glyphs: Vec<GlyphQuad>,
    pub images: Vec<ImageQuad>,
    pub resident_images: Vec<ImageKey>,
}

pub fn build(state: &TerminalState, cache: &mut GlyphCache) -> Scene {
    build_at(
        state,
        cache,
        BlinkPhase::On,
        &Paints::default(),
        CursorOptions::default(),
        None,
        None,
    )
}

pub fn build_at(
    state: &TerminalState,
    cache: &mut GlyphCache,
    phase: BlinkPhase,
    paints: &Paints,
    cursor: CursorOptions,
    hovered_link: Option<&LinkRun>,
    preedit: Option<&Preedit>,
) -> Scene {
    let scene = build_once(
        SceneInputs {
            state: state,
            phase: phase,
            paints: paints,
            cursor: cursor,
            hovered_link: hovered_link,
            preedit: preedit,
        },
        cache,
    );
    if !cache.exhausted() {
        return scene;
    }
    cache.flush();
    build_once(
        SceneInputs {
            state: state,
            phase: phase,
            paints: paints,
            cursor: cursor,
            hovered_link: hovered_link,
            preedit: preedit,
        },
        cache,
    )
}

pub(super) fn build_once(inputs: SceneInputs<'_>, cache: &mut GlyphCache) -> Scene {
    let state = inputs.state;
    let context = RowContext::new(inputs, cache);
    let mut scene = Scene {
        width: context.width(),
        height: context.height(),
        clear: context.clear,
        rects: Vec::new(),
        glyphs: Vec::new(),
        color_glyphs: Vec::new(),
        images: images_of(state, context.metrics),
        resident_images: resident_images(state),
    };

    let mut scratch = RowScratch::default();
    let mut row_scene = RowScene::default();
    for row in 0..context.size.rows {
        build_row(&context, cache, row, &mut scratch, &mut row_scene);
        scene.rects.append(&mut row_scene.rects);
        scene.glyphs.append(&mut row_scene.glyphs);
        scene.color_glyphs.append(&mut row_scene.color_glyphs);
    }

    scene
}

pub fn ligating_sequences(state: &TerminalState, cache: &mut GlyphCache) -> Vec<String> {
    use crate::text::font::GlyphId;

    let size = state.size();
    let paints = Paints::default();
    let cursor = state.cursor_is_visible().then_some(state.cursor_position());
    let mut found = Vec::new();

    for row in 0..size.rows {
        let cells: Vec<RowCell> = (0..size.cols)
            .map(|col| {
                let cell = state.screen().cell(row, col);
                let occupancy = state.occupancy(row, col);
                RowCell {
                    cell,
                    occupancy,
                    paint: paint_of(cell, occupancy, &paints, None),
                    explicit_background: false,
                    on_cursor: false,
                    inverted_by_cursor: false,
                    composing: false,
                    held: false,
                    cluster: state.screen().cluster_at(row, col).is_some(),
                    continuation: occupancy != Occupancy::WideTail && cell.is_group_continuation(),
                    complex: false,
                    unit_end: 0,
                }
            })
            .collect();
        let mut cells = cells;
        mark_units(&mut cells);
        let keys: Vec<Option<RunKey>> = (0..size.cols)
            .map(|col| run_key(cache, &cells[col], state.geometry(), row, col, cursor))
            .collect();

        let mut at = 0;
        while let Some((start, end, key)) = next_run(&keys, at) {
            at = end + 1;
            let characters: Vec<char> = cells[start..=end]
                .iter()
                .map(|entry| entry.cell.character())
                .collect();
            let text: String = characters.iter().collect();
            let glyphs = cache.shape(key.font, &text);
            let nominal: Vec<Option<GlyphId>> = characters
                .iter()
                .map(|character| cache.nominal_of(key.style, *character))
                .collect();
            let mut changed = vec![false; characters.len()];
            for glyph in glyphs.iter() {
                let single = glyph.span == 1
                    && glyphs
                        .iter()
                        .filter(|other| other.cell == glyph.cell)
                        .count()
                        == 1;
                if single && nominal.get(glyph.cell) == Some(&Some(glyph.glyph)) {
                    continue;
                }
                for cell in glyph.cell..(glyph.cell + glyph.span).min(changed.len()) {
                    changed[cell] = true;
                }
            }

            let mut from = 0;
            while from < changed.len() {
                if !changed[from] {
                    from += 1;
                    continue;
                }
                let mut to = from;
                while to + 1 < changed.len() && changed[to + 1] {
                    to += 1;
                }
                found.push(characters[from..=to].iter().collect::<String>());
                from = to + 1;
            }
        }
    }

    found.sort();
    found.dedup();
    found
}
