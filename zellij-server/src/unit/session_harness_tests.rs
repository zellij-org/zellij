//! Integration test harness prototype.
//!
//! Demonstrates the session registry (sessions.kdl) lifecycle through direct
//! registry API calls — register, rename, exit, migration.
//!
//! ## Concurrency note
//!
//! These tests share a single `sessions.kdl` file (determined by the
//! `ZELLIJ_SOCK_DIR` lazy_static). Each test performs its full workflow
//! inside a single `with_registry` call to avoid races with parallel tests.
//!
//! ## Future direction
//!
//! A full `TestSession` harness that spawns a real server thread (with
//! `ZELLIJ_NO_DAEMONIZE=1` on Unix) and communicates via IPC sockets
//! requires:
//! - Per-test `ZELLIJ_SOCKET_DIR` isolation (before lazy_static init)
//! - A `ServerOsApi` mock that implements `new_client` with real sockets
//! - Thread lifecycle management (spawn, connect, kill, join)

use zellij_utils::sessions::{
    ensure_registry, generate_session_id, with_registry, SessionEntry, SessionState,
};

/// Verify that registering and reading back a session works atomically.
#[test]
fn registry_register_and_read_back() {
    let id = generate_session_id();
    let display_name = format!("harness-test-{}", &id[..8]);

    with_registry(|reg| {
        reg.sessions.push(SessionEntry {
            id: id.clone(),
            display_name: display_name.clone(),
            pid: Some(99999),
            state: SessionState::Running,
            created_at: "2024-01-15T10:00:00Z".to_string(),
            exited_at: None,
        });

        let entry = reg.find_by_id(&id).expect("session not found after insert");
        assert_eq!(entry.display_name, display_name);
        assert_eq!(entry.state, SessionState::Running);
        assert_eq!(entry.pid, Some(99999));

        // Clean up within the same transaction.
        reg.remove_by_id(&id);
    })
    .expect("with_registry failed");
}

/// Verify that renaming a session updates display_name but not the id.
#[test]
fn registry_rename_session() {
    let id = generate_session_id();
    let original_name = format!("rename-orig-{}", &id[..8]);
    let new_name = format!("rename-new-{}", &id[..8]);

    with_registry(|reg| {
        reg.sessions.push(SessionEntry {
            id: id.clone(),
            display_name: original_name.clone(),
            pid: Some(99999),
            state: SessionState::Running,
            created_at: "2024-01-15T10:00:00Z".to_string(),
            exited_at: None,
        });

        // Rename.
        let entry = reg.find_by_id_mut(&id).expect("session not found");
        entry.display_name = new_name.clone();

        // Verify.
        assert!(reg.find_running_by_name(&original_name).is_none());
        let entry = reg
            .find_running_by_name(&new_name)
            .expect("renamed session not found");
        assert_eq!(entry.id, id, "id must not change on rename");

        reg.remove_by_id(&id);
    })
    .expect("with_registry failed");
}

/// Verify that marking a session as exited clears PID and removes it from
/// running lookups.
#[test]
fn registry_mark_session_exited() {
    let id = generate_session_id();
    let name = format!("exit-test-{}", &id[..8]);

    with_registry(|reg| {
        reg.sessions.push(SessionEntry {
            id: id.clone(),
            display_name: name.clone(),
            pid: Some(99999),
            state: SessionState::Running,
            created_at: "2024-01-15T10:00:00Z".to_string(),
            exited_at: None,
        });

        // Simulate server exit.
        let entry = reg.find_by_id_mut(&id).expect("session not found");
        entry.state = SessionState::Exited;
        entry.pid = None;
        entry.exited_at = Some("2024-01-15T18:00:00Z".to_string());

        // Verify.
        let entry = reg.find_by_id(&id).expect("session not found after exit");
        assert_eq!(entry.state, SessionState::Exited);
        assert!(entry.pid.is_none());
        assert!(entry.exited_at.is_some());
        assert!(
            reg.find_running_by_name(&name).is_none(),
            "exited session should not appear in running lookups"
        );

        reg.remove_by_id(&id);
    })
    .expect("with_registry failed");
}

/// Verify that ensure_registry creates the file if missing.
#[test]
fn ensure_registry_creates_file() {
    let registry = ensure_registry();
    assert!(
        zellij_utils::consts::ZELLIJ_SESSIONS_KDL.exists(),
        "sessions.kdl should exist after ensure_registry"
    );
    let _ = registry.running_sessions();
}
