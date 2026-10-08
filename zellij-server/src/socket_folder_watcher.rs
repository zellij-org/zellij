use std::path::PathBuf;

#[cfg(not(target_os = "macos"))]
pub struct SocketFolderWatcher {
    _watcher: notify_debouncer_full::notify::RecommendedWatcher,
}

#[cfg(not(target_os = "macos"))]
impl SocketFolderWatcher {
    pub fn start<F>(folder: PathBuf, on_change: F) -> Result<Self, String>
    where
        F: Fn() + Send + 'static,
    {
        use notify_debouncer_full::notify::{self, RecursiveMode, Watcher};
        let _ = std::fs::create_dir_all(&folder);
        let mut watcher = notify::recommended_watcher(
            move |result: notify::Result<notify::Event>| {
                if let Ok(event) = result {
                    if matches!(
                        event.kind,
                        notify::EventKind::Create(_)
                            | notify::EventKind::Remove(_)
                            | notify::EventKind::Modify(notify::event::ModifyKind::Name(_))
                    ) {
                        on_change();
                    }
                }
            },
        )
        .map_err(|e| e.to_string())?;
        watcher
            .watch(&folder, RecursiveMode::NonRecursive)
            .map_err(|e| e.to_string())?;
        Ok(SocketFolderWatcher { _watcher: watcher })
    }
}

#[cfg(target_os = "macos")]
pub struct SocketFolderWatcher {
    queue: std::os::raw::c_int,
    thread: Option<std::thread::JoinHandle<()>>,
}

#[cfg(target_os = "macos")]
const STOP_EVENT_IDENT: usize = 1;

#[cfg(target_os = "macos")]
fn kevent_for(ident: usize, filter: i16, flags: u16, fflags: u32) -> libc::kevent {
    libc::kevent {
        ident,
        filter,
        flags,
        fflags,
        data: 0,
        udata: std::ptr::null_mut(),
    }
}

#[cfg(target_os = "macos")]
impl SocketFolderWatcher {
    pub fn start<F>(folder: PathBuf, on_change: F) -> Result<Self, String>
    where
        F: Fn() + Send + 'static,
    {
        use std::ffi::CString;
        use std::os::unix::ffi::OsStrExt;
        let _ = std::fs::create_dir_all(&folder);
        let path = CString::new(folder.as_os_str().as_bytes()).map_err(|e| e.to_string())?;
        let folder_fd = unsafe { libc::open(path.as_ptr(), libc::O_EVTONLY) };
        if folder_fd < 0 {
            return Err(std::io::Error::last_os_error().to_string());
        }
        let queue = unsafe { libc::kqueue() };
        if queue < 0 {
            unsafe { libc::close(folder_fd) };
            return Err(std::io::Error::last_os_error().to_string());
        }
        let changes = [
            kevent_for(
                folder_fd as usize,
                libc::EVFILT_VNODE,
                libc::EV_ADD | libc::EV_CLEAR,
                libc::NOTE_WRITE | libc::NOTE_DELETE | libc::NOTE_RENAME,
            ),
            kevent_for(
                STOP_EVENT_IDENT,
                libc::EVFILT_USER,
                libc::EV_ADD | libc::EV_CLEAR,
                0,
            ),
        ];
        let registered = unsafe {
            libc::kevent(
                queue,
                changes.as_ptr(),
                changes.len() as libc::c_int,
                std::ptr::null_mut(),
                0,
                std::ptr::null(),
            )
        };
        if registered < 0 {
            unsafe {
                libc::close(folder_fd);
                libc::close(queue);
            }
            return Err(std::io::Error::last_os_error().to_string());
        }
        let thread = std::thread::Builder::new()
            .name("socket_folder_watcher".to_owned())
            .spawn(move || {
                loop {
                    let mut event = kevent_for(0, 0, 0, 0);
                    let received = unsafe {
                        libc::kevent(
                            queue,
                            std::ptr::null(),
                            0,
                            &mut event,
                            1,
                            std::ptr::null(),
                        )
                    };
                    if received < 0 {
                        if std::io::Error::last_os_error().kind()
                            == std::io::ErrorKind::Interrupted
                        {
                            continue;
                        }
                        break;
                    }
                    if received == 0 {
                        continue;
                    }
                    if event.filter == libc::EVFILT_USER {
                        break;
                    }
                    on_change();
                }
                unsafe {
                    libc::close(folder_fd);
                    libc::close(queue);
                }
            })
            .map_err(|e| e.to_string())?;
        Ok(SocketFolderWatcher {
            queue,
            thread: Some(thread),
        })
    }
}

#[cfg(target_os = "macos")]
impl Drop for SocketFolderWatcher {
    fn drop(&mut self) {
        let trigger = [kevent_for(
            STOP_EVENT_IDENT,
            libc::EVFILT_USER,
            0,
            libc::NOTE_TRIGGER,
        )];
        unsafe {
            libc::kevent(
                self.queue,
                trigger.as_ptr(),
                1,
                std::ptr::null_mut(),
                0,
                std::ptr::null(),
            );
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::channel;
    use std::time::Duration;

    #[test]
    fn a_new_entry_in_the_folder_is_reported() {
        let folder = tempfile::tempdir().unwrap();
        let (sender, receiver) = channel();
        let watcher = SocketFolderWatcher::start(folder.path().to_path_buf(), move || {
            let _ = sender.send(());
        })
        .unwrap();
        std::fs::write(folder.path().join("session"), b"").unwrap();
        assert!(receiver.recv_timeout(Duration::from_secs(5)).is_ok());
        drop(watcher);
    }

    #[test]
    fn changes_in_subfolders_are_not_watched() {
        let folder = tempfile::tempdir().unwrap();
        let nested = folder.path().join("nested");
        std::fs::create_dir_all(&nested).unwrap();
        let (sender, receiver) = channel();
        let _watcher = SocketFolderWatcher::start(folder.path().to_path_buf(), move || {
            let _ = sender.send(());
        })
        .unwrap();
        std::fs::write(nested.join("inner"), b"").unwrap();
        assert!(receiver.recv_timeout(Duration::from_millis(300)).is_err());
    }
}
