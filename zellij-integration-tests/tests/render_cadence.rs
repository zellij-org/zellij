#![cfg(unix)]

use std::thread::sleep;
use std::time::Duration;
use zellij_integration_tests::{claim_first_terminal_and_wait_for_prompt, start_zellij};

const WITHIN_THE_COALESCE_WINDOW: Duration = Duration::from_millis(1);

#[test]
fn output_arriving_within_the_coalesce_window_is_still_rendered() {
    let mut zellij = start_zellij();
    let terminal = claim_first_terminal_and_wait_for_prompt(&zellij);

    let mut burst = String::new();
    for index in 0..40 {
        burst.push_str(&format!("burst line {:02}\r\n", index));
    }
    terminal.output(burst.as_bytes());
    sleep(WITHIN_THE_COALESCE_WINDOW);
    terminal.output(b"BURST-END");

    zellij.wait_until("the tail of the burst was rendered", |grid_snapshot| {
        grid_snapshot.contains("BURST-END")
    });
    zellij.quit();
}

#[test]
fn an_alternate_screen_painted_in_chunks_is_fully_rendered() {
    let mut zellij = start_zellij();
    let terminal = claim_first_terminal_and_wait_for_prompt(&zellij);

    let mut paint = String::from("\x1b[?1049h\x1b[H");
    for index in 0..20 {
        paint.push_str(&format!("\x1b[{};1Halt screen row {:02}", index + 1, index));
    }
    terminal.output(paint.as_bytes());
    sleep(WITHIN_THE_COALESCE_WINDOW);
    terminal.output(b"\x1b[20;30HALT-END");

    let grid_snapshot = zellij.wait_until("the alternate screen paint", |grid_snapshot| {
        grid_snapshot.contains("ALT-END")
    });
    assert!(grid_snapshot.contains("alt screen row 00"));
    assert!(grid_snapshot.contains("alt screen row 09"));
    zellij.quit();
}
