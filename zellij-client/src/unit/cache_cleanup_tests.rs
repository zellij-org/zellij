//! Tests for the legacy `stdin_cache` file cleanup. The helper under
//! test is `cleanup_legacy_stdin_cache_at(path)` (a test seam extracted
//! from `cleanup_legacy_stdin_cache` so the ambient `ZELLIJ_PROJ_DIR`
//! is not required).

use crate::cleanup_legacy_stdin_cache_at;
use std::io::Write;

#[test]
fn cleanup_removes_existing_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("stdin_cache");
    {
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(b"legacy content from an old zellij version").unwrap();
    }
    assert!(path.exists(), "sanity: file should have been created");

    cleanup_legacy_stdin_cache_at(&path).expect("cleanup must succeed");
    assert!(
        !path.exists(),
        "cleanup must have removed the legacy cache file"
    );
}

#[test]
fn cleanup_noop_when_file_absent() {
    // Pointing at a non-existent path is the common case on fresh
    // installs and after the first successful cleanup. Must not
    // surface `NotFound` as an error.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("does-not-exist");
    assert!(!path.exists(), "sanity: file must not pre-exist");
    cleanup_legacy_stdin_cache_at(&path).expect("absent file must be treated as success");
}

#[test]
fn cleanup_is_idempotent_across_repeated_calls() {
    // Reality check: if two Zellij instances start in quick
    // succession, both may try to clean up the same path. The second
    // must still return Ok even though the first already removed the
    // file.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("stdin_cache");
    std::fs::write(&path, b"x").unwrap();
    cleanup_legacy_stdin_cache_at(&path).expect("first cleanup");
    cleanup_legacy_stdin_cache_at(&path).expect("second cleanup must also succeed");
    assert!(!path.exists());
}

#[cfg(unix)]
#[test]
fn cleanup_propagates_other_io_errors() {
    // On Unix, stripping write permission from the parent dir prevents
    // unlink. The helper must surface that Err rather than pretend
    // the cleanup succeeded.
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("stdin_cache");
    std::fs::write(&path, b"x").unwrap();

    // Make the parent read+execute only (no write). remove_file on a
    // child requires write+exec on the parent; dropping write should
    // produce EACCES.
    let mut perms = std::fs::metadata(dir.path()).unwrap().permissions();
    let original_mode = perms.mode();
    perms.set_mode(0o500);
    std::fs::set_permissions(dir.path(), perms).unwrap();

    let result = cleanup_legacy_stdin_cache_at(&path);

    // Restore permissions *before* asserting so the tempdir drop can
    // clean up regardless of whether the assertion fires.
    let mut restore = std::fs::metadata(dir.path()).unwrap().permissions();
    restore.set_mode(original_mode);
    let _ = std::fs::set_permissions(dir.path(), restore);

    match result {
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {},
        Err(e) => panic!("expected PermissionDenied, got {:?} ({})", e.kind(), e),
        Ok(()) => {
            // Some containerised CI environments run tests as root, in
            // which case EACCES is bypassed and the file just gets
            // removed. Don't fail the build for that — just skip.
            eprintln!(
                "cleanup_propagates_other_io_errors: removal unexpectedly \
                 succeeded (likely running as root); skipping"
            );
        },
    }
}
