use std::net::{IpAddr, Ipv4Addr};

use clap::{CommandFactory, Parser, Subcommand};
use zellij_utils::cli::{AttachArgs, CliArgs, Command, Sessions, WindowArgs};

#[test]
fn verify_cli() {
    CliArgs::command().debug_assert();
}

#[test]
fn web_cli_status_alone_works() {
    let args = CliArgs::try_parse_from(["zellij", "web", "--status"]);
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
    let args = CliArgs::try_parse_from(["zellij", "web", "--status", "--timeout", "5"]);
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
    let args = CliArgs::try_parse_from(["zellij", "web", "--timeout", "10", "--status"]);
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
    let args = CliArgs::try_parse_from(["zellij", "web", "--timeout", "5"]);
    assert!(args.is_err());
}

#[test]
fn web_cli_status_with_start_fails() {
    let args = CliArgs::try_parse_from(["zellij", "web", "--status", "--start"]);
    assert!(args.is_err());
}

#[test]
fn web_cli_status_with_stop_fails() {
    let args = CliArgs::try_parse_from(["zellij", "web", "--status", "--stop"]);
    assert!(args.is_err());
}

#[test]
fn web_cli_status_with_ip_works() {
    let args = CliArgs::try_parse_from(["zellij", "web", "--status", "--ip", "127.0.0.1"]);
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
    let args = CliArgs::try_parse_from(["zellij", "web", "--status", "--port", "9000"]);
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
    let args = CliArgs::try_parse_from([
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
    match CliArgs::try_parse_from(args).map(|parsed| parsed.command) {
        Ok(Some(Command::Sessions(Sessions::Attach { args, .. }))) => args,
        other => panic!("expected an attach command, got {:?}", other),
    }
}

fn window_args(argv: &[&str]) -> WindowArgs {
    let mut args = vec!["zellij", "window"];
    args.extend_from_slice(argv);
    match CliArgs::try_parse_from(args).map(|parsed| parsed.command) {
        Ok(Some(Command::Window(args))) => args,
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
        let window = window_args(&argv).attach;
        assert_eq!(
            format!("{:?}", attach),
            format!("{:?}", window),
            "`zellij attach {}` and `zellij window {}` disagree",
            argv.join(" "),
            argv.join(" ")
        );
    }
}

#[test]
fn a_backgrounded_session_is_a_usage_error_on_the_window() {
    assert!(CliArgs::try_parse_from(["zellij", "attach", "-b", "my-session"]).is_ok());
    assert!(CliArgs::try_parse_from(["zellij", "window", "-b", "my-session"]).is_err());
    assert!(
        CliArgs::try_parse_from(["zellij", "window", "--create-background", "my-session"]).is_err()
    );
}

#[test]
fn the_window_watches_where_attach_cannot() {
    assert!(window_args(&["my-session", "--watch"]).watch);
    assert!(window_args(&["--watch"]).watch);
    assert!(CliArgs::try_parse_from(["zellij", "attach", "--watch", "my-session"]).is_err());
}

#[test]
fn watching_refuses_the_flags_that_would_change_the_session_it_follows() {
    for argv in [
        vec!["--watch", "--create", "my-session"],
        vec!["--watch", "--force-run-commands", "my-session"],
        vec!["--watch", "--index", "1"],
        vec!["--watch", "my-session", "--", "htop"],
    ] {
        let mut args = vec!["zellij", "window"];
        args.extend_from_slice(&argv);
        assert!(
            CliArgs::try_parse_from(args).is_err(),
            "`zellij window {}` was accepted",
            argv.join(" ")
        );
    }
}

#[test]
fn the_fixture_flags_are_on_the_verb_and_hidden_from_it() {
    let args = window_args(&[
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
    assert_eq!(args.rows, 24);
    assert_eq!(args.cols, 80);
    assert_eq!(args.cell_width, 8);
    assert_eq!(args.cell_height, 16);
    assert_eq!(args.duration_secs, Some(2));

    let help = Command::augment_subcommands(clap::Command::new("zellij"))
        .find_subcommand_mut("window")
        .expect("the window verb is missing")
        .render_long_help()
        .to_string();
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
    let help = Command::augment_subcommands(clap::Command::new("zellij"))
        .render_long_help()
        .to_string();
    assert!(
        help.contains("Open a session in zellij's own window, without a terminal emulator"),
        "{}",
        help
    );
}
