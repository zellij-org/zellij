#![cfg(unix)]

use std::collections::HashMap;
use std::path::Path;
use std::time::Instant;

use zellij_integration_tests::{
    claim_first_terminal_and_wait_for_prompt, default_timeout, keys,
    split_right_and_wait_for_prompt, TestRunner, TestSession, TERMINAL_SIZE,
};

fn start_with_background_binding(env_file: &Path) -> TestSession {
    let config = format!(
        r#"
        keybinds {{
            shared {{
                bind "Alt y" {{
                    Run "sh" "-c" "env > \"$1.tmp\" && mv \"$1.tmp\" \"$1\"" "sh" "{}" {{
                        background true;
                    }};
                }}
                bind "Alt n" {{ NewPane; }}
            }}
        }}
        "#,
        env_file.display()
    );
    TestRunner::new(TERMINAL_SIZE).with_config(&config).start()
}

fn wait_for_env_file(env_file: &Path) -> HashMap<String, String> {
    let deadline = Instant::now() + default_timeout();
    loop {
        if let Ok(contents) = std::fs::read_to_string(env_file) {
            return contents
                .lines()
                .filter_map(|line| line.split_once('='))
                .map(|(name, value)| (name.to_owned(), value.to_owned()))
                .collect();
        }
        assert!(
            Instant::now() < deadline,
            "background command did not write {}",
            env_file.display()
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

#[test]
fn a_background_command_runs_without_a_pane_and_knows_the_focused_terminal() {
    let temp_dir = tempfile::tempdir().unwrap();
    let env_file = temp_dir.path().join("env");
    let mut zellij = start_with_background_binding(&env_file);
    claim_first_terminal_and_wait_for_prompt(&zellij);
    let second_terminal = split_right_and_wait_for_prompt(&zellij);

    zellij.send_stdin(&keys::alt('y'));
    let env = wait_for_env_file(&env_file);

    let expected_origin = format!("terminal_{}", second_terminal.terminal_id());
    assert_eq!(env.get("ZELLIJ_ORIGIN_PANE_ID"), Some(&expected_origin));
    assert_eq!(
        env.get("ZELLIJ_PANE_ID"),
        Some(&second_terminal.terminal_id().to_string())
    );

    zellij.send_stdin(&keys::alt('n'));
    let next_terminal = zellij.expect_pty_spawn();
    assert_eq!(
        next_terminal.terminal_id(),
        second_terminal.terminal_id() + 1
    );
    zellij.quit();
}

#[test]
fn a_background_command_from_a_focused_plugin_pane_has_no_terminal_pane_id() {
    let temp_dir = tempfile::tempdir().unwrap();
    let env_file = temp_dir.path().join("env");
    let mut zellij = start_with_background_binding(&env_file);
    claim_first_terminal_and_wait_for_prompt(&zellij);
    zellij.send_stdin(&keys::ctrl('o'));
    zellij.send_stdin(&keys::key('a'));
    zellij.wait_until("about plugin shown", |grid_snapshot| {
        grid_snapshot.contains("About Zellij")
    });

    zellij.send_stdin(&keys::alt('y'));
    let env = wait_for_env_file(&env_file);

    let origin = env
        .get("ZELLIJ_ORIGIN_PANE_ID")
        .expect("origin pane id is set");
    let plugin_id = origin
        .strip_prefix("plugin_")
        .unwrap_or_else(|| panic!("expected a plugin origin, got {}", origin));
    assert!(plugin_id.parse::<u32>().is_ok(), "{}", origin);
    assert_eq!(env.get("ZELLIJ_PANE_ID"), None);
    zellij.quit();
}
