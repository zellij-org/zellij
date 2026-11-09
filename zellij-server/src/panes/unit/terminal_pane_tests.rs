use super::super::{TerminalPane, MAX_PENDING_PTY_BYTES};
use crate::panes::kitty_graphics::KittyImageStore;
use crate::panes::sixel::SixelImageStore;
use crate::tab::Pane;
use insta::assert_snapshot;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use zellij_utils::{
    data::{Palette, Style},
    pane_size::{Offset, PaneGeom, SizeInPixels},
    position::Position,
};

use std::fmt::Write;

fn terminal_pane_with_size(rows: usize, cols: usize) -> TerminalPane {
    let mut geom = PaneGeom::default();
    geom.cols.set_inner(cols);
    geom.rows.set_inner(rows);
    TerminalPane::new(
        1,
        geom,
        Style::default(),
        0,
        String::new(),
        Rc::new(RefCell::new(None)),
        Rc::new(RefCell::new(SixelImageStore::default())),
        Rc::new(RefCell::new(KittyImageStore::default())),
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
    )
}

fn read_fixture(fixture_name: &str) -> Vec<u8> {
    let mut path_to_file = std::path::PathBuf::new();
    path_to_file.push("../src");
    path_to_file.push("tests");
    path_to_file.push("fixtures");
    path_to_file.push(fixture_name);
    std::fs::read(path_to_file)
        .unwrap_or_else(|_| panic!("could not read fixture {:?}", &fixture_name))
}

#[test]
pub fn scrolling_inside_a_pane() {
    let fake_client_id = 1;
    let mut fake_win_size = PaneGeom::default();
    fake_win_size.cols.set_inner(121);
    fake_win_size.rows.set_inner(20);

    let pid = 1;
    let style = Style::default();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_colors = Rc::new(RefCell::new(Palette::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let osc8_hyperlinks = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut terminal_pane = TerminalPane::new(
        pid,
        fake_win_size,
        style,
        0,
        String::new(),
        Rc::new(RefCell::new(None)),
        sixel_image_store,
        Rc::new(RefCell::new(KittyImageStore::default())),
        terminal_emulator_colors,
        terminal_emulator_color_codes,
        None,
        None,
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
        None,
    ); // 0 is the pane index
    let mut text_to_fill_pane = String::new();
    for i in 0..30 {
        writeln!(&mut text_to_fill_pane, "\rline {}", i + 1).unwrap();
    }
    terminal_pane.handle_pty_bytes(text_to_fill_pane.into_bytes());
    terminal_pane.scroll_up(10, fake_client_id);
    assert_snapshot!(format!("{:?}", terminal_pane.grid));
    terminal_pane.scroll_down(3, fake_client_id);
    assert_snapshot!(format!("{:?}", terminal_pane.grid));
    terminal_pane.clear_scroll();
    assert_snapshot!(format!("{:?}", terminal_pane.grid));
}

#[test]
fn maximum_rep_resumes_from_deferred_terminal_pane_work() {
    let mut geom = PaneGeom::default();
    geom.cols.set_inner(10);
    geom.rows.set_inner(4);
    let mut pane = TerminalPane::new(
        1,
        geom,
        Style::default(),
        0,
        String::new(),
        Rc::new(RefCell::new(None)),
        Rc::new(RefCell::new(SixelImageStore::default())),
        Rc::new(RefCell::new(KittyImageStore::default())),
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
    pane.handle_pty_bytes(b"a\x1b[65535bZ".to_vec());
    assert!(pane.grid.rep_pending());
    assert!(pane.has_pending_pty_input());

    let mut steps = 0;
    while pane.has_deferred_pane_work() || pane.has_pending_pty_input() {
        if pane.has_deferred_pane_work() {
            pane.advance_deferred_pane_work();
        } else {
            pane.handle_pty_bytes(Vec::new());
        }
        steps += 1;
        assert!(steps < 100_000, "REP deferred work did not settle");
    }
    assert!(steps > 1, "maximum REP must span multiple pane turns");
    assert!(!pane.grid.rep_pending());
    assert!(!pane.has_pending_pty_input());
    assert!(
        pane.grid
            .lines_above
            .iter()
            .chain(pane.grid.viewport.iter())
            .flat_map(|row| row.columns.iter())
            .any(|cell| cell.character == 'Z'),
        "bytes after REP must be applied after the repeat settles"
    );
}

#[test]
fn unfinished_rep_counts_as_pending_output_after_its_bytes_are_consumed() {
    let mut pane = terminal_pane_with_size(4, 10);
    pane.handle_pty_bytes_for_test(b"a\x1b[65535b".to_vec());
    assert!(pane.grid.rep_pending());
    assert!(
        !pane.has_pending_pty_input(),
        "the repeat sequence itself is fully consumed"
    );
    assert!(pane.has_pending_output());

    while pane.grid.rep_pending() {
        pane.advance_deferred_pane_work();
    }
    assert!(!pane.has_pending_output());
}

#[test]
fn scrolled_rep_continues_on_the_live_model() {
    let mut pane = terminal_pane_with_size(3, 2048);
    for _ in 0..5 {
        pane.handle_pty_bytes_for_test(vec![b'H'; 2048]);
        pane.handle_pty_bytes_for_test(b"\r\n".to_vec());
    }
    assert!(pane.grid.lines_above.len() >= 2);
    pane.scroll_up(2, 1);
    assert!(pane.is_scrolled());

    pane.handle_pty_bytes_for_test(b"\x1b[1;1HA\x1b[1025bZ".to_vec());
    assert!(pane.grid.rep_pending());
    let mut steps = 0;
    while pane.has_deferred_pane_work() || pane.has_pending_pty_input() {
        pane.advance_deferred_pane_work();
        steps += 1;
        assert!(steps < 10_000, "REP deferred work did not settle");
    }
    assert!(
        pane.is_scrolled(),
        "deferred REP should restore the held history view"
    );

    pane.clear_scroll();
    let viewport = pane.grid.as_character_lines();
    let first_row: String = viewport[0].iter().map(|cell| cell.character).collect();
    let expected = format!("{}Z", "A".repeat(1026));
    assert!(
        first_row.starts_with(&expected),
        "repeat resumed in history instead of the live model: {:?}",
        &first_row[..first_row.len().min(40)]
    );
}

#[test]
fn rep_paused_output_is_bounded_and_dropped_oldest() {
    let mut pane = terminal_pane_with_size(4, 10);
    pane.handle_pty_bytes_for_test(b"a\x1b[65535b".to_vec());
    assert!(pane.grid.rep_pending());

    let chunk = vec![b'x'; MAX_PENDING_PTY_BYTES + 10_000];
    pane.handle_pty_bytes_for_test(chunk);
    pane.handle_pty_bytes_for_test(vec![b'y'; 10_000]);
    let pending = pane.pending_pty_input.len() - pane.pending_pty_offset;
    assert!(
        pending <= MAX_PENDING_PTY_BYTES,
        "pending bytes grew past the admission ceiling: {pending}"
    );
    assert!(pane.has_pending_output());
}

#[test]
fn image_capable_chunks_do_not_park_scrolled_panes() {
    let mut pane = terminal_pane_with_size(4, 20);
    assert!(pane.input_is_parkable(b"plain text"));
    assert!(!pane.input_is_parkable(b"\x1bPq#0;2;0;0;0"));
    assert!(!pane.input_is_parkable(b"\x1b_Ga=t,f=24;AAAA\x1b\\"));
    assert!(!pane.input_is_parkable(b"\x1b]1337;File=inline=1:AAAA\x07"));
    pane.pending_pty_input = b"queued \x1b_Ga=t,f=24;AAAA\x1b\\".to_vec();
    assert!(
        !pane.input_is_parkable(b""),
        "a queued image suffix must revoke parking"
    );
    pane.pending_pty_input.clear();
    pane.pending_pty_input = b"queued \x1b".to_vec();
    assert!(
        !pane.input_is_parkable(b"Pq#0;2;0;0;0"),
        "an introducer split across the queue boundary must revoke parking"
    );
    pane.pending_pty_input.clear();
    pane.held_escape = true;
    assert!(
        !pane.input_is_parkable(b"text"),
        "a retained escape prefix must revoke parking"
    );
}

#[test]
fn backlog_overflow_rejects_the_whole_queue_instead_of_splicing_a_suffix() {
    for (prefix, suffix) in [
        (b"\x1b]2;secret".as_slice(), b"VISIBLE\x07".as_slice()),
        (b"\x1b[31".as_slice(), b"mVISIBLE".as_slice()),
        (b"\x1bPq".as_slice(), b"#0;2;0;0;0".as_slice()),
        (b"\x1b_Ga=t,f=24;".as_slice(), b"AAAA\x1b\\".as_slice()),
        (b"\xe2\x82".as_slice(), b"\xac".as_slice()),
    ] {
        let mut pane = terminal_pane_with_size(4, 20);
        pane.append_pending_pty_input(prefix);
        let queued = pane.pending_pty_input.len();
        pane.append_pending_pty_input(&vec![b'x'; MAX_PENDING_PTY_BYTES - queued + 1]);
        assert!(!pane.has_pending_pty_input(), "queue survived overflow");
        assert!(pane.vte_parser.is_ground(), "parser retained a prefix");
        assert!(pane.output_truncated);
        pane.handle_pty_bytes_for_test(suffix.to_vec());
        assert!(
            pane.grid.title.is_none(),
            "suffix completed the discarded title sequence"
        );
    }
}

#[test]
fn clear_scroll_moves_at_most_one_row_quantum() {
    use crate::panes::grid::SCREEN_ROW_QUANTUM;

    let mut pane = terminal_pane_with_size(4, 20);
    for index in 0..(SCREEN_ROW_QUANTUM + 30) {
        pane.handle_pty_bytes_for_test(format!("L{index}\r\n").into_bytes());
    }
    pane.scroll_up(SCREEN_ROW_QUANTUM + 10, 1);
    let before = pane.grid.lines_below.len();
    assert!(
        before > SCREEN_ROW_QUANTUM,
        "the fixture must sit further than one quantum from the bottom, below={before}"
    );
    pane.clear_scroll();
    let moved = before - pane.grid.lines_below.len();
    assert!(
        !pane.grid.lines_below.is_empty(),
        "one clear_scroll finished a {before}-row return"
    );
    assert!(
        moved > 0 && moved <= SCREEN_ROW_QUANTUM,
        "clear_scroll moved {moved} rows"
    );
    assert!(pane.viewport_goal_pending());
    let mid = pane.grid.lines_below.len();
    pane.advance_deferred_pane_work();
    assert!(
        pane.grid.lines_below.len() < mid,
        "a later turn should continue the scroll to the bottom"
    );
}

#[test]
fn deep_scroll_defers_image_bytes_and_moves_one_quantum() {
    use crate::panes::grid::SCREEN_ROW_QUANTUM;

    let mut pane = terminal_pane_with_size(5, 20);
    for index in 0..(SCREEN_ROW_QUANTUM + 40) {
        pane.handle_pty_bytes_for_test(format!("row-{index}\r\n").into_bytes());
    }
    pane.scroll_up(SCREEN_ROW_QUANTUM + 20, 1);
    let before = pane.grid.lines_below.len();
    assert!(before > SCREEN_ROW_QUANTUM);
    let bytes = b"\x1b_Ga=t,f=24;AAAA\x1b\\";
    pane.handle_pty_bytes_for_test(bytes.to_vec());
    let moved = before - pane.grid.lines_below.len();
    assert!(
        !pane.grid.lines_below.is_empty(),
        "image input snapped the whole scrolled distance"
    );
    assert!(
        moved <= SCREEN_ROW_QUANTUM,
        "image input moved {moved} rows in one turn"
    );
    let queued = pane.pending_pty_input.len() - pane.pending_pty_offset;
    assert_eq!(
        queued,
        bytes.len(),
        "image bytes were parsed against a half-moved viewport"
    );
}

#[test]
fn scrolled_image_output_does_not_resume_in_history() {
    let mut pane = terminal_pane_with_size(5, 20);
    for _ in 0..30 {
        pane.handle_pty_bytes_for_test(b"HHHHHHHHHHHHHHHHHHHH\r\n".to_vec());
    }
    assert!(pane.grid.lines_above.len() > 1);
    pane.scroll_up(1, 1);
    assert!(pane.is_scrolled());

    let sixel = "\u{1b}Pq\n#0;2;0;0;0\n#1;2;100;100;0\n#2;2;0;100;0\n\
                 #1~~@@vv@@~~@@~~$\n#2??}}GG}}??}}??-\n#1!14@\n\u{1b}\\";
    pane.handle_pty_bytes_for_test(format!("{sixel}\r\nX").into_bytes());
    pane.clear_scroll();

    let viewport = pane.grid.as_character_lines();
    let contains_x = viewport
        .iter()
        .any(|row| row.iter().any(|cell| cell.character == 'X'));
    assert!(
        contains_x,
        "output after a scrolled image landed in history instead of the live model"
    );
}

#[test]
fn image_input_revokes_a_park_created_by_an_earlier_text_quantum() {
    let mut pane = terminal_pane_with_size(5, 20);
    for _ in 0..30 {
        pane.handle_pty_bytes_for_test(b"HHHHHHHHHHHHHHHHHHHH\r\n".to_vec());
    }
    pane.scroll_up(2, 1);
    assert!(pane.grid.park_at_live_bottom());

    pane.with_live_terminal(b"\x1b_Ga=t,f=24;AAAA\x1b\\", |pane| {
        assert!(
            !pane.grid.is_live_bottom_parked(),
            "image input must leave the parked historical projection"
        );
        assert!(!pane.grid.is_scrolled);
    });
    assert!(
        !pane.grid.is_live_bottom_parked(),
        "the historical view must be restored without a live-model park"
    );
}

#[test]
pub fn sixel_image_inside_terminal_pane() {
    let mut fake_win_size = PaneGeom::default();
    fake_win_size.cols.set_inner(121);
    fake_win_size.rows.set_inner(20);

    let pid = 1;
    let style = Style::default();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_colors = Rc::new(RefCell::new(Palette::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let character_cell_size = Rc::new(RefCell::new(Some(SizeInPixels {
        width: 8,
        height: 21,
    })));
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let osc8_hyperlinks = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut terminal_pane = TerminalPane::new(
        pid,
        fake_win_size,
        style,
        0,
        String::new(),
        character_cell_size,
        sixel_image_store,
        Rc::new(RefCell::new(KittyImageStore::default())),
        terminal_emulator_colors,
        terminal_emulator_color_codes,
        None,
        None,
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
        None,
    ); // 0 is the pane index
    let sixel_image_bytes = "\u{1b}Pq
        #0;2;0;0;0#1;2;100;100;0#2;2;0;100;0
        #1~~@@vv@@~~@@~~$
        #2??}}GG}}??}}??-
        #1!14@
        \u{1b}\\";

    terminal_pane.handle_pty_bytes(Vec::from(sixel_image_bytes.as_bytes()));
    assert_snapshot!(format!("{:?}", terminal_pane.grid));
}

#[test]
pub fn partial_sixel_image_inside_terminal_pane() {
    // here we test to make sure we partially render an image that is partially hidden in the
    // scrollbuffer
    let mut fake_win_size = PaneGeom::default();
    fake_win_size.cols.set_inner(121);
    fake_win_size.rows.set_inner(20);

    let pid = 1;
    let style = Style::default();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_colors = Rc::new(RefCell::new(Palette::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let character_cell_size = Rc::new(RefCell::new(Some(SizeInPixels {
        width: 8,
        height: 21,
    })));
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let osc8_hyperlinks = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut terminal_pane = TerminalPane::new(
        pid,
        fake_win_size,
        style,
        0,
        String::new(),
        character_cell_size,
        sixel_image_store,
        Rc::new(RefCell::new(KittyImageStore::default())),
        terminal_emulator_colors,
        terminal_emulator_color_codes,
        None,
        None,
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
        None,
    ); // 0 is the pane index
    let pane_content = read_fixture("sixel-image-500px.six");
    terminal_pane.handle_pty_bytes(pane_content);
    assert_snapshot!(format!("{:?}", terminal_pane.grid));
}

#[test]
pub fn overflowing_sixel_image_inside_terminal_pane() {
    // here we test to make sure we properly render an image that overflows both in the width and
    // height of the pane
    let mut fake_win_size = PaneGeom::default();
    fake_win_size.cols.set_inner(50);
    fake_win_size.rows.set_inner(20);

    let pid = 1;
    let style = Style::default();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_colors = Rc::new(RefCell::new(Palette::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let character_cell_size = Rc::new(RefCell::new(Some(SizeInPixels {
        width: 8,
        height: 21,
    })));
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let osc8_hyperlinks = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut terminal_pane = TerminalPane::new(
        pid,
        fake_win_size,
        style,
        0,
        String::new(),
        character_cell_size,
        sixel_image_store,
        Rc::new(RefCell::new(KittyImageStore::default())),
        terminal_emulator_colors,
        terminal_emulator_color_codes,
        None,
        None,
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
        None,
    ); // 0 is the pane index
    let pane_content = read_fixture("sixel-image-500px.six");
    terminal_pane.handle_pty_bytes(pane_content);
    assert_snapshot!(format!("{:?}", terminal_pane.grid));
}

#[test]
pub fn scrolling_through_a_sixel_image() {
    let fake_client_id = 1;
    let mut fake_win_size = PaneGeom::default();
    fake_win_size.cols.set_inner(121);
    fake_win_size.rows.set_inner(20);

    let pid = 1;
    let style = Style::default();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_colors = Rc::new(RefCell::new(Palette::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let character_cell_size = Rc::new(RefCell::new(Some(SizeInPixels {
        width: 8,
        height: 21,
    })));
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let osc8_hyperlinks = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut terminal_pane = TerminalPane::new(
        pid,
        fake_win_size,
        style,
        0,
        String::new(),
        character_cell_size,
        sixel_image_store,
        Rc::new(RefCell::new(KittyImageStore::default())),
        terminal_emulator_colors,
        terminal_emulator_color_codes,
        None,
        None,
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
        None,
    ); // 0 is the pane index
    let mut text_to_fill_pane = String::new();
    for i in 0..30 {
        writeln!(&mut text_to_fill_pane, "\rline {}", i + 1).unwrap();
    }
    writeln!(&mut text_to_fill_pane, "\r").unwrap();
    let pane_sixel_content = read_fixture("sixel-image-500px.six");
    terminal_pane.handle_pty_bytes(text_to_fill_pane.into_bytes());
    terminal_pane.handle_pty_bytes(pane_sixel_content);
    terminal_pane.scroll_up(10, fake_client_id);
    assert_snapshot!(format!("{:?}", terminal_pane.grid));
    terminal_pane.scroll_down(3, fake_client_id);
    assert_snapshot!(format!("{:?}", terminal_pane.grid));
    terminal_pane.clear_scroll();
    assert_snapshot!(format!("{:?}", terminal_pane.grid));
}

#[test]
pub fn multiple_sixel_images_in_pane() {
    let fake_client_id = 1;
    let mut fake_win_size = PaneGeom::default();
    fake_win_size.cols.set_inner(121);
    fake_win_size.rows.set_inner(20);

    let pid = 1;
    let style = Style::default();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_colors = Rc::new(RefCell::new(Palette::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let character_cell_size = Rc::new(RefCell::new(Some(SizeInPixels {
        width: 8,
        height: 21,
    })));
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let osc8_hyperlinks = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut terminal_pane = TerminalPane::new(
        pid,
        fake_win_size,
        style,
        0,
        String::new(),
        character_cell_size,
        sixel_image_store,
        Rc::new(RefCell::new(KittyImageStore::default())),
        terminal_emulator_colors,
        terminal_emulator_color_codes,
        None,
        None,
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
        None,
    ); // 0 is the pane index
    let mut text_to_fill_pane = String::new();
    for i in 0..5 {
        writeln!(&mut text_to_fill_pane, "\rline {}", i + 1).unwrap();
    }
    writeln!(&mut text_to_fill_pane, "\r").unwrap();
    let pane_sixel_content = read_fixture("sixel-image-500px.six");
    terminal_pane.handle_pty_bytes(pane_sixel_content.clone()); // one image above text
    terminal_pane.handle_pty_bytes(text_to_fill_pane.into_bytes());
    terminal_pane.handle_pty_bytes(pane_sixel_content); // one image below text
    terminal_pane.scroll_up(20, fake_client_id); // scroll up to see both images
    assert_snapshot!(format!("{:?}", terminal_pane.grid));
}

#[test]
pub fn resizing_pane_with_sixel_images() {
    // here we test, for example, that sixel images don't wrap with other lines
    let fake_client_id = 1;
    let mut fake_win_size = PaneGeom::default();
    fake_win_size.cols.set_inner(121);
    fake_win_size.rows.set_inner(20);

    let pid = 1;
    let style = Style::default();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_colors = Rc::new(RefCell::new(Palette::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let character_cell_size = Rc::new(RefCell::new(Some(SizeInPixels {
        width: 8,
        height: 21,
    })));
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let osc8_hyperlinks = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut terminal_pane = TerminalPane::new(
        pid,
        fake_win_size,
        style,
        0,
        String::new(),
        character_cell_size,
        sixel_image_store,
        Rc::new(RefCell::new(KittyImageStore::default())),
        terminal_emulator_colors,
        terminal_emulator_color_codes,
        None,
        None,
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
        None,
    ); // 0 is the pane index
    let mut text_to_fill_pane = String::new();
    for i in 0..5 {
        writeln!(&mut text_to_fill_pane, "\rline {}", i + 1).unwrap();
    }
    writeln!(&mut text_to_fill_pane, "\r").unwrap();
    let pane_sixel_content = read_fixture("sixel-image-500px.six");
    terminal_pane.handle_pty_bytes(pane_sixel_content.clone());
    terminal_pane.handle_pty_bytes(text_to_fill_pane.into_bytes());
    terminal_pane.handle_pty_bytes(pane_sixel_content);
    let mut new_win_size = PaneGeom::default();
    new_win_size.cols.set_inner(100);
    new_win_size.rows.set_inner(20);
    terminal_pane.set_geom(new_win_size);
    terminal_pane.scroll_up(20, fake_client_id); // scroll up to see both images
    assert_snapshot!(format!("{:?}", terminal_pane.grid));
}

#[test]
pub fn changing_character_cell_size_with_sixel_images() {
    let fake_client_id = 1;
    let mut fake_win_size = PaneGeom::default();
    fake_win_size.cols.set_inner(121);
    fake_win_size.rows.set_inner(20);

    let pid = 1;
    let style = Style::default();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_colors = Rc::new(RefCell::new(Palette::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let character_cell_size = Rc::new(RefCell::new(Some(SizeInPixels {
        width: 8,
        height: 21,
    })));
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let osc8_hyperlinks = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut terminal_pane = TerminalPane::new(
        pid,
        fake_win_size,
        style,
        0,
        String::new(),
        character_cell_size.clone(),
        sixel_image_store,
        Rc::new(RefCell::new(KittyImageStore::default())),
        terminal_emulator_colors,
        terminal_emulator_color_codes,
        None,
        None,
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
        None,
    ); // 0 is the pane index
    let mut text_to_fill_pane = String::new();
    for i in 0..5 {
        writeln!(&mut text_to_fill_pane, "\rline {}", i + 1).unwrap();
    }
    writeln!(&mut text_to_fill_pane, "\r").unwrap();
    let pane_sixel_content = read_fixture("sixel-image-500px.six");
    terminal_pane.handle_pty_bytes(pane_sixel_content.clone());
    terminal_pane.handle_pty_bytes(text_to_fill_pane.into_bytes());
    terminal_pane.handle_pty_bytes(pane_sixel_content);
    // here the new_win_size is the same as the old one, we just update the character_cell_size
    // which will be picked up upon resize (which is why we're doing set_geom below)
    let mut new_win_size = PaneGeom::default();
    new_win_size.cols.set_inner(121);
    new_win_size.rows.set_inner(20);
    *character_cell_size.borrow_mut() = Some(SizeInPixels {
        width: 8,
        height: 18,
    });
    terminal_pane.set_geom(new_win_size);
    terminal_pane.scroll_up(10, fake_client_id); // scroll up to see both images
    assert_snapshot!(format!("{:?}", terminal_pane.grid));
}

#[test]
pub fn keep_working_after_corrupted_sixel_image() {
    let mut fake_win_size = PaneGeom::default();
    fake_win_size.cols.set_inner(121);
    fake_win_size.rows.set_inner(20);

    let pid = 1;
    let style = Style::default();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_colors = Rc::new(RefCell::new(Palette::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let character_cell_size = Rc::new(RefCell::new(Some(SizeInPixels {
        width: 8,
        height: 21,
    })));
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let osc8_hyperlinks = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut terminal_pane = TerminalPane::new(
        pid,
        fake_win_size,
        style,
        0,
        String::new(),
        character_cell_size,
        sixel_image_store,
        Rc::new(RefCell::new(KittyImageStore::default())),
        terminal_emulator_colors,
        terminal_emulator_color_codes,
        None,
        None,
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
        None,
    ); // 0 is the pane index

    let sixel_image_bytes = "\u{1b}PI AM CORRUPTED BWAHAHAq
        #0;2;0;0;0#1;2;100;100;0#2;2;0;100;0
        #1~~@@vv@@~~@@~~$
        #2??}}GG}}??}}??-
        #1!14@
        \u{1b}\\";

    terminal_pane.handle_pty_bytes(Vec::from(sixel_image_bytes.as_bytes()));
    let mut text_to_fill_pane = String::new();
    for i in 0..5 {
        writeln!(&mut text_to_fill_pane, "\rline {}", i + 1).unwrap();
    }
    terminal_pane.handle_pty_bytes(text_to_fill_pane.into_bytes());
    assert_snapshot!(format!("{:?}", terminal_pane.grid));
}

#[test]
pub fn pane_with_frame_position_is_on_frame() {
    let mut fake_win_size = PaneGeom {
        x: 10,
        y: 10,
        ..PaneGeom::default()
    };
    fake_win_size.cols.set_inner(121);
    fake_win_size.rows.set_inner(20);

    let pid = 1;
    let style = Style::default();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_colors = Rc::new(RefCell::new(Palette::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let character_cell_size = Rc::new(RefCell::new(Some(SizeInPixels {
        width: 8,
        height: 21,
    })));
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let osc8_hyperlinks = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut terminal_pane = TerminalPane::new(
        pid,
        fake_win_size,
        style,
        0,
        String::new(),
        character_cell_size,
        sixel_image_store,
        Rc::new(RefCell::new(KittyImageStore::default())),
        terminal_emulator_colors,
        terminal_emulator_color_codes,
        None,
        None,
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
        None,
    ); // 0 is the pane index

    terminal_pane.set_content_offset(Offset::frame(1));

    // row above pane: no border
    assert!(!terminal_pane.position_is_on_frame(&Position::new(9, 9)));
    assert!(!terminal_pane.position_is_on_frame(&Position::new(9, 10)));
    assert!(!terminal_pane.position_is_on_frame(&Position::new(9, 11)));
    assert!(!terminal_pane.position_is_on_frame(&Position::new(9, 70)));
    assert!(!terminal_pane.position_is_on_frame(&Position::new(9, 129)));
    assert!(!terminal_pane.position_is_on_frame(&Position::new(9, 130)));
    assert!(!terminal_pane.position_is_on_frame(&Position::new(9, 131)));

    // first row:  border for 10 <= col <= 130
    assert!(!terminal_pane.position_is_on_frame(&Position::new(10, 9)));
    assert!(terminal_pane.position_is_on_frame(&Position::new(10, 10)));
    assert!(terminal_pane.position_is_on_frame(&Position::new(10, 11)));
    assert!(terminal_pane.position_is_on_frame(&Position::new(10, 70)));
    assert!(terminal_pane.position_is_on_frame(&Position::new(10, 129)));
    assert!(terminal_pane.position_is_on_frame(&Position::new(10, 130)));
    assert!(!terminal_pane.position_is_on_frame(&Position::new(10, 131)));

    // second row: border only at col=10,130
    assert!(!terminal_pane.position_is_on_frame(&Position::new(11, 9)));
    assert!(terminal_pane.position_is_on_frame(&Position::new(11, 10)));
    assert!(!terminal_pane.position_is_on_frame(&Position::new(11, 11)));
    assert!(!terminal_pane.position_is_on_frame(&Position::new(11, 70)));
    assert!(terminal_pane.position_is_on_frame(&Position::new(11, 130)));
    assert!(!terminal_pane.position_is_on_frame(&Position::new(11, 131)));

    // row in the middle: border only at col=10,130
    assert!(!terminal_pane.position_is_on_frame(&Position::new(15, 9)));
    assert!(terminal_pane.position_is_on_frame(&Position::new(15, 10)));
    assert!(!terminal_pane.position_is_on_frame(&Position::new(15, 11)));
    assert!(!terminal_pane.position_is_on_frame(&Position::new(15, 70)));
    assert!(terminal_pane.position_is_on_frame(&Position::new(15, 130)));
    assert!(!terminal_pane.position_is_on_frame(&Position::new(15, 131)));

    // last row: border for 10 <= col <= 130
    assert!(!terminal_pane.position_is_on_frame(&Position::new(29, 9)));
    assert!(terminal_pane.position_is_on_frame(&Position::new(29, 10)));
    assert!(terminal_pane.position_is_on_frame(&Position::new(29, 11)));
    assert!(terminal_pane.position_is_on_frame(&Position::new(29, 70)));
    assert!(terminal_pane.position_is_on_frame(&Position::new(29, 130)));
    assert!(!terminal_pane.position_is_on_frame(&Position::new(29, 131)));

    // row below pane: no border
    assert!(!terminal_pane.position_is_on_frame(&Position::new(30, 10)));
    assert!(!terminal_pane.position_is_on_frame(&Position::new(30, 11)));
    assert!(!terminal_pane.position_is_on_frame(&Position::new(30, 70)));
    assert!(!terminal_pane.position_is_on_frame(&Position::new(30, 130)));
    assert!(!terminal_pane.position_is_on_frame(&Position::new(30, 131)));
}

#[test]
pub fn pane_with_bottom_and_right_borders_position_is_on_frame() {
    let mut fake_win_size = PaneGeom {
        x: 10,
        y: 10,
        ..PaneGeom::default()
    };
    fake_win_size.cols.set_inner(121);
    fake_win_size.rows.set_inner(20);

    let pid = 1;
    let style = Style::default();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_colors = Rc::new(RefCell::new(Palette::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let character_cell_size = Rc::new(RefCell::new(Some(SizeInPixels {
        width: 8,
        height: 21,
    })));
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let osc8_hyperlinks = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut terminal_pane = TerminalPane::new(
        pid,
        fake_win_size,
        style,
        0,
        String::new(),
        character_cell_size,
        sixel_image_store,
        Rc::new(RefCell::new(KittyImageStore::default())),
        terminal_emulator_colors,
        terminal_emulator_color_codes,
        None,
        None,
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
        None,
    ); // 0 is the pane index

    terminal_pane.set_content_offset(Offset::shift(1, 1));

    // row above pane: no border
    assert!(!terminal_pane.position_is_on_frame(&Position::new(9, 9)));
    assert!(!terminal_pane.position_is_on_frame(&Position::new(9, 10)));
    assert!(!terminal_pane.position_is_on_frame(&Position::new(9, 11)));
    assert!(!terminal_pane.position_is_on_frame(&Position::new(9, 70)));
    assert!(!terminal_pane.position_is_on_frame(&Position::new(9, 129)));
    assert!(!terminal_pane.position_is_on_frame(&Position::new(9, 130)));
    assert!(!terminal_pane.position_is_on_frame(&Position::new(9, 131)));

    // first row: border only at col=130
    assert!(!terminal_pane.position_is_on_frame(&Position::new(10, 9)));
    assert!(!terminal_pane.position_is_on_frame(&Position::new(10, 10)));
    assert!(!terminal_pane.position_is_on_frame(&Position::new(10, 11)));
    assert!(!terminal_pane.position_is_on_frame(&Position::new(10, 70)));
    assert!(!terminal_pane.position_is_on_frame(&Position::new(10, 129)));
    assert!(terminal_pane.position_is_on_frame(&Position::new(10, 130)));
    assert!(!terminal_pane.position_is_on_frame(&Position::new(10, 131)));

    // second row: border only at col=130
    assert!(!terminal_pane.position_is_on_frame(&Position::new(11, 9)));
    assert!(!terminal_pane.position_is_on_frame(&Position::new(11, 10)));
    assert!(!terminal_pane.position_is_on_frame(&Position::new(11, 11)));
    assert!(!terminal_pane.position_is_on_frame(&Position::new(11, 70)));
    assert!(terminal_pane.position_is_on_frame(&Position::new(11, 130)));
    assert!(!terminal_pane.position_is_on_frame(&Position::new(11, 131)));

    // row in the middle: border only at col=130
    assert!(!terminal_pane.position_is_on_frame(&Position::new(15, 9)));
    assert!(!terminal_pane.position_is_on_frame(&Position::new(15, 10)));
    assert!(!terminal_pane.position_is_on_frame(&Position::new(15, 11)));
    assert!(!terminal_pane.position_is_on_frame(&Position::new(15, 70)));
    assert!(terminal_pane.position_is_on_frame(&Position::new(15, 130)));
    assert!(!terminal_pane.position_is_on_frame(&Position::new(15, 131)));

    // last row: border for 10 <= col <= 130
    assert!(!terminal_pane.position_is_on_frame(&Position::new(29, 9)));
    assert!(terminal_pane.position_is_on_frame(&Position::new(29, 10)));
    assert!(terminal_pane.position_is_on_frame(&Position::new(29, 11)));
    assert!(terminal_pane.position_is_on_frame(&Position::new(29, 70)));
    assert!(terminal_pane.position_is_on_frame(&Position::new(29, 130)));
    assert!(!terminal_pane.position_is_on_frame(&Position::new(29, 131)));

    // row below pane: no border
    assert!(!terminal_pane.position_is_on_frame(&Position::new(30, 10)));
    assert!(!terminal_pane.position_is_on_frame(&Position::new(30, 11)));
    assert!(!terminal_pane.position_is_on_frame(&Position::new(30, 70)));
    assert!(!terminal_pane.position_is_on_frame(&Position::new(30, 130)));
    assert!(!terminal_pane.position_is_on_frame(&Position::new(30, 131)));
}

fn make_terminal_pane_for_bell() -> TerminalPane {
    let mut fake_win_size = PaneGeom::default();
    fake_win_size.cols.set_inner(121);
    fake_win_size.rows.set_inner(20);
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_colors = Rc::new(RefCell::new(Palette::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    TerminalPane::new(
        1,
        fake_win_size,
        Style::default(),
        0,
        String::new(),
        Rc::new(RefCell::new(None)),
        sixel_image_store,
        Rc::new(RefCell::new(KittyImageStore::default())),
        terminal_emulator_colors,
        terminal_emulator_color_codes,
        None,
        None,
        false,
        true,
        true,
        true,
        false,
        None,
    )
}

#[test]
pub fn bell_notification_state_set_and_cleared() {
    let mut terminal_pane = make_terminal_pane_for_bell();

    assert!(
        !terminal_pane.get_bell_notification(),
        "Initially no bell notification"
    );

    terminal_pane.set_bell_notification(true);
    assert!(
        terminal_pane.get_bell_notification(),
        "Bell notification should be set"
    );

    terminal_pane.set_bell_notification(false);
    assert!(
        !terminal_pane.get_bell_notification(),
        "Bell notification should be cleared"
    );
}

#[test]
pub fn has_bell_reflects_grid_ring_bell() {
    let mut terminal_pane = make_terminal_pane_for_bell();

    assert!(
        !terminal_pane.has_bell(),
        "Initially has_bell should be false"
    );

    terminal_pane.handle_pty_bytes(vec![7u8]);
    assert!(
        terminal_pane.has_bell(),
        "has_bell should be true after pty bell byte"
    );

    terminal_pane.consume_bell();
    assert!(
        !terminal_pane.has_bell(),
        "has_bell should be false after consume_bell"
    );
}

#[test]
pub fn osc7_payload_stored_from_pty_bytes() {
    let mut terminal_pane = make_terminal_pane_for_bell();

    assert!(
        terminal_pane.osc7_payload().is_none(),
        "Initially osc7_payload should be None"
    );

    // Send OSC 7 sequence: \x1b]7;file://localhost/tmp\x1b\\
    let osc7_bytes = b"\x1b]7;file://localhost/tmp\x1b\\";
    terminal_pane.handle_pty_bytes(osc7_bytes.to_vec());

    assert_eq!(
        terminal_pane.drain_osc7_cwd(),
        Some(std::path::PathBuf::from("/tmp")),
        "OSC 7 should still update Zellij's internal CWD tracking"
    );
    assert!(terminal_pane.drain_osc7_cwd().is_none());
    assert_eq!(
        terminal_pane.osc7_payload(),
        Some("file://localhost/tmp"),
        "The forwarding URI should persist after the internal CWD update is drained"
    );
}

#[test]
pub fn osc7_payload_updated_on_subsequent_osc7() {
    let mut terminal_pane = make_terminal_pane_for_bell();

    let osc7_first = b"\x1b]7;file://localhost/first\x1b\\";
    terminal_pane.handle_pty_bytes(osc7_first.to_vec());
    assert_eq!(terminal_pane.osc7_payload(), Some("file://localhost/first"));

    let osc7_second = b"\x1b]7;file://localhost/second\x1b\\";
    terminal_pane.handle_pty_bytes(osc7_second.to_vec());
    assert_eq!(
        terminal_pane.osc7_payload(),
        Some("file://localhost/second")
    );
}

#[test]
pub fn osc7_payload_preserves_percent_encoding() {
    let mut terminal_pane = make_terminal_pane_for_bell();

    let osc7_bytes = b"\x1b]7;file://localhost/path%20with%20spaces\x1b\\";
    terminal_pane.handle_pty_bytes(osc7_bytes.to_vec());
    assert_eq!(
        terminal_pane.drain_osc7_cwd(),
        Some(std::path::PathBuf::from("/path with spaces")),
    );
    assert_eq!(
        terminal_pane.osc7_payload(),
        Some("file://localhost/path%20with%20spaces"),
    );
}

#[test]
pub fn osc7_payload_works_with_bel_terminator() {
    let mut terminal_pane = make_terminal_pane_for_bell();

    // BEL (\x07) is an alternative terminator for OSC sequences
    let osc7_bytes = b"\x1b]7;file://localhost/tmp\x07";
    terminal_pane.handle_pty_bytes(osc7_bytes.to_vec());
    assert_eq!(terminal_pane.osc7_payload(), Some("file://localhost/tmp"),);
}

#[test]
pub fn osc7_payload_rejects_c0_control_characters() {
    // Test the Grid's osc_dispatch directly, since the VTE parser strips
    // control characters before they reach osc_dispatch. This validates
    // the defense-in-depth guard in the b"7" handler.
    use crate::vt_parser::Handler;

    let mut terminal_pane = make_terminal_pane_for_bell();
    let params: &[&[u8]] = &[b"7", b"file://host/path\x01bad"];
    terminal_pane.grid.osc_end(params, false);
    assert!(
        terminal_pane.osc7_payload().is_none(),
        "URIs with C0 control characters should be rejected"
    );
}

#[test]
pub fn osc7_payload_rejects_c1_control_characters() {
    // C1 control characters (U+0080..U+009F) include ST (U+009C) which
    // could terminate the OSC sequence in the parent terminal.
    use crate::vt_parser::Handler;

    let mut terminal_pane = make_terminal_pane_for_bell();
    // U+009C (ST) in UTF-8 is 0xC2 0x9C
    let params: &[&[u8]] = &[b"7", "file://host/path\u{9c}bad".as_bytes()];
    terminal_pane.grid.osc_end(params, false);
    assert!(
        terminal_pane.osc7_payload().is_none(),
        "URIs with C1 control characters should be rejected"
    );
}

#[test]
pub fn osc7_payload_rejects_an_empty_payload() {
    let mut terminal_pane = make_terminal_pane_for_bell();

    terminal_pane.handle_pty_bytes(b"\x1b]7;file://localhost/tmp\x1b\\".to_vec());
    terminal_pane.handle_pty_bytes(b"\x1b]7;\x1b\\".to_vec());

    assert_eq!(
        terminal_pane.osc7_payload(),
        Some("file://localhost/tmp"),
        "An empty OSC 7 is rejected and the previous URI is retained"
    );
}

#[test]
pub fn osc7_payload_rejects_invalid_utf8() {
    use crate::vt_parser::Handler;

    let mut terminal_pane = make_terminal_pane_for_bell();
    terminal_pane.handle_pty_bytes(b"\x1b]7;file://localhost/tmp\x1b\\".to_vec());

    let params: &[&[u8]] = &[b"7", b"file://localhost/\xff\xfe"];
    terminal_pane.grid.osc_end(params, false);

    assert_eq!(
        terminal_pane.osc7_payload(),
        Some("file://localhost/tmp"),
        "A non-UTF-8 OSC 7 is rejected and the previous URI is retained"
    );
}

#[test]
pub fn osc7_payload_joins_semicolon_separated_segments() {
    let mut terminal_pane = make_terminal_pane_for_bell();

    terminal_pane.handle_pty_bytes(b"\x1b]7;file://localhost/a;b;c\x1b\\".to_vec());

    assert_eq!(
        terminal_pane.osc7_payload(),
        Some("file://localhost/a;b;c"),
        "A URI containing semicolons is reassembled from the split params"
    );
}

#[test]
pub fn osc7_payload_is_cleared_by_a_terminal_reset() {
    let mut terminal_pane = make_terminal_pane_for_bell();

    terminal_pane.handle_pty_bytes(b"\x1b]7;file://localhost/tmp\x1b\\".to_vec());
    assert_eq!(terminal_pane.osc7_payload(), Some("file://localhost/tmp"));

    terminal_pane.handle_pty_bytes(b"\x1bc".to_vec());

    assert!(
        terminal_pane.osc7_payload().is_none(),
        "RIS clears the reported working directory"
    );
}

#[test]
pub fn frameless_pane_position_is_on_frame() {
    let mut fake_win_size = PaneGeom {
        x: 10,
        y: 10,
        ..PaneGeom::default()
    };
    fake_win_size.cols.set_inner(121);
    fake_win_size.rows.set_inner(20);

    let pid = 1;
    let style = Style::default();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_colors = Rc::new(RefCell::new(Palette::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let character_cell_size = Rc::new(RefCell::new(Some(SizeInPixels {
        width: 8,
        height: 21,
    })));
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let osc8_hyperlinks = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut terminal_pane = TerminalPane::new(
        pid,
        fake_win_size,
        style,
        0,
        String::new(),
        character_cell_size,
        sixel_image_store,
        Rc::new(RefCell::new(KittyImageStore::default())),
        terminal_emulator_colors,
        terminal_emulator_color_codes,
        None,
        None,
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
        None,
    ); // 0 is the pane index

    terminal_pane.set_content_offset(Offset::default());

    // row above pane: no border
    assert!(!terminal_pane.position_is_on_frame(&Position::new(9, 9)));
    assert!(!terminal_pane.position_is_on_frame(&Position::new(9, 10)));
    assert!(!terminal_pane.position_is_on_frame(&Position::new(9, 11)));
    assert!(!terminal_pane.position_is_on_frame(&Position::new(9, 70)));
    assert!(!terminal_pane.position_is_on_frame(&Position::new(9, 129)));
    assert!(!terminal_pane.position_is_on_frame(&Position::new(9, 130)));
    assert!(!terminal_pane.position_is_on_frame(&Position::new(9, 131)));

    // first row: no border
    assert!(!terminal_pane.position_is_on_frame(&Position::new(10, 9)));
    assert!(!terminal_pane.position_is_on_frame(&Position::new(10, 10)));
    assert!(!terminal_pane.position_is_on_frame(&Position::new(10, 11)));
    assert!(!terminal_pane.position_is_on_frame(&Position::new(10, 70)));
    assert!(!terminal_pane.position_is_on_frame(&Position::new(10, 129)));
    assert!(!terminal_pane.position_is_on_frame(&Position::new(10, 130)));
    assert!(!terminal_pane.position_is_on_frame(&Position::new(10, 131)));

    // second row: no border
    assert!(!terminal_pane.position_is_on_frame(&Position::new(11, 9)));
    assert!(!terminal_pane.position_is_on_frame(&Position::new(11, 10)));
    assert!(!terminal_pane.position_is_on_frame(&Position::new(11, 11)));
    assert!(!terminal_pane.position_is_on_frame(&Position::new(11, 70)));
    assert!(!terminal_pane.position_is_on_frame(&Position::new(11, 130)));
    assert!(!terminal_pane.position_is_on_frame(&Position::new(11, 131)));

    // random row in the middle: no border
    assert!(!terminal_pane.position_is_on_frame(&Position::new(15, 9)));
    assert!(!terminal_pane.position_is_on_frame(&Position::new(15, 10)));
    assert!(!terminal_pane.position_is_on_frame(&Position::new(15, 11)));
    assert!(!terminal_pane.position_is_on_frame(&Position::new(15, 70)));
    assert!(!terminal_pane.position_is_on_frame(&Position::new(15, 130)));
    assert!(!terminal_pane.position_is_on_frame(&Position::new(15, 131)));

    // last row: no border
    assert!(!terminal_pane.position_is_on_frame(&Position::new(29, 9)));
    assert!(!terminal_pane.position_is_on_frame(&Position::new(29, 10)));
    assert!(!terminal_pane.position_is_on_frame(&Position::new(29, 11)));
    assert!(!terminal_pane.position_is_on_frame(&Position::new(29, 70)));
    assert!(!terminal_pane.position_is_on_frame(&Position::new(29, 130)));
    assert!(!terminal_pane.position_is_on_frame(&Position::new(29, 131)));

    // row below pane: no border
    assert!(!terminal_pane.position_is_on_frame(&Position::new(30, 10)));
    assert!(!terminal_pane.position_is_on_frame(&Position::new(30, 11)));
    assert!(!terminal_pane.position_is_on_frame(&Position::new(30, 70)));
    assert!(!terminal_pane.position_is_on_frame(&Position::new(30, 130)));
    assert!(!terminal_pane.position_is_on_frame(&Position::new(30, 131)));
}

fn create_guest_modal_pane() -> TerminalPane {
    let mut fake_win_size = PaneGeom::default();
    fake_win_size.cols.set_inner(80);
    fake_win_size.rows.set_inner(24);
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_colors = Rc::new(RefCell::new(Palette::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    TerminalPane::new(
        1,
        fake_win_size,
        Style::default(),
        0,
        String::new(),
        Rc::new(RefCell::new(None)),
        sixel_image_store,
        Rc::new(RefCell::new(KittyImageStore::default())),
        terminal_emulator_colors,
        terminal_emulator_color_codes,
        None,
        None,
        false,
        true,
        true,
        true,
        false,
        None,
    )
}

fn press_key(
    pane: &mut TerminalPane,
    bare_key: zellij_utils::data::BareKey,
    client_id: crate::ClientId,
) -> Option<crate::tab::AdjustedInput> {
    let key = zellij_utils::data::KeyWithModifier::new(bare_key);
    pane.adjust_input_to_terminal(&Some(key), vec![], false, Some(client_id))
}

#[test]
pub fn guest_modal_navigation_wraps() {
    use zellij_utils::data::BareKey;
    let mut pane = create_guest_modal_pane();
    let client_id = 1;
    pane.set_guest_modal(&[client_id]);
    assert_eq!(pane.guest_modal_selection(client_id), Some(0));
    press_key(&mut pane, BareKey::Up, client_id);
    assert_eq!(pane.guest_modal_selection(client_id), Some(1));
    press_key(&mut pane, BareKey::Down, client_id);
    assert_eq!(pane.guest_modal_selection(client_id), Some(0));
    press_key(&mut pane, BareKey::Down, client_id);
    assert_eq!(pane.guest_modal_selection(client_id), Some(1));
    press_key(&mut pane, BareKey::Up, client_id);
    assert_eq!(pane.guest_modal_selection(client_id), Some(0));
}

#[test]
pub fn guest_modal_tab_is_ignored() {
    use zellij_utils::data::{BareKey, KeyWithModifier};
    let mut pane = create_guest_modal_pane();
    let client_id = 1;
    pane.set_guest_modal(&[client_id]);
    press_key(&mut pane, BareKey::Tab, client_id);
    assert_eq!(pane.guest_modal_selection(client_id), Some(0));
    let key = KeyWithModifier::new(BareKey::Tab).with_shift_modifier();
    pane.adjust_input_to_terminal(&Some(key), vec![], false, Some(client_id));
    assert_eq!(pane.guest_modal_selection(client_id), Some(0));
}

#[test]
pub fn guest_modal_enter_confirms_selection() {
    use crate::tab::AdjustedInput;
    use zellij_utils::data::BareKey;
    let mut pane = create_guest_modal_pane();
    let client_id = 1;

    pane.set_guest_modal(&[client_id]);
    let outcome = press_key(&mut pane, BareKey::Enter, client_id);
    assert!(matches!(outcome, Some(AdjustedInput::GuestModalZoom)));

    pane.set_guest_modal(&[client_id]);
    press_key(&mut pane, BareKey::Down, client_id);
    let outcome = press_key(&mut pane, BareKey::Enter, client_id);
    assert!(matches!(outcome, Some(AdjustedInput::GuestModalDescend)));
}

#[test]
pub fn guest_modal_digit_shortcuts() {
    use crate::tab::AdjustedInput;
    use zellij_utils::data::BareKey;
    let mut pane = create_guest_modal_pane();
    let client_id = 1;

    pane.set_guest_modal(&[client_id]);
    let outcome = press_key(&mut pane, BareKey::Char('1'), client_id);
    assert!(matches!(outcome, Some(AdjustedInput::GuestModalZoom)));

    pane.set_guest_modal(&[client_id]);
    let outcome = press_key(&mut pane, BareKey::Char('2'), client_id);
    assert!(matches!(outcome, Some(AdjustedInput::GuestModalDescend)));
}

#[test]
pub fn guest_modal_esc_descends() {
    use crate::tab::AdjustedInput;
    use zellij_utils::data::BareKey;
    let mut pane = create_guest_modal_pane();
    let client_id = 1;
    pane.set_guest_modal(&[client_id]);
    let outcome = press_key(&mut pane, BareKey::Esc, client_id);
    assert!(matches!(outcome, Some(AdjustedInput::GuestModalDescend)));
}

#[test]
pub fn guest_modal_swallows_other_input() {
    use zellij_utils::data::BareKey;
    let mut pane = create_guest_modal_pane();
    let client_id = 1;
    pane.set_guest_modal(&[client_id]);
    let outcome = press_key(&mut pane, BareKey::Char('x'), client_id);
    assert!(outcome.is_none());
    assert_eq!(pane.guest_modal_selection(client_id), Some(0));
}

#[test]
pub fn guest_modal_is_per_client() {
    use zellij_utils::data::BareKey;
    let mut pane = create_guest_modal_pane();
    let client_a = 1;
    let client_b = 2;
    pane.set_guest_modal(&[client_a, client_b]);
    press_key(&mut pane, BareKey::Down, client_a);
    assert_eq!(pane.guest_modal_selection(client_a), Some(1));
    assert_eq!(pane.guest_modal_selection(client_b), Some(0));
}

#[test]
pub fn guest_modal_no_modal_passes_input_through() {
    use zellij_utils::data::BareKey;
    let mut pane = create_guest_modal_pane();
    let client_id = 1;
    let outcome = press_key(&mut pane, BareKey::Char('x'), client_id);
    assert!(matches!(
        outcome,
        Some(crate::tab::AdjustedInput::WriteBytesToTerminal(_))
    ));
}

fn mirror_source_pane(cols: usize, rows: usize) -> TerminalPane {
    let mut geom = PaneGeom::default();
    geom.cols.set_inner(cols);
    geom.rows.set_inner(rows);
    TerminalPane::new(
        1,
        geom,
        Style::default(),
        0,
        String::new(),
        Rc::new(RefCell::new(None)),
        Rc::new(RefCell::new(SixelImageStore::default())),
        Rc::new(RefCell::new(KittyImageStore::default())),
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
    )
}

#[test]
pub fn a_mirror_snapshot_carries_styles_wide_characters_links_and_the_cursor() {
    use zellij_utils::structured_render::ATTR_BOLD;
    let mut pane = mirror_source_pane(20, 4);
    pane.handle_pty_bytes(
        "\u{1b}[1mbold\u{1b}[0m 中 \u{1b}]8;;https://zellij.dev\u{1b}\\link\u{1b}]8;;\u{1b}\\\r\nnext"
            .as_bytes()
            .to_vec(),
    );
    let screen = pane
        .mirror_screen()
        .expect("a terminal pane can be mirrored");
    assert_eq!((screen.cols, screen.rows), (20, 4));
    let bold = screen.cell(0, 0).unwrap();
    assert_eq!(bold.cell.ch, 'b' as u32);
    assert!(bold.cell.has(ATTR_BOLD));
    assert!(!screen.cell(5, 0).unwrap().cell.has(ATTR_BOLD));
    let wide = screen.cell(5, 0).unwrap();
    assert_eq!(wide.cell.ch, '中' as u32);
    assert_eq!(wide.cell.width, 2);
    let link = screen.cell(8, 0).unwrap();
    assert_eq!(link.cell.ch, 'l' as u32);
    assert_eq!(link.link.as_deref(), Some("https://zellij.dev"));
    assert_eq!(screen.cell(0, 1).unwrap().cell.ch, 'n' as u32);
    assert_eq!((screen.cursor.x, screen.cursor.y), (4, 1));
    assert!(screen.cursor.visible);
    assert!(!screen.wants_mouse);
}

#[test]
pub fn a_mirror_snapshot_reports_mouse_tracking_and_survives_encoding() {
    use zellij_utils::pane_mirror::{MirrorFrameEncoder, MirrorScreen};
    let mut pane = mirror_source_pane(10, 3);
    pane.handle_pty_bytes(b"\x1b[?1000hone\r\ntwo".to_vec());
    let first = pane.mirror_screen().unwrap();
    assert!(first.wants_mouse);
    let mut encoder = MirrorFrameEncoder::new();
    let full = encoder.encode(&first).unwrap();
    assert!(full.full);
    let mut viewer = MirrorScreen::from_frame(&full.bytes).unwrap();
    assert_eq!(viewer, first);
    pane.handle_pty_bytes(b"\r\nthree".to_vec());
    let second = pane.mirror_screen().unwrap();
    let delta = encoder.encode(&second).unwrap();
    assert!(!delta.full);
    let applied = viewer.apply_frame(&delta.bytes).unwrap();
    assert_eq!(
        applied.changed_rows.into_iter().collect::<Vec<_>>(),
        vec![2]
    );
    assert_eq!(viewer, second);
    assert_eq!(encoder.encode(&pane.mirror_screen().unwrap()), None);
}

fn rendered_chunks(pane: &mut TerminalPane) -> Vec<crate::output::CharacterChunk> {
    pane.set_should_render(true);
    pane.render(None)
        .unwrap()
        .map(|(chunks, _, _, _)| chunks)
        .unwrap_or_default()
}

fn chunk_text(chunk: &crate::output::CharacterChunk) -> String {
    chunk
        .terminal_characters
        .iter()
        .map(|character| character.character)
        .collect()
}

fn chunk_width(chunk: &crate::output::CharacterChunk) -> usize {
    chunk
        .terminal_characters
        .iter()
        .map(|character| character.width())
        .sum()
}

fn pinned(cols: usize, rows: usize) -> Option<zellij_utils::pane_size::Size> {
    Some(zellij_utils::pane_size::Size { cols, rows })
}

#[test]
pub fn a_collapsed_stack_program_size_keeps_pty_nonzero() {
    let mut pane = mirror_source_pane(20, 1);
    assert_eq!((pane.program_columns(), pane.program_rows()), (20, 1));
    pane.set_collapsed_stack_program_size(Some(zellij_utils::pane_size::Size {
        cols: 18,
        rows: 12,
    }));
    assert_eq!((pane.program_columns(), pane.program_rows()), (18, 12));
    assert_eq!(
        (pane.get_content_columns(), pane.get_content_rows()),
        (20, 1),
        "visual content stays the one-liner title row"
    );
    pane.set_collapsed_stack_program_size(None);
    assert_eq!((pane.program_columns(), pane.program_rows()), (20, 1));
}

#[test]
pub fn a_pinned_pane_gives_its_program_the_pinned_size_until_released() {
    let mut pane = mirror_source_pane(20, 4);
    assert_eq!((pane.program_columns(), pane.program_rows()), (20, 4));
    assert!(pane.set_mirror_pinned_size(pinned(10, 2)));
    assert!(!pane.set_mirror_pinned_size(pinned(10, 2)));
    assert_eq!((pane.program_columns(), pane.program_rows()), (10, 2));
    assert_eq!(
        (pane.get_content_columns(), pane.get_content_rows()),
        (20, 4)
    );
    let screen = pane.mirror_screen().unwrap();
    assert_eq!((screen.cols, screen.rows), (10, 2));
    let mut geom = pane.position_and_size();
    geom.rows.set_inner(1);
    pane.set_geom(geom);
    assert_eq!(
        (pane.program_columns(), pane.program_rows()),
        (10, 2),
        "a layout change does not undo the pin, even when the pane collapses"
    );
    assert_eq!(pane.mirror_screen().unwrap().rows, 2);
    assert!(pane.set_mirror_pinned_size(None));
    assert_eq!((pane.program_columns(), pane.program_rows()), (20, 1));
    assert_eq!(pane.mirror_screen().unwrap().cols, 20);
}

#[test]
pub fn a_pinned_program_larger_than_its_pane_is_cropped() {
    let mut pane = mirror_source_pane(10, 3);
    pane.set_mirror_pinned_size(pinned(20, 5));
    pane.handle_pty_bytes(b"abcdefghijklmnopqrst\r\nline two\r\n\r\n\r\nfifth".to_vec());
    let chunks = rendered_chunks(&mut pane);
    assert!(!chunks.is_empty());
    for chunk in &chunks {
        assert!(chunk.y < 3, "a row below the pane was drawn: {}", chunk.y);
        assert!(chunk.x + chunk_width(chunk) <= 10, "a chunk is too wide");
    }
    let first_row = chunks.iter().find(|chunk| chunk.y == 0).unwrap();
    assert_eq!(chunk_text(first_row), "abcdefghij");
    assert!(!chunks
        .iter()
        .any(|chunk| chunk_text(chunk).contains("fifth")));
}

#[test]
pub fn a_wide_character_on_the_crop_edge_becomes_a_blank() {
    let mut pane = mirror_source_pane(5, 2);
    pane.set_mirror_pinned_size(pinned(10, 2));
    pane.handle_pty_bytes("abcd中ef".as_bytes().to_vec());
    let chunks = rendered_chunks(&mut pane);
    let first_row = chunks.iter().find(|chunk| chunk.y == 0).unwrap();
    assert_eq!(chunk_text(first_row), "abcd ");
    assert_eq!(chunk_width(first_row), 5);
}

#[test]
pub fn a_pinned_program_smaller_than_its_pane_is_padded_on_full_redraws() {
    let mut pane = mirror_source_pane(10, 4);
    pane.set_mirror_pinned_size(pinned(6, 2));
    pane.handle_pty_bytes(b"left".to_vec());
    let chunks = rendered_chunks(&mut pane);
    for y in 0..2 {
        assert!(
            chunks
                .iter()
                .any(|chunk| chunk.y == y && chunk.x == 6 && chunk_width(chunk) == 4),
            "the right side of row {} is not padded",
            y
        );
    }
    for y in 2..4 {
        assert!(
            chunks
                .iter()
                .any(|chunk| chunk.y == y && chunk.x == 0 && chunk_width(chunk) == 10),
            "row {} below the program is not padded",
            y
        );
    }
    pane.handle_pty_bytes(b"!".to_vec());
    let partial = rendered_chunks(&mut pane);
    assert!(partial.iter().all(|chunk| chunk.y < 2 && chunk.x < 6));
}

#[test]
pub fn the_cursor_of_a_cropped_pane_is_hidden_outside_the_visible_area() {
    let mut pane = mirror_source_pane(10, 3);
    pane.set_mirror_pinned_size(pinned(20, 5));
    pane.handle_pty_bytes(b"\x1b[1;16H".to_vec());
    assert_eq!(Pane::cursor_coordinates(&pane, None), None);
    pane.handle_pty_bytes(b"\x1b[5;2H".to_vec());
    assert_eq!(Pane::cursor_coordinates(&pane, None), None);
    pane.handle_pty_bytes(b"\x1b[2;3H".to_vec());
    assert_eq!(Pane::cursor_coordinates(&pane, None), Some((2, 1, true)));
    pane.set_mirror_pinned_size(None);
    pane.handle_pty_bytes(b"\x1b[3;8H".to_vec());
    assert_eq!(Pane::cursor_coordinates(&pane, None), Some((7, 2, true)));
}

#[test]
pub fn the_change_counter_moves_only_when_the_pane_changes() {
    let mut pane = mirror_source_pane(10, 3);
    let before = pane.mirror_change_counter();
    assert_eq!(pane.mirror_change_counter(), before);
    let _ = pane.mirror_screen();
    let _ = rendered_chunks(&mut pane);
    let after_render = pane.mirror_change_counter();
    let _ = pane.render(None);
    assert_eq!(pane.mirror_change_counter(), after_render);
    pane.handle_pty_bytes(b"x".to_vec());
    let after_output = pane.mirror_change_counter();
    assert_ne!(after_output, after_render);
    pane.set_mirror_pinned_size(pinned(5, 2));
    assert_ne!(pane.mirror_change_counter(), after_output);
}

fn pane_for_split_test() -> TerminalPane {
    let mut fake_win_size = PaneGeom::default();
    fake_win_size.cols.set_inner(40);
    fake_win_size.rows.set_inner(10);
    TerminalPane::new(
        1,
        fake_win_size,
        Style::default(),
        0,
        String::new(),
        Rc::new(RefCell::new(None)),
        Rc::new(RefCell::new(SixelImageStore::default())),
        Rc::new(RefCell::new(KittyImageStore::default())),
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
    )
}

#[test]
fn output_split_at_any_byte_renders_the_same_as_output_in_one_piece() {
    let stream: &[u8] = b"\x1b[31mred\x1b[0m plain \x1b_Gi=1,a=q;AAAA\x1b\\after\x1b[1;32mgreen\x1b[m\r\n\x1b[4mund\x1b[24m end\x1b";
    let mut whole = pane_for_split_test();
    whole.handle_pty_bytes(stream.to_vec());
    whole.handle_pty_bytes(b"[7mrev".to_vec());
    let expected = format!("{:?}", whole.grid);
    for split in 1..stream.len() {
        let mut pane = pane_for_split_test();
        pane.handle_pty_bytes(stream[..split].to_vec());
        pane.handle_pty_bytes(stream[split..].to_vec());
        pane.handle_pty_bytes(b"[7mrev".to_vec());
        assert_eq!(
            format!("{:?}", pane.grid),
            expected,
            "split at byte {}",
            split
        );
    }
}

#[test]
fn pane_frame_color_override_reports_change_only_on_real_delta() {
    let mut pane = pane_for_split_test();
    assert!(pane.add_red_pane_frame_color_override(Some("err".into())));
    // the same request applied again is a no-op and must not render
    assert!(!pane.add_red_pane_frame_color_override(Some("err".into())));
    assert!(pane.add_highlight_pane_frame_color_override(Some("hi".into()), None));
    assert!(!pane.add_highlight_pane_frame_color_override(Some("hi".into()), None));
    assert!(pane.clear_pane_frame_color_override(None));
    // clearing an already-clear pane is also a no-op
    assert!(!pane.clear_pane_frame_color_override(None));
}

#[test]
fn truncation_is_reported_on_the_title_and_not_as_terminal_bytes() {
    let mut pane = pane_for_split_test();
    pane.pane_name = "shell".to_owned();
    pane.note_output_truncated();
    pane.note_output_truncated();
    let title = pane.current_title();
    assert_eq!(title.matches(" [output cut off]").count(), 1, "{title}");
    assert!(title.starts_with("shell"), "{title}");
    pane.grid.set_history_truncated_for_test(true);
    let title = pane.current_title();
    assert!(title.contains(" [history cut off]"), "{title}");
    assert_eq!(title.matches(" [output cut off]").count(), 1, "{title}");
}

#[test]
fn discarding_a_pre_open_gap_drops_held_escape_and_kitty_apc_state() {
    let mut pane = pane_for_split_test();
    pane.handle_pty_bytes(b"prefix\x1b".to_vec());
    pane.discard_unfinished_input();
    pane.handle_pty_bytes(b"]0;TITLE\x07VISIBLE".to_vec());
    assert_eq!(pane.grid.title, None);
    assert!(
        pane.grid.dump_screen(false).contains("]0;TITLE"),
        "the held escape completed an OSC across the gap"
    );

    pane.handle_pty_bytes(b"\x1b_G".to_vec());
    assert!(!pane.kitty_interceptor.is_ground());
    pane.pending_pty_input = b"prefix-leftover".to_vec();
    pane.pending_pty_offset = 6;
    pane.discard_unfinished_input();
    assert!(
        pane.kitty_interceptor.is_ground(),
        "an unfinished kitty APC survived the gap"
    );
    assert!(pane.pending_pty_input.is_empty());
    assert_eq!(pane.pending_pty_offset, 0);
}

#[test]
fn resetting_the_grid_cancels_output_catchup() {
    let mut pane = pane_for_split_test();
    pane.output_catchup = Some(super::OutputCatchup {
        anchor: pane.grid.viewport_anchor(),
        phase: super::CatchupPhase::Draining,
        restore_in_turn: false,
    });
    pane.pending_pty_input = b"stale input".to_vec();
    pane.pending_pty_offset = 3;
    assert!(pane.has_deferred_pane_work());
    pane.clear_screen();
    assert!(pane.output_catchup.is_none());
    assert!(pane.pending_pty_input.is_empty());
    assert_eq!(pane.pending_pty_offset, 0);
    assert!(!pane.has_deferred_pane_work());
}

#[test]
fn an_idle_catchup_at_a_deep_scroll_settles_after_backlog_rejection() {
    let mut pane = terminal_pane_with_size(3, 20);
    for _ in 0..100 {
        pane.handle_pty_bytes_for_test(b"history\r\n".to_vec());
    }
    pane.scroll_up(80, 1);
    assert!(pane.is_scrolled());
    pane.grid.lock_renders();
    pane.output_catchup = Some(super::OutputCatchup {
        anchor: pane.grid.viewport_anchor(),
        phase: super::CatchupPhase::ToBottom,
        restore_in_turn: false,
    });

    pane.reject_unparsed_input();
    let mut steps = 0;
    while pane.output_catchup.is_some() {
        pane.advance_deferred_pane_work();
        steps += 1;
        assert!(steps < 1_000, "catch-up did not settle after backlog loss");
    }
    assert!(!pane.has_deferred_pane_work());
}

#[test]
fn resetting_the_grid_drops_output_buffered_behind_a_forward_pause() {
    let mut pane = pane_for_split_test();
    pane.arm_forward_pause();
    pane.handle_pty_bytes(b"stale output from the old run".to_vec());
    assert!(pane.is_forward_paused());
    assert!(pane.has_pending_pty_input());

    pane.reset_grid_state();
    assert!(!pane.is_forward_paused());
    assert!(!pane.has_pending_pty_input());
    assert_eq!(pane.pending_bytes_appended(), 0);

    pane.handle_pty_bytes(b"fresh output".to_vec());
    let screen = pane.grid.dump_screen(false);
    assert!(screen.contains("fresh output"), "{screen}");
    assert!(!screen.contains("stale output"), "{screen}");
}
