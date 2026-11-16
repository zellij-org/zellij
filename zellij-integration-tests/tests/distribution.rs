#![cfg(unix)]

use std::sync::Once;

use zellij_integration_tests::{normalized, LayoutInfo, TestRunner, TERMINAL_SIZE};
use zellij_utils::distribution::{self, Distribution, DistributionLayout};

const FIXTURE_PLUGIN: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../zellij-utils/assets/plugins/fixture-plugin-for-tests.wasm"
));

const FIXTURE_PLUGIN_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../zellij-utils/assets/plugins/fixture-plugin-for-tests.wasm"
);

const PERMISSION_PROMPT: &str = "Allow? (y/n)";

const BUNDLED_LAYOUT: &str = r#"
layout {
    pane size=1 borderless=true {
        plugin location="tab-bar"
    }
    pane
    pane size=10 borderless=true {
        plugin location="zellij:bundled-plugin"
    }
    pane size=2 borderless=true {
        plugin location="status-bar"
    }
}
"#;

static INSTALL: Once = Once::new();

fn install_distribution() {
    INSTALL.call_once(|| {
        distribution::set_distribution(
            Distribution::new("testdist", "9.9.9")
                .with_display_name("Test Distribution")
                .with_plugin("bundled-plugin", FIXTURE_PLUGIN)
                .with_layout(DistributionLayout::new("bundled", BUNDLED_LAYOUT)),
        )
        .expect("the test distribution should install");
    });
}

fn start_with_bundled_layout() -> zellij_integration_tests::TestSession {
    install_distribution();
    TestRunner::new(TERMINAL_SIZE)
        .with_layout(LayoutInfo::BuiltIn("bundled".to_owned()))
        .start()
}

#[test]
fn a_bundled_plugin_runs_without_asking_for_permissions() {
    let mut zellij = start_with_bundled_layout();
    let terminal = zellij.expect_pty_spawn();
    terminal.output(b"$ ");

    let grid_snapshot = zellij.wait_until("bundled plugin rendered its own output", |grid| {
        grid.contains("Received events:") && grid.tab_bar_appears() && grid.status_bar_appears()
    });
    assert!(
        !grid_snapshot.contains(PERMISSION_PROMPT),
        "a bundled plugin must not ask for permissions, got:\n{}",
        grid_snapshot.text
    );
    zellij.quit();
}

#[test]
fn the_same_plugin_loaded_from_a_file_still_asks_for_permissions() {
    install_distribution();
    let file_plugin_layout = format!(
        r#"
        layout {{
            pane size=1 borderless=true {{
                plugin location="tab-bar"
            }}
            pane
            pane size=10 borderless=true {{
                plugin location="file:{}"
            }}
            pane size=2 borderless=true {{
                plugin location="status-bar"
            }}
        }}
        "#,
        FIXTURE_PLUGIN_PATH
    );
    let mut zellij = TestRunner::new(TERMINAL_SIZE)
        .with_layout(LayoutInfo::Stringified(file_plugin_layout))
        .start();
    let terminal = zellij.expect_pty_spawn();
    terminal.output(b"$ ");

    zellij.wait_until("file plugin stops on the permission prompt", |grid| {
        grid.contains(PERMISSION_PROMPT)
    });
    zellij.quit();
}

#[test]
fn a_bundled_layout_is_used_to_lay_out_the_session() {
    let mut zellij = start_with_bundled_layout();
    let terminal = zellij.expect_pty_spawn();
    terminal.output(b"$ ");

    let grid_snapshot = zellij.wait_until("bundled layout rendered", |grid| {
        grid.contains("Received events:") && grid.tab_bar_appears() && grid.status_bar_appears()
    });
    insta::assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn the_session_runs_under_the_distributions_own_directories() {
    let mut zellij = start_with_bundled_layout();
    let terminal = zellij.expect_pty_spawn();
    terminal.output(b"$ ");
    zellij.wait_until("app loaded", |grid| {
        grid.tab_bar_appears() && grid.status_bar_appears()
    });

    let cache_dir = zellij_utils::consts::ZELLIJ_CACHE_DIR.display().to_string();
    assert!(
        cache_dir.contains("testdist"),
        "the running session should use the distribution's cache dir, got '{}'",
        cache_dir
    );
    zellij.quit();
}
