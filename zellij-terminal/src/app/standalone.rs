use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use interprocess::local_socket::prelude::*;
use zellij_utils::app_control::{
    read_reply, refusal_for, write_request, AppReply, AppRequest, OpenWindowRequest, PopupView,
    ReadError, WindowState,
};
use zellij_utils::data::PaneId;

use crate::app::instance::{spawn_answering, Handler};
use crate::app::AppEvent;
use crate::window::AppPoster;

const WINDOWS_DIR_NAME: &str = "windows";
const ANSWER_WAIT: Duration = Duration::from_millis(500);

pub fn windows_dir() -> PathBuf {
    zellij_utils::consts::app_dir().join(WINDOWS_DIR_NAME)
}

pub fn raise_request(session: &str, pane_id: Option<(u32, bool)>) -> AppRequest {
    AppRequest::OpenWindow(OpenWindowRequest {
        session: Some(session.to_owned()),
        pane_id,
        ..Default::default()
    })
}

pub fn detach_request() -> AppRequest {
    AppRequest::ShowPopup(PopupView::Quit)
}

fn pane_of(pane_id: Option<(u32, bool)>) -> Option<PaneId> {
    pane_id.map(|(id, plugin)| {
        if plugin {
            PaneId::Plugin(id)
        } else {
            PaneId::Terminal(id)
        }
    })
}

fn answer(request: AppRequest, poster: &AppPoster) -> AppReply {
    let event = match request {
        AppRequest::Ping => return AppReply::Done,
        AppRequest::Describe => return crate::describe(poster),
        AppRequest::OpenWindow(open) => AppEvent::RaiseWindows {
            pane: pane_of(open.pane_id),
        },
        AppRequest::ShowPopup(PopupView::Quit) => AppEvent::DetachWindows,
        _ => return refusal_for(&ReadError::Unknown),
    };
    if poster.post(event) {
        AppReply::Done
    } else {
        AppReply::Refused("The window is closing.".to_owned())
    }
}

pub struct Listening {
    path: PathBuf,
}

impl Drop for Listening {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

pub(crate) fn listen(poster: AppPoster) -> Option<Listening> {
    match listen_in(&windows_dir(), move |request| answer(request, &poster)) {
        Ok(listening) => Some(listening),
        Err(e) => {
            report!("the desktop app will not see this window: {:#}", e);
            None
        },
    }
}

pub fn listen_in(
    dir: &Path,
    handle: impl Fn(AppRequest) -> AppReply + Send + Sync + 'static,
) -> Result<Listening> {
    if let Some(parent) = dir.parent() {
        zellij_utils::consts::prepare_private_dir(parent)
            .with_context(|| format!("failed to create the folder {}", parent.display()))?;
    }
    zellij_utils::consts::prepare_private_dir(dir)
        .with_context(|| format!("failed to create the folder {}", dir.display()))?;
    let path = dir.join(std::process::id().to_string());
    #[cfg(unix)]
    let _ = std::fs::remove_file(&path);
    let listener = zellij_utils::consts::ipc_bind(&path)
        .with_context(|| format!("failed to listen on {}", path.display()))?;
    let listening = Listening { path: path.clone() };
    let handle: Handler = Arc::new(handle);
    #[cfg(windows)]
    {
        let reply_listener = zellij_utils::consts::ipc_bind_reply(&path)
            .with_context(|| format!("failed to listen on {}-reply", path.display()))?;
        spawn_answering(listener, reply_listener, handle)?;
    }
    #[cfg(not(windows))]
    spawn_answering(listener, handle)?;
    Ok(listening)
}

enum Asked {
    Answered(AppReply),
    Gone,
    Silent,
}

fn ask(socket: &Path, request: &AppRequest) -> Asked {
    let Ok(mut stream) = zellij_utils::consts::ipc_connect(socket) else {
        return Asked::Gone;
    };
    #[cfg(windows)]
    let reply_stream = zellij_utils::consts::ipc_connect_reply(socket);
    #[cfg(not(windows))]
    let reply_stream = interprocess::TryClone::try_clone(&stream);
    let Ok(mut reply_stream) = reply_stream else {
        return Asked::Silent;
    };
    let _ = stream.set_recv_timeout(Some(ANSWER_WAIT));
    let _ = reply_stream.set_recv_timeout(Some(ANSWER_WAIT));
    if write_request(&mut stream, request).is_err() {
        return Asked::Silent;
    }
    match read_reply(&mut reply_stream) {
        Ok(reply) => Asked::Answered(reply),
        Err(_) => Asked::Silent,
    }
}

fn ask_all(dir: &Path, request: &AppRequest) -> Vec<(PathBuf, AppReply)> {
    let sockets: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .filter_map(|entry| entry.ok())
                .map(|entry| entry.path())
                .collect()
        })
        .unwrap_or_default();
    let mut answers = vec![];
    for socket in sockets {
        match ask(&socket, request) {
            Asked::Answered(reply) => answers.push((socket, reply)),
            Asked::Gone => {
                let _ = std::fs::remove_file(&socket);
            },
            Asked::Silent => {},
        }
    }
    answers
}

pub fn windows_in(dir: &Path) -> Vec<(PathBuf, WindowState)> {
    ask_all(dir, &AppRequest::Describe)
        .into_iter()
        .flat_map(|(socket, reply)| match reply {
            AppReply::State(state) => state
                .windows
                .into_iter()
                .map(|window| (socket.clone(), window))
                .collect::<Vec<_>>(),
            _ => vec![],
        })
        .collect()
}

pub fn with_standalone(reply: AppReply) -> AppReply {
    match reply {
        AppReply::State(mut state) => {
            state.windows.extend(
                windows_in(&windows_dir())
                    .into_iter()
                    .map(|(_, window)| window),
            );
            AppReply::State(state)
        },
        other => other,
    }
}

pub fn raise_in(dir: &Path, session: &str, pane_id: Option<(u32, bool)>) -> bool {
    windows_in(dir)
        .into_iter()
        .find(|(_, window)| window.session.as_deref() == Some(session))
        .map(|(socket, _)| {
            matches!(
                ask(&socket, &raise_request(session, pane_id)),
                Asked::Answered(AppReply::Done)
            )
        })
        .unwrap_or(false)
}

pub fn detach_all_in(dir: &Path) -> usize {
    ask_all(dir, &detach_request()).len()
}

pub fn detach_all_soon() {
    let _ = std::thread::Builder::new()
        .name("app_detach_standalone".to_owned())
        .spawn(|| {
            detach_all_in(&windows_dir());
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::channel;
    use std::sync::Mutex;
    use zellij_utils::app_control::AppState;

    fn on(session: &str) -> AppReply {
        AppReply::State(AppState {
            windows: vec![WindowState {
                session: Some(session.to_owned()),
            }],
            ..Default::default()
        })
    }

    #[test]
    fn a_listening_window_is_described_raised_and_detached_and_its_socket_goes_with_it() {
        let dir = tempfile::TempDir::new().unwrap();
        let windows = dir.path().join(WINDOWS_DIR_NAME);
        let (sender, received) = channel();
        let sender = Mutex::new(sender);
        let listening = listen_in(&windows, move |request| match request {
            AppRequest::Describe => on("solo"),
            request => {
                let _ = sender.lock().unwrap().send(request);
                AppReply::Done
            },
        })
        .unwrap();
        let found = windows_in(&windows);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].1.session.as_deref(), Some("solo"));
        assert!(!raise_in(&windows, "other", None));
        assert!(raise_in(&windows, "solo", Some((3, false))));
        assert_eq!(
            received.recv_timeout(Duration::from_secs(2)).unwrap(),
            raise_request("solo", Some((3, false)))
        );
        assert_eq!(detach_all_in(&windows), 1);
        assert_eq!(
            received.recv_timeout(Duration::from_secs(2)).unwrap(),
            detach_request()
        );
        drop(listening);
        assert!(windows_in(&windows).is_empty());
        assert_eq!(std::fs::read_dir(&windows).unwrap().count(), 0);
    }

    #[cfg(unix)]
    #[test]
    fn a_socket_left_by_a_window_that_is_gone_is_removed_by_the_scan() {
        let dir = tempfile::TempDir::new().unwrap();
        let stale = dir.path().join("999999");
        {
            let _listener = std::os::unix::net::UnixListener::bind(&stale).unwrap();
        }
        assert!(stale.exists());
        assert!(windows_in(dir.path()).is_empty());
        assert!(!stale.exists());
    }
}
