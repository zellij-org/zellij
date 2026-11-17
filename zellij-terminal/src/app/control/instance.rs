use std::fs::{File, OpenOptions, TryLockError};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Context, Result};
use interprocess::local_socket::prelude::*;
use zellij_utils::app_control::{
    describe_app_at, is_older_app, read_request, refusal_for, send_app_request_to, write_reply,
    AppReply, AppRequest,
};
use zellij_utils::consts::{APP_CONTROL_SOCKET_NAME, APP_LOCK_FILE_NAME};

use crate::app::control::ipc::{self, Listeners};

#[cfg_attr(windows, path = "instance/windows.rs")]
#[cfg_attr(not(windows), path = "instance/unix.rs")]
mod os;

pub const HANDOFF_WAIT: Duration = Duration::from_secs(3);
pub const QUIT_WAIT: Duration = Duration::from_secs(10);

pub enum Claim {
    Primary(Instance),
    Handed(AppReply),
}

pub struct Instance {
    _lock: File,
    control: PathBuf,
    listeners: Listeners,
}

pub fn claim(dir: &Path, request: &AppRequest, wait: Duration) -> Result<Claim> {
    zellij_utils::consts::prepare_private_dir(dir)
        .with_context(|| format!("failed to create the folder {}", dir.display()))?;
    let lock_path = dir.join(APP_LOCK_FILE_NAME);
    let control = dir.join(APP_CONTROL_SOCKET_NAME);
    let deadline = Instant::now() + wait;
    loop {
        let lock = open_lock(&lock_path)?;
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

pub fn older_app(dir: &Path) -> Option<String> {
    let state = describe_app_at(&dir.join(APP_CONTROL_SOCKET_NAME)).ok()?;
    let version = state.version?;
    is_older_app(Some(&version), zellij_utils::consts::VERSION).then_some(version)
}

pub fn close_running(dir: &Path, wait: Duration) -> Result<()> {
    match send_app_request_to(&dir.join(APP_CONTROL_SOCKET_NAME), &AppRequest::QuitApp) {
        Ok(AppReply::Done) => wait_for_release(dir, wait),
        Ok(AppReply::Refused(reason)) => bail!("the older Zellij app did not close: {}", reason),
        Ok(other) => bail!(
            "the older Zellij app answered {:?} instead of closing",
            other
        ),
        Err(e) => bail!("the older Zellij app could not be asked to close: {}", e),
    }
}

fn wait_for_release(dir: &Path, wait: Duration) -> Result<()> {
    let lock_path = dir.join(APP_LOCK_FILE_NAME);
    let deadline = Instant::now() + wait;
    loop {
        let lock = open_lock(&lock_path)?;
        match lock.try_lock() {
            Ok(()) => return Ok(()),
            Err(TryLockError::WouldBlock) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(100))
            },
            Err(TryLockError::WouldBlock) => bail!(
                "the older Zellij app did not close within {} seconds",
                wait.as_secs()
            ),
            Err(TryLockError::Error(e)) => {
                return Err(e).with_context(|| format!("failed to lock {}", lock_path.display()))
            },
        }
    }
}

fn open_lock(lock_path: &Path) -> Result<File> {
    OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(lock_path)
        .with_context(|| format!("failed to open {}", lock_path.display()))
}

fn become_primary(mut lock: File, control: PathBuf) -> Result<Instance> {
    let _ = lock.set_len(0);
    let _ = write!(lock, "{}", std::process::id());
    if control.exists() {
        ipc::remove_stale(&control)
            .with_context(|| format!("failed to remove the stale socket {}", control.display()))?;
    }
    let listeners = Listeners::bind(&control)?;
    Ok(Instance {
        _lock: lock,
        control,
        listeners,
    })
}

pub(crate) type Handler = Arc<dyn Fn(AppRequest) -> AppReply + Send + Sync>;

impl Instance {
    pub fn serve(
        self,
        handle: impl Fn(AppRequest) -> AppReply + Send + Sync + 'static,
    ) -> Result<Holder> {
        let handle: Handler = Arc::new(handle);
        let Instance {
            _lock,
            control,
            listeners,
        } = self;
        let guard = os::Guard::new(&control);
        spawn_answering(listeners, handle.clone())?;
        guard.start(&control, handle)?;
        Ok(Holder {
            _lock,
            control,
            guard,
        })
    }
}

pub(crate) fn spawn_answering(listeners: Listeners, handle: Handler) -> std::io::Result<()> {
    std::thread::Builder::new()
        .name("app_control".to_owned())
        .spawn(move || {
            for (stream, reply) in listeners.connections() {
                answer(stream, reply, &*handle);
            }
        })?;
    Ok(())
}

fn answer(
    mut stream: interprocess::local_socket::Stream,
    mut reply: interprocess::local_socket::Stream,
    handle: &dyn Fn(AppRequest) -> AppReply,
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
    guard: os::Guard,
}

impl Drop for Holder {
    fn drop(&mut self) {
        self.guard.release(&self.control);
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

    #[cfg(unix)]
    #[test]
    fn a_control_socket_replaced_by_another_process_is_made_again() {
        let dir = tempfile::TempDir::new().unwrap();
        let instance = match claim(dir.path(), &request(), HANDOFF_WAIT).unwrap() {
            Claim::Primary(instance) => instance,
            Claim::Handed(_) => panic!("nothing was running yet"),
        };
        let holder = instance.serve(|_| AppReply::Done).unwrap();
        let control = dir.path().join(APP_CONTROL_SOCKET_NAME);
        std::fs::remove_file(&control).unwrap();
        {
            let _stray = std::os::unix::net::UnixListener::bind(&control).unwrap();
        }
        assert!(send_app_request_to(&control, &AppRequest::Ping).is_err());
        match claim(dir.path(), &request(), HANDOFF_WAIT).unwrap() {
            Claim::Handed(reply) => assert_eq!(reply, AppReply::Done),
            Claim::Primary(_) => panic!("the running app still holds the lock"),
        }
        drop(holder);
        assert!(!control.exists(), "a clean exit removes the new socket");
    }

    fn serving(dir: &Path, version: Option<&str>) -> (Holder, std::sync::mpsc::Receiver<()>) {
        let instance = match claim(dir, &request(), HANDOFF_WAIT).unwrap() {
            Claim::Primary(instance) => instance,
            Claim::Handed(_) => panic!("nothing was running yet"),
        };
        let version = version.map(str::to_owned);
        let (quit, asked_to_quit) = channel();
        let holder = instance
            .serve(move |request| match request {
                AppRequest::Describe => AppReply::State(zellij_utils::app_control::AppState {
                    version: version.clone(),
                    ..Default::default()
                }),
                AppRequest::QuitApp => {
                    let _ = quit.send(());
                    AppReply::Done
                },
                _ => AppReply::Done,
            })
            .unwrap();
        (holder, asked_to_quit)
    }

    #[test]
    fn only_an_app_reporting_an_older_version_is_offered_for_replacement() {
        let dir = tempfile::TempDir::new().unwrap();
        assert_eq!(older_app(dir.path()), None, "no app is running");
        for (reported, older) in [
            (None, false),
            (Some(zellij_utils::consts::VERSION), false),
            (Some("999.0.0"), false),
            (Some("0.0.1"), true),
        ] {
            let (holder, _) = serving(dir.path(), reported);
            assert_eq!(
                older_app(dir.path()),
                older.then(|| "0.0.1".to_owned()),
                "{:?}",
                reported
            );
            drop(holder);
        }
    }

    #[test]
    fn an_older_app_is_asked_to_quit_and_its_place_is_free_once_it_lets_go() {
        let dir = tempfile::TempDir::new().unwrap();
        let (holder, asked_to_quit) = serving(dir.path(), Some("0.0.1"));
        let releasing = std::thread::spawn(move || {
            asked_to_quit.recv_timeout(Duration::from_secs(2)).unwrap();
            std::thread::sleep(Duration::from_millis(200));
            drop(holder);
        });
        close_running(dir.path(), QUIT_WAIT).unwrap();
        releasing.join().unwrap();
        match claim(dir.path(), &request(), HANDOFF_WAIT).unwrap() {
            Claim::Primary(_) => {},
            Claim::Handed(_) => panic!("the older app has gone"),
        }
    }

    #[test]
    fn an_app_that_refuses_or_keeps_running_is_not_replaced() {
        let dir = tempfile::TempDir::new().unwrap();
        let instance = match claim(dir.path(), &request(), HANDOFF_WAIT).unwrap() {
            Claim::Primary(instance) => instance,
            Claim::Handed(_) => panic!("nothing was running yet"),
        };
        let refusing = instance
            .serve(|_| AppReply::Refused("busy".to_owned()))
            .unwrap();
        let error = close_running(dir.path(), QUIT_WAIT).err().unwrap();
        assert!(format!("{:#}", error).contains("busy"), "{:#}", error);
        drop(refusing);
        let (_stubborn, _) = serving(dir.path(), Some("0.0.1"));
        assert!(close_running(dir.path(), Duration::from_millis(300)).is_err());
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
