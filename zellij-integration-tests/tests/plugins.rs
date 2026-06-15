#![cfg(unix)]

use insta::assert_snapshot;
use zellij_integration_tests::{normalized, Size, TestRunner};

const TERMINAL_SIZE: Size = Size {
    cols: 120,
    rows: 24,
};

fn load_background_plugin_config() -> String {
    let plugin_path = format!(
        "{}/../zellij-utils/assets/plugins/fixture-plugin-for-tests.wasm",
        env!("CARGO_MANIFEST_DIR")
    );
    format!(
        "plugins {{\n  permission-requester location=\"file:{}\"\n}}\nload_plugins {{\n  \"permission-requester\"\n}}\n",
        plugin_path
    )
}

#[test]
fn load_plugins_in_background_on_startup() {
    let mut zellij = TestRunner::new(TERMINAL_SIZE)
        .with_config(&load_background_plugin_config())
        .start();
    let terminal = zellij.expect_pty_spawn();
    terminal.output(b"$ ");

    let grid_snapshot = zellij.wait_until("background plugin requests permissions", |grid_snapshot| {
        grid_snapshot.contains("Allow? (y/n)") && grid_snapshot.tab_bar_appears()
    });
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.send_stdin(b"y");
    zellij.wait_until("permission prompt dismissed after granting", |grid_snapshot| {
        !grid_snapshot.contains("Allow? (y/n)")
    });
    zellij.quit();
}
