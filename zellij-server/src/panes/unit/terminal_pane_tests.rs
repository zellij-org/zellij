use super::super::TerminalPane;
use crate::panes::sixel::SixelImageStore;
use crate::panes::LinkHandler;
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
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(None)),
        sixel_image_store,
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
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(None)),
        sixel_image_store,
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
    client_id: u16,
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
    assert_eq!(pane.guest_modal_selection(client_id), Some(2));
    press_key(&mut pane, BareKey::Down, client_id);
    assert_eq!(pane.guest_modal_selection(client_id), Some(0));
    press_key(&mut pane, BareKey::Down, client_id);
    assert_eq!(pane.guest_modal_selection(client_id), Some(1));
    press_key(&mut pane, BareKey::Tab, client_id);
    assert_eq!(pane.guest_modal_selection(client_id), Some(2));
    press_key(&mut pane, BareKey::Tab, client_id);
    assert_eq!(pane.guest_modal_selection(client_id), Some(0));
}

#[test]
pub fn guest_modal_shift_tab_moves_up() {
    use zellij_utils::data::{BareKey, KeyWithModifier};
    let mut pane = create_guest_modal_pane();
    let client_id = 1;
    pane.set_guest_modal(&[client_id]);
    let key = KeyWithModifier::new(BareKey::Tab).with_shift_modifier();
    pane.adjust_input_to_terminal(&Some(key), vec![], false, Some(client_id));
    assert_eq!(pane.guest_modal_selection(client_id), Some(2));
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

    pane.set_guest_modal(&[client_id]);
    press_key(&mut pane, BareKey::Down, client_id);
    press_key(&mut pane, BareKey::Down, client_id);
    let outcome = press_key(&mut pane, BareKey::Enter, client_id);
    assert!(matches!(outcome, Some(AdjustedInput::GuestModalDismiss)));
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

    pane.set_guest_modal(&[client_id]);
    let outcome = press_key(&mut pane, BareKey::Char('3'), client_id);
    assert!(matches!(outcome, Some(AdjustedInput::GuestModalDismiss)));
}

#[test]
pub fn guest_modal_esc_dismisses() {
    use crate::tab::AdjustedInput;
    use zellij_utils::data::BareKey;
    let mut pane = create_guest_modal_pane();
    let client_id = 1;
    pane.set_guest_modal(&[client_id]);
    let outcome = press_key(&mut pane, BareKey::Esc, client_id);
    assert!(matches!(outcome, Some(AdjustedInput::GuestModalDismiss)));
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

#[test]
pub fn guest_modal_chunks_normal_geometry() {
    use crate::panes::terminal_character::guest_modal_chunks;
    let style = Style::default();
    let chunks = guest_modal_chunks(60, 20, 0, 0, &style, "my-session", 1);
    assert_eq!(chunks.len(), 20);
    let rendered: String = chunks
        .iter()
        .flat_map(|chunk| chunk.terminal_characters.iter().map(|c| c.character))
        .collect();
    assert!(rendered.contains("Nested Zellij session detected: my-session"));
    assert!(rendered.contains("Zoom in and control this session"));
    assert!(rendered.contains("(AUTO)"));
    assert!(rendered.contains("(MANUAL)"));
}

#[test]
pub fn guest_modal_chunks_small_geometry_fallback() {
    use crate::panes::terminal_character::guest_modal_chunks;
    let style = Style::default();
    let chunks = guest_modal_chunks(40, 4, 0, 0, &style, "s", 0);
    assert_eq!(chunks.len(), 4);
    let rendered: String = chunks
        .iter()
        .flat_map(|chunk| chunk.terminal_characters.iter().map(|c| c.character))
        .collect();
    assert!(rendered.contains("Nested Zellij session"));
}

#[test]
pub fn guest_modal_chunks_tiny_geometry_no_panic() {
    use crate::panes::terminal_character::guest_modal_chunks;
    let style = Style::default();
    let chunks = guest_modal_chunks(3, 2, 0, 0, &style, "session", 0);
    assert_eq!(chunks.len(), 2);
    let empty = guest_modal_chunks(0, 0, 0, 0, &style, "session", 0);
    assert!(empty.is_empty());
}

#[test]
pub fn guest_modal_option_hit_test() {
    use crate::panes::terminal_character::guest_modal_option_at_content_row;
    let block_height = 7;
    let start_row = (20 - block_height) / 2;
    let first_option = start_row + 2;
    assert_eq!(
        guest_modal_option_at_content_row(20, 60, first_option),
        Some(0)
    );
    assert_eq!(
        guest_modal_option_at_content_row(20, 60, first_option + 1),
        Some(1)
    );
    assert_eq!(
        guest_modal_option_at_content_row(20, 60, first_option + 2),
        Some(2)
    );
    assert_eq!(guest_modal_option_at_content_row(20, 60, 0), None);
    assert_eq!(guest_modal_option_at_content_row(4, 10, 2), None);
}
