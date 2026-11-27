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

fn create_pane_with_content(content: &str, cols: usize) -> TerminalPane {
    let mut fake_win_size = PaneGeom::default();
    fake_win_size.cols.set_inner(cols);
    fake_win_size.rows.set_inner(10);
    let mut terminal_pane = TerminalPane::new(
        1,
        fake_win_size,
        Style::default(),
        0,
        String::new(),
        Rc::new(RefCell::new(LinkHandler::new())),
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
    terminal_pane.handle_pty_bytes(content.as_bytes().to_vec());
    terminal_pane
}

fn found_ranges(terminal_pane: &TerminalPane) -> Vec<((isize, usize), (isize, usize))> {
    terminal_pane
        .grid
        .search_results
        .selections
        .iter()
        .map(|selection| {
            (
                (selection.start.line(), selection.start.column()),
                (selection.end.line(), selection.end.column()),
            )
        })
        .collect()
}

const FAMILY: &str = "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}";

#[test]
pub fn searching_for_a_decomposed_accent_matches_the_whole_cluster() {
    let mut terminal_pane = create_pane_with_content("cafe\u{0301} au lait", 40);
    terminal_pane.update_search_term("e\u{0301}");
    assert_eq!(found_ranges(&terminal_pane), vec![((0, 3), (0, 4))]);
}

#[test]
pub fn searching_for_a_base_letter_does_not_match_inside_a_decomposed_cluster() {
    let mut terminal_pane = create_pane_with_content("cafe\u{0301} e", 40);
    terminal_pane.update_search_term("e");
    assert_eq!(found_ranges(&terminal_pane), vec![((0, 5), (0, 6))]);
}

#[test]
pub fn searching_for_a_family_emoji_in_default_mode_highlights_its_group() {
    let mut terminal_pane = create_pane_with_content(&format!("ab{}cd", FAMILY), 40);
    terminal_pane.update_search_term(FAMILY);
    assert_eq!(found_ranges(&terminal_pane), vec![((0, 2), (0, 8))]);
}

#[test]
pub fn searching_for_a_family_emoji_in_grapheme_mode_highlights_one_cell() {
    let mut terminal_pane = create_pane_with_content(&format!("\u{1b}[?2027hab{}cd", FAMILY), 40);
    terminal_pane.update_search_term(FAMILY);
    assert_eq!(found_ranges(&terminal_pane), vec![((0, 2), (0, 4))]);
}

#[test]
pub fn searching_for_part_of_a_family_emoji_does_not_match() {
    let mut terminal_pane = create_pane_with_content(&format!("ab{}cd", FAMILY), 40);
    terminal_pane.update_search_term("\u{1F469}");
    assert_eq!(found_ranges(&terminal_pane), vec![]);
}

#[test]
pub fn searching_non_ascii_text_does_not_panic_and_finds_it() {
    let mut terminal_pane = create_pane_with_content("grüße und grüße", 40);
    terminal_pane.update_search_term("grüße");
    assert_eq!(
        found_ranges(&terminal_pane),
        vec![((0, 0), (0, 5)), ((0, 10), (0, 15))]
    );
}

#[test]
pub fn a_match_after_wide_characters_highlights_screen_columns() {
    let mut terminal_pane = create_pane_with_content("你好 world", 40);
    terminal_pane.update_search_term("world");
    assert_eq!(found_ranges(&terminal_pane), vec![((0, 5), (0, 10))]);
    assert_snapshot!(format!("{:?}", terminal_pane.grid));
}

#[test]
pub fn a_non_ascii_match_across_a_wrapped_line_is_found() {
    let mut terminal_pane = create_pane_with_content("xxxxxxxgrüße", 10);
    terminal_pane.update_search_term("grüße");
    assert_eq!(found_ranges(&terminal_pane), vec![((0, 7), (1, 2))]);
}

#[test]
pub fn case_insensitive_search_folds_greek_cyrillic_and_turkish() {
    let mut terminal_pane = create_pane_with_content("ΣΟΦΙΑ МОСКВА \u{0130}stanbul", 60);
    terminal_pane.update_search_term("σοφια");
    terminal_pane.toggle_search_case_sensitivity();
    assert_eq!(found_ranges(&terminal_pane), vec![((0, 0), (0, 5))]);
    terminal_pane.update_search_term("\0");
    terminal_pane.update_search_term("москва");
    terminal_pane.toggle_search_case_sensitivity();
    assert_eq!(found_ranges(&terminal_pane), vec![((0, 6), (0, 12))]);
    terminal_pane.update_search_term("\0");
    terminal_pane.update_search_term("i\u{0307}stanbul");
    terminal_pane.toggle_search_case_sensitivity();
    assert_eq!(found_ranges(&terminal_pane), vec![((0, 13), (0, 21))]);
}

#[test]
pub fn whole_word_search_respects_non_ascii_neighbours() {
    let mut terminal_pane = create_pane_with_content("überall all állé all", 40);
    terminal_pane.update_search_term("all");
    terminal_pane.toggle_search_whole_words();
    assert_eq!(
        found_ranges(&terminal_pane),
        vec![((0, 8), (0, 11)), ((0, 17), (0, 20))]
    );
}
