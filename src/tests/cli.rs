use clap::Parser;
use zellij_utils::cli::{CliArgs, Command};

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
