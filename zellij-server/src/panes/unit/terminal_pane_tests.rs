use super::super::TerminalPane;
use crate::tab::Pane;
use ::insta::assert_snapshot;
use zellij_utils::pane_size::PaneGeom;
use zellij_utils::zellij_tile::data::Palette;

use std::fmt::Write;

#[test]
pub fn scrolling_inside_a_pane() {
    let mut fake_win_size = PaneGeom::default();
    fake_win_size.cols.set_inner(121);
    fake_win_size.rows.set_inner(20);

    let pid = 1;
    let palette = Palette::default();
    let mut terminal_pane = TerminalPane::new(pid, fake_win_size, palette, 0, String::new()); // 0 is the pane index
    let mut text_to_fill_pane = String::new();
    for i in 0..30 {
        writeln!(&mut text_to_fill_pane, "\rline {}", i + 1).unwrap();
    }
    terminal_pane.handle_pty_bytes(text_to_fill_pane.into_bytes());
    terminal_pane.scroll_up(10);
    assert_snapshot!(format!("{:?}", terminal_pane.grid));
    terminal_pane.scroll_down(3);
    assert_snapshot!(format!("{:?}", terminal_pane.grid));
    terminal_pane.clear_scroll();
    assert_snapshot!(format!("{:?}", terminal_pane.grid));
}
