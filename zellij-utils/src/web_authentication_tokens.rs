// TODO: GATE THIS WHOLE FILE AND RELEVANT DEPS BEHIND web_server_capability
use crate::consts::ZELLIJ_PROJ_DIR;
use rusqlite::Connection;
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

#[derive(Debug)]
pub struct TokenInfo {
    pub name: String,
    pub created_at: String,
    pub read_only: bool,
}

#[derive(Debug)]
pub enum TokenError {
    Database(rusqlite::Error),
    Io(std::io::Error),
    DuplicateName(String),
    TokenNotFound(String),
    InvalidToken,
}

impl std::fmt::Display for TokenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TokenError::Database(e) => write!(f, "Database error: {}", e),
            TokenError::Io(e) => write!(f, "IO error: {}", e),
            TokenError::DuplicateName(name) => write!(f, "Token name '{}' already exists", name),
            TokenError::TokenNotFound(name) => write!(f, "Token '{}' not found", name),
            TokenError::InvalidToken => write!(f, "Invalid token"),
        }
    }
}

impl std::error::Error for TokenError {}

impl From<rusqlite::Error> for TokenError {
    fn from(error: rusqlite::Error) -> Self {
        match error {
            rusqlite::Error::SqliteFailure(ffi_error, _)
                if ffi_error.code == rusqlite::ErrorCode::ConstraintViolation =>
            {
                TokenError::DuplicateName("unknown".to_string())
            },
            _ => TokenError::Database(error),
        }
    }
}

impl From<std::io::Error> for TokenError {
    fn from(error: std::io::Error) -> Self {
        TokenError::Io(error)
    }
}

type Result<T> = std::result::Result<T, TokenError>;

fn get_db_path() -> Result<PathBuf> {
    let data_dir = ZELLIJ_PROJ_DIR.data_dir();
    std::fs::create_dir_all(&data_dir)?;

    let db_path = if cfg!(debug_assertions) {
        data_dir.join("tokens_for_dev.db")
    } else {
        data_dir.join("tokens.db")
    };

    Ok(db_path)
}

/// Database files whose schema has already been set up by this process.
///
/// Schema setup (several `CREATE TABLE IF NOT EXISTS` statements and a
/// migration) only has to run once per database file, not on every request.
static INITIALIZED_DBS: Mutex<Option<HashSet<PathBuf>>> = Mutex::new(None);

#[cfg(test)]
static SCHEMA_INIT_COUNTS: Mutex<Option<std::collections::HashMap<PathBuf, usize>>> =
    Mutex::new(None);

fn open_db() -> Result<Connection> {
    open_db_at(&get_db_path()?)
}

fn open_db_at(db_path: &Path) -> Result<Connection> {
    let freshly_created = create_private_db_file(db_path)?;
    let conn = Connection::open(db_path)?;
    // these are per-connection settings, so they have to be applied every time
    conn.execute_batch("PRAGMA busy_timeout = 5000")?;
    conn.execute_batch("PRAGMA foreign_keys = ON")?;

    let mut initialized = INITIALIZED_DBS.lock().unwrap();
    let initialized = initialized.get_or_insert_with(HashSet::new);
    if freshly_created {
        // the file was (re)created behind our back, so whatever we knew about
        // its schema is stale
        initialized.remove(db_path);
    }
    if !initialized.contains(db_path) {
        init_db(&conn)?;
        initialized.insert(db_path.to_path_buf());
        #[cfg(test)]
        {
            *SCHEMA_INIT_COUNTS
                .lock()
                .unwrap()
                .get_or_insert_with(Default::default)
                .entry(db_path.to_path_buf())
                .or_insert(0) += 1;
        }
    }

    Ok(conn)
}

/// Creates the database file if it does not exist yet. On unix it is created
/// with mode 0600 right away, so there is no window in which it is readable by
/// other users. Returns whether this call created the file.
fn create_private_db_file(db_path: &Path) -> Result<bool> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    match options.open(db_path) {
        Ok(_) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            repair_private_db_file_permissions(db_path)?;
            Ok(false)
        },
        Err(e) => Err(e.into()),
    }
}

#[cfg(unix)]
fn repair_private_db_file_permissions(db_path: &Path) -> Result<()> {
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(nix::fcntl::OFlag::O_NOFOLLOW.bits())
        .open(db_path)?;
    if file.metadata()?.permissions().mode() & 0o777 != 0o600 {
        file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

#[cfg(not(unix))]
fn repair_private_db_file_permissions(_db_path: &Path) -> Result<()> {
    Ok(())
}

fn init_db(conn: &Connection) -> Result<()> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS tokens (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            token_hash TEXT UNIQUE NOT NULL,
            name TEXT UNIQUE NOT NULL,
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP
        )",
        [],
    )?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS session_tokens (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            session_token_hash TEXT UNIQUE NOT NULL,
            auth_token_hash TEXT NOT NULL,
            remember_me BOOLEAN NOT NULL DEFAULT 0,
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
            expires_at DATETIME NOT NULL,
            FOREIGN KEY (auth_token_hash) REFERENCES tokens(token_hash)
        )",
        [],
    )?;

    // Migration: Add read_only column if it doesn't exist
    match conn.execute(
        "ALTER TABLE tokens ADD COLUMN read_only BOOLEAN NOT NULL DEFAULT 0",
        [],
    ) {
        Ok(_) => {},
        Err(e) => {
            let err_msg = e.to_string();
            // "duplicate column name" is the expected error when the column already exists
            if !err_msg.contains("duplicate column name") {
                return Err(TokenError::Database(e));
            }
        },
    }

    Ok(())
}

pub fn hash_token(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    let digest = hasher.finalize();
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut hash = String::with_capacity(digest.len() * 2);
    for byte in digest {
        hash.push(HEX[(byte >> 4) as usize] as char);
        hash.push(HEX[(byte & 0xf) as usize] as char);
    }
    hash
}

pub fn create_token(name: Option<String>, read_only: bool) -> Result<(String, String)> {
    let conn = open_db()?;

    let token = Uuid::new_v4().to_string();
    let token_hash = hash_token(&token);

    let token_name = if let Some(n) = name {
        n.to_string()
    } else {
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM tokens", [], |row| row.get(0))?;
        format!("token_{}", count + 1)
    };

    match conn.execute(
        "INSERT INTO tokens (token_hash, name, read_only) VALUES (?1, ?2, ?3)",
        [&token_hash, &token_name, &(read_only as i64).to_string()],
    ) {
        Err(rusqlite::Error::SqliteFailure(ffi_error, _))
            if ffi_error.code == rusqlite::ErrorCode::ConstraintViolation =>
        {
            Err(TokenError::DuplicateName(token_name))
        },
        Err(e) => Err(TokenError::Database(e)),
        Ok(_) => Ok((token, token_name)),
    }
}

pub fn create_session_token(auth_token: &str, remember_me: bool) -> Result<String> {
    create_session_token_on(&open_db()?, auth_token, remember_me)
}

fn create_session_token_on(
    conn: &Connection,
    auth_token: &str,
    remember_me: bool,
) -> Result<String> {
    let auth_token_hash = hash_token(auth_token);

    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM tokens WHERE token_hash = ?1",
        [&auth_token_hash],
        |row| row.get(0),
    )?;

    if count == 0 {
        return Err(TokenError::InvalidToken);
    }

    // only after the credentials checked out: failed logins must not have any
    // side effects on other people's sessions
    cleanup_expired_sessions_on(conn)?;

    let session_token = Uuid::new_v4().to_string();
    let session_token_hash = hash_token(&session_token);

    let expires_at = if remember_me {
        use std::time::{SystemTime, UNIX_EPOCH};
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let four_weeks = 4 * 7 * 24 * 60 * 60;
        format!("datetime({}, 'unixepoch')", now + four_weeks)
    } else {
        // For session-only: very short expiration (e.g., 5 minutes)
        // The browser will handle the session aspect via cookie expiration
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let short_duration = 5 * 60; // 5 minutes
        format!("datetime({}, 'unixepoch')", now + short_duration)
    };

    conn.execute(
        &format!("INSERT INTO session_tokens (session_token_hash, auth_token_hash, remember_me, expires_at) VALUES (?1, ?2, ?3, {})", expires_at),
        [&session_token_hash, &auth_token_hash, &(remember_me as i64).to_string()],
    )?;

    Ok(session_token)
}

pub fn session_token_status(session_token: &str) -> Result<Option<bool>> {
    session_token_status_on(&open_db()?, session_token)
}

fn session_token_status_on(conn: &Connection, session_token: &str) -> Result<Option<bool>> {
    let session_token_hash = hash_token(session_token);

    match conn.query_row(
        "SELECT COALESCE(t.read_only, 1) FROM session_tokens st
         LEFT JOIN tokens t ON t.token_hash = st.auth_token_hash
         WHERE st.session_token_hash = ?1 AND st.expires_at > datetime('now')",
        [&session_token_hash],
        |row| row.get::<_, i64>(0),
    ) {
        Ok(read_only) => Ok(Some(read_only != 0)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(TokenError::Database(e)),
    }
}

pub fn validate_session_token(session_token: &str) -> Result<bool> {
    Ok(session_token_status(session_token)?.is_some())
}

pub fn is_session_token_read_only(session_token: &str) -> Result<bool> {
    session_token_status(session_token)?.ok_or(TokenError::InvalidToken)
}

/// Whether the session identified by `session_token_hash` may stay connected.
///
/// This is for connections that are already open and only kept the hash of
/// their session token. A session is no longer allowed once it has been revoked
/// (its row is gone) or, for a "remember me" session, once it has expired.
///
/// A session without "remember me" is only given a few minutes by
/// `create_session_token`, enough for the browser to establish its connections;
/// it is meant to last as long as the browser session does. Cutting off a live
/// terminal a few minutes after login would be a change of behaviour, so for
/// those sessions only revocation counts.
pub fn session_allows_live_connection(session_token_hash: &str) -> Result<bool> {
    session_allows_live_connection_on(&open_db()?, session_token_hash)
}

fn session_allows_live_connection_on(conn: &Connection, session_token_hash: &str) -> Result<bool> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM session_tokens
         WHERE session_token_hash = ?1
         AND (expires_at > datetime('now') OR remember_me = 0)",
        [&session_token_hash],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

/// How long the row of an expired session without "remember me" is kept.
///
/// Open connections treat a missing row as a revoked session (see
/// `session_allows_live_connection`), and such sessions are meant to last as
/// long as the browser session does, so their rows must not be deleted as soon
/// as their short login window is over.
const SHORT_SESSION_ROW_RETENTION: &str = "-28 days";

pub fn cleanup_expired_sessions() -> Result<usize> {
    cleanup_expired_sessions_on(&open_db()?)
}

fn cleanup_expired_sessions_on(conn: &Connection) -> Result<usize> {
    let rows_affected = conn.execute(
        "DELETE FROM session_tokens
         WHERE (remember_me = 1 AND expires_at <= datetime('now'))
         OR (remember_me = 0 AND expires_at <= datetime('now', ?1))",
        [SHORT_SESSION_ROW_RETENTION],
    )?;

    Ok(rows_affected)
}

pub fn revoke_session_token(session_token: &str) -> Result<bool> {
    let conn = open_db()?;

    let session_token_hash = hash_token(session_token);
    let rows_affected = conn.execute(
        "DELETE FROM session_tokens WHERE session_token_hash = ?1",
        [&session_token_hash],
    )?;

    Ok(rows_affected > 0)
}

pub fn revoke_sessions_for_auth_token(auth_token: &str) -> Result<usize> {
    let conn = open_db()?;

    let auth_token_hash = hash_token(auth_token);
    let rows_affected = conn.execute(
        "DELETE FROM session_tokens WHERE auth_token_hash = ?1",
        [&auth_token_hash],
    )?;

    Ok(rows_affected)
}

pub fn revoke_token(name: &str) -> Result<bool> {
    let mut conn = open_db()?;

    let tx = conn.transaction().map_err(TokenError::Database)?;

    let token_hash = match tx.query_row(
        "SELECT token_hash FROM tokens WHERE name = ?1",
        [&name],
        |row| row.get::<_, String>(0),
    ) {
        Ok(hash) => Some(hash),
        Err(rusqlite::Error::QueryReturnedNoRows) => None,
        Err(e) => return Err(TokenError::Database(e)),
    };

    if let Some(token_hash) = token_hash {
        tx.execute(
            "DELETE FROM session_tokens WHERE auth_token_hash = ?1",
            [&token_hash],
        )?;
    }

    let rows_affected = tx.execute("DELETE FROM tokens WHERE name = ?1", [&name])?;
    tx.commit().map_err(TokenError::Database)?;
    Ok(rows_affected > 0)
}

pub fn revoke_all_tokens() -> Result<usize> {
    let mut conn = open_db()?;

    let tx = conn.transaction().map_err(TokenError::Database)?;
    tx.execute("DELETE FROM session_tokens", [])?;
    let rows_affected = tx.execute("DELETE FROM tokens", [])?;
    tx.commit().map_err(TokenError::Database)?;
    Ok(rows_affected)
}

pub fn rename_token(old_name: &str, new_name: &str) -> Result<()> {
    let conn = open_db()?;

    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM tokens WHERE name = ?1",
        [&old_name],
        |row| row.get(0),
    )?;

    if count == 0 {
        return Err(TokenError::TokenNotFound(old_name.to_string()));
    }

    match conn.execute(
        "UPDATE tokens SET name = ?1 WHERE name = ?2",
        [&new_name, &old_name],
    ) {
        Err(rusqlite::Error::SqliteFailure(ffi_error, _))
            if ffi_error.code == rusqlite::ErrorCode::ConstraintViolation =>
        {
            Err(TokenError::DuplicateName(new_name.to_string()))
        },
        Err(e) => Err(TokenError::Database(e)),
        Ok(_) => Ok(()),
    }
}

pub fn list_tokens() -> Result<Vec<TokenInfo>> {
    let conn = open_db()?;

    let mut stmt =
        conn.prepare("SELECT name, created_at, read_only FROM tokens ORDER BY created_at")?;
    let rows = stmt.query_map([], |row| {
        Ok(TokenInfo {
            name: row.get::<_, String>(0)?,
            created_at: row.get::<_, String>(1)?,
            read_only: row.get::<_, i64>(2)? != 0,
        })
    })?;

    let mut tokens = Vec::new();
    for token in rows {
        tokens.push(token?);
    }
    Ok(tokens)
}

pub fn delete_db() -> Result<()> {
    let db_path = get_db_path()?;
    if db_path.exists() {
        std::fs::remove_file(&db_path)?;
    }
    if let Some(initialized) = INITIALIZED_DBS.lock().unwrap().as_mut() {
        initialized.remove(&db_path);
    }
    Ok(())
}

pub fn validate_token(token: &str) -> Result<bool> {
    let conn = open_db()?;

    let token_hash = hash_token(token);

    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM tokens WHERE token_hash = ?1",
        [&token_hash],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn schema_init_count(db_path: &Path) -> usize {
        SCHEMA_INIT_COUNTS
            .lock()
            .unwrap()
            .as_ref()
            .and_then(|counts| counts.get(db_path).copied())
            .unwrap_or(0)
    }

    #[test]
    fn hashes_are_lowercase_hex_sha256() {
        assert_eq!(
            hash_token("abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn schema_is_set_up_once_per_database_file() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("tokens.db");
        for _ in 0..5 {
            let conn = open_db_at(&db_path).unwrap();
            // the schema is there for every connection
            conn.query_row("SELECT COUNT(*) FROM session_tokens", [], |r| {
                r.get::<_, i64>(0)
            })
            .unwrap();
        }
        assert_eq!(schema_init_count(&db_path), 1);
    }

    #[test]
    fn schema_is_set_up_again_if_the_database_file_is_deleted() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("tokens.db");
        drop(open_db_at(&db_path).unwrap());
        std::fs::remove_file(&db_path).unwrap();
        let conn = open_db_at(&db_path).unwrap();
        conn.query_row("SELECT COUNT(*) FROM tokens", [], |r| r.get::<_, i64>(0))
            .unwrap();
        assert_eq!(schema_init_count(&db_path), 2);
    }

    #[test]
    fn live_connections_survive_only_unrevoked_unexpired_sessions() {
        let dir = tempfile::tempdir().unwrap();
        let conn = open_db_at(&dir.path().join("tokens.db")).unwrap();
        conn.execute(
            "INSERT INTO tokens (token_hash, name) VALUES ('auth', 'auth')",
            [],
        )
        .unwrap();
        let add_session = |hash: &str, remember_me: i64, expires_at: &str| {
            conn.execute(
                "INSERT INTO session_tokens
                 (session_token_hash, auth_token_hash, remember_me, expires_at)
                 VALUES (?1, 'auth', ?2, datetime('now', ?3))",
                rusqlite::params![hash, remember_me, expires_at],
            )
            .unwrap();
        };
        add_session("valid", 1, "+1 hour");
        add_session("remembered_but_expired", 1, "-1 hour");
        add_session("short_lived_and_past_its_window", 0, "-1 hour");

        let allowed = |hash: &str| session_allows_live_connection_on(&conn, hash).unwrap();
        assert!(allowed("valid"));
        assert!(!allowed("remembered_but_expired"));
        assert!(allowed("short_lived_and_past_its_window"));
        assert!(!allowed("never_existed"));

        conn.execute(
            "DELETE FROM session_tokens WHERE session_token_hash IN ('valid', 'short_lived_and_past_its_window')",
            [],
        )
        .unwrap();
        assert!(!allowed("valid"));
        assert!(!allowed("short_lived_and_past_its_window"));
    }

    #[test]
    fn cleanup_keeps_rows_of_live_sessions_without_remember_me() {
        let dir = tempfile::tempdir().unwrap();
        let conn = open_db_at(&dir.path().join("tokens.db")).unwrap();
        conn.execute(
            "INSERT INTO tokens (token_hash, name) VALUES ('auth', 'auth')",
            [],
        )
        .unwrap();
        for (hash, remember_me, expires_at) in [
            ("short_past_its_login_window", 0, "-1 hour"),
            ("short_long_gone", 0, "-35 days"),
            ("remembered_expired", 1, "-1 hour"),
            ("remembered_valid", 1, "+1 hour"),
        ] {
            conn.execute(
                "INSERT INTO session_tokens
                 (session_token_hash, auth_token_hash, remember_me, expires_at)
                 VALUES (?1, 'auth', ?2, datetime('now', ?3))",
                rusqlite::params![hash, remember_me, expires_at],
            )
            .unwrap();
        }
        assert_eq!(cleanup_expired_sessions_on(&conn).unwrap(), 2);
        let allowed = |hash: &str| session_allows_live_connection_on(&conn, hash).unwrap();
        assert!(allowed("short_past_its_login_window"));
        assert!(allowed("remembered_valid"));
        assert!(!allowed("short_long_gone"));
        assert!(!allowed("remembered_expired"));
    }

    #[test]
    fn a_failed_login_does_not_disturb_live_sessions() {
        let dir = tempfile::tempdir().unwrap();
        let conn = open_db_at(&dir.path().join("tokens.db")).unwrap();
        conn.execute(
            "INSERT INTO tokens (token_hash, name) VALUES (?1, 'auth')",
            [hash_token("the-auth-token")],
        )
        .unwrap();
        // a login without "remember me", well past its few minutes
        let session = create_session_token_on(&conn, "the-auth-token", false).unwrap();
        conn.execute(
            "UPDATE session_tokens SET expires_at = datetime('now', '-1 hour')",
            [],
        )
        .unwrap();
        let session_hash = hash_token(&session);
        let allowed = || session_allows_live_connection_on(&conn, &session_hash).unwrap();
        assert!(allowed());

        // anyone can make bad login attempts, none may cost a user their session
        for _ in 0..3 {
            assert!(matches!(
                create_session_token_on(&conn, "guess", true),
                Err(TokenError::InvalidToken)
            ));
        }
        assert!(allowed(), "failed logins must not revoke live sessions");

        // nor do successful ones by somebody else
        create_session_token_on(&conn, "the-auth-token", true).unwrap();
        assert!(allowed());
    }

    #[test]
    fn session_status_returns_validity_and_read_only_in_one_query() {
        let dir = tempfile::tempdir().unwrap();
        let conn = open_db_at(&dir.path().join("tokens.db")).unwrap();
        conn.execute(
            "INSERT INTO tokens (token_hash, name, read_only) VALUES (?1, 'auth', 1)",
            [hash_token("the-auth-token")],
        )
        .unwrap();
        let session = create_session_token_on(&conn, "the-auth-token", true).unwrap();

        assert_eq!(
            session_token_status_on(&conn, &session).unwrap(),
            Some(true)
        );

        conn.execute("UPDATE tokens SET read_only = 0", []).unwrap();
        assert_eq!(
            session_token_status_on(&conn, &session).unwrap(),
            Some(false)
        );

        conn.execute(
            "UPDATE session_tokens SET expires_at = datetime('now', '-1 hour')",
            [],
        )
        .unwrap();
        assert_eq!(session_token_status_on(&conn, &session).unwrap(), None);
        assert_eq!(session_token_status_on(&conn, "unknown").unwrap(), None);
    }

    #[cfg(unix)]
    #[test]
    fn database_file_is_private_from_the_moment_it_is_created() {
        use nix::sys::stat::{umask, Mode};
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("tokens.db");
        // a permissive umask must not leak into the new file
        let old_umask = umask(Mode::empty());
        let created = create_private_db_file(&db_path);
        umask(old_umask);
        assert!(created.unwrap());
        let mode = std::fs::metadata(&db_path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
        // creating it a second time reports that it already existed
        assert!(!create_private_db_file(&db_path).unwrap());
    }

    #[cfg(unix)]
    #[test]
    fn existing_database_file_permissions_are_repaired() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("tokens.db");
        std::fs::write(&db_path, b"").unwrap();
        std::fs::set_permissions(&db_path, std::fs::Permissions::from_mode(0o644)).unwrap();

        assert!(!create_private_db_file(&db_path).unwrap());
        let mode = std::fs::metadata(&db_path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }
}
