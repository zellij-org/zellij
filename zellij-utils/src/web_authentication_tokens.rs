// TODO: GATE THIS WHOLE FILE AND RELEVANT DEPS BEHIND web_server_capability
use rusqlite::Connection;
use std::path::PathBuf;
use uuid::Uuid;
use sha2::{Digest, Sha256};
use crate::consts::ZELLIJ_PROJ_DIR;

#[derive(Debug)]
pub enum TokenError {
    Database(rusqlite::Error),
    Io(std::io::Error),
    InvalidPath,
    DuplicateName(String),
}

impl std::fmt::Display for TokenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TokenError::Database(e) => write!(f, "Database error: {}", e),
            TokenError::Io(e) => write!(f, "IO error: {}", e),
            TokenError::InvalidPath => write!(f, "Invalid path"),
            TokenError::DuplicateName(name) => write!(f, "Token name '{}' already exists", name),
        }
    }
}

impl std::error::Error for TokenError {}

impl From<rusqlite::Error> for TokenError {
    fn from(error: rusqlite::Error) -> Self {
        match error {
            rusqlite::Error::SqliteFailure(ffi_error, _) 
                if ffi_error.code == rusqlite::ErrorCode::ConstraintViolation => {
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
    std::fs::create_dir_all(data_dir)?;
    
    Ok(data_dir.join("tokens.db"))
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
    Ok(())
}

pub fn create_token(name: Option<&str>) -> Result<String> {
    let db_path = get_db_path()?;
    let conn = Connection::open(db_path)?;
    init_db(&conn)?;
    
    let token = Uuid::new_v4().to_string();
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    let token_hash = format!("{:x}", hasher.finalize());
    
    let token_name = if let Some(n) = name {
        n.to_string()
    } else {
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM tokens",
            [],
            |row| row.get(0)
        )?;
        format!("token_{}", count + 1)
    };
    
    match conn.execute(
        "INSERT INTO tokens (token_hash, name) VALUES (?1, ?2)",
        [&token_hash, &token_name],
    ) {
        Err(rusqlite::Error::SqliteFailure(ffi_error, _)) 
            if ffi_error.code == rusqlite::ErrorCode::ConstraintViolation => {
            Err(TokenError::DuplicateName(token_name))
        },
        Err(e) => Err(TokenError::Database(e)),
        Ok(_) => Ok(token),
    }
}

pub fn revoke_token(token: &str) -> Result<bool> {
    let db_path = get_db_path()?;
    let conn = Connection::open(db_path)?;
    init_db(&conn)?;
    
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    let token_hash = format!("{:x}", hasher.finalize());
    
    let rows_affected = conn.execute(
        "DELETE FROM tokens WHERE token_hash = ?1",
        [&token_hash],
    )?;
    Ok(rows_affected > 0)
}

pub fn list_tokens() -> Result<Vec<String>> {
    let db_path = get_db_path()?;
    let conn = Connection::open(db_path)?;
    init_db(&conn)?;
    
    let mut stmt = conn.prepare("SELECT name FROM tokens ORDER BY created_at")?;
    let rows = stmt.query_map([], |row| {
        Ok(row.get::<_, String>(0)?)
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
    
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    let token_hash = format!("{:x}", hasher.finalize());
    
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM tokens WHERE token_hash = ?1",
        [&token_hash],
        |row| row.get(0)
    )?;
    Ok(count > 0)
}
