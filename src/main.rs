#[cfg(test)]
mod tests;

use std::convert::TryFrom;
use std::os::unix::fs::FileTypeExt;
use std::{fs, io, process};
use zellij_client::{os_input_output::get_client_os_input, start_client};
use zellij_server::{os_input_output::get_server_os_input, start_server};
use zellij_utils::{
    cli::{CliArgs, Command, Sessions},
    consts::{ZELLIJ_SOCK_DIR, ZELLIJ_TMP_DIR, ZELLIJ_TMP_LOG_DIR},
    input::config::Config,
    logging::*,
    setup::Setup,
    structopt::StructOpt,
};

pub fn main() {
    let opts = CliArgs::from_args();

    if let Some(Command::Sessions(Sessions::ListSessions)) = opts.command {
        list_sessions();
    } else if let Some(Command::Setup(ref setup)) = opts.command {
        Setup::from_cli(setup, &opts).expect("Failed to print to stdout");
    }

    let config = match Config::try_from(&opts) {
        Ok(config) => config,
        Err(e) => {
            eprintln!("There was an error in the config file:\n{}", e);
            process::exit(1);
        }
    };
    atomic_create_dir(&*ZELLIJ_TMP_DIR).unwrap();
    atomic_create_dir(&*ZELLIJ_TMP_LOG_DIR).unwrap();
    if let Some(path) = opts.server {
        let os_input = match get_server_os_input() {
            Ok(server_os_input) => server_os_input,
            Err(e) => {
                eprintln!("failed to open terminal:\n{}", e);
                process::exit(1);
            }
        };
        start_server(Box::new(os_input), path);
    } else {
        let os_input = match get_client_os_input() {
            Ok(os_input) => os_input,
            Err(e) => {
                eprintln!("failed to open terminal:\n{}", e);
                process::exit(1);
            }
        };
        start_client(Box::new(os_input), opts, config);
    }
}

fn list_sessions() {
    match fs::read_dir(&*ZELLIJ_SOCK_DIR) {
        Ok(files) => {
            let mut is_empty = true;
            let session_name = std::env::var("ZELLIJ_SESSION_NAME").unwrap_or("".into());
            files.for_each(|file| {
                let file = file.unwrap();
                if file.file_type().unwrap().is_socket() {
                    let fname = file.file_name().into_string().unwrap();
                    let suffix = if session_name == fname {
                        " (current)"
                    } else {
                        ""
                    };
                    println!("{}{}", fname, suffix);
                    is_empty = false;
                }
            });
            if is_empty {
                println!("No active zellij sessions found.");
            }
        }
        Err(err) => {
            if let io::ErrorKind::NotFound = err.kind() {
                println!("No active zellij sessions found.");
            } else {
                eprintln!("Error occured: {}", err);
                process::exit(1);
            }
        }
    }
    process::exit(0);
}
