#![cfg(unix)]

use std::time::Duration;
use zellij_integration_tests::inbox::{background_session_with, ATTENTION_CONFIG};
use zellij_integration_tests::{
    CliCaller, CliOutputHandle, FakePtyHandle, LayoutInfo, TestRunner, TestSession, TERMINAL_SIZE,
};
use zellij_utils::cli::CliAction;

const FIXTURE_PLUGIN_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../zellij-utils/assets/plugins/fixture-plugin-for-tests.wasm"
);
const PERMISSION_PROMPT: &str = "Allow? (y/n)";
const TYPED: &str = "typed by the fixture";

fn fixture_plugin_url() -> String {
    format!("file:{}", FIXTURE_PLUGIN_PATH)
}

fn start_with_the_fixture_plugin(answer: &[u8]) -> TestSession {
    let layout = format!(
        r#"
        layout {{
            pane size=1 borderless=true {{
                plugin location="tab-bar"
            }}
            pane focus=true {{
                plugin location="{}"
            }}
            pane
            pane size=2 borderless=true {{
                plugin location="status-bar"
            }}
        }}
        "#,
        fixture_plugin_url()
    );
    let zellij = TestRunner::new(TERMINAL_SIZE)
        .with_layout(LayoutInfo::Stringified(layout))
        .start();
    let terminal = zellij.expect_pty_spawn();
    terminal.output(b"$ ");
    zellij.wait_until("the fixture plugin asks for permissions", |grid_snapshot| {
        grid_snapshot.contains(PERMISSION_PROMPT)
    });
    zellij.send_stdin(answer);
    zellij.wait_until("the fixture plugin is running", |grid_snapshot| {
        !grid_snapshot.contains(PERMISSION_PROMPT)
            && grid_snapshot.contains("Rows:")
            && grid_snapshot.status_bar_appears()
    });
    zellij
}

fn write_into(zellij: &TestSession, session: &str, terminal: &FakePtyHandle) -> CliOutputHandle {
    let args = format!(
        "command=write,session={},pane={}",
        session,
        terminal.terminal_id()
    );
    let pipe = CliAction::Pipe {
        name: Some("session_pane_command".to_owned()),
        payload: None,
        args: Some(args.parse().unwrap()),
        plugin: Some(fixture_plugin_url()),
        plugin_configuration: None,
        force_launch_plugin: false,
        skip_plugin_cache: false,
        floating_plugin: None,
        in_place_plugin: None,
        plugin_cwd: None,
        plugin_title: None,
        popup: None,
        popup_no_focus: false,
        popup_capture_all_keys: false,
        popup_outside_click: None,
    };
    zellij.run_cli_pipe(
        pipe,
        CliCaller {
            pane_id: None,
            stdin: None,
        },
    )
}

#[test]
fn a_plugin_without_write_permission_cannot_type_into_another_session() {
    let (other, terminal) = background_session_with(ATTENTION_CONFIG);
    let mut zellij = start_with_the_fixture_plugin(b"n");
    let before = terminal.stdin_bytes().len();
    let _pipe = write_into(&zellij, other.session_name(), &terminal);
    std::thread::sleep(Duration::from_millis(1500));
    let typed = String::from_utf8_lossy(&terminal.stdin_bytes()[before..]).into_owned();
    assert!(
        !typed.contains(TYPED),
        "a plugin without permission typed into another session: {:?}",
        typed
    );
    zellij.quit();
}

#[test]
fn a_plugin_with_write_permission_types_into_another_session() {
    let (other, terminal) = background_session_with(ATTENTION_CONFIG);
    let mut zellij = start_with_the_fixture_plugin(b"y");
    write_into(&zellij, other.session_name(), &terminal).wait_for_exit();
    terminal.wait_for_stdin("the plugin typed into the other session", |stdin| {
        String::from_utf8_lossy(stdin).contains(TYPED)
    });
    zellij.quit();
}
