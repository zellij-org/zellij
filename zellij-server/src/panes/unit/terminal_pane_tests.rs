use super::super::TerminalPane;
use crate::panes::sixel::SixelImageStore;
use crate::panes::LinkHandler;
use crate::tab::Pane;
use ::insta::assert_snapshot;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use zellij_utils::{
    data::{Palette, Style},
    pane_size::{Offset, PaneGeom, SizeInPixels},
    position::Position,
};

use std::fmt::Write;

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
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut terminal_pane = TerminalPane::new(
        pid,
        fake_win_size,
        style,
        0,
        String::new(),
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(None)),
        sixel_image_store,
        terminal_emulator_colors,
        terminal_emulator_color_codes,
        None,
        None,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
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
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut terminal_pane = TerminalPane::new(
        pid,
        fake_win_size,
        style,
        0,
        String::new(),
        Rc::new(RefCell::new(LinkHandler::new())),
        character_cell_size,
        sixel_image_store,
        terminal_emulator_colors,
        terminal_emulator_color_codes,
        None,
        None,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
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
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut terminal_pane = TerminalPane::new(
        pid,
        fake_win_size,
        style,
        0,
        String::new(),
        Rc::new(RefCell::new(LinkHandler::new())),
        character_cell_size,
        sixel_image_store,
        terminal_emulator_colors,
        terminal_emulator_color_codes,
        None,
        None,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
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
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut terminal_pane = TerminalPane::new(
        pid,
        fake_win_size,
        style,
        0,
        String::new(),
        Rc::new(RefCell::new(LinkHandler::new())),
        character_cell_size,
        sixel_image_store,
        terminal_emulator_colors,
        terminal_emulator_color_codes,
        None,
        None,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
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
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut terminal_pane = TerminalPane::new(
        pid,
        fake_win_size,
        style,
        0,
        String::new(),
        Rc::new(RefCell::new(LinkHandler::new())),
        character_cell_size,
        sixel_image_store,
        terminal_emulator_colors,
        terminal_emulator_color_codes,
        None,
        None,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
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
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut terminal_pane = TerminalPane::new(
        pid,
        fake_win_size,
        style,
        0,
        String::new(),
        Rc::new(RefCell::new(LinkHandler::new())),
        character_cell_size,
        sixel_image_store,
        terminal_emulator_colors,
        terminal_emulator_color_codes,
        None,
        None,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
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
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut terminal_pane = TerminalPane::new(
        pid,
        fake_win_size,
        style,
        0,
        String::new(),
        Rc::new(RefCell::new(LinkHandler::new())),
        character_cell_size,
        sixel_image_store,
        terminal_emulator_colors,
        terminal_emulator_color_codes,
        None,
        None,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
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
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut terminal_pane = TerminalPane::new(
        pid,
        fake_win_size,
        style,
        0,
        String::new(),
        Rc::new(RefCell::new(LinkHandler::new())),
        character_cell_size.clone(),
        sixel_image_store,
        terminal_emulator_colors,
        terminal_emulator_color_codes,
        None,
        None,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
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
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut terminal_pane = TerminalPane::new(
        pid,
        fake_win_size,
        style,
        0,
        String::new(),
        Rc::new(RefCell::new(LinkHandler::new())),
        character_cell_size,
        sixel_image_store,
        terminal_emulator_colors,
        terminal_emulator_color_codes,
        None,
        None,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
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
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut terminal_pane = TerminalPane::new(
        pid,
        fake_win_size,
        style,
        0,
        String::new(),
        Rc::new(RefCell::new(LinkHandler::new())),
        character_cell_size,
        sixel_image_store,
        terminal_emulator_colors,
        terminal_emulator_color_codes,
        None,
        None,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
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
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut terminal_pane = TerminalPane::new(
        pid,
        fake_win_size,
        style,
        0,
        String::new(),
        Rc::new(RefCell::new(LinkHandler::new())),
        character_cell_size,
        sixel_image_store,
        terminal_emulator_colors,
        terminal_emulator_color_codes,
        None,
        None,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
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
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut terminal_pane = TerminalPane::new(
        pid,
        fake_win_size,
        style,
        0,
        String::new(),
        Rc::new(RefCell::new(LinkHandler::new())),
        character_cell_size,
        sixel_image_store,
        terminal_emulator_colors,
        terminal_emulator_color_codes,
        None,
        None,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
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
