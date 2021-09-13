mod install;
mod sessions;
#[cfg(test)]
mod tests;

use crate::install::populate_data_dir;
use sessions::{assert_session, assert_session_ne, get_active_session, list_sessions};
use std::process;
use zellij_client::{os_input_output::get_client_os_input, start_client, ClientInfo};
use zellij_server::{os_input_output::get_server_os_input, start_server};
use zellij_utils::{
    cli::{CliArgs, Command, SessionCommand, Sessions},
    consts::{ZELLIJ_TMP_DIR, ZELLIJ_TMP_LOG_DIR},
    logging::*,
    setup::{get_default_data_dir, Setup},
    structopt::StructOpt,
};

pub fn main() {
    configure_logger();
    let opts = CliArgs::from_args();

    if let Some(Command::Sessions(Sessions::ListSessions)) = opts.command {
        list_sessions();
    }

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
        let (config, layout, config_options) = match Setup::from_options(&opts) {
            Ok(results) => results,
            Err(e) => {
                eprintln!("{}", e);
                process::exit(1);
            }
        };

        let os_input = match get_client_os_input() {
            Ok(os_input) => os_input,
            Err(e) => {
                eprintln!("failed to open terminal:\n{}", e);
                process::exit(1);
            }
        };
        if let Some(Command::Sessions(Sessions::Attach {
            mut session_name,
            force,
            options,
        })) = opts.command.clone()
        {
            if let Some(session) = session_name.as_ref() {
                assert_session(session);
            } else {
                session_name = Some(get_active_session());
            }

            let config_options = match options {
                Some(SessionCommand::Options(o)) => config_options.merge(o),
                None => config_options,
            };

            start_client(
                Box::new(os_input),
                opts,
                config,
                config_options.clone(),
                ClientInfo::Attach(session_name.unwrap(), force, config_options),
                None,
            );
        } else {
            let session_name = opts
                .session
                .clone()
                .unwrap_or_else(|| names::Generator::default().next().unwrap());
            assert_session_ne(&session_name);

            // Determine and initialize the data directory
            let data_dir = opts.data_dir.clone().unwrap_or_else(get_default_data_dir);
            #[cfg(not(disable_automatic_asset_installation))]
            populate_data_dir(&data_dir);

            start_client(
                Box::new(os_input),
                opts,
                config,
                config_options,
                ClientInfo::New(session_name),
                layout,
            );
        }
    }
}
