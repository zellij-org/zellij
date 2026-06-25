#![cfg(unix)]

use zellij_integration_tests::{claim_first_terminal_and_wait_for_prompt, keys, start_zellij};

#[test]
fn launch_session_manager_plugin() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);

    zellij.send_stdin(&keys::ctrl('o'));
    zellij.send_stdin(&keys::key('w'));

    zellij.wait_until("session manager plugin opened", |grid_snapshot| {
        grid_snapshot.contains("Session Manager") && grid_snapshot.status_bar_appears()
    });
    zellij.quit();
}

#[test]
fn launch_about_plugin() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);

    zellij.send_stdin(&keys::ctrl('o'));
    zellij.send_stdin(&keys::key('a'));

    zellij.wait_until("about plugin opened", |grid_snapshot| {
        grid_snapshot.contains("About Zellij") && grid_snapshot.status_bar_appears()
    });
    zellij.quit();
}
