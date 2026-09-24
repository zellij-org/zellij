#![cfg(unix)]

use zellij_client::session_resolution::{self, ResolutionFailure, Stream};
use zellij_client::ClientInfo;
use zellij_integration_tests::{keys, LayoutInfo, TestRunner, TestSession, PROMPT, TERMINAL_SIZE};
use zellij_utils::cli::AttachArgs;
use zellij_utils::consts::session_layout_cache_file_name;
use zellij_utils::input::options::Options;

const RESURRECT_LAYOUT: &str = r#"
layout {
    default_tab_template {
        pane size=1 borderless=true {
            plugin location="tab-bar"
        }
        children
        pane size=1 borderless=true {
            plugin location="status-bar"
        }
    }
    tab name="alpha" {
        pane
    }
}
"#;

fn named(session_name: Option<&str>) -> AttachArgs {
    AttachArgs {
        session_name: session_name.map(|name| name.to_owned()),
        ..AttachArgs::default()
    }
}

fn resolved(args: &AttachArgs) -> Result<ClientInfo, ResolutionFailure> {
    zellij_integration_tests::test_env::init();
    session_resolution::resolve_attach(args, Options::default(), false, None)
        .map(|resolved| resolved.client)
}

fn start_named_session() -> TestSession {
    let zellij = TestRunner::new(TERMINAL_SIZE).start();
    let terminal = zellij.expect_pty_spawn();
    terminal.output(PROMPT);
    zellij.wait_until("the session is up", |grid_snapshot| {
        grid_snapshot.status_bar_appears() && grid_snapshot.tab_bar_appears()
    });
    zellij
}

#[test]
fn a_live_session_named_in_full_resolves_to_an_attach() {
    let mut zellij = start_named_session();
    let session_name = zellij.session_name().to_owned();

    match resolved(&named(Some(&session_name))) {
        Ok(ClientInfo::Attach(name, _)) => assert_eq!(name, session_name),
        other => panic!("a live session resolved to {:?}", other.map(described)),
    }

    zellij.quit();
}

#[test]
fn a_live_session_named_by_a_unique_prefix_resolves_to_an_attach_on_its_full_name() {
    let mut zellij = start_named_session();
    let session_name = zellij.session_name().to_owned();
    let prefix = &session_name[..session_name.len() - 1];

    match resolved(&named(Some(prefix))) {
        Ok(ClientInfo::Attach(name, _)) => assert_eq!(name, session_name),
        other => panic!("a unique prefix resolved to {:?}", other.map(described)),
    }

    zellij.quit();
}

#[test]
fn a_dead_session_with_a_serialized_layout_resolves_to_a_resurrection_that_opens() {
    let mut zellij = TestRunner::new(TERMINAL_SIZE)
        .with_config("session_serialization true")
        .with_layout(LayoutInfo::Stringified(RESURRECT_LAYOUT.to_string()))
        .start();
    zellij.wait_until("the layout is up", |grid_snapshot| {
        grid_snapshot.contains("alpha") && grid_snapshot.status_bar_appears()
    });
    zellij.send_stdin(&keys::ctrl('p'));
    zellij.send_stdin(&keys::key('r'));
    let new_pane = zellij.expect_pty_spawn();
    new_pane.output(PROMPT);
    zellij.wait_until("a second pane exists to be serialized", |grid_snapshot| {
        grid_snapshot.contains("│")
    });
    zellij.save_session();
    zellij.wait_for_serialized_session();
    let session_name = zellij.session_name().to_owned();
    zellij.quit();

    match resolved(&named(Some(&session_name))) {
        Ok(ClientInfo::Resurrect(name, layout_path, force_run_commands, cwd)) => {
            assert_eq!(name, session_name);
            assert_eq!(layout_path, session_layout_cache_file_name(&session_name));
            assert!(!force_run_commands);
            assert_eq!(cwd, None);
        },
        other => panic!(
            "a dead resurrectable session resolved to {:?}",
            other.map(described)
        ),
    }

    zellij.resurrect(TERMINAL_SIZE);
    zellij.wait_until("the resurrected session is up", |grid_snapshot| {
        grid_snapshot.contains("alpha") && grid_snapshot.status_bar_appears()
    });
    zellij.quit();
}

#[test]
fn an_unknown_name_is_refused_on_stderr_and_created_only_when_asked() {
    let unknown = "attach-parity-no-such-session";

    let failure = resolved(&named(Some(unknown))).expect_err("an unknown name to be refused");
    assert_eq!(
        failure.message,
        format!("No session with the name '{}' found!", unknown)
    );
    assert_eq!(failure.stream, Stream::Err);
    assert_eq!(failure.exit_code, 1);

    let mut creating = named(Some(unknown));
    creating.create = true;
    match resolved(&creating) {
        Ok(ClientInfo::New(name, layout, _, _)) => {
            assert_eq!(name, unknown);
            assert!(layout.is_none());
        },
        other => panic!("--create resolved to {:?}", other.map(described)),
    }
}

#[test]
fn no_name_at_all_takes_the_only_live_session_and_refuses_when_there_is_none() {
    let failure = resolved(&named(None)).expect_err("no session to attach to");
    assert_eq!(failure.message, "No active zellij sessions found.");
    assert_eq!(failure.stream, Stream::Err);

    let mut zellij = start_named_session();
    let session_name = zellij.session_name().to_owned();
    match resolved(&named(None)) {
        Ok(ClientInfo::Attach(name, _)) => assert_eq!(name, session_name),
        other => panic!("a lone live session resolved to {:?}", other.map(described)),
    }
    zellij.quit();
}

fn described(client: ClientInfo) -> String {
    match client {
        ClientInfo::Attach(name, _) => format!("Attach({})", name),
        ClientInfo::New(name, layout, _, _) => format!("New({}, {:?})", name, layout),
        ClientInfo::Resurrect(name, path, force, cwd) => {
            format!("Resurrect({}, {:?}, {}, {:?})", name, path, force, cwd)
        },
        ClientInfo::Watch(name, _) => format!("Watch({})", name),
    }
}
