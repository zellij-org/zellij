use crate::{ipc_pipe_length_error, session_ipc_pipe_length_error};
use std::path::{Path, PathBuf};
use zellij_utils::consts::ZELLIJ_SOCK_MAX_LENGTH;

fn path_of_length(length: usize) -> PathBuf {
    let mut path = String::from("/tmp/");
    path.push_str(&"a".repeat(length - path.len()));
    PathBuf::from(path)
}

#[test]
fn a_socket_path_below_the_limit_is_accepted() {
    assert!(ipc_pipe_length_error(&path_of_length(ZELLIJ_SOCK_MAX_LENGTH - 1)).is_none());
}

#[test]
fn a_socket_path_at_the_limit_is_rejected_with_its_length() {
    let message = ipc_pipe_length_error(&path_of_length(ZELLIJ_SOCK_MAX_LENGTH))
        .expect("a path at the limit must be rejected");
    assert!(
        message.contains(&format!("({} bytes", ZELLIJ_SOCK_MAX_LENGTH)),
        "the message must report the offending length: {}",
        message
    );
    assert!(message.contains("ZELLIJ_SOCKET_DIR="));
}

#[test]
fn the_startup_check_measures_the_socket_directory_joined_with_the_session_name() {
    let sock_dir = Path::new("/run/user/1000/zellij/contract_version_1");
    let short_name = "multiverse";
    let long_name = format!(
        "multiverse-{}",
        "a".repeat(ZELLIJ_SOCK_MAX_LENGTH - sock_dir.as_os_str().len())
    );
    assert!(session_ipc_pipe_length_error(sock_dir, short_name).is_none());
    let message = session_ipc_pipe_length_error(sock_dir, &long_name)
        .expect("a session name that pushes the socket path past the limit must be rejected");
    assert!(message.contains(&sock_dir.join(&long_name).display().to_string()));
}
