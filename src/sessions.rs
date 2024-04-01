use std::collections::HashMap;
use std::fs::DirEntry;
use std::iter::empty;
#[cfg(unix)]
use std::os::unix::fs::FileTypeExt;
use std::time::{Duration, SystemTime};
use std::{fs, io, process, thread};
use suggest::Suggest;
use zellij_utils::anyhow::Context;
use zellij_utils::consts::ZELLIJ_SESSION_INFO_CACHE_DIR;
use zellij_utils::errors::FatalError;
use zellij_utils::{
    anyhow,
    consts::{session_info_folder_for_session, session_layout_cache_file_name, ZELLIJ_SOCK_DIR},
    envs,
    humantime::format_duration,
    input::layout::Layout,
    ipc::{ClientToServerMsg, ServerToClientMsg},
};

pub(crate) fn get_sessions() -> Result<Vec<(String, Duration)>, io::ErrorKind> {
    match iter_sessions() {
        Ok(files) => {
            let mut sessions = Vec::new();
            files.for_each(|file| {
                let file_name = file.file_name().into_string().unwrap();
                let ctime = std::fs::metadata(&file.path())
                    .ok()
                    .and_then(|f| f.created().ok())
                    .and_then(|d| d.elapsed().ok())
                    .unwrap_or_default();
                let duration = Duration::from_secs(ctime.as_secs());
                if is_socket(&file).unwrap() && assert_socket(&file_name) {
                    sessions.push((file_name, duration));
                }
                // TODO windows
            });
            Ok(sessions)
        },
        Err(err) => Err(err.kind()),
    }
}

fn iter_sessions() -> Result<Box<dyn Iterator<Item = DirEntry>>, io::Error> {
    #[cfg(windows)]
    {
        use std::path::PathBuf;

        let path = PathBuf::from("\\\\.\\pipe\\");
        match fs::read_dir(path) {
            Ok(pipes) => Ok(Box::new(
                pipes
                    .map(|file| file.unwrap())
                    .filter(|file| file.path().starts_with(&*ZELLIJ_SOCK_DIR)),
            )),
            Err(err) if io::ErrorKind::NotFound != err.kind() => Err(err),
            Err(_) => Ok(Box::new(empty())),
        }
    }
    #[cfg(unix)]
    {
        match fs::read_dir(&*ZELLIJ_SOCK_DIR) {
            Ok(files) => Ok(files.map(|file| file.unwrap())),
            Err(err) if io::ErrorKind::NotFound != err.kind() => Err(err),
            Err(_) => Ok(empty()),
        }
    }
}

fn is_socket(file: &DirEntry) -> io::Result<bool> {
    zellij_utils::is_socket(file)
}

pub(crate) fn get_resurrectable_sessions() -> Vec<(String, Duration, Layout)> {
    match fs::read_dir(&*ZELLIJ_SESSION_INFO_CACHE_DIR) {
        Ok(files_in_session_info_folder) => {
            let files_that_are_folders = files_in_session_info_folder
                .filter_map(|f| f.ok().map(|f| f.path()))
                .filter(|f| f.is_dir());
            files_that_are_folders
                .filter_map(|folder_name| {
                    let layout_file_name =
                        session_layout_cache_file_name(&folder_name.display().to_string());
                    let raw_layout = match std::fs::read_to_string(&layout_file_name) {
                        Ok(raw_layout) => raw_layout,
                        Err(e) => {
                            log::error!(
                                "Failed to read resurrection layout file: {:?} at {:?}",
                                e,
                                &layout_file_name
                            );
                            return None;
                        },
                    };
                    let ctime = match std::fs::metadata(&layout_file_name)
                        .and_then(|metadata| metadata.created())
                    {
                        Ok(created) => Some(created),
                        Err(e) => {
                            log::error!(
                                "Failed to read created stamp of resurrection file: {:?}",
                                e
                            );
                            None
                        },
                    };
                    let layout = match Layout::from_kdl(
                        &raw_layout,
                        layout_file_name.display().to_string(),
                        None,
                        None,
                    ) {
                        Ok(layout) => layout,
                        Err(e) => {
                            log::error!("Failed to parse resurrection layout file: {}", e);
                            return None;
                        },
                    };
                    let elapsed_duration = ctime
                        .map(|ctime| {
                            Duration::from_secs(ctime.elapsed().ok().unwrap_or_default().as_secs())
                        })
                        .unwrap_or_default();
                    let session_name = folder_name
                        .file_name()
                        .map(|f| std::path::PathBuf::from(f).display().to_string())?;
                    Some((session_name, elapsed_duration, layout))
                })
                .collect()
        },
        Err(e) => {
            log::error!(
                "Failed to read session_info cache folder: \"{:?}\": {:?}",
                &*ZELLIJ_SESSION_INFO_CACHE_DIR,
                e
            );
            vec![]
        },
    }
}

pub(crate) fn get_sessions_sorted_by_mtime() -> anyhow::Result<Vec<String>> {
    let mut sessions_with_mtime: Vec<(String, SystemTime)> = Vec::new();
    for file in iter_sessions()? {
        let file_name = file.file_name().into_string().unwrap();
        let file_modified_at = file.metadata()?.modified()?;
        if is_socket(&file)? && assert_socket(&file_name) {
            sessions_with_mtime.push((file_name, file_modified_at));
        }
    }
    sessions_with_mtime.sort_by_key(|x| x.1); // the oldest one will be the first
    let sessions = sessions_with_mtime.iter().map(|x| x.0.clone()).collect();
    Ok(sessions)
}

fn assert_socket(name: &str) -> bool {
    let path = &*ZELLIJ_SOCK_DIR.join(name);
    match zellij_utils::ipc::try_connect_to_server::<ClientToServerMsg, ServerToClientMsg>(path) {
        Ok((mut sender, mut receiver)) => {
            sender
                .send(ClientToServerMsg::ConnStatus)
                .with_context(|| "Query connection status")
                .non_fatal();
            match receiver.recv() {
                Ok((ServerToClientMsg::Connected, _)) => true,
                Err(_) => false,
                Ok(x) => {
                    dbg!(x);
                    false
                },
            }
        },
        Err(e) if e.kind() == io::ErrorKind::ConnectionRefused => {
            drop(fs::remove_file(path));
            false
        },
        Err(_) => false,
    }
}

pub(crate) fn print_sessions(
    mut sessions: Vec<(String, Duration, bool)>,
    no_formatting: bool,
    short: bool,
) {
    // (session_name, timestamp, is_dead)
    let curr_session = envs::get_session_name().unwrap_or_else(|_| "".into());
    sessions.sort_by(|a, b| a.1.cmp(&b.1));
    sessions
        .iter()
        .for_each(|(session_name, timestamp, is_dead)| {
            if short {
                println!("{}", session_name);
                return;
            }
            if no_formatting {
                let suffix = if curr_session == *session_name {
                    format!("(current)")
                } else if *is_dead {
                    format!("(EXITED - attach to resurrect)")
                } else {
                    String::new()
                };
                let timestamp = format!("[Created {} ago]", format_duration(*timestamp));
                println!("{} {} {}", session_name, timestamp, suffix);
            } else {
                let formatted_session_name = format!("\u{1b}[32;1m{}\u{1b}[m", session_name);
                let suffix = if curr_session == *session_name {
                    format!("(current)")
                } else if *is_dead {
                    format!("(\u{1b}[31;1mEXITED\u{1b}[m - attach to resurrect)")
                } else {
                    String::new()
                };
                let timestamp = format!(
                    "[Created \u{1b}[35;1m{}\u{1b}[m ago]",
                    format_duration(*timestamp)
                );
                println!("{} {} {}", formatted_session_name, timestamp, suffix);
            }
        })
}

pub(crate) fn print_sessions_with_index(sessions: Vec<String>) {
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

pub(crate) enum ActiveSession {
    None,
    One(String),
    Many,
}

pub(crate) fn get_active_session() -> ActiveSession {
    match get_sessions() {
        Ok(sessions) if sessions.is_empty() => ActiveSession::None,
        Ok(mut sessions) if sessions.len() == 1 => ActiveSession::One(sessions.pop().unwrap().0),
        Ok(_) => ActiveSession::Many,
        Err(e) => {
            eprintln!("Error occurred: {:?}", e);
            process::exit(1);
        },
    }
}

pub(crate) fn kill_session(name: &str) {
    let path = &*ZELLIJ_SOCK_DIR.join(name);
    match zellij_utils::ipc::try_connect_to_server::<ClientToServerMsg, ServerToClientMsg>(path) {
        Ok((mut sender, _receiver)) => {
            sender
                .send(ClientToServerMsg::KillSession)
                .with_context(|| format!("Killing session {name}"))
                .non_fatal();
        },
        Err(e) => {
            eprintln!("Error occurred: {:?}", e);
            process::exit(1);
        },
    };
}

pub(crate) fn delete_session(name: &str, force: bool) {
    if force {
        let path = &*ZELLIJ_SOCK_DIR.join(name);
        let _ =
            zellij_utils::ipc::try_connect_to_server::<ClientToServerMsg, ServerToClientMsg>(path)
                .map(|(mut sender, _receiver)| {
                    sender.send(ClientToServerMsg::KillSession).ok();
                });
    }
    if let Err(e) = std::fs::remove_dir_all(session_info_folder_for_session(name)) {
        if e.kind() == std::io::ErrorKind::NotFound {
            eprintln!("Session: {:?} not found.", name);
            process::exit(2);
        } else {
            log::error!("Failed to remove session {:?}: {:?}", name, e);
        }
    } else {
        println!("Session: {:?} successfully deleted.", name);
    }
}

pub(crate) fn list_sessions(no_formatting: bool, short: bool) {
    let exit_code = match get_sessions() {
        Ok(running_sessions) => {
            let resurrectable_sessions = get_resurrectable_sessions();
            let mut all_sessions: HashMap<String, (Duration, bool)> = resurrectable_sessions
                .iter()
                .map(|(name, timestamp, _layout)| (name.clone(), (timestamp.clone(), true)))
                .collect();
            for (session_name, duration) in running_sessions {
                all_sessions.insert(session_name.clone(), (duration, false));
            }
            if all_sessions.is_empty() {
                eprintln!("No active zellij sessions found.");
                1
            } else {
                print_sessions(
                    all_sessions
                        .iter()
                        .map(|(name, (timestamp, is_dead))| {
                            (name.clone(), timestamp.clone(), *is_dead)
                        })
                        .collect(),
                    no_formatting,
                    short,
                );
                0
            }
        },
        Err(e) => {
            eprintln!("Error occurred: {:?}", e);
            1
        },
    };
    process::exit(exit_code);
}

#[derive(Debug, Clone)]
pub enum SessionNameMatch {
    AmbiguousPrefix(Vec<String>),
    UniquePrefix(String),
    Exact(String),
    None,
}

pub(crate) fn match_session_name(prefix: &str) -> Result<SessionNameMatch, io::ErrorKind> {
    let sessions = get_sessions()?;

    let filtered_sessions: Vec<_> = sessions
        .iter()
        .filter(|s| s.0.starts_with(prefix))
        .collect();

    if filtered_sessions.iter().any(|s| s.0 == prefix) {
        return Ok(SessionNameMatch::Exact(prefix.to_string()));
    }

    Ok({
        match &filtered_sessions[..] {
            [] => SessionNameMatch::None,
            [s] => SessionNameMatch::UniquePrefix(s.0.to_string()),
            _ => SessionNameMatch::AmbiguousPrefix(
                filtered_sessions.into_iter().map(|s| s.0.clone()).collect(),
            ),
        }
    })
}

pub(crate) fn session_exists(name: &str) -> Result<bool, io::ErrorKind> {
    match match_session_name(name) {
        Ok(SessionNameMatch::Exact(_)) => Ok(true),
        Ok(_) => Ok(false),
        Err(e) => Err(e),
    }
}

// if the session is resurrecable, the returned layout is the one to be used to resurrect it
pub(crate) fn resurrection_layout(session_name_to_resurrect: &str) -> Option<Layout> {
    let resurrectable_sessions = get_resurrectable_sessions();
    resurrectable_sessions
        .iter()
        .find_map(|(name, _timestamp, layout)| {
            if name == session_name_to_resurrect {
                Some(layout.clone())
            } else {
                None
            }
        })
}

pub(crate) fn assert_session(name: &str) {
    match session_exists(name) {
        Ok(result) => {
            if result {
                return;
            } else {
                println!("No session named {:?} found.", name);
                if let Some(sugg) = get_sessions()
                    .unwrap()
                    .iter()
                    .map(|s| s.0.clone())
                    .collect::<Vec<_>>()
                    .suggest(name)
                {
                    println!("  help: Did you mean `{}`?", sugg);
                }
            }
        },
        Err(e) => {
            eprintln!("Error occurred: {:?}", e);
        },
    };
    process::exit(1);
}

pub(crate) fn assert_dead_session(name: &str, force: bool) {
    match session_exists(name) {
        Ok(exists) => {
            if exists && !force {
                println!(
                    "A session by the name {:?} exists and is active, use --force to delete it.",
                    name
                )
            } else if exists && force {
                println!("A session by the name {:?} exists and is active, but will be force killed and deleted.", name);
                return;
            } else {
                return;
            }
        },
        Err(e) => {
            eprintln!("Error occurred: {:?}", e);
        },
    };
    process::exit(1);
}

pub(crate) fn assert_session_ne(name: &str) {
    if name.trim().is_empty() {
        eprintln!("Session name cannot be empty. Please provide a specific session name.");
        process::exit(1);
    }
    if name == "." || name == ".." {
        eprintln!("Invalid session name: \"{}\".", name);
        process::exit(1);
    }
    if name.contains('/') {
        eprintln!("Session name cannot contain '/'.");
        process::exit(1);
    }

    match session_exists(name) {
        Ok(result) if !result => {
            let resurrectable_sessions = get_resurrectable_sessions();
            if resurrectable_sessions.iter().find(|(s, _, _)| s == name).is_some() {
                println!("Session with name {:?} already exists, but is dead. Use the attach command to resurrect it or, the delete-session command to kill it or specify a different name.", name);
            } else {
                return
            }
        }
        Ok(_) => println!("Session with name {:?} already exists. Use attach command to connect to it or specify a different name.", name),
        Err(e) => eprintln!("Error occurred: {:?}", e),
    };
    process::exit(1);
}

/// Create a new random name generator
///
/// Used to provide a memorable handle for a session when users don't specify a session name when the session is
/// created.
///
/// Uses the list of adjectives and nouns defined below, with the intention of avoiding unfortunate
/// and offensive combinations. Care should be taken when adding or removing to either list due to the birthday paradox/
/// hash collisions, e.g. with 4096 unique names, the likelihood of a collision in 10 session names is 1%.
pub(crate) fn get_name_generator() -> impl Iterator<Item = String> {
    names::Generator::new(&ADJECTIVES, &NOUNS, names::Name::Plain)
}

const ADJECTIVES: &[&'static str] = &[
    "adamant",
    "adept",
    "adventurous",
    "arcadian",
    "auspicious",
    "awesome",
    "blossoming",
    "brave",
    "charming",
    "chatty",
    "circular",
    "considerate",
    "cubic",
    "curious",
    "delighted",
    "didactic",
    "diligent",
    "effulgent",
    "erudite",
    "excellent",
    "exquisite",
    "fabulous",
    "fascinating",
    "friendly",
    "glowing",
    "gracious",
    "gregarious",
    "hopeful",
    "implacable",
    "inventive",
    "joyous",
    "judicious",
    "jumping",
    "kind",
    "likable",
    "loyal",
    "lucky",
    "marvellous",
    "mellifluous",
    "nautical",
    "oblong",
    "outstanding",
    "polished",
    "polite",
    "profound",
    "quadratic",
    "quiet",
    "rectangular",
    "remarkable",
    "rusty",
    "sensible",
    "sincere",
    "sparkling",
    "splendid",
    "stellar",
    "tenacious",
    "tremendous",
    "triangular",
    "undulating",
    "unflappable",
    "unique",
    "verdant",
    "vitreous",
    "wise",
    "zippy",
];

const NOUNS: &[&'static str] = &[
    "aardvark",
    "accordion",
    "apple",
    "apricot",
    "bee",
    "brachiosaur",
    "cactus",
    "capsicum",
    "clarinet",
    "cowbell",
    "crab",
    "cuckoo",
    "cymbal",
    "diplodocus",
    "donkey",
    "drum",
    "duck",
    "echidna",
    "elephant",
    "foxglove",
    "galaxy",
    "glockenspiel",
    "goose",
    "hill",
    "horse",
    "iguanadon",
    "jellyfish",
    "kangaroo",
    "lake",
    "lemon",
    "lemur",
    "magpie",
    "megalodon",
    "mountain",
    "mouse",
    "muskrat",
    "newt",
    "oboe",
    "ocelot",
    "orange",
    "panda",
    "peach",
    "pepper",
    "petunia",
    "pheasant",
    "piano",
    "pigeon",
    "platypus",
    "quasar",
    "rhinoceros",
    "river",
    "rustacean",
    "salamander",
    "sitar",
    "stegosaurus",
    "tambourine",
    "tiger",
    "tomato",
    "triceratops",
    "ukulele",
    "viola",
    "weasel",
    "xylophone",
    "yak",
    "zebra",
];
