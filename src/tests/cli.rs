use std::{
    net::{IpAddr, Ipv4Addr},
    path::PathBuf,
};

use clap::{CommandFactory, Parser};
use zellij_utils::cli::{CliArgs, Command};

const CLI_TEST_STACK_SIZE: usize = 16 * 1024 * 1024;

fn run_cli_test(test: impl FnOnce() + Send + 'static) {
    // clap 3 recursively walks this large command graph, and the default test thread stack is too
    // small once the web subcommand gains another option.
    let handle = std::thread::Builder::new()
        .stack_size(CLI_TEST_STACK_SIZE)
        .spawn(test)
        .expect("failed to spawn CLI test thread");
    if let Err(payload) = handle.join() {
        std::panic::resume_unwind(payload);
    }
}

#[test]
fn verify_cli() {
    run_cli_test(|| {
        CliArgs::command().debug_assert();
    });
}

#[test]
fn web_cli_status_alone_works() {
    run_cli_test(|| {
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
    });
}

#[test]
fn web_cli_status_with_timeout_works() {
    run_cli_test(|| {
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
    });
}

#[test]
fn web_cli_timeout_with_status_works() {
    run_cli_test(|| {
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
    });
}

#[test]
fn web_cli_timeout_without_status_fails() {
    run_cli_test(|| {
        let args = CliArgs::try_parse_from(["zellij", "web", "--timeout", "5"]);
        assert!(args.is_err());
    });
}

#[test]
fn web_cli_status_with_start_fails() {
    run_cli_test(|| {
        let args = CliArgs::try_parse_from(["zellij", "web", "--status", "--start"]);
        assert!(args.is_err());
    });
}

#[test]
fn web_cli_status_with_stop_fails() {
    run_cli_test(|| {
        let args = CliArgs::try_parse_from(["zellij", "web", "--status", "--stop"]);
        assert!(args.is_err());
    });
}

#[test]
fn web_cli_status_with_ip_works() {
    run_cli_test(|| {
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
    });
}

#[test]
fn web_cli_status_with_port_works() {
    run_cli_test(|| {
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
    });
}

#[test]
fn web_cli_status_with_ip_and_port_works() {
    run_cli_test(|| {
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
    });
}

#[test]
fn web_cli_status_with_socket_works() {
    run_cli_test(|| {
        let args = CliArgs::try_parse_from([
            "zellij",
            "web",
            "--status",
            "--socket",
            "/tmp/zellij-web.sock",
        ]);
        assert!(args.is_ok());
        if let Ok(CliArgs {
            command: Some(Command::Web(web)),
            ..
        }) = args
        {
            assert!(web.status);
            assert_eq!(web.web_socket, Some(PathBuf::from("/tmp/zellij-web.sock")));
            assert!(web.validate_socket_args().is_ok());
        } else {
            panic!("Expected Web command");
        }
    });
}

#[test]
fn web_cli_socket_with_ip_fails_validation() {
    run_cli_test(|| {
        let args = CliArgs::try_parse_from([
            "zellij",
            "web",
            "--socket",
            "/tmp/zellij-web.sock",
            "--ip",
            "127.0.0.1",
        ]);
        assert!(args.is_ok());
        if let Ok(CliArgs {
            command: Some(Command::Web(web)),
            ..
        }) = args
        {
            assert!(web.validate_socket_args().is_err());
        } else {
            panic!("Expected Web command");
        }
    });
}

#[test]
fn web_cli_socket_with_cert_fails_validation() {
    run_cli_test(|| {
        let args = CliArgs::try_parse_from([
            "zellij",
            "web",
            "--socket",
            "/tmp/zellij-web.sock",
            "--cert",
            "/tmp/cert.pem",
        ]);
        assert!(args.is_ok());
        if let Ok(CliArgs {
            command: Some(Command::Web(web)),
            ..
        }) = args
        {
            assert!(web.validate_socket_args().is_err());
        } else {
            panic!("Expected Web command");
        }
    });
}
