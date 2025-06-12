// TODO: GATE THIS WHOLE FILE AND RELEVANT DEPS BEHIND web_server_capability
use crate::consts::ZELLIJ_PROJ_DIR;
use rusqlite::Connection;
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

#[derive(Debug)]
pub struct TokenInfo {
    pub name: String,
    pub created_at: String,
}

#[derive(Debug)]
pub enum TokenError {
    Database(rusqlite::Error),
    Io(std::io::Error),
    InvalidPath,
    DuplicateName(String),
    TokenNotFound(String),
    InvalidToken,
}

impl std::fmt::Display for TokenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TokenError::Database(e) => write!(f, "Database error: {}", e),
            TokenError::Io(e) => write!(f, "IO error: {}", e),
            TokenError::InvalidPath => write!(f, "Invalid path"),
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
    if cfg!(debug_assertions) {
        // tests db
        let data_dir = ZELLIJ_PROJ_DIR.data_dir();
        std::fs::create_dir_all(&data_dir)?;
        Ok(data_dir.join("tokens_for_dev.db"))
    } else {
        // prod db
        let data_dir = ZELLIJ_PROJ_DIR.data_dir();
        std::fs::create_dir_all(data_dir)?;
        Ok(data_dir.join("tokens.db"))
    }
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

    Ok(())
}

fn hash_token(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    format!("{:x}", hasher.finalize())
}

pub fn create_token(name: Option<String>) -> Result<(String, String)> {
    let db_path = get_db_path()?;
    let conn = Connection::open(db_path)?;
    init_db(&conn)?;

    let token = Uuid::new_v4().to_string();
    let token_hash = hash_token(&token);

    let token_name = if let Some(n) = name {
        n.to_string()
    } else {
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM tokens", [], |row| row.get(0))?;
        format!("token_{}", count + 1)
    };

    match conn.execute(
        "INSERT INTO tokens (token_hash, name) VALUES (?1, ?2)",
        [&token_hash, &token_name],
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
    let db_path = get_db_path()?;
    let conn = Connection::open(db_path)?;
    init_db(&conn)?;

    cleanup_expired_sessions()?;

    let auth_token_hash = hash_token(auth_token);

    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM tokens WHERE token_hash = ?1",
        [&auth_token_hash],
        |row| row.get(0),
    )?;

    if count == 0 {
        return Err(TokenError::InvalidToken);
    }

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

pub fn validate_session_token(session_token: &str) -> Result<bool> {
    let db_path = get_db_path()?;
    let conn = Connection::open(db_path)?;
    init_db(&conn)?;

    let session_token_hash = hash_token(session_token);

    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM session_tokens WHERE session_token_hash = ?1 AND expires_at > datetime('now')",
        [&session_token_hash],
        |row| row.get(0),
    )?;

    Ok(count > 0)
}

pub fn cleanup_expired_sessions() -> Result<usize> {
    let db_path = get_db_path()?;
    let conn = Connection::open(db_path)?;
    init_db(&conn)?;

    let rows_affected = conn.execute(
        "DELETE FROM session_tokens WHERE expires_at <= datetime('now')",
        [],
    )?;

    Ok(rows_affected)
}

pub fn revoke_session_token(session_token: &str) -> Result<bool> {
    let db_path = get_db_path()?;
    let conn = Connection::open(db_path)?;
    init_db(&conn)?;

    let session_token_hash = hash_token(session_token);
    let rows_affected = conn.execute(
        "DELETE FROM session_tokens WHERE session_token_hash = ?1",
        [&session_token_hash],
    )?;

    Ok(rows_affected > 0)
}

pub fn revoke_sessions_for_auth_token(auth_token: &str) -> Result<usize> {
    let db_path = get_db_path()?;
    let conn = Connection::open(db_path)?;
    init_db(&conn)?;

    let auth_token_hash = hash_token(auth_token);
    let rows_affected = conn.execute(
        "DELETE FROM session_tokens WHERE auth_token_hash = ?1",
        [&auth_token_hash],
    )?;

    Ok(rows_affected)
}

pub fn revoke_token(name: &str) -> Result<bool> {
    let db_path = get_db_path()?;
    let conn = Connection::open(db_path)?;
    init_db(&conn)?;

    let token_hash = match conn.query_row(
        "SELECT token_hash FROM tokens WHERE name = ?1",
        [&name],
        |row| row.get::<_, String>(0),
    ) {
        Ok(hash) => Some(hash),
        Err(rusqlite::Error::QueryReturnedNoRows) => None,
        Err(e) => return Err(TokenError::Database(e)),
    };

    if let Some(token_hash) = token_hash {
        conn.execute(
            "DELETE FROM session_tokens WHERE auth_token_hash = ?1",
            [&token_hash],
        )?;
    }

    let rows_affected = conn.execute("DELETE FROM tokens WHERE name = ?1", [&name])?;
    Ok(rows_affected > 0)
}

pub fn revoke_all_tokens() -> Result<usize> {
    let db_path = get_db_path()?;
    let conn = Connection::open(db_path)?;
    init_db(&conn)?;

    conn.execute("DELETE FROM session_tokens", [])?;
    let rows_affected = conn.execute("DELETE FROM tokens", [])?;
    Ok(rows_affected)
}

pub fn rename_token(old_name: &str, new_name: &str) -> Result<()> {
    let db_path = get_db_path()?;
    let conn = Connection::open(db_path)?;
    init_db(&conn)?;

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
    let db_path = get_db_path()?;
    let conn = Connection::open(db_path)?;
    init_db(&conn)?;

    let mut stmt = conn.prepare("SELECT name, created_at FROM tokens ORDER BY created_at")?;
    let rows = stmt.query_map([], |row| {
        Ok(TokenInfo {
            name: row.get::<_, String>(0)?,
            created_at: row.get::<_, String>(1)?,
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
        std::fs::remove_file(db_path)?;
    }
    Ok(())
}

pub fn validate_token(token: &str) -> Result<bool> {
    let db_path = get_db_path()?;
    let conn = Connection::open(db_path)?;
    init_db(&conn)?;

    let token_hash = hash_token(token);

    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM tokens WHERE token_hash = ?1",
        [&token_hash],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}
