mod install;
mod sessions;
#[cfg(test)]
mod tests;

use crate::install::populate_data_dir;
use sessions::{assert_session, assert_session_ne, get_active_session, list_sessions};
use std::convert::TryFrom;
use std::process;
use zellij_client::{os_input_output::get_client_os_input, start_client, ClientInfo};
use zellij_server::{os_input_output::get_server_os_input, start_server};
use zellij_utils::{
    cli::{CliArgs, Command, Sessions},
    consts::{ZELLIJ_TMP_DIR, ZELLIJ_TMP_LOG_DIR},
    input::config::Config,
    input::layout::Layout,
    input::options::Options,
    logging::*,
    setup::{find_default_config_dir, get_default_data_dir, get_layout_dir, Setup},
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
    let config_options = Options::from_cli(&config.options, opts.command.clone());

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
        
        let client_info = match opts.command.clone() {
            Some(Command::Sessions(Sessions::Attach {mut session_name, force, create})) => {
                let is_new_session = match (session_name.as_ref(), create) {
                    (Some(_), true) => true,
                    (Some(session), false) => {
                        assert_session(session);
                        false
                    },
                    (None, _) => {
                        session_name = Some(get_active_session());
                        false 
                    }
                };

                if is_new_session {
                    ClientInfo::New(session_name.unwrap())
                } else {
                    ClientInfo::Attach(session_name.unwrap(), force, config_options.clone())
                }
            }
            _ => {
                let session_name = opts
                    .session
                    .clone()
                    .unwrap_or_else(|| names::Generator::default().next().unwrap());
                assert_session_ne(&session_name);
                
                ClientInfo::New(session_name)
            }
        };
        
        let layout = match client_info {
            ClientInfo::New(_) => {
                // Determine and initialize the data directory
                let data_dir = opts.data_dir.clone().unwrap_or_else(get_default_data_dir);
                #[cfg(not(disable_automatic_asset_installation))]
                populate_data_dir(&data_dir);

                let layout_dir = config_options.layout_dir.or_else(|| {
                    get_layout_dir(opts.config_dir.clone().or_else(find_default_config_dir))
                });
                Layout::from_path_or_default(
                    opts.layout.as_ref(),
                    opts.layout_path.as_ref(),
                    layout_dir,
                )
            }
            ClientInfo::Attach(..) => None
        };

        start_client(
            Box::new(os_input),
            opts,
            config,
            client_info,
            layout,
        );
    }
}
