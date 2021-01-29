#[cfg(test)]
mod tests;

mod cli;
mod common;
// TODO mod server;
mod client;

use common::{pty_bus, os_input_output, screen, ipc, wasm_vm, command_is_executing, errors, utils, start, ApiCommand};
use client::{tab, layout, boundaries, panes};


use std::io::Write;
use std::os::unix::net::UnixStream;

use structopt::StructOpt;

use crate::cli::CliArgs;
use crate::command_is_executing::CommandIsExecuting;
use crate::os_input_output::{get_os_input};
use crate::pty_bus::{VteEvent};
use crate::utils::{
    consts::{MOSAIC_IPC_PIPE, MOSAIC_TMP_DIR, MOSAIC_TMP_LOG_DIR},
    logging::*,
};

pub fn main() {
    let opts = CliArgs::from_args();
    if let Some(split_dir) = opts.split {
        match split_dir {
            'h' => {
                let mut stream = UnixStream::connect(MOSAIC_IPC_PIPE).unwrap();
                let api_command = bincode::serialize(&ApiCommand::SplitHorizontally).unwrap();
                stream.write_all(&api_command).unwrap();
            }
            'v' => {
                let mut stream = UnixStream::connect(MOSAIC_IPC_PIPE).unwrap();
                let api_command = bincode::serialize(&ApiCommand::SplitVertically).unwrap();
                stream.write_all(&api_command).unwrap();
            }
            _ => {}
        };
    } else if opts.move_focus {
        let mut stream = UnixStream::connect(MOSAIC_IPC_PIPE).unwrap();
        let api_command = bincode::serialize(&ApiCommand::MoveFocus).unwrap();
        stream.write_all(&api_command).unwrap();
    } else if let Some(file_to_open) = opts.open_file {
        let mut stream = UnixStream::connect(MOSAIC_IPC_PIPE).unwrap();
        let api_command = bincode::serialize(&ApiCommand::OpenFile(file_to_open)).unwrap();
        stream.write_all(&api_command).unwrap();
    } else {
        let os_input = get_os_input();
        atomic_create_dir(MOSAIC_TMP_DIR).unwrap();
        atomic_create_dir(MOSAIC_TMP_LOG_DIR).unwrap();
        start(Box::new(os_input), opts);
    }
}