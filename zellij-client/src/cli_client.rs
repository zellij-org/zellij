//! The `[cli_client]` is used to attach to a running server session
//! and dispatch actions, that are specified through the command line.
use std::{fs, path::PathBuf};
use zellij_utils::envs;

use crate::{
    os_input_output::ClientOsApi,
};
use zellij_utils::{
    data::ClientId,
    input::actions::Action,
    ipc::ClientToServerMsg,
};

pub fn start_cli_client(
    os_input: Box<dyn ClientOsApi>,
    session_name: &str,
    actions: Vec<Action>,
) {
    let client_id: Option<ClientId> = envs::get_client_id().ok().map(|s| s.encode_utf16().next().unwrap()); // TODO: better? does this even work?
    let zellij_ipc_pipe: PathBuf = {
        let mut sock_dir = zellij_utils::consts::ZELLIJ_SOCK_DIR.clone();
        fs::create_dir_all(&sock_dir).unwrap();
        zellij_utils::shared::set_permissions(&sock_dir, 0o700).unwrap();
        sock_dir.push(session_name);
        sock_dir
    };
    os_input.connect_to_server(&*zellij_ipc_pipe);
    for action in actions {
        let msg = ClientToServerMsg::Action(action, client_id);
        os_input.send_to_server(msg);
    }
}
