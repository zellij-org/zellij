use super::*;
use std::collections::BTreeSet;
use zellij_utils::structured_render::{LINK_NONE, UNDERLINE_SHIFT};

fn plain(character: char) -> MirrorCell {
    MirrorCell::plain(WireCell {
        ch: character as u32,
        ..WireCell::BLANK
    })
}

fn screen_with(rows: &[Vec<MirrorCell>], cols: u16) -> MirrorScreen {
    let mut screen = MirrorScreen::blank(cols, rows.len() as u16);
    for (y, row) in rows.iter().enumerate() {
        for (x, cell) in row.iter().enumerate() {
            screen.cells[y * cols as usize + x] = cell.clone();
        }
    }
    screen
}

fn text_row(text: &str) -> Vec<MirrorCell> {
    text.chars().map(plain).collect()
}

fn chunk_text(chunk: &CharacterChunk) -> String {
    let mut text = String::new();
    for character in &chunk.terminal_characters {
        character.push_text_to(&mut text);
    }
    text
}

fn no_links(_: &str) -> Option<LinkAnchor> {
    None
}

#[test]
fn styles_and_colours_become_character_styles() {
    let cell = WireCell {
        ch: 'x' as u32,
        fg: WireColor::Rgb(10, 20, 30).pack(),
        bg: WireColor::Indexed(200).pack(),
        underline_color: WireColor::Named(1).pack(),
        attrs: ATTR_BOLD | ATTR_ITALIC | ATTR_REVERSE | (UNDERLINE_CURLY << UNDERLINE_SHIFT),
        width: 1,
        link: LINK_NONE,
    };
    let styles = cell_styles(&cell, None, true);
    assert_eq!(styles.foreground, Some(AnsiCode::RgbCode((10, 20, 30))));
    assert_eq!(styles.background, Some(AnsiCode::ColorIndex(200)));
    assert_eq!(
        styles.underline_color,
        Some(AnsiCode::NamedColor(NamedColor::Red))
    );
    assert_eq!(styles.bold, Some(AnsiCode::On));
    assert_eq!(styles.italic, Some(AnsiCode::On));
    assert_eq!(styles.reverse, Some(AnsiCode::On));
    assert_eq!(styles.dim, Some(AnsiCode::Reset));
    assert_eq!(styles.strike, Some(AnsiCode::Reset));
    assert_eq!(
        styles.underline,
        Some(AnsiCode::Underline(Some(AnsiStyledUnderline::Undercurl)))
    );
    assert_eq!(styles.link_anchor, Some(LinkAnchor::End));
    let blank = cell_styles(&WireCell::BLANK, None, true);
    assert_eq!(blank.foreground, Some(AnsiCode::Reset));
    assert_eq!(blank.background, Some(AnsiCode::Reset));
    assert_eq!(blank.underline, Some(AnsiCode::Reset));
}

#[test]
fn every_named_colour_keeps_its_index() {
    let expected = [
        NamedColor::Black,
        NamedColor::Red,
        NamedColor::Green,
        NamedColor::Yellow,
        NamedColor::Blue,
        NamedColor::Magenta,
        NamedColor::Cyan,
        NamedColor::White,
        NamedColor::BrightBlack,
        NamedColor::BrightRed,
        NamedColor::BrightGreen,
        NamedColor::BrightYellow,
        NamedColor::BrightBlue,
        NamedColor::BrightMagenta,
        NamedColor::BrightCyan,
        NamedColor::BrightWhite,
    ];
    for (index, named) in expected.iter().enumerate() {
        assert_eq!(
            ansi_color(WireColor::Named(index as u8).pack()),
            AnsiCode::NamedColor(*named)
        );
    }
}

#[test]
fn a_wide_character_takes_one_character_and_two_columns() {
    let wide = MirrorCell::plain(WireCell {
        ch: '中' as u32,
        width: 2,
        ..WireCell::BLANK
    });
    let row = vec![plain('a'), wide, plain(' '), plain('b')];
    let screen = screen_with(&[row], 4);
    let chunk = screen_row_chunk(&screen, 0, 3, 7, 4, &mut no_links, true);
    assert_eq!((chunk.x, chunk.y), (3, 7));
    assert_eq!(chunk.width(), 4);
    assert_eq!(chunk.terminal_characters.len(), 3);
    assert_eq!(chunk.terminal_characters[1].character, '中');
    assert_eq!(chunk.terminal_characters[1].width(), 2);
    assert_eq!(chunk_text(&chunk), "a中b");
}

#[test]
fn a_wide_character_that_does_not_fit_is_replaced_by_blanks() {
    let wide = MirrorCell::plain(WireCell {
        ch: '中' as u32,
        width: 2,
        ..WireCell::BLANK
    });
    let row = vec![plain('a'), plain('b'), wide, plain(' ')];
    let screen = screen_with(&[row], 4);
    let chunk = screen_row_chunk(&screen, 0, 0, 0, 3, &mut no_links, true);
    assert_eq!(chunk.width(), 3);
    assert_eq!(chunk_text(&chunk), "ab ");
}

#[test]
fn a_grapheme_cluster_stays_one_character() {
    let cluster = MirrorCell {
        cell: WireCell {
            ch: 0,
            width: 2,
            ..WireCell::BLANK
        },
        text: Some("\u{1F44D}\u{1F3FD}".to_owned()),
        link: None,
    };
    let row = vec![cluster, plain(' '), plain('z')];
    let screen = screen_with(&[row], 3);
    let chunk = screen_row_chunk(&screen, 0, 0, 0, 3, &mut no_links, true);
    assert!(chunk.may_have_groups);
    assert_eq!(chunk.terminal_characters.len(), 2);
    assert!(chunk.terminal_characters[0].has_attached());
    assert_eq!(chunk.terminal_characters[0].width(), 2);
    assert_eq!(chunk_text(&chunk), "\u{1F44D}\u{1F3FD}z");
}

#[test]
fn a_link_is_registered_and_carried_by_the_cell() {
    let linked = MirrorCell {
        link: Some("https://example.com".to_owned()),
        ..plain('l')
    };
    let row = vec![linked, plain('n')];
    let screen = screen_with(&[row], 2);
    let mut asked = vec![];
    let mut link_for = |uri: &str| {
        asked.push(uri.to_owned());
        Some(LinkAnchor::Start(7))
    };
    let chunk = screen_row_chunk(&screen, 0, 0, 0, 2, &mut link_for, true);
    assert_eq!(asked, vec!["https://example.com".to_owned()]);
    assert_eq!(
        chunk.terminal_characters[0].styles.link_anchor,
        Some(LinkAnchor::Start(7))
    );
    assert_eq!(
        chunk.terminal_characters[1].styles.link_anchor,
        Some(LinkAnchor::End)
    );
}

#[test]
fn a_smaller_source_is_padded_with_blank_rows_and_columns() {
    let screen = screen_with(&[text_row("hi"), text_row("yo")], 2);
    let area = LiveViewRect::new(10, 5, 6, 4);
    let chunks = screen_chunks(&screen, &area, &MirrorRedraw::Full, &mut no_links, true);
    assert_eq!(chunks.len(), 4);
    for (index, chunk) in chunks.iter().enumerate() {
        assert_eq!((chunk.x, chunk.y), (10, 5 + index));
        assert_eq!(chunk.width(), 6);
    }
    assert_eq!(chunk_text(&chunks[0]), "hi    ");
    assert_eq!(chunk_text(&chunks[1]), "yo    ");
    assert_eq!(chunk_text(&chunks[3]), "      ");
}

#[test]
fn a_larger_source_is_cropped_to_the_rectangle() {
    let screen = screen_with(
        &[
            text_row("abcdefgh"),
            text_row("ijklmnop"),
            text_row("qrstuvwx"),
        ],
        8,
    );
    let area = LiveViewRect::new(0, 0, 3, 2);
    let chunks = screen_chunks(&screen, &area, &MirrorRedraw::Full, &mut no_links, true);
    assert_eq!(chunks.len(), 2);
    assert_eq!(chunk_text(&chunks[0]), "abc");
    assert_eq!(chunk_text(&chunks[1]), "ijk");
}

#[test]
fn a_taller_source_is_cropped_to_keep_the_cursor_row_in_view() {
    assert_eq!(row_offset(3, 0, 2), 0);
    assert_eq!(row_offset(3, 1, 2), 0);
    assert_eq!(row_offset(3, 2, 2), 1);
    assert_eq!(row_offset(10, 9, 4), 6);
    assert_eq!(row_offset(10, 40, 4), 6);
    assert_eq!(row_offset(2, 1, 4), 0);
    assert_eq!(row_offset(5, 4, 0), 0);
    let screen = screen_with(
        &[
            text_row("one"),
            text_row("two"),
            text_row("six"),
            text_row("ten"),
        ],
        3,
    );
    let area = LiveViewRect::new(0, 3, 3, 2);
    let chunks = screen_chunks_from(&screen, &area, &MirrorRedraw::Full, 2, &mut no_links, true);
    assert_eq!(chunks.len(), 2);
    assert_eq!((chunks[0].y, chunk_text(&chunks[0])), (3, "six".to_owned()));
    assert_eq!((chunks[1].y, chunk_text(&chunks[1])), (4, "ten".to_owned()));
    let rows: BTreeSet<u16> = [0, 3].into_iter().collect();
    let chunks = screen_chunks_from(
        &screen,
        &area,
        &MirrorRedraw::Rows(rows),
        2,
        &mut no_links,
        true,
    );
    assert_eq!(chunks.len(), 1);
    assert_eq!((chunks[0].y, chunk_text(&chunks[0])), (4, "ten".to_owned()));
}

fn screen_of(lines: &[&str], rows: usize, cols: u16) -> MirrorScreen {
    let mut all: Vec<Vec<MirrorCell>> = lines.iter().map(|line| text_row(line)).collect();
    all.resize(rows, vec![]);
    screen_with(&all, cols)
}

#[test]
fn a_visible_cursor_keeps_its_row_in_view() {
    let mut screen = screen_of(&["a", "b", "c", "d", "e", "f"], 6, 3);
    screen.cursor.visible = true;
    screen.cursor.y = 1;
    assert_eq!(anchor_row(&screen), 1);
    assert_eq!(screen_offset(&screen, 3), 0);
    screen.cursor.y = 4;
    assert_eq!(screen_offset(&screen, 3), 2);
}

#[test]
fn a_hidden_cursor_keeps_the_last_row_with_content_in_view() {
    let mut screen = screen_of(&["top", "", "", "", "", "end"], 6, 3);
    screen.cursor.visible = false;
    screen.cursor.y = 0;
    assert_eq!(anchor_row(&screen), 5);
    assert_eq!(screen_offset(&screen, 3), 3);
}

#[test]
fn a_hidden_cursor_on_a_nearly_empty_screen_shows_the_top() {
    let mut screen = screen_of(&["one", "two", "six"], 10, 3);
    screen.cursor.visible = false;
    assert_eq!(anchor_row(&screen), 2);
    assert_eq!(screen_offset(&screen, 4), 0);
    let empty = screen_of(&[], 10, 3);
    assert_eq!(anchor_row(&empty), 0);
    assert_eq!(screen_offset(&empty, 4), 0);
}

#[test]
fn a_coloured_blank_row_counts_as_content() {
    let mut screen = screen_of(&["one"], 6, 2);
    let painted = MirrorCell::plain(WireCell {
        bg: WireColor::Indexed(4).pack(),
        ..WireCell::BLANK
    });
    screen.cells[5 * 2] = painted;
    screen.cursor.visible = false;
    assert_eq!(anchor_row(&screen), 5);
}

#[test]
fn a_row_redraw_draws_only_the_changed_rows_inside_the_rectangle() {
    let screen = screen_with(&[text_row("one"), text_row("two"), text_row("six")], 3);
    let area = LiveViewRect::new(1, 1, 3, 2);
    let rows: BTreeSet<u16> = [1, 2].into_iter().collect();
    let chunks = screen_chunks(
        &screen,
        &area,
        &MirrorRedraw::Rows(rows),
        &mut no_links,
        true,
    );
    assert_eq!(chunks.len(), 1);
    assert_eq!(chunks[0].y, 2);
    assert_eq!(chunk_text(&chunks[0]), "two");
    let nothing = screen_chunks(&screen, &area, &MirrorRedraw::Nothing, &mut no_links, true);
    assert!(nothing.is_empty());
    let full = screen_chunks(&screen, &area, &MirrorRedraw::Full, &mut no_links, true);
    assert_eq!(full.len(), 2);
}

#[test]
fn a_rectangle_is_clamped_to_the_content_area() {
    let rect = LiveViewRect::new(5, 2, 40, 10);
    assert_eq!(rect.clamped_to(80, 20), rect);
    assert_eq!(rect.clamped_to(20, 8), LiveViewRect::new(5, 2, 15, 6));
    assert!(rect.clamped_to(4, 8).is_empty());
    assert!(rect.clamped_to(20, 1).is_empty());
    let content = LiveViewRect::new(30, 4, 20, 8);
    assert_eq!(rect.placed_in(&content), LiveViewRect::new(35, 6, 15, 6));
    assert!(rect.placed_in(&content).contains(35, 6));
    assert!(!rect.placed_in(&content).contains(50, 6));
    assert!(!rect.placed_in(&content).contains(35, 12));
}

#[test]
fn a_placeholder_is_centred_in_the_rectangle() {
    let area = LiveViewRect::new(2, 3, 20, 5);
    let lines = vec!["The pane was closed".to_owned(), "api / 3".to_owned()];
    let chunks = placeholder_chunks(&lines, &area);
    assert_eq!(chunks.len(), 5);
    assert!(chunks
        .iter()
        .all(|chunk| chunk.width() == 20 && chunk.x == 2));
    assert_eq!(chunk_text(&chunks[1]), "The pane was closed ");
    assert_eq!(chunk_text(&chunks[2]), "      api / 3       ");
    assert_eq!(
        chunks[1].terminal_characters[0].styles.bold,
        Some(AnsiCode::On)
    );
    assert_eq!(chunk_text(&chunks[0]).trim(), "");
    assert!(placeholder_chunks(&lines, &LiveViewRect::new(0, 0, 0, 3)).is_empty());
}

#[test]
fn a_long_placeholder_line_is_cut_to_the_rectangle() {
    let area = LiveViewRect::new(0, 0, 5, 1);
    let chunks = placeholder_chunks(&["Connecting...".to_owned()], &area);
    assert_eq!(chunks.len(), 1);
    assert_eq!(chunk_text(&chunks[0]), "Conne");
}

#[test]
fn cursor_shapes_map_back_to_their_sequences() {
    assert_eq!(cursor_shape_csi(CURSOR_SHAPE_BEAM), "\u{1b}[6 q");
    assert_eq!(
        cursor_shape_csi(CURSOR_SHAPE_UNDERLINE | CURSOR_BLINKING),
        "\u{1b}[3 q"
    );
    assert_eq!(cursor_shape_csi(CURSOR_SHAPE_BLOCK), "\u{1b}[2 q");
}
