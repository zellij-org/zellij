use std::os::unix::fs::FileTypeExt;
use std::{fs, io, process};
use zellij_utils::{
    consts::ZELLIJ_SOCK_DIR,
    interprocess::local_socket::LocalSocketStream,
    ipc::{ClientToServerMsg, IpcSenderWithContext},
};

fn get_sessions() -> Result<Vec<String>, io::ErrorKind> {
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
        Err(err) => {
            if let io::ErrorKind::NotFound = err.kind() {
                Ok(Vec::with_capacity(0))
            } else {
                Err(err.kind())
            }
        }
    }
}

pub(crate) fn list_sessions() {
    let exit_code = match get_sessions() {
        Ok(sessions) => {
            if sessions.is_empty() {
                println!("No active zellij sessions found.");
            } else {
                let curr_session =
                    std::env::var("ZELLIJ_SESSION_NAME").unwrap_or_else(|_| "".into());
                sessions.iter().for_each(|session| {
                    let suffix = if curr_session == *session {
                        " (current)"
                    } else {
                        ""
                    };
                    println!("{}{}", session, suffix);
                })
            }
            0
        }
        Err(e) => {
            eprintln!("Error occured: {:?}", e);
            1
        }
    };
    process::exit(exit_code);
}

pub(crate) fn assert_session(name: &str) {
    let exit_code = match get_sessions() {
        Ok(sessions) => {
            if sessions.iter().any(|s| s == name) {
                return;
            }
            println!("No session named {:?} found.", name);
            0
        }
        Err(e) => {
            eprintln!("Error occured: {:?}", e);
            1
        }
    };
    process::exit(exit_code);
}

pub(crate) fn assert_session_ne(name: &str) {
    let exit_code = match get_sessions() {
        Ok(sessions) => {
            if sessions.iter().all(|s| s != name) {
                return;
            }
            println!("Session with name {:?} aleady exists. Use attach command to connect to it or specify a different name.", name);
            0
        }
        Err(e) => {
            eprintln!("Error occured: {:?}", e);
            1
        }
    };
    process::exit(exit_code);
}

fn assert_socket(name: &str) -> bool {
    let path = &*ZELLIJ_SOCK_DIR.join(name);
    match LocalSocketStream::connect(path) {
        Ok(stream) => {
            IpcSenderWithContext::new(stream).send(ClientToServerMsg::ClientExited);
            true
        }
        Err(e) => {
            if e.kind() == io::ErrorKind::ConnectionRefused {
                drop(fs::remove_file(path));
                false
            } else {
                true
            }
        }
    }
}
