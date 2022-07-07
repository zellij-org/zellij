use crate::install::populate_data_dir;
use crate::sessions::kill_session as kill_session_impl;
use crate::sessions::{
    assert_session, assert_session_ne, get_active_session, get_sessions,
    get_sessions_sorted_by_mtime, match_session_name, print_sessions, print_sessions_with_index,
    session_exists, ActiveSession, SessionNameMatch,
};
use dialoguer::Confirm;
use miette::Result;
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

#[cfg(feature = "unstable")]
use miette::IntoDiagnostic;
#[cfg(feature = "unstable")]
use zellij_utils::input::actions::ActionsFromYaml;

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

pub(crate) fn start_server(path: PathBuf) {
    let os_input = get_os_input(get_server_os_input);
    start_server_impl(Box::new(os_input), path);
}

fn create_new_client() -> ClientInfo {
    ClientInfo::New(names::Generator::default().next().unwrap())
}

fn install_default_assets(opts: &CliArgs) {
    let data_dir = opts.data_dir.clone().unwrap_or_else(get_default_data_dir);
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
        },
    }
}

/// Send a vec of `[Action]` to a currently running session.
#[cfg(feature = "unstable")]
pub(crate) fn send_action_to_session(opts: zellij_utils::cli::CliArgs) {
    match get_active_session() {
        ActiveSession::None => {
            eprintln!("There is no active session!");
            std::process::exit(1);
        },
        ActiveSession::One(session_name) => {
            attach_with_fake_client(opts, &session_name);
        },
        ActiveSession::Many => {
            if let Some(session_name) = opts.session.clone() {
                attach_with_fake_client(opts, &session_name);
            } else if let Ok(session_name) = envs::get_session_name() {
                attach_with_fake_client(opts, &session_name);
            } else {
                println!("Please specify the session name to send actions to. The following sessions are active:");
                print_sessions(get_sessions().unwrap());
                std::process::exit(1);
            }
        },
    };
}

#[cfg(feature = "unstable")]
fn attach_with_fake_client(opts: zellij_utils::cli::CliArgs, name: &str) {
    if let Some(zellij_utils::cli::Command::Sessions(zellij_utils::cli::Sessions::Action {
        action: Some(action),
    })) = opts.command.clone()
    {
        let action = format!("[{}]", action);
        match zellij_utils::serde_yaml::from_str::<ActionsFromYaml>(&action).into_diagnostic() {
            Ok(parsed) => {
                let (config, _, config_options) = match Setup::from_options(&opts) {
                    Ok(results) => results,
                    Err(e) => {
                        eprintln!("{}", e);
                        process::exit(1);
                    },
                };
                let os_input = get_os_input(zellij_client::os_input_output::get_client_os_input);

                let actions = parsed.actions().to_vec();
                log::debug!("Starting fake Zellij client!");
                zellij_client::fake_client::start_fake_client(
                    Box::new(os_input),
                    opts,
                    *Box::new(config),
                    config_options,
                    ClientInfo::New(name.to_string()),
                    None,
                    actions,
                );
                log::debug!("Quitting fake client now.");
                std::process::exit(0);
            },
            Err(e) => {
                eprintln!("{:?}", e);
                std::process::exit(1);
            },
        };
    };
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
            if !session_exists(session).unwrap() {
                ClientInfo::New(session_name.unwrap())
            } else {
                ClientInfo::Attach(session_name.unwrap(), config_options)
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
    let (config, layout, config_options) = match Setup::from_options(&opts) {
        Ok(results) => results,
        Err(e) => {
            eprintln!("{}", e);
            process::exit(1);
        },
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
                eprintln!("You are trying to attach to the current session(\"{}\"). Zellij does not support nesting a session in itself.", val);
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
