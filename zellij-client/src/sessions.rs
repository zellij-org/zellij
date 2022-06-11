use std::process;

use zellij_utils::consts::ZELLIJ_SOCK_DIR;
use zellij_utils::interprocess::local_socket::LocalSocketStream;
use zellij_utils::ipc::{ClientToServerMsg, IpcSenderWithContext};

pub(crate) fn kill_session(name: &str) {
    let path = &*ZELLIJ_SOCK_DIR.join(name);
    match LocalSocketStream::connect(path) {
        Ok(stream) => {
            IpcSenderWithContext::new(stream).send(ClientToServerMsg::KillSession);
        },
        Err(e) => {
            eprintln!("Error occurred: {:?}", e);
            process::exit(1);
        },
    };
}
