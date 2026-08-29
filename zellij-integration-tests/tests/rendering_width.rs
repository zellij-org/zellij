#![cfg(unix)]

use insta::assert_snapshot;
use zellij_integration_tests::{
    claim_first_terminal_and_wait_for_prompt, normalized, start_zellij, FakePtyHandle, TestSession,
};

const SENTINEL: &str = "FIXTURE-END";

fn render_fixture(zellij: &TestSession, terminal: &FakePtyHandle, fixture: &str) -> String {
    terminal.output(fixture.as_bytes());
    terminal.output(format!("\r\n{}", SENTINEL).as_bytes());
    let grid_snapshot = zellij.wait_until("fixture rendered", |grid_snapshot| {
        grid_snapshot.contains(SENTINEL) && grid_snapshot.status_bar_appears()
    });
    normalized(&grid_snapshot)
}

#[test]
fn cjk_text_renders_at_double_width() {
    let mut zellij = start_zellij();
    let terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    let rendered = render_fixture(
        &zellij,
        &terminal,
        "\u{4f60}\u{597d}\u{4e16}\u{754c} \u{65e5}\u{672c}\u{8a9e}\u{30c6}\u{30ad}\u{30b9}\u{30c8} \u{d55c}\u{ad6d}\u{c5b4} |ascii|",
    );
    assert_snapshot!(rendered);
    zellij.quit();
}

#[test]
fn emoji_zwj_and_skin_tone_sequences_render() {
    let mut zellij = start_zellij();
    let terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    let rendered = render_fixture(
        &zellij,
        &terminal,
        "\u{1f600}|\u{1f468}\u{200d}\u{1f469}\u{200d}\u{1f467}\u{200d}\u{1f466}|\u{1f44b}\u{1f3fd}|\u{1f9d1}\u{1f3ff}\u{200d}\u{1f680}|\u{26a0}\u{fe0f}|\u{1f1fa}\u{1f1f8}|",
    );
    assert_snapshot!(rendered);
    zellij.quit();
}

#[test]
fn combining_marks_render_over_their_base_characters() {
    let mut zellij = start_zellij();
    let terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    let rendered = render_fixture(
        &zellij,
        &terminal,
        "e\u{301}cole|a\u{300}\u{308}|o\u{331}|\u{e9}cole|\u{1100}\u{1161}\u{11a8}|",
    );
    assert_snapshot!(rendered);
    zellij.quit();
}

#[test]
fn ambiguous_width_characters_render_narrow() {
    let mut zellij = start_zellij();
    let terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    let rendered = render_fixture(
        &zellij,
        &terminal,
        "|\u{b1}\u{b0}\u{2192}\u{3b1}\u{3b2}\u{a7}\u{b6}\u{203b}\u{d7}\u{f7}\u{2318}\u{26a0}|\u{2500}\u{2500}|",
    );
    assert_snapshot!(rendered);
    zellij.quit();
}

#[test]
fn box_drawing_characters_render_narrow() {
    let mut zellij = start_zellij();
    let terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    let rendered = render_fixture(
        &zellij,
        &terminal,
        "\u{250c}\u{2500}\u{252c}\u{2500}\u{2510}\r\n\u{2502}A\u{2502}B\u{2502}\r\n\u{251c}\u{2500}\u{253c}\u{2500}\u{2524}\r\n\u{2514}\u{2500}\u{2534}\u{2500}\u{2518}\r\n\u{2554}\u{2550}\u{2557}\u{2588}\u{2589}\u{258a}",
    );
    assert_snapshot!(rendered);
    zellij.quit();
}

#[test]
fn wide_character_straddling_the_right_edge_wraps_whole() {
    let mut zellij = start_zellij();
    let terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    let (cols, _rows) = terminal.wait_for_size("pane reported its size", |cols, _| cols > 4);
    terminal.output(format!("\u{1b}[1;{}HAB\u{4e16}\u{754c}", cols - 2).as_bytes());
    terminal.output(format!("\u{1b}[3;1H{}", SENTINEL).as_bytes());
    let grid_snapshot = zellij.wait_until("edge fixture rendered", |grid_snapshot| {
        grid_snapshot.contains(SENTINEL) && grid_snapshot.status_bar_appears()
    });
    let lines = grid_snapshot.lines();
    assert!(
        lines[1].trim_end().ends_with("AB"),
        "the wide character must not be split across the pane edge, got {:?}",
        lines[1]
    );
    assert!(
        lines[2].starts_with("\u{4e16}\u{754c}"),
        "the straddling wide character must wrap whole to the next row, got {:?}",
        lines[2]
    );
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn ech_over_a_wide_character_keeps_the_rest_of_the_line_aligned() {
    let mut zellij = start_zellij();
    let terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    terminal.output("\u{1b}[2;1H\u{4e16}\u{754c}abc".as_bytes());
    zellij.wait_until("wide characters rendered", |grid_snapshot| {
        grid_snapshot.contains("\u{4e16}\u{754c}abc")
    });
    terminal.output("\u{1b}[2;1H\u{1b}[1X".as_bytes());
    terminal.output(format!("\u{1b}[4;1H{}", SENTINEL).as_bytes());
    let grid_snapshot = zellij.wait_until("erased wide character rendered", |grid_snapshot| {
        grid_snapshot.contains(SENTINEL) && grid_snapshot.status_bar_appears()
    });
    let lines = grid_snapshot.lines();
    assert!(
        lines[2].starts_with("  \u{754c}abc"),
        "erasing the leading half of a wide character must blank both of its columns, got {:?}",
        lines[2]
    );
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn cursor_forward_past_content_pads_the_row_before_wide_characters() {
    let mut zellij = start_zellij();
    let terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    terminal.output("\u{1b}[4;1H\u{1b}[6C\u{4e16}\u{754c}X".as_bytes());
    terminal.output(format!("\u{1b}[6;1H{}", SENTINEL).as_bytes());
    let grid_snapshot = zellij.wait_until("padded row rendered", |grid_snapshot| {
        grid_snapshot.contains(SENTINEL) && grid_snapshot.status_bar_appears()
    });
    let lines = grid_snapshot.lines();
    assert!(
        lines[4].starts_with("      \u{4e16}\u{754c}X"),
        "cursor-forward past the end of content must pad with blanks, got {:?}",
        lines[4]
    );
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn characters_whose_width_changed_in_the_new_width_tables_render() {
    let mut zellij = start_zellij();
    let terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    let rendered = render_fixture(
        &zellij,
        &terminal,
        "|\u{2630}\u{2637}|\u{268a}\u{268f}|\u{4dc0}\u{4dff}|\u{31e4}\u{31ef}|\r\n|\u{ff76}\u{ff9e}|\u{ff8a}\u{ff9f}|\u{ad}|\u{3164}|\u{302e}|\u{17d8}|\u{17a4}|",
    );
    assert_snapshot!(rendered);
    zellij.quit();
}
