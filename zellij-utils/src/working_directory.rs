use std::path::Path;

/// Whether the server can start its daemonized process in this directory.
///
/// The server adopts the working directory of the client that spawned it. When the client runs
/// in a directory the current user is not allowed to enter (for example another user's home),
/// daemonizing there fails with `EACCES` and the server exits with a panic.
#[cfg(unix)]
pub fn is_usable_working_directory(path: &Path) -> bool {
    // Opening a directory requires the search permission on it, the same permission a `chdir`
    // into it needs, and it leaves the working directory of this process alone.
    match std::fs::metadata(path) {
        Ok(metadata) if metadata.is_dir() => std::fs::File::open(path).is_ok(),
        _ => false,
    }
}

#[cfg(not(unix))]
pub fn is_usable_working_directory(path: &Path) -> bool {
    path.is_dir()
}

#[cfg(all(test, unix))]
mod tests {
    use super::is_usable_working_directory;
    use nix::unistd::{geteuid, Uid};
    use std::os::unix::fs::PermissionsExt;

    fn is_root() -> bool {
        geteuid() == Uid::from_raw(0)
    }

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "zellij-working-directory-{}-{}",
            name,
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_directory_the_user_owns_is_usable() {
        let dir = temp_dir("owned");
        assert!(is_usable_working_directory(&dir));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_directory_without_search_permission_is_not_usable() {
        if is_root() {
            // Root ignores directory permissions, so the case cannot be built here.
            return;
        }
        let dir = temp_dir("no-search");
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o000)).unwrap();
        assert!(!is_usable_working_directory(&dir));
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_regular_file_and_a_missing_path_are_not_usable() {
        let dir = temp_dir("file");
        let file = dir.join("file");
        std::fs::write(&file, b"contents").unwrap();
        assert!(!is_usable_working_directory(&file));
        assert!(!is_usable_working_directory(&dir.join("missing")));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
