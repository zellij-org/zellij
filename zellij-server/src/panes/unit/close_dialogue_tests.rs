use super::*;
use zellij_utils::data::{BareKey, KeyWithModifier, Style};

fn lines(chunks: &[CharacterChunk]) -> Vec<String> {
    chunks
        .iter()
        .map(|chunk| {
            chunk
                .terminal_characters
                .iter()
                .map(|c| c.character)
                .collect()
        })
        .collect()
}

fn text(chunks: &[CharacterChunk]) -> String {
    lines(chunks).join("\n")
}

fn key(bare: BareKey) -> CloseDialogueInput {
    close_dialogue_input(Some(&KeyWithModifier::new(bare)), &[], DETACH_OPTION)
}

fn key_from(bare: BareKey, selected: usize) -> CloseDialogueInput {
    close_dialogue_input(Some(&KeyWithModifier::new(bare)), &[], selected)
}

fn bytes(raw: &[u8], selected: usize) -> CloseDialogueInput {
    close_dialogue_input(None, raw, selected)
}

#[test]
fn renders_all_expected_content() {
    let chunks = close_dialogue_chunks(80, 24, 0, 0, &Style::default(), "my-session", 0);
    assert_eq!(chunks.len(), 24);
    let rendered = text(&chunks);
    assert!(rendered.contains("Close this window?"));
    assert!(
        rendered.contains("The session \"my-session\" keeps running and can be attached again.")
    );
    assert!(rendered.contains("> 1. Detach and leave running in the background."));
    assert!(rendered.contains("  2. Quit and shut down the entire session."));
    assert!(rendered.contains("  3. Cancel"));
    assert!(rendered.contains("<↓↑> select"));
    assert!(rendered.contains("<q> quit"));
    assert!(rendered.contains("<Enter> confirm"));
    assert!(rendered.contains("<Esc> cancel"));
}

#[test]
fn the_selection_marker_follows_the_selected_option() {
    let rendered = text(&close_dialogue_chunks(
        80,
        24,
        0,
        0,
        &Style::default(),
        "s",
        CANCEL_OPTION,
    ));
    assert!(rendered.contains("  1. Detach and leave running in the background."));
    assert!(rendered.contains("> 3. Cancel"));
}

#[test]
fn every_row_covers_the_whole_pane_at_its_offset() {
    let chunks = close_dialogue_chunks(50, 12, 7, 3, &Style::default(), "s", 0);
    assert_eq!(chunks.len(), 12);
    for (row, chunk) in chunks.iter().enumerate() {
        assert_eq!(chunk.x, 7);
        assert_eq!(chunk.y, 3 + row);
        assert_eq!(chunk.terminal_characters.len(), 50);
    }
}

#[test]
fn a_long_session_name_wraps_inside_the_block() {
    let name = "a-session-with-a-rather-long-name";
    let chunks = close_dialogue_chunks(60, 24, 0, 0, &Style::default(), name, 0);
    let rendered = text(&chunks);
    assert!(rendered.contains(name));
    assert!(rendered.contains("can be attached again."));
}

#[test]
fn narrow_panes_render_the_fallback() {
    let rendered = text(&close_dialogue_chunks(
        18,
        20,
        0,
        0,
        &Style::default(),
        "s",
        0,
    ));
    assert!(rendered.contains("Close"));
    assert!(!rendered.contains("Detach and leave running in the background."));
}

#[test]
fn short_panes_render_the_fallback() {
    let rendered = text(&close_dialogue_chunks(
        80,
        5,
        0,
        0,
        &Style::default(),
        "s",
        0,
    ));
    assert!(rendered.contains("Close this window? <y> detach <q> quit <Esc> cancel"));
    assert!(!rendered.contains("Detach and leave running in the background."));
}

#[test]
fn tiny_geometry_does_not_panic() {
    assert_eq!(
        close_dialogue_chunks(3, 2, 0, 0, &Style::default(), "s", 0).len(),
        2
    );
    assert!(close_dialogue_chunks(0, 0, 0, 0, &Style::default(), "s", 0).is_empty());
}

#[test]
fn clicking_a_row_finds_the_option_drawn_on_it() {
    let style = Style::default();
    let chunks = close_dialogue_chunks(80, 24, 0, 0, &style, "s", 0);
    let rows = lines(&chunks);
    let detach_row = rows
        .iter()
        .position(|line| line.contains("Detach and leave running in the background."))
        .unwrap();
    let cancel_row = rows
        .iter()
        .position(|line| line.contains("3. Cancel"))
        .unwrap();
    assert_eq!(
        close_dialogue_option_at_content_row(24, 80, detach_row, &style, "s", 0),
        Some(DETACH_OPTION)
    );
    assert_eq!(
        close_dialogue_option_at_content_row(24, 80, cancel_row, &style, "s", 0),
        Some(CANCEL_OPTION)
    );
    let quit_row = rows
        .iter()
        .position(|line| line.contains("2. Quit"))
        .unwrap();
    assert_eq!(
        close_dialogue_option_at_content_row(24, 80, quit_row, &style, "s", 0),
        Some(QUIT_OPTION)
    );
    assert_eq!(
        CloseDialogueOutcome::of_option(QUIT_OPTION),
        CloseDialogueOutcome::Quit
    );
    assert_eq!(
        close_dialogue_option_at_content_row(24, 80, 0, &style, "s", 0),
        None
    );
    assert_eq!(
        close_dialogue_option_at_content_row(5, 80, 2, &style, "s", 0),
        None,
        "the fallback has no options to click"
    );
}

#[test]
fn arrows_and_vim_keys_move_the_selection() {
    let select = CloseDialogueInput::Select;
    assert_eq!(key(BareKey::Down), select(QUIT_OPTION));
    assert_eq!(key(BareKey::Char('j')), select(QUIT_OPTION));
    assert_eq!(key(BareKey::Up), select(CANCEL_OPTION));
    assert_eq!(key(BareKey::Char('k')), select(CANCEL_OPTION));
    assert_eq!(key_from(BareKey::Down, QUIT_OPTION), select(CANCEL_OPTION));
    assert_eq!(
        key_from(BareKey::Down, CANCEL_OPTION),
        select(DETACH_OPTION)
    );
    assert_eq!(key_from(BareKey::Up, QUIT_OPTION), select(DETACH_OPTION));
    assert_eq!(bytes(b"\x1b[B", 0), select(QUIT_OPTION));
    assert_eq!(bytes(b"k", 1), select(DETACH_OPTION));
}

#[test]
fn digits_choose_directly() {
    let detach = CloseDialogueInput::Choose(CloseDialogueOutcome::Detach);
    let cancel = CloseDialogueInput::Choose(CloseDialogueOutcome::Cancel);
    assert_eq!(key_from(BareKey::Char('1'), CANCEL_OPTION), detach);
    assert_eq!(key_from(BareKey::Char('3'), DETACH_OPTION), cancel);
    assert_eq!(bytes(b"1", 1), detach);
    assert_eq!(bytes(b"3", 0), cancel);
    let quit = CloseDialogueInput::Choose(CloseDialogueOutcome::Quit);
    assert_eq!(key(BareKey::Char('2')), quit);
    assert_eq!(bytes(b"2", 0), quit);
}

#[test]
fn q_quits_the_session_and_enter_on_the_second_option_does_too() {
    let quit = CloseDialogueInput::Choose(CloseDialogueOutcome::Quit);
    assert_eq!(key(BareKey::Char('q')), quit);
    assert_eq!(bytes(b"q", 0), quit);
    assert_eq!(key_from(BareKey::Enter, QUIT_OPTION), quit);
    assert_eq!(
        close_dialogue_input(
            Some(&KeyWithModifier::new(BareKey::Char('q')).with_ctrl_modifier()),
            &[0x11],
            0
        ),
        CloseDialogueInput::Swallow,
        "Ctrl q is not a quit inside the dialogue"
    );
}

#[test]
fn enter_applies_the_selected_option_and_detach_is_selected_first() {
    assert_eq!(
        key(BareKey::Enter),
        CloseDialogueInput::Choose(CloseDialogueOutcome::Detach)
    );
    assert_eq!(
        key_from(BareKey::Enter, CANCEL_OPTION),
        CloseDialogueInput::Choose(CloseDialogueOutcome::Cancel)
    );
    assert_eq!(
        bytes(b"\r", CANCEL_OPTION),
        CloseDialogueInput::Choose(CloseDialogueOutcome::Cancel)
    );
    assert_eq!(
        CloseDialogue::new(PaneId::Terminal(1), String::new()).selected,
        DETACH_OPTION
    );
}

#[test]
fn y_detaches_and_n_or_escape_cancel() {
    let detach = CloseDialogueInput::Choose(CloseDialogueOutcome::Detach);
    let cancel = CloseDialogueInput::Choose(CloseDialogueOutcome::Cancel);
    assert_eq!(key_from(BareKey::Char('y'), CANCEL_OPTION), detach);
    assert_eq!(key(BareKey::Char('n')), cancel);
    assert_eq!(key(BareKey::Esc), cancel);
    assert_eq!(bytes(b"y", 1), detach);
    assert_eq!(bytes(b"n", 0), cancel);
    assert_eq!(bytes(b"\x1b", 0), cancel);
}

#[test]
fn everything_else_is_swallowed() {
    assert_eq!(key(BareKey::Char('x')), CloseDialogueInput::Swallow);
    assert_eq!(key(BareKey::Tab), CloseDialogueInput::Swallow);
    assert_eq!(
        close_dialogue_input(
            Some(&KeyWithModifier::new(BareKey::Char('p')).with_ctrl_modifier()),
            &[0x10],
            0
        ),
        CloseDialogueInput::Swallow
    );
    assert_eq!(
        close_dialogue_input(
            Some(&KeyWithModifier::new(BareKey::Char('y')).with_ctrl_modifier()),
            &[0x19],
            0
        ),
        CloseDialogueInput::Swallow,
        "a modified y is not a yes"
    );
    assert_eq!(bytes(b"hello", 0), CloseDialogueInput::Swallow);
}
