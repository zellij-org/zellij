mod install;
mod sessions;
#[cfg(test)]
mod tests;

use crate::install::populate_data_dir;
use sessions::{assert_session_ne, get_sessions, print_sessions, print_sessions_and_exit};
use std::convert::TryFrom;
use std::process;
use zellij_client::{os_input_output::get_client_os_input, start_client, ClientInfo};
use zellij_server::{os_input_output::get_server_os_input, start_server};
use zellij_utils::{
    cli::{CliArgs, Command, Sessions},
    consts::{ZELLIJ_TMP_DIR, ZELLIJ_TMP_LOG_DIR},
    input::{config::Config, layout::Layout, options::Options},
    logging::*,
    setup::{find_default_config_dir, get_default_data_dir, get_layout_dir, Setup},
    structopt::StructOpt,
};

fn start_new_session_name(opts: &CliArgs) -> String {
    let session_name = opts
        .session
        .clone()
        .unwrap_or_else(|| names::Generator::default().next().unwrap());
    assert_session_ne(&session_name);
    return session_name;
}

fn start_new_session_layout(opts: &CliArgs, config_options: &Options) -> Option<Layout> {
    // Determine and initialize the data directory
    let data_dir = opts.data_dir.clone().unwrap_or_else(get_default_data_dir);
    #[cfg(not(disable_automatic_asset_installation))]
    populate_data_dir(&data_dir);

    let layout_dir = config_options
        .layout_dir
        .clone()
        .or_else(|| get_layout_dir(opts.config_dir.clone().or_else(find_default_config_dir)));

    let layout =
        Layout::from_path_or_default(opts.layout.as_ref(), opts.layout_path.as_ref(), layout_dir);

    return layout;
}

pub fn main() {
    let mut opts = CliArgs::from_args();

    if let Some(Command::Sessions(Sessions::ListSessions)) = opts.command {
        print_sessions_and_exit();
    }

    let config = match Config::try_from(&opts) {
        Ok(config) => config,
        Err(e) => {
            eprintln!("There was an error in the config file:\n{}", e);
            process::exit(1);
        }
    };
    let config_options = Options::from_cli(&config.options, opts.command.clone());

    if let Some(Command::Setup(ref setup)) = opts.command {
        Setup::from_cli(setup, &opts, &config_options).expect("Failed to print to stdout");
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
        let os_input = match get_client_os_input() {
            Ok(os_input) => os_input,
            Err(e) => {
                eprintln!("failed to open terminal:\n{}", e);
                process::exit(1);
            }
        };
        if let Some(Command::Sessions(Sessions::Attach {
            mut session_name,
            create,
            force,
        })) = opts.command.clone()
        {
            let mut sessions = get_sessions();
            let mut start_new_session = false;

            if session_name.is_none() {
                if sessions.len() == 1 {
                    // If a session name was omitted but there is only one session, attach it
                    session_name = sessions.pop();
                    println!("Attaching session {:?}", session_name);
                } else if create {
                    // If a session name was omitted, but --create was used, attach to new session
                    // with random name
                    start_new_session = true;
                    println!("Creating a new session to attach");
                } else if sessions.is_empty() {
                    // If a session name was omitted and there are no sessions, exit with error
                    eprintln!("ERROR: No active Zellij sessions found");
                    process::exit(1);
                } else {
                    // If session name was omitted but there are some sessions, list them before
                    // exiting with error
                    eprintln!("ERROR: Please specify the session name to attach to. The following sessions are active:");
                    print_sessions(sessions);
                    process::exit(1);
                }
            } else if sessions
                    .iter()
                    .any(|s| s.to_string() == session_name.as_deref().unwrap())
            {
                // If a session name was given, and it exists, attach to it
            } else if create {
                // If a session name was given, but does not exist while --create was used,
                // attach a new session using that name
                start_new_session = true;
                println!(
                    "Creating new session {:?} to attach",
                    session_name.clone().unwrap()
                );
                opts.session = session_name.clone();
            } else {
                // If a session name was given, but does not exist and will not be created,
                // just list sessions and exit
                eprintln!("ERROR: No session named {:?} found", session_name.unwrap());
                process::exit(1);
            }

            if start_new_session {
                start_client(
                    Box::new(os_input),
                    opts.clone(),
                    config,
                    ClientInfo::New(start_new_session_name(&opts)),
                    start_new_session_layout(&opts, &config_options),
                );
            } else {
                start_client(
                    Box::new(os_input),
                    opts,
                    config,
                    ClientInfo::Attach(session_name.unwrap(), force, config_options),
                    None,
                );
            }
        } else {
            start_client(
                Box::new(os_input),
                opts.clone(),
                config,
                ClientInfo::New(start_new_session_name(&opts)),
                start_new_session_layout(&opts, &config_options),
            );
        }
    }
}
