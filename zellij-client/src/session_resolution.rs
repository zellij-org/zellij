use std::path::PathBuf;
use std::time::Duration;

use zellij_utils::cli::{AttachArgs, SessionCommand};
use zellij_utils::consts::session_layout_cache_file_name;
use zellij_utils::envs;
use zellij_utils::input::options::Options;
use zellij_utils::sessions::{
    assert_session_ne, generate_unique_session_name, get_active_session,
    get_sessions_sorted_by_mtime, match_session_name, print_sessions, print_sessions_with_index,
    resurrection_layout, session_exists, session_listing_error_message, ActiveSession,
    SessionNameMatch,
};

use crate::ClientInfo;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stream {
    Out,
    Err,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionListing {
    Active,
    Indexed(Vec<String>),
    Named(Vec<String>),
}

impl SessionListing {
    pub fn print(&self) {
        match self {
            SessionListing::Active => zellij_utils::sessions::list_sessions(false, false, true),
            SessionListing::Indexed(sessions) => print_sessions_with_index(sessions.clone()),
            SessionListing::Named(sessions) => print_sessions(
                sessions
                    .iter()
                    .map(|name| (name.clone(), Duration::default(), false))
                    .collect(),
                false,
                false,
                true,
            ),
        }
    }

    pub fn names(&self) -> Vec<String> {
        match self {
            SessionListing::Active => zellij_utils::sessions::get_sessions()
                .unwrap_or_default()
                .into_iter()
                .map(|(name, _)| name)
                .collect(),
            SessionListing::Indexed(sessions) | SessionListing::Named(sessions) => sessions.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolutionFailure {
    pub message: String,
    pub stream: Stream,
    pub exit_code: i32,
    pub listing: Option<SessionListing>,
}

impl ResolutionFailure {
    fn to_stderr(message: impl Into<String>, exit_code: i32) -> Self {
        ResolutionFailure {
            message: message.into(),
            stream: Stream::Err,
            exit_code,
            listing: None,
        }
    }

    fn to_stdout(message: impl Into<String>, exit_code: i32) -> Self {
        ResolutionFailure {
            message: message.into(),
            stream: Stream::Out,
            exit_code,
            listing: None,
        }
    }

    fn listing(mut self, listing: SessionListing) -> Self {
        self.listing = Some(listing);
        self
    }

    pub fn report(&self) {
        match self.stream {
            Stream::Out => println!("{}", self.message),
            Stream::Err => eprintln!("{}", self.message),
        }
        if let Some(listing) = &self.listing {
            listing.print();
        }
    }

    pub fn report_and_exit(&self) -> ! {
        self.report();
        std::process::exit(self.exit_code)
    }

    pub fn text(&self) -> String {
        match &self.listing {
            Some(listing) => {
                let names = listing.names();
                if names.is_empty() {
                    self.message.clone()
                } else {
                    format!("{}\n{}", self.message, names.join("\n"))
                }
            },
            None => self.message.clone(),
        }
    }
}

pub struct Resolved {
    pub client: ClientInfo,
    pub config_options: Options,
}

fn create_new_client() -> Result<ClientInfo, ResolutionFailure> {
    match generate_unique_session_name() {
        Some(name) => Ok(ClientInfo::New(name, None, None, None)),
        None => Err(ResolutionFailure::to_stderr(
            "Failed to generate a unique session name, giving up",
            1,
        )),
    }
}

fn find_indexed_session(
    sessions: Vec<String>,
    config_options: Options,
    index: usize,
    create: bool,
) -> Result<ClientInfo, ResolutionFailure> {
    match sessions.get(index) {
        Some(session) => Ok(ClientInfo::Attach(session.clone(), config_options)),
        None if create => create_new_client(),
        None => Err(ResolutionFailure::to_stdout(
            format!(
                "No session indexed by {} found. The following sessions are active:",
                index
            ),
            1,
        )
        .listing(SessionListing::Indexed(sessions))),
    }
}

fn attach_with_session_index(
    config_options: Options,
    index: usize,
    create: bool,
) -> Result<ClientInfo, ResolutionFailure> {
    match get_sessions_sorted_by_mtime() {
        Ok(sessions) if sessions.is_empty() => {
            if create {
                create_new_client()
            } else {
                Err(ResolutionFailure::to_stderr(
                    "No active zellij sessions found.",
                    1,
                ))
            }
        },
        Ok(sessions) => find_indexed_session(sessions, config_options, index, create),
        Err(e) => Err(ResolutionFailure::to_stderr(
            format!("Error occurred: {:?}", e),
            1,
        )),
    }
}

pub fn attach_by_name(
    session_name: Option<String>,
    config_options: Options,
    create: bool,
) -> Result<ClientInfo, ResolutionFailure> {
    match &session_name {
        Some(session) if create => match session_exists(session) {
            Ok(true) => Ok(ClientInfo::Attach(session_name.unwrap(), config_options)),
            Ok(false) => Ok(ClientInfo::New(session_name.unwrap(), None, None, None)),
            Err(kind) => Err(ResolutionFailure::to_stderr(
                session_listing_error_message(kind),
                1,
            )),
        },
        Some(prefix) => match match_session_name(prefix) {
            Ok(SessionNameMatch::UniquePrefix(s)) | Ok(SessionNameMatch::Exact(s)) => {
                Ok(ClientInfo::Attach(s, config_options))
            },
            Ok(SessionNameMatch::AmbiguousPrefix(sessions)) => Err(ResolutionFailure::to_stdout(
                format!(
                    "Ambiguous selection: multiple sessions names start with '{}':",
                    prefix
                ),
                1,
            )
            .listing(SessionListing::Named(sessions))),
            Ok(SessionNameMatch::None) => Err(ResolutionFailure::to_stderr(
                format!("No session with the name '{}' found!", prefix),
                1,
            )),
            Err(kind) => Err(ResolutionFailure::to_stderr(
                session_listing_error_message(kind),
                1,
            )),
        },
        None => match get_active_session() {
            ActiveSession::None if create => create_new_client(),
            ActiveSession::None => Err(ResolutionFailure::to_stderr(
                "No active zellij sessions found.",
                1,
            )),
            ActiveSession::One(session_name) => {
                Ok(ClientInfo::Attach(session_name, config_options))
            },
            ActiveSession::Many => Err(ResolutionFailure::to_stdout(
                "Please specify the session to attach to, either by using the full name or a unique prefix.\nThe following sessions are active:",
                1,
            )
            .listing(SessionListing::Active)),
        },
    }
}

pub fn resolve_attach(
    args: &AttachArgs,
    config_options: Options,
    create_detached: bool,
    new_session_cwd: Option<PathBuf>,
) -> Result<Resolved, ResolutionFailure> {
    let config_options = match args.options.as_deref() {
        Some(SessionCommand::Options(o)) => config_options.merge_from_cli(o.to_owned().into()),
        None => config_options,
    };
    let create = args.create || create_detached;
    let session_name = args.session_name.clone();

    let client = if let Some(index) = args.index {
        attach_with_session_index(config_options.clone(), index, create)?
    } else {
        let session_exists = session_name
            .as_ref()
            .and_then(|s| session_exists(&s).ok())
            .unwrap_or(false);
        let resurrection_layout = match session_name
            .as_ref()
            .map(|s| resurrection_layout(&s))
            .transpose()
        {
            Ok(layout) => layout.flatten(),
            Err(e) => return Err(ResolutionFailure::to_stderr(format!("{}", e), 2)),
        };
        if create && !session_exists && resurrection_layout.is_none() {
            session_name.clone().map(|name| assert_session_ne(&name));
        }
        match (session_name.as_ref(), resurrection_layout) {
            (Some(session_name), Some(mut resurrection_layout)) if !session_exists => {
                if args.force_run_commands {
                    resurrection_layout.recursively_add_start_suspended(Some(false));
                }
                ClientInfo::Resurrect(
                    session_name.clone(),
                    session_layout_cache_file_name(session_name.as_ref()),
                    args.force_run_commands,
                    new_session_cwd,
                )
            },
            _ => attach_by_name(session_name, config_options.clone(), create)?,
        }
    };

    Ok(Resolved {
        client,
        config_options,
    })
}

pub fn resolve_watch(
    session_name: Option<String>,
    config_options: Options,
) -> Result<ClientInfo, ResolutionFailure> {
    match &session_name {
        Some(prefix) => match match_session_name(prefix) {
            Ok(SessionNameMatch::UniquePrefix(s)) | Ok(SessionNameMatch::Exact(s)) => {
                Ok(ClientInfo::Watch(s, config_options))
            },
            Ok(SessionNameMatch::AmbiguousPrefix(sessions)) => Err(ResolutionFailure::to_stderr(
                format!(
                    "Ambiguous selection: multiple sessions names start with '{}':",
                    prefix
                ),
                1,
            )
            .listing(SessionListing::Named(sessions))),
            Ok(SessionNameMatch::None) => Err(ResolutionFailure::to_stderr(
                format!("No session with the name '{}' found!", prefix),
                1,
            )),
            Err(kind) => Err(ResolutionFailure::to_stderr(
                session_listing_error_message(kind),
                1,
            )),
        },
        None => match get_active_session() {
            ActiveSession::None => Err(ResolutionFailure::to_stderr(
                "No active zellij sessions found.",
                1,
            )),
            ActiveSession::One(name) => Ok(ClientInfo::Watch(name, config_options)),
            ActiveSession::Many => Err(ResolutionFailure::to_stderr(
                "Please specify the session name to watch.",
                1,
            )),
        },
    }
}

pub fn refuse_self_attach(client: &ClientInfo) -> Result<(), ResolutionFailure> {
    match std::env::var(envs::SESSION_NAME_ENV_KEY) {
        Ok(val) if val == *client.get_session_name() => Err(ResolutionFailure::to_stderr(
            format!(
                "You are trying to attach to the current session (\"{}\"). This is not supported.",
                val
            ),
            1,
        )),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_that_is_neither_live_nor_resurrectable_is_refused_without_creation() {
        let args = AttachArgs {
            session_name: Some("zellij-window-no-such-session".to_owned()),
            ..AttachArgs::default()
        };
        let failure = resolve_attach(&args, Options::default(), false, None)
            .err()
            .expect("a session that does not exist must not resolve");
        assert!(
            failure.message.contains("zellij-window-no-such-session"),
            "{}",
            failure.message
        );
    }

    #[test]
    fn a_name_that_does_not_exist_yet_is_created_when_creation_is_asked_for() {
        let args = AttachArgs {
            session_name: Some("zellij-window-no-such-session".to_owned()),
            create: true,
            ..AttachArgs::default()
        };
        let resolved = resolve_attach(&args, Options::default(), false, None).unwrap();
        assert!(matches!(
            resolved.client,
            ClientInfo::New(ref name, ..) if name == "zellij-window-no-such-session"
        ));
    }

    #[test]
    fn a_watcher_refuses_a_name_that_is_not_live() {
        assert!(resolve_watch(
            Some("zellij-window-no-such-session".to_owned()),
            Options::default()
        )
        .is_err());
    }
}
