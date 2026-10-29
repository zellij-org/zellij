use std::net::{IpAddr, Ipv4Addr};

use clap::{CommandFactory, Parser, Subcommand};
use zellij_utils::cli::{AttachArgs, CliArgs, Command, Sessions, TerminalArgs};

const CLI_PARSE_TEST_STACK_SIZE: usize = 16 * 1024 * 1024;

fn on_large_stack<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    std::thread::Builder::new()
        .stack_size(CLI_PARSE_TEST_STACK_SIZE)
        .spawn(f)
        .expect("failed to spawn cli parse test thread")
        .join()
        .expect("cli parse test thread panicked")
}

fn try_parse(args: &[&str]) -> Result<CliArgs, clap::Error> {
    let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    on_large_stack(move || CliArgs::try_parse_from(args))
}

#[test]
fn verify_cli() {
    on_large_stack(|| CliArgs::command().debug_assert());
}

#[test]
fn web_cli_status_alone_works() {
    let args = try_parse(&["zellij", "web", "--status"]);
    assert!(args.is_ok());
    if let Ok(CliArgs {
        command: Some(Command::Web(web)),
        ..
    }) = args
    {
        assert!(web.status);
        assert!(web.timeout.is_none());
    } else {
        panic!("Expected Web command");
    }
}

#[test]
fn web_cli_status_with_timeout_works() {
    let args = try_parse(&["zellij", "web", "--status", "--timeout", "5"]);
    assert!(args.is_ok());
    if let Ok(CliArgs {
        command: Some(Command::Web(web)),
        ..
    }) = args
    {
        assert!(web.status);
        assert_eq!(web.timeout, Some(5));
    } else {
        panic!("Expected Web command");
    }
}

#[test]
fn web_cli_timeout_with_status_works() {
    // Test with --timeout before --status (order shouldn't matter)
    let args = try_parse(&["zellij", "web", "--timeout", "10", "--status"]);
    assert!(args.is_ok());
    if let Ok(CliArgs {
        command: Some(Command::Web(web)),
        ..
    }) = args
    {
        assert!(web.status);
        assert_eq!(web.timeout, Some(10));
    } else {
        panic!("Expected Web command");
    }
}

#[test]
fn web_cli_timeout_without_status_fails() {
    let args = try_parse(&["zellij", "web", "--timeout", "5"]);
    assert!(args.is_err());
}

#[test]
fn web_cli_status_with_start_fails() {
    let args = try_parse(&["zellij", "web", "--status", "--start"]);
    assert!(args.is_err());
}

#[test]
fn web_cli_status_with_stop_fails() {
    let args = try_parse(&["zellij", "web", "--status", "--stop"]);
    assert!(args.is_err());
}

#[test]
fn web_cli_status_with_ip_works() {
    let args = try_parse(&["zellij", "web", "--status", "--ip", "127.0.0.1"]);
    assert!(args.is_ok());
    if let Ok(CliArgs {
        command: Some(Command::Web(web)),
        ..
    }) = args
    {
        assert!(web.status);
        assert_eq!(web.ip, Some(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1))));
    } else {
        panic!("Expected Web command");
    }
}

#[test]
fn web_cli_status_with_port_works() {
    let args = try_parse(&["zellij", "web", "--status", "--port", "9000"]);
    assert!(args.is_ok());
    if let Ok(CliArgs {
        command: Some(Command::Web(web)),
        ..
    }) = args
    {
        assert!(web.status);
        assert_eq!(web.port, Some(9000));
    } else {
        panic!("Expected Web command");
    }
}

#[test]
fn web_cli_status_with_ip_and_port_works() {
    let args = try_parse(&[
        "zellij", "web", "--status", "--ip", "0.0.0.0", "--port", "9000",
    ]);
    assert!(args.is_ok());
    if let Ok(CliArgs {
        command: Some(Command::Web(web)),
        ..
    }) = args
    {
        assert!(web.status);
        assert_eq!(web.ip, Some(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0))));
        assert_eq!(web.port, Some(9000));
    } else {
        panic!("Expected Web command");
    }
}

fn attach_args(argv: &[&str]) -> AttachArgs {
    let mut args = vec!["zellij", "attach"];
    args.extend_from_slice(argv);
    match try_parse(&args).map(|parsed| parsed.command) {
        Ok(Some(Command::Sessions(Sessions::Attach { args, .. }))) => args,
        other => panic!("expected an attach command, got {:?}", other),
    }
}

fn terminal_args(argv: &[&str]) -> TerminalArgs {
    let mut args = vec!["zellij", "terminal"];
    args.extend_from_slice(argv);
    match try_parse(&args).map(|parsed| parsed.command) {
        Ok(Some(Command::Terminal(args))) => args,
        other => panic!("expected a window command, got {:?}", other),
    }
}

#[test]
fn the_window_parses_attachs_flags_exactly_as_attach_does() {
    for argv in [
        vec!["my-session"],
        vec!["my-session", "--create"],
        vec!["my-session", "-c", "--force-run-commands"],
        vec!["--index", "2"],
        vec!["https://example.com/my-session", "--token", "abc"],
        vec![
            "https://example.com/my-session",
            "--token",
            "abc",
            "--remember",
            "--forget",
            "--ca-cert",
            "/tmp/ca.pem",
            "--insecure",
        ],
        vec!["my-session", "--", "htop"],
        vec!["my-session", "--close-on-exit", "--", "htop"],
        vec!["my-session", "--start-suspended", "--", "htop"],
        vec!["my-session", "options", "--simplified-ui", "true"],
    ] {
        let attach = attach_args(&argv);
        let window = terminal_args(&argv).attach;
        assert_eq!(
            format!("{:?}", attach),
            format!("{:?}", window),
            "`zellij attach {}` and `zellij terminal {}` disagree",
            argv.join(" "),
            argv.join(" ")
        );
    }
}

#[test]
fn a_backgrounded_session_is_a_usage_error_on_the_window() {
    assert!(try_parse(&["zellij", "attach", "-b", "my-session"]).is_ok());
    assert!(try_parse(&["zellij", "terminal", "-b", "my-session"]).is_err());
    assert!(try_parse(&["zellij", "terminal", "--create-background", "my-session"]).is_err());
}

#[test]
fn the_window_takes_a_command_the_way_terminal_emulators_do() {
    let args = terminal_args(&["-e", "vim", "-u", "NONE", "--clean", "file.txt"]);
    assert_eq!(
        args.execute,
        vec!["vim", "-u", "NONE", "--clean", "file.txt"]
    );
    assert_eq!(
        args.command(),
        &["vim", "-u", "NONE", "--clean", "file.txt"]
    );
    assert!(args.waits_for_command());
    assert!(args.closes_command_on_exit());
    assert!(args.attach.session_name.is_none());

    let held = terminal_args(&["--hold", "--working-directory=/tmp", "-e", "ls", "-la"]);
    assert!(held.hold);
    assert!(!held.closes_command_on_exit());
    assert_eq!(
        held.working_directory,
        Some(std::path::PathBuf::from("/tmp"))
    );
    assert_eq!(held.execute, vec!["ls", "-la"]);

    let spaced = terminal_args(&["--working-directory", "/srv", "-e", "htop"]);
    assert_eq!(
        spaced.working_directory,
        Some(std::path::PathBuf::from("/srv"))
    );

    let plain = terminal_args(&["--", "htop"]);
    assert!(!plain.waits_for_command());
    assert!(!plain.closes_command_on_exit());
    assert_eq!(plain.command(), &["htop"]);
}

#[test]
fn a_waited_command_refuses_the_flags_that_open_something_else() {
    for argv in [
        vec!["--watch", "-e", "htop"],
        vec!["--inbox", "-e", "htop"],
        vec!["--sessions", "-e", "htop"],
        vec!["--headless", "-e", "htop"],
    ] {
        let mut args = vec!["zellij", "terminal"];
        args.extend_from_slice(&argv);
        assert!(
            try_parse(&args).is_err(),
            "`zellij terminal {}` was accepted",
            argv.join(" ")
        );
    }
    assert!(terminal_args(&["--hold"]).validate().is_err());
    assert!(terminal_args(&["--hold", "-e", "ls"]).validate().is_ok());
}

#[test]
fn the_window_watches_where_attach_cannot() {
    assert!(terminal_args(&["my-session", "--watch"]).watch);
    assert!(terminal_args(&["--watch"]).watch);
    assert!(try_parse(&["zellij", "attach", "--watch", "my-session"]).is_err());
}

#[test]
fn watching_refuses_the_flags_that_would_change_the_session_it_follows() {
    for argv in [
        vec!["--watch", "--create", "my-session"],
        vec!["--watch", "--force-run-commands", "my-session"],
        vec!["--watch", "--index", "1"],
        vec!["--watch", "my-session", "--", "htop"],
    ] {
        let mut args = vec!["zellij", "terminal"];
        args.extend_from_slice(&argv);
        assert!(
            try_parse(&args).is_err(),
            "`zellij terminal {}` was accepted",
            argv.join(" ")
        );
    }
}

#[test]
fn the_fixture_flags_are_on_the_verb_and_hidden_from_it() {
    let args = terminal_args(&[
        "--headless",
        "--record",
        "/tmp/fixture.jsonl",
        "--rows",
        "24",
        "--cols",
        "80",
        "--cell-width",
        "8",
        "--cell-height",
        "16",
        "--duration-secs",
        "2",
    ]);
    assert!(args.headless);
    assert_eq!(args.rows, Some(24));
    assert_eq!(args.cols, Some(80));
    assert_eq!(args.cell_width, 8);
    assert_eq!(args.cell_height, 16);
    assert_eq!(args.duration_secs, Some(2));

    let help = on_large_stack(|| {
        Command::augment_subcommands(clap::Command::new("zellij"))
            .find_subcommand_mut("terminal")
            .expect("the window verb is missing")
            .render_long_help()
            .to_string()
    });
    for hidden in [
        "--headless",
        "--record",
        "--rows",
        "--cols",
        "--cell-width",
        "--cell-height",
        "--duration-secs",
    ] {
        assert!(
            !help.contains(hidden),
            "{} is development tooling and must not be advertised:\n{}",
            hidden,
            help
        );
    }
    assert!(help.contains("--create"), "{}", help);
    assert!(help.contains("--watch"), "{}", help);
}

#[test]
fn the_window_is_described_by_what_it_is_for() {
    let help = on_large_stack(|| {
        Command::augment_subcommands(clap::Command::new("zellij"))
            .render_long_help()
            .to_string()
    });
    assert!(
        help.contains("Open a session in the Zellij terminal, without another terminal emulator"),
        "{}",
        help
    );
}

#[test]
fn the_window_can_toggle_one_panel_of_the_desktop_app() {
    assert!(terminal_args(&["--inbox"]).inbox);
    assert!(terminal_args(&["--sessions"]).sessions);
    for argv in [
        vec!["--inbox", "--sessions"],
        vec!["--inbox", "--standalone"],
        vec!["--sessions", "--watch"],
    ] {
        let mut args = vec!["zellij", "terminal"];
        args.extend_from_slice(&argv);
        assert!(
            try_parse(&args).is_err(),
            "`zellij terminal {}` was accepted",
            argv.join(" ")
        );
    }
}

#[test]
fn the_window_can_run_on_its_own_in_a_separate_socket_folder() {
    let args = terminal_args(&["--standalone", "--socket-dir", "/tmp/elsewhere"]);
    assert!(args.standalone);
    assert_eq!(
        args.socket_dir.as_deref(),
        Some(std::path::Path::new("/tmp/elsewhere"))
    );
    assert!(!terminal_args(&[]).standalone);
    assert_eq!(terminal_args(&[]).socket_dir, None);
}

#[test]
fn only_the_window_command_moves_the_socket_folder() {
    let window = try_parse(&["zellij", "terminal", "--socket-dir", "/tmp/elsewhere"]).unwrap();
    assert_eq!(
        crate::socket_dir_override(&window),
        Some(std::path::Path::new("/tmp/elsewhere"))
    );
    let plain = try_parse(&["zellij", "terminal"]).unwrap();
    assert_eq!(crate::socket_dir_override(&plain), None);
    let other = try_parse(&["zellij", "list-sessions"]).unwrap();
    assert_eq!(crate::socket_dir_override(&other), None);
}

#[cfg(unix)]
fn fake_app(
    reply: zellij_utils::app_control::AppReply,
) -> (
    std::path::PathBuf,
    std::sync::mpsc::Receiver<zellij_utils::app_control::AppRequest>,
) {
    use interprocess::local_socket::prelude::*;
    let dir = std::env::temp_dir().join(format!(
        "zellij-fake-app-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let socket = dir.join(zellij_utils::consts::APP_CONTROL_SOCKET_NAME);
    let _ = std::fs::remove_file(&socket);
    let listener = zellij_utils::consts::ipc_bind(&socket).unwrap();
    let (requests, received) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        if let Some(Ok(mut stream)) = listener.incoming().next() {
            if let Ok(request) = zellij_utils::app_control::read_request(&mut stream) {
                let _ = requests.send(request);
            }
            let _ = zellij_utils::app_control::write_reply(&mut stream, &reply);
        }
    });
    (socket, received)
}

#[cfg(unix)]
#[test]
fn the_cli_toggles_the_tray_icon_by_asking_the_desktop_app() {
    use zellij_utils::app_control::{AppReply, AppRequest};
    let (socket, received) = fake_app(AppReply::Done);
    assert_eq!(crate::commands::toggle_tray_icon_at(&socket), Ok(()));
    assert_eq!(
        received
            .recv_timeout(std::time::Duration::from_secs(2))
            .unwrap(),
        AppRequest::ToggleTrayIcon
    );
    let _ = std::fs::remove_file(&socket);
}

#[cfg(unix)]
#[test]
fn the_cli_reports_a_refusal_or_a_missing_desktop_app() {
    use zellij_utils::app_control::AppReply;
    let (socket, _received) = fake_app(AppReply::Refused("The Zellij app is closing.".to_owned()));
    assert_eq!(
        crate::commands::toggle_tray_icon_at(&socket),
        Err("The Zellij app is closing.".to_owned())
    );
    let _ = std::fs::remove_file(&socket);
    assert_eq!(
        crate::commands::toggle_tray_icon_at(&socket),
        Err("The Zellij desktop app is not running.".to_owned())
    );
}

fn parse_action(args: &[&str]) -> zellij_utils::cli::CliAction {
    let mut line = vec!["zellij", "action"];
    line.extend_from_slice(args);
    match try_parse(&line) {
        Ok(CliArgs {
            command: Some(Command::Action(action)),
            ..
        }) => *action,
        other => panic!("{:?} did not parse as an action: {:?}", args, other),
    }
}

#[test]
fn the_attention_actions_parse_from_the_command_line() {
    use zellij_utils::cli::CliAction;
    assert!(matches!(
        parse_action(&["mute-pane"]),
        CliAction::MutePane { pane_id: None }
    ));
    assert!(matches!(
        parse_action(&["mute-pane", "-p", "terminal_3"]),
        CliAction::MutePane { pane_id: Some(id) } if id == "terminal_3"
    ));
    assert!(matches!(
        parse_action(&["ignore-pane-output", "--pane-id", "4"]),
        CliAction::IgnorePaneOutput { pane_id: Some(id) } if id == "4"
    ));
    assert!(matches!(
        parse_action(&["watch-pane"]),
        CliAction::WatchPane { pane_id: None }
    ));
    assert!(matches!(
        parse_action(&["mute-tab", "-t", "2"]),
        CliAction::MuteTab { tab_id: Some(2) }
    ));
    assert!(matches!(
        parse_action(&["mute-session"]),
        CliAction::MuteSession
    ));
    assert!(matches!(
        parse_action(&["mark-pane-seen", "-p", "terminal_1"]),
        CliAction::MarkPaneSeen { pane_id: Some(id) } if id == "terminal_1"
    ));
    assert!(matches!(
        parse_action(&["mark-all-seen"]),
        CliAction::MarkAllSeen {
            this_session: false
        }
    ));
    assert!(matches!(
        parse_action(&["mark-all-seen", "--this-session"]),
        CliAction::MarkAllSeen { this_session: true }
    ));
}

#[test]
fn the_session_switching_actions_parse_from_the_command_line() {
    use zellij_utils::cli::CliAction;
    assert!(matches!(
        parse_action(&["toggle-session"]),
        CliAction::ToggleSession
    ));
    assert!(matches!(
        parse_action(&["go-to-previous-session"]),
        CliAction::GoToPreviousSession
    ));
    assert!(matches!(
        parse_action(&["go-to-next-session"]),
        CliAction::GoToNextSession
    ));
    assert!(matches!(
        parse_action(&["go-to-next-attention"]),
        CliAction::GoToNextAttention
    ));
    assert!(matches!(
        parse_action(&["toggle-tray-icon"]),
        CliAction::ToggleTrayIcon
    ));
}

#[test]
fn new_panes_and_runs_can_be_watched_with_notify() {
    use zellij_utils::cli::CliAction;
    assert!(matches!(
        parse_action(&["new-pane", "--notify", "--", "make"]),
        CliAction::NewPane { notify: true, .. }
    ));
    assert!(matches!(
        parse_action(&["new-pane", "--", "make"]),
        CliAction::NewPane { notify: false, .. }
    ));
    match try_parse(&["zellij", "run", "--notify", "--", "make"]) {
        Ok(CliArgs {
            command:
                Some(Command::Sessions(Sessions::Run {
                    notify, command, ..
                })),
            ..
        }) => {
            assert!(notify);
            assert_eq!(command, vec!["make".to_owned()]);
        },
        other => panic!("zellij run --notify did not parse: {:?}", other),
    }
}

fn setup_of(args: &[&str]) -> zellij_utils::setup::Setup {
    match try_parse(args).unwrap().command {
        Some(Command::Setup(setup)) => setup,
        other => panic!("expected the setup command, got {:?}", other),
    }
}

#[test]
fn every_setup_screen_action_is_a_setup_flag_and_they_combine() {
    let setup = setup_of(&[
        "zellij",
        "setup",
        "--copy-to-permanent-folder",
        "--replace-existing",
        "--install-desktop-entry",
        "--make-default-terminal",
        "--keybinding-preset",
        "unlock-first",
    ]);
    let actions = setup.desktop_actions();
    assert!(actions.copy_to_permanent_folder);
    assert!(actions.install_desktop_entry);
    assert!(actions.make_default_terminal);
    assert!(!actions.uninstall_desktop_entry);
    assert!(setup.replace_existing);
    assert_eq!(setup.keybinding_preset.as_deref(), Some("unlock-first"));
    assert!(setup.runs_setup_actions());
    assert!(setup_of(&["zellij", "setup", "--uninstall-desktop-entry"]).runs_setup_actions());
    assert!(!setup_of(&["zellij", "setup", "--check"]).runs_setup_actions());
}

#[test]
fn setup_actions_refuse_the_other_setup_flags_and_replace_needs_a_copy() {
    assert!(try_parse(&[
        "zellij",
        "setup",
        "--install-desktop-entry",
        "--dump-config"
    ])
    .is_err());
    assert!(try_parse(&["zellij", "setup", "--make-default-terminal", "--check"]).is_err());
    assert!(try_parse(&["zellij", "setup", "--replace-existing"]).is_err());
}
