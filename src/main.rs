#[cfg(test)]
mod tests;

use std::convert::TryFrom;
use zellij_client::{os_input_output::get_client_os_input, start_client};
use zellij_server::{os_input_output::get_server_os_input, start_server};
use zellij_utils::{
    cli::{CliArgs, ConfigCli},
    consts::{ZELLIJ_TMP_DIR, ZELLIJ_TMP_LOG_DIR},
    input::config::Config,
    logging::*,
    setup::Setup,
    structopt::StructOpt,
};

pub fn main() {
    let opts = CliArgs::from_args();

    if let Some(ConfigCli::Setup(setup)) = opts.option.clone() {
        Setup::from_cli(&setup, &opts).expect("Failed to print to stdout");
    }

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
