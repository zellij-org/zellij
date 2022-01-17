use std::os::unix::fs::FileTypeExt;
use std::path::Path;
use std::time::SystemTime;
use std::{fs, io, process};
use suggestion::Suggest;
use crate::{
    consts::ZELLIJ_SOCK_DIR,
    envs,
    interprocess::local_socket::LocalSocketStream,
    ipc::{ClientToServerMsg, IpcSenderWithContext},
};

pub fn get_sessions() -> Result<Vec<String>, io::ErrorKind> {
    match fs::read_dir(&*ZELLIJ_SOCK_DIR) {
        Ok(files) => {
            let mut sessions = Vec::new();
            files.for_each(|file| {
                let file = file.unwrap();
                let file_name = file.file_name().into_string().unwrap();
                if file.file_type().unwrap().is_socket() && assert_socket(&file_name) {
                    sessions.push(file_name);
                }
            });
            Ok(sessions)
        }
        Err(err) if io::ErrorKind::NotFound != err.kind() => Err(err.kind()),
        Err(_) => Ok(Vec::with_capacity(0)),
    }
}

pub fn get_sessions_sorted_by_creation_date() -> anyhow::Result<Vec<String>> {
    match fs::read_dir(&*ZELLIJ_SOCK_DIR) {
        Ok(files) => {
            let mut sessions_with_creation_date: Vec<(String, SystemTime)> = Vec::new();
            for file in files {
                let file = file?;
                let file_name = file.file_name().into_string().unwrap();
                let file_created_at = file.metadata()?.created()?;
                if file.file_type()?.is_socket() && assert_socket(&file_name) {
                    sessions_with_creation_date.push((file_name, file_created_at));
                }
            }
            sessions_with_creation_date.sort_by_key(|x| x.1); // the oldest one will be the first

            let sessions = sessions_with_creation_date
                .iter()
                .map(|x| x.0.clone())
                .collect();
            Ok(sessions)
        }
        Err(err) if io::ErrorKind::NotFound != err.kind() => Err(err.into()),
        Err(_) => Ok(Vec::with_capacity(0)),
    }
}

pub fn rename_session(target_session: String, new_session_name: String) -> anyhow::Result<()> {
    match fs::read_dir(&*ZELLIJ_SOCK_DIR) {
        Ok(files) => {
            if let Some(session) = files
                .filter(|f| f.is_ok())
                .map(|f| f.unwrap())
                .find(|f| f.file_name().into_string().unwrap() == target_session)
            {
                match fs::rename(
                    session.path(),
                    Path::new(&*ZELLIJ_SOCK_DIR).join(new_session_name.clone()),
                ) {
                    Ok(_) => {
                        envs::set_session_name(new_session_name);
                        Ok(())
                    },
                    Err(err) => Err(anyhow::Error::msg(format!("Rename failed with err: {}", err))),
                }
            }
            else {
                Err(anyhow::Error::msg("Failed reading zellij socket directory"))
            }
        }
        _ => Err(anyhow::Error::msg("Session not found")),
    }
}

fn assert_socket(name: &str) -> bool {
    let path = &*ZELLIJ_SOCK_DIR.join(name);
    match LocalSocketStream::connect(path) {
        Ok(stream) => {
            IpcSenderWithContext::new(stream).send(ClientToServerMsg::ClientExited);
            true
        }
        Err(e) if e.kind() == io::ErrorKind::ConnectionRefused => {
            drop(fs::remove_file(path));
            false
        }
        Err(_) => true,
    }
}

pub fn print_sessions(sessions: Vec<String>) {
    let curr_session = envs::get_session_name().unwrap_or_else(|_| "".into());
    sessions.iter().for_each(|session| {
        let suffix = if curr_session == *session {
            " (current)"
        } else {
            ""
        };
        println!("{}{}", session, suffix);
    })
}

pub fn print_sessions_with_index(sessions: Vec<String>) {
    let curr_session = envs::get_session_name().unwrap_or_else(|_| "".into());
    for (i, session) in sessions.iter().enumerate() {
        let suffix = if curr_session == *session {
            " (current)"
        } else {
            ""
        };
        println!("{}: {}{}", i, session, suffix);
    }
}

pub enum ActiveSession {
    None,
    One(String),
    Many,
}

pub fn get_active_session() -> ActiveSession {
    match get_sessions() {
        Ok(sessions) if sessions.is_empty() => ActiveSession::None,
        Ok(mut sessions) if sessions.len() == 1 => ActiveSession::One(sessions.pop().unwrap()),
        Ok(_) => ActiveSession::Many,
        Err(e) => {
            eprintln!("Error occurred: {:?}", e);
            process::exit(1);
        }
    }
}

pub fn kill_session(name: &str) {
    let path = &*ZELLIJ_SOCK_DIR.join(name);
    match LocalSocketStream::connect(path) {
        Ok(stream) => {
            IpcSenderWithContext::new(stream).send(ClientToServerMsg::KillSession);
        }
        Err(e) => {
            eprintln!("Error occurred: {:?}", e);
            process::exit(1);
        }
    };
}

pub fn list_sessions() {
    let exit_code = match get_sessions() {
        Ok(sessions) if !sessions.is_empty() => {
            print_sessions(sessions);
            0
        }
        Ok(_) => {
            println!("No active zellij sessions found.");
            0
        }
        Err(e) => {
            eprintln!("Error occurred: {:?}", e);
            1
        }
    };
    process::exit(exit_code);
}

pub fn session_exists(name: &str) -> Result<bool, io::ErrorKind> {
    return match get_sessions() {
        Ok(sessions) if sessions.iter().any(|s| s == name) => Ok(true),
        Ok(_) => Ok(false),
        Err(e) => Err(e),
    };
}

pub fn assert_session(name: &str) {
    match session_exists(name) {
        Ok(result) => {
            if result {
                return;
            } else {
                println!("No session named {:?} found.", name);
                if let Some(sugg) = get_sessions().unwrap().suggest(name) {
                    println!("  help: Did you mean `{}`?", sugg);
                }
            }
        }
        Err(e) => {
            eprintln!("Error occurred: {:?}", e);
        }
    };
    process::exit(1);
}

pub fn assert_session_ne(name: &str) {
    match session_exists(name) {
        Ok(result) if !result => return,
        Ok(_) => println!("Session with name {:?} already exists. Use attach command to connect to it or specify a different name.", name),
        Err(e) => eprintln!("Error occurred: {:?}", e),
    };
    process::exit(1);
}
