use std::{collections::HashMap, fs, io::Write, path::PathBuf};

use crate::{consts::ZELLIJ_PLUGIN_PERMISSIONS_CACHE, data::PermissionType};

pub type GrantedPermission = HashMap<String, Vec<PermissionType>>;

#[derive(Default, Debug)]
pub struct PermissionCache {
    path: PathBuf,
    granted: GrantedPermission,
}

impl PermissionCache {
    /// Add grants to the existing set for this plugin. Permissions are never
    /// removed here; a denial for a new request must not revoke earlier ones.
    /// Returns whether the durable set grew (new plugin key or new permission).
    pub fn grant_permissions(
        &mut self,
        plugin_name: String,
        permissions: Vec<PermissionType>,
    ) -> bool {
        let mut changed = !self.granted.contains_key(&plugin_name);
        let granted = self.granted.entry(plugin_name).or_default();
        for permission in permissions {
            if !granted.contains(&permission) {
                granted.push(permission);
                changed = true;
            }
        }
        changed
    }

    pub fn get_permissions(&self, plugin_name: String) -> Option<&Vec<PermissionType>> {
        self.granted.get(&plugin_name)
    }

    pub fn check_permissions(
        &self,
        plugin_name: String,
        permissions_to_check: &Vec<PermissionType>,
    ) -> bool {
        if let Some(target) = self.granted.get(&plugin_name) {
            let mut all_granted = true;
            for permission in permissions_to_check {
                if !target.contains(permission) {
                    all_granted = false;
                }
            }
            return all_granted;
        }

        false
    }

    pub fn from_path_or_default(cache_path: Option<PathBuf>) -> Self {
        let cache_path = cache_path.unwrap_or(ZELLIJ_PLUGIN_PERMISSIONS_CACHE.to_path_buf());

        let granted = match fs::read_to_string(cache_path.clone()) {
            Ok(raw_string) => PermissionCache::from_string(raw_string).unwrap_or_default(),
            Err(e) => {
                log::error!("Failed to read permission cache file: {}", e);
                GrantedPermission::default()
            },
        };

        PermissionCache {
            path: cache_path,
            granted,
        }
    }

    pub fn write_to_file(&self) -> std::io::Result<()> {
        let contents = PermissionCache::to_string(&self.granted);
        if let Ok(existing) = fs::read_to_string(&self.path) {
            if existing == contents {
                return Ok(());
            }
        }
        write_atomically(&self.path, contents.as_bytes())
    }

    /// Fresh-read the on-disk cache, apply a grant delta, and publish under a
    /// short exclusive lock. The lock is held only for this transaction — never
    /// across user prompts, plugin execution, or network waits.
    ///
    /// Returns whether the durable set grew (and therefore whether bytes were
    /// published). Unchanged grants leave the file untouched, including any
    /// trailing comments a test or operator may have appended.
    pub fn grant_and_persist(
        cache_path: Option<PathBuf>,
        plugin_name: String,
        permissions: Vec<PermissionType>,
    ) -> std::io::Result<bool> {
        let path = cache_path.unwrap_or(ZELLIJ_PLUGIN_PERMISSIONS_CACHE.to_path_buf());
        with_permission_cache_lock(&path, || {
            let mut cache = PermissionCache::from_path_or_default(Some(path.clone()));
            // Ensure we always write to the intended path even if the default
            // const would otherwise differ (tests pass an explicit path).
            cache.path = path.clone();
            let changed = cache.grant_permissions(plugin_name, permissions);
            if changed {
                cache.write_to_file()?;
            }
            Ok(changed)
        })
    }
}

fn lock_path_for(cache_path: &PathBuf) -> PathBuf {
    let mut lock = cache_path.clone();
    let file_name = cache_path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "permissions.kdl".to_owned());
    lock.set_file_name(format!(".{file_name}.lock"));
    lock
}

fn with_permission_cache_lock<T>(
    cache_path: &PathBuf,
    body: impl FnOnce() -> std::io::Result<T>,
) -> std::io::Result<T> {
    if let Some(parent) = cache_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let lock_path = lock_path_for(cache_path);
    let lock_file = fs::OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .open(&lock_path)?;
    let _guard = exclusive_lock(lock_file)?;
    body()
}

#[cfg(unix)]
fn exclusive_lock(file: fs::File) -> std::io::Result<nix::fcntl::Flock<fs::File>> {
    use nix::fcntl::{Flock, FlockArg};
    // Blocking exclusive flock. `Flock` releases it on drop, including unwind.
    Flock::lock(file, FlockArg::LockExclusive).map_err(|(_file, errno)| std::io::Error::from(errno))
}

/// Whole-file byte range passed to `LockFileEx` / `UnlockFileEx`.
#[cfg(windows)]
const WHOLE_FILE_LOCK_LEN: u32 = u32::MAX;

/// Holds a `LockFileEx` exclusive lock. Drop unlocks, so a panic cannot leave
/// the cache lock stuck for other processes.
#[cfg(windows)]
struct WindowsPermissionLock {
    file: fs::File,
}

#[cfg(windows)]
impl Drop for WindowsPermissionLock {
    fn drop(&mut self) {
        if let Err(err) = windows_whole_file_lock(&self.file, false) {
            log::error!("Failed to unlock permission cache lock: {err}");
        }
    }
}

#[cfg(windows)]
fn exclusive_lock(file: fs::File) -> std::io::Result<WindowsPermissionLock> {
    windows_whole_file_lock(&file, true)?;
    Ok(WindowsPermissionLock { file })
}

#[cfg(windows)]
fn windows_whole_file_lock(file: &fs::File, exclusive: bool) -> std::io::Result<()> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Foundation::HANDLE;
    use windows_sys::Win32::Storage::FileSystem::{
        LockFileEx, UnlockFileEx, LOCKFILE_EXCLUSIVE_LOCK,
    };
    use windows_sys::Win32::System::IO::OVERLAPPED;

    // Synchronous `std::fs::File` handles block inside `LockFileEx` until the
    // exclusive lock is granted. Offset zero plus `u32::MAX` covers the file.
    let mut overlapped: OVERLAPPED = unsafe { std::mem::zeroed() };
    let handle = file.as_raw_handle() as HANDLE;
    let ok = unsafe {
        if exclusive {
            LockFileEx(
                handle,
                LOCKFILE_EXCLUSIVE_LOCK,
                0,
                WHOLE_FILE_LOCK_LEN,
                WHOLE_FILE_LOCK_LEN,
                &mut overlapped,
            )
        } else {
            UnlockFileEx(
                handle,
                0,
                WHOLE_FILE_LOCK_LEN,
                WHOLE_FILE_LOCK_LEN,
                &mut overlapped,
            )
        }
    };
    if ok == 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(not(any(unix, windows)))]
fn exclusive_lock(_file: fs::File) -> std::io::Result<fs::File> {
    // No cross-process lock on this target. Fail closed instead of publishing
    // a read-modify-write that another process can overwrite.
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "permission cache locking is not implemented on this platform",
    ))
}

fn write_atomically(path: &PathBuf, contents: &[u8]) -> std::io::Result<()> {
    let parent = path.parent().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "permission cache path has no parent directory",
        )
    })?;
    fs::create_dir_all(parent)?;
    let mut temporary = tempfile::Builder::new()
        .prefix(".zellij-permissions-")
        .tempfile_in(parent)?;
    temporary.write_all(contents)?;
    temporary.as_file().sync_all()?;
    temporary.persist(path).map_err(|error| error.error)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::PermissionType;
    use std::collections::{HashMap, HashSet};
    use std::io::Write;
    use std::sync::{Arc, Barrier};

    #[test]
    fn write_to_file_replaces_atomically_and_leaves_no_temp() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("permissions.kdl");
        fs::write(&path, "old").unwrap();

        let mut granted = HashMap::new();
        granted.insert(
            "example".to_owned(),
            vec![PermissionType::ReadApplicationState],
        );
        let cache = PermissionCache {
            path: path.clone(),
            granted,
        };
        cache.write_to_file().unwrap();

        let written = fs::read_to_string(&path).unwrap();
        assert!(written.contains("example"));
        assert!(!written.contains("old"));
        let leftover_temps: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|entry| entry.ok())
            .filter(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".zellij-permissions-")
            })
            .collect();
        assert!(
            leftover_temps.is_empty(),
            "temporary permission files must not remain: {:?}",
            leftover_temps
        );
    }

    #[test]
    fn write_to_file_skips_unchanged_contents() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("permissions.kdl");
        let mut granted = HashMap::new();
        granted.insert(
            "example".to_owned(),
            vec![PermissionType::ReadApplicationState],
        );
        let cache = PermissionCache {
            path: path.clone(),
            granted,
        };
        cache.write_to_file().unwrap();
        let first_modified = fs::metadata(&path).unwrap().modified().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        cache.write_to_file().unwrap();
        let second_modified = fs::metadata(&path).unwrap().modified().unwrap();
        assert_eq!(first_modified, second_modified);
    }

    #[test]
    fn grants_add_to_the_existing_set_without_duplicates_or_replacement() {
        let mut cache = PermissionCache::default();
        assert!(cache.grant_permissions(
            "example".to_owned(),
            vec![PermissionType::ReadApplicationState],
        ));
        assert!(
            cache.grant_permissions("example".to_owned(), vec![PermissionType::ReadPaneContents])
        );
        assert!(!cache.grant_permissions(
            "example".to_owned(),
            vec![PermissionType::ReadApplicationState],
        ));
        let granted = cache.get_permissions("example".to_owned()).unwrap();
        assert_eq!(
            granted,
            &vec![
                PermissionType::ReadApplicationState,
                PermissionType::ReadPaneContents
            ]
        );
    }

    #[test]
    fn grant_and_persist_keeps_earlier_disk_grants_and_skips_unchanged_rewrites() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("permissions.kdl");
        // Disk-only grant from an earlier session.
        assert!(PermissionCache::grant_and_persist(
            Some(path.clone()),
            "plugin".to_owned(),
            vec![PermissionType::WriteToStdin],
        )
        .unwrap());

        assert!(PermissionCache::grant_and_persist(
            Some(path.clone()),
            "plugin".to_owned(),
            vec![PermissionType::ReadApplicationState],
        )
        .unwrap());
        assert!(PermissionCache::grant_and_persist(
            Some(path.clone()),
            "plugin".to_owned(),
            vec![PermissionType::RunCommands],
        )
        .unwrap());

        let untouched = "// untouched\n";
        fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap()
            .write_all(untouched.as_bytes())
            .unwrap();

        // Denial is a no-op at the cache boundary (callers must not persist it).
        // Re-granting an already-held permission must not rewrite the file.
        assert!(!PermissionCache::grant_and_persist(
            Some(path.clone()),
            "plugin".to_owned(),
            vec![PermissionType::ReadApplicationState],
        )
        .unwrap());
        assert!(
            fs::read_to_string(&path).unwrap().ends_with(untouched),
            "unchanged grant must leave trailing comments intact"
        );

        let cached: HashSet<_> = PermissionCache::from_path_or_default(Some(path))
            .get_permissions("plugin".to_owned())
            .into_iter()
            .flatten()
            .copied()
            .collect();
        assert_eq!(
            cached,
            HashSet::from([
                PermissionType::WriteToStdin,
                PermissionType::ReadApplicationState,
                PermissionType::RunCommands,
            ])
        );
    }

    #[test]
    fn concurrent_writers_both_read_base_then_both_deltas_survive() {
        // Schedule: seed {A}; force both writers to start together so each would
        // otherwise read the same base before publishing {A,B} / {A,C}. The
        // transaction lock must keep both accepted deltas.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("permissions.kdl");
        assert!(PermissionCache::grant_and_persist(
            Some(path.clone()),
            "plugin".to_owned(),
            vec![PermissionType::WriteToStdin],
        )
        .unwrap());

        let start = Arc::new(Barrier::new(2));
        let path_b = path.clone();
        let start_b = start.clone();
        let writer_b = std::thread::spawn(move || {
            start_b.wait();
            PermissionCache::grant_and_persist(
                Some(path_b),
                "plugin".to_owned(),
                vec![PermissionType::ReadApplicationState],
            )
            .unwrap()
        });
        start.wait();
        assert!(PermissionCache::grant_and_persist(
            Some(path.clone()),
            "plugin".to_owned(),
            vec![PermissionType::RunCommands],
        )
        .unwrap());
        assert!(writer_b.join().unwrap());

        let cached: HashSet<_> = PermissionCache::from_path_or_default(Some(path))
            .get_permissions("plugin".to_owned())
            .into_iter()
            .flatten()
            .copied()
            .collect();
        assert_eq!(
            cached,
            HashSet::from([
                PermissionType::WriteToStdin,
                PermissionType::ReadApplicationState,
                PermissionType::RunCommands,
            ]),
            "both concurrent deltas must survive on top of the seeded base"
        );
    }

    #[test]
    fn concurrent_grant_and_persist_keeps_both_writers_grants() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("permissions.kdl");
        let barrier = Arc::new(Barrier::new(2));

        let path_a = path.clone();
        let barrier_a = barrier.clone();
        let a = std::thread::spawn(move || {
            barrier_a.wait();
            for _ in 0..40 {
                PermissionCache::grant_and_persist(
                    Some(path_a.clone()),
                    "plugin-a".to_owned(),
                    vec![PermissionType::ReadApplicationState],
                )
                .unwrap();
                PermissionCache::grant_and_persist(
                    Some(path_a.clone()),
                    "plugin-shared".to_owned(),
                    vec![PermissionType::RunCommands],
                )
                .unwrap();
            }
        });

        let path_b = path.clone();
        let barrier_b = barrier;
        let b = std::thread::spawn(move || {
            barrier_b.wait();
            for _ in 0..40 {
                PermissionCache::grant_and_persist(
                    Some(path_b.clone()),
                    "plugin-b".to_owned(),
                    vec![PermissionType::WriteToStdin],
                )
                .unwrap();
                PermissionCache::grant_and_persist(
                    Some(path_b.clone()),
                    "plugin-shared".to_owned(),
                    vec![PermissionType::ReadPaneContents],
                )
                .unwrap();
            }
        });

        a.join().unwrap();
        b.join().unwrap();

        let cache = PermissionCache::from_path_or_default(Some(path.clone()));
        assert!(cache.check_permissions(
            "plugin-a".to_owned(),
            &vec![PermissionType::ReadApplicationState]
        ));
        assert!(cache.check_permissions("plugin-b".to_owned(), &vec![PermissionType::WriteToStdin]));
        let shared: HashSet<_> = cache
            .get_permissions("plugin-shared".to_owned())
            .into_iter()
            .flatten()
            .copied()
            .collect();
        assert_eq!(
            shared,
            HashSet::from([
                PermissionType::RunCommands,
                PermissionType::ReadPaneContents,
            ])
        );

        // Failed publish must not leave partial temps behind.
        let leftover_temps: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|entry| entry.ok())
            .filter(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".zellij-permissions-")
            })
            .collect();
        assert!(leftover_temps.is_empty(), "{leftover_temps:?}");
    }

    #[test]
    fn a_failed_write_keeps_the_old_cache_and_leaves_no_partial_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("permissions.kdl");
        // Make the destination a directory so persist/rename fails after the
        // temporary has been written.
        fs::create_dir(&path).unwrap();
        fs::write(path.join("keep"), "").unwrap();

        let err = PermissionCache::grant_and_persist(
            Some(path.clone()),
            "plugin".to_owned(),
            vec![PermissionType::RunCommands],
        );
        assert!(err.is_err());

        let leftover_temps: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|entry| entry.ok())
            .filter(|entry| {
                let name = entry.file_name().to_string_lossy().into_owned();
                name.starts_with(".zellij-permissions-")
            })
            .collect();
        assert!(
            leftover_temps.is_empty(),
            "failed publish must not leave partial temps: {leftover_temps:?}"
        );
        assert!(path.join("keep").exists());
    }

    #[cfg(unix)]
    #[test]
    fn two_threads_granting_different_permissions_keep_both() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("permissions.kdl");
        let barrier = Arc::new(Barrier::new(2));

        let path_a = path.clone();
        let barrier_a = barrier.clone();
        let writer_a = std::thread::spawn(move || {
            barrier_a.wait();
            PermissionCache::grant_and_persist(
                Some(path_a),
                "plugin".to_owned(),
                vec![PermissionType::ReadApplicationState],
            )
            .unwrap()
        });

        let path_b = path.clone();
        barrier.wait();
        assert!(PermissionCache::grant_and_persist(
            Some(path_b),
            "plugin".to_owned(),
            vec![PermissionType::WriteToStdin],
        )
        .unwrap());
        assert!(writer_a.join().unwrap());

        let cached: HashSet<_> = PermissionCache::from_path_or_default(Some(path))
            .get_permissions("plugin".to_owned())
            .into_iter()
            .flatten()
            .copied()
            .collect();
        assert_eq!(
            cached,
            HashSet::from([
                PermissionType::ReadApplicationState,
                PermissionType::WriteToStdin,
            ])
        );
    }

    /// The lock file is the exclusion mechanism. A process-local mutex inside
    /// `grant_and_persist` would not stall this caller: the other thread holds
    /// `exclusive_lock` on that file directly. A no-op lock would publish while
    /// the guard is still held.
    #[cfg(unix)]
    #[test]
    fn lock_file_blocks_grant_and_persist_until_released() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("permissions.kdl");
        let lock_path = lock_path_for(&path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        let lock_file = fs::OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .open(&lock_path)
            .unwrap();
        let guard = exclusive_lock(lock_file).unwrap();

        let (tid_tx, tid_rx) = std::sync::mpsc::channel();
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        let path_worker = path.clone();
        let worker = std::thread::spawn(move || {
            // Linux: publish the tid before blocking so the parent can see the
            // flock wait in /proc. Other Unix targets have no /proc wchan.
            let _ = tid_tx.send(linux_thread_id());
            let result = PermissionCache::grant_and_persist(
                Some(path_worker),
                "plugin".to_owned(),
                vec![PermissionType::RunCommands],
            );
            let _ = done_tx.send(());
            result
        });

        let tid = tid_rx
            .recv_timeout(std::time::Duration::from_secs(2))
            .expect("worker did not start");
        if std::path::Path::new("/proc/self/task").is_dir() {
            assert!(
                wait_until_blocked_on_file_lock(tid, &done_rx),
                "grant_and_persist must block in flock while the lock file is held"
            );
        } else {
            match done_rx.recv_timeout(std::time::Duration::from_millis(200)) {
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {},
                Ok(()) => panic!("grant_and_persist finished while the lock file was still held"),
                Err(err) => panic!("worker ended before acquiring the lock: {err}"),
            }
        }

        drop(guard);
        done_rx
            .recv_timeout(std::time::Duration::from_secs(2))
            .expect("grant_and_persist did not resume after the lock file was released");
        assert!(worker.join().unwrap().unwrap());
        assert!(PermissionCache::from_path_or_default(Some(path))
            .check_permissions("plugin".to_owned(), &vec![PermissionType::RunCommands]));
    }

    #[cfg(unix)]
    fn linux_thread_id() -> Option<u32> {
        let status = fs::read_to_string("/proc/thread-self/status").ok()?;
        status.lines().find_map(|line| {
            let rest = line.strip_prefix("Pid:")?;
            rest.trim().parse().ok()
        })
    }

    #[cfg(unix)]
    fn wait_until_blocked_on_file_lock(
        tid: Option<u32>,
        done_rx: &std::sync::mpsc::Receiver<()>,
    ) -> bool {
        let Some(tid) = tid else {
            return false;
        };
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while std::time::Instant::now() < deadline {
            match done_rx.try_recv() {
                Ok(()) => return false,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => return false,
                Err(std::sync::mpsc::TryRecvError::Empty) => {},
            }
            let wchan =
                fs::read_to_string(format!("/proc/self/task/{tid}/wchan")).unwrap_or_default();
            // `flock` sleeps in `locks_lock_inode_wait` (older kernels say `flock`).
            if wchan.contains("flock") || wchan.contains("lock_inode") {
                return true;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        false
    }
}
