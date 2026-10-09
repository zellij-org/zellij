#![cfg(unix)]

use zellij_integration_tests::{
    claim_first_terminal_and_wait_for_prompt, keys, start_zellij, GridSnapshot, TestRunner,
    TestSession, TERMINAL_SIZE,
};

fn session_manager_pane_count(grid_snapshot: &GridSnapshot) -> usize {
    grid_snapshot
        .lines()
        .iter()
        .map(|line| {
            line.matches("Session Manager").count() + line.matches("session-manager").count()
        })
        .sum()
}

fn focus_the_about_plugin(zellij: &TestSession) {
    zellij.send_stdin(&keys::ctrl('o'));
    zellij.send_stdin(&keys::key('a'));
    zellij.wait_until("about plugin shown", |grid_snapshot| {
        grid_snapshot.contains("About Zellij")
    });
}

fn press_launch_or_focus_three_times(zellij: &TestSession, launch_keys: &[&[u8]]) {
    for _ in 0..3 {
        for key in launch_keys {
            zellij.send_stdin(key);
        }
        zellij.wait_until("session manager shown", |grid_snapshot| {
            session_manager_pane_count(grid_snapshot) > 0
        });
    }
}

fn session_managers_once_all_launches_are_handled(zellij: &TestSession) -> GridSnapshot {
    zellij.send_stdin(&keys::ctrl('o'));
    zellij.send_stdin(&keys::key('l'));
    zellij.wait_until(
        "layout manager opened after every earlier launch",
        |grid_snapshot| grid_snapshot.contains("Layout Manager"),
    );
    zellij.send_stdin(&keys::ctrl('p'));
    zellij.send_stdin(&keys::key('x'));
    zellij.wait_until("layout manager closed", |grid_snapshot| {
        !grid_snapshot.contains("Layout Manager") && grid_snapshot.status_bar_appears()
    })
}

fn assert_one_session_manager(zellij: &TestSession) {
    let grid_snapshot = session_managers_once_all_launches_are_handled(zellij);
    assert_eq!(
        session_manager_pane_count(&grid_snapshot),
        1,
        "\n{}",
        grid_snapshot.text
    );
}

fn start_with_session_manager_binding(launch_action: &str, binding_settings: &str) -> TestSession {
    let config = format!(
        r#"
keybinds {{
    shared {{
        bind "Ctrl y" {{
            {} "session-manager" {{
                {}
            }};
        }}
    }}
}}
"#,
        launch_action, binding_settings
    );
    let zellij = TestRunner::new(TERMINAL_SIZE).with_config(&config).start();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    zellij
}

#[test]
fn launch_or_focus_plugin_with_the_default_alias_binding_reuses_the_running_instance() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    focus_the_about_plugin(&zellij);

    press_launch_or_focus_three_times(&zellij, &[&keys::ctrl('o'), &keys::key('w')]);

    assert_one_session_manager(&zellij);
    zellij.quit();
}

#[test]
fn launch_or_focus_plugin_with_an_alias_and_a_plugin_setting_reuses_the_running_instance() {
    let mut zellij = start_with_session_manager_binding(
        "LaunchOrFocusPlugin",
        "floating true; ignore_case true",
    );
    focus_the_about_plugin(&zellij);

    press_launch_or_focus_three_times(&zellij, &[&keys::ctrl('y')]);

    assert_one_session_manager(&zellij);
    zellij.quit();
}

#[test]
fn launch_or_focus_plugin_with_an_alias_and_move_to_focused_pane_reuses_the_running_instance() {
    let mut zellij = start_with_session_manager_binding(
        "LaunchOrFocusPlugin",
        "floating true; move_to_focused_pane true",
    );
    focus_the_about_plugin(&zellij);

    press_launch_or_focus_three_times(&zellij, &[&keys::ctrl('y')]);

    assert_one_session_manager(&zellij);
    zellij.quit();
}

#[test]
fn launch_or_focus_plugin_with_a_zellij_url_reuses_the_running_instance() {
    let mut zellij = TestRunner::new(TERMINAL_SIZE)
        .with_config(
            r#"
keybinds {
    shared {
        bind "Ctrl y" {
            LaunchOrFocusPlugin "zellij:session-manager" {
                floating true
                move_to_focused_tab true
            };
        }
    }
}
"#,
        )
        .start();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    focus_the_about_plugin(&zellij);

    press_launch_or_focus_three_times(&zellij, &[&keys::ctrl('y')]);

    assert_one_session_manager(&zellij);
    zellij.quit();
}

#[test]
fn launch_plugin_opens_a_new_session_manager_every_time() {
    let mut zellij = start_with_session_manager_binding("LaunchPlugin", "floating true");
    focus_the_about_plugin(&zellij);

    press_launch_or_focus_three_times(&zellij, &[&keys::ctrl('y')]);

    let grid_snapshot = session_managers_once_all_launches_are_handled(&zellij);
    assert_eq!(
        session_manager_pane_count(&grid_snapshot),
        3,
        "\n{}",
        grid_snapshot.text
    );
    zellij.quit();
}
