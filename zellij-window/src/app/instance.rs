use std::fs::{File, OpenOptions, TryLockError};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{anyhow, Context, Result};
use interprocess::local_socket::prelude::*;
use zellij_utils::app_control::{
    read_request, refusal_for, send_app_request_to, write_reply, AppReply, AppRequest,
};
use zellij_utils::consts::{APP_CONTROL_SOCKET_NAME, APP_LOCK_FILE_NAME};

pub const HANDOFF_WAIT: Duration = Duration::from_secs(3);

pub enum Claim {
    Primary(Instance),
    Handed(AppReply),
}

pub struct Instance {
    _lock: File,
    control: PathBuf,
    listener: interprocess::local_socket::Listener,
    #[cfg(windows)]
    reply_listener: interprocess::local_socket::Listener,
}

pub fn claim(dir: &Path, request: &AppRequest, wait: Duration) -> Result<Claim> {
    std::fs::create_dir_all(dir)
        .with_context(|| format!("failed to create the folder {}", dir.display()))?;
    let lock_path = dir.join(APP_LOCK_FILE_NAME);
    let control = dir.join(APP_CONTROL_SOCKET_NAME);
    let deadline = Instant::now() + wait;
    loop {
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&lock_path)
            .with_context(|| format!("failed to open {}", lock_path.display()))?;
        match lock.try_lock() {
            Ok(()) => return become_primary(lock, control).map(Claim::Primary),
            Err(TryLockError::WouldBlock) => {},
            Err(TryLockError::Error(e)) => {
                return Err(e).with_context(|| format!("failed to lock {}", lock_path.display()))
            },
        }
        match send_app_request_to(&control, request) {
            Ok(reply) => return Ok(Claim::Handed(reply)),
            Err(e) if Instant::now() >= deadline => {
                return Err(anyhow!(
                    "another Zellij app holds {} but does not answer: {}",
                    lock_path.display(),
                    e
                ))
            },
            Err(_) => std::thread::sleep(Duration::from_millis(100)),
        }
    }
}

fn become_primary(mut lock: File, control: PathBuf) -> Result<Instance> {
    let _ = lock.set_len(0);
    let _ = write!(lock, "{}", std::process::id());
    #[cfg(unix)]
    if control.exists() {
        std::fs::remove_file(&control)
            .with_context(|| format!("failed to remove the stale socket {}", control.display()))?;
    }
    let listener = zellij_utils::consts::ipc_bind(&control)
        .with_context(|| format!("failed to listen on {}", control.display()))?;
    #[cfg(windows)]
    let reply_listener = zellij_utils::consts::ipc_bind_reply(&control)
        .with_context(|| format!("failed to listen on {}-reply", control.display()))?;
    Ok(Instance {
        _lock: lock,
        control,
        listener,
        #[cfg(windows)]
        reply_listener,
    })
}

impl Instance {
    pub fn serve(self, handle: impl Fn(AppRequest) -> AppReply + Send + 'static) -> Result<Holder> {
        let Instance {
            _lock,
            control,
            listener,
            #[cfg(windows)]
            reply_listener,
        } = self;
        let holder = Holder {
            _lock,
            control: control.clone(),
        };
        std::thread::Builder::new()
            .name("app_control".to_owned())
            .spawn(move || {
                for stream in listener.incoming() {
                    let Ok(stream) = stream else {
                        continue;
                    };
                    #[cfg(windows)]
                    let reply = match reply_listener.accept() {
                        Ok(reply) => reply,
                        Err(_) => continue,
                    };
                    #[cfg(not(windows))]
                    let reply = match interprocess::TryClone::try_clone(&stream) {
                        Ok(reply) => reply,
                        Err(_) => continue,
                    };
                    answer(stream, reply, &handle);
                }
            })?;
        Ok(holder)
    }
}

fn answer(
    mut stream: interprocess::local_socket::Stream,
    mut reply: interprocess::local_socket::Stream,
    handle: &impl Fn(AppRequest) -> AppReply,
) {
    let _ = stream.set_recv_timeout(Some(Duration::from_secs(2)));
    let answer = match read_request(&mut stream) {
        Ok(request) => handle(request),
        Err(e) => refusal_for(&e),
    };
    let _ = write_reply(&mut reply, &answer);
}

pub struct Holder {
    _lock: File,
    control: PathBuf,
}

impl Drop for Holder {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.control);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::channel;

    fn request() -> AppRequest {
        AppRequest::OpenWindow(Default::default())
    }

    #[test]
    fn the_first_launch_becomes_the_app_and_a_second_hands_its_request_over() {
        let dir = tempfile::TempDir::new().unwrap();
        let first = match claim(dir.path(), &request(), HANDOFF_WAIT).unwrap() {
            Claim::Primary(instance) => instance,
            Claim::Handed(_) => panic!("nothing was running yet"),
        };
        let (sender, received) = channel();
        let _holder = first
            .serve(move |request| {
                let _ = sender.send(request);
                AppReply::Done
            })
            .unwrap();
        let wanted = AppRequest::OpenWindow(zellij_utils::app_control::OpenWindowRequest {
            session: Some("api".to_owned()),
            ..Default::default()
        });
        match claim(dir.path(), &wanted, HANDOFF_WAIT).unwrap() {
            Claim::Handed(reply) => assert_eq!(reply, AppReply::Done),
            Claim::Primary(_) => panic!("a second instance must not start"),
        }
        assert_eq!(
            received.recv_timeout(Duration::from_secs(2)).unwrap(),
            wanted
        );
    }

    #[test]
    fn a_panel_shortcut_is_handed_to_the_running_app_with_its_token() {
        let dir = tempfile::TempDir::new().unwrap();
        let first = match claim(dir.path(), &request(), HANDOFF_WAIT).unwrap() {
            Claim::Primary(instance) => instance,
            Claim::Handed(_) => panic!("nothing was running yet"),
        };
        let (sender, received) = channel();
        let _holder = first
            .serve(move |request| {
                let _ = sender.send(request);
                AppReply::Done
            })
            .unwrap();
        let wanted = AppRequest::TogglePanel {
            view: zellij_utils::app_control::PopupView::Inbox,
            token: Some("token".to_owned()),
        };
        match claim(dir.path(), &wanted, HANDOFF_WAIT).unwrap() {
            Claim::Handed(reply) => assert_eq!(reply, AppReply::Done),
            Claim::Primary(_) => panic!("a second instance must not start"),
        }
        assert_eq!(
            received.recv_timeout(Duration::from_secs(2)).unwrap(),
            wanted
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_lock_and_socket_left_by_a_crashed_app_are_taken_over() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join(APP_LOCK_FILE_NAME), "999999").unwrap();
        let stale = dir.path().join(APP_CONTROL_SOCKET_NAME);
        {
            let _listener = std::os::unix::net::UnixListener::bind(&stale).unwrap();
        }
        assert!(
            stale.exists(),
            "a crashed app leaves its socket file behind"
        );
        match claim(dir.path(), &request(), Duration::from_millis(200)).unwrap() {
            Claim::Primary(instance) => {
                let holder = instance.serve(|_| AppReply::Done).unwrap();
                assert_eq!(
                    send_app_request_to(&stale, &AppRequest::Ping).unwrap(),
                    AppReply::Done
                );
                drop(holder);
                assert!(!stale.exists(), "a clean exit removes the socket");
            },
            Claim::Handed(_) => panic!("a dead app cannot take a request"),
        }
    }

    #[test]
    fn a_held_lock_with_no_answer_is_reported_after_the_wait() {
        let dir = tempfile::TempDir::new().unwrap();
        let held = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(dir.path().join(APP_LOCK_FILE_NAME))
            .unwrap();
        held.lock().unwrap();
        let started = Instant::now();
        assert!(claim(dir.path(), &request(), Duration::from_millis(300)).is_err());
        assert!(started.elapsed() >= Duration::from_millis(300));
    }
}
