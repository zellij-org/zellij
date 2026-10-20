use crate::{ipc_pipe_length_error, sock_dir_ipc_pipe_length_error};
use std::path::{Path, PathBuf};
use zellij_utils::consts::{SESSION_ID_LENGTH, ZELLIJ_SOCK_MAX_LENGTH};

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
fn the_startup_check_measures_the_socket_directory_joined_with_a_session_id() {
    let sock_dir = Path::new("/run/user/1000/zellij/contract_version_1");
    assert!(sock_dir_ipc_pipe_length_error(sock_dir).is_none());

    // The longest socket directory that still fits a session id, and one byte more.
    let fitting_len = ZELLIJ_SOCK_MAX_LENGTH - 1 - 1 - SESSION_ID_LENGTH;
    assert!(sock_dir_ipc_pipe_length_error(&path_of_length(fitting_len)).is_none());
    let too_long = path_of_length(fitting_len + 1);
    let message = sock_dir_ipc_pipe_length_error(&too_long)
        .expect("a socket directory too long to hold a session id must be rejected");
    assert!(message.contains(&too_long.display().to_string()));
}
