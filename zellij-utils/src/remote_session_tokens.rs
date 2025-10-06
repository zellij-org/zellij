use crate::consts::ZELLIJ_PROJ_DIR;
use crate::shared::set_permissions;
use rusqlite::Connection;
use std::path::PathBuf;

#[derive(Debug)]
pub enum TokenError {
    Database(rusqlite::Error),
    Io(std::io::Error),
    InvalidPath,
}

impl std::fmt::Display for TokenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TokenError::Database(e) => write!(f, "Database error: {}", e),
            TokenError::Io(e) => write!(f, "IO error: {}", e),
            TokenError::InvalidPath => write!(f, "Invalid path"),
        }
    }
}

impl std::error::Error for TokenError {}

impl From<rusqlite::Error> for TokenError {
    fn from(error: rusqlite::Error) -> Self {
        TokenError::Database(error)
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
    std::fs::create_dir_all(data_dir)?;
    let db_path = data_dir.join("remote_sessions.db");
    Ok(db_path)
}

fn init_db(conn: &Connection) -> Result<()> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS remote_sessions (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            server_url TEXT UNIQUE NOT NULL,
            session_token TEXT NOT NULL,
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
            last_used_at DATETIME DEFAULT CURRENT_TIMESTAMP
        )",
        [],
    )?;

    Ok(())
}

/// Save session token for a server (upsert)
pub fn save_session_token(server_url: &str, session_token: &str) -> Result<()> {
    let db_path = get_db_path()?;

    // Set file permissions to 0600 if creating new file
    let is_new = !db_path.exists();

    let conn = Connection::open(&db_path)?;
    init_db(&conn)?;

    if is_new {
        set_permissions(&db_path, 0o600)?;
    }

    conn.execute(
        "INSERT OR REPLACE INTO remote_sessions (server_url, session_token, last_used_at)
         VALUES (?1, ?2, CURRENT_TIMESTAMP)",
        [server_url, session_token],
    )?;

    Ok(())
}

/// Get session token for a server, update last_used_at
pub fn get_session_token(server_url: &str) -> Result<Option<String>> {
    let db_path = get_db_path()?;

    if !db_path.exists() {
        return Ok(None);
    }

    let conn = Connection::open(db_path)?;
    init_db(&conn)?;

    let token = match conn.query_row(
        "SELECT session_token FROM remote_sessions WHERE server_url = ?1",
        [server_url],
        |row| row.get::<_, String>(0),
    ) {
        Ok(token) => Some(token),
        Err(rusqlite::Error::QueryReturnedNoRows) => None,
        Err(e) => return Err(TokenError::Database(e)),
    };

    if token.is_some() {
        // Update last_used_at
        conn.execute(
            "UPDATE remote_sessions SET last_used_at = CURRENT_TIMESTAMP WHERE server_url = ?1",
            [server_url],
        )?;
    }

    Ok(token)
}

/// Delete session token for a server
pub fn delete_session_token(server_url: &str) -> Result<bool> {
    let db_path = get_db_path()?;

    if !db_path.exists() {
        return Ok(false);
    }

    let conn = Connection::open(db_path)?;
    init_db(&conn)?;

    let rows_affected = conn.execute(
        "DELETE FROM remote_sessions WHERE server_url = ?1",
        [server_url],
    )?;

    Ok(rows_affected > 0)
}
