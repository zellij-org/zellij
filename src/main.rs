mod install;
mod sessions;
#[cfg(test)]
mod tests;

use crate::install::populate_data_dir;
use sessions::{
    assert_session, assert_session_ne, get_active_session, get_sessions, kill_session,
    list_sessions, print_sessions, session_exists, ActiveSession,
};
use std::process;
use zellij_client::{os_input_output::get_client_os_input, start_client, ClientInfo};
use zellij_server::{os_input_output::get_server_os_input, start_server};
use zellij_utils::{
    cli::{CliArgs, Command, SessionCommand, Sessions},
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

    if let Some(Command::Sessions(Sessions::KillAllSessions { yes })) = opts.command {
        match get_sessions() {
            Ok(sessions) => {
                if sessions.is_empty() {
                    println!("No active zellij sessions found.");
                    process::exit(1);
                } else {
                    let kill_all_sessions = |sessions: Vec<std::string::String>| {
                        for session in sessions.iter() {
                            kill_session(session);
                        }
                        process::exit(0)
                    };

                    if yes {
                        kill_all_sessions(sessions);
                    } else {
                        use std::io::{stdin, stdout, Write};

                        let mut answer = String::new();
                        println!("WARNING: this action will kill all sessions.");
                        print!("Do you want to continue? [y/N] ");
                        let _ = stdout().flush();
                        stdin().read_line(&mut answer).unwrap();

                        match answer.as_str().trim() {
                            "y" | "Y" | "yes" | "Yes" => kill_all_sessions(sessions),
                            _ => {
                                println!("Abort.");
                                process::exit(1);
                            }
                        }
                    }
                }
            }
            Err(e) => {
                eprintln!("Error occured: {:?}", e);
                process::exit(1);
            }
        }
    }

    if let Some(Command::Sessions(Sessions::KillSession { target_session })) = opts.command.clone()
    {
        match target_session.as_ref() {
            Some(target_session) => {
                assert_session(target_session);
                kill_session(target_session);
                process::exit(0);
            }
            None => {
                println!("Please specify the session name to kill.");
                process::exit(1);
            }
        }
    }

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
            session_name,
            create,
            options,
        })) = opts.command.clone()
        {
            let config_options = match options {
                Some(SessionCommand::Options(o)) => config_options.merge(o),
                None => config_options,
            };

            let (client, attach_layout) = match session_name.as_ref() {
                Some(session) => {
                    if create {
                        if !session_exists(session).unwrap() {
                            (ClientInfo::New(session_name.unwrap()), layout)
                        } else {
                            (
                                ClientInfo::Attach(session_name.unwrap(), config_options.clone()),
                                None,
                            )
                        }
                    } else {
                        assert_session(session);
                        (
                            ClientInfo::Attach(session_name.unwrap(), config_options.clone()),
                            None,
                        )
                    }
                }
                None => match get_active_session() {
                    ActiveSession::None => {
                        if create {
                            (
                                ClientInfo::New(names::Generator::default().next().unwrap()),
                                layout,
                            )
                        } else {
                            println!("No active zellij sessions found.");
                            process::exit(1);
                        }
                    }
                    ActiveSession::One(session_name) => (
                        ClientInfo::Attach(session_name, config_options.clone()),
                        None,
                    ),
                    ActiveSession::Many => {
                        println!("Please specify the session name to attach to. The following sessions are active:");
                        print_sessions(get_sessions().unwrap());
                        process::exit(1);
                    }
                },
            };

            start_client(
                Box::new(os_input),
                opts,
                config,
                config_options,
                client,
                attach_layout,
            );
        } else {
            let session_name = opts.session.clone().unwrap_or_else(|| {
                if let Some(l) = layout.clone() {
                    if let Some(name) = l.session.name {
                        return name;
                    }
                }
                names::Generator::default().next().unwrap()
            });

            match get_sessions() {
                Ok(sessions) => {
                    let session = sessions.iter().find(|&name| name == &session_name);

                    match session {
                        Some(s) => {
                            if let Some(l) = layout.clone() {
                                let attach = l.session.attach.unwrap_or(true);
                                if attach {
                                    let client =
                                        ClientInfo::Attach(s.to_owned(), config_options.clone());
                                    start_client(
                                        Box::new(os_input),
                                        opts,
                                        config,
                                        config_options,
                                        client,
                                        layout,
                                    );
                                    process::exit(0);
                                }
                            }
                            println!("Session with name {:?} aleady exists. Use attach command to connect to it or specify a different name.", s);
                            process::exit(1);
                        }
                        None => {
                            // Determine and initialize the data directory
                            let data_dir =
                                opts.data_dir.clone().unwrap_or_else(get_default_data_dir);
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
                Err(e) => {
                    eprintln!("Error occured: {:?}", e);
                    process::exit(1)
                }
            }
        }
    }
}
