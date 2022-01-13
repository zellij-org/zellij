use crate::install::populate_data_dir;
use crate::sessions::kill_session as kill_session_impl;
use crate::sessions::{
    assert_session, assert_session_ne, get_active_session, get_sessions,
    get_sessions_sorted_by_creation_date, print_sessions, print_sessions_with_index,
    session_exists, ActiveSession, rename_session
};
use dialoguer::Confirm;
use std::path::PathBuf;
use std::process;
use zellij_client::start_client as start_client_impl;
use zellij_client::{os_input_output::get_client_os_input, ClientInfo};
use zellij_server::os_input_output::get_server_os_input;
use zellij_server::start_server as start_server_impl;
use zellij_utils::input::options::Options;
use zellij_utils::nix;
use zellij_utils::{
    cli::{CliArgs, Command, SessionCommand, Sessions},
    envs,
    setup::{get_default_data_dir, Setup},
};

pub(crate) use crate::sessions::list_sessions;

pub(crate) fn kill_all_sessions(yes: bool) {
    match get_sessions() {
        Ok(sessions) if sessions.is_empty() => {
            println!("No active zellij sessions found.");
            process::exit(1);
        }
        Ok(sessions) => {
            if !yes {
                println!("WARNING: this action will kill all sessions.");
                if !Confirm::new()
                    .with_prompt("Do you want to continue?")
                    .interact()
                    .unwrap()
                {
                    println!("Abort.");
                    process::exit(1);
                }
            }
            for session in &sessions {
                kill_session_impl(session);
            }
            process::exit(0);
        }
        Err(e) => {
            eprintln!("Error occurred: {:?}", e);
            process::exit(1);
        }
    }
}

pub(crate) fn kill_session(target_session: &Option<String>) {
    match target_session {
        Some(target_session) => {
            assert_session(target_session);
            kill_session_impl(target_session);
            process::exit(0);
        }
        None => {
            println!("Please specify the session name to kill.");
            process::exit(1);
        }
    }
}

pub(crate) fn rename_current_or_target_session(target_session: Option<String>, new_session_name: String) {
    match target_session {
        Some(target_session) => {
            assert_session(&target_session);
            rename_session(target_session, new_session_name);
            process::exit(0);
        }
        None => {
            if let Ok(curr_session) = envs::get_session_name() {
                rename_session(curr_session, new_session_name);
            } else {
                eprintln!("there is no attached session, you need to specify the target session.");
                process::exit(1);
            }
        }
    }
}

fn get_os_input<OsInputOutput>(
    fn_get_os_input: fn() -> Result<OsInputOutput, nix::Error>,
) -> OsInputOutput {
    match fn_get_os_input() {
        Ok(os_input) => os_input,
        Err(e) => {
            eprintln!("failed to open terminal:\n{}", e);
            process::exit(1);
        }
    }
}

pub(crate) fn start_server(path: PathBuf) {
    let os_input = get_os_input(get_server_os_input);
    start_server_impl(Box::new(os_input), path);
}

fn create_new_client() -> ClientInfo {
    ClientInfo::New(names::Generator::default().next().unwrap())
}

fn install_default_assets(opts: &CliArgs) {
    let data_dir = opts.data_dir.clone().unwrap_or_else(get_default_data_dir);
    #[cfg(not(disable_automatic_asset_installation))]
    populate_data_dir(&data_dir);
}

fn find_indexed_session(
    sessions: Vec<String>,
    config_options: Options,
    index: usize,
    create: bool,
) -> ClientInfo {
    match sessions.get(index) {
        Some(session) => ClientInfo::Attach(session.clone(), config_options),
        None if create => create_new_client(),
        None => {
            println!(
                "No session indexed by {} found. The following sessions are active:",
                index
            );
            print_sessions_with_index(sessions);
            process::exit(1);
        }
    }
}

fn attach_with_session_index(config_options: Options, index: usize, create: bool) -> ClientInfo {
    // Ignore the session_name when `--index` is provided
    match get_sessions_sorted_by_creation_date() {
        Ok(sessions) if sessions.is_empty() => {
            if create {
                create_new_client()
            } else {
                println!("No active zellij sessions found.");
                process::exit(1);
            }
        }
        Ok(sessions) => find_indexed_session(sessions, config_options, index, create),
        Err(e) => {
            eprintln!("Error occurred: {:?}", e);
            process::exit(1);
        }
    }
}

fn attach_with_session_name(
    session_name: Option<String>,
    config_options: Options,
    create: bool,
) -> ClientInfo {
    match &session_name {
        Some(session) if create => {
            if !session_exists(session).unwrap() {
                ClientInfo::New(session_name.unwrap())
            } else {
                ClientInfo::Attach(session_name.unwrap(), config_options)
            }
        }
        Some(session) => {
            assert_session(session);
            ClientInfo::Attach(session_name.unwrap(), config_options)
        }
        None => match get_active_session() {
            ActiveSession::None if create => create_new_client(),
            ActiveSession::None => {
                println!("No active zellij sessions found.");
                process::exit(1);
            }
            ActiveSession::One(session_name) => ClientInfo::Attach(session_name, config_options),
            ActiveSession::Many => {
                println!("Please specify the session name to attach to. The following sessions are active:");
                print_sessions(get_sessions().unwrap());
                process::exit(1);
            }
        },
    }
}

pub(crate) fn start_client(opts: CliArgs) {
    let (config, layout, config_options) = match Setup::from_options(&opts) {
        Ok(results) => results,
        Err(e) => {
            eprintln!("{}", e);
            process::exit(1);
        }
    };
    let os_input = get_os_input(get_client_os_input);

    if let Some(Command::Sessions(Sessions::Attach {
        session_name,
        create,
        index,
        options,
    })) = opts.command.clone()
    {
        let config_options = match options {
            Some(SessionCommand::Options(o)) => config_options.merge_from_cli(o.into()),
            None => config_options,
        };

        let client = if let Some(idx) = index {
            attach_with_session_index(config_options.clone(), idx, create)
        } else {
            attach_with_session_name(session_name, config_options.clone(), create)
        };

        if let Ok(val) = std::env::var(envs::SESSION_NAME_ENV_KEY) {
            if val == *client.get_session_name() {
                eprintln!("You are trying to attach to the current session(\"{}\"). Zellij does not support nesting a session in itself", val);
                process::exit(1);
            }
        }

        let attach_layout = match client {
            ClientInfo::Attach(_, _) => None,
            ClientInfo::New(_) => layout,
        };

        if create {
            install_default_assets(&opts);
        }

        start_client_impl(
            Box::new(os_input),
            opts,
            config,
            config_options,
            client,
            attach_layout,
        );
    } else {
        let start_client_plan = |session_name: std::string::String| {
            assert_session_ne(&session_name);
            install_default_assets(&opts);
        };

        if let Some(session_name) = opts.session.clone() {
            start_client_plan(session_name.clone());
            start_client_impl(
                Box::new(os_input),
                opts,
                config,
                config_options,
                ClientInfo::New(session_name),
                layout,
            );
        } else {
            if let Some(layout_some) = layout.clone() {
                if let Some(session_name) = layout_some.session.name {
                    if layout_some.session.attach.unwrap() {
                        let client = attach_with_session_name(
                            Some(session_name),
                            config_options.clone(),
                            true,
                        );

                        let attach_layout = match client {
                            ClientInfo::Attach(_, _) => None,
                            ClientInfo::New(_) => layout,
                        };

                        start_client_impl(
                            Box::new(os_input),
                            opts,
                            config,
                            config_options,
                            client,
                            attach_layout,
                        );
                    } else {
                        start_client_plan(session_name.clone());
                        start_client_impl(
                            Box::new(os_input),
                            opts,
                            config,
                            config_options,
                            ClientInfo::New(session_name),
                            layout,
                        );
                    }

                    process::exit(0);
                }
            }

            let session_name = names::Generator::default().next().unwrap();
            start_client_plan(session_name.clone());
            start_client_impl(
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
