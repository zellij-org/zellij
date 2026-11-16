use super::*;
use crate::text::font::DEFAULT_FONT_SIZE;

fn cache() -> GlyphCache {
    GlyphCache::new(FontStack::embedded(DEFAULT_FONT_SIZE).unwrap())
}

fn bitmap(width: u32, height: u32, fill: u8) -> GlyphBitmap {
    GlyphBitmap {
        left: 0,
        top: 0,
        width,
        height,
        coverage: vec![fill; (width * height) as usize],
        content: GlyphContent::Mask,
    }
}

fn color_bitmap(width: u32, height: u32, fill: u8) -> GlyphBitmap {
    GlyphBitmap {
        left: 0,
        top: 0,
        width,
        height,
        coverage: vec![fill; (width * height * 4) as usize],
        content: GlyphContent::Color,
    }
}

fn read(atlas: &GlyphAtlas, entry: AtlasEntry) -> Vec<u8> {
    let channels = atlas.channels() as usize;
    (0..entry.height)
        .flat_map(|row| {
            let start = ((entry.y + row) * atlas.width() + entry.x) as usize * channels;
            atlas.coverage()[start..start + entry.width as usize * channels].to_vec()
        })
        .collect()
}

fn rendered(lookup: Lookup) -> RenderedGlyph {
    match lookup {
        Lookup::Rendered(rendered) => rendered,
        other => panic!("expected a rendered glyph, got {:?}", other),
    }
}

#[test]
fn inserted_coverage_is_readable_at_the_reported_position() {
    let mut atlas = GlyphAtlas::new(1);
    let entry = atlas.insert(&bitmap(3, 2, 77)).unwrap();
    assert_eq!(read(&atlas, entry), vec![77; 6]);
}

#[test]
fn a_color_atlas_stores_four_bytes_per_pixel() {
    let mut atlas = GlyphAtlas::new(4);
    let entry = atlas.insert(&color_bitmap(2, 2, 9)).unwrap();
    assert_eq!(read(&atlas, entry), vec![9; 16]);
}

#[test]
fn an_atlas_refuses_glyphs_of_the_wrong_channel_count() {
    assert!(GlyphAtlas::new(4).insert(&bitmap(2, 2, 1)).is_err());
    assert!(GlyphAtlas::new(1).insert(&color_bitmap(2, 2, 1)).is_err());
}

#[test]
fn entries_do_not_overlap() {
    let mut atlas = GlyphAtlas::new(1);
    let first = atlas.insert(&bitmap(4, 4, 1)).unwrap();
    let second = atlas.insert(&bitmap(4, 4, 2)).unwrap();
    assert_ne!((first.x, first.y), (second.x, second.y));
    assert_eq!(read(&atlas, first), vec![1; 16]);
    assert_eq!(read(&atlas, second), vec![2; 16]);
}

#[test]
fn a_full_shelf_wraps_onto_the_next_one() {
    let mut atlas = GlyphAtlas::new(1);
    let mut previous = atlas.insert(&bitmap(8, 8, 1)).unwrap();
    let mut wrapped = false;
    for _ in 0..(ATLAS_WIDTH / 8) {
        let entry = atlas.insert(&bitmap(8, 8, 1)).unwrap();
        if entry.y > previous.y {
            wrapped = true;
            break;
        }
        previous = entry;
    }
    assert!(wrapped, "the atlas never opened a second shelf");
}

#[test]
fn the_atlas_grows_in_height_without_moving_earlier_entries() {
    let mut atlas = GlyphAtlas::new(1);
    let first = atlas.insert(&bitmap(4, 4, 42)).unwrap();
    for _ in 0..5000 {
        atlas.insert(&bitmap(16, 16, 7)).unwrap();
    }
    assert!(atlas.height() > INITIAL_HEIGHT);
    assert_eq!(read(&atlas, first), vec![42; 16]);
}

#[test]
fn a_glyph_wider_than_the_atlas_is_rejected() {
    let mut atlas = GlyphAtlas::new(1);
    assert!(atlas.insert(&bitmap(ATLAS_WIDTH, 1, 0)).is_err());
}

#[test]
fn the_revision_advances_only_when_the_atlas_changes() {
    let mut cache = cache();
    let before = cache.atlases().mask.revision();
    rendered(cache.glyph(FaceStyle::Regular, 'x', 1));
    let after = cache.atlases().mask.revision();
    assert!(after > before);
    rendered(cache.glyph(FaceStyle::Regular, 'x', 1));
    assert_eq!(cache.atlases().mask.revision(), after);
}

#[test]
fn a_repeated_lookup_returns_the_same_entry() {
    let mut cache = cache();
    let first = rendered(cache.glyph(FaceStyle::Regular, 'W', 1));
    let second = rendered(cache.glyph(FaceStyle::Regular, 'W', 1));
    assert_eq!(first, second);
}

#[test]
fn styles_are_cached_independently() {
    let mut cache = cache();
    let regular = rendered(cache.glyph(FaceStyle::Regular, 'A', 1)).entry;
    let bold = rendered(cache.glyph(FaceStyle::Bold, 'A', 1)).entry;
    assert_ne!((regular.x, regular.y), (bold.x, bold.y));
}

#[test]
fn budgets_are_cached_independently() {
    let mut cache = cache();
    let wide = rendered(cache.glyph(FaceStyle::Regular, '\u{4f60}', 2)).entry;
    let narrow = rendered(cache.glyph(FaceStyle::Regular, '\u{4f60}', 1)).entry;
    assert_ne!((wide.x, wide.y), (narrow.x, narrow.y));
    assert!(narrow.width < wide.width);
}

#[test]
fn block_and_box_characters_are_drawn_to_fill_the_cell() {
    let mut cache = cache();
    let metrics = cache.metrics();
    for character in ['\u{2588}', '\u{2500}', '\u{2502}', '\u{e0b0}'] {
        let sprite = rendered(cache.glyph(FaceStyle::Regular, character, 1));
        assert_eq!(sprite.content, GlyphContent::Mask);
        assert_eq!(
            (sprite.entry.width, sprite.entry.height),
            (metrics.width, metrics.height),
            "{:?}",
            character
        );
        assert_eq!(
            (sprite.entry.left, sprite.entry.top),
            (0, metrics.baseline as i32)
        );
    }
    let block = rendered(cache.glyph(FaceStyle::Bold, '\u{2588}', 1));
    assert!(read(&cache.mask, block.entry)
        .iter()
        .all(|alpha| *alpha == 255));
}

#[test]
fn a_blank_character_is_distinguished_from_a_missing_one() {
    let mut cache = cache();
    assert_eq!(cache.glyph(FaceStyle::Regular, ' ', 1), Lookup::Blank);
    assert_eq!(
        cache.glyph(FaceStyle::Regular, '\u{10fffd}', 1),
        Lookup::Missing
    );
}

#[test]
fn color_glyphs_land_in_the_color_atlas_alone() {
    let mut cache = cache();
    let before = cache.atlases().mask.revision();
    let emoji = rendered(cache.glyph(FaceStyle::Regular, '\u{1f600}', 2));
    assert_eq!(emoji.content, GlyphContent::Color);
    assert_eq!(cache.atlases().mask.revision(), before);
    assert!(cache.atlases().color.revision() > 0);
}

#[test]
fn a_fallback_glyph_is_a_mask_when_the_face_has_no_color() {
    let mut cache = cache();
    let cjk = rendered(cache.glyph(FaceStyle::Regular, '\u{4f60}', 2));
    assert_eq!(cjk.content, GlyphContent::Mask);
}

fn tiny() -> GlyphCache {
    GlyphCache::tiny(FontStack::embedded(DEFAULT_FONT_SIZE).unwrap())
}

fn letters() -> impl Iterator<Item = char> {
    (b'!'..=b'~').map(char::from)
}

fn fill_until_exhausted(cache: &mut GlyphCache) -> char {
    for letter in letters() {
        cache.glyph(FaceStyle::Regular, letter, 1);
        if cache.exhausted() {
            return letter;
        }
    }
    panic!("a 64 px atlas swallowed every printable ASCII glyph");
}

#[test]
fn an_atlas_that_cannot_grow_past_its_budget_says_so_instead_of_growing() {
    let mut atlas = GlyphAtlas::sized(1, 64, 64, (64 * 64) as usize);
    let mut refused = false;
    for _ in 0..1000 {
        if atlas.insert(&bitmap(16, 16, 3)).is_err() {
            refused = true;
            break;
        }
    }
    assert!(refused, "the atlas grew past the bytes it was allowed");
    assert_eq!(atlas.bytes(), (64 * 64) as usize);
}

#[test]
fn a_full_atlas_reports_itself_exhausted_rather_than_remembering_a_blank() {
    let mut cache = tiny();
    let lost = fill_until_exhausted(&mut cache);
    assert!(cache.exhausted());
    cache.flush();
    assert!(!cache.exhausted());
    let placed = rendered(cache.glyph(FaceStyle::Regular, lost, 1));
    assert!(
        placed.entry.width > 0,
        "the glyph the full atlas turned away must be rasterized after a flush"
    );
}

#[test]
fn a_flush_renews_both_atlases_with_revisions_no_texture_has_seen() {
    let mut cache = tiny();
    fill_until_exhausted(&mut cache);
    let (mask, color, generation, bytes) = (
        cache.atlases().mask.revision(),
        cache.atlases().color.revision(),
        cache.generation(),
        cache.bytes(),
    );
    cache.flush();
    assert!(cache.atlases().mask.revision() > mask);
    assert!(cache.atlases().color.revision() > color);
    assert!(cache.generation() > generation);
    assert_eq!(cache.entries(), 0);
    assert_eq!(
        cache.bytes(),
        bytes,
        "a renewed atlas keeps the geometry it started with"
    );
    assert_eq!(cache.flushes(), 1);
}

#[test]
fn the_cache_stays_inside_its_budget_however_much_text_it_is_shown() {
    let mut cache = tiny();
    let ceiling = cache.bytes();
    for round in 0..40 {
        for letter in letters() {
            cache.glyph(FaceStyle::Regular, letter, 1);
            cache.glyph(FaceStyle::Bold, letter, 1);
            if cache.exhausted() {
                cache.flush();
            }
        }
        assert_eq!(cache.bytes(), ceiling, "round {}", round);
        assert!(cache.entries() < MAX_ENTRIES, "round {}", round);
    }
}

#[test]
fn a_glyph_too_wide_for_the_atlas_is_told_apart_from_one_that_merely_does_not_fit() {
    let mut cache = tiny();
    assert!(
        matches!(cache.insert(&bitmap(64, 4, 1)), Insertion::Impossible),
        "a glyph wider than the atlas will not fit after a flush either"
    );
    let mut out_of_room = false;
    for _ in 0..100 {
        if matches!(cache.insert(&bitmap(24, 24, 1)), Insertion::OutOfRoom) {
            out_of_room = true;
            break;
        }
    }
    assert!(out_of_room, "the tiny atlas never reported itself full");
}

#[test]
fn only_the_rows_touched_since_an_upload_are_reported_dirty() {
    let mut atlas = GlyphAtlas::new(1);
    let uploaded = atlas.revision();
    assert_eq!(atlas.dirty_rows_since(uploaded), Some((0, 0)));
    let first = atlas.insert(&bitmap(4, 6, 1)).unwrap();
    assert_eq!(
        atlas.dirty_rows_since(uploaded),
        Some((first.y, first.y + 6))
    );
    let seen = atlas.revision();
    for _ in 0..2000 {
        atlas.insert(&bitmap(8, 8, 1)).unwrap();
    }
    let (top, bottom) = atlas.dirty_rows_since(seen).unwrap();
    assert!(top >= first.y && bottom > top);
    assert_eq!(
        atlas.dirty_rows_since(uploaded),
        atlas.dirty_rows_since(uploaded),
    );
}

#[test]
fn growth_forces_a_whole_upload() {
    let mut atlas = GlyphAtlas::new(1);
    let uploaded = atlas.revision();
    while atlas.height() == INITIAL_HEIGHT {
        atlas.insert(&bitmap(64, 64, 1)).unwrap();
    }
    assert_eq!(atlas.dirty_rows_since(uploaded), None);
}

#[test]
fn an_exhausted_cache_stops_rasterizing_until_it_is_flushed() {
    let mut cache = tiny();
    fill_until_exhausted(&mut cache);
    let entries = cache.entries();
    for letter in letters() {
        cache.glyph(FaceStyle::Bold, letter, 1);
    }
    assert_eq!(cache.entries(), entries);
}

#[test]
fn an_overflowing_screen_is_not_flushed_again_straight_away() {
    let mut cache = tiny();
    let now = Instant::now();
    assert!(cache.may_flush(now));
    fill_until_exhausted(&mut cache);
    cache.note_overflow(now);
    assert!(!cache.may_flush(now));
    assert!(cache.may_flush(now + OVERFLOW_RETRY));
    cache.flush();
    cache.settled();
    assert!(cache.may_flush(now));
}

fn every_nerd_font_icon() -> impl Iterator<Item = char> {
    ('\u{e000}'..='\u{f8ff}').chain('\u{f0000}'..='\u{f1af0}')
}

#[test]
fn every_nerd_font_icon_fits_in_the_atlas_at_once_at_normal_and_doubled_sizes() {
    for size in [DEFAULT_FONT_SIZE, DEFAULT_FONT_SIZE * 2.0] {
        let mut cache = GlyphCache::new(FontStack::embedded(size).unwrap());
        let mut drawn = 0;
        for icon in every_nerd_font_icon() {
            if matches!(
                cache.glyph_with_room(FaceStyle::Regular, icon, 1, 1),
                Lookup::Rendered(_)
            ) {
                drawn += 1;
            }
            assert!(
                !cache.exhausted(),
                "the atlas filled up at {:?} after {} icons at size {}",
                icon,
                drawn,
                size
            );
        }
        assert!(
            drawn > 10_000,
            "only {} icons were drawn at size {}",
            drawn,
            size
        );
    }
}

#[test]
fn a_warm_cache_rasterizes_nothing_new_for_a_screen_it_has_seen() {
    let mut cache = cache();
    let icons: Vec<char> = every_nerd_font_icon().take(2000).collect();
    for icon in &icons {
        cache.glyph_with_room(FaceStyle::Regular, *icon, 1, 1);
    }
    let (entries, mask, color) = (
        cache.entries(),
        cache.atlases().mask.revision(),
        cache.atlases().color.revision(),
    );
    for icon in &icons {
        cache.glyph_with_room(FaceStyle::Regular, *icon, 1, 1);
    }
    assert_eq!(cache.entries(), entries);
    assert_eq!(cache.atlases().mask.revision(), mask);
    assert_eq!(cache.atlases().color.revision(), color);
}

#[test]
fn the_entry_table_cannot_grow_without_bound() {
    let mut cache = cache();
    let font = cache.primary_of(FaceStyle::Regular);
    for glyph in 0..MAX_ENTRIES as GlyphId {
        cache.remember(
            RasterKey {
                font,
                glyph,
                budget: 1,
                spill: 0,
            },
            None,
        );
    }
    assert!(cache.exhausted());
    cache.flush();
    assert_eq!(cache.entries(), 0);
}

#[test]
fn a_single_code_point_is_a_char_key_and_anything_longer_a_text_key() {
    assert_eq!(GlyphKey::of("a"), GlyphKey::Char('a'));
    assert_eq!(
        GlyphKey::of("e\u{301}"),
        GlyphKey::Text(Arc::from("e\u{301}"))
    );
    let shared: Arc<str> = Arc::from("\u{1f44d}\u{1f3fd}");
    match GlyphKey::shared(&shared) {
        GlyphKey::Text(text) => assert!(Arc::ptr_eq(&text, &shared)),
        other => panic!("{:?}", other),
    }
}

#[test]
fn a_text_keyed_glyph_is_rasterized_once_per_style_and_budget() {
    let mut cache = cache();
    let key = GlyphKey::of("e\u{301}");
    let first = cache.cluster_glyph(FaceStyle::Regular, &key, 1);
    assert!(matches!(first, ClusterLookup::Sprite(_)), "{:?}", first);
    let revision = cache.atlases().mask.revision();
    let entries = cache.cluster_entries();
    assert_eq!(cache.cluster_glyph(FaceStyle::Regular, &key, 1), first);
    assert_eq!(cache.atlases().mask.revision(), revision);
    assert_eq!(cache.cluster_entries(), entries);

    let bold = cache.cluster_glyph(FaceStyle::Bold, &key, 1);
    assert_ne!(bold, first);
    assert_eq!(cache.cluster_entries(), entries + 1);
}

#[test]
fn text_and_char_keys_never_share_an_entry() {
    let mut cache = cache();
    cache.cluster_glyph(FaceStyle::Regular, &GlyphKey::of("e\u{301}"), 1);
    cache.cluster_glyph(FaceStyle::Regular, &GlyphKey::Char('e'), 1);
    assert_eq!(cache.cluster_entries(), 2);
}

#[test]
fn an_emoji_sequence_lands_in_the_color_atlas_as_one_entry() {
    let mut cache = cache();
    let warning = GlyphKey::of("\u{26a0}\u{fe0f}");
    let before = cache.atlases().mask.revision();
    match cache.cluster_glyph(FaceStyle::Regular, &warning, 2) {
        ClusterLookup::Sprite(rendered) => {
            assert_eq!(rendered.content, GlyphContent::Color);
            assert!(rendered.entry.width <= cache.metrics().width * 2);
        },
        other => panic!(
            "the emoji-style warning sign was not one sprite: {:?}",
            other
        ),
    }
    assert_eq!(cache.atlases().mask.revision(), before);
    let entries = cache.cluster_entries();
    let family = GlyphKey::of("\u{1f468}\u{200d}\u{1f469}\u{200d}\u{1f467}");
    match cache.cluster_glyph(FaceStyle::Regular, &family, 2) {
        ClusterLookup::Sprite(rendered) => {
            assert_eq!(rendered.content, GlyphContent::Color);
            assert!(rendered.entry.width <= cache.metrics().width * 2);
        },
        other => panic!("the family was not one sprite: {:?}", other),
    }
    assert_eq!(cache.cluster_entries(), entries + 1);
    assert_eq!(cache.atlases().mask.revision(), before);
}

#[test]
fn a_cluster_nothing_covers_is_remembered_as_drawn_cell_by_cell() {
    let mut cache = cache();
    let key = GlyphKey::of("\u{10fffd}\u{301}");
    assert_eq!(
        cache.cluster_glyph(FaceStyle::Regular, &key, 1),
        ClusterLookup::PerCell
    );
    assert_eq!(cache.cluster_entries(), 1);
}

#[test]
fn text_keyed_glyphs_are_evicted_by_a_flush_and_come_back_afterwards() {
    let mut cache = tiny();
    let mut lost = None;
    for letter in letters() {
        let key = GlyphKey::of(&format!("{}\u{301}\u{323}", letter));
        cache.cluster_glyph(FaceStyle::Regular, &key, 1);
        if cache.exhausted() {
            lost = Some(key);
            break;
        }
    }
    let lost = lost.expect("a 64 px atlas swallowed every accented letter");
    assert!(cache.cluster_entries() > 0);
    assert_eq!(
        cache.cluster_glyph(FaceStyle::Regular, &lost, 1),
        ClusterLookup::Blank,
        "an exhausted cache must not rasterize more text keys"
    );
    cache.flush();
    assert_eq!(cache.cluster_entries(), 0);
    assert_eq!(cache.entries(), 0);
    assert!(matches!(
        cache.cluster_glyph(FaceStyle::Regular, &lost, 1),
        ClusterLookup::Sprite(_)
    ));
}

#[test]
fn text_keys_count_against_the_entry_ceiling() {
    let mut cache = cache();
    for index in 0..MAX_ENTRIES {
        cache.clusters.insert(
            (
                FaceStyle::Regular,
                GlyphKey::Text(Arc::from(format!("{}\u{301}", index))),
                1,
            ),
            ClusterLookup::Blank,
        );
    }
    cache.cluster_glyph(FaceStyle::Regular, &GlyphKey::of("e\u{301}"), 1);
    assert!(cache.exhausted());
    cache.flush();
    assert_eq!(cache.entries(), 0);
}
