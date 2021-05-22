mod list_sessions;
#[cfg(test)]
mod tests;

use list_sessions::{assert_session, assert_session_ne, list_sessions};
use std::convert::TryFrom;
use std::process;
use zellij_client::{os_input_output::get_client_os_input, start_client, ClientInfo};
use zellij_server::{os_input_output::get_server_os_input, start_server};
use zellij_utils::{
    cli::{CliArgs, Command, Sessions},
    consts::{ZELLIJ_TMP_DIR, ZELLIJ_TMP_LOG_DIR},
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
        if let Some(Command::Sessions(Sessions::Attach {
            session_name,
            force,
        })) = opts.command.clone()
        {
            assert_session(&session_name);
            start_client(
                Box::new(os_input),
                opts,
                config,
                ClientInfo::Attach(session_name, force),
            );
        } else {
            let session_name = opts
                .session
                .clone()
                .unwrap_or_else(|| names::Generator::default().next().unwrap());
            assert_session_ne(&session_name);
            start_client(
                Box::new(os_input),
                opts,
                config,
                ClientInfo::New(session_name),
            );
        }
    }
}
