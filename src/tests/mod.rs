pub mod e2e;
pub mod fakes;
pub mod integration;
pub mod possible_tty_inputs;
pub mod tty_inputs;
pub mod utils;

use std::path::PathBuf;
use zellij_client::{os_input_output::ClientOsApi, start_client, ClientInfo};
use zellij_server::{os_input_output::ServerOsApi, start_server};
use zellij_utils::{cli::CliArgs, input::config::Config};

pub fn start(
    client_os_input: Box<dyn ClientOsApi>,
    opts: CliArgs,
    server_os_input: Box<dyn ServerOsApi>,
    config: Config,
) {
    let server_thread = std::thread::Builder::new()
        .name("server_thread".into())
        .spawn(move || {
            start_server(server_os_input, PathBuf::from(""));
        })
        .unwrap();
    start_client(client_os_input, opts, config, ClientInfo::New("".into()));
    let _ = server_thread.join();
}
