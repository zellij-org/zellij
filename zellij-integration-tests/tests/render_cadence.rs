#![cfg(unix)]

use std::thread::sleep;
use std::time::Duration;
use zellij_integration_tests::client_screen::render_bytes;
use zellij_integration_tests::{
    claim_first_terminal_and_wait_for_prompt, start_zellij, HostTerminal, TestRunner, TERMINAL_SIZE,
};

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

#[test]
fn a_redraw_split_across_pty_reads_is_never_shown_half_done() {
    let (tap_tx, tap_rx) = crossbeam::channel::unbounded::<Vec<u8>>();
    let mut zellij = TestRunner::new(TERMINAL_SIZE)
        .with_stdout_tap(tap_tx)
        .with_host_terminal(HostTerminal::Basic)
        .start();
    let terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    terminal.output(b"PROGRESS 00");
    zellij.wait_until("the first progress line", |grid_snapshot| {
        grid_snapshot.contains("PROGRESS 00")
    });

    let mut frames: Vec<Vec<u8>> = tap_rx.try_iter().collect();
    let settled = frames.len();

    for round in 1..=10 {
        terminal.output(b"\r\x1b[2K");
        terminal.output(format!("PROGRESS {:02}", round).as_bytes());
        let expected = format!("PROGRESS {:02}", round);
        zellij.wait_until("the redrawn progress line", |grid_snapshot| {
            grid_snapshot.contains(&expected)
        });
    }
    frames.extend(tap_rx.try_iter());

    let mut replayed: Vec<u8> = frames[..settled].concat();
    for (index, frame) in frames[settled..].iter().enumerate() {
        replayed.extend_from_slice(frame);
        let grid_snapshot = render_bytes(&replayed, TERMINAL_SIZE);
        assert!(
            grid_snapshot.contains("PROGRESS"),
            "frame {} showed the progress line cleared but not yet redrawn:\n{}",
            settled + index,
            grid_snapshot.text
        );
    }
    zellij.quit();
}
