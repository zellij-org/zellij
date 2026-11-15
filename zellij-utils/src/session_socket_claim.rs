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

/// Exclusive claim shared by non-Unix `try_acquire` and Linux tests.
///
/// The lock file is created with `create_new` (Windows `CREATE_NEW`). A leftover
/// file is taken only with the OS file lock (`LockFileEx` / `flock` via
/// [`std::fs::File::try_lock`]), never by unlinking a path that may already be a
/// newer claim. Drop unlinks only when the marker still contains this claim's nonce.
#[derive(Debug)]
pub struct ExclusiveMarkerClaim {
    lock_path: PathBuf,
    nonce: [u8; 16],
    file: std::fs::File,
}

impl ExclusiveMarkerClaim {
    pub fn try_acquire(socket_path: &Path) -> std::io::Result<Self> {
        Self::try_acquire_with_liveness(
            socket_path,
            crate::sessions::liveness_of_session_marker(socket_path),
        )
    }

    /// Same as [`Self::try_acquire`], with the marker liveness supplied by the caller.
    ///
    /// Production calls this with [`crate::sessions::liveness_of_session_marker`]. A leftover
    /// lock file is taken only when that liveness is [`crate::sessions::WindowsPidLiveness::Dead`]
    /// and the OS lock is free. Alive-but-unverified and unknown leave the lock and the marker
    /// in place. Two reclaimers that both observed `Dead` cannot unlink each other's marker:
    /// the loser of [`std::fs::File::try_lock`] gets [`std::io::ErrorKind::AddrInUse`].
    pub fn try_acquire_with_liveness(
        socket_path: &Path,
        liveness: crate::sessions::WindowsPidLiveness,
    ) -> std::io::Result<Self> {
        let lock_path = SessionSocketClaim::lock_path(socket_path)?;
        if let Some(parent) = lock_path.parent() {
            crate::consts::prepare_private_dir(parent)?;
        }
        match open_exclusive_lock(&lock_path) {
            Ok(file) => finish_exclusive_claim(file, lock_path, socket_path),
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {
                if !reclaim_may_remove_marker(liveness) {
                    return Err(marker_addr_in_use(socket_path));
                }
                // The previous process is dead, but a concurrent reclaimer may already
                // hold this path. Lock the existing file; do not unlink it.
                let file = std::fs::OpenOptions::new()
                    .read(true)
                    .write(true)
                    .open(&lock_path)?;
                finish_exclusive_claim(file, lock_path, socket_path)
            },
            Err(err) => Err(err),
        }
    }

    /// Delete `socket_path` only when `liveness` is dead. The caller must already hold this claim.
    pub fn reclaim_marker(
        &self,
        socket_path: &Path,
        liveness: crate::sessions::WindowsPidLiveness,
    ) -> std::io::Result<()> {
        let _held = self;
        remove_marker_if_reclaim_allowed(socket_path, liveness)
    }

    pub fn lock_path(&self) -> &Path {
        &self.lock_path
    }
}

fn open_exclusive_lock(lock_path: &Path) -> std::io::Result<std::fs::File> {
    std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(lock_path)
}

fn fresh_claim_nonce() -> [u8; 16] {
    *uuid::Uuid::new_v4().as_bytes()
}

/// `true` only when `contents` is exactly the nonce this claim published.
///
/// A replacement marker, an empty file, or a different claim must not match.
pub fn marker_contents_belong_to_claim(contents: &[u8], claim_nonce: &[u8]) -> bool {
    !claim_nonce.is_empty() && contents == claim_nonce
}

fn try_lock_claim(file: &std::fs::File, socket_path: &Path) -> std::io::Result<()> {
    match file.try_lock() {
        Ok(()) => Ok(()),
        Err(std::fs::TryLockError::WouldBlock) => Err(marker_addr_in_use(socket_path)),
        Err(std::fs::TryLockError::Error(err)) => Err(err),
    }
}

fn publish_nonce(file: &mut std::fs::File, nonce: &[u8]) -> std::io::Result<()> {
    use std::io::{Seek, SeekFrom, Write};
    file.seek(SeekFrom::Start(0))?;
    file.set_len(0)?;
    file.write_all(nonce)?;
    file.sync_all()?;
    Ok(())
}

/// Read marker bytes from `file`, the handle that already holds the exclusive lock.
///
/// Windows `LockFileEx` is per handle. A second open of the path, including
/// `std::fs::read` in this process, fails while that lock is held. Callers must
/// not drop the lock before comparing these bytes to the nonce just written.
fn read_locked_marker(file: &mut std::fs::File) -> std::io::Result<Vec<u8>> {
    use std::io::{Read, Seek, SeekFrom};
    file.seek(SeekFrom::Start(0))?;
    let mut contents = Vec::new();
    file.read_to_end(&mut contents)?;
    Ok(contents)
}

#[cfg(test)]
thread_local! {
    static VERIFY_MARKER_READ: std::cell::Cell<
        Option<fn(&mut std::fs::File) -> std::io::Result<Vec<u8>>>,
    > = const { std::cell::Cell::new(None) };
}

fn read_marker_for_verify(file: &mut std::fs::File) -> std::io::Result<Vec<u8>> {
    #[cfg(test)]
    if let Some(read) = VERIFY_MARKER_READ.with(|slot| slot.get()) {
        return read(file);
    }
    read_locked_marker(file)
}

/// Bytes that decide whether this claim may unlink `path`.
///
/// A path read sees a replaced inode. When that read is impossible because this
/// handle still holds the lock, the owned handle is the only copy of the bytes.
/// `NotFound` is not treated as an empty marker.
fn read_marker_for_unlink(file: &mut std::fs::File, path: &Path) -> std::io::Result<Vec<u8>> {
    match std::fs::read(path) {
        Ok(contents) => Ok(contents),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Err(err),
        Err(_) => read_locked_marker(file),
    }
}

fn unlink_if_nonce_matches(
    path: &Path,
    contents: &[u8],
    claim_nonce: &[u8],
) -> std::io::Result<bool> {
    if !marker_contents_belong_to_claim(contents, claim_nonce) {
        return Ok(false);
    }
    match std::fs::remove_file(path) {
        Ok(()) => Ok(true),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(err) => Err(err),
    }
}

/// Unlink `path` only when it still contains `claim_nonce`.
///
/// Returns `Ok(false)` when the path is missing or names a replacement. The
/// caller must not treat that as a failed claim it is allowed to destroy.
/// Production release uses [`unlink_locked_marker_if_owned`] while the file
/// lock is still held. This unlocked read is the test double of that rule.
#[cfg(test)]
fn unlink_marker_if_owned(path: &Path, claim_nonce: &[u8]) -> std::io::Result<bool> {
    let contents = match std::fs::read(path) {
        Ok(contents) => contents,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(err) => return Err(err),
    };
    unlink_if_nonce_matches(path, &contents, claim_nonce)
}

/// Same ownership rule as `unlink_marker_if_owned`, while `file` still holds the lock.
fn unlink_locked_marker_if_owned(
    file: &mut std::fs::File,
    path: &Path,
    claim_nonce: &[u8],
) -> std::io::Result<bool> {
    let contents = match read_marker_for_unlink(file, path) {
        Ok(contents) => contents,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(err) => return Err(err),
    };
    unlink_if_nonce_matches(path, &contents, claim_nonce)
}

fn release_failed_publish(file: &mut std::fs::File, path: &Path, nonce: &[u8]) {
    // Truncate already cleared this handle. Unlink only that empty file or a
    // nonce this claim published — never a replacement's bytes.
    let Ok(contents) = read_marker_for_unlink(file, path) else {
        return;
    };
    if contents.is_empty() || marker_contents_belong_to_claim(&contents, nonce) {
        let _ = std::fs::remove_file(path);
    }
}

fn finish_exclusive_claim(
    mut file: std::fs::File,
    lock_path: PathBuf,
    socket_path: &Path,
) -> std::io::Result<ExclusiveMarkerClaim> {
    try_lock_claim(&file, socket_path)?;
    let nonce = fresh_claim_nonce();
    if let Err(err) = publish_nonce(&mut file, &nonce) {
        let _ = file.set_len(0);
        release_failed_publish(&mut file, &lock_path, &nonce);
        return Err(err);
    }
    // Still holding the lock. Identity is the locked handle's bytes, not a second open.
    match read_marker_for_verify(&mut file) {
        Ok(contents) if marker_contents_belong_to_claim(&contents, &nonce) => {
            Ok(ExclusiveMarkerClaim {
                lock_path,
                nonce,
                file,
            })
        },
        Ok(_) => Err(marker_addr_in_use(socket_path)),
        Err(err) => Err(err),
    }
}

fn marker_addr_in_use(socket_path: &Path) -> std::io::Error {
    std::io::Error::new(
        std::io::ErrorKind::AddrInUse,
        format!(
            "another server is already running on {}",
            socket_path.display()
        ),
    )
}

impl Drop for ExclusiveMarkerClaim {
    fn drop(&mut self) {
        // The lock is still held here. Unlink only when the bytes are this nonce.
        let _ = unlink_locked_marker_if_owned(&mut self.file, &self.lock_path, &self.nonce);
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
        if let Some(parent) = lock_path.parent() {
            crate::consts::prepare_private_dir(parent)?;
        }
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
        let liveness = crate::sessions::liveness_of_session_marker(socket_path);
        self.marker.reclaim_marker(socket_path, liveness)
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

    #[test]
    fn a_claim_creates_a_missing_socket_directory() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("missing").join("session");
        let _claim = SessionSocketClaim::try_acquire(&path).unwrap();
        assert!(SessionSocketClaim::lock_path(&path).unwrap().is_file());
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

    #[test]
    fn leftover_lock_of_a_dead_marker_is_replaced_and_the_marker_reclaimed() {
        use crate::sessions::WindowsPidLiveness;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("session");
        std::fs::write(&path, b"dead-pid").unwrap();
        let lock_path = SessionSocketClaim::lock_path(&path).unwrap();
        std::fs::write(&lock_path, b"stale-lock").unwrap();

        let claim =
            ExclusiveMarkerClaim::try_acquire_with_liveness(&path, WindowsPidLiveness::Dead)
                .expect("a dead owner's leftover lock must be replaceable");
        claim
            .reclaim_marker(&path, WindowsPidLiveness::Dead)
            .unwrap();
        assert!(
            !path.exists(),
            "reclaim after replacing the stale lock must remove the dead marker"
        );
        assert!(claim.lock_path().is_file(), "the replacement lock is held");

        std::fs::write(&path, b"winner").unwrap();
        let error = ExclusiveMarkerClaim::try_acquire_with_liveness(
            &path,
            WindowsPidLiveness::AliveUnverified,
        )
        .unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::AddrInUse);
        assert_eq!(std::fs::read(&path).unwrap(), b"winner");
        assert!(
            claim.lock_path().is_file(),
            "an unverified second claim must leave the replacement lock"
        );
        drop(claim);
    }

    #[test]
    fn leftover_lock_of_an_unknown_marker_stays_in_use() {
        use crate::sessions::WindowsPidLiveness;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("session");
        std::fs::write(&path, b"unknown-pid").unwrap();
        let lock_path = SessionSocketClaim::lock_path(&path).unwrap();
        std::fs::write(&lock_path, b"live-lock").unwrap();

        for liveness in [
            WindowsPidLiveness::Unknown,
            WindowsPidLiveness::AliveUnverified,
        ] {
            let error =
                ExclusiveMarkerClaim::try_acquire_with_liveness(&path, liveness).unwrap_err();
            assert_eq!(error.kind(), std::io::ErrorKind::AddrInUse);
            assert_eq!(std::fs::read(&path).unwrap(), b"unknown-pid");
            assert_eq!(std::fs::read(&lock_path).unwrap(), b"live-lock");
        }
    }

    #[test]
    fn marker_ownership_matches_only_this_claim_nonce() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("marker");
        let first = b"claim-nonce-aaaa";
        let second = *b"claim-nonce-bbbb";
        assert_eq!(first.len(), second.len());
        std::fs::write(&path, second).unwrap();
        let contents = std::fs::read(&path).unwrap();
        assert!(
            !marker_contents_belong_to_claim(&contents, first),
            "a different claim must not own the marker"
        );
        assert!(marker_contents_belong_to_claim(&contents, &second));
        assert!(!marker_contents_belong_to_claim(&contents, b""));
        assert!(
            !unlink_marker_if_owned(&path, first).unwrap(),
            "dropping an older nonce must not unlink the replacement"
        );
        assert_eq!(std::fs::read(&path).unwrap(), second);
        assert!(unlink_marker_if_owned(&path, &second).unwrap());
        assert!(!path.exists());
    }

    #[test]
    fn dropping_a_claim_does_not_unlink_a_replacement_nonce() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("session");
        let mut claim = ExclusiveMarkerClaim::try_acquire(&path).unwrap();
        let lock_path = claim.lock_path().to_path_buf();
        let replacement = b"replacement-nonce";
        // Windows locks the marker per handle. Replace the bytes through the
        // handle that owns the lock instead of opening a second one.
        publish_nonce(&mut claim.file, replacement).unwrap();
        drop(claim);
        assert_eq!(
            std::fs::read(&lock_path).unwrap(),
            replacement,
            "drop must leave a marker whose nonce is not this claim"
        );
    }

    #[test]
    fn a_second_dead_reclaimer_does_not_unlink_the_holder() {
        use crate::sessions::WindowsPidLiveness;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("session");
        std::fs::write(&path, b"dead-pid").unwrap();
        let lock_path = SessionSocketClaim::lock_path(&path).unwrap();
        std::fs::write(&lock_path, b"stale-lock").unwrap();

        let mut first =
            ExclusiveMarkerClaim::try_acquire_with_liveness(&path, WindowsPidLiveness::Dead)
                .expect("the first dead reclaimer takes the leftover lock");
        // Read through the locked handle: Windows refuses a second open while
        // this claim holds its lock.
        let held = read_locked_marker(&mut first.file).unwrap();
        let error =
            ExclusiveMarkerClaim::try_acquire_with_liveness(&path, WindowsPidLiveness::Dead)
                .expect_err("the lock holder must exclude the second reclaimer");
        assert_eq!(error.kind(), std::io::ErrorKind::AddrInUse);
        assert_eq!(
            read_locked_marker(&mut first.file).unwrap(),
            held,
            "the loser must not replace or unlink the holder's marker"
        );
        drop(first);
        assert!(
            !lock_path.exists(),
            "the holder still owned the nonce, so drop may unlink it"
        );
    }

    fn locked_read_with_a_flipped_byte(file: &mut std::fs::File) -> std::io::Result<Vec<u8>> {
        let mut contents = read_locked_marker(file)?;
        if let Some(byte) = contents.first_mut() {
            *byte ^= 0xff;
        }
        Ok(contents)
    }

    struct ClearVerifyMarkerRead;

    impl Drop for ClearVerifyMarkerRead {
        fn drop(&mut self) {
            VERIFY_MARKER_READ.with(|slot| slot.set(None));
        }
    }

    #[test]
    fn verification_reads_the_locked_handle_not_a_second_open() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("session");
        let lock_path = SessionSocketClaim::lock_path(&path).unwrap();
        // On Linux a second open still sees the published nonce, so it would
        // accept the claim. Verification must use the locked handle instead.
        VERIFY_MARKER_READ.with(|slot| slot.set(Some(locked_read_with_a_flipped_byte)));
        let _clear = ClearVerifyMarkerRead;
        let error = ExclusiveMarkerClaim::try_acquire(&path).expect_err(
            "a second open of the path sees the nonce on Linux; verification must read the locked handle",
        );
        assert_eq!(error.kind(), std::io::ErrorKind::AddrInUse);
        let published =
            std::fs::read(&lock_path).expect("a mismatched locked read must not unlink");
        assert_eq!(published.len(), 16);
        assert!(marker_contents_belong_to_claim(&published, &published));
        drop(_clear);
        // The rejected observation left this claim's nonce behind and did not
        // take ownership. Remove that file, then check the real locked read.
        std::fs::remove_file(&lock_path).unwrap();
        let mut claim = ExclusiveMarkerClaim::try_acquire(&path).unwrap();
        assert_eq!(read_locked_marker(&mut claim.file).unwrap(), claim.nonce);
    }
}
