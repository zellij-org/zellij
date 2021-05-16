mod cli;
mod client;
mod common;
mod server;
#[cfg(test)]
mod tests;

use client::{boundaries, layout, panes, start_client, tab};
use common::{
    command_is_executing, errors, os_input_output, pty, screen, setup::Setup, utils, wasm_vm,
};
use server::start_server;
use structopt::StructOpt;

use crate::cli::CliArgs;
use crate::command_is_executing::CommandIsExecuting;
use crate::common::input::config::Config;
use crate::os_input_output::{get_client_os_input, get_server_os_input};
use crate::utils::{
    consts::{ZELLIJ_TMP_DIR, ZELLIJ_TMP_LOG_DIR},
    logging::*,
};
use std::convert::TryFrom;

pub fn main() {
    let opts = CliArgs::from_args();

    if let Some(crate::cli::ConfigCli::Setup(setup)) = opts.option.clone() {
        Setup::from_cli(&setup, opts).expect("Failed to print to stdout");
        std::process::exit(0);
    } else {
        let config = match Config::try_from(&opts) {
            Ok(config) => config,
            Err(e) => {
                eprintln!("There was an error in the config file:\n{}", e);
                std::process::exit(1);
            }
        };
        atomic_create_dir(&*ZELLIJ_TMP_DIR).unwrap();
        atomic_create_dir(&*ZELLIJ_TMP_LOG_DIR).unwrap();
        if let Some(path) = opts.server {
            let os_input = match get_server_os_input() {
                Ok(server_os_input) => server_os_input,
                Err(e) => {
                    eprintln!("failed to open terminal:\n{}", e);
                    std::process::exit(1);
                }
            };
            start_server(Box::new(os_input), path);
        } else {
            let os_input = match get_client_os_input() {
                Ok(os_input) => os_input,
                Err(e) => {
                    eprintln!("failed to open terminal:\n{}", e);
                    std::process::exit(1);
                }
            };
            start_client(Box::new(os_input), opts, config);
        }
    }
}
