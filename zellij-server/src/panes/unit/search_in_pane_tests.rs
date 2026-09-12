use super::super::TerminalPane;
use crate::panes::kitty_graphics::KittyImageStore;
use crate::panes::sixel::SixelImageStore;
use crate::panes::LinkHandler;
use crate::tab::Pane;
use insta::assert_snapshot;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use zellij_utils::data::{Palette, Style};
use zellij_utils::pane_size::PaneGeom;

fn read_fixture() -> Vec<u8> {
    let mut path_to_file = std::path::PathBuf::new();
    path_to_file.push("../src");
    path_to_file.push("tests");
    path_to_file.push("fixtures");
    path_to_file.push("grid_copy");
    std::fs::read(path_to_file)
        .unwrap_or_else(|_| panic!("could not read fixture ../src/tests/fixtures/grid_copy"))
}

fn create_pane() -> TerminalPane {
    let mut fake_win_size = PaneGeom::default();
    fake_win_size.cols.set_inner(121);
    fake_win_size.rows.set_inner(20);

    let pid = 1;
    let style = Style::default();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let search_auto_jump_on_input = true;
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
        Rc::new(RefCell::new(KittyImageStore::default())),
        Rc::new(RefCell::new(Palette::default())),
        terminal_emulator_color_codes,
        None,
        None,
        debug,
        arrow_fonts,
        styled_underlines,
        search_auto_jump_on_input,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
        None,
    ); // 0 is the pane index
    let content = read_fixture();
    terminal_pane.handle_pty_bytes(content);
    terminal_pane
}

#[test]
pub fn searching_inside_a_viewport() {
    let mut terminal_pane = create_pane();
    terminal_pane.update_search_term("tortor");
    assert_snapshot!(
        "grid_copy_tortor_highlighted",
        format!("{:?}", terminal_pane.grid)
    );
    terminal_pane.search_up();
    // snapshot-size optimization: We use a named one here to de-duplicate
    assert_snapshot!(
        "grid_copy_search_cursor_at_bottom",
        format!("{:?}", terminal_pane.grid)
    );
    terminal_pane.search_up();
    assert_snapshot!(
        "grid_copy_search_cursor_at_second",
        format!("{:?}", terminal_pane.grid)
    );
}

#[test]
pub fn searching_scroll_viewport() {
    let mut terminal_pane = create_pane();
    terminal_pane.update_search_term("tortor");
    terminal_pane.search_up();
    // snapshot-size optimization: We use a named one here to de-duplicate
    assert_snapshot!(
        "grid_copy_search_cursor_at_bottom",
        format!("{:?}", terminal_pane.grid)
    );
    terminal_pane.search_up();
    assert_snapshot!(
        "grid_copy_search_cursor_at_second",
        format!("{:?}", terminal_pane.grid)
    );
    // Scroll away
    terminal_pane.search_up();
    assert_snapshot!(
        "grid_copy_search_scrolled_up",
        format!("{:?}", terminal_pane.grid)
    );
}

/// Regression test for https://github.com/zellij-org/zellij/issues/5551
///
/// With `search_auto_jump_on_input` disabled, typing a search query (i.e. calling
/// `update_search_term` per keystroke) must not jump the viewport around the
/// scrollback just because the current viewport does not contain a hit yet. The
/// auto-jump to the nearest hit belongs to explicit navigation with `n`/`p`
/// (`search_down`/`search_up`).
///
/// We scroll to the bottom of the scrollback and then type "odio", a term that
/// occurs in the scrollback above, but not in the current viewport.
/// Pressing the navigation keys afterwards must still jump to the nearest hit.
#[test]
pub fn updating_the_search_term_does_not_auto_jump_when_disabled() {
    let fake_client_id = 1;
    let mut terminal_pane = create_pane();
    terminal_pane.grid.update_search_auto_jump_on_input(false);

    // Scroll the viewport to the very bottom of the scrollback,
    // so that "odio" (which occurs above) is not visible
    terminal_pane.scroll_up(4, fake_client_id);
    terminal_pane.scroll_down(100, fake_client_id);
    assert!(terminal_pane.grid.lines_below.is_empty());

    // Now type the query one character at a time, like the client does
    for c in "odio".chars() {
        terminal_pane.update_search_term(&c.to_string());
    }

    // Typing the query must not have moved the viewport
    assert!(
        terminal_pane.grid.lines_below.is_empty(),
        "typing the search query scrolled the viewport: \n{:?}",
        terminal_pane.grid
    );
    // and no active hit should be selected
    assert!(terminal_pane.grid.search_results.active.is_none());

    // Explicit navigation must still jump to the nearest hit
    terminal_pane.search_up();
    assert!(terminal_pane.grid.search_results.active.is_some());
    assert!(!terminal_pane.grid.lines_below.is_empty());
}

/// Test for https://github.com/zellij-org/zellij/issues/5551
///
/// The default (`search_auto_jump_on_input = true`, like in Firefox or helix) is
/// to automatically jump to the nearest hit while typing, if the current
/// viewport does not contain one. This test locks that default in.
#[test]
pub fn updating_the_search_term_auto_jumps_by_default() {
    let fake_client_id = 1;
    let mut terminal_pane = create_pane();

    // Scroll the viewport to the very bottom of the scrollback,
    // so that "odio" (which occurs above) is not visible
    terminal_pane.scroll_up(4, fake_client_id);
    terminal_pane.scroll_down(100, fake_client_id);
    assert!(terminal_pane.grid.lines_below.is_empty());

    // Now type the query one character at a time, like the client does
    for c in "odio".chars() {
        terminal_pane.update_search_term(&c.to_string());
    }

    // With the default on, typing the query must jump the viewport to the
    // nearest hit, so the scrollback above the viewport is now occupied
    assert!(
        !terminal_pane.grid.lines_below.is_empty(),
        "typing the search query did not auto-jump to the nearest hit: \n{:?}",
        terminal_pane.grid
    );
}

#[test]
pub fn searching_with_wrap() {
    let mut terminal_pane = create_pane();
    // Searching for "tortor"
    terminal_pane.update_search_term("tortor");
    // Selecting the last place tortor was found
    terminal_pane.search_up();
    assert_snapshot!(
        "grid_copy_search_cursor_at_bottom",
        format!("{:?}", terminal_pane.grid)
    );
    // Search backwards again
    terminal_pane.search_up();
    assert_snapshot!(
        "grid_copy_search_cursor_at_second",
        format!("{:?}", terminal_pane.grid)
    );
    terminal_pane.search_down();
    assert_snapshot!(
        "grid_copy_search_cursor_at_bottom",
        format!("{:?}", terminal_pane.grid)
    );
    // Searching forward again should do nothing here
    terminal_pane.search_down();
    assert_snapshot!(
        "grid_copy_search_cursor_at_bottom",
        format!("{:?}", terminal_pane.grid)
    );
    // Only after wrapping search is active, do we actually jump in the scroll buffer
    terminal_pane.toggle_search_wrap();
    terminal_pane.search_down();
    assert_snapshot!(
        "grid_copy_search_cursor_at_top",
        format!("{:?}", terminal_pane.grid)
    );

    // Deactivate wrap again
    terminal_pane.toggle_search_wrap();
    // Should be a no-op again
    terminal_pane.search_up();
    assert_snapshot!(
        "grid_copy_search_cursor_at_top",
        format!("{:?}", terminal_pane.grid)
    );

    // Re-activate wrap again
    terminal_pane.toggle_search_wrap();
    terminal_pane.search_up();
    assert_snapshot!(
        "grid_copy_search_cursor_at_bottom",
        format!("{:?}", terminal_pane.grid)
    );
}

#[test]
pub fn searching_case_insensitive() {
    let mut terminal_pane = create_pane();
    terminal_pane.update_search_term("quam");
    assert_snapshot!(
        "grid_copy_quam_highlighted",
        format!("{:?}", terminal_pane.grid)
    );

    // sensitivity off
    terminal_pane.toggle_search_case_sensitivity();

    assert_snapshot!(
        "grid_copy_quam_insensitive_highlighted",
        format!("{:?}", terminal_pane.grid)
    );

    // sensitivity on
    terminal_pane.toggle_search_case_sensitivity();

    assert_snapshot!(
        "grid_copy_quam_highlighted",
        format!("{:?}", terminal_pane.grid)
    );

    // Select one and check that we keep the current selection,
    // if it wasn't one that vanished
    terminal_pane.search_up();
    assert_snapshot!(
        "grid_copy_quam_highlighted_cursor_bottom",
        format!("{:?}", terminal_pane.grid)
    );

    // sensitivity off
    terminal_pane.toggle_search_case_sensitivity();

    assert_snapshot!(
        "grid_copy_quam_insensitive_cursor_bottom",
        format!("{:?}", terminal_pane.grid)
    );

    // sensitivity on
    terminal_pane.toggle_search_case_sensitivity();

    assert_snapshot!(
        "grid_copy_quam_highlighted_cursor_bottom",
        format!("{:?}", terminal_pane.grid)
    );

    // sensitivity off
    terminal_pane.toggle_search_case_sensitivity();

    // Selecting the case insensitive result
    terminal_pane.search_up();
    terminal_pane.search_up();
    terminal_pane.search_up();
    terminal_pane.search_up();
    terminal_pane.search_up();
    assert_snapshot!(
        "grid_copy_quam_insensitive_selection",
        format!("{:?}", terminal_pane.grid)
    );

    // sensitivity on
    terminal_pane.toggle_search_case_sensitivity();
    // Now the selected result vanished and we should be back at
    // the beginning
    assert_snapshot!(
        "grid_copy_quam_highlighted",
        format!("{:?}", terminal_pane.grid)
    );
    terminal_pane.search_up();
    assert_snapshot!(
        "grid_copy_quam_highlighted_cursor_bottom",
        format!("{:?}", terminal_pane.grid)
    );
}

#[test]
pub fn searching_inside_and_scroll() {
    let fake_client_id = 1;
    let mut terminal_pane = create_pane();
    terminal_pane.update_search_term("quam");
    terminal_pane.search_up();
    assert_snapshot!(
        "grid_copy_quam_highlighted_cursor_bottom",
        format!("{:?}", terminal_pane.grid)
    );
    assert_eq!(
        terminal_pane.grid.search_results.active.as_ref(),
        terminal_pane.grid.search_results.selections.last()
    );
    // Scrolling up until a new search result appears
    terminal_pane.scroll_up(4, fake_client_id);

    // Scrolling back down should give the same result as before
    terminal_pane.scroll_down(4, fake_client_id);
    assert_eq!(
        terminal_pane.grid.search_results.active.as_ref(),
        terminal_pane.grid.search_results.selections.last()
    );
    assert_snapshot!(
        "grid_copy_quam_highlighted_cursor_bottom",
        format!("{:?}", terminal_pane.grid)
    );

    // Scrolling up until a the active marker goes out of view
    terminal_pane.scroll_up(5, fake_client_id);
    assert_eq!(terminal_pane.grid.search_results.active, None);

    terminal_pane.scroll_down(5, fake_client_id);
    assert_eq!(terminal_pane.grid.search_results.active, None);
    assert_snapshot!(
        "grid_copy_quam_highlighted",
        format!("{:?}", terminal_pane.grid)
    );
}

#[test]
pub fn searching_and_resize() {
    let mut terminal_pane = create_pane();
    terminal_pane.update_search_term("tortor");
    assert_snapshot!(
        "grid_copy_tortor_highlighted",
        format!("{:?}", terminal_pane.grid)
    );

    // Highlights should still be there, if pane gets resized
    terminal_pane.grid.change_size(20, 150);
    assert_snapshot!(
        "grid_copy_tortor_highlighted_wide",
        format!("{:?}", terminal_pane.grid)
    );

    terminal_pane.grid.change_size(20, 80);
    assert_snapshot!(
        "grid_copy_tortor_highlighted_narrow",
        format!("{:?}", terminal_pane.grid)
    );
}

#[test]
pub fn searching_across_line_wrap() {
    let mut terminal_pane = create_pane();
    terminal_pane.update_search_term("aliquam sem fringilla");
    // Spread across two lines
    terminal_pane.grid.change_size(30, 60);
    assert_snapshot!(
        "grid_copy_multiline_highlighted",
        format!("{:?}", terminal_pane.grid)
    );

    // Spread across 4 lines
    terminal_pane.grid.change_size(40, 4);
    assert_snapshot!(
        "grid_copy_multiline_highlighted_narrow",
        format!("{:?}", terminal_pane.grid)
    );

    terminal_pane.search_up();
    assert_snapshot!(
        "grid_copy_multiline_selected_narrow",
        format!("{:?}", terminal_pane.grid)
    );

    // Wrap on
    terminal_pane.toggle_search_wrap();
    terminal_pane.search_down();
    assert_snapshot!(
        "grid_copy_multiline_selected_wrap_narrow",
        format!("{:?}", terminal_pane.grid)
    );

    // Wrap off
    terminal_pane.toggle_search_wrap();
    // Don't forget the current selection
    terminal_pane.search_up();
    assert_snapshot!(
        "grid_copy_multiline_selected_wrap_narrow",
        format!("{:?}", terminal_pane.grid)
    );

    // Wrap on
    terminal_pane.toggle_search_wrap();
    terminal_pane.search_up();
    assert_snapshot!(
        "grid_copy_multiline_selected_narrow",
        format!("{:?}", terminal_pane.grid)
    );
}

#[test]
pub fn searching_whole_word() {
    let mut terminal_pane = create_pane();
    terminal_pane.update_search_term("quam");
    assert_snapshot!(
        "grid_copy_quam_highlighted",
        format!("{:?}", terminal_pane.grid)
    );

    terminal_pane.toggle_search_whole_words();
    assert_snapshot!(
        "grid_copy_quam_whole_word_only",
        format!("{:?}", terminal_pane.grid)
    );

    terminal_pane.toggle_search_whole_words();
    assert_snapshot!(
        "grid_copy_quam_highlighted",
        format!("{:?}", terminal_pane.grid)
    );
}

#[test]
pub fn searching_whole_word_across_line_wrap() {
    let mut terminal_pane = create_pane();
    terminal_pane.handle_pty_bytes(
        "a:--:aaaaaaaaa:--:--:--:aaaaaaaaaaa:--: :--: :--: aaa :--::--: aaa"
            .as_bytes()
            .to_vec(),
    );
    terminal_pane.grid.change_size(20, 5);
    terminal_pane.update_search_term(":--:");
    assert_snapshot!(
        "grid_copy_multiline_not_whole_word",
        format!("{:?}", terminal_pane.grid)
    );

    terminal_pane.toggle_search_whole_words();
    assert_snapshot!(
        "grid_copy_multiline_whole_word",
        format!("{:?}", terminal_pane.grid)
    );
}

#[test]
pub fn searching_whole_word_case_insensitive() {
    let mut terminal_pane = create_pane();
    terminal_pane.update_search_term("quam");
    assert_snapshot!(
        "grid_copy_quam_highlighted",
        format!("{:?}", terminal_pane.grid)
    );

    terminal_pane.toggle_search_whole_words();
    assert_snapshot!(
        "grid_copy_quam_whole_word_only",
        format!("{:?}", terminal_pane.grid)
    );

    terminal_pane.toggle_search_case_sensitivity();
    assert_snapshot!(
        "grid_copy_quam_whole_word_case_insensitive",
        format!("{:?}", terminal_pane.grid)
    );

    terminal_pane.toggle_search_whole_words();
    assert_snapshot!(
        "grid_copy_quam_insensitive_highlighted",
        format!("{:?}", terminal_pane.grid)
    );

    terminal_pane.toggle_search_case_sensitivity();
    assert_snapshot!(
        "grid_copy_quam_highlighted",
        format!("{:?}", terminal_pane.grid)
    );
}
