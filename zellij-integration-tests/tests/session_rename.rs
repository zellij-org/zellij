#![cfg(unix)]

use std::time::{Duration, Instant};

use zellij_integration_tests::{
    claim_first_terminal_and_wait_for_prompt, col, default_timeout, keys, TestRunner,
    TERMINAL_SIZE,
};
use zellij_utils::cli::CliAction;
use zellij_utils::sessions::resolve_session_socket_path;

/// Renaming a session must not move its socket / named pipe — named pipes cannot
/// be renamed on Windows, which is the bug session ids exist to fix. Instead
/// the session index maps the new display name onto the same, unchanged
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

    // The server processes the rename asynchronously; wait for the index to
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

fn wait_for_session_name(name: &str) {
    let deadline = Instant::now() + default_timeout();
    while resolve_session_socket_path(name).is_none() {
        assert!(
            Instant::now() < deadline,
            "timed out waiting for the session to be renamed to {:?}",
            name
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn rename_from_session_mode_shows_the_draft_live_and_applies_it_on_enter() {
    let zellij = TestRunner::new(TERMINAL_SIZE)
        .with_config("mirror_session false")
        .start();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    let old_name = zellij.session_name().to_string();
    let other_client = zellij.attach_client_by_name(&old_name, TERMINAL_SIZE);
    other_client.wait_until("second client loads the app", |grid_snapshot| {
        grid_snapshot.tab_bar_appears() && grid_snapshot.status_bar_appears()
    });

    zellij.send_stdin(&keys::ctrl('o'));
    zellij.wait_until("session mode active", |grid_snapshot| {
        grid_snapshot.contains("Detach")
    });
    zellij.send_stdin(&keys::key('r'));
    zellij.wait_until("the session name is cleared", |grid_snapshot| {
        grid_snapshot.contains("(Enter name...)")
    });
    let new_name = "renamed-with-keys";
    for character in new_name.chars() {
        zellij.send_stdin(&keys::key(character));
    }
    zellij.wait_until("the draft is shown in the bar", |grid_snapshot| {
        grid_snapshot.contains(&format!("({})", new_name))
    });
    other_client.wait_until("the other client keeps the old name", |grid_snapshot| {
        grid_snapshot.contains(&format!("({})", old_name))
    });
    assert!(
        resolve_session_socket_path(&old_name).is_some(),
        "the session must not be renamed while typing"
    );

    let bytes_before_enter = zellij.raw_bytes().len();
    zellij.send_stdin(&keys::ENTER);
    wait_for_session_name(new_name);
    other_client.wait_until("the other client sees the new name", |grid_snapshot| {
        grid_snapshot.contains(&format!("({})", new_name))
    });
    zellij.wait_until("the status bar leaves session mode", |grid_snapshot| {
        !grid_snapshot.contains("RENAMING SESSION") && !grid_snapshot.contains("Detach")
    });
    let after_enter = String::from_utf8_lossy(&zellij.raw_bytes()[bytes_before_enter..]).into_owned();
    assert!(
        !after_enter.contains(&format!("({})", old_name)),
        "the old session name flashed in the bar after pressing Enter"
    );
}

#[test]
fn escape_cancels_a_session_rename() {
    let zellij = TestRunner::new(TERMINAL_SIZE).start();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    let old_name = zellij.session_name().to_string();

    zellij.send_stdin(&keys::ctrl('o'));
    zellij.wait_until("session mode active", |grid_snapshot| {
        grid_snapshot.contains("Detach")
    });
    zellij.send_stdin(&keys::key('r'));
    for character in "never-applied".chars() {
        zellij.send_stdin(&keys::key(character));
    }
    zellij.wait_until("the draft is shown in the bar", |grid_snapshot| {
        grid_snapshot.contains("(never-applied)")
    });
    zellij.send_stdin(&keys::ESC);
    zellij.wait_until("the old name is back", |grid_snapshot| {
        grid_snapshot.contains(&format!("({})", old_name))
    });
    assert!(resolve_session_socket_path("never-applied").is_none());
    assert!(resolve_session_socket_path(&old_name).is_some());
}

#[test]
fn a_rejected_session_rename_shows_why_and_restores_the_name() {
    let zellij = TestRunner::new(TERMINAL_SIZE).start();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    let old_name = zellij.session_name().to_string();

    zellij.send_stdin(&keys::ctrl('o'));
    zellij.wait_until("session mode active", |grid_snapshot| {
        grid_snapshot.contains("Detach")
    });
    zellij.send_stdin(&keys::key('r'));
    zellij.wait_until("the session name is cleared", |grid_snapshot| {
        grid_snapshot.contains("(Enter name...)")
    });
    zellij.send_stdin(&keys::ENTER);
    zellij.wait_until(
        "a popup explains the failure and the old name is back",
        |grid_snapshot| {
            grid_snapshot.contains("Could not rename session")
                && grid_snapshot.contains(&format!("({})", old_name))
        },
    );
    assert!(resolve_session_socket_path(&old_name).is_some());
}

#[test]
fn renaming_to_the_name_of_another_running_session_is_rejected() {
    let other = TestRunner::new(TERMINAL_SIZE)
        .skip_concurrency_slot()
        .start_in_background();
    let other_name = other.session_name().to_string();
    wait_for_session_name(&other_name);

    let zellij = TestRunner::new(TERMINAL_SIZE).start();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    let old_name = zellij.session_name().to_string();

    zellij.send_stdin(&keys::ctrl('o'));
    zellij.wait_until("session mode active", |grid_snapshot| {
        grid_snapshot.contains("Detach")
    });
    zellij.send_stdin(&keys::key('r'));
    zellij.wait_until("the session name is cleared", |grid_snapshot| {
        grid_snapshot.contains("(Enter name...)")
    });
    for character in other_name.chars() {
        zellij.send_stdin(&keys::key(character));
    }
    zellij.wait_until("the draft is shown in the bar", |grid_snapshot| {
        grid_snapshot.contains(&format!("({})", other_name))
    });
    zellij.send_stdin(&keys::ENTER);
    zellij.wait_until(
        "a popup says the name is taken and the old name is back",
        |grid_snapshot| {
            grid_snapshot.contains("Could not rename session")
                && grid_snapshot.contains("already exists")
                && grid_snapshot.contains(&format!("({})", old_name))
        },
    );
    assert!(resolve_session_socket_path(&old_name).is_some());
    drop(other);
}
