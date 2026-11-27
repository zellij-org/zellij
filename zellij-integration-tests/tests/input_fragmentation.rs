#![cfg(unix)]

use std::thread::sleep;
use std::time::{Duration, Instant};

use zellij_integration_tests::{
    claim_first_terminal_and_wait_for_prompt, keys, start_zellij, TestSession,
};

const IDLE_BEYOND_SHORT_FLUSH: Duration = Duration::from_millis(150);
const IDLE_BEYOND_LONE_ESC_FLUSH: Duration = Duration::from_millis(80);
const FAST_FLUSH_DELIVERY_CEILING: Duration = Duration::from_millis(700);
const PASTE_FRAGMENT_SIZE: usize = 4_000;
const PASTE_SEGMENTS: u32 = 15_000;

fn lock_interface(zellij: &TestSession) {
    zellij.send_stdin(&keys::ctrl('g'));
    zellij.wait_until("interface locked", |grid_snapshot| {
        grid_snapshot.contains("LOCK") && !grid_snapshot.contains("PANE")
    });
}

fn unlock_interface(zellij: &TestSession) {
    zellij.send_stdin(&keys::ctrl('g'));
    zellij.wait_until("interface unlocked", |grid_snapshot| {
        grid_snapshot.contains("PANE")
    });
}

fn contains_subslice(haystack: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() || haystack.len() < needle.len() {
        return false;
    }
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

fn occurrences_of(haystack: &[u8], needle: &[u8]) -> usize {
    if needle.is_empty() || haystack.len() < needle.len() {
        return 0;
    }
    haystack
        .windows(needle.len())
        .filter(|window| *window == needle)
        .count()
}

fn large_paste_payload() -> Vec<u8> {
    let mut payload = Vec::with_capacity(PASTE_SEGMENTS as usize * 8);
    for segment in 0..PASTE_SEGMENTS {
        payload.extend_from_slice(format!("{:08}", segment).as_bytes());
    }
    payload
}

#[test]
fn a_csi_sequence_split_across_delayed_chunks_arrives_without_garbage() {
    let mut zellij = start_zellij();
    let terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    terminal.disable_echo();
    lock_interface(&zellij);

    zellij.send_stdin(b"\x1b[");
    sleep(IDLE_BEYOND_SHORT_FLUSH);
    zellij.send_stdin(b"1;");
    sleep(IDLE_BEYOND_SHORT_FLUSH);
    zellij.send_stdin(b"5");
    sleep(IDLE_BEYOND_SHORT_FLUSH);
    zellij.send_stdin(b"C");
    zellij.send_stdin(b"x");

    let pane_stdin = terminal.wait_for_stdin(
        "the split ctrl-right sequence and the key after it reached the pane",
        |stdin_bytes| contains_subslice(stdin_bytes, b"\x1b[1;5Cx"),
    );
    assert_eq!(
        pane_stdin, b"\x1b[1;5Cx",
        "no bytes other than the sequence and the following key may reach the pane"
    );

    unlock_interface(&zellij);
    zellij.quit();
}

#[test]
fn a_bare_csi_introducer_reaches_the_pane_without_the_reply_guard() {
    let mut zellij = start_zellij();
    let terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    terminal.disable_echo();
    lock_interface(&zellij);

    let sent_at = Instant::now();
    zellij.send_stdin(b"\x1b[");

    let pane_stdin = terminal.wait_for_stdin("the bare Alt+[ reached the pane", |stdin_bytes| {
        contains_subslice(stdin_bytes, b"\x1b[")
    });
    let elapsed = sent_at.elapsed();
    assert_eq!(
        pane_stdin, b"\x1b[",
        "only the Alt+[ bytes may reach the pane"
    );
    assert!(
        elapsed < FAST_FLUSH_DELIVERY_CEILING,
        "Alt+[ must be released by the short idle flush, not the 1s reply guard: took {:?}",
        elapsed
    );

    unlock_interface(&zellij);
    zellij.quit();
}

#[test]
fn a_bare_osc_introducer_reaches_the_pane_without_the_reply_guard() {
    let mut zellij = start_zellij();
    let terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    terminal.disable_echo();
    lock_interface(&zellij);

    let sent_at = Instant::now();
    zellij.send_stdin(b"\x1b]");

    let pane_stdin = terminal.wait_for_stdin("the bare Alt+] reached the pane", |stdin_bytes| {
        contains_subslice(stdin_bytes, b"\x1b]")
    });
    let elapsed = sent_at.elapsed();
    assert_eq!(
        pane_stdin, b"\x1b]",
        "only the Alt+] bytes may reach the pane"
    );
    assert!(
        elapsed < FAST_FLUSH_DELIVERY_CEILING,
        "Alt+] must be released by the short idle flush, not the 1s reply guard: took {:?}",
        elapsed
    );

    unlock_interface(&zellij);
    zellij.quit();
}

#[test]
fn a_key_typed_after_a_bare_osc_introducer_is_not_swallowed() {
    let mut zellij = start_zellij();
    let terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    terminal.disable_echo();
    lock_interface(&zellij);

    zellij.send_stdin(b"\x1b]");
    sleep(IDLE_BEYOND_SHORT_FLUSH);
    let typed_at = Instant::now();
    zellij.send_stdin(b"z");

    let pane_stdin = terminal.wait_for_stdin(
        "the key typed after Alt+] reached the pane",
        |stdin_bytes| stdin_bytes.contains(&b'z'),
    );
    let elapsed = typed_at.elapsed();
    assert_eq!(
        pane_stdin, b"\x1b]z",
        "Alt+] and the key after it must arrive intact and separate"
    );
    assert!(
        elapsed < FAST_FLUSH_DELIVERY_CEILING,
        "the key after Alt+] must not be absorbed into the OSC accumulator: took {:?}",
        elapsed
    );

    unlock_interface(&zellij);
    zellij.quit();
}

#[test]
fn a_large_fragmented_paste_arrives_intact_and_ordered() {
    let mut zellij = start_zellij();
    let terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    terminal.disable_echo();
    lock_interface(&zellij);

    let payload = large_paste_payload();
    assert!(
        payload.len() > 100_000,
        "the paste fixture must exceed 100KB"
    );

    zellij.send_stdin(&keys::BRACKETED_PASTE_START);
    for fragment in payload.chunks(PASTE_FRAGMENT_SIZE) {
        zellij.send_stdin(fragment);
    }
    zellij.send_stdin(&keys::BRACKETED_PASTE_END);

    let pane_stdin = terminal.wait_for_stdin("the whole paste reached the pane", |stdin_bytes| {
        contains_subslice(stdin_bytes, &payload)
    });
    assert_eq!(
        occurrences_of(&pane_stdin, &payload),
        1,
        "the paste must reach the pane exactly once"
    );

    unlock_interface(&zellij);
    zellij.quit();
}

#[test]
fn keystrokes_interleaved_with_fragmented_mouse_motion_do_not_spam_the_pane() {
    let mut zellij = start_zellij();
    let terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    terminal.disable_echo();
    lock_interface(&zellij);

    zellij.send_stdin(b"a\x1b[<35;30;10");
    zellij.send_stdin(b"Mb\x1b[<35;31;10M\x1b[<35;32");
    zellij.send_stdin(b";10Mc");

    let pane_stdin = terminal
        .wait_for_stdin("all three keystrokes reached the pane", |stdin_bytes| {
            contains_subslice(stdin_bytes, b"abc")
        });
    assert_eq!(
        pane_stdin, b"abc",
        "only the keystrokes may reach the pane, never the mouse-report bytes"
    );

    unlock_interface(&zellij);
    zellij.quit();
}

#[test]
fn an_esc_directly_before_a_mouse_report_produces_no_spurious_keys() {
    let mut zellij = start_zellij();
    let terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    terminal.disable_echo();
    lock_interface(&zellij);

    zellij.send_stdin(&keys::ESC);
    zellij.send_stdin(b"\x1b[<35;30;10M");
    zellij.send_stdin(b"z");

    let pane_stdin = terminal.wait_for_stdin(
        "the keystroke after the mouse report reached the pane",
        |stdin_bytes| stdin_bytes.contains(&b'z'),
    );
    assert!(
        pane_stdin == b"z" || pane_stdin == b"\x1bz",
        "only a lone escape and the following key may reach the pane: {:?}",
        pane_stdin
    );

    unlock_interface(&zellij);
    zellij.quit();
}

#[test]
fn a_repeated_key_burst_after_a_paste_swallows_no_keystrokes() {
    let mut zellij = start_zellij();
    let terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    terminal.disable_echo();
    lock_interface(&zellij);

    zellij.send_stdin(b"\x1b[200~clip\x1b[201");
    sleep(IDLE_BEYOND_LONE_ESC_FLUSH);
    zellij.send_stdin(b"~");
    sleep(IDLE_BEYOND_LONE_ESC_FLUSH);
    zellij.send_stdin(b"abcdefghijklmnopqrst");

    let pane_stdin = terminal.wait_for_stdin(
        "the whole key burst following the paste reached the pane",
        |stdin_bytes| contains_subslice(stdin_bytes, b"abcdefghijklmnopqrst"),
    );
    assert_eq!(
        occurrences_of(&pane_stdin, b"abcdefghijklmnopqrst"),
        1,
        "the key burst must reach the pane exactly once: {:?}",
        pane_stdin
    );
    assert_eq!(
        occurrences_of(&pane_stdin, b"clip"),
        1,
        "the paste must reach the pane exactly once: {:?}",
        pane_stdin
    );

    unlock_interface(&zellij);
    zellij.quit();
}
