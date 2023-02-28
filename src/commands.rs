use dialoguer::Confirm;
use miette::{Report, Result};
use std::{fs::File, io::prelude::*, path::PathBuf, process};

use crate::sessions::{
    assert_session, assert_session_ne, get_active_session, get_sessions,
    get_sessions_sorted_by_mtime, kill_session as kill_session_impl, match_session_name,
    print_sessions, print_sessions_with_index, session_exists, ActiveSession, SessionNameMatch,
};
use zellij_client::{
    old_config_converter::{
        config_yaml_to_config_kdl, convert_old_yaml_files, layout_yaml_to_layout_kdl,
    },
    os_input_output::get_client_os_input,
    start_client as start_client_impl, ClientInfo,
};
use zellij_server::{os_input_output::get_server_os_input, start_server as start_server_impl};
use zellij_utils::{
    cli::{CliArgs, Command, SessionCommand, Sessions},
    envs,
    input::{
        actions::Action,
        config::{Config, ConfigError},
        options::Options,
    },
    nix,
    setup::Setup,
};

pub(crate) use crate::sessions::list_sessions;

pub(crate) fn kill_all_sessions(yes: bool) {
    match get_sessions() {
        Ok(sessions) if sessions.is_empty() => {
            eprintln!("No active zellij sessions found.");
            process::exit(1);
        },
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
        },
        Err(e) => {
            eprintln!("Error occurred: {:?}", e);
            process::exit(1);
        },
    }
}

pub(crate) fn kill_session(target_session: &Option<String>) {
    match target_session {
        Some(target_session) => {
            assert_session(target_session);
            kill_session_impl(target_session);
            process::exit(0);
        },
        None => {
            println!("Please specify the session name to kill.");
            process::exit(1);
        },
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
        },
    }
}

pub(crate) fn start_server(path: PathBuf, debug: bool) {
    // Set instance-wide debug mode
    zellij_utils::consts::DEBUG_MODE.set(debug).unwrap();
    let os_input = get_os_input(get_server_os_input);
    start_server_impl(Box::new(os_input), path);
}

fn create_new_client() -> ClientInfo {
    ClientInfo::New(names::Generator::default().next().unwrap())
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
        },
    }
}

pub(crate) fn send_action_to_session(
    cli_action: zellij_utils::cli::CliAction,
    requested_session_name: Option<String>,
    config: Option<Config>,
) {
    match get_active_session() {
        ActiveSession::None => {
            eprintln!("There is no active session!");
            std::process::exit(1);
        },
        ActiveSession::One(session_name) => {
            if let Some(requested_session_name) = requested_session_name {
                if requested_session_name != session_name {
                    eprintln!(
                        "Session '{}' not found. The following sessions are active:",
                        requested_session_name
                    );
                    eprintln!("{}", session_name);
                    std::process::exit(1);
                }
            }
            attach_with_cli_client(cli_action, &session_name, config);
        },
        ActiveSession::Many => {
            let existing_sessions = get_sessions().unwrap();
            if let Some(session_name) = requested_session_name {
                if existing_sessions.contains(&session_name) {
                    attach_with_cli_client(cli_action, &session_name, config);
                } else {
                    eprintln!(
                        "Session '{}' not found. The following sessions are active:",
                        session_name
                    );
                    print_sessions(existing_sessions);
                    std::process::exit(1);
                }
            } else if let Ok(session_name) = envs::get_session_name() {
                attach_with_cli_client(cli_action, &session_name, config);
            } else {
                eprintln!("Please specify the session name to send actions to. The following sessions are active:");
                print_sessions(existing_sessions);
                std::process::exit(1);
            }
        },
    };
}
pub(crate) fn convert_old_config_file(old_config_file: PathBuf) {
    match File::open(&old_config_file) {
        Ok(mut handle) => {
            let mut raw_config_file = String::new();
            let _ = handle.read_to_string(&mut raw_config_file);
            match config_yaml_to_config_kdl(&raw_config_file, false) {
                Ok(kdl_config) => {
                    println!("{}", kdl_config);
                    process::exit(0);
                },
                Err(e) => {
                    eprintln!("Failed to convert config: {}", e);
                    process::exit(1);
                },
            }
        },
        Err(e) => {
            eprintln!("Failed to open file: {}", e);
            process::exit(1);
        },
    }
}

pub(crate) fn convert_old_layout_file(old_layout_file: PathBuf) {
    match File::open(&old_layout_file) {
        Ok(mut handle) => {
            let mut raw_layout_file = String::new();
            let _ = handle.read_to_string(&mut raw_layout_file);
            match layout_yaml_to_layout_kdl(&raw_layout_file) {
                Ok(kdl_layout) => {
                    println!("{}", kdl_layout);
                    process::exit(0);
                },
                Err(e) => {
                    eprintln!("Failed to convert layout: {}", e);
                    process::exit(1);
                },
            }
        },
        Err(e) => {
            eprintln!("Failed to open file: {}", e);
            process::exit(1);
        },
    }
}

pub(crate) fn convert_old_theme_file(old_theme_file: PathBuf) {
    match File::open(&old_theme_file) {
        Ok(mut handle) => {
            let mut raw_config_file = String::new();
            let _ = handle.read_to_string(&mut raw_config_file);
            match config_yaml_to_config_kdl(&raw_config_file, true) {
                Ok(kdl_config) => {
                    println!("{}", kdl_config);
                    process::exit(0);
                },
                Err(e) => {
                    eprintln!("Failed to convert config: {}", e);
                    process::exit(1);
                },
            }
        },
        Err(e) => {
            eprintln!("Failed to open file: {}", e);
            process::exit(1);
        },
    }
}

fn attach_with_cli_client(
    cli_action: zellij_utils::cli::CliAction,
    session_name: &str,
    config: Option<Config>,
) {
    let os_input = get_os_input(zellij_client::os_input_output::get_cli_client_os_input);
    let get_current_dir = || std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    match Action::actions_from_cli(cli_action, Box::new(get_current_dir), config) {
        Ok(actions) => {
            zellij_client::cli_client::start_cli_client(Box::new(os_input), session_name, actions);
            std::process::exit(0);
        },
        Err(e) => {
            eprintln!("{}", e);
            log::error!("Error sending action: {}", e);
            std::process::exit(2);
        },
    }
}

fn attach_with_session_index(config_options: Options, index: usize, create: bool) -> ClientInfo {
    // Ignore the session_name when `--index` is provided
    match get_sessions_sorted_by_mtime() {
        Ok(sessions) if sessions.is_empty() => {
            if create {
                create_new_client()
            } else {
                eprintln!("No active zellij sessions found.");
                process::exit(1);
            }
        },
        Ok(sessions) => find_indexed_session(sessions, config_options, index, create),
        Err(e) => {
            eprintln!("Error occurred: {:?}", e);
            process::exit(1);
        },
    }
}

fn attach_with_session_name(
    session_name: Option<String>,
    config_options: Options,
    create: bool,
) -> ClientInfo {
    match &session_name {
        Some(session) if create => {
            if session_exists(session).unwrap() {
                ClientInfo::Attach(session_name.unwrap(), config_options)
            } else {
                ClientInfo::New(session_name.unwrap())
            }
        },
        Some(prefix) => match match_session_name(prefix).unwrap() {
            SessionNameMatch::UniquePrefix(s) | SessionNameMatch::Exact(s) => {
                ClientInfo::Attach(s, config_options)
            },
            SessionNameMatch::AmbiguousPrefix(sessions) => {
                println!(
                    "Ambiguous selection: multiple sessions names start with '{}':",
                    prefix
                );
                print_sessions(sessions);
                process::exit(1);
            },
            SessionNameMatch::None => {
                eprintln!("No session with the name '{}' found!", prefix);
                process::exit(1);
            },
        },
        None => match get_active_session() {
            ActiveSession::None if create => create_new_client(),
            ActiveSession::None => {
                eprintln!("No active zellij sessions found.");
                process::exit(1);
            },
            ActiveSession::One(session_name) => ClientInfo::Attach(session_name, config_options),
            ActiveSession::Many => {
                println!("Please specify the session to attach to, either by using the full name or a unique prefix.\nThe following sessions are active:");
                print_sessions(get_sessions().unwrap());
                process::exit(1);
            },
        },
    }
}

pub(crate) fn start_client(opts: CliArgs) {
    // look for old YAML config/layout/theme files and convert them to KDL
    convert_old_yaml_files(&opts);
    let (config, layout, config_options) = match Setup::from_cli_args(&opts) {
        Ok(results) => results,
        Err(e) => {
            if let ConfigError::KdlError(error) = e {
                let report: Report = error.into();
                eprintln!("{:?}", report);
            } else {
                eprintln!("{}", e);
            }
            process::exit(1);
        },
    };
    let os_input = get_os_input(get_client_os_input);

    let start_client_plan = |session_name: std::string::String| {
        assert_session_ne(&session_name);
    };

    if let Some(Command::Sessions(Sessions::Attach {
        session_name,
        create,
        index,
        options,
    })) = opts.command.clone()
    {
        let config_options = match options.as_deref() {
            Some(SessionCommand::Options(o)) => config_options.merge_from_cli(o.to_owned().into()),
            None => config_options,
        };

        let client = if let Some(idx) = index {
            attach_with_session_index(config_options.clone(), idx, create)
        } else {
            if create {
                session_name.clone().map(start_client_plan);
            }
            attach_with_session_name(session_name, config_options.clone(), create)
        };

        if let Ok(val) = std::env::var(envs::SESSION_NAME_ENV_KEY) {
            if val == *client.get_session_name() {
                eprintln!("You are trying to attach to the current session (\"{}\"). Zellij does not support nesting a session in itself.", val);
                process::exit(1);
            }
        }

        let attach_layout = match client {
            ClientInfo::Attach(_, _) => None,
            ClientInfo::New(_) => Some(layout),
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
        if let Some(session_name) = opts.session.clone() {
            start_client_plan(session_name.clone());
            start_client_impl(
                Box::new(os_input),
                opts,
                config,
                config_options,
                ClientInfo::New(session_name),
                Some(layout),
            );
        } else {
            if let Some(session_name) = config_options.session_name.as_ref() {
                if let Ok(val) = envs::get_session_name() {
                    // This prevents the same type of recursion as above, only that here we
                    // don't get the command to "attach", but to start a new session instead.
                    // This occurs for example when declaring the session name inside a layout
                    // file and then, from within this session, trying to open a new zellij
                    // session with the same layout. This causes an infinite recursion in the
                    // `zellij_server::terminal_bytes::listen` task, flooding the server and
                    // clients with infinite `Render` requests.
                    if *session_name == val {
                        eprintln!("You are trying to attach to the current session (\"{}\"). Zellij does not support nesting a session in itself.", session_name);
                        process::exit(1);
                    }
                }
                match config_options.attach_to_session {
                    Some(true) => {
                        let client = attach_with_session_name(
                            Some(session_name.clone()),
                            config_options.clone(),
                            true,
                        );
                        let attach_layout = match client {
                            ClientInfo::Attach(_, _) => None,
                            ClientInfo::New(_) => Some(layout),
                        };
                        start_client_impl(
                            Box::new(os_input),
                            opts,
                            config,
                            config_options,
                            client,
                            attach_layout,
                        );
                    },
                    _ => {
                        start_client_plan(session_name.clone());
                        start_client_impl(
                            Box::new(os_input),
                            opts,
                            config,
                            config_options.clone(),
                            ClientInfo::New(session_name.clone()),
                            Some(layout),
                        );
                    },
                }
                // after we detach, this happens and so we need to exit before the rest of the
                // function happens
                // TODO: offload this to a different function
                process::exit(0);
            }

            let session_name = names::Generator::default().next().unwrap();
            start_client_plan(session_name.clone());
            start_client_impl(
                Box::new(os_input),
                opts,
                config,
                config_options,
                ClientInfo::New(session_name),
                Some(layout),
            );
        }
    }
}
