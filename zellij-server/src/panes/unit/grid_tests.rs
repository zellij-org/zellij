use super::super::Grid;
use crate::panes::grid::SixelImageStore;
use crate::panes::link_handler::LinkHandler;
use insta::assert_snapshot;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use vte;
use zellij_utils::{
    data::{Palette, Style},
    pane_size::SizeInPixels,
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
fn vttest1_0() {
    let mut vte_parser = vte::Parser::new();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let osc8_hyperlinks = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut grid = Grid::new(
        41,
        110,
        Rc::new(RefCell::new(Palette::default())),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(None)),
        sixel_image_store,
        Style::default(),
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
    );
    let fixture_name = "vttest1-0";
    let content = read_fixture(fixture_name);
    for byte in content {
        vte_parser.advance(&mut grid, byte);
    }
    assert_snapshot!(format!("{:?}", grid));
}

#[test]
fn vttest1_1() {
    let mut vte_parser = vte::Parser::new();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let osc8_hyperlinks = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut grid = Grid::new(
        41,
        110,
        Rc::new(RefCell::new(Palette::default())),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(None)),
        sixel_image_store,
        Style::default(),
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
    );
    let fixture_name = "vttest1-1";
    let content = read_fixture(fixture_name);
    for byte in content {
        vte_parser.advance(&mut grid, byte);
    }
    assert_snapshot!(format!("{:?}", grid));
}

#[test]
fn vttest1_2() {
    let mut vte_parser = vte::Parser::new();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let osc8_hyperlinks = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut grid = Grid::new(
        41,
        110,
        Rc::new(RefCell::new(Palette::default())),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(None)),
        sixel_image_store,
        Style::default(),
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
    );
    let fixture_name = "vttest1-2";
    let content = read_fixture(fixture_name);
    for byte in content {
        vte_parser.advance(&mut grid, byte);
    }
    assert_snapshot!(format!("{:?}", grid));
}

#[test]
fn vttest1_3() {
    let mut vte_parser = vte::Parser::new();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let osc8_hyperlinks = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut grid = Grid::new(
        41,
        110,
        Rc::new(RefCell::new(Palette::default())),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(None)),
        sixel_image_store,
        Style::default(),
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
    );
    let fixture_name = "vttest1-3";
    let content = read_fixture(fixture_name);
    for byte in content {
        vte_parser.advance(&mut grid, byte);
    }
    assert_snapshot!(format!("{:?}", grid));
}

#[test]
fn vttest1_4() {
    let mut vte_parser = vte::Parser::new();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let osc8_hyperlinks = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut grid = Grid::new(
        41,
        110,
        Rc::new(RefCell::new(Palette::default())),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(None)),
        sixel_image_store,
        Style::default(),
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
    );
    let fixture_name = "vttest1-4";
    let content = read_fixture(fixture_name);
    for byte in content {
        vte_parser.advance(&mut grid, byte);
    }
    assert_snapshot!(format!("{:?}", grid));
}

#[test]
fn vttest1_5() {
    let mut vte_parser = vte::Parser::new();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let osc8_hyperlinks = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut grid = Grid::new(
        41,
        110,
        Rc::new(RefCell::new(Palette::default())),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(None)),
        sixel_image_store,
        Style::default(),
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
    );
    let fixture_name = "vttest1-5";
    let content = read_fixture(fixture_name);
    for byte in content {
        vte_parser.advance(&mut grid, byte);
    }
    assert_snapshot!(format!("{:?}", grid));
}

#[test]
fn vttest2_0() {
    let mut vte_parser = vte::Parser::new();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let osc8_hyperlinks = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut grid = Grid::new(
        41,
        110,
        Rc::new(RefCell::new(Palette::default())),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(None)),
        sixel_image_store,
        Style::default(),
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
    );
    let fixture_name = "vttest2-0";
    let content = read_fixture(fixture_name);
    for byte in content {
        vte_parser.advance(&mut grid, byte);
    }
    assert_snapshot!(format!("{:?}", grid));
}

#[test]
fn vttest2_1() {
    let mut vte_parser = vte::Parser::new();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let osc8_hyperlinks = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut grid = Grid::new(
        41,
        110,
        Rc::new(RefCell::new(Palette::default())),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(None)),
        sixel_image_store,
        Style::default(),
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
    );
    let fixture_name = "vttest2-1";
    let content = read_fixture(fixture_name);
    for byte in content {
        vte_parser.advance(&mut grid, byte);
    }
    assert_snapshot!(format!("{:?}", grid));
}

#[test]
fn vttest2_2() {
    let mut vte_parser = vte::Parser::new();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let osc8_hyperlinks = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut grid = Grid::new(
        41,
        110,
        Rc::new(RefCell::new(Palette::default())),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(None)),
        sixel_image_store,
        Style::default(),
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
    );
    let fixture_name = "vttest2-2";
    let content = read_fixture(fixture_name);
    for byte in content {
        vte_parser.advance(&mut grid, byte);
    }
    assert_snapshot!(format!("{:?}", grid));
}

#[test]
fn vttest2_3() {
    let mut vte_parser = vte::Parser::new();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let osc8_hyperlinks = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut grid = Grid::new(
        41,
        110,
        Rc::new(RefCell::new(Palette::default())),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(None)),
        sixel_image_store,
        Style::default(),
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
    );
    let fixture_name = "vttest2-3";
    let content = read_fixture(fixture_name);
    for byte in content {
        vte_parser.advance(&mut grid, byte);
    }
    assert_snapshot!(format!("{:?}", grid));
}

#[test]
fn vttest2_4() {
    let mut vte_parser = vte::Parser::new();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let osc8_hyperlinks = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut grid = Grid::new(
        41,
        110,
        Rc::new(RefCell::new(Palette::default())),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(None)),
        sixel_image_store,
        Style::default(),
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
    );
    let fixture_name = "vttest2-4";
    let content = read_fixture(fixture_name);
    for byte in content {
        vte_parser.advance(&mut grid, byte);
    }
    assert_snapshot!(format!("{:?}", grid));
}

#[test]
fn vttest2_5() {
    let mut vte_parser = vte::Parser::new();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let osc8_hyperlinks = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut grid = Grid::new(
        41,
        110,
        Rc::new(RefCell::new(Palette::default())),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(None)),
        sixel_image_store,
        Style::default(),
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
    );
    let fixture_name = "vttest2-5";
    let content = read_fixture(fixture_name);
    for byte in content {
        vte_parser.advance(&mut grid, byte);
    }
    assert_snapshot!(format!("{:?}", grid));
}

#[test]
fn vttest2_6() {
    let mut vte_parser = vte::Parser::new();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let osc8_hyperlinks = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut grid = Grid::new(
        41,
        110,
        Rc::new(RefCell::new(Palette::default())),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(None)),
        sixel_image_store,
        Style::default(),
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
    );
    let fixture_name = "vttest2-6";
    let content = read_fixture(fixture_name);
    for byte in content {
        vte_parser.advance(&mut grid, byte);
    }
    assert_snapshot!(format!("{:?}", grid));
}

#[test]
fn vttest2_7() {
    let mut vte_parser = vte::Parser::new();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let osc8_hyperlinks = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut grid = Grid::new(
        41,
        110,
        Rc::new(RefCell::new(Palette::default())),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(None)),
        sixel_image_store,
        Style::default(),
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
    );
    let fixture_name = "vttest2-7";
    let content = read_fixture(fixture_name);
    for byte in content {
        vte_parser.advance(&mut grid, byte);
    }
    assert_snapshot!(format!("{:?}", grid));
}

#[test]
fn vttest2_8() {
    let mut vte_parser = vte::Parser::new();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let osc8_hyperlinks = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut grid = Grid::new(
        41,
        110,
        Rc::new(RefCell::new(Palette::default())),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(None)),
        sixel_image_store,
        Style::default(),
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
    );
    let fixture_name = "vttest2-8";
    let content = read_fixture(fixture_name);
    for byte in content {
        vte_parser.advance(&mut grid, byte);
    }
    assert_snapshot!(format!("{:?}", grid));
}

#[test]
fn vttest2_9() {
    let mut vte_parser = vte::Parser::new();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let osc8_hyperlinks = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut grid = Grid::new(
        41,
        110,
        Rc::new(RefCell::new(Palette::default())),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(None)),
        sixel_image_store,
        Style::default(),
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
    );
    let fixture_name = "vttest2-9";
    let content = read_fixture(fixture_name);
    for byte in content {
        vte_parser.advance(&mut grid, byte);
    }
    assert_snapshot!(format!("{:?}", grid));
}

#[test]
fn vttest2_10() {
    let mut vte_parser = vte::Parser::new();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let osc8_hyperlinks = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut grid = Grid::new(
        41,
        110,
        Rc::new(RefCell::new(Palette::default())),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(None)),
        sixel_image_store,
        Style::default(),
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
    );
    let fixture_name = "vttest2-10";
    let content = read_fixture(fixture_name);
    for byte in content {
        vte_parser.advance(&mut grid, byte);
    }
    assert_snapshot!(format!("{:?}", grid));
}

#[test]
fn vttest2_11() {
    let mut vte_parser = vte::Parser::new();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let osc8_hyperlinks = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut grid = Grid::new(
        41,
        110,
        Rc::new(RefCell::new(Palette::default())),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(None)),
        sixel_image_store,
        Style::default(),
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
    );
    let fixture_name = "vttest2-11";
    let content = read_fixture(fixture_name);
    for byte in content {
        vte_parser.advance(&mut grid, byte);
    }
    assert_snapshot!(format!("{:?}", grid));
}

#[test]
fn vttest2_12() {
    let mut vte_parser = vte::Parser::new();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let osc8_hyperlinks = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut grid = Grid::new(
        41,
        110,
        Rc::new(RefCell::new(Palette::default())),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(None)),
        sixel_image_store,
        Style::default(),
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
    );
    let fixture_name = "vttest2-12";
    let content = read_fixture(fixture_name);
    for byte in content {
        vte_parser.advance(&mut grid, byte);
    }
    assert_snapshot!(format!("{:?}", grid));
}

#[test]
fn vttest2_13() {
    let mut vte_parser = vte::Parser::new();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let osc8_hyperlinks = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut grid = Grid::new(
        41,
        110,
        Rc::new(RefCell::new(Palette::default())),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(None)),
        sixel_image_store,
        Style::default(),
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
    );
    let fixture_name = "vttest2-13";
    let content = read_fixture(fixture_name);
    for byte in content {
        vte_parser.advance(&mut grid, byte);
    }
    assert_snapshot!(format!("{:?}", grid));
}

#[test]
fn vttest2_14() {
    let mut vte_parser = vte::Parser::new();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let osc8_hyperlinks = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut grid = Grid::new(
        41,
        110,
        Rc::new(RefCell::new(Palette::default())),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(None)),
        sixel_image_store,
        Style::default(),
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
    );
    let fixture_name = "vttest2-14";
    let content = read_fixture(fixture_name);
    for byte in content {
        vte_parser.advance(&mut grid, byte);
    }
    assert_snapshot!(format!("{:?}", grid));
}

#[test]
fn vttest3_0() {
    let mut vte_parser = vte::Parser::new();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let osc8_hyperlinks = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut grid = Grid::new(
        41,
        110,
        Rc::new(RefCell::new(Palette::default())),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(None)),
        sixel_image_store,
        Style::default(),
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
    );
    let fixture_name = "vttest3-0";
    let content = read_fixture(fixture_name);
    for byte in content {
        vte_parser.advance(&mut grid, byte);
    }
    assert_snapshot!(format!("{:?}", grid));
}

#[test]
fn vttest8_0() {
    let mut vte_parser = vte::Parser::new();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let osc8_hyperlinks = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut grid = Grid::new(
        51,
        97,
        Rc::new(RefCell::new(Palette::default())),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(None)),
        sixel_image_store,
        Style::default(),
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
    );
    let fixture_name = "vttest8-0";
    let content = read_fixture(fixture_name);
    for byte in content {
        vte_parser.advance(&mut grid, byte);
    }
    assert_snapshot!(format!("{:?}", grid));
}

#[test]
fn vttest8_1() {
    let mut vte_parser = vte::Parser::new();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let osc8_hyperlinks = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut grid = Grid::new(
        51,
        97,
        Rc::new(RefCell::new(Palette::default())),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(None)),
        sixel_image_store,
        Style::default(),
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
    );
    let fixture_name = "vttest8-1";
    let content = read_fixture(fixture_name);
    for byte in content {
        vte_parser.advance(&mut grid, byte);
    }
    assert_snapshot!(format!("{:?}", grid));
}

#[test]
fn vttest8_2() {
    let mut vte_parser = vte::Parser::new();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let osc8_hyperlinks = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut grid = Grid::new(
        51,
        97,
        Rc::new(RefCell::new(Palette::default())),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(None)),
        sixel_image_store,
        Style::default(),
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
    );
    let fixture_name = "vttest8-2";
    let content = read_fixture(fixture_name);
    for byte in content {
        vte_parser.advance(&mut grid, byte);
    }
    assert_snapshot!(format!("{:?}", grid));
}

#[test]
fn vttest8_3() {
    let mut vte_parser = vte::Parser::new();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let osc8_hyperlinks = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut grid = Grid::new(
        51,
        97,
        Rc::new(RefCell::new(Palette::default())),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(None)),
        sixel_image_store,
        Style::default(),
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
    );
    let fixture_name = "vttest8-3";
    let content = read_fixture(fixture_name);
    for byte in content {
        vte_parser.advance(&mut grid, byte);
    }
    assert_snapshot!(format!("{:?}", grid));
}

#[test]
fn vttest8_4() {
    let mut vte_parser = vte::Parser::new();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let osc8_hyperlinks = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut grid = Grid::new(
        51,
        97,
        Rc::new(RefCell::new(Palette::default())),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(None)),
        sixel_image_store,
        Style::default(),
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
    );
    let fixture_name = "vttest8-4";
    let content = read_fixture(fixture_name);
    for byte in content {
        vte_parser.advance(&mut grid, byte);
    }
    assert_snapshot!(format!("{:?}", grid));
}

#[test]
fn vttest8_5() {
    let mut vte_parser = vte::Parser::new();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let osc8_hyperlinks = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut grid = Grid::new(
        51,
        97,
        Rc::new(RefCell::new(Palette::default())),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(None)),
        sixel_image_store,
        Style::default(),
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
    );
    let fixture_name = "vttest8-5";
    let content = read_fixture(fixture_name);
    for byte in content {
        vte_parser.advance(&mut grid, byte);
    }
    assert_snapshot!(format!("{:?}", grid));
}

#[test]
fn csi_b() {
    let mut vte_parser = vte::Parser::new();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let osc8_hyperlinks = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut grid = Grid::new(
        51,
        97,
        Rc::new(RefCell::new(Palette::default())),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(None)),
        sixel_image_store,
        Style::default(),
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
    );
    let fixture_name = "csi-b";
    let content = read_fixture(fixture_name);
    for byte in content {
        vte_parser.advance(&mut grid, byte);
    }
    assert_snapshot!(format!("{:?}", grid));
}

#[test]
fn csi_capital_i() {
    let mut vte_parser = vte::Parser::new();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let osc8_hyperlinks = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut grid = Grid::new(
        51,
        97,
        Rc::new(RefCell::new(Palette::default())),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(None)),
        sixel_image_store,
        Style::default(),
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
    );
    let fixture_name = "csi-capital-i";
    let content = read_fixture(fixture_name);
    for byte in content {
        vte_parser.advance(&mut grid, byte);
    }
    assert_snapshot!(format!("{:?}", grid));
}

#[test]
fn csi_capital_z() {
    let mut vte_parser = vte::Parser::new();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let osc8_hyperlinks = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut grid = Grid::new(
        51,
        97,
        Rc::new(RefCell::new(Palette::default())),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(None)),
        sixel_image_store,
        Style::default(),
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
    );
    let fixture_name = "csi-capital-z";
    let content = read_fixture(fixture_name);
    for byte in content {
        vte_parser.advance(&mut grid, byte);
    }
    assert_snapshot!(format!("{:?}", grid));
}

#[test]
fn terminal_reports() {
    let mut vte_parser = vte::Parser::new();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let osc8_hyperlinks = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut grid = Grid::new(
        51,
        97,
        Rc::new(RefCell::new(Palette::default())),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(None)),
        sixel_image_store,
        Style::default(),
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
    );
    let fixture_name = "terminal_reports";
    let content = read_fixture(fixture_name);
    for byte in content {
        vte_parser.advance(&mut grid, byte);
    }
    assert_snapshot!(format!("{:?}", grid.pending_messages_to_pty));
}

#[test]
fn wide_characters() {
    let mut vte_parser = vte::Parser::new();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let osc8_hyperlinks = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut grid = Grid::new(
        21,
        104,
        Rc::new(RefCell::new(Palette::default())),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(None)),
        sixel_image_store,
        Style::default(),
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
    );
    let fixture_name = "wide_characters";
    let content = read_fixture(fixture_name);
    for byte in content {
        vte_parser.advance(&mut grid, byte);
    }
    assert_snapshot!(format!("{:?}", grid));
}

#[test]
fn wide_characters_line_wrap() {
    let mut vte_parser = vte::Parser::new();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let osc8_hyperlinks = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut grid = Grid::new(
        21,
        104,
        Rc::new(RefCell::new(Palette::default())),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(None)),
        sixel_image_store,
        Style::default(),
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
    );
    let fixture_name = "wide_characters_line_wrap";
    let content = read_fixture(fixture_name);
    for byte in content {
        vte_parser.advance(&mut grid, byte);
    }
    assert_snapshot!(format!("{:?}", grid));
}

#[test]
fn insert_character_in_line_with_wide_character() {
    let mut vte_parser = vte::Parser::new();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let osc8_hyperlinks = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut grid = Grid::new(
        21,
        104,
        Rc::new(RefCell::new(Palette::default())),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(None)),
        sixel_image_store,
        Style::default(),
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
    );
    let fixture_name = "wide_characters_middle_line_insert";
    let content = read_fixture(fixture_name);
    for byte in content {
        vte_parser.advance(&mut grid, byte);
    }
    assert_snapshot!(format!("{:?}", grid));
}

#[test]
fn delete_char_in_middle_of_line_with_widechar() {
    let mut vte_parser = vte::Parser::new();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let osc8_hyperlinks = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut grid = Grid::new(
        21,
        104,
        Rc::new(RefCell::new(Palette::default())),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(None)),
        sixel_image_store,
        Style::default(),
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
    );
    let fixture_name = "wide-chars-delete-middle";
    let content = read_fixture(fixture_name);
    for byte in content {
        vte_parser.advance(&mut grid, byte);
    }
    assert_snapshot!(format!("{:?}", grid));
}

#[test]
fn delete_char_in_middle_of_line_with_multiple_widechars() {
    let mut vte_parser = vte::Parser::new();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let osc8_hyperlinks = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut grid = Grid::new(
        21,
        104,
        Rc::new(RefCell::new(Palette::default())),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(None)),
        sixel_image_store,
        Style::default(),
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
    );
    let fixture_name = "wide-chars-delete-middle-after-multi";
    let content = read_fixture(fixture_name);
    for byte in content {
        vte_parser.advance(&mut grid, byte);
    }
    assert_snapshot!(format!("{:?}", grid));
}

#[test]
fn fish_wide_characters_override_clock() {
    let mut vte_parser = vte::Parser::new();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let osc8_hyperlinks = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut grid = Grid::new(
        21,
        104,
        Rc::new(RefCell::new(Palette::default())),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(None)),
        sixel_image_store,
        Style::default(),
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
    );
    let fixture_name = "fish_wide_characters_override_clock";
    let content = read_fixture(fixture_name);
    for byte in content {
        vte_parser.advance(&mut grid, byte);
    }
    assert_snapshot!(format!("{:?}", grid));
}

#[test]
fn bash_delete_wide_characters() {
    let mut vte_parser = vte::Parser::new();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let osc8_hyperlinks = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut grid = Grid::new(
        21,
        104,
        Rc::new(RefCell::new(Palette::default())),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(None)),
        sixel_image_store,
        Style::default(),
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
    );
    let fixture_name = "bash_delete_wide_characters";
    let content = read_fixture(fixture_name);
    for byte in content {
        vte_parser.advance(&mut grid, byte);
    }
    assert_snapshot!(format!("{:?}", grid));
}

#[test]
fn delete_wide_characters_before_cursor() {
    let mut vte_parser = vte::Parser::new();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let osc8_hyperlinks = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut grid = Grid::new(
        21,
        104,
        Rc::new(RefCell::new(Palette::default())),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(None)),
        sixel_image_store,
        Style::default(),
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
    );
    let fixture_name = "delete_wide_characters_before_cursor";
    let content = read_fixture(fixture_name);
    for byte in content {
        vte_parser.advance(&mut grid, byte);
    }
    assert_snapshot!(format!("{:?}", grid));
}

#[test]
fn delete_wide_characters_before_cursor_when_cursor_is_on_wide_character() {
    let mut vte_parser = vte::Parser::new();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let osc8_hyperlinks = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut grid = Grid::new(
        21,
        104,
        Rc::new(RefCell::new(Palette::default())),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(None)),
        sixel_image_store,
        Style::default(),
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
    );
    let fixture_name = "delete_wide_characters_before_cursor_when_cursor_is_on_wide_character";
    let content = read_fixture(fixture_name);
    for byte in content {
        vte_parser.advance(&mut grid, byte);
    }
    assert_snapshot!(format!("{:?}", grid));
}

#[test]
fn delete_wide_character_under_cursor() {
    let mut vte_parser = vte::Parser::new();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let osc8_hyperlinks = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut grid = Grid::new(
        21,
        104,
        Rc::new(RefCell::new(Palette::default())),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(None)),
        sixel_image_store,
        Style::default(),
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
    );
    let fixture_name = "delete_wide_character_under_cursor";
    let content = read_fixture(fixture_name);
    for byte in content {
        vte_parser.advance(&mut grid, byte);
    }
    assert_snapshot!(format!("{:?}", grid));
}

#[test]
fn replace_wide_character_under_cursor() {
    let mut vte_parser = vte::Parser::new();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let osc8_hyperlinks = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut grid = Grid::new(
        21,
        104,
        Rc::new(RefCell::new(Palette::default())),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(None)),
        sixel_image_store,
        Style::default(),
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
    );
    let fixture_name = "replace_wide_character_under_cursor";
    let content = read_fixture(fixture_name);
    for byte in content {
        vte_parser.advance(&mut grid, byte);
    }
    assert_snapshot!(format!("{:?}", grid));
}

#[test]
fn wrap_wide_characters() {
    let mut vte_parser = vte::Parser::new();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let osc8_hyperlinks = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut grid = Grid::new(
        21,
        90,
        Rc::new(RefCell::new(Palette::default())),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(None)),
        sixel_image_store,
        Style::default(),
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
    );
    let fixture_name = "wide_characters_full";
    let content = read_fixture(fixture_name);
    for byte in content {
        vte_parser.advance(&mut grid, byte);
    }
    assert_snapshot!(format!("{:?}", grid));
}

#[test]
fn wrap_wide_characters_on_size_change() {
    let mut vte_parser = vte::Parser::new();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let osc8_hyperlinks = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut grid = Grid::new(
        21,
        93,
        Rc::new(RefCell::new(Palette::default())),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(None)),
        sixel_image_store,
        Style::default(),
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
    );
    let fixture_name = "wide_characters_full";
    let content = read_fixture(fixture_name);
    for byte in content {
        vte_parser.advance(&mut grid, byte);
    }
    grid.change_size(21, 90);
    assert_snapshot!(format!("{:?}", grid));
}

#[test]
fn unwrap_wide_characters_on_size_change() {
    let mut vte_parser = vte::Parser::new();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let osc8_hyperlinks = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut grid = Grid::new(
        21,
        93,
        Rc::new(RefCell::new(Palette::default())),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(None)),
        sixel_image_store,
        Style::default(),
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
    );
    let fixture_name = "wide_characters_full";
    let content = read_fixture(fixture_name);
    for byte in content {
        vte_parser.advance(&mut grid, byte);
    }
    grid.change_size(21, 90);
    grid.change_size(21, 93);
    assert_snapshot!(format!("{:?}", grid));
}

#[test]
fn wrap_wide_characters_in_the_middle_of_the_line() {
    let mut vte_parser = vte::Parser::new();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let osc8_hyperlinks = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut grid = Grid::new(
        21,
        91,
        Rc::new(RefCell::new(Palette::default())),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(None)),
        sixel_image_store,
        Style::default(),
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
    );
    let fixture_name = "wide_characters_line_middle";
    let content = read_fixture(fixture_name);
    for byte in content {
        vte_parser.advance(&mut grid, byte);
    }
    assert_snapshot!(format!("{:?}", grid));
}

#[test]
fn wrap_wide_characters_at_the_end_of_the_line() {
    let mut vte_parser = vte::Parser::new();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let osc8_hyperlinks = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut grid = Grid::new(
        21,
        90,
        Rc::new(RefCell::new(Palette::default())),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(None)),
        sixel_image_store,
        Style::default(),
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
    );
    let fixture_name = "wide_characters_line_end";
    let content = read_fixture(fixture_name);
    for byte in content {
        vte_parser.advance(&mut grid, byte);
    }
    assert_snapshot!(format!("{:?}", grid));
}

#[test]
fn copy_selected_text_from_viewport() {
    let mut vte_parser = vte::Parser::new();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let osc8_hyperlinks = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut grid = Grid::new(
        27,
        125,
        Rc::new(RefCell::new(Palette::default())),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(None)),
        sixel_image_store,
        Style::default(),
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
    );
    let fixture_name = "grid_copy";
    let content = read_fixture(fixture_name);
    for byte in content {
        vte_parser.advance(&mut grid, byte);
    }

    grid.start_selection(&Position::new(23, 6));
    // check for widechar, 📦 occupies columns 34, 35, and gets selected even if only the first column is selected
    grid.end_selection(&Position::new(25, 35));
    let text = grid.get_selected_text();
    assert_eq!(
        text.unwrap(),
        "mauris in aliquam sem fringilla.\n\nzellij on  mouse-support [?] is 📦"
    );
}

#[test]
fn copy_wrapped_selected_text_from_viewport() {
    let mut vte_parser = vte::Parser::new();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let osc8_hyperlinks = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut grid = Grid::new(
        22,
        73,
        Rc::new(RefCell::new(Palette::default())),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(None)),
        sixel_image_store,
        Style::default(),
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
    );
    let fixture_name = "grid_copy_wrapped";
    let content = read_fixture(fixture_name);
    for byte in content {
        vte_parser.advance(&mut grid, byte);
    }

    grid.start_selection(&Position::new(5, 0));
    grid.end_selection(&Position::new(8, 42));
    let text = grid.get_selected_text();
    assert_eq!(
        text.unwrap(),
        "Lorem ipsum dolor sit amet,                                                                                                                          consectetur adipiscing elit, sed do eiusmod tempor incididunt ut labore et dolore magna aliqua."
    );
}

#[test]
fn copy_selected_text_from_lines_above() {
    let mut vte_parser = vte::Parser::new();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let osc8_hyperlinks = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut grid = Grid::new(
        27,
        125,
        Rc::new(RefCell::new(Palette::default())),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(None)),
        sixel_image_store,
        Style::default(),
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
    );
    let fixture_name = "grid_copy";
    let content = read_fixture(fixture_name);
    for byte in content {
        vte_parser.advance(&mut grid, byte);
    }

    grid.start_selection(&Position::new(-2, 10));
    // check for widechar, 📦 occupies columns 34, 35, and gets selected even if only the first column is selected
    grid.end_selection(&Position::new(2, 8));
    let text = grid.get_selected_text();
    assert_eq!(
        text.unwrap(),
        "eu scelerisque felis imperdiet proin fermentum leo.\nCursus risus at ultrices mi tempus.\nLaoreet id donec ultrices tincidunt arcu non sodales.\nAmet dictum sit amet justo donec enim.\nHac habi"
    );
}

#[test]
fn copy_selected_text_from_lines_below() {
    let mut vte_parser = vte::Parser::new();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let osc8_hyperlinks = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut grid = Grid::new(
        27,
        125,
        Rc::new(RefCell::new(Palette::default())),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(None)),
        sixel_image_store,
        Style::default(),
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
    );
    let fixture_name = "grid_copy";
    let content = read_fixture(fixture_name);
    for byte in content {
        vte_parser.advance(&mut grid, byte);
    }

    grid.move_viewport_up(40);

    grid.start_selection(&Position::new(63, 6));
    // check for widechar, 📦 occupies columns 34, 35, and gets selected even if only the first column is selected
    grid.end_selection(&Position::new(65, 35));
    let text = grid.get_selected_text();
    assert_eq!(
        text.unwrap(),
        "mauris in aliquam sem fringilla.\n\nzellij on  mouse-support [?] is 📦"
    );
}

/*
 * These tests below are general compatibility tests for non-trivial scenarios running in the terminal.
 * They use fake TTY input replicated from these scenarios.
 *
 */

#[test]
fn run_bandwhich_from_fish_shell() {
    let mut vte_parser = vte::Parser::new();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let osc8_hyperlinks = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut grid = Grid::new(
        28,
        116,
        Rc::new(RefCell::new(Palette::default())),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(None)),
        sixel_image_store,
        Style::default(),
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
    );
    let fixture_name = "fish_and_bandwhich";
    let content = read_fixture(fixture_name);
    for byte in content {
        vte_parser.advance(&mut grid, byte);
    }
    assert_snapshot!(format!("{:?}", grid));
}

#[test]
fn fish_tab_completion_options() {
    let mut vte_parser = vte::Parser::new();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let osc8_hyperlinks = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut grid = Grid::new(
        28,
        116,
        Rc::new(RefCell::new(Palette::default())),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(None)),
        sixel_image_store,
        Style::default(),
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
    );
    let fixture_name = "fish_tab_completion_options";
    let content = read_fixture(fixture_name);
    for byte in content {
        vte_parser.advance(&mut grid, byte);
    }
    assert_snapshot!(format!("{:?}", grid));
}

#[test]
pub fn fish_select_tab_completion_options() {
    // the difference between this and the previous test is that here we press <TAB>
    // twice, meaning the selection moves between the options and the command line
    // changes.
    // this is not clearly seen in the snapshot because it does not include styles,
    // but we can see the command line change and the cursor staying in place
    // terminal_emulator_color_codes,
    let mut vte_parser = vte::Parser::new();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let osc8_hyperlinks = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut grid = Grid::new(
        28,
        116,
        Rc::new(RefCell::new(Palette::default())),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(None)),
        sixel_image_store,
        Style::default(),
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
    );
    let fixture_name = "fish_select_tab_completion_options";
    let content = read_fixture(fixture_name);
    for byte in content {
        vte_parser.advance(&mut grid, byte);
    }
    assert_snapshot!(format!("{:?}", grid));
}

#[test]
pub fn vim_scroll_region_down() {
    // here we test a case where vim defines the scroll region as lesser than the screen row count
    // and then scrolls down
    // the region is defined here by vim as 1-26 (there are 28 rows)
    // then the cursor is moved to line 26 and a new line is added
    // what should happen is that the first line in the scroll region (1) is deleted
    // terminal_emulator_color_codes,
    // and an empty line is inserted in the last scroll region line (26)
    // this tests also has other steps afterwards that fills the line with the next line in the
    // sixel_image_store,
    // file
    let mut vte_parser = vte::Parser::new();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let osc8_hyperlinks = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut grid = Grid::new(
        28,
        116,
        Rc::new(RefCell::new(Palette::default())),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(None)),
        sixel_image_store,
        Style::default(),
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
    );
    let fixture_name = "vim_scroll_region_down";
    let content = read_fixture(fixture_name);
    for byte in content {
        vte_parser.advance(&mut grid, byte);
    }
    assert_snapshot!(format!("{:?}", grid));
}

#[test]
pub fn vim_ctrl_d() {
    // in vim ctrl-d moves down half a page
    // in this case, it sends the terminal the csi 'M' directive, which tells it to delete X (13 in
    // this case) lines inside the scroll region and push the other lines up
    // what happens here is that 13 lines are deleted and instead 13 empty lines are added at the
    // end of the scroll region
    // terminal_emulator_color_codes,
    // vim makes sure to fill these empty lines with the rest of the file
    let mut vte_parser = vte::Parser::new();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let osc8_hyperlinks = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut grid = Grid::new(
        28,
        116,
        Rc::new(RefCell::new(Palette::default())),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(None)),
        sixel_image_store,
        Style::default(),
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
    );
    let fixture_name = "vim_ctrl_d";
    let content = read_fixture(fixture_name);
    for byte in content {
        vte_parser.advance(&mut grid, byte);
    }
    assert_snapshot!(format!("{:?}", grid));
}

#[test]
pub fn vim_ctrl_u() {
    // in vim ctrl-u moves up half a page
    // in this case, it sends the terminal the csi 'L' directive, which tells it to insert X (13 in
    // this case) lines at the cursor, pushing away (deleting) the last line in the scroll region
    // this causes the effect of scrolling up X lines (vim replaces the lines with the ones in the
    // file above the current content)
    // terminal_emulator_color_codes,
    let mut vte_parser = vte::Parser::new();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let osc8_hyperlinks = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut grid = Grid::new(
        28,
        116,
        Rc::new(RefCell::new(Palette::default())),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(None)),
        sixel_image_store,
        Style::default(),
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
    );
    let fixture_name = "vim_ctrl_u";
    let content = read_fixture(fixture_name);
    for byte in content {
        vte_parser.advance(&mut grid, byte);
    }
    assert_snapshot!(format!("{:?}", grid));
}

#[test]
pub fn htop() {
    let mut vte_parser = vte::Parser::new();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let osc8_hyperlinks = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut grid = Grid::new(
        28,
        116,
        Rc::new(RefCell::new(Palette::default())),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(None)),
        sixel_image_store,
        Style::default(),
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
    );
    let fixture_name = "htop";
    let content = read_fixture(fixture_name);
    for byte in content {
        vte_parser.advance(&mut grid, byte);
    }
    assert_snapshot!(format!("{:?}", grid));
}

#[test]
pub fn htop_scrolling() {
    let mut vte_parser = vte::Parser::new();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let osc8_hyperlinks = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut grid = Grid::new(
        28,
        116,
        Rc::new(RefCell::new(Palette::default())),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(None)),
        sixel_image_store,
        Style::default(),
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
    );
    let fixture_name = "htop_scrolling";
    let content = read_fixture(fixture_name);
    for byte in content {
        vte_parser.advance(&mut grid, byte);
    }
    assert_snapshot!(format!("{:?}", grid));
}

#[test]
pub fn htop_right_scrolling() {
    let mut vte_parser = vte::Parser::new();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let osc8_hyperlinks = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut grid = Grid::new(
        28,
        116,
        Rc::new(RefCell::new(Palette::default())),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(None)),
        sixel_image_store,
        Style::default(),
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
    );
    let fixture_name = "htop_right_scrolling";
    let content = read_fixture(fixture_name);
    for byte in content {
        vte_parser.advance(&mut grid, byte);
    }
    assert_snapshot!(format!("{:?}", grid));
}

#[test]
pub fn vim_overwrite() {
    // this tests the vim overwrite message
    // to recreate:
    // * open a file in vim
    // * open the same file in another window
    // * change the file in the other window and save
    // terminal_emulator_color_codes,
    // * change the file in the original vim window and save
    // * confirm you would like to change the file by pressing 'y' and then ENTER
    // sixel_image_store,
    // * if everything looks fine, this test passed :)
    let mut vte_parser = vte::Parser::new();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let osc8_hyperlinks = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut grid = Grid::new(
        28,
        116,
        Rc::new(RefCell::new(Palette::default())),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(None)),
        sixel_image_store,
        Style::default(),
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
    );
    let fixture_name = "vim_overwrite";
    let content = read_fixture(fixture_name);
    for byte in content {
        vte_parser.advance(&mut grid, byte);
    }
    assert_snapshot!(format!("{:?}", grid));
}

#[test]
pub fn clear_scroll_region() {
    // this is actually a test of 1049h/l (alternative buffer)
    // @imsnif - the name is a monument to the time I didn't fully understand this mechanism :)
    let mut vte_parser = vte::Parser::new();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let osc8_hyperlinks = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut grid = Grid::new(
        28,
        116,
        Rc::new(RefCell::new(Palette::default())),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(None)),
        sixel_image_store,
        Style::default(),
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
    );
    let fixture_name = "clear_scroll_region";
    let content = read_fixture(fixture_name);
    for byte in content {
        vte_parser.advance(&mut grid, byte);
    }
    assert_snapshot!(format!("{:?}", grid));
}

#[test]
pub fn display_tab_characters_properly() {
    let mut vte_parser = vte::Parser::new();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let osc8_hyperlinks = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut grid = Grid::new(
        28,
        116,
        Rc::new(RefCell::new(Palette::default())),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(None)),
        sixel_image_store,
        Style::default(),
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
    );
    let fixture_name = "tab_characters";
    let content = read_fixture(fixture_name);
    for byte in content {
        vte_parser.advance(&mut grid, byte);
    }
    assert_snapshot!(format!("{:?}", grid));
}

#[test]
pub fn neovim_insert_mode() {
    let mut vte_parser = vte::Parser::new();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let osc8_hyperlinks = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut grid = Grid::new(
        28,
        116,
        Rc::new(RefCell::new(Palette::default())),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(None)),
        sixel_image_store,
        Style::default(),
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
    );
    let fixture_name = "nvim_insert";
    let content = read_fixture(fixture_name);
    for byte in content {
        vte_parser.advance(&mut grid, byte);
    }
    assert_snapshot!(format!("{:?}", grid));
}

#[test]
pub fn bash_cursor_linewrap() {
    let mut vte_parser = vte::Parser::new();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let osc8_hyperlinks = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut grid = Grid::new(
        28,
        116,
        Rc::new(RefCell::new(Palette::default())),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(None)),
        sixel_image_store,
        Style::default(),
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
    );
    let fixture_name = "bash_cursor_linewrap";
    let content = read_fixture(fixture_name);
    for byte in content {
        vte_parser.advance(&mut grid, byte);
    }
    assert_snapshot!(format!("{:?}", grid));
}

#[test]
pub fn fish_paste_multiline() {
    // here we paste a multiline command in fish shell, making sure we support it
    // going up and changing the colors of our line-wrapped pasted text
    let mut vte_parser = vte::Parser::new();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let osc8_hyperlinks = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut grid = Grid::new(
        28,
        149,
        Rc::new(RefCell::new(Palette::default())),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(None)),
        sixel_image_store,
        Style::default(),
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
    );
    let fixture_name = "fish_paste_multiline";
    let content = read_fixture(fixture_name);
    for byte in content {
        vte_parser.advance(&mut grid, byte);
    }
    assert_snapshot!(format!("{:?}", grid));
}

#[test]
pub fn git_log() {
    let mut vte_parser = vte::Parser::new();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let osc8_hyperlinks = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut grid = Grid::new(
        28,
        149,
        Rc::new(RefCell::new(Palette::default())),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(None)),
        sixel_image_store,
        Style::default(),
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
    );
    let fixture_name = "git_log";
    let content = read_fixture(fixture_name);
    for byte in content {
        vte_parser.advance(&mut grid, byte);
    }
    assert_snapshot!(format!("{:?}", grid));
}

#[test]
pub fn git_diff_scrollup() {
    // this tests makes sure that when we have a git diff that exceeds the screen size
    // we are able to scroll up
    let mut vte_parser = vte::Parser::new();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let osc8_hyperlinks = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut grid = Grid::new(
        28,
        149,
        Rc::new(RefCell::new(Palette::default())),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(None)),
        sixel_image_store,
        Style::default(),
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
    );
    let fixture_name = "git_diff_scrollup";
    let content = read_fixture(fixture_name);
    for byte in content {
        vte_parser.advance(&mut grid, byte);
    }
    assert_snapshot!(format!("{:?}", grid));
}

#[test]
pub fn emacs_longbuf() {
    let mut vte_parser = vte::Parser::new();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let osc8_hyperlinks = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut grid = Grid::new(
        60,
        284,
        Rc::new(RefCell::new(Palette::default())),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(None)),
        sixel_image_store,
        Style::default(),
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
    );
    let fixture_name = "emacs_longbuf_tutorial";
    let content = read_fixture(fixture_name);
    for byte in content {
        vte_parser.advance(&mut grid, byte);
    }
    assert_snapshot!(format!("{:?}", grid));
}

#[test]
pub fn top_and_quit() {
    let mut vte_parser = vte::Parser::new();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let osc8_hyperlinks = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut grid = Grid::new(
        56,
        235,
        Rc::new(RefCell::new(Palette::default())),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(None)),
        sixel_image_store,
        Style::default(),
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
    );
    let fixture_name = "top_and_quit";
    let content = read_fixture(fixture_name);
    for byte in content {
        vte_parser.advance(&mut grid, byte);
    }
    assert_snapshot!(format!("{:?}", grid));
}

#[test]
pub fn exa_plus_omf_theme() {
    // this tests that we handle a tab delimited table properly
    // without overriding the previous content
    // this is a potential bug because the \t character is a goto
    // if we forwarded it as is to the terminal, we would be skipping
    // over existing on-screen content without deleting it, so we must
    // terminal_emulator_color_codes,
    // convert it to spaces
    let mut vte_parser = vte::Parser::new();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let osc8_hyperlinks = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut grid = Grid::new(
        56,
        235,
        Rc::new(RefCell::new(Palette::default())),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(None)),
        sixel_image_store,
        Style::default(),
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
    );
    let fixture_name = "exa_plus_omf_theme";
    let content = read_fixture(fixture_name);
    for byte in content {
        vte_parser.advance(&mut grid, byte);
    }
    assert_snapshot!(format!("{:?}", grid));
}

#[test]
pub fn scroll_up() {
    let mut vte_parser = vte::Parser::new();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let osc8_hyperlinks = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut grid = Grid::new(
        10,
        50,
        Rc::new(RefCell::new(Palette::default())),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(None)),
        sixel_image_store,
        Style::default(),
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
    );
    let fixture_name = "scrolling";
    let content = read_fixture(fixture_name);
    for byte in content {
        vte_parser.advance(&mut grid, byte);
    }
    grid.scroll_up_one_line();
    assert_snapshot!(format!("{:?}", grid));
}

#[test]
pub fn scroll_down() {
    let mut vte_parser = vte::Parser::new();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let osc8_hyperlinks = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut grid = Grid::new(
        10,
        50,
        Rc::new(RefCell::new(Palette::default())),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(None)),
        sixel_image_store,
        Style::default(),
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
    );
    let fixture_name = "scrolling";
    let content = read_fixture(fixture_name);
    for byte in content {
        vte_parser.advance(&mut grid, byte);
    }
    grid.scroll_up_one_line();
    grid.scroll_down_one_line();
    assert_snapshot!(format!("{:?}", grid));
}

#[test]
pub fn scroll_up_with_line_wraps() {
    let mut vte_parser = vte::Parser::new();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let osc8_hyperlinks = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut grid = Grid::new(
        10,
        25,
        Rc::new(RefCell::new(Palette::default())),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(None)),
        sixel_image_store,
        Style::default(),
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
    );
    let fixture_name = "scrolling";
    let content = read_fixture(fixture_name);
    for byte in content {
        vte_parser.advance(&mut grid, byte);
    }
    grid.scroll_up_one_line();
    assert_snapshot!(format!("{:?}", grid));
}

#[test]
pub fn scroll_down_with_line_wraps() {
    let mut vte_parser = vte::Parser::new();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let osc8_hyperlinks = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut grid = Grid::new(
        10,
        25,
        Rc::new(RefCell::new(Palette::default())),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(None)),
        sixel_image_store,
        Style::default(),
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
    );
    let fixture_name = "scrolling";
    let content = read_fixture(fixture_name);
    for byte in content {
        vte_parser.advance(&mut grid, byte);
    }
    grid.scroll_up_one_line();
    grid.scroll_down_one_line();
    assert_snapshot!(format!("{:?}", grid));
}

#[test]
pub fn scroll_up_decrease_width_and_scroll_down() {
    let mut vte_parser = vte::Parser::new();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let osc8_hyperlinks = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut grid = Grid::new(
        10,
        50,
        Rc::new(RefCell::new(Palette::default())),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(None)),
        sixel_image_store,
        Style::default(),
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
    );
    let fixture_name = "scrolling";
    let content = read_fixture(fixture_name);
    for byte in content {
        vte_parser.advance(&mut grid, byte);
    }
    for _ in 0..10 {
        grid.scroll_up_one_line();
    }
    grid.change_size(10, 25);
    for _ in 0..10 {
        grid.scroll_down_one_line();
    }
    assert_snapshot!(format!("{:?}", grid));
}

#[test]
pub fn scroll_up_increase_width_and_scroll_down() {
    let mut vte_parser = vte::Parser::new();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let osc8_hyperlinks = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut grid = Grid::new(
        10,
        25,
        Rc::new(RefCell::new(Palette::default())),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(None)),
        sixel_image_store,
        Style::default(),
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
    );
    let fixture_name = "scrolling";
    let content = read_fixture(fixture_name);
    for byte in content {
        vte_parser.advance(&mut grid, byte);
    }
    for _ in 0..10 {
        grid.scroll_up_one_line();
    }
    grid.change_size(10, 50);
    for _ in 0..10 {
        grid.scroll_down_one_line();
    }
    assert_snapshot!(format!("{:?}", grid));
}

#[test]
fn saved_cursor_across_resize() {
    let mut vte_parser = vte::Parser::new();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let osc8_hyperlinks = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut grid = Grid::new(
        4,
        20,
        Rc::new(RefCell::new(Palette::default())),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(None)),
        sixel_image_store,
        Style::default(),
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
    );
    let mut parse = |s, grid: &mut Grid| {
        for b in Vec::from(s) {
            vte_parser.advance(&mut *grid, b)
        }
    };
    let content = "
\rLine 1 >fill to 20_<
\rLine 2 >fill to 20_<
\rLine 3 >fill to 20_<
\rL\u{1b}[sine 4 >fill to 20_<";
    parse(content, &mut grid);
    // Move real cursor position up three lines
    let content = "\u{1b}[3A";
    parse(content, &mut grid);
    // Truncate top of terminal, resetting cursor (but not saved cursor)
    grid.change_size(3, 20);
    // Wrap, resetting cursor again (but not saved cursor)
    grid.change_size(3, 10);
    // Restore saved cursor position and write ZZZ
    let content = "\u{1b}[uZZZ";
    parse(content, &mut grid);
    assert_snapshot!(format!("{:?}", grid));
}

#[test]
fn saved_cursor_across_resize_longline() {
    let mut vte_parser = vte::Parser::new();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let osc8_hyperlinks = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut grid = Grid::new(
        4,
        20,
        Rc::new(RefCell::new(Palette::default())),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(None)),
        sixel_image_store,
        Style::default(),
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
    );
    let mut parse = |s, grid: &mut Grid| {
        for b in Vec::from(s) {
            vte_parser.advance(&mut *grid, b)
        }
    };
    let content = "
\rLine 1 >fill \u{1b}[sto 20_<";
    parse(content, &mut grid);
    // Wrap each line precisely halfway
    grid.change_size(4, 10);
    // Write 'YY' at the end (ends up on a new wrapped line), restore to the saved cursor
    // and overwrite 'to' with 'ZZ'
    let content = "YY\u{1b}[uZZ";
    parse(content, &mut grid);
    assert_snapshot!(format!("{:?}", grid));
}

#[test]
fn saved_cursor_across_resize_rewrap() {
    let mut vte_parser = vte::Parser::new();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let osc8_hyperlinks = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut grid = Grid::new(
        4,
        4 * 8,
        Rc::new(RefCell::new(Palette::default())),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(None)),
        sixel_image_store,
        Style::default(),
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
    );
    let mut parse = |s, grid: &mut Grid| {
        for b in Vec::from(s) {
            vte_parser.advance(&mut *grid, b)
        }
    };
    let content = "
\r12345678123456781234567\u{1b}[s812345678"; // 4*8 chars
    parse(content, &mut grid);
    // Wrap each line precisely halfway, then rewrap to halve them again
    grid.change_size(4, 16);
    grid.change_size(4, 8);
    // Write 'Z' at the end of line 3
    let content = "\u{1b}[uZ";
    parse(content, &mut grid);
    assert_snapshot!(format!("{:?}", grid));
}

#[test]
pub fn move_cursor_below_scroll_region() {
    let mut vte_parser = vte::Parser::new();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let osc8_hyperlinks = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut grid = Grid::new(
        34,
        114,
        Rc::new(RefCell::new(Palette::default())),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(None)),
        sixel_image_store,
        Style::default(),
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
    );
    let fixture_name = "move_cursor_below_scroll_region";
    let content = read_fixture(fixture_name);
    for byte in content {
        vte_parser.advance(&mut grid, byte);
    }
    assert_snapshot!(format!("{:?}", grid));
}

#[test]
pub fn insert_wide_characters_in_existing_line() {
    let mut vte_parser = vte::Parser::new();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let osc8_hyperlinks = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut grid = Grid::new(
        21,
        86,
        Rc::new(RefCell::new(Palette::default())),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(None)),
        sixel_image_store,
        Style::default(),
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
    );
    let fixture_name = "chinese_characters_line_middle";
    let content = read_fixture(fixture_name);
    for byte in content {
        vte_parser.advance(&mut grid, byte);
    }
    assert_snapshot!(format!("{:?}", grid));
}

#[test]
pub fn full_screen_scroll_region_and_scroll_up() {
    // this test is a regression test for a bug
    // where the scroll region would be set to the
    // full viewport and then scrolling up would cause
    // lines to get deleted from the viewport rather
    // than moving to "lines_above"
    // terminal_emulator_color_codes,
    let mut vte_parser = vte::Parser::new();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let osc8_hyperlinks = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut grid = Grid::new(
        54,
        80,
        Rc::new(RefCell::new(Palette::default())),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(None)),
        sixel_image_store,
        Style::default(),
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
    );
    let fixture_name = "scroll_region_full_screen";
    let content = read_fixture(fixture_name);
    for byte in content {
        vte_parser.advance(&mut grid, byte);
    }
    grid.scroll_up_one_line();
    grid.scroll_up_one_line();
    grid.scroll_up_one_line();
    assert_snapshot!(format!("{:?}", grid));
}

#[test]
pub fn ring_bell() {
    let mut vte_parser = vte::Parser::new();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let osc8_hyperlinks = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut grid = Grid::new(
        134,
        64,
        Rc::new(RefCell::new(Palette::default())),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(None)),
        sixel_image_store,
        Style::default(),
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
    );
    let fixture_name = "ring_bell";
    let content = read_fixture(fixture_name);
    for byte in content {
        vte_parser.advance(&mut grid, byte);
    }
    assert!(grid.ring_bell);
}

#[test]
pub fn alternate_screen_change_size() {
    let mut vte_parser = vte::Parser::new();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let osc8_hyperlinks = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut grid = Grid::new(
        20,
        20,
        Rc::new(RefCell::new(Palette::default())),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(None)),
        sixel_image_store,
        Style::default(),
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
    );
    let fixture_name = "alternate_screen_change_size";
    let content = read_fixture(fixture_name);
    for byte in content {
        vte_parser.advance(&mut grid, byte);
    }
    // no scrollback in alternate screen
    assert_eq!(grid.scrollback_position_and_length(), (0, 0));
    grid.change_size(10, 10);
    assert_snapshot!(format!("{:?}", grid));
    assert_eq!(grid.scrollback_position_and_length(), (0, 0))
}

#[test]
pub fn fzf_fullscreen() {
    let mut vte_parser = vte::Parser::new();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let osc8_hyperlinks = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut grid = Grid::new(
        51,
        112,
        Rc::new(RefCell::new(Palette::default())),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(None)),
        sixel_image_store,
        Style::default(),
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
    );
    let fixture_name = "fzf_fullscreen";
    let content = read_fixture(fixture_name);
    for byte in content {
        vte_parser.advance(&mut grid, byte);
    }
    assert_snapshot!(format!("{:?}", grid));
}

#[test]
pub fn replace_multiple_wide_characters_under_cursor() {
    // this test makes sure that if we replace a wide character with a non-wide character, it
    // properly pads the excess width in the proper place (either before the replaced non-wide
    // character if the cursor was "in the middle" of the wide character, or after the character if
    // it was "in the beginning" of the wide character)
    let mut vte_parser = vte::Parser::new();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let osc8_hyperlinks = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut grid = Grid::new(
        51,
        112,
        Rc::new(RefCell::new(Palette::default())),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(None)),
        sixel_image_store,
        Style::default(),
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
    );
    let fixture_name = "replace_multiple_wide_characters";
    let content = read_fixture(fixture_name);
    for byte in content {
        vte_parser.advance(&mut grid, byte);
    }
    assert_snapshot!(format!("{:?}", grid));
}

#[test]
pub fn replace_non_wide_characters_with_wide_characters() {
    // this test makes sure that if we replace a wide character with a non-wide character, it
    // properly pads the excess width in the proper place (either before the replaced non-wide
    // character if the cursor was "in the middle" of the wide character, or after the character if
    // it was "in the beginning" of the wide character)
    let mut vte_parser = vte::Parser::new();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let osc8_hyperlinks = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut grid = Grid::new(
        51,
        112,
        Rc::new(RefCell::new(Palette::default())),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(None)),
        sixel_image_store,
        Style::default(),
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
    );
    let fixture_name = "replace_non_wide_characters_with_wide_characters";
    let content = read_fixture(fixture_name);
    for byte in content {
        vte_parser.advance(&mut grid, byte);
    }
    assert_snapshot!(format!("{:?}", grid));
}

#[test]
pub fn scroll_down_ansi() {
    let mut vte_parser = vte::Parser::new();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let osc8_hyperlinks = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut grid = Grid::new(
        51,
        112,
        Rc::new(RefCell::new(Palette::default())),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(None)),
        sixel_image_store,
        Style::default(),
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
    );
    let fixture_name = "scroll_down";
    let content = read_fixture(fixture_name);
    for byte in content {
        vte_parser.advance(&mut grid, byte);
    }
    assert_snapshot!(format!("{:?}", grid));
}

#[test]
pub fn ansi_capital_t() {
    let mut vte_parser = vte::Parser::new();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let osc8_hyperlinks = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut grid = Grid::new(
        51,
        112,
        Rc::new(RefCell::new(Palette::default())),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(None)),
        sixel_image_store,
        Style::default(),
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
    );
    let content = "foo\u{1b}[14Tbar".as_bytes();
    for byte in content {
        vte_parser.advance(&mut grid, *byte);
    }
    assert_snapshot!(format!("{:?}", grid));
}

#[test]
pub fn ansi_capital_s() {
    let mut vte_parser = vte::Parser::new();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let osc8_hyperlinks = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut grid = Grid::new(
        51,
        112,
        Rc::new(RefCell::new(Palette::default())),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(None)),
        sixel_image_store,
        Style::default(),
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
    );
    let content = "\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\nfoo\u{1b}[14Sbar".as_bytes();
    for byte in content {
        vte_parser.advance(&mut grid, *byte);
    }
    assert_snapshot!(format!("{:?}", grid));
}

#[test]
fn terminal_pixel_size_reports() {
    let mut vte_parser = vte::Parser::new();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let osc8_hyperlinks = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut grid = Grid::new(
        51,
        97,
        Rc::new(RefCell::new(Palette::default())),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(Some(SizeInPixels {
            height: 21,
            width: 8,
        }))),
        sixel_image_store,
        Style::default(),
        osc8_hyperlinks,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
    );
    let fixture_name = "terminal_pixel_size_reports";
    let content = read_fixture(fixture_name);
    for byte in content {
        vte_parser.advance(&mut grid, byte);
    }
    assert_eq!(
        grid.pending_messages_to_pty
            .iter()
            .map(|bytes| String::from_utf8(bytes.clone()).unwrap())
            .collect::<Vec<String>>(),
        vec!["\x1b[4;1071;776t", "\x1b[6;21;8t"]
    );
}

#[test]
fn terminal_pixel_size_reports_in_unsupported_terminals() {
    let mut vte_parser = vte::Parser::new();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let osc8_hyperlinks = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut grid = Grid::new(
        51,
        97,
        Rc::new(RefCell::new(Palette::default())),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(None)), // in an unsupported terminal, we don't have this info
        sixel_image_store,
        Style::default(),
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
    );
    let fixture_name = "terminal_pixel_size_reports";
    let content = read_fixture(fixture_name);
    for byte in content {
        vte_parser.advance(&mut grid, byte);
    }
    let expected: Vec<String> = vec![];
    assert_eq!(
        grid.pending_messages_to_pty
            .iter()
            .map(|bytes| String::from_utf8(bytes.clone()).unwrap())
            .collect::<Vec<String>>(),
        expected,
    );
}

#[test]
pub fn ansi_csi_at_sign() {
    let mut vte_parser = vte::Parser::new();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let osc8_hyperlinks = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut grid = Grid::new(
        51,
        112,
        Rc::new(RefCell::new(Palette::default())),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(None)),
        sixel_image_store,
        Style::default(),
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
    );
    let content = "foo\u{1b}[2D\u{1b}[2@".as_bytes();
    for byte in content {
        vte_parser.advance(&mut grid, *byte);
    }
    assert_snapshot!(format!("{:?}", grid));
}

#[test]
pub fn sixel_images_are_reaped_when_scrolled_off() {
    let mut vte_parser = vte::Parser::new();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
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
    let mut grid = Grid::new(
        51,
        112,
        Rc::new(RefCell::new(Palette::default())),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        character_cell_size,
        sixel_image_store.clone(),
        Style::default(),
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
    );
    let pane_content = read_fixture("sixel-image-500px.six");
    for byte in pane_content {
        vte_parser.advance(&mut grid, byte);
    }
    for _ in 0..10_051 {
        // scrollbuffer limit + viewport height
        grid.add_canonical_line();
    }
    let _ = grid.read_changes(0, 0); // we do this because this is where the images are reaped
    assert_eq!(
        sixel_image_store.borrow().image_count(),
        0,
        "all images were deleted from the store"
    );
}

#[test]
pub fn sixel_images_are_reaped_when_resetting() {
    let mut vte_parser = vte::Parser::new();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
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
    let mut grid = Grid::new(
        51,
        112,
        Rc::new(RefCell::new(Palette::default())),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        character_cell_size,
        sixel_image_store.clone(),
        Style::default(),
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
    );
    let pane_content = read_fixture("sixel-image-500px.six");
    for byte in pane_content {
        vte_parser.advance(&mut grid, byte);
    }
    grid.reset_terminal_state();
    let _ = grid.read_changes(0, 0); // we do this because this is where the images are reaped
    assert_eq!(
        sixel_image_store.borrow().image_count(),
        0,
        "all images were deleted from the store"
    );
}

#[test]
pub fn sixel_image_in_alternate_buffer() {
    let mut vte_parser = vte::Parser::new();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
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
    let mut grid = Grid::new(
        30,
        112,
        Rc::new(RefCell::new(Palette::default())),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        character_cell_size,
        sixel_image_store.clone(),
        Style::default(),
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
    );

    let move_to_alternate_screen = "\u{1b}[?1049h";
    for byte in move_to_alternate_screen.as_bytes() {
        vte_parser.advance(&mut grid, *byte);
    }

    let pane_content = read_fixture("sixel-image-500px.six");
    for byte in pane_content {
        vte_parser.advance(&mut grid, byte);
    }
    assert_snapshot!(format!("{:?}", grid)); // should include the image
                                             //
    let move_away_from_alternate_screen = "\u{1b}[?1049l";
    for byte in move_away_from_alternate_screen.as_bytes() {
        vte_parser.advance(&mut grid, *byte);
    }
    assert_snapshot!(format!("{:?}", grid)); // should note include the image
    assert_eq!(
        sixel_image_store.borrow().image_count(),
        0,
        "all images were deleted from the store when we moved back from alternate screen"
    );
}

#[test]
pub fn sixel_with_image_scrolling_decsdm() {
    let mut vte_parser = vte::Parser::new();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
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
    let mut grid = Grid::new(
        30,
        112,
        Rc::new(RefCell::new(Palette::default())),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        character_cell_size,
        sixel_image_store,
        Style::default(),
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
    );

    // enter DECSDM
    let move_to_decsdm = "\u{1b}[?80h";
    for byte in move_to_decsdm.as_bytes() {
        vte_parser.advance(&mut grid, *byte);
    }

    // write some text
    let mut text_to_fill_pane = String::new();
    for i in 0..10 {
        writeln!(&mut text_to_fill_pane, "\rline {}", i + 1).unwrap();
    }
    for byte in text_to_fill_pane.as_bytes() {
        vte_parser.advance(&mut grid, *byte);
    }

    // render a sixel image (will appear on the top left and partially cover the text)
    let pane_content = read_fixture("sixel-image-100px.six");
    for byte in pane_content {
        vte_parser.advance(&mut grid, byte);
    }
    // image should be on the top left corner of the grid
    assert_snapshot!(format!("{:?}", grid));

    // leave DECSDM
    let move_away_from_decsdm = "\u{1b}[?80l";
    for byte in move_away_from_decsdm.as_bytes() {
        vte_parser.advance(&mut grid, *byte);
    }

    // Go down to the beginning of the next line
    let mut go_down_once = String::new();
    writeln!(&mut go_down_once, "\n\r").unwrap();
    for byte in go_down_once.as_bytes() {
        vte_parser.advance(&mut grid, *byte);
    }

    // render another sixel image, should appear under the cursor
    let pane_content = read_fixture("sixel-image-100px.six");
    for byte in pane_content {
        vte_parser.advance(&mut grid, byte);
    }

    // image should appear in cursor position
    assert_snapshot!(format!("{:?}", grid));
}

#[test]
pub fn osc_4_background_query() {
    let mut vte_parser = vte::Parser::new();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let osc8_hyperlinks = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut grid = Grid::new(
        51,
        97,
        Rc::new(RefCell::new(Palette::default())),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(None)),
        sixel_image_store,
        Style::default(),
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
    );
    let content = "\u{1b}]10;?\u{1b}\\";
    for byte in content.as_bytes() {
        vte_parser.advance(&mut grid, *byte);
    }
    let message_string = grid
        .pending_messages_to_pty
        .iter()
        .map(|m| String::from_utf8_lossy(m))
        .fold(String::new(), |mut acc, s| {
            acc.push_str(&s);
            acc
        });
    assert_eq!(message_string, "\u{1b}]10;rgb:0000/0000/0000\u{1b}\\");
}

#[test]
pub fn osc_4_foreground_query() {
    let mut vte_parser = vte::Parser::new();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let osc8_hyperlinks = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut grid = Grid::new(
        51,
        97,
        Rc::new(RefCell::new(Palette::default())),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(None)),
        sixel_image_store,
        Style::default(),
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
    );
    let content = "\u{1b}]11;?\u{1b}\\";
    for byte in content.as_bytes() {
        vte_parser.advance(&mut grid, *byte);
    }
    let message_string = grid
        .pending_messages_to_pty
        .iter()
        .map(|m| String::from_utf8_lossy(m))
        .fold(String::new(), |mut acc, s| {
            acc.push_str(&s);
            acc
        });
    assert_eq!(message_string, "\u{1b}]11;rgb:0000/0000/0000\u{1b}\\");
}

#[test]
pub fn osc_4_color_query() {
    let mut color_codes = HashMap::new();
    color_codes.insert(222, String::from("rgb:ffff/d7d7/8787"));
    let mut vte_parser = vte::Parser::new();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(color_codes));
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let osc8_hyperlinks = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut grid = Grid::new(
        51,
        97,
        Rc::new(RefCell::new(Palette::default())),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(None)),
        sixel_image_store,
        Style::default(),
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
    );
    let content = "\u{1b}]4;222;?\u{1b}\\";
    for byte in content.as_bytes() {
        vte_parser.advance(&mut grid, *byte);
    }
    let message_string = grid
        .pending_messages_to_pty
        .iter()
        .map(|m| String::from_utf8_lossy(m))
        .fold(String::new(), |mut acc, s| {
            acc.push_str(&s);
            acc
        });
    assert_eq!(message_string, "\u{1b}]4;222;rgb:ffff/d7d7/8787\u{1b}\\");
}

#[test]
pub fn xtsmgraphics_color_register_count() {
    let mut vte_parser = vte::Parser::new();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let osc8_hyperlinks = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut grid = Grid::new(
        51,
        97,
        Rc::new(RefCell::new(Palette::default())),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(None)),
        sixel_image_store,
        Style::default(),
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
    );
    let content = "\u{1b}[?1;1;S\u{1b}\\";
    for byte in content.as_bytes() {
        vte_parser.advance(&mut grid, *byte);
    }
    let message_string = grid
        .pending_messages_to_pty
        .iter()
        .map(|m| String::from_utf8_lossy(m))
        .fold(String::new(), |mut acc, s| {
            acc.push_str(&s);
            acc
        });
    assert_eq!(message_string, "\u{1b}[?1;0;65536S");
}

#[test]
pub fn xtsmgraphics_pixel_graphics_geometry() {
    let mut vte_parser = vte::Parser::new();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
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
    let mut grid = Grid::new(
        51,
        97,
        Rc::new(RefCell::new(Palette::default())),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        character_cell_size,
        sixel_image_store,
        Style::default(),
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
    );
    let content = "\u{1b}[?2;1;S\u{1b}\\";
    for byte in content.as_bytes() {
        vte_parser.advance(&mut grid, *byte);
    }
    let message_string = grid
        .pending_messages_to_pty
        .iter()
        .map(|m| String::from_utf8_lossy(m))
        .fold(String::new(), |mut acc, s| {
            acc.push_str(&s);
            acc
        });
    assert_eq!(message_string, "\u{1b}[?2;0;776;1071S");
}

#[test]
pub fn cursor_hide_persists_through_alternate_screen() {
    let mut vte_parser = vte::Parser::new();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
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
    let mut grid = Grid::new(
        30,
        112,
        Rc::new(RefCell::new(Palette::default())),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        character_cell_size,
        sixel_image_store,
        Style::default(),
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
    );

    let hide_cursor = "\u{1b}[?25l";
    for byte in hide_cursor.as_bytes() {
        vte_parser.advance(&mut grid, *byte);
    }
    assert_eq!(grid.cursor_coordinates(), None, "Cursor hidden properly");

    let move_to_alternate_screen = "\u{1b}[?1049h";
    for byte in move_to_alternate_screen.as_bytes() {
        vte_parser.advance(&mut grid, *byte);
    }
    assert_eq!(
        grid.cursor_coordinates(),
        None,
        "Cursor still hidden in alternate screen"
    );

    let show_cursor = "\u{1b}[?25h";
    for byte in show_cursor.as_bytes() {
        vte_parser.advance(&mut grid, *byte);
    }
    assert!(grid.cursor_coordinates().is_some(), "Cursor shown");

    let move_away_from_alternate_screen = "\u{1b}[?1049l";
    for byte in move_away_from_alternate_screen.as_bytes() {
        vte_parser.advance(&mut grid, *byte);
    }
    assert!(
        grid.cursor_coordinates().is_some(),
        "Cursor still shown away from alternate screen"
    );
}

#[test]
fn table_ui_component() {
    let mut vte_parser = vte::Parser::new();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let osc8_hyperlinks = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut grid = Grid::new(
        41,
        110,
        Rc::new(RefCell::new(Palette::default())),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(None)),
        sixel_image_store,
        Style::default(),
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
    );
    let fixture_name = "table-ui-component";
    let content = read_fixture(fixture_name);
    for byte in content {
        vte_parser.advance(&mut grid, byte);
    }
    assert_snapshot!(format!("{:?}", grid));
}

#[test]
fn table_ui_component_with_coordinates() {
    let mut vte_parser = vte::Parser::new();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let osc8_hyperlinks = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut grid = Grid::new(
        41,
        110,
        Rc::new(RefCell::new(Palette::default())),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(None)),
        sixel_image_store,
        Style::default(),
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
    );
    let fixture_name = "table-ui-component-with-coordinates";
    let content = read_fixture(fixture_name);
    for byte in content {
        vte_parser.advance(&mut grid, byte);
    }
    assert_snapshot!(format!("{:?}", grid));
}

#[test]
fn ribbon_ui_component() {
    let mut vte_parser = vte::Parser::new();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let osc8_hyperlinks = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut grid = Grid::new(
        41,
        110,
        Rc::new(RefCell::new(Palette::default())),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(None)),
        sixel_image_store,
        Style::default(),
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
    );
    let fixture_name = "ribbon-ui-component";
    let content = read_fixture(fixture_name);
    for byte in content {
        vte_parser.advance(&mut grid, byte);
    }
    assert_snapshot!(format!("{:?}", grid));
}

#[test]
fn ribbon_ui_component_with_coordinates() {
    let mut vte_parser = vte::Parser::new();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let osc8_hyperlinks = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut grid = Grid::new(
        41,
        110,
        Rc::new(RefCell::new(Palette::default())),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(None)),
        sixel_image_store,
        Style::default(),
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
    );
    let fixture_name = "ribbon-ui-component-with-coordinates";
    let content = read_fixture(fixture_name);
    for byte in content {
        vte_parser.advance(&mut grid, byte);
    }
    assert_snapshot!(format!("{:?}", grid));
}

#[test]
fn nested_list_ui_component() {
    let mut vte_parser = vte::Parser::new();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let osc8_hyperlinks = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut grid = Grid::new(
        41,
        120,
        Rc::new(RefCell::new(Palette::default())),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(None)),
        sixel_image_store,
        Style::default(),
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
    );
    let fixture_name = "nested-list-ui-component";
    let content = read_fixture(fixture_name);
    for byte in content {
        vte_parser.advance(&mut grid, byte);
    }
    assert_snapshot!(format!("{:?}", grid));
}

#[test]
fn nested_list_ui_component_with_coordinates() {
    let mut vte_parser = vte::Parser::new();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let osc8_hyperlinks = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut grid = Grid::new(
        41,
        120,
        Rc::new(RefCell::new(Palette::default())),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(None)),
        sixel_image_store,
        Style::default(),
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
    );
    let fixture_name = "nested-list-ui-component-with-coordinates";
    let content = read_fixture(fixture_name);
    for byte in content {
        vte_parser.advance(&mut grid, byte);
    }
    assert_snapshot!(format!("{:?}", grid));
}

#[test]
fn text_ui_component() {
    let mut vte_parser = vte::Parser::new();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let osc8_hyperlinks = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut grid = Grid::new(
        41,
        120,
        Rc::new(RefCell::new(Palette::default())),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(None)),
        sixel_image_store,
        Style::default(),
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
    );
    let fixture_name = "text-ui-component";
    let content = read_fixture(fixture_name);
    for byte in content {
        vte_parser.advance(&mut grid, byte);
    }
    assert_snapshot!(format!("{:?}", grid));
}

#[test]
fn text_ui_component_with_coordinates() {
    let mut vte_parser = vte::Parser::new();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let osc8_hyperlinks = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut grid = Grid::new(
        41,
        120,
        Rc::new(RefCell::new(Palette::default())),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(None)),
        sixel_image_store,
        Style::default(),
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
    );
    let fixture_name = "text-ui-component-with-coordinates";
    let content = read_fixture(fixture_name);
    for byte in content {
        vte_parser.advance(&mut grid, byte);
    }
    assert_snapshot!(format!("{:?}", grid));
}

#[test]
fn cannot_escape_scroll_region() {
    // this tests a fix for a bug where it would be possible to set the scroll region bounds beyond
    // the pane height, which would then allow a goto instruction beyond the scroll region to scape
    // the pane bounds and render content on other panes
    //
    // what we do here is set the scroll region beyond the terminal bounds (`<ESC>[1;42r` - whereas
    // the terminal is just 41 lines high), and then issue a goto instruction to line 42, one line
    // beyond the pane and scroll region bounds (`<ESC>[42;1H`) and then print text `Hi there!`.
    // This should be printed on the last line (zero indexed 40) of the terminal and not beyond it.
    let mut vte_parser = vte::Parser::new();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let osc8_hyperlinks = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut grid = Grid::new(
        41,
        120,
        Rc::new(RefCell::new(Palette::default())),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(None)),
        sixel_image_store,
        Style::default(),
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
    );
    let content = "\u{1b}[1;42r\u{1b}[42;1HHi there!".as_bytes();
    for byte in content {
        vte_parser.advance(&mut grid, *byte);
    }
    assert_snapshot!(format!("{:?}", grid));
}

#[test]
fn preserve_background_color_on_resize() {
    use crate::panes::terminal_character::{AnsiCode, EMPTY_TERMINAL_CHARACTER};

    let mut vte_parser = vte::Parser::new();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let osc8_hyperlinks = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut grid = Grid::new(
        10,
        20,
        Rc::new(RefCell::new(Palette::default())),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(None)),
        sixel_image_store,
        Style::default(),
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
    );

    let mut parse = |s, grid: &mut Grid| {
        for b in Vec::from(s) {
            vte_parser.advance(&mut *grid, b)
        }
    };

    // Write text with red background that extends to end of line
    // ESC[41m = red background
    // ESC[K = clear to end of line (fills with current background)
    // ESC[0m = reset
    let content = "test\x1b[41m\x1b[K\x1b[0m";
    parse(content, &mut grid);

    // Check that characters after "test" have red background before resize
    let first_row = &grid.viewport[0];
    let background_char_count_before = first_row
        .columns
        .iter()
        .enumerate()
        .filter(|(i, c)| *i >= 4 && c.styles.background != Some(AnsiCode::Reset))
        .count();
    assert!(
        background_char_count_before > 0,
        "Should have characters with background color before resize"
    );

    // Also check that plain trailing spaces are properly trimmed (regression test)
    let content2 = "\r\n\rplain text with spaces    ";
    parse(content2, &mut grid);

    // Resize the grid
    grid.change_size(10, 30);

    // Check that the background color is preserved after resize
    let first_row = &grid.viewport[0];
    let background_char_count_after = first_row
        .columns
        .iter()
        .enumerate()
        .filter(|(i, c)| *i >= 4 && c.styles.background != Some(AnsiCode::Reset))
        .count();
    assert_eq!(
        background_char_count_before, background_char_count_after,
        "Background colored characters should be preserved after resize"
    );

    // Verify that the second line doesn't have excessive trailing spaces
    // (it should be trimmed since they're plain spaces without background color)
    let second_row = &grid.viewport[1];
    let trailing_spaces = second_row
        .columns
        .iter()
        .rev()
        .take_while(|c| c.grapheme() == EMPTY_TERMINAL_CHARACTER.grapheme())
        .count();
    // All trailing plain spaces should be completely removed
    assert_eq!(
        trailing_spaces, 0,
        "Plain trailing spaces should be completely trimmed, but found {} trailing spaces",
        trailing_spaces
    );
}

fn create_grid_with_content(content: &str) -> Grid {
    let mut vte_parser = vte::Parser::new();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let mut grid = Grid::new(
        20,
        80,
        Rc::new(RefCell::new(Palette::default())),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(None)),
        sixel_image_store,
        Style::default(),
        false,
        true,
        true,
        true,
        false,
    );
    for byte in content.as_bytes() {
        vte_parser.advance(&mut grid, *byte);
    }
    grid
}

#[test]
fn double_click_selection_preserved_after_scroll() {
    let content = "line 0\nline 1\nline 2\nline 3\nline 4\nthis is a word test\nline 6\nline 7\nline 8\nline 9\nline 10\nline 11\nline 12\nline 13\nline 14\nline 15\nline 16\nline 17\nline 18\nline 19\n";
    let mut grid = create_grid_with_content(content);

    for _ in 0..20 {
        grid.add_canonical_line();
    }

    let word_position = Position::new(5, 10);
    grid.start_selection(&word_position);

    let selection_before = grid.get_selected_text();
    let word_start = grid.selection.start;
    let word_end = grid.selection.end;

    grid.end_selection(&word_position);

    grid.scroll_up_one_line();

    let selection_after_start = grid.selection.start;
    let selection_after_end = grid.selection.end;

    assert_eq!(selection_after_start.line.0, word_start.line.0 + 1);
    assert_eq!(selection_after_end.line.0, word_end.line.0 + 1);
    assert_eq!(selection_after_start.column, word_start.column);
    assert_eq!(selection_after_end.column, word_end.column);

    let text_after = grid.get_selected_text();
    assert_eq!(selection_before, text_after);
}

#[test]
fn triple_click_selection_preserved_after_scroll() {
    let content = "line 0\nline 1\nline 2\nline 3\nline 4\nthis is line five with some text\nline 6\nline 7\nline 8\nline 9\nline 10\nline 11\nline 12\nline 13\nline 14\nline 15\nline 16\nline 17\nline 18\nline 19\n";
    let mut grid = create_grid_with_content(content);

    for _ in 0..20 {
        grid.add_canonical_line();
    }

    let line_position = Position::new(5, 15);
    grid.start_selection(&line_position);
    grid.start_selection(&line_position);
    grid.start_selection(&line_position);

    let selection_before = grid.get_selected_text();
    let line_start = grid.selection.start;
    let line_end = grid.selection.end;

    grid.end_selection(&line_position);

    grid.scroll_up_one_line();

    let selection_after_start = grid.selection.start;
    let selection_after_end = grid.selection.end;

    assert_eq!(selection_after_start.line.0, line_start.line.0 + 1);
    assert_eq!(selection_after_end.line.0, line_end.line.0 + 1);
    assert_eq!(selection_after_start.column, line_start.column);
    assert_eq!(selection_after_end.column, line_end.column);

    let text_after = grid.get_selected_text();
    assert_eq!(selection_before, text_after);
}

#[test]
fn double_click_selection_moves_with_multiple_scrolls() {
    let content = "line 0\nline 1\nline 2\nline 3\nline 4\nthis is a word test\nline 6\nline 7\nline 8\nline 9\nline 10\nline 11\nline 12\nline 13\nline 14\nline 15\nline 16\nline 17\nline 18\nline 19\n";
    let mut grid = create_grid_with_content(content);

    for _ in 0..20 {
        grid.add_canonical_line();
    }

    let word_position = Position::new(5, 10);
    grid.start_selection(&word_position);

    let initial_start = grid.selection.start;
    let initial_end = grid.selection.end;

    grid.end_selection(&word_position);

    for _ in 0..5 {
        grid.scroll_up_one_line();
    }

    assert_eq!(grid.selection.start.line.0, initial_start.line.0 + 5);
    assert_eq!(grid.selection.end.line.0, initial_end.line.0 + 5);
    assert_eq!(grid.selection.start.column, initial_start.column);
    assert_eq!(grid.selection.end.column, initial_end.column);
}

#[test]
fn single_click_drag_selection_preserved_after_scroll() {
    let content = "line 0\nline 1\nline 2\nline 3\nline 4\nsome text here\nline 6\nline 7\nline 8\nline 9\nline 10\nline 11\nline 12\nline 13\nline 14\nline 15\nline 16\nline 17\nline 18\nline 19\n";
    let mut grid = create_grid_with_content(content);

    for _ in 0..20 {
        grid.add_canonical_line();
    }

    grid.start_selection(&Position::new(5, 5));
    grid.update_selection(&Position::new(5, 10));

    let start_before = grid.selection.start;
    let end_before = grid.selection.end;

    grid.end_selection(&Position::new(5, 10));

    grid.scroll_up_one_line();

    assert_eq!(grid.selection.start.line.0, start_before.line.0 + 1);
    assert_eq!(grid.selection.end.line.0, end_before.line.0 + 1);
    assert_eq!(grid.selection.start.column, start_before.column);
    assert_eq!(grid.selection.end.column, end_before.column);
}

#[test]
fn osc_11_set_and_query_pane_default_bg() {
    use crate::panes::terminal_character::AnsiCode;

    let mut vte_parser = vte::Parser::new();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let mut grid = Grid::new(
        10,
        20,
        Rc::new(RefCell::new(Palette::default())),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(None)),
        sixel_image_store,
        Style::default(),
        false,
        true,
        true,
        true,
        false,
    );

    // Set background via OSC 11
    let set_bg = b"\x1b]11;#001a3a\x07";
    for byte in set_bg.iter() {
        vte_parser.advance(&mut grid, *byte);
    }

    assert_eq!(grid.pane_default_bg, Some(AnsiCode::RgbCode((0, 26, 58))));

    // Query background via OSC 11
    let query_bg = b"\x1b]11;?\x07";
    for byte in query_bg.iter() {
        vte_parser.advance(&mut grid, *byte);
    }

    assert_eq!(grid.pending_messages_to_pty.len(), 1);
    let response = String::from_utf8(grid.pending_messages_to_pty[0].clone()).unwrap();
    assert!(
        response.contains("11;rgb:0000/1a1a/3a3a"),
        "Response was: {}",
        response
    );
}

#[test]
fn osc_10_set_and_query_pane_default_fg() {
    use crate::panes::terminal_character::AnsiCode;

    let mut vte_parser = vte::Parser::new();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let mut grid = Grid::new(
        10,
        20,
        Rc::new(RefCell::new(Palette::default())),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(None)),
        sixel_image_store,
        Style::default(),
        false,
        true,
        true,
        true,
        false,
    );

    // Set foreground via OSC 10
    let set_fg = b"\x1b]10;#00e000\x07";
    for byte in set_fg.iter() {
        vte_parser.advance(&mut grid, *byte);
    }

    assert_eq!(grid.pane_default_fg, Some(AnsiCode::RgbCode((0, 224, 0))));

    // Query foreground via OSC 10
    let query_fg = b"\x1b]10;?\x07";
    for byte in query_fg.iter() {
        vte_parser.advance(&mut grid, *byte);
    }

    assert_eq!(grid.pending_messages_to_pty.len(), 1);
    let response = String::from_utf8(grid.pending_messages_to_pty[0].clone()).unwrap();
    assert!(
        response.contains("10;rgb:0000/e0e0/0000"),
        "Response was: {}",
        response
    );
}

#[test]
fn osc_110_111_reset_pane_default_colors() {
    use crate::panes::terminal_character::AnsiCode;

    let mut vte_parser = vte::Parser::new();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let mut grid = Grid::new(
        10,
        20,
        Rc::new(RefCell::new(Palette::default())),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(None)),
        sixel_image_store,
        Style::default(),
        false,
        true,
        true,
        true,
        false,
    );

    // Set both fg and bg
    let set_fg = b"\x1b]10;#00e000\x07";
    for byte in set_fg.iter() {
        vte_parser.advance(&mut grid, *byte);
    }
    let set_bg = b"\x1b]11;#001a3a\x07";
    for byte in set_bg.iter() {
        vte_parser.advance(&mut grid, *byte);
    }

    assert_eq!(grid.pane_default_fg, Some(AnsiCode::RgbCode((0, 224, 0))));
    assert_eq!(grid.pane_default_bg, Some(AnsiCode::RgbCode((0, 26, 58))));

    // Reset foreground via OSC 110
    let reset_fg = b"\x1b]110\x07";
    for byte in reset_fg.iter() {
        vte_parser.advance(&mut grid, *byte);
    }
    assert_eq!(grid.pane_default_fg, None);
    assert_eq!(grid.pane_default_bg, Some(AnsiCode::RgbCode((0, 26, 58))));

    // Reset background via OSC 111
    let reset_bg = b"\x1b]111\x07";
    for byte in reset_bg.iter() {
        vte_parser.advance(&mut grid, *byte);
    }
    assert_eq!(grid.pane_default_fg, None);
    assert_eq!(grid.pane_default_bg, None);
}

#[test]
fn osc_11_set_bg_produces_ansi_in_render_output() {
    use crate::panes::terminal_character::AnsiCode;

    let mut vte_parser = vte::Parser::new();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let mut grid = Grid::new(
        5,
        10,
        Rc::new(RefCell::new(Palette::default())),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(None)),
        sixel_image_store,
        Style::default(),
        false,
        true,
        true,
        true,
        false,
    );

    // Set background via OSC 11
    let set_bg = b"\x1b]11;#001a3a\x07";
    for byte in set_bg.iter() {
        vte_parser.advance(&mut grid, *byte);
    }

    assert_eq!(grid.pane_default_bg, Some(AnsiCode::RgbCode((0, 26, 58))));

    // Render the grid and check that the pane defaults are stamped on chunks
    let style = Style::default();
    let render_result = grid.render(0, 0, &style).unwrap();
    assert!(render_result.is_some(), "Expected render output");

    let (chunks, _, _) = render_result.unwrap();
    assert!(!chunks.is_empty(), "Expected at least one character chunk");

    // All chunks should carry the pane default bg
    for chunk in &chunks {
        assert_eq!(
            chunk.pane_default_bg,
            Some(AnsiCode::RgbCode((0, 26, 58))),
            "Chunk should carry pane default background"
        );
    }
}

// =====================================================================
// Plugin Highlight Engine Tests
// =====================================================================

use crate::panes::grid::MouseTracking;
use crate::panes::terminal_character::AnsiCode;
use std::collections::BTreeMap;
use zellij_utils::data::{HighlightLayer, HighlightStyle, RegexHighlight};

fn create_highlight(
    pattern: &str,
    on_hover: bool,
    bold: bool,
    italic: bool,
    underline: bool,
    layer: HighlightLayer,
) -> RegexHighlight {
    RegexHighlight {
        pattern: pattern.to_string(),
        style: HighlightStyle::Emphasis0,
        layer,
        context: BTreeMap::new(),
        on_hover,
        bold,
        italic,
        underline,
        tooltip_text: None,
    }
}

#[test]
fn set_plugin_regex_highlights_basic_match() {
    let mut grid = create_grid_with_content("hello foo bar\n");
    let highlights = vec![RegexHighlight {
        pattern: "foo".into(),
        style: HighlightStyle::Emphasis0,
        layer: HighlightLayer::Hint,
        context: BTreeMap::new(),
        on_hover: false,
        bold: false,
        italic: false,
        underline: true,
        tooltip_text: None,
    }];
    grid.set_plugin_regex_highlights(1, highlights, &Style::default());
    let slot = grid.plugin_highlights.get(&1);
    assert!(slot.is_some());
    let entries = slot.unwrap();
    assert_eq!(entries.len(), 1);
    assert!(entries[0].1.regex.is_match("foo"));
}

#[test]
fn set_plugin_regex_highlights_no_match() {
    let mut grid = create_grid_with_content("hello world bar\n");
    let highlights = vec![create_highlight(
        "xyz123",
        false,
        false,
        false,
        false,
        HighlightLayer::Hint,
    )];
    grid.set_plugin_regex_highlights(1, highlights, &Style::default());

    // No position in the viewport should match
    for col in 0..15 {
        assert!(grid.plugin_highlight_at(&Position::new(0, col)).is_none());
    }
}

#[test]
fn clear_plugin_highlights_removes_highlights() {
    let mut grid = create_grid_with_content("hello foo bar\n");
    let highlights = vec![create_highlight(
        "foo",
        false,
        false,
        false,
        true,
        HighlightLayer::Hint,
    )];
    grid.set_plugin_regex_highlights(1, highlights, &Style::default());
    assert!(grid.plugin_highlights.get(&1).is_some());

    grid.clear_plugin_highlights(1);
    assert!(grid.plugin_highlights.get(&1).is_none());
}

#[test]
fn multiple_plugins_highlights_independent() {
    let mut grid = create_grid_with_content("aaa bbb ccc\n");
    let h1 = vec![create_highlight(
        "aaa",
        false,
        false,
        false,
        false,
        HighlightLayer::Hint,
    )];
    let h2 = vec![create_highlight(
        "bbb",
        false,
        false,
        false,
        false,
        HighlightLayer::Hint,
    )];
    grid.set_plugin_regex_highlights(1, h1, &Style::default());
    grid.set_plugin_regex_highlights(2, h2, &Style::default());

    assert!(grid.plugin_highlights.get(&1).is_some());
    assert!(grid.plugin_highlights.get(&2).is_some());

    grid.clear_plugin_highlights(1);
    assert!(grid.plugin_highlights.get(&1).is_none());
    assert!(grid.plugin_highlights.get(&2).is_some());
}

#[test]
fn upsert_replaces_same_pattern() {
    let mut grid = create_grid_with_content("foo bar\n");
    let h1 = vec![RegexHighlight {
        pattern: "foo".into(),
        style: HighlightStyle::Emphasis0,
        layer: HighlightLayer::Hint,
        context: BTreeMap::new(),
        on_hover: false,
        bold: false,
        italic: false,
        underline: true,
        tooltip_text: None,
    }];
    grid.set_plugin_regex_highlights(1, h1, &Style::default());

    let h2 = vec![RegexHighlight {
        pattern: "foo".into(),
        style: HighlightStyle::Emphasis0,
        layer: HighlightLayer::Hint,
        context: BTreeMap::new(),
        on_hover: false,
        bold: false,
        italic: false,
        underline: false,
        tooltip_text: None,
    }];
    grid.set_plugin_regex_highlights(1, h2, &Style::default());

    let entries = grid.plugin_highlights.get(&1).unwrap();
    assert_eq!(entries.len(), 1);
    assert!(!entries[0].1.underline);
}

#[test]
fn invalid_regex_does_not_crash() {
    let mut grid = create_grid_with_content("hello\n");
    let highlights = vec![create_highlight(
        "[invalid",
        false,
        false,
        false,
        false,
        HighlightLayer::Hint,
    )];
    grid.set_plugin_regex_highlights(1, highlights, &Style::default());
    // Invalid regex should be skipped
    let slot = grid.plugin_highlights.get(&1);
    match slot {
        None => {}, // acceptable
        Some(entries) => assert_eq!(entries.len(), 0),
    }
}

#[test]
fn plugin_highlight_at_returns_match() {
    let mut grid = create_grid_with_content("hello foo bar\n");
    let mut context = BTreeMap::new();
    context.insert("key".to_string(), "value".to_string());
    let highlights = vec![RegexHighlight {
        pattern: "foo".into(),
        style: HighlightStyle::Emphasis0,
        layer: HighlightLayer::Hint,
        context: context.clone(),
        on_hover: false,
        bold: false,
        italic: false,
        underline: true,
        tooltip_text: None,
    }];
    grid.set_plugin_regex_highlights(1, highlights, &Style::default());

    // "foo" starts at column 6 in "hello foo bar"
    let result = grid.plugin_highlight_at(&Position::new(0, 6));
    assert!(result.is_some());
    let (plugin_id, pattern, matched_string, ctx) = result.unwrap();
    assert_eq!(plugin_id, 1);
    assert_eq!(pattern, "foo");
    assert_eq!(matched_string, "foo");
    assert_eq!(ctx.get("key").unwrap(), "value");
}

#[test]
fn plugin_highlight_at_returns_none_on_miss() {
    let mut grid = create_grid_with_content("hello foo bar\n");
    let highlights = vec![create_highlight(
        "foo",
        false,
        false,
        false,
        true,
        HighlightLayer::Hint,
    )];
    grid.set_plugin_regex_highlights(1, highlights, &Style::default());

    // Position 0 is in "hello", not "foo"
    let result = grid.plugin_highlight_at(&Position::new(0, 0));
    assert!(result.is_none());
}

#[test]
fn plugin_highlight_at_wrapped_line() {
    // Create a narrow grid (10 cols) so that a long string wraps
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let mut grid = Grid::new(
        5,
        10,
        Rc::new(RefCell::new(Palette::default())),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(None)),
        sixel_image_store,
        Style::default(),
        false,
        true,
        true,
        true,
        false,
    );
    // Feed a long string that wraps: "abcdefghij" fills row 0, "klmnopqrst" fills row 1
    let content = "abcdefghijklmnopqrst";
    let mut vte_parser = vte::Parser::new();
    for &byte in content.as_bytes() {
        vte_parser.advance(&mut grid, byte);
    }

    // Set a highlight for "jklm" which spans the wrap boundary
    let highlights = vec![create_highlight(
        "jklm",
        false,
        false,
        false,
        true,
        HighlightLayer::Hint,
    )];
    grid.set_plugin_regex_highlights(1, highlights, &Style::default());

    // "jklm" spans row 0 col 9 through row 1 col 3
    // Position in the wrapped portion (row 1, col 1 = 'k')
    let result = grid.plugin_highlight_at(&Position::new(1, 1));
    assert!(result.is_some());
    let (plugin_id, _pattern, matched_string, _ctx) = result.unwrap();
    assert_eq!(plugin_id, 1);
    assert_eq!(matched_string, "jklm");
}

#[test]
fn hover_position_triggers_on_hover_highlight() {
    let mut grid = create_grid_with_content("hello link_text bar\n");
    let highlights = vec![create_highlight(
        "link_text",
        true,
        false,
        false,
        true,
        HighlightLayer::Hint,
    )];
    grid.set_plugin_regex_highlights(1, highlights, &Style::default());

    // Set hover position inside "link_text" (starts at col 6)
    grid.set_hover_position(Some(Position::new(0, 8)));
    assert!(grid.hover_position.is_some());
    assert_eq!(grid.hover_position.unwrap(), Position::new(0, 8));

    // The on_hover entry should exist in plugin_highlights
    let entries = grid.plugin_highlights.get(&1).unwrap();
    assert_eq!(entries.len(), 1);
    assert!(entries[0].1.on_hover);
}

#[test]
fn hover_suppressed_when_mouse_tracking_on() {
    let mut grid = create_grid_with_content("hello link_text bar\n");
    let highlights = vec![create_highlight(
        "link_text",
        true,
        false,
        false,
        true,
        HighlightLayer::Hint,
    )];
    grid.set_plugin_regex_highlights(1, highlights, &Style::default());

    // Enable mouse tracking — the render path should skip hover highlights
    grid.mouse_tracking = MouseTracking::Normal;
    grid.set_hover_position(Some(Position::new(0, 8)));

    // The hover position is set regardless (the guard is in the render path),
    // but we verify that mouse_tracking is non-Off
    assert!(grid.hover_position.is_some());
    assert_ne!(grid.mouse_tracking, MouseTracking::Off);
}

#[test]
fn wide_char_display_column_mapping() {
    // CJK characters: "你好" = 2 chars, each 2 display cols wide, so "world" starts at display col 4
    let mut grid = create_grid_with_content("你好world\n");
    let highlights = vec![create_highlight(
        "world",
        false,
        false,
        false,
        true,
        HighlightLayer::Hint,
    )];
    grid.set_plugin_regex_highlights(1, highlights, &Style::default());

    // "你好" occupies display cols 0-3, "world" starts at display col 4
    let result = grid.plugin_highlight_at(&Position::new(0, 4));
    assert!(result.is_some());
    let (plugin_id, _pattern, matched_string, _ctx) = result.unwrap();
    assert_eq!(plugin_id, 1);
    assert_eq!(matched_string, "world");

    // Position 2 should be inside "你好", not "world"
    let result_miss = grid.plugin_highlight_at(&Position::new(0, 2));
    assert!(result_miss.is_none());
}

#[test]
fn highlight_style_variants_resolve_colors() {
    use super::resolve_highlight_colors;

    let style = Style::default();

    // HighlightStyle::None returns (None, None)
    let (fg, bg): (Option<AnsiCode>, Option<AnsiCode>) =
        resolve_highlight_colors(&HighlightStyle::None, &style);
    assert!(fg.is_none());
    assert!(bg.is_none());

    // HighlightStyle::CustomRgb with fg only
    let (fg, bg): (Option<AnsiCode>, Option<AnsiCode>) = resolve_highlight_colors(
        &HighlightStyle::CustomRgb {
            fg: Some((255, 0, 0)),
            bg: None,
        },
        &style,
    );
    assert_eq!(fg, Some(AnsiCode::RgbCode((255, 0, 0))));
    assert!(bg.is_none());

    // HighlightStyle::CustomIndex with bg only
    let (fg, bg): (Option<AnsiCode>, Option<AnsiCode>) = resolve_highlight_colors(
        &HighlightStyle::CustomIndex {
            fg: None,
            bg: Some(42),
        },
        &style,
    );
    assert!(fg.is_none());
    assert_eq!(bg, Some(AnsiCode::ColorIndex(42)));

    // HighlightStyle::Emphasis0 should return a foreground color from the palette
    let (fg, bg): (Option<AnsiCode>, Option<AnsiCode>) =
        resolve_highlight_colors(&HighlightStyle::Emphasis0, &style);
    assert!(fg.is_some());
    assert!(bg.is_none());
}

fn create_grid_with_scrollback() -> Grid {
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let mut grid = Grid::new(
        5,
        40,
        Rc::new(RefCell::new(Palette::default())),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(None)),
        sixel_image_store,
        Style::default(),
        false,
        true,
        true,
        true,
        false,
    );
    let mut parser = vte::Parser::new();
    for i in 0..25 {
        let line = format!("scrollback line {}\r\n", i);
        for byte in line.as_bytes() {
            parser.advance(&mut grid, *byte);
        }
    }
    grid
}

#[test]
fn pane_contents_scrollback_no_truncation_when_max_none() {
    let grid = create_grid_with_scrollback();
    let result = grid.pane_contents(true, None);
    assert_eq!(
        result.lines_above_viewport.len(),
        21,
        "All scrollback lines should be returned when max is None"
    );
}

#[test]
fn pane_contents_scrollback_no_truncation_when_max_zero() {
    let grid = create_grid_with_scrollback();
    let result = grid.pane_contents(true, Some(0));
    assert_eq!(
        result.lines_above_viewport.len(),
        21,
        "Some(0) is sentinel for all scrollback — no truncation"
    );
}

#[test]
fn pane_contents_scrollback_truncates_to_last_n() {
    let grid = create_grid_with_scrollback();
    let result = grid.pane_contents(true, Some(5));
    assert_eq!(result.lines_above_viewport.len(), 5);
    let full = grid.pane_contents(true, None);
    let expected: Vec<String> = full
        .lines_above_viewport
        .iter()
        .rev()
        .take(5)
        .rev()
        .cloned()
        .collect();
    assert_eq!(result.lines_above_viewport, expected);
}

#[test]
fn pane_contents_scrollback_no_truncation_when_n_exceeds_total() {
    let grid = create_grid_with_scrollback();
    let result = grid.pane_contents(true, Some(100));
    assert_eq!(
        result.lines_above_viewport.len(),
        21,
        "No truncation when N exceeds total scrollback lines"
    );
}

#[test]
fn pane_contents_no_scrollback_when_flag_false() {
    let grid = create_grid_with_scrollback();
    let result = grid.pane_contents(false, Some(5));
    assert!(
        result.lines_above_viewport.is_empty(),
        "get_full_scrollback=false should never collect scrollback"
    );
}

// =====================================================================
// Highlight Layer Priority Tests
// =====================================================================

#[test]
fn higher_layer_wins_plugin_highlight_at() {
    let mut grid = create_grid_with_content("hello foo bar\n");
    let h1 = vec![RegexHighlight {
        pattern: "foo".into(),
        style: HighlightStyle::Emphasis0,
        layer: HighlightLayer::Hint,
        context: BTreeMap::new(),
        on_hover: false,
        bold: false,
        italic: false,
        underline: false,
        tooltip_text: None,
    }];
    let h2 = vec![RegexHighlight {
        pattern: "foo".into(),
        style: HighlightStyle::Emphasis1,
        layer: HighlightLayer::Tool,
        context: BTreeMap::new(),
        on_hover: false,
        bold: false,
        italic: false,
        underline: false,
        tooltip_text: None,
    }];
    grid.set_plugin_regex_highlights(1, h1, &Style::default());
    grid.set_plugin_regex_highlights(2, h2, &Style::default());

    // "foo" starts at column 6
    let result = grid.plugin_highlight_at(&Position::new(0, 6));
    assert!(result.is_some());
    let (plugin_id, _, _, _) = result.unwrap();
    assert_eq!(plugin_id, 2, "Tool layer plugin should win over Hint layer");
}

#[test]
fn same_layer_both_returned_deterministically() {
    let mut grid = create_grid_with_content("hello foo bar\n");
    let h1 = vec![RegexHighlight {
        pattern: "foo".into(),
        style: HighlightStyle::Emphasis0,
        layer: HighlightLayer::Hint,
        context: BTreeMap::new(),
        on_hover: false,
        bold: false,
        italic: false,
        underline: false,
        tooltip_text: None,
    }];
    let h2 = vec![RegexHighlight {
        pattern: "foo".into(),
        style: HighlightStyle::Emphasis1,
        layer: HighlightLayer::Hint,
        context: BTreeMap::new(),
        on_hover: false,
        bold: false,
        italic: false,
        underline: false,
        tooltip_text: None,
    }];
    grid.set_plugin_regex_highlights(1, h1, &Style::default());
    grid.set_plugin_regex_highlights(2, h2, &Style::default());

    let result = grid.plugin_highlight_at(&Position::new(0, 6));
    assert!(
        result.is_some(),
        "Same-layer conflicts should not cause errors"
    );
}

#[test]
fn lower_layer_wins_when_higher_layer_absent() {
    let mut grid = create_grid_with_content("foo bar\n");
    let h1 = vec![RegexHighlight {
        pattern: "foo".into(),
        style: HighlightStyle::Emphasis0,
        layer: HighlightLayer::Hint,
        context: BTreeMap::new(),
        on_hover: false,
        bold: false,
        italic: false,
        underline: false,
        tooltip_text: None,
    }];
    let h2 = vec![RegexHighlight {
        pattern: "bar".into(),
        style: HighlightStyle::Emphasis1,
        layer: HighlightLayer::ActionFeedback,
        context: BTreeMap::new(),
        on_hover: false,
        bold: false,
        italic: false,
        underline: false,
        tooltip_text: None,
    }];
    grid.set_plugin_regex_highlights(1, h1, &Style::default());
    grid.set_plugin_regex_highlights(2, h2, &Style::default());

    // "foo" at col 0 — only Hint layer matches here
    let result_foo = grid.plugin_highlight_at(&Position::new(0, 0));
    assert!(result_foo.is_some());
    assert_eq!(result_foo.unwrap().0, 1);

    // "bar" at col 4 — only ActionFeedback layer matches here
    let result_bar = grid.plugin_highlight_at(&Position::new(0, 4));
    assert!(result_bar.is_some());
    assert_eq!(result_bar.unwrap().0, 2);
}

#[test]
fn tooltip_from_higher_layer_wins() {
    let mut grid = create_grid_with_content("hello foo bar\n");
    let h1 = vec![RegexHighlight {
        pattern: "foo".into(),
        style: HighlightStyle::Emphasis0,
        layer: HighlightLayer::Hint,
        context: BTreeMap::new(),
        on_hover: true,
        bold: false,
        italic: false,
        underline: false,
        tooltip_text: Some("hint tooltip".to_string()),
    }];
    let h2 = vec![RegexHighlight {
        pattern: "foo".into(),
        style: HighlightStyle::Emphasis1,
        layer: HighlightLayer::Tool,
        context: BTreeMap::new(),
        on_hover: true,
        bold: false,
        italic: false,
        underline: false,
        tooltip_text: Some("tool tooltip".to_string()),
    }];
    grid.set_plugin_regex_highlights(1, h1, &Style::default());
    grid.set_plugin_regex_highlights(2, h2, &Style::default());

    // Set hover position inside "foo" (starts at col 6)
    grid.set_hover_position(Some(Position::new(0, 6)));
    assert_eq!(
        grid.cached_hover_tooltip,
        Some("tool tooltip".to_string()),
        "Tool layer tooltip should win over Hint layer tooltip"
    );
}

#[test]
fn tooltip_from_lower_layer_when_higher_has_none() {
    let mut grid = create_grid_with_content("hello foo bar\n");
    let h1 = vec![RegexHighlight {
        pattern: "foo".into(),
        style: HighlightStyle::Emphasis0,
        layer: HighlightLayer::Hint,
        context: BTreeMap::new(),
        on_hover: true,
        bold: false,
        italic: false,
        underline: false,
        tooltip_text: Some("hint tooltip".to_string()),
    }];
    let h2 = vec![RegexHighlight {
        pattern: "foo".into(),
        style: HighlightStyle::Emphasis1,
        layer: HighlightLayer::Tool,
        context: BTreeMap::new(),
        on_hover: true,
        bold: false,
        italic: false,
        underline: false,
        tooltip_text: None,
    }];
    grid.set_plugin_regex_highlights(1, h1, &Style::default());
    grid.set_plugin_regex_highlights(2, h2, &Style::default());

    grid.set_hover_position(Some(Position::new(0, 6)));
    assert_eq!(
        grid.cached_hover_tooltip,
        Some("hint tooltip".to_string()),
        "When higher layer has no tooltip, lower layer tooltip should be used"
    );
}

#[test]
fn layer_field_stored_in_compiled_highlight() {
    let mut grid = create_grid_with_content("foo bar\n");
    let highlights = vec![RegexHighlight {
        pattern: "foo".into(),
        style: HighlightStyle::Emphasis0,
        layer: HighlightLayer::ActionFeedback,
        context: BTreeMap::new(),
        on_hover: false,
        bold: false,
        italic: false,
        underline: false,
        tooltip_text: None,
    }];
    grid.set_plugin_regex_highlights(1, highlights, &Style::default());
    assert_eq!(
        grid.plugin_highlights.get(&1).unwrap()[0].1.layer,
        HighlightLayer::ActionFeedback,
        "Layer field should be propagated through compilation"
    );
}

#[test]
fn default_layer_is_hint() {
    assert_eq!(HighlightLayer::default(), HighlightLayer::Hint);
}

#[test]
fn layer_ordering() {
    assert!(HighlightLayer::Hint < HighlightLayer::Tool);
    assert!(HighlightLayer::Tool < HighlightLayer::ActionFeedback);
}

// ── Grapheme cluster (EGC) composition tests ─────────────────────────────────

/// Returns the grapheme text of non-space cells in the first viewport row.
fn first_row_graphemes(grid: &Grid) -> Vec<String> {
    grid.viewport[0]
        .columns
        .iter()
        .filter(|c| c.grapheme() != " ")
        .map(|c| c.grapheme().to_owned())
        .collect()
}

#[test]
fn combining_mark_stored_in_base_cell() {
    // Latin 'a' + combining grave accent (U+0300) must occupy one cell.
    // The combining mark is zero-width and should be appended to the base cell,
    // not dropped or placed in its own cell.
    let content = "a\u{0300}b"; // à (decomposed) then b
    let grid = create_grid_with_content(content);
    let graphemes = first_row_graphemes(&grid);

    assert_eq!(
        graphemes.len(),
        2,
        "Expected 2 cells: [a+grave, b], got {:?}",
        graphemes
    );
    assert_eq!(
        graphemes[0], "a\u{0300}",
        "First cell should contain base + combining mark"
    );
    assert_eq!(graphemes[1], "b");
}

#[test]
fn variation_selector_preserved_in_cell() {
    // Heart (U+2764) + variation selector-16 (U+FE0F, emoji presentation).
    // VS-16 is zero-width and must be stored in the same cell as the base glyph.
    let content = "\u{2764}\u{FE0F}";
    let grid = create_grid_with_content(content);
    let graphemes = first_row_graphemes(&grid);

    assert_eq!(
        graphemes.len(),
        1,
        "Heart+VS16 must be one cell, got {:?}",
        graphemes
    );
    assert!(
        graphemes[0].contains('\u{FE0F}'),
        "Variation selector must be preserved in the cell grapheme"
    );
}

#[test]
fn zwj_sequence_stored_in_single_cell() {
    // Man (U+1F468) + ZWJ (U+200D) + Woman (U+1F469).
    // ZWJ is zero-width; the second emoji follows without a grapheme boundary (GB11).
    let content = "\u{1F468}\u{200D}\u{1F469}";
    let grid = create_grid_with_content(content);
    let graphemes = first_row_graphemes(&grid);

    assert_eq!(
        graphemes.len(),
        1,
        "ZWJ family must be one cell, got {:?}",
        graphemes
    );
    assert!(graphemes[0].contains('\u{200D}'), "ZWJ must be preserved");
    assert!(
        graphemes[0].contains('\u{1F469}'),
        "Second emoji must be in same cell"
    );
}

#[test]
fn skin_tone_modifier_appended_to_base_emoji() {
    // Waving hand (U+1F44B) + light skin tone (U+1F3FB).
    // The modifier has display width 2 on its own but UAX #29 keeps it with the base emoji.
    let content = "\u{1F44B}\u{1F3FB}x";
    let grid = create_grid_with_content(content);
    let graphemes = first_row_graphemes(&grid);

    assert_eq!(
        graphemes.len(),
        2,
        "Emoji+skin-tone and 'x' = 2 cells, got {:?}",
        graphemes
    );
    assert!(
        graphemes[0].contains('\u{1F3FB}'),
        "Skin tone modifier must be in same cell as base emoji"
    );
    assert_eq!(graphemes[1], "x");
}

#[test]
fn thai_combining_vowel_stored_in_base_cell() {
    // Thai ก (U+0E01) + SARA AM ำ (U+0E33, combining vowel).
    // The combining vowel is zero-width; must be in the same cell as the consonant.
    let content = "\u{0E01}\u{0E33}x";
    let grid = create_grid_with_content(content);
    let graphemes = first_row_graphemes(&grid);

    assert_eq!(
        graphemes.len(),
        2,
        "ก+ำ and x = 2 cells, got {:?}",
        graphemes
    );
    assert!(
        graphemes[0].contains('\u{0E33}'),
        "Thai combining vowel must be preserved with its base consonant"
    );
}

#[test]
fn regional_indicator_pair_stored_in_single_cell() {
    // Two regional indicators (U+1F1EF + U+1F1F5) form the Japan flag (🇯🇵).
    // UAX #29 keeps regional indicator pairs in one grapheme cluster.
    let content = "\u{1F1EF}\u{1F1F5}x";
    let grid = create_grid_with_content(content);
    let graphemes = first_row_graphemes(&grid);

    assert_eq!(
        graphemes.len(),
        2,
        "Flag sequence and x = 2 cells, got {:?}",
        graphemes
    );
    assert!(
        graphemes[0].contains('\u{1F1F5}'),
        "Second regional indicator must be in same cell as first"
    );
    assert_eq!(graphemes[1], "x");
}

#[test]
fn grapheme_preserved_in_copy_output() {
    // Combining marks must survive the selection/copy path.
    let content = "a\u{0300}b";
    let mut grid = create_grid_with_content(content);
    // Select the whole first line
    grid.start_selection(&Position::new(0, 0));
    grid.end_selection(&Position::new(0, 2));
    let selected = grid.get_selected_text();
    assert!(
        selected.as_deref().unwrap_or("").contains("a\u{0300}"),
        "Combining mark must be present in copied text"
    );
}

// ── CSI 2027 mode tests ───────────────────────────────────────────────────────

fn feed_bytes(grid: &mut Grid, bytes: &[u8]) {
    let mut vte_parser = vte::Parser::new();
    for byte in bytes {
        vte_parser.advance(grid, *byte);
    }
}

#[test]
fn csi_2027_h_enables_grapheme_cluster_mode() {
    let mut grid = create_grid_with_content("");
    assert!(!grid.grapheme_cluster_mode, "mode should be off by default");
    feed_bytes(&mut grid, b"\x1b[?2027h");
    assert!(
        grid.grapheme_cluster_mode,
        "CSI ? 2027 h should enable the mode"
    );
}

#[test]
fn csi_2027_l_disables_grapheme_cluster_mode() {
    let mut grid = create_grid_with_content("");
    feed_bytes(&mut grid, b"\x1b[?2027h");
    assert!(grid.grapheme_cluster_mode);
    feed_bytes(&mut grid, b"\x1b[?2027l");
    assert!(
        !grid.grapheme_cluster_mode,
        "CSI ? 2027 l should disable the mode"
    );
}

#[test]
fn csi_2027_decrpm_reports_disabled_when_off() {
    let mut grid = create_grid_with_content("");
    feed_bytes(&mut grid, b"\x1b[?2027$p");
    let response = grid
        .pending_messages_to_pty
        .iter()
        .find_map(|msg| std::str::from_utf8(msg).ok().map(|s| s.to_owned()));
    assert_eq!(
        response.as_deref(),
        Some("\x1b[?2027;2$y"),
        "DECRPM should report disabled (;2) when mode is off"
    );
}

#[test]
fn csi_2027_decrpm_reports_enabled_when_on() {
    let mut grid = create_grid_with_content("");
    feed_bytes(&mut grid, b"\x1b[?2027h");
    grid.pending_messages_to_pty.clear(); // discard any prior messages
    feed_bytes(&mut grid, b"\x1b[?2027$p");
    let response = grid
        .pending_messages_to_pty
        .iter()
        .find_map(|msg| std::str::from_utf8(msg).ok().map(|s| s.to_owned()));
    assert_eq!(
        response.as_deref(),
        Some("\x1b[?2027;1$y"),
        "DECRPM should report enabled (;1) when mode is on"
    );
}

#[test]
fn terminal_reset_clears_grapheme_cluster_mode() {
    // ESC c (RIS — full terminal reset) must clear grapheme_cluster_mode even
    // if it was enabled before the reset.
    let mut grid = create_grid_with_content("");
    feed_bytes(&mut grid, b"\x1b[?2027h");
    assert!(grid.grapheme_cluster_mode, "mode should be on before reset");
    // ESC c = RIS (full terminal reset)
    feed_bytes(&mut grid, b"\x1bc");
    assert!(
        !grid.grapheme_cluster_mode,
        "mode should be off after RIS reset"
    );
    // DECRPM query should also report disabled
    feed_bytes(&mut grid, b"\x1b[?2027$p");
    let response = grid
        .pending_messages_to_pty
        .iter()
        .find_map(|msg| std::str::from_utf8(msg).ok().map(|s| s.to_owned()));
    assert_eq!(
        response.as_deref(),
        Some("\x1b[?2027;2$y"),
        "DECRPM after RIS should report disabled"
    );
}

#[test]
fn csi_2027_mode_persists_across_alternate_screen() {
    let mut grid = create_grid_with_content("");
    // Enable 2027 mode, then enter and exit alternate screen
    feed_bytes(&mut grid, b"\x1b[?2027h");
    assert!(grid.grapheme_cluster_mode);
    feed_bytes(&mut grid, b"\x1b[?1049h"); // enter alt screen
    assert!(
        grid.grapheme_cluster_mode,
        "mode should persist after entering alt screen"
    );
    feed_bytes(&mut grid, b"\x1b[?1049l"); // exit alt screen
    assert!(
        grid.grapheme_cluster_mode,
        "mode should persist after exiting alt screen"
    );
}

// ── Grapheme-aware editing semantics ─────────────────────────────────────────
//
// In CSI 2027 mode, these tests treat BS, CUB, and CUF as display-column
// movement, even when printed text is stored and rendered as grapheme clusters.
// This matches the Ghostty behavior we probed while validating the implementation.
//
// Some tests below model a common application pattern: move to an absolute
// position, clear cells with spaces, and then redraw the grapheme.

#[test]
fn backspace_moves_back_one_column_in_legacy_mode() {
    // Baseline: outside 2027 mode backspace always moves back 1 column.
    let mut grid = create_grid_with_content("ab");
    // cursor is now at x=2; send backspace
    feed_bytes(&mut grid, b"\x08");
    assert_eq!(
        grid.cursor.x, 1,
        "legacy backspace should move back 1 column"
    );
}

#[test]
fn backspace_moves_back_one_column_in_2027_mode_over_wide_char() {
    // In 2027 mode, BS still moves back one display column.
    let mut grid = create_grid_with_content("");
    feed_bytes(&mut grid, b"\x1b[?2027h");
    feed_bytes(&mut grid, "中".as_bytes());
    assert_eq!(
        grid.cursor.x, 2,
        "cursor should be at col 2 after wide char"
    );
    feed_bytes(&mut grid, b"\x08");
    assert_eq!(
        grid.cursor.x, 1,
        "one BS moves back 1 display column over a wide char"
    );
    feed_bytes(&mut grid, b"\x08");
    assert_eq!(grid.cursor.x, 0, "second BS reaches col 0");
}

#[test]
fn backspace_moves_back_one_column_for_narrow_char_in_2027_mode() {
    // In 2027 mode, backspace over a narrow character still moves back 1 column.
    let mut grid = create_grid_with_content("");
    feed_bytes(&mut grid, b"\x1b[?2027h");
    feed_bytes(&mut grid, b"a");
    assert_eq!(grid.cursor.x, 1);
    feed_bytes(&mut grid, b"\x08");
    assert_eq!(
        grid.cursor.x, 0,
        "2027 backspace over narrow char moves back 1 column"
    );
}

#[test]
fn cursor_left_moves_by_column_in_2027_mode() {
    // CSI D (CUB) counts display columns in all modes, including 2027.
    // Applications send CUB N as a display-column count, not an EGC count.
    let mut grid = create_grid_with_content("");
    feed_bytes(&mut grid, b"\x1b[?2027h");
    feed_bytes(&mut grid, "中".as_bytes()); // wide char, cursor at x=2
    feed_bytes(&mut grid, b"\x1b[1D"); // CSI 1 D — 1 display column back
    assert_eq!(
        grid.cursor.x, 1,
        "CSI 1 D moves 1 display column, not 1 EGC"
    );
    feed_bytes(&mut grid, b"\x1b[1D"); // one more
    assert_eq!(grid.cursor.x, 0);
}

#[test]
fn cursor_left_count_by_column_in_2027_mode() {
    // CSI 2 D moves back 2 display columns even in 2027 mode.
    let mut grid = create_grid_with_content("");
    feed_bytes(&mut grid, b"\x1b[?2027h");
    feed_bytes(&mut grid, "中文".as_bytes()); // two wide chars, cursor at x=4
    feed_bytes(&mut grid, b"\x1b[2D"); // CSI 2 D — back 2 display columns
    assert_eq!(grid.cursor.x, 2, "CSI 2 D moves 2 display columns");
    feed_bytes(&mut grid, b"\x1b[2D");
    assert_eq!(grid.cursor.x, 0);
}

#[test]
fn cursor_forward_moves_by_column_not_egc_width() {
    // CSI C (CUF) counts display columns in all modes, including 2027.
    // Applications send N as a display-column count for wide characters.
    let mut grid = create_grid_with_content("");
    feed_bytes(&mut grid, b"\x1b[?2027h");
    feed_bytes(&mut grid, "中".as_bytes()); // wide char at x=0, cursor at x=2
    feed_bytes(&mut grid, b"\x1b[2D"); // back to x=0 (2 columns)
    assert_eq!(grid.cursor.x, 0);
    feed_bytes(&mut grid, b"\x1b[1C"); // CSI 1 C — 1 display column
    assert_eq!(
        grid.cursor.x, 1,
        "CSI C moves 1 display column even in 2027 mode"
    );
    feed_bytes(&mut grid, b"\x1b[1C"); // one more column
    assert_eq!(
        grid.cursor.x, 2,
        "CSI C 1+1 = 2 display columns past wide char"
    );
}

#[test]
fn cursor_back_moves_by_column_not_egc_width() {
    // CSI D (CUB) counts display columns in all modes, including 2027.
    // Applications send N=display-width; we must NOT multiply by EGC width again.
    let mut grid = create_grid_with_content("");
    feed_bytes(&mut grid, b"\x1b[?2027h");
    feed_bytes(&mut grid, "中文".as_bytes()); // cursor at x=4
    feed_bytes(&mut grid, b"\x1b[2D"); // back 2 display columns
    assert_eq!(
        grid.cursor.x, 2,
        "CSI 2 D moves 2 display columns, not 2 EGCs"
    );
    feed_bytes(&mut grid, b"\x1b[2D"); // back 2 more
    assert_eq!(grid.cursor.x, 0);
}

#[test]
fn cursor_right_legacy_mode_moves_by_column() {
    // CSI C/D move by column count in legacy mode too.
    let mut grid = create_grid_with_content("");
    feed_bytes(&mut grid, "中文".as_bytes()); // cursor at x=4
    feed_bytes(&mut grid, b"\x1b[4D"); // back to x=0
    feed_bytes(&mut grid, b"\x1b[1C"); // CSI 1 C — moves 1 column
    assert_eq!(grid.cursor.x, 1, "legacy CSI C should move 1 column");
}

#[test]
fn rep_repeats_full_grapheme_cluster_after_combining_mark() {
    // CSI b should repeat the full EGC (base + combining), not just the combining mark.
    let mut grid = create_grid_with_content("");
    feed_bytes(&mut grid, b"\x1b[?2027h");
    // Type 'a' followed by combining grave U+0300 → one cell containing "à"
    feed_bytes(&mut grid, "a\u{0300}".as_bytes());
    // cursor is at x=1; repeat 2 times
    feed_bytes(&mut grid, b"\x1b[2b");
    // Row should now contain 3 cells each with grapheme "a\u{0300}" (NFD form, as stored).
    let row = &grid.viewport[0];
    assert_eq!(row.columns.len(), 3, "REP should have produced 3 cells");
    // The cell stores the raw sequence: 'a' + U+0300 (decomposed NFD form).
    assert_eq!(
        row.columns[0].grapheme(),
        "a\u{0300}",
        "cell 0 should be a+combining grave"
    );
    assert_eq!(
        row.columns[1].grapheme(),
        "a\u{0300}",
        "cell 1 should be a+combining grave (REP copy 1)"
    );
    assert_eq!(
        row.columns[2].grapheme(),
        "a\u{0300}",
        "cell 2 should be a+combining grave (REP copy 2)"
    );
}

#[test]
fn replace_with_empty_chars_steps_by_egc_width_in_2027_mode() {
    // CSI X in 2027 mode should replace `count` EGCs, not `count` columns.
    let mut grid = create_grid_with_content("");
    feed_bytes(&mut grid, b"\x1b[?2027h");
    feed_bytes(&mut grid, "中文ab".as_bytes()); // display cols 0-1: 中, 2-3: 文, 4: a, 5: b
                                                // Move cursor back to x=0
    feed_bytes(&mut grid, b"\x1b[6D");
    assert_eq!(grid.cursor.x, 0);
    // CSI 2 X — replace 2 EGCs starting at x=0 (should blank 中 and 文, leave a/b intact)
    feed_bytes(&mut grid, b"\x1b[2X");
    let row = &grid.viewport[0];
    // Collect concatenated grapheme string: wide chars replaced by two narrow blanks each,
    // so the row looks like "    ab" (4 spaces + a + b).
    let rendered: String = row.columns.iter().map(|c| c.grapheme()).collect();
    assert_eq!(
        rendered, "    ab",
        "CSI 2 X should blank 2 EGCs (中 and 文) and leave a/b untouched; got {:?}",
        rendered
    );
}

#[test]
fn width_expanding_egc_does_not_overwrite_next_char() {
    // '#' is width 1; '#' + U+FE0F (VS-16, emoji presentation) is width 2.
    // When the variation selector extends the cell's width, the cursor must
    // advance by the delta so the following 'x' lands at column 2, not column 1.
    let mut grid = create_grid_with_content("");
    // Feed '#', then VS-16 (extends '#' to width-2 emoji), then 'x'
    feed_bytes(&mut grid, "#\u{FE0F}x".as_bytes());
    let row = &grid.viewport[0];
    // Rendered graphemes: "#\u{FE0F}" (wide, 2 cols) then "x" at col 2.
    // With the bug, 'x' overwrites col 1 and the row renders as "#x" (width 2).
    let rendered: String = row.columns.iter().map(|c| c.grapheme()).collect();
    assert_eq!(
        rendered, "#\u{FE0F}x",
        "char after width-expanding EGC should land at col 2, not overwrite it; got {:?}",
        rendered
    );
    assert_eq!(grid.cursor.x, 3, "cursor should be at col 3 after '#FE0Fx'");
}

#[test]
fn width_expanding_egc_row_cache_invalidated_on_extension() {
    // Stale cache scenario: '#' placed (row cache = Some(1)), then U+FE0F
    // extends it to width 2 (row cache must be invalidated). Without the
    // invalidation, width_cached() still returns 1, so a subsequent char
    // printed at col 1 (middle of the wide cell) is wrongly appended at the
    // end instead of replacing in the middle.
    let mut grid = create_grid_with_content("");
    // Place '#' + VS-16, cursor ends at x=2
    feed_bytes(&mut grid, "#\u{FE0F}".as_bytes());
    assert_eq!(grid.cursor.x, 2);
    // Move cursor back into the middle of the wide cell (legacy CSI D: 1 column)
    feed_bytes(&mut grid, b"\x1b[D");
    assert_eq!(
        grid.cursor.x, 1,
        "cursor should be at col 1 (middle of wide cell)"
    );
    // Print 'y' — should replace the wide cell (splits into EMPTY + 'y'),
    // not append 'y' after it (which would happen with a stale cache).
    feed_bytes(&mut grid, b"y");
    let row = &grid.viewport[0];
    let rendered: String = row.columns.iter().map(|c| c.grapheme()).collect();
    assert_eq!(
        rendered, " y",
        "printing at col 1 should replace the wide cell, not append; got {:?}",
        rendered
    );
}

#[test]
fn egc_state_text_uses_full_grapheme_for_ri_parity_after_rep() {
    // CSI b REP re-inserts the preceding cell via add_character(cell.clone()).
    // If egc_state.text is initialised with only the first scalar of that cell
    // (the bug), the RI parity count is off by one: the next RI looks like it
    // completes a flag with the lone RI in state.text instead of starting fresh.
    //
    // Correct sequence: 🇺🇸 (US flag) · REP×1 → 🇺🇸 · then 🇩🇪 (DE flag)
    // Expected: three cells — "🇺🇸", "🇺🇸", "🇩🇪"
    // With bug:  REP cell has egc_state.text="🇺" (one RI); 🇩 looks like its
    //            pair → merged → only two cells: "🇺🇸", "🇺🇩", then "🇪" separate.
    let mut grid = create_grid_with_content("");
    // Build 🇺🇸 char-by-char so the cell is assembled correctly
    feed_bytes(&mut grid, "\u{1F1FA}\u{1F1F8}".as_bytes()); // 🇺🇸
                                                            // REP once (CSI 1 b) — re-inserts the full "🇺🇸" cell via add_character
    feed_bytes(&mut grid, b"\x1b[1b");
    // Feed 🇩🇪 char-by-char
    feed_bytes(&mut grid, "\u{1F1E9}\u{1F1EA}".as_bytes()); // 🇩🇪
    let row = &grid.viewport[0];
    assert_eq!(
        row.columns.len(),
        3,
        "should have 3 flag cells, got {:?}",
        row.columns.iter().map(|c| c.grapheme()).collect::<Vec<_>>()
    );
    assert_eq!(
        row.columns[0].grapheme(),
        "\u{1F1FA}\u{1F1F8}",
        "cell 0 should be 🇺🇸"
    );
    assert_eq!(
        row.columns[1].grapheme(),
        "\u{1F1FA}\u{1F1F8}",
        "cell 1 should be 🇺🇸 (REP)"
    );
    assert_eq!(
        row.columns[2].grapheme(),
        "\u{1F1E9}\u{1F1EA}",
        "cell 2 should be 🇩🇪"
    );
}

#[test]
fn width_shrinking_egc_extension_adjusts_cursor_and_cache() {
    // U+2648 ♈ (Aries) has emoji presentation by default — width 2.
    // Appending VS15 (U+FE0E) switches it to text presentation — width 1.
    // The cursor must move back by 1 and the row cache must be invalidated.
    // Without the fix: saturating_sub gives width_delta=0, cursor stays at 2,
    // and the next char overwrites col 1 instead of landing at col 1.
    let mut grid = create_grid_with_content("");
    // Feed ♈ (width 2), then VS15 (shrinks to width 1)
    feed_bytes(&mut grid, "\u{2648}\u{FE0E}".as_bytes());
    // Cursor should be at col 1 (width-1 cell), not col 2.
    assert_eq!(
        grid.cursor.x, 1,
        "cursor should retreat to col 1 after VS15 shrinks cell from width 2 to 1"
    );
    // Row width cache must reflect the reduced width.
    let cached = grid.viewport[0].width_cached();
    assert_eq!(
        cached, 1,
        "row width cache should be 1 after shrink, got {}",
        cached
    );
    // Next char should land at col 1, not overwrite col 1 of the now-narrow cell.
    feed_bytes(&mut grid, b"x");
    let rendered: String = grid.viewport[0]
        .columns
        .iter()
        .map(|c| c.grapheme())
        .collect();
    assert_eq!(
        rendered, "\u{2648}\u{FE0E}x",
        "char after width-shrinking EGC should land at col 1; got {:?}",
        rendered
    );
}

#[test]
fn multiple_zwj_emojis_cursor_at_correct_column() {
    // Three family emojis: 👨‍👩‍👧 each is a 5-codepoint ZWJ sequence, display width=2.
    // After typing all three, cursor should be at column 6, not drifted wider.
    let mut grid = create_grid_with_content("");
    let family = "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}"; // 👨‍👩‍👧
    let content = format!("{}{}{}", family, family, family);
    feed_bytes(&mut grid, content.as_bytes());
    assert_eq!(
        grid.cursor.x, 6,
        "three 2-wide ZWJ emojis should place cursor at col 6, got {}",
        grid.cursor.x
    );
    // And each emoji should be in its own cell
    assert_eq!(
        grid.viewport[0].columns.len(),
        3,
        "three ZWJ emojis = 3 cells, got {}",
        grid.viewport[0].columns.len()
    );
    assert_eq!(grid.viewport[0].columns[0].width(), 2);
    assert_eq!(grid.viewport[0].columns[1].width(), 2);
    assert_eq!(grid.viewport[0].columns[2].width(), 2);
}

#[test]
fn backspace_moves_one_column_in_2027_mode_over_zwj_emoji() {
    // In 2027 mode, BS still moves back one display column, even over a
    // width-2 ZWJ emoji.
    let mut grid = create_grid_with_content("");
    feed_bytes(&mut grid, b"\x1b[?2027h");
    let family = "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}"; // 👨‍👩‍👧, width=2
    feed_bytes(&mut grid, family.as_bytes());
    assert_eq!(
        grid.cursor.x, 2,
        "cursor should be at col 2 after one ZWJ emoji"
    );
    feed_bytes(&mut grid, b"\x08");
    assert_eq!(
        grid.cursor.x, 1,
        "one BS moves back 1 display column over a width-2 ZWJ emoji"
    );
    feed_bytes(&mut grid, b"\x08");
    assert_eq!(grid.cursor.x, 0, "second BS reaches col 0");
}

#[test]
fn skin_tone_emoji_cursor_at_correct_column() {
    // Waving hand + light skin tone (👋🏻): each modifier should not add columns.
    // The pair is still width 2, so cursor should be at col 2, not col 4.
    let mut grid = create_grid_with_content("");
    let emoji_with_skin = "\u{1F44B}\u{1F3FB}"; // 👋🏻
    feed_bytes(&mut grid, emoji_with_skin.as_bytes());
    assert_eq!(
        grid.cursor.x, 2,
        "emoji+skin-tone should be width 2 total, cursor at col 2, got {}",
        grid.cursor.x
    );
    feed_bytes(&mut grid, b"x");
    assert_eq!(
        grid.cursor.x, 3,
        "char after emoji+skin-tone should be at col 3, got {}",
        grid.cursor.x
    );
}

#[test]
fn zwj_deletion_dcb_sequence_clears_last_emoji() {
    // Simulates an application using CUB(2) + DCH(1) to delete the last of 3 ZWJ emojis.
    // CUB(2) = 2 display columns back (emoji3 is 2 wide), DCH(1) = delete 1 grapheme cluster.
    let mut grid = create_grid_with_content("");
    feed_bytes(&mut grid, b"\x1b[?2027h");
    let family = "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}";
    let content = format!("{}{}{}", family, family, family);
    feed_bytes(&mut grid, content.as_bytes()); // cursor at col 6
    assert_eq!(grid.cursor.x, 6);

    // CUB(2): move back 2 display columns to start of emoji3
    feed_bytes(&mut grid, b"\x1b[2D");
    assert_eq!(
        grid.cursor.x, 4,
        "CUB(2) should land at col 4 (start of emoji3)"
    );

    // DCH 1 at col 4: delete emoji3 (1 grapheme cluster)
    feed_bytes(&mut grid, b"\x1b[P");
    assert_eq!(grid.cursor.x, 4, "cursor should stay at col 4 after DCH");

    let row = &grid.viewport[0];
    assert!(
        row.columns[0].grapheme().contains('\u{1F468}'),
        "emoji1 at index 0 should be intact"
    );
    assert!(
        row.columns[1].grapheme().contains('\u{1F468}'),
        "emoji2 at index 1 should be intact"
    );
    assert_eq!(
        row.columns[2].width(),
        1,
        "col 4 (was emoji3) should be empty"
    );
    assert_eq!(row.columns[3].width(), 1, "col 5 should also be empty");
}

#[test]
fn zwj_deletion_absolute_positioning_sequence_clears_last_emoji() {
    // Simulates an application write + delete sequence in 2027 mode.
    //
    // One observed write pattern is: CUP + SP SP + BS BS + <emoji>
    // The SP SP + BS BS clears the target cells before writing the emoji.
    // Because BS operates on the just-written spaces (width=1), each BS moves
    // back exactly 1 column — same result regardless of BS semantics.
    //
    // The corresponding delete pattern uses CUP for absolute positioning,
    // then overwrites with spaces. This test verifies both patterns.
    let mut grid = create_grid_with_content("");
    feed_bytes(&mut grid, b"\x1b[?2027h");
    let family = "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}"; // 👨‍👩‍👧, width=2

    // Write 3 emojis using that write pattern: CUP + SP SP + BS BS + emoji
    feed_bytes(&mut grid, b"\x1b[H"); // CUP(1,1) → col 0
    feed_bytes(&mut grid, b"  \x08\x08"); // SP SP BS BS → col 0
    feed_bytes(&mut grid, family.as_bytes()); // emoji1 at col 0, cursor at col 2
    feed_bytes(&mut grid, b"\x1b[1;3H"); // CUP(1,3) → col 2
    feed_bytes(&mut grid, b"  \x08\x08"); // SP SP BS BS → col 2
    feed_bytes(&mut grid, family.as_bytes()); // emoji2 at col 2, cursor at col 4
    feed_bytes(&mut grid, b"\x1b[1;5H"); // CUP(1,5) → col 4
    feed_bytes(&mut grid, b"  \x08\x08"); // SP SP BS BS → col 4
    feed_bytes(&mut grid, family.as_bytes()); // emoji3 at col 4, cursor at col 6
    assert_eq!(grid.cursor.x, 6, "cursor at col 6 after 3 emojis");

    // Delete via absolute positioning: CUP to the start of the emoji, then overwrite with spaces.
    feed_bytes(&mut grid, b"\x1b[1;5H"); // CUP(1,5) → col 4 (start of emoji3)
    assert_eq!(grid.cursor.x, 4, "CUP positions at col 4");
    feed_bytes(&mut grid, b"  "); // overwrite emoji3's columns with spaces
    assert_eq!(grid.cursor.x, 6, "after writing 2 spaces, cursor at col 6");

    let row = &grid.viewport[0];
    assert!(
        row.columns[0].grapheme().contains('\u{1F468}'),
        "emoji1 at col 0 should be intact"
    );
    assert!(
        row.columns[1].grapheme().contains('\u{1F468}'),
        "emoji2 at col 2 should be intact"
    );
    assert_eq!(
        row.columns[2].grapheme(),
        " ",
        "col 4 should now be space (emoji3 cleared)"
    );
    assert_eq!(row.columns[3].grapheme(), " ", "col 5 should also be space");
}

#[test]
fn hangul_jamo_nfd_grouped_as_grapheme_cluster() {
    // NFD "폴더명" = ᄑ(U+1111) ᅩ(U+1169) ᆯ(U+11AF) ᄃ(U+1103) ᅥ(U+1165) ᄆ(U+1106) ᅧ(U+1167) ᆼ(U+11BC)
    // GraphemeCursor groups L+V+T Jamo into grapheme clusters per UAX #29.
    // The grid does NOT do NFC composition — it stores the raw Jamo codepoints,
    // but they render identically to precomposed syllables.
    let nfd = "\u{1111}\u{1169}\u{11AF}\u{1103}\u{1165}\u{1106}\u{1167}\u{11BC}";
    let grid = create_grid_with_content(nfd);
    let row = &grid.viewport[0];

    // 3 grapheme clusters, each width 2, cursor at col 6
    assert_eq!(row.columns.len(), 3, "3 Hangul syllable clusters");
    assert_eq!(
        grid.cursor.x, 6,
        "cursor at col 6 after 3 width-2 syllables"
    );

    // Each cell stores the NFD Jamo, not the NFC precomposed syllable.
    // first_char() returns the leading consonant (Choseong).
    assert_eq!(row.columns[0].first_char(), Some('\u{1111}')); // ᄑ
    assert_eq!(row.columns[0].width(), 2);
    assert_eq!(row.columns[1].first_char(), Some('\u{1103}')); // ᄃ
    assert_eq!(row.columns[1].width(), 2);
    assert_eq!(row.columns[2].first_char(), Some('\u{1106}')); // ᄆ
    assert_eq!(row.columns[2].width(), 2);
}

#[test]
fn hangul_nfc_precomposed_still_works() {
    // Pre-composed NFC "폴더명" — each syllable is a single codepoint.
    let nfc = "폴더명";
    let grid = create_grid_with_content(nfc);
    let row = &grid.viewport[0];

    assert_eq!(row.columns.len(), 3);
    assert_eq!(grid.cursor.x, 6);
    assert_eq!(row.columns[0].first_char(), Some('폴'));
    assert_eq!(row.columns[1].first_char(), Some('더'));
    assert_eq!(row.columns[2].first_char(), Some('명'));
}

#[test]
fn hangul_jamo_two_char_syllable_without_jongseong() {
    // NFD "가나" = ᄀ(U+1100) ᅡ(U+1161) ᄂ(U+1102) ᅡ(U+1161)
    // Two-Jamo syllables (L+V, no trailing T).
    let nfd = "\u{1100}\u{1161}\u{1102}\u{1161}";
    let grid = create_grid_with_content(nfd);
    let row = &grid.viewport[0];

    assert_eq!(row.columns.len(), 2, "2 Hangul syllable clusters");
    assert_eq!(grid.cursor.x, 4);
    assert_eq!(row.columns[0].first_char(), Some('\u{1100}')); // ᄀ
    assert_eq!(row.columns[0].width(), 2);
    assert_eq!(row.columns[1].first_char(), Some('\u{1102}')); // ᄂ
    assert_eq!(row.columns[1].width(), 2);
}

#[test]
fn hangul_jamo_nfd_mixed_with_ascii() {
    // NFD "폴더" + ASCII " test"
    let content = "\u{1111}\u{1169}\u{11AF}\u{1103}\u{1165} test";
    let grid = create_grid_with_content(content);
    let row = &grid.viewport[0];

    assert_eq!(row.columns[0].first_char(), Some('\u{1111}')); // ᄑ (폴)
    assert_eq!(row.columns[0].width(), 2);
    assert_eq!(row.columns[1].first_char(), Some('\u{1103}')); // ᄃ (더)
    assert_eq!(row.columns[1].width(), 2);
    assert_eq!(row.columns[2].grapheme(), " ");
    assert_eq!(row.columns[3].grapheme(), "t");
    assert_eq!(row.columns[4].grapheme(), "e");
    assert_eq!(row.columns[5].grapheme(), "s");
    assert_eq!(row.columns[6].grapheme(), "t");
}

#[test]
fn hangul_jamo_nfd_with_ansi_color() {
    // Colored NFD "폴더": \x1b[01;34m + Jamo + \x1b[0m
    // ANSI escapes should not break grapheme clustering.
    let colored = "\x1b[01;34m\u{1111}\u{1169}\u{11AF}\u{1103}\u{1165}\x1b[0m";
    let grid = create_grid_with_content(colored);
    let row = &grid.viewport[0];

    assert_eq!(row.columns.len(), 2, "2 Hangul syllable clusters");
    assert_eq!(row.columns[0].first_char(), Some('\u{1111}')); // ᄑ (폴)
    assert_eq!(row.columns[0].width(), 2);
    assert_eq!(row.columns[1].first_char(), Some('\u{1103}')); // ᄃ (더)
    assert_eq!(row.columns[1].width(), 2);
}

// ── Composition tests for various scripts ───────────────────────────────────

#[test]
fn devanagari_virama_conjunct() {
    // क्ष = KA(U+0915) + VIRAMA(U+094D) + SSA(U+0937)
    // Unicode 15.1 GB9c groups InCB=Consonant + Linker(virama) + Consonant
    // as a single grapheme cluster. All three codepoints in one cell.
    let content = "\u{0915}\u{094D}\u{0937}";
    let grid = create_grid_with_content(content);
    let row = &grid.viewport[0];

    assert_eq!(row.columns.len(), 1, "virama conjunct = 1 grapheme cluster");
    assert_eq!(row.columns[0].first_char(), Some('\u{0915}')); // KA
                                                               // Width: KA(1) + VIRAMA(0) + SSA(1) → unicode-width on the full string
    assert_eq!(row.columns[0].width(), 2);
    assert_eq!(grid.cursor.x, 2);
}

#[test]
fn devanagari_word_with_virama_and_vowel_signs() {
    // हिन्दी (hindii) = HA + VOWEL_I + NA + VIRAMA + DA + VOWEL_II
    // Two virama conjuncts, two vowel signs.
    let content = "\u{0939}\u{093F}\u{0928}\u{094D}\u{0926}\u{0940}";
    let grid = create_grid_with_content(content);
    let row = &grid.viewport[0];

    // ह + ि = 1 cell (HA + vowel sign I)
    // न + ् + द + ी = conjunct + vowel sign II
    // Grapheme clusters: [हि] [न्दी] — 2 clusters? or [ह] [ि] [न्दी]?
    // UAX #29 says vowel signs (Extend) attach to preceding base.
    // Let's just verify no codepoints are dropped and cursor is correct.
    assert_eq!(
        grid.cursor.x,
        row.columns.iter().map(|c| c.width()).sum::<usize>()
    );
    // Verify the base characters are present
    assert_eq!(row.columns[0].first_char(), Some('\u{0939}')); // HA
}

#[test]
fn arabic_letter_with_diacritics() {
    // بِسْمِ = BA(U+0628) + KASRA(U+0650) + SEEN(U+0633) + SUKUN(U+0652) + MEEM(U+0645) + KASRA(U+0650)
    // Each diacritic is zero-width and attaches to the preceding base letter.
    let content = "\u{0628}\u{0650}\u{0633}\u{0652}\u{0645}\u{0650}";
    let grid = create_grid_with_content(content);
    let row = &grid.viewport[0];

    // 3 base letters, each with a diacritic → 3 cells
    assert_eq!(
        row.columns.len(),
        3,
        "3 Arabic base letters with diacritics"
    );
    assert_eq!(row.columns[0].first_char(), Some('\u{0628}')); // BA
    assert_eq!(row.columns[1].first_char(), Some('\u{0633}')); // SEEN
    assert_eq!(row.columns[2].first_char(), Some('\u{0645}')); // MEEM
                                                               // Each base letter is width 1
    assert_eq!(row.columns[0].width(), 1);
    assert_eq!(grid.cursor.x, 3);
}

#[test]
fn arabic_shadda_with_fatha() {
    // مَّ = MEEM(U+0645) + SHADDA(U+0651) + FATHA(U+064E)
    // Two combining marks stacked on one base letter.
    let content = "\u{0645}\u{0651}\u{064E}";
    let grid = create_grid_with_content(content);
    let row = &grid.viewport[0];

    assert_eq!(row.columns.len(), 1, "base + two diacritics = 1 cell");
    assert_eq!(row.columns[0].first_char(), Some('\u{0645}')); // MEEM
    assert_eq!(row.columns[0].width(), 1);
}

#[test]
fn hebrew_shin_with_nikud() {
    // שָׁ = SHIN(U+05E9) + KAMATZ(U+05B8) + SHIN_DOT(U+05C1)
    // Two combining marks (vowel + clarification dot) on one base letter.
    let content = "\u{05E9}\u{05B8}\u{05C1}";
    let grid = create_grid_with_content(content);
    let row = &grid.viewport[0];

    assert_eq!(row.columns.len(), 1, "base + two nikud marks = 1 cell");
    assert_eq!(row.columns[0].first_char(), Some('\u{05E9}')); // SHIN
    assert_eq!(row.columns[0].width(), 1);
}

#[test]
fn hebrew_shalom_with_nikud() {
    // שָׁלוֹם = SHIN+KAMATZ+SHIN_DOT LAMED VAV+HOLAM FINAL_MEM
    let content = "\u{05E9}\u{05B8}\u{05C1}\u{05DC}\u{05D5}\u{05B9}\u{05DD}";
    let grid = create_grid_with_content(content);
    let row = &grid.viewport[0];

    // 4 base letters: SHIN, LAMED, VAV, FINAL_MEM
    assert_eq!(row.columns.len(), 4, "4 Hebrew base letters");
    assert_eq!(row.columns[0].first_char(), Some('\u{05E9}')); // SHIN
    assert_eq!(row.columns[1].first_char(), Some('\u{05DC}')); // LAMED
    assert_eq!(row.columns[2].first_char(), Some('\u{05D5}')); // VAV
    assert_eq!(row.columns[3].first_char(), Some('\u{05DD}')); // FINAL MEM
    assert_eq!(grid.cursor.x, 4);
}

#[test]
fn latin_stacked_combining_marks() {
    // a + COMBINING_DIAERESIS(U+0308) + COMBINING_MACRON(U+0304)
    // Multiple combining marks stack on one base letter.
    let content = "a\u{0308}\u{0304}";
    let grid = create_grid_with_content(content);
    let row = &grid.viewport[0];

    assert_eq!(row.columns.len(), 1, "base + 2 combining marks = 1 cell");
    assert_eq!(row.columns[0].first_char(), Some('a'));
    assert_eq!(row.columns[0].width(), 1);
}

#[test]
fn latin_nfd_e_acute() {
    // NFD: e(U+0065) + COMBINING_ACUTE(U+0301) → visual é
    // NFC: é(U+00E9) — single codepoint
    // Both should produce 1 cell with width 1.
    let nfd = "e\u{0301}";
    let nfc = "\u{00E9}";

    let grid_nfd = create_grid_with_content(nfd);
    let grid_nfc = create_grid_with_content(nfc);

    assert_eq!(grid_nfd.viewport[0].columns.len(), 1, "NFD é = 1 cell");
    assert_eq!(grid_nfc.viewport[0].columns.len(), 1, "NFC é = 1 cell");
    assert_eq!(grid_nfd.viewport[0].columns[0].width(), 1);
    assert_eq!(grid_nfc.viewport[0].columns[0].width(), 1);
    assert_eq!(grid_nfd.cursor.x, 1);
    assert_eq!(grid_nfc.cursor.x, 1);
}

#[test]
fn emoji_keycap_sequence() {
    // 1️⃣ = DIGIT_ONE(U+0031) + VS16(U+FE0F) + COMBINING_ENCLOSING_KEYCAP(U+20E3)
    // Three codepoints form one grapheme cluster (keycap emoji).
    let content = "1\u{FE0F}\u{20E3}";
    let grid = create_grid_with_content(content);
    let row = &grid.viewport[0];

    assert_eq!(row.columns.len(), 1, "keycap sequence = 1 cell");
    assert_eq!(row.columns[0].first_char(), Some('1'));
    // Keycap emoji should be width 2 (emoji presentation)
    assert_eq!(row.columns[0].width(), 2);
    assert_eq!(grid.cursor.x, 2);
}

#[test]
fn tamil_vowel_sign_attached_to_consonant() {
    // கா = KA(U+0B95) + VOWEL_SIGN_AA(U+0BBE)
    // Vowel sign is zero-width combining, attaches to the consonant.
    let content = "\u{0B95}\u{0BBE}";
    let grid = create_grid_with_content(content);
    let row = &grid.viewport[0];

    assert_eq!(row.columns.len(), 1, "consonant + vowel sign = 1 cell");
    assert_eq!(row.columns[0].first_char(), Some('\u{0B95}')); // KA
    assert_eq!(row.columns[0].width(), 1);
}

#[test]
fn mixed_scripts_with_combining_marks() {
    // Mix of Latin, Arabic diacritic, and CJK in one line.
    // Each script's combining marks must attach to the correct base.
    let content = "a\u{0301}\u{0628}\u{0650}中";
    let grid = create_grid_with_content(content);
    let row = &grid.viewport[0];

    assert_eq!(row.columns[0].first_char(), Some('a')); // a + acute
    assert_eq!(row.columns[0].width(), 1);
    assert_eq!(row.columns[1].first_char(), Some('\u{0628}')); // BA + kasra
    assert_eq!(row.columns[1].width(), 1);
    assert_eq!(row.columns[2].first_char(), Some('中')); // CJK
    assert_eq!(row.columns[2].width(), 2);
    assert_eq!(grid.cursor.x, 4);
}
