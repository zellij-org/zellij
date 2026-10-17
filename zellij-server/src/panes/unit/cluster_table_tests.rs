use crate::panes::cluster_table;
use crate::panes::terminal_character::{TerminalCharacter, MAX_CODE_POINTS_PER_CELL};
use std::cell::RefCell;

fn cell_with(text: &str) -> TerminalCharacter {
    let mut chars = text.chars();
    let mut cell = TerminalCharacter::new(chars.next().unwrap());
    for c in chars {
        assert!(cell.attach(c));
    }
    cell
}

#[cfg(target_arch = "x86_64")]
#[test]
fn terminal_character_stays_sixteen_bytes() {
    assert_eq!(std::mem::size_of::<TerminalCharacter>(), 16);
}

#[test]
fn one_attached_code_point_is_stored_inline() {
    for attached in ['\u{0301}', '\u{FE0F}', '\u{1F3FD}', '\u{10FFFF}'] {
        let mut cell = TerminalCharacter::new('a');
        assert!(!cell.has_attached());
        assert!(cell.attach(attached));
        assert!(cell.has_attached());
        assert_eq!(cell.text(), format!("a{}", attached));
        assert_eq!(cell.code_point_count(), 2);
        assert_eq!(cell.last_char(), attached);
        assert_eq!(cluster_table::live_entries(), 0);
    }
}

#[test]
fn a_third_code_point_moves_the_cell_into_the_table() {
    let cell = cell_with("\u{1F468}\u{200D}\u{1F469}");
    assert_eq!(cluster_table::live_entries(), 1);
    assert_eq!(cell.text(), "\u{1F468}\u{200D}\u{1F469}");
    assert_eq!(cell.code_point_count(), 3);
    assert_eq!(cell.last_char(), '\u{1F469}');
    let mut pushed = String::from(">");
    cell.push_text_to(&mut pushed);
    assert_eq!(pushed, ">\u{1F468}\u{200D}\u{1F469}");
    assert_eq!(cell.utf8_len(), "\u{1F468}\u{200D}\u{1F469}".len());
}

#[test]
fn a_cell_keeps_at_most_thirty_two_code_points() {
    let mut cell = TerminalCharacter::new('a');
    for _ in 1..MAX_CODE_POINTS_PER_CELL {
        assert!(cell.attach('\u{0301}'));
    }
    assert_eq!(cell.code_point_count(), MAX_CODE_POINTS_PER_CELL);
    let before = cell.text().into_owned();
    assert!(!cell.attach('\u{0302}'));
    assert_eq!(cell.text(), before);
}

#[test]
fn identical_texts_share_one_entry_and_are_freed_with_their_last_cell() {
    let first = cell_with("e\u{0301}\u{0302}");
    let second = cell_with("e\u{0301}\u{0302}");
    assert_eq!(cluster_table::live_entries(), 1);
    assert!(first == second);
    let clones: Vec<TerminalCharacter> = (0..5).map(|_| first.clone()).collect();
    assert_eq!(cluster_table::live_entries(), 1);
    drop(clones);
    drop(first);
    assert_eq!(cluster_table::live_entries(), 1);
    drop(second);
    assert_eq!(cluster_table::live_entries(), 0);
    assert_eq!(cluster_table::bytes(), 0);
}

#[test]
fn freed_ids_are_reused() {
    let id = cluster_table::intern("ab").unwrap();
    cluster_table::release(id);
    let reused = cluster_table::intern("cd").unwrap();
    assert_eq!(id, reused);
    assert_eq!(cluster_table::refs_for_tests(reused), 1);
    cluster_table::retain(reused);
    assert_eq!(cluster_table::refs_for_tests(reused), 2);
    cluster_table::release(reused);
    cluster_table::release(reused);
    assert_eq!(cluster_table::live_entries(), 0);
}

#[test]
fn copy_glyph_from_keeps_reference_counts_correct() {
    let tagged = cell_with("a\u{0301}\u{0302}");
    let inline = cell_with("b\u{0303}");
    let plain = TerminalCharacter::new('c');
    let mut target = TerminalCharacter::new('x');
    target.copy_glyph_from(&tagged);
    assert_eq!(target.text(), "a\u{0301}\u{0302}");
    assert_eq!(cluster_table::live_entries(), 1);
    target.copy_glyph_from(&inline);
    assert_eq!(target.text(), "b\u{0303}");
    drop(tagged);
    assert_eq!(cluster_table::live_entries(), 0);
    let tagged_again = cell_with("d\u{0301}\u{0302}");
    target.copy_glyph_from(&tagged_again);
    drop(tagged_again);
    assert_eq!(cluster_table::live_entries(), 1);
    target.copy_glyph_from(&plain);
    assert_eq!(target.text(), "c");
    assert!(!target.has_attached());
    assert_eq!(cluster_table::live_entries(), 0);
}

#[test]
fn equality_compares_the_full_text() {
    assert!(cell_with("e\u{0301}") == cell_with("e\u{0301}"));
    assert!(cell_with("e\u{0301}") != cell_with("e\u{0300}"));
    assert!(cell_with("e\u{0301}") != TerminalCharacter::new('e'));
    assert!(cell_with("e\u{0301}\u{0302}") == cell_with("e\u{0301}\u{0302}"));
    assert!(cell_with("e\u{0301}\u{0302}") != cell_with("e\u{0301}\u{0303}"));
}

#[test]
fn the_byte_limit_refuses_new_entries_until_space_is_freed() {
    cluster_table::set_limit_for_tests(cluster_table::ENTRY_OVERHEAD + 8);
    let first = cell_with("a\u{0301}\u{0302}");
    let mut refused = TerminalCharacter::new('b');
    assert!(refused.attach('\u{0303}'));
    assert!(!refused.attach('\u{0304}'));
    assert_eq!(refused.text(), "b\u{0303}");
    drop(first);
    assert!(refused.attach('\u{0304}'));
    assert_eq!(refused.text(), "b\u{0303}\u{0304}");
    cluster_table::set_limit_for_tests(cluster_table::DEFAULT_BYTE_LIMIT);
}

#[test]
fn dropping_cells_during_thread_teardown_does_not_panic() {
    let handle = std::thread::spawn(|| {
        let _ = cluster_table::bytes();
        thread_local! {
            static HOLDER: RefCell<Vec<TerminalCharacter>> = RefCell::new(Vec::new());
        }
        HOLDER.with(|holder| {
            holder
                .borrow_mut()
                .push(cell_with("a\u{0301}\u{0302}\u{0303}"));
            holder.borrow_mut().push(cell_with("b\u{0301}"));
        });
    });
    assert!(handle.join().is_ok());
}
