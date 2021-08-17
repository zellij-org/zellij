use super::super::TerminalPane;
use crate::tab::Pane;
use ansi_term::Color::{Fixed, RGB};
use insta::assert_snapshot;
use zellij_utils::pane_size::PositionAndSize;
use zellij_utils::zellij_tile::data::Palette;

#[test]
pub fn scrolling_inside_a_pane() {
    let fake_win_size = PositionAndSize {
        cols: 121,
        rows: 20,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let pid = 1;
    let palette = Some(Palette::default());
    let mut terminal_pane = TerminalPane::new(pid, fake_win_size, palette, 0); // 0 is the pane index
    let mut text_to_fill_pane = String::new();
    for i in 0..30 {
        text_to_fill_pane.push_str(&format!("\rline {}\n", i + 1));
    }
    terminal_pane.handle_pty_bytes(text_to_fill_pane.as_bytes().to_vec());
    terminal_pane.scroll_up(10);
    assert_snapshot!(format!("{:?}", terminal_pane.grid));
    terminal_pane.scroll_down(3);
    assert_snapshot!(format!("{:?}", terminal_pane.grid));
    terminal_pane.clear_scroll();
    assert_snapshot!(format!("{:?}", terminal_pane.grid));
}

#[test]
pub fn monochrome_pane() {
    let fake_win_size = PositionAndSize {
        cols: 10,
        rows: 1,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let pid = 1;
    let palette = None;
    let mut terminal_pane = TerminalPane::new(pid, fake_win_size, palette, 0);
    let text_style = ansi_term::Style::new()
        .fg(Fixed(77))
        .on(RGB(7, 7, 7))
        .bold(); // styles other than colors should not be stripped
    let text_to_fill_pane = format!("{}", text_style.paint("Hi Zellij"));

    terminal_pane.handle_pty_bytes(dbg!(text_to_fill_pane).as_bytes().to_vec());
    assert_snapshot!(format!("{:?}", terminal_pane.render().unwrap()));
}
