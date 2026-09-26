//! One live server per session name (zellient#18).
//!
//! A second `zellij --server <path>` for a live name used to remove the live server's socket and
//! bind its own. The first server went on running with no path: unreachable, its panes' programs
//! still alive, and still writing the session's metadata. Whatever made a client decide the
//! session was gone (a list that missed it, a resurrect, a web client's create), the server now
//! refuses before it touches the path. It holds an exclusive lock on the name for its whole life,
//! and a listener still accepting on the path (a server started before this lock existed) counts
//! as live too. It runs before daemonizing, so a refusal is the spawning client's non-zero exit.

use interprocess::local_socket::LocalSocketStream;
use nix::fcntl::{flock, FlockArg};
use std::ffi::OsString;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};
use zellij_utils::ipc::{ClientToServerMsg, IpcSenderWithContext};
use zellij_utils::shared::set_permissions;

/// Where the lock for `socket_path`'s name lives: `<socket dir>.server-locks/<name>`, a sibling of
/// the socket dir, so it can never be taken for a session nor collide with a session's name.
pub fn claim_path(socket_path: &Path) -> Result<PathBuf, String> {
    let name = socket_path
        .file_name()
        .ok_or_else(|| format!("no session name in socket path {:?}", socket_path))?;
    let dir = socket_path
        .parent()
        .ok_or_else(|| format!("no socket dir in socket path {:?}", socket_path))?;
    let mut locks = OsString::from(dir.as_os_str());
    locks.push(".server-locks");
    Ok(PathBuf::from(locks).join(name))
}

/// Claim `socket_path`'s session name for this process. Hold the returned file for the server's
/// life: the lock goes when the last descriptor on it closes, so a crash never leaves it stale.
pub fn claim_session_name(socket_path: &Path) -> Result<File, String> {
    let lock_path = claim_path(socket_path)?;
    let lock_dir = lock_path.parent().expect("claim_path always has a parent");
    std::fs::create_dir_all(lock_dir)
        .and_then(|_| set_permissions(lock_dir, 0o700))
        .map_err(|e| format!("cannot create the server lock dir {:?}: {}", lock_dir, e))?;
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .open(&lock_path)
        .map_err(|e| format!("cannot open the server lock {:?}: {}", lock_path, e))?;
    if let Err(e) = flock(file.as_raw_fd(), FlockArg::LockExclusiveNonblock) {
        let holder = std::fs::read_to_string(&lock_path).unwrap_or_else(|r| r.to_string());
        return Err(format!(
            "session {:?} already has a live server (pid {}); refusing to start a second one \
             ({}, lock {:?})",
            socket_path,
            holder.trim(),
            e,
            lock_path
        ));
    }
    if let Ok(stream) = LocalSocketStream::connect(socket_path) {
        // The status probe `zellij list-sessions` sends; the server answers and drops us.
        let mut sender = IpcSenderWithContext::new(stream);
        if let Err(e) = sender.send(ClientToServerMsg::ConnStatus) {
            log::warn!("status probe to the live server on {:?}: {}", socket_path, e);
        }
        return Err(format!(
            "session {:?} already has a live server listening on it (one started without the \
             name lock); refusing to start a second one",
            socket_path
        ));
    }
    Ok(file)
}

/// Record the daemon's pid in its claim, for the message a refused second server prints.
pub fn record_holder(claim: &mut File) -> std::io::Result<()> {
    claim.set_len(0)?;
    write!(claim, "{}", std::process::id())?;
    claim.flush()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::net::UnixListener;

    fn socket_in(dir: &tempfile::TempDir) -> PathBuf {
        dir.path().join("sock").join("session-3")
    }

    #[test]
    fn a_second_claim_on_a_live_name_is_refused_and_names_the_holder() {
        let dir = tempfile::tempdir().unwrap();
        let socket = socket_in(&dir);
        let mut first = claim_session_name(&socket).expect("the first claim");
        record_holder(&mut first).unwrap();
        let refused = claim_session_name(&socket).expect_err("a second claim must be refused");
        assert!(refused.contains("already has a live server"), "{}", refused);
        assert!(refused.contains(&std::process::id().to_string()), "{}", refused);
    }

    #[test]
    fn the_name_is_free_again_once_its_holder_is_gone() {
        let dir = tempfile::tempdir().unwrap();
        let socket = socket_in(&dir);
        drop(claim_session_name(&socket).expect("the first claim"));
        claim_session_name(&socket).expect("a claim after the holder exited");
    }

    #[test]
    fn a_listener_that_predates_the_lock_counts_as_live() {
        let dir = tempfile::tempdir().unwrap();
        let socket = socket_in(&dir);
        std::fs::create_dir_all(socket.parent().unwrap()).unwrap();
        let _old_server = UnixListener::bind(&socket).unwrap();
        let refused = claim_session_name(&socket).expect_err("a live listener must refuse");
        assert!(refused.contains("listening on it"), "{}", refused);
        assert!(socket.exists(), "the refusal must leave the live server's socket alone");
    }

    #[test]
    fn a_stale_socket_with_no_listener_does_not_block_a_new_server() {
        let dir = tempfile::tempdir().unwrap();
        let socket = socket_in(&dir);
        std::fs::create_dir_all(socket.parent().unwrap()).unwrap();
        drop(UnixListener::bind(&socket).unwrap());
        assert!(socket.exists(), "the dead server's socket file is still there");
        claim_session_name(&socket).expect("a dead server's socket must not refuse");
    }

    #[test]
    fn the_lock_is_a_sibling_of_the_socket_dir_never_inside_it() {
        let lock = claim_path(Path::new("/run/user/1000/zellij/0.43.1/session-3")).unwrap();
        assert_eq!(
            lock,
            PathBuf::from("/run/user/1000/zellij/0.43.1.server-locks/session-3")
        );
    }
}
