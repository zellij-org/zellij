#![cfg(unix)]

use std::time::{Duration, Instant};

use zellij_integration_tests::{fake_server_os_api::FakeServerOsApi, test_env};

/// `zellij list-sessions` connects to every session socket to check whether the
/// session is alive, then closes the connection again. When that happens before
/// the first real client has initialized the session, the server used to panic
/// on the `None` session data in the `RemoveClient` handler and the brand new
/// session died (#5632).
#[test]
fn a_probe_connection_that_closes_before_the_first_client_does_not_kill_the_session() {
    test_env::init();
    let session_name = test_env::unique_session_name();
    let socket_path = zellij_utils::consts::ZELLIJ_SOCK_DIR.join(&session_name);
    std::fs::create_dir_all(&socket_path.parent().unwrap()).unwrap();

    let fake_server_os_api = FakeServerOsApi::default();
    let server_thread = std::thread::Builder::new()
        .name("zellij_server_under_test".to_string())
        .spawn(move || {
            let install_panic_hook = false;
            zellij_server::start_server_impl(
                Box::new(fake_server_os_api),
                socket_path,
                install_panic_hook,
            );
        })
        .unwrap();

    connect_and_close_a_probe_socket(&session_name);

    // Give the server the same moment a real one gets to act on the removal
    // before deciding whether it survived.
    let deadline = Instant::now() + Duration::from_secs(1);
    while Instant::now() < deadline {
        if server_thread.is_finished() {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }

    assert!(
        !server_thread.is_finished(),
        "the server died when a probe connection closed before the first client \
         initialized the session"
    );
}

fn connect_and_close_a_probe_socket(session_name: &str) {
    let socket_path = zellij_utils::consts::ZELLIJ_SOCK_DIR.join(session_name);
    let deadline = Instant::now() + zellij_integration_tests::default_timeout();
    loop {
        match zellij_utils::consts::ipc_connect(&socket_path) {
            Ok(socket) => {
                drop(socket);
                return;
            },
            Err(e) if Instant::now() >= deadline => {
                panic!("could not connect to the session socket: {e}")
            },
            Err(_) => std::thread::sleep(Duration::from_millis(10)),
        }
    }
}
