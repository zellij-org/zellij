//! Exclusive claim on a session socket, shared by server bind and client creation.
//!
//! Binding a Unix socket requires deleting any leftover socket file first, so without a lock a
//! second server could delete the socket of a live one. The same lock serialises creation: a
//! client that cannot acquire it must not spawn a replacement. The OS releases the lock on crash.
//!
//! Ordinary listing and pane work never take this lock.

use std::path::{Path, PathBuf};

/// Proof that this process currently owns the session-socket lock file.
///
/// Only the holder may delete the socket on exit. A creator that lost the claim must leave the
/// winner's socket alone and attach instead of spawning.
#[cfg(unix)]
#[derive(Debug)]
pub struct SessionSocketClaim {
    lock: nix::fcntl::Flock<std::fs::File>,
    lock_path: PathBuf,
}

#[cfg(not(unix))]
#[derive(Debug)]
pub struct SessionSocketClaim {
    marker: ExclusiveMarkerClaim,
}

/// Exclusive claim that does not use `flock`, so non-Unix `try_acquire` and Linux tests share it.
///
/// The lock file is the claim marker. A second acquire fails with [`std::io::ErrorKind::AddrInUse`]
/// and must not unlink the winner's marker.
#[derive(Debug)]
pub struct ExclusiveMarkerClaim {
    lock_path: PathBuf,
    _file: std::fs::File,
}

impl ExclusiveMarkerClaim {
    pub fn try_acquire(socket_path: &Path) -> std::io::Result<Self> {
        let lock_path = SessionSocketClaim::lock_path(socket_path)?;
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&lock_path)
            .map_err(|err| {
                if err.kind() == std::io::ErrorKind::AlreadyExists {
                    std::io::Error::new(
                        std::io::ErrorKind::AddrInUse,
                        format!(
                            "another server is already running on {}",
                            socket_path.display()
                        ),
                    )
                } else {
                    err
                }
            })?;
        Ok(ExclusiveMarkerClaim {
            lock_path,
            _file: file,
        })
    }

    pub fn lock_path(&self) -> &Path {
        &self.lock_path
    }
}

impl Drop for ExclusiveMarkerClaim {
    fn drop(&mut self) {
        // A failed second acquire never builds a claim, so it cannot unlink this marker.
        let _ = std::fs::remove_file(&self.lock_path);
    }
}

/// `true` only for [`crate::sessions::WindowsPidLiveness::Dead`].
/// Alive-but-unverified and unknown markers must not be deleted.
pub fn reclaim_may_remove_marker(liveness: crate::sessions::WindowsPidLiveness) -> bool {
    matches!(liveness, crate::sessions::WindowsPidLiveness::Dead)
}

/// Unlink `path` only when [`reclaim_may_remove_marker`] allows it. Otherwise leave the file.
pub fn remove_marker_if_reclaim_allowed(
    path: &Path,
    liveness: crate::sessions::WindowsPidLiveness,
) -> std::io::Result<()> {
    if !reclaim_may_remove_marker(liveness) {
        return Ok(());
    }
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err),
    }
}

impl SessionSocketClaim {
    /// Path of the lock file that guards `socket_path`.
    pub fn lock_path(socket_path: &Path) -> std::io::Result<PathBuf> {
        let file_name = socket_path.file_name().ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!(
                    "session socket path {} has no file name",
                    socket_path.display()
                ),
            )
        })?;
        let mut lock_name = std::ffi::OsString::from(".");
        lock_name.push(file_name);
        lock_name.push(".lock");
        Ok(socket_path.with_file_name(lock_name))
    }

    /// Try to take the exclusive startup claim for `socket_path` without blocking.
    ///
    /// `ErrorKind::AddrInUse` means another live owner (or creator) already holds the claim.
    #[cfg(unix)]
    pub fn try_acquire(socket_path: &Path) -> std::io::Result<Self> {
        use nix::fcntl::{Flock, FlockArg};
        use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
        let lock_path = Self::lock_path(socket_path)?;
        loop {
            let file = std::fs::OpenOptions::new()
                .create(true)
                .write(true)
                .mode(0o600)
                .open(&lock_path)?;
            let lock = Flock::lock(file, FlockArg::LockExclusiveNonblock).map_err(
                |(_, errno)| match errno {
                    nix::errno::Errno::EWOULDBLOCK => std::io::Error::new(
                        std::io::ErrorKind::AddrInUse,
                        format!(
                            "another server is already running on {}",
                            socket_path.display()
                        ),
                    ),
                    errno => std::io::Error::from(errno),
                },
            )?;
            // An exiting owner may have deleted the lock file between our open and lock. Retry,
            // otherwise two claimants could each hold a lock on a different inode.
            let held = lock.metadata()?;
            match std::fs::metadata(&lock_path) {
                Ok(current) if current.dev() == held.dev() && current.ino() == held.ino() => {
                    return Ok(SessionSocketClaim { lock, lock_path });
                },
                _ => continue,
            }
        }
    }

    /// Non-Unix has no flock. The exclusive marker claim is what a second creator must lose,
    /// and losing it must not delete the winner's marker.
    #[cfg(not(unix))]
    pub fn try_acquire(socket_path: &Path) -> std::io::Result<Self> {
        Ok(SessionSocketClaim {
            marker: ExclusiveMarkerClaim::try_acquire(socket_path)?,
        })
    }

    /// Drop the claim while still holding the flock, then release the flock so a successor can
    /// lock a fresh file rather than the unlinked inode.
    #[cfg(unix)]
    pub fn release(self) {
        drop(std::fs::remove_file(&self.lock_path));
        drop(self.lock);
    }

    #[cfg(not(unix))]
    pub fn release(self) {
        drop(self);
    }

    /// While this claim is held, unlink `socket_path` only after confirming it is still abandoned
    /// (connection refused or missing). A live listener is left alone. Ordinary listing must never
    /// call this — that is the stale-observer race.
    #[cfg(unix)]
    pub fn reclaim_stale_socket(&self, socket_path: &Path) -> std::io::Result<()> {
        use crate::consts::ipc_connect_with_timeout;
        use std::time::Duration;
        match ipc_connect_with_timeout(socket_path, Duration::from_millis(100)) {
            Err(err)
                if matches!(
                    err.kind(),
                    std::io::ErrorKind::ConnectionRefused | std::io::ErrorKind::NotFound
                ) =>
            {
                match std::fs::remove_file(socket_path) {
                    Ok(()) => Ok(()),
                    Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
                    Err(err) => Err(err),
                }
            },
            Ok(_) => Err(std::io::Error::new(
                std::io::ErrorKind::AddrInUse,
                format!(
                    "session socket {} became live while reclaiming under ownership",
                    socket_path.display()
                ),
            )),
            // Holding the exclusive claim means no peer binder owns this path; clear a wedged
            // leftover so bind can proceed.
            Err(_) => match std::fs::remove_file(socket_path) {
                Ok(()) => Ok(()),
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(err) => Err(err),
            },
        }
    }

    /// A claim is not proof the marker is stale. Delete it only when the PID is proven dead.
    #[cfg(not(unix))]
    pub fn reclaim_stale_socket(&self, socket_path: &Path) -> std::io::Result<()> {
        let _held = &self.marker;
        let liveness = crate::sessions::liveness_of_session_marker(socket_path);
        remove_marker_if_reclaim_allowed(socket_path, liveness)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn a_second_claim_on_the_same_socket_is_rejected_while_the_first_is_held() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("session");
        let first = SessionSocketClaim::try_acquire(&path).unwrap();
        let error = SessionSocketClaim::try_acquire(&path).unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::AddrInUse);
        first.release();
        let second = SessionSocketClaim::try_acquire(&path).unwrap();
        second.release();
    }

    #[cfg(unix)]
    #[test]
    fn lock_path_is_a_dotfile_sibling_of_the_socket() {
        let path = Path::new("/run/zellij/abcdef");
        assert_eq!(
            SessionSocketClaim::lock_path(path).unwrap(),
            PathBuf::from("/run/zellij/.abcdef.lock")
        );
    }

    #[cfg(unix)]
    #[test]
    fn reclaim_is_a_no_op_when_the_pathname_is_already_gone() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("gone");
        let claim = SessionSocketClaim::try_acquire(&path).unwrap();
        claim.reclaim_stale_socket(&path).unwrap();
        claim.release();
    }

    #[test]
    fn second_exclusive_claim_reclaim_does_not_remove_the_winners_marker() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("session");
        std::fs::write(&path, b"pid-marker").unwrap();
        let first = ExclusiveMarkerClaim::try_acquire(&path).unwrap();
        let error = ExclusiveMarkerClaim::try_acquire(&path).unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::AddrInUse);
        assert_eq!(std::fs::read(&path).unwrap(), b"pid-marker");
        assert!(
            first.lock_path().is_file(),
            "the losing claim must leave the winner's lock marker in place"
        );
        drop(first);
    }

    #[test]
    fn reclaim_may_remove_marker_only_when_the_pid_is_dead() {
        use crate::sessions::WindowsPidLiveness;

        assert!(!reclaim_may_remove_marker(
            WindowsPidLiveness::AliveUnverified
        ));
        assert!(!reclaim_may_remove_marker(WindowsPidLiveness::Unknown));
        assert!(reclaim_may_remove_marker(WindowsPidLiveness::Dead));

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("marker");
        std::fs::write(&path, b"alive").unwrap();
        remove_marker_if_reclaim_allowed(&path, WindowsPidLiveness::AliveUnverified).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"alive");
        remove_marker_if_reclaim_allowed(&path, WindowsPidLiveness::Unknown).unwrap();
        assert!(
            path.is_file(),
            "unknown liveness must not delete the marker"
        );
        remove_marker_if_reclaim_allowed(&path, WindowsPidLiveness::Dead).unwrap();
        assert!(!path.exists(), "a dead pid may remove the marker");
    }
}
