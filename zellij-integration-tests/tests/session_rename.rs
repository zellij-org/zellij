#![cfg(unix)]

use std::time::{Duration, Instant};

use zellij_integration_tests::{
    claim_first_terminal_and_wait_for_prompt, col, default_timeout, TestRunner, TERMINAL_SIZE,
};
use zellij_utils::cli::CliAction;
use zellij_utils::sessions::resolve_session_socket_path;

/// Renaming a session must not move its socket / named pipe — named pipes cannot
/// be renamed on Windows, which is the bug this whole registry exists to fix.
/// Instead the registry remaps the new display name onto the same, unchanged
/// pipe, so a client can reattach using the new name.
#[test]
fn rename_keeps_socket_and_reattaches_by_new_name() {
    let zellij = TestRunner::new(TERMINAL_SIZE)
        .with_config("mirror_session true")
        .start();
    claim_first_terminal_and_wait_for_prompt(&zellij);

    let old_name = zellij.session_name().to_string();
    let pipe_before = resolve_session_socket_path(&old_name)
        .expect("the running session should resolve to a socket path before rename");

    let new_name = "renamed-by-integration-test";
    assert_eq!(
        zellij.run_cli_action(CliAction::RenameSession {
            name: new_name.to_string(),
        }),
        0,
        "the rename cli action should exit successfully"
    );

    // The server processes the rename asynchronously; wait for the registry to
    // reflect it (the new name resolving to a running session).
    let deadline = Instant::now() + default_timeout();
    while resolve_session_socket_path(new_name).is_none() {
        assert!(
            Instant::now() < deadline,
            "timed out waiting for the session to be renamed to {:?}",
            new_name
        );
        std::thread::sleep(Duration::from_millis(20));
    }

    // The socket / pipe was NOT renamed: the new name resolves to the exact same
    // path the old name did.
    let pipe_after = resolve_session_socket_path(new_name)
        .expect("the new name should resolve after the rename");
    assert_eq!(
        pipe_before, pipe_after,
        "renaming a session must not move its socket/pipe"
    );

    // The old name no longer refers to a running session.
    assert!(
        resolve_session_socket_path(&old_name).is_none(),
        "the old session name should no longer resolve to a running session"
    );

    // A fresh client can reattach using the new name and load the app.
    let reattached = zellij.attach_client_by_name(new_name, TERMINAL_SIZE);
    reattached.wait_until(
        "reattached client loads the app by the new name",
        |grid_snapshot| {
            grid_snapshot.tab_bar_appears()
                && grid_snapshot.status_bar_appears()
                && grid_snapshot.cursor_is_at(col(2).row(1))
        },
    );
}
