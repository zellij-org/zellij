use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::app::control::instance::{spawn_answering, Handler};
use crate::app::control::ipc::Listeners;

const SOCKET_CHECK: Duration = Duration::from_secs(2);

pub struct Guard {
    watch: Arc<Mutex<Watch>>,
}

impl Guard {
    pub fn new(control: &Path) -> Self {
        Guard {
            watch: Arc::new(Mutex::new(Watch {
                identity: socket_identity(control),
                stopped: false,
            })),
        }
    }

    pub fn start(&self, control: &Path, handle: Handler) -> std::io::Result<()> {
        spawn_socket_watch(control.to_path_buf(), self.watch.clone(), handle)
    }

    pub fn release(&self, control: &Path) {
        let mut watch = match self.watch.lock() {
            Ok(watch) => watch,
            Err(poisoned) => poisoned.into_inner(),
        };
        watch.stopped = true;
        let current = socket_identity(control);
        if current.is_some() && current == watch.identity {
            let _ = std::fs::remove_file(control);
        }
    }
}

struct Watch {
    identity: Option<(u64, u64)>,
    stopped: bool,
}

fn socket_identity(path: &Path) -> Option<(u64, u64)> {
    use std::os::unix::fs::MetadataExt;
    std::fs::symlink_metadata(path)
        .ok()
        .map(|metadata| (metadata.dev(), metadata.ino()))
}

fn spawn_socket_watch(
    control: PathBuf,
    watch: Arc<Mutex<Watch>>,
    handle: Handler,
) -> std::io::Result<()> {
    std::thread::Builder::new()
        .name("app_control_watch".to_owned())
        .spawn(move || loop {
            std::thread::sleep(SOCKET_CHECK);
            let mut watch = match watch.lock() {
                Ok(watch) => watch,
                Err(poisoned) => poisoned.into_inner(),
            };
            if watch.stopped {
                return;
            }
            let current = socket_identity(&control);
            if current.is_some() && current == watch.identity {
                continue;
            }
            let _ = std::fs::remove_file(&control);
            match zellij_utils::consts::ipc_bind(&control) {
                Ok(listener) => {
                    watch.identity = socket_identity(&control);
                    log::warn!(
                        "the app's control socket {} was removed or replaced, so it was made again",
                        control.display()
                    );
                    if let Err(e) = spawn_answering(Listeners::of(listener), handle.clone()) {
                        log::warn!("the app cannot answer on its new control socket: {}", e);
                        return;
                    }
                },
                Err(e) => log::warn!(
                    "the app could not make its control socket {} again: {}",
                    control.display(),
                    e
                ),
            }
        })?;
    Ok(())
}
