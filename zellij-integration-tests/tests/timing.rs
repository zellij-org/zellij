#![cfg(unix)]

use std::time::Instant;
use zellij_integration_tests::{Size, TestRunner};

#[test]
#[ignore]
fn timing_breakdown() {
    let t0 = Instant::now();
    let mut zellij = TestRunner::new(Size {
        cols: 120,
        rows: 24,
    })
    .start();
    eprintln!("start() returned:        {:>7.1?}", t0.elapsed());

    let shell = zellij.expect_pty_spawn();
    eprintln!("first pty spawned:       {:>7.1?}", t0.elapsed());

    shell.output(b"$ ");
    zellij.wait_until("first pane content rendered", |grid_snapshot| {
        grid_snapshot.contains("$")
    });
    eprintln!("first pane render:       {:>7.1?}", t0.elapsed());

    zellij.wait_until("tab bar rendered", |grid_snapshot| {
        grid_snapshot.tab_bar_appears()
    });
    eprintln!("tab-bar plugin render:   {:>7.1?}", t0.elapsed());

    zellij.wait_until("status bar rendered", |grid_snapshot| {
        grid_snapshot.status_bar_appears()
    });
    eprintln!("status-bar plugin render:{:>7.1?}", t0.elapsed());

    zellij.wait_until("cursor in place", |grid_snapshot| {
        grid_snapshot.cursor_is_at(3, 2)
    });
    eprintln!("full app loaded:         {:>7.1?}", t0.elapsed());

    let t_quit = Instant::now();
    zellij.quit();
    eprintln!(
        "quit/joins:              {:>7.1?}  (total {:?})",
        t_quit.elapsed(),
        t0.elapsed()
    );
}
