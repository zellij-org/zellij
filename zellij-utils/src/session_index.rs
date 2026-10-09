use crate::consts::{ZELLIJ_SESSION_INFO_CACHE_DIR, ZELLIJ_SOCK_DIR};
use crate::input::session_suggestions::{
    MatchRule, ThenOrder, FACT_DIRECTORY, FACT_GIT_BRANCH, FACT_GIT_REPO, FACT_GIT_WORKTREE,
    FACT_HOSTNAME,
};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub const SESSION_INDEX_FILE_NAME: &str = "session_index.sqlite";
pub const SESSION_LAYOUT_FILE_NAME: &str = "session-layout.kdl";
const BUSY_TIMEOUT: Duration = Duration::from_millis(2000);
const RECENTLY_CREATED_GRACE_SECS: u64 = 60;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionContext {
    pub folder: Option<PathBuf>,
    pub repo: Option<String>,
    pub worktree: Option<String>,
    pub branch: Option<String>,
    pub facts: BTreeMap<String, String>,
}

impl SessionContext {
    pub fn fact(&self, name: &str) -> Option<String> {
        match name {
            FACT_DIRECTORY => self.folder.as_ref().map(|f| folder_key(f)),
            FACT_GIT_REPO => self.repo.clone(),
            FACT_GIT_BRANCH => self.branch.clone(),
            FACT_GIT_WORKTREE => self.worktree.clone(),
            other => self.facts.get(other).cloned(),
        }
    }
    pub fn fact_matches(&self, candidate: &SessionContext, name: &str) -> bool {
        match name {
            FACT_DIRECTORY => match (&self.folder, &candidate.folder) {
                (Some(target), Some(candidate)) => folder_key(candidate) == folder_key(target),
                _ => false,
            },
            _ => match (self.fact(name), candidate.fact(name)) {
                (Some(target), Some(candidate)) => target == candidate,
                _ => false,
            },
        }
    }
    pub fn normalized(&self) -> SessionContext {
        let mut normalized = self.clone();
        normalized.folder = self.folder.as_ref().map(|f| PathBuf::from(folder_key(f)));
        normalized
    }
    pub fn matches_rule(&self, candidate: &SessionContext, rule: &MatchRule) -> bool {
        !rule.facts.is_empty() && rule.facts.iter().all(|f| self.fact_matches(candidate, f))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionRow {
    pub name: String,
    pub context: SessionContext,
    pub created_at: u64,
    pub last_used_at: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionState {
    Running,
    Resumable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub row: SessionRow,
    pub state: SessionState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RankedCandidate {
    pub candidate: Candidate,
    pub tier: Option<usize>,
    pub matching_facts: Vec<String>,
    pub script_label: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GitInfo {
    pub toplevel: String,
    pub common_dir: String,
    pub branch: Option<String>,
}

#[derive(Debug)]
pub enum SessionIndexError {
    Database(rusqlite::Error),
    Io(std::io::Error),
}

impl std::fmt::Display for SessionIndexError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SessionIndexError::Database(e) => write!(f, "session index database error: {}", e),
            SessionIndexError::Io(e) => write!(f, "session index io error: {}", e),
        }
    }
}

impl std::error::Error for SessionIndexError {}

impl From<rusqlite::Error> for SessionIndexError {
    fn from(e: rusqlite::Error) -> Self {
        SessionIndexError::Database(e)
    }
}

impl From<std::io::Error> for SessionIndexError {
    fn from(e: std::io::Error) -> Self {
        SessionIndexError::Io(e)
    }
}

pub type Result<T> = std::result::Result<T, SessionIndexError>;

pub fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub fn folder_key(path: &Path) -> String {
    let mut key = path.display().to_string();
    if !key.ends_with('/') {
        key.push('/');
    }
    key
}

fn folder_prefix_upper_bound(prefix: &str) -> String {
    let mut upper = prefix.to_owned();
    upper.pop();
    upper.push('0');
    upper
}

pub fn default_index_path() -> PathBuf {
    ZELLIJ_SESSION_INFO_CACHE_DIR
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| ZELLIJ_SESSION_INFO_CACHE_DIR.clone())
        .join(SESSION_INDEX_FILE_NAME)
}

pub fn hostname() -> Option<String> {
    #[cfg(unix)]
    {
        let mut buffer = [0u8; 256];
        let result =
            unsafe { libc::gethostname(buffer.as_mut_ptr() as *mut libc::c_char, buffer.len()) };
        if result != 0 {
            return None;
        }
        let end = buffer.iter().position(|b| *b == 0).unwrap_or(buffer.len());
        let name = String::from_utf8_lossy(&buffer[..end]).trim().to_owned();
        if name.is_empty() {
            None
        } else {
            Some(name)
        }
    }
    #[cfg(not(unix))]
    {
        std::env::var("COMPUTERNAME").ok().filter(|h| !h.is_empty())
    }
}

pub fn parse_git_rev_parse(cwd: &Path, stdout: &str) -> Option<GitInfo> {
    let mut lines = stdout.lines().map(|l| l.trim()).filter(|l| !l.is_empty());
    let toplevel = lines.next()?.to_owned();
    let common_dir = lines.next()?;
    let branch = lines.next().map(|b| b.to_owned());
    let common_dir_path = PathBuf::from(common_dir);
    let absolute = if common_dir_path.is_absolute() {
        common_dir_path
    } else {
        cwd.join(common_dir_path)
    };
    let canonical = std::fs::canonicalize(&absolute).unwrap_or(absolute);
    let branch = branch.filter(|b| b != "HEAD");
    Some(GitInfo {
        toplevel,
        common_dir: canonical.display().to_string(),
        branch,
    })
}

pub fn parse_ranking_script_output(stdout: &str) -> Vec<(String, Option<String>)> {
    let mut seen = HashSet::new();
    stdout
        .lines()
        .filter_map(|line| {
            let line = line.trim_end_matches('\r');
            if line.trim().is_empty() {
                return None;
            }
            let (name, label) = match line.split_once('\t') {
                Some((name, label)) => {
                    let label = label.trim();
                    (
                        name.trim().to_owned(),
                        if label.is_empty() {
                            None
                        } else {
                            Some(label.to_owned())
                        },
                    )
                },
                None => (line.trim().to_owned(), None),
            };
            if name.is_empty() || !seen.insert(name.clone()) {
                None
            } else {
                Some((name, label))
            }
        })
        .collect()
}

pub fn parse_fact_lines(stdout: &str) -> BTreeMap<String, String> {
    stdout
        .lines()
        .filter_map(|line| {
            let (key, value) = line.split_once('\t').or_else(|| line.split_once('='))?;
            let key = key.trim();
            let value = value.trim();
            if key.is_empty() || value.is_empty() {
                None
            } else {
                Some((key.to_owned(), value.to_owned()))
            }
        })
        .collect()
}

fn compare_within_tier(a: &Candidate, b: &Candidate) -> std::cmp::Ordering {
    let a_running = a.state == SessionState::Running;
    let b_running = b.state == SessionState::Running;
    b_running
        .cmp(&a_running)
        .then(b.row.last_used_at.cmp(&a.row.last_used_at))
        .then(a.row.name.cmp(&b.row.name))
}

pub fn matching_facts(
    target: &SessionContext,
    candidate: &SessionContext,
    facts: &[String],
) -> Vec<String> {
    facts
        .iter()
        .filter(|f| target.fact_matches(candidate, f))
        .cloned()
        .collect()
}

pub fn rank_candidates(
    target: &SessionContext,
    rules: &[MatchRule],
    then: ThenOrder,
    candidates: Vec<Candidate>,
    column_facts: &[String],
) -> Vec<RankedCandidate> {
    let mut tiers: Vec<Vec<Candidate>> = vec![vec![]; rules.len()];
    let mut rest = vec![];
    for candidate in candidates {
        match rules
            .iter()
            .position(|rule| target.matches_rule(&candidate.row.context, rule))
        {
            Some(tier) => tiers[tier].push(candidate),
            None => rest.push(candidate),
        }
    }
    let mut ranked = vec![];
    for (tier, mut members) in tiers.into_iter().enumerate() {
        members.sort_by(compare_within_tier);
        for candidate in members {
            let matching = matching_facts(target, &candidate.row.context, column_facts);
            ranked.push(RankedCandidate {
                candidate,
                tier: Some(tier),
                matching_facts: matching,
                script_label: None,
            });
        }
    }
    match then {
        ThenOrder::Recent => rest.sort_by(compare_within_tier),
        ThenOrder::Alphabetical => rest.sort_by(|a, b| a.row.name.cmp(&b.row.name)),
        ThenOrder::None => rest.clear(),
    }
    for candidate in rest {
        let matching = matching_facts(target, &candidate.row.context, column_facts);
        ranked.push(RankedCandidate {
            candidate,
            tier: None,
            matching_facts: matching,
            script_label: None,
        });
    }
    ranked
}

pub fn apply_script_order(
    ranked: Vec<RankedCandidate>,
    script_output: &[(String, Option<String>)],
    hide_unlisted: bool,
) -> Vec<RankedCandidate> {
    let mut remaining = ranked;
    let mut ordered = vec![];
    for (name, label) in script_output {
        if let Some(position) = remaining.iter().position(|r| &r.candidate.row.name == name) {
            let mut item = remaining.remove(position);
            item.script_label = label.clone();
            ordered.push(item);
        }
    }
    if !hide_unlisted {
        ordered.extend(remaining);
    }
    ordered
}

pub struct SessionIndex {
    conn: Connection,
    sock_dir: PathBuf,
    cache_dir: PathBuf,
    indexed_facts: HashSet<String>,
}

fn sanitize_identifier(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect()
}

fn json_path_for_fact(name: &str) -> String {
    format!("$.\"{}\"", name.replace('"', ""))
}

impl SessionIndex {
    pub fn open_default() -> Result<Self> {
        Self::open(
            &default_index_path(),
            ZELLIJ_SOCK_DIR.clone(),
            ZELLIJ_SESSION_INFO_CACHE_DIR.clone(),
        )
    }

    pub fn open(path: &Path, sock_dir: PathBuf, cache_dir: PathBuf) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let existed = path.exists();
        let conn = Connection::open(path)?;
        conn.busy_timeout(BUSY_TIMEOUT)?;
        let _: String = conn.query_row("PRAGMA journal_mode=WAL", [], |row| row.get(0))?;
        let integrity: String = conn.query_row("PRAGMA quick_check", [], |row| row.get(0))?;
        if integrity != "ok" {
            return Err(SessionIndexError::Database(rusqlite::Error::InvalidQuery));
        }
        let had_table: bool = conn
            .query_row(
                "SELECT 1 FROM sqlite_master WHERE type='table' AND name='sessions'",
                [],
                |_| Ok(true),
            )
            .optional()?
            .unwrap_or(false);
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS sessions (
                name TEXT PRIMARY KEY,
                folder TEXT,
                repo TEXT,
                worktree TEXT,
                branch TEXT,
                facts TEXT NOT NULL DEFAULT '{}',
                created_at INTEGER NOT NULL,
                last_used_at INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS sessions_folder ON sessions(folder);
            CREATE INDEX IF NOT EXISTS sessions_repo_branch ON sessions(repo, branch);
            CREATE INDEX IF NOT EXISTS sessions_last_used_at ON sessions(last_used_at);",
        )?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
        }
        let mut index = SessionIndex {
            conn,
            sock_dir,
            cache_dir,
            indexed_facts: HashSet::new(),
        };
        if !existed || !had_table {
            index.backfill_from_saved_sessions();
        }
        Ok(index)
    }

    pub fn ensure_fact_index(&mut self, fact: &str) -> Result<()> {
        if self.indexed_facts.contains(fact) {
            return Ok(());
        }
        let statement = format!(
            "CREATE INDEX IF NOT EXISTS sessions_fact_{} ON sessions(json_extract(facts, '{}'))",
            sanitize_identifier(fact),
            json_path_for_fact(fact)
        );
        self.conn.execute_batch(&statement)?;
        self.indexed_facts.insert(fact.to_owned());
        Ok(())
    }

    fn backfill_from_saved_sessions(&mut self) {
        let Ok(entries) = std::fs::read_dir(&self.cache_dir) else {
            return;
        };
        let mut rows = vec![];
        for entry in entries.flatten() {
            let path = entry.path();
            let layout_file = path.join(SESSION_LAYOUT_FILE_NAME);
            let Ok(metadata) = std::fs::metadata(&layout_file) else {
                continue;
            };
            let Some(name) = path.file_name().map(|n| n.to_string_lossy().to_string()) else {
                continue;
            };
            let modified = metadata
                .modified()
                .ok()
                .and_then(|m| m.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let created = metadata
                .created()
                .ok()
                .and_then(|m| m.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(modified);
            let folder = std::fs::read_to_string(&layout_file)
                .ok()
                .and_then(|raw| layout_cwd(&raw));
            rows.push(SessionRow {
                name,
                context: SessionContext {
                    folder,
                    ..Default::default()
                },
                created_at: created,
                last_used_at: modified,
            });
        }
        let Ok(transaction) = self.conn.transaction() else {
            return;
        };
        for row in rows {
            let _ = transaction.execute(
                "INSERT OR IGNORE INTO sessions (name, folder, repo, worktree, branch, facts, created_at, last_used_at)
                 VALUES (?1, ?2, NULL, NULL, NULL, '{}', ?3, ?4)",
                params![
                    row.name,
                    row.context.folder.as_ref().map(|f| folder_key(f)),
                    row.created_at as i64,
                    row.last_used_at as i64
                ],
            );
        }
        let _ = transaction.commit();
    }

    fn row_from_sql(row: &rusqlite::Row) -> rusqlite::Result<SessionRow> {
        let name: String = row.get(0)?;
        let folder: Option<String> = row.get(1)?;
        let repo: Option<String> = row.get(2)?;
        let worktree: Option<String> = row.get(3)?;
        let branch: Option<String> = row.get(4)?;
        let facts: String = row.get(5)?;
        let created_at: i64 = row.get(6)?;
        let last_used_at: i64 = row.get(7)?;
        Ok(SessionRow {
            name,
            context: SessionContext {
                folder: folder.map(PathBuf::from),
                repo,
                worktree,
                branch,
                facts: serde_json::from_str(&facts).unwrap_or_default(),
            },
            created_at: created_at.max(0) as u64,
            last_used_at: last_used_at.max(0) as u64,
        })
    }

    const COLUMNS: &'static str =
        "name, folder, repo, worktree, branch, facts, created_at, last_used_at";

    pub fn get(&self, name: &str) -> Result<Option<SessionRow>> {
        let sql = format!("SELECT {} FROM sessions WHERE name = ?1", Self::COLUMNS);
        Ok(self
            .conn
            .query_row(&sql, params![name], Self::row_from_sql)
            .optional()?)
    }

    pub fn write_context(&self, name: &str, context: &SessionContext, now: u64) -> Result<bool> {
        let context = &context.normalized();
        let existing = self.get(name)?;
        let folder = context.folder.as_ref().map(|f| folder_key(f));
        let facts = serde_json::to_string(&context.facts).unwrap_or_else(|_| "{}".to_owned());
        match existing {
            Some(row) if &row.context == context => Ok(false),
            Some(_) => {
                self.conn.execute(
                    "UPDATE sessions SET folder = ?2, repo = ?3, worktree = ?4, branch = ?5, facts = ?6 WHERE name = ?1",
                    params![name, folder, context.repo, context.worktree, context.branch, facts],
                )?;
                Ok(true)
            },
            None => {
                self.conn.execute(
                    "INSERT INTO sessions (name, folder, repo, worktree, branch, facts, created_at, last_used_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)",
                    params![
                        name,
                        folder,
                        context.repo,
                        context.worktree,
                        context.branch,
                        facts,
                        now as i64
                    ],
                )?;
                Ok(true)
            },
        }
    }

    pub fn ensure_row(&self, name: &str, now: u64) -> Result<bool> {
        let inserted = self.conn.execute(
            "INSERT OR IGNORE INTO sessions (name, facts, created_at, last_used_at) VALUES (?1, '{}', ?2, ?2)",
            params![name, now as i64],
        )?;
        Ok(inserted > 0)
    }

    pub fn touch(&self, name: &str, now: u64) -> Result<bool> {
        let changed = self.conn.execute(
            "UPDATE sessions SET last_used_at = ?2 WHERE name = ?1 AND last_used_at <> ?2",
            params![name, now as i64],
        )?;
        Ok(changed > 0)
    }

    pub fn rename(&self, old_name: &str, new_name: &str) -> Result<bool> {
        self.conn
            .execute("DELETE FROM sessions WHERE name = ?1", params![new_name])?;
        let changed = self.conn.execute(
            "UPDATE sessions SET name = ?2 WHERE name = ?1",
            params![old_name, new_name],
        )?;
        Ok(changed > 0)
    }

    pub fn delete(&self, name: &str) -> Result<bool> {
        let changed = self
            .conn
            .execute("DELETE FROM sessions WHERE name = ?1", params![name])?;
        Ok(changed > 0)
    }

    pub fn session_state(&self, name: &str) -> Option<SessionState> {
        if self.sock_dir.join(name).exists() {
            Some(SessionState::Running)
        } else if self
            .cache_dir
            .join(name)
            .join(SESSION_LAYOUT_FILE_NAME)
            .exists()
        {
            Some(SessionState::Resumable)
        } else {
            None
        }
    }

    fn query_rows(&self, sql: &str, params: &[&dyn rusqlite::ToSql]) -> Result<Vec<SessionRow>> {
        let mut statement = self.conn.prepare_cached(sql)?;
        let rows = statement
            .query_map(params, Self::row_from_sql)?
            .filter_map(|r| r.ok())
            .collect();
        Ok(rows)
    }

    pub fn rows_in_folder(&self, folder: &Path) -> Result<Vec<SessionRow>> {
        let prefix = folder_key(folder);
        let upper = folder_prefix_upper_bound(&prefix);
        let sql = format!(
            "SELECT {} FROM sessions WHERE folder >= ?1 AND folder < ?2",
            Self::COLUMNS
        );
        self.query_rows(&sql, &[&prefix, &upper])
    }

    pub fn rows_exactly_in_folder(&self, folder: &Path) -> Result<Vec<SessionRow>> {
        let key = folder_key(folder);
        let sql = format!("SELECT {} FROM sessions WHERE folder = ?1", Self::COLUMNS);
        self.query_rows(&sql, &[&key])
    }

    pub fn rows_in_repo(&self, repo: &str, branch: Option<&str>) -> Result<Vec<SessionRow>> {
        match branch {
            Some(branch) => {
                let sql = format!(
                    "SELECT {} FROM sessions WHERE repo = ?1 AND branch = ?2",
                    Self::COLUMNS
                );
                self.query_rows(&sql, &[&repo, &branch])
            },
            None => {
                let sql = format!("SELECT {} FROM sessions WHERE repo = ?1", Self::COLUMNS);
                self.query_rows(&sql, &[&repo])
            },
        }
    }

    pub fn rows_with_column(&self, column: &str, value: &str) -> Result<Vec<SessionRow>> {
        let column = match column {
            "worktree" => "worktree",
            "branch" => "branch",
            _ => return Ok(vec![]),
        };
        let sql = format!(
            "SELECT {} FROM sessions WHERE {} = ?1",
            Self::COLUMNS,
            column
        );
        self.query_rows(&sql, &[&value])
    }

    pub fn rows_with_fact(&mut self, fact: &str, value: &str) -> Result<Vec<SessionRow>> {
        self.ensure_fact_index(fact)?;
        let sql = format!(
            "SELECT {} FROM sessions WHERE json_extract(facts, '{}') = ?1",
            Self::COLUMNS,
            json_path_for_fact(fact)
        );
        self.query_rows(&sql, &[&value])
    }

    pub fn recent_rows(&self, limit: usize, offset: usize) -> Result<Vec<SessionRow>> {
        let sql = format!(
            "SELECT {} FROM sessions ORDER BY last_used_at DESC, name ASC LIMIT ?1 OFFSET ?2",
            Self::COLUMNS
        );
        self.query_rows(&sql, &[&(limit as i64), &(offset as i64)])
    }

    pub fn alphabetical_rows(&self, limit: usize) -> Result<Vec<SessionRow>> {
        let sql = format!(
            "SELECT {} FROM sessions ORDER BY name ASC LIMIT ?1",
            Self::COLUMNS
        );
        self.query_rows(&sql, &[&(limit as i64)])
    }

    pub fn rows_for_names(&self, names: &[String]) -> Result<Vec<SessionRow>> {
        let mut rows = vec![];
        for name in names {
            if let Some(row) = self.get(name)? {
                rows.push(row);
            }
        }
        Ok(rows)
    }

    pub fn rows_matching_rule(
        &mut self,
        target: &SessionContext,
        rule: &MatchRule,
    ) -> Result<Vec<SessionRow>> {
        let has = |fact: &str| rule.facts.iter().any(|f| f == fact);
        let rows = if has(FACT_DIRECTORY) {
            match &target.folder {
                Some(folder) => self.rows_in_folder(folder)?,
                None => vec![],
            }
        } else if has(FACT_GIT_REPO) {
            match &target.repo {
                Some(repo) => {
                    let branch = if has(FACT_GIT_BRANCH) {
                        target.branch.as_deref()
                    } else {
                        None
                    };
                    if has(FACT_GIT_BRANCH) && branch.is_none() {
                        vec![]
                    } else {
                        self.rows_in_repo(repo, branch)?
                    }
                },
                None => vec![],
            }
        } else if has(FACT_GIT_WORKTREE) {
            match &target.worktree {
                Some(worktree) => self.rows_with_column("worktree", worktree)?,
                None => vec![],
            }
        } else if has(FACT_GIT_BRANCH) {
            match &target.branch {
                Some(branch) => self.rows_with_column("branch", branch)?,
                None => vec![],
            }
        } else {
            let fact = rule
                .facts
                .first()
                .cloned()
                .unwrap_or_else(|| FACT_HOSTNAME.to_owned());
            match target.fact(&fact) {
                Some(value) => self.rows_with_fact(&fact, &value)?,
                None => vec![],
            }
        };
        Ok(rows
            .into_iter()
            .filter(|row| target.matches_rule(&row.context, rule))
            .collect())
    }

    pub fn resolve_candidates(
        &self,
        rows: Vec<SessionRow>,
        exclude: &str,
        now: u64,
    ) -> Vec<Candidate> {
        let mut seen = HashSet::new();
        let mut candidates = vec![];
        for row in rows {
            if row.name == exclude || !seen.insert(row.name.clone()) {
                continue;
            }
            match self.session_state(&row.name) {
                Some(state) => candidates.push(Candidate { row, state }),
                None => {
                    if now.saturating_sub(row.created_at) > RECENTLY_CREATED_GRACE_SECS {
                        let _ = self.delete(&row.name);
                    }
                },
            }
        }
        candidates
    }

    pub fn data_version(&self) -> Option<i64> {
        self.conn
            .query_row("PRAGMA data_version", [], |row| row.get(0))
            .ok()
    }

    pub fn running_session_names(&self) -> Vec<String> {
        running_session_names_in(&self.sock_dir)
    }

    pub fn matching_candidates(
        &mut self,
        target: &SessionContext,
        rules: &[MatchRule],
        exclude: &str,
        now: u64,
    ) -> Result<Vec<Candidate>> {
        let mut rows = vec![];
        for rule in rules {
            rows.extend(self.rows_matching_rule(target, rule)?);
        }
        Ok(self.resolve_candidates(rows, exclude, now))
    }

    pub fn suggestion_candidates(
        &mut self,
        target: &SessionContext,
        rules: &[MatchRule],
        then: ThenOrder,
        exclude: &str,
        limit: usize,
        now: u64,
    ) -> Result<Vec<Candidate>> {
        let mut rows = vec![];
        for rule in rules {
            rows.extend(self.rows_matching_rule(target, rule)?);
        }
        let running = self.running_session_names();
        let known: HashSet<String> = rows.iter().map(|r| r.name.clone()).collect();
        let running_rows = self.rows_for_names(
            &running
                .iter()
                .filter(|n| !known.contains(*n))
                .cloned()
                .collect::<Vec<_>>(),
        )?;
        let running_with_rows: HashSet<String> =
            running_rows.iter().map(|r| r.name.clone()).collect();
        rows.extend(running_rows);
        for name in &running {
            if !known.contains(name) && !running_with_rows.contains(name) {
                rows.push(SessionRow {
                    name: name.clone(),
                    context: SessionContext::default(),
                    created_at: now,
                    last_used_at: 0,
                });
            }
        }
        let extra = limit.saturating_sub(rows.len().min(limit));
        if extra > 0 {
            match then {
                ThenOrder::Recent => rows.extend(self.recent_rows(extra + 1, 0)?),
                ThenOrder::Alphabetical => rows.extend(self.alphabetical_rows(extra + 1)?),
                ThenOrder::None => {},
            }
        }
        let mut candidates = vec![];
        let mut seen = HashSet::new();
        for row in rows {
            if row.name == exclude || !seen.insert(row.name.clone()) {
                continue;
            }
            if running.contains(&row.name) {
                candidates.push(Candidate {
                    row,
                    state: SessionState::Running,
                });
            } else {
                candidates.extend(self.resolve_candidates(vec![row], exclude, now));
            }
        }
        Ok(candidates)
    }
}

pub fn running_session_names_in(sock_dir: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(sock_dir) else {
        return vec![];
    };
    entries
        .flatten()
        .filter(|entry| {
            entry
                .file_type()
                .map(|ft| crate::consts::is_ipc_socket(&ft) || ft.is_file())
                .unwrap_or(false)
        })
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| name != "web_server_bus" && !name.ends_with("-reply"))
        .collect()
}

pub fn layout_cwd(raw_layout: &str) -> Option<PathBuf> {
    let document: kdl::KdlDocument = raw_layout.parse().ok()?;
    let layout = document.get("layout")?;
    let cwd = layout
        .children()?
        .get("cwd")?
        .entries()
        .first()?
        .value()
        .as_string()?
        .to_owned();
    Some(PathBuf::from(cwd))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::session_suggestions::default_rules;

    struct TestIndex {
        index: SessionIndex,
        root: tempfile::TempDir,
    }

    impl TestIndex {
        fn new() -> Self {
            let root = tempfile::tempdir().unwrap();
            let sock_dir = root.path().join("sock");
            let cache_dir = root.path().join("cache");
            std::fs::create_dir_all(&sock_dir).unwrap();
            std::fs::create_dir_all(&cache_dir).unwrap();
            let index =
                SessionIndex::open(&root.path().join("index.sqlite"), sock_dir, cache_dir).unwrap();
            TestIndex { index, root }
        }
        fn mark_running(&self, name: &str) {
            std::fs::write(self.root.path().join("sock").join(name), b"").unwrap();
        }
        fn mark_resumable(&self, name: &str) {
            let folder = self.root.path().join("cache").join(name);
            std::fs::create_dir_all(&folder).unwrap();
            std::fs::write(folder.join(SESSION_LAYOUT_FILE_NAME), b"layout {\n}\n").unwrap();
        }
        fn add(&self, name: &str, context: SessionContext, last_used_at: u64) {
            self.index.write_context(name, &context, 1000).unwrap();
            self.index.touch(name, last_used_at).unwrap();
        }
    }

    fn context(folder: &str, repo: Option<&str>, branch: Option<&str>) -> SessionContext {
        SessionContext {
            folder: Some(PathBuf::from(folder)),
            repo: repo.map(|r| r.to_owned()),
            branch: branch.map(|b| b.to_owned()),
            ..Default::default()
        }
    }

    #[test]
    fn folder_prefix_matching_includes_subfolders_only() {
        let test = TestIndex::new();
        test.add("same", context("/work/api", None, None), 10);
        test.add("sub", context("/work/api/src", None, None), 10);
        test.add("sibling", context("/work/api-old", None, None), 10);
        test.add("parent", context("/work", None, None), 10);
        let mut names: Vec<String> = test
            .index
            .rows_in_folder(Path::new("/work/api"))
            .unwrap()
            .into_iter()
            .map(|r| r.name)
            .collect();
        names.sort();
        assert_eq!(names, vec!["same".to_owned(), "sub".to_owned()]);
        let exact: Vec<String> = test
            .index
            .rows_exactly_in_folder(Path::new("/work/api/"))
            .unwrap()
            .into_iter()
            .map(|r| r.name)
            .collect();
        assert_eq!(exact, vec!["same".to_owned()]);
    }

    #[test]
    fn rows_are_written_only_when_changed() {
        let test = TestIndex::new();
        let ctx = context("/a", Some("repo"), Some("main"));
        assert!(test.index.write_context("s", &ctx, 5).unwrap());
        assert!(!test.index.write_context("s", &ctx, 6).unwrap());
        assert!(test.index.touch("s", 7).unwrap());
        assert!(!test.index.touch("s", 7).unwrap());
        let row = test.index.get("s").unwrap().unwrap();
        assert_eq!(row.created_at, 5);
        assert_eq!(row.last_used_at, 7);
        assert_eq!(row.context.folder, Some(PathBuf::from("/a/")));
    }

    #[test]
    fn rename_and_delete_update_the_index() {
        let test = TestIndex::new();
        test.add("old", context("/a", None, None), 1);
        assert!(test.index.rename("old", "new").unwrap());
        assert!(test.index.get("old").unwrap().is_none());
        assert!(test.index.get("new").unwrap().is_some());
        assert!(test.index.delete("new").unwrap());
        assert!(test.index.get("new").unwrap().is_none());
    }

    #[test]
    fn candidates_without_socket_or_layout_are_removed_lazily() {
        let test = TestIndex::new();
        test.index
            .write_context("gone", &context("/a", None, None), 1)
            .unwrap();
        test.add("alive", context("/a", None, None), 5);
        test.mark_resumable("alive");
        let rows = test.index.rows_in_folder(Path::new("/a")).unwrap();
        let candidates = test.index.resolve_candidates(rows, "me", 100_000);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].row.name, "alive");
        assert_eq!(candidates[0].state, SessionState::Resumable);
        assert!(test.index.get("gone").unwrap().is_none());
    }

    #[test]
    fn tiers_follow_the_default_rule_order() {
        let mut test = TestIndex::new();
        let target = SessionContext {
            folder: Some(PathBuf::from("/repo/app")),
            repo: Some("/repo/.git".to_owned()),
            branch: Some("main".to_owned()),
            ..Default::default()
        };
        test.add(
            "folder-branch",
            context("/repo/app", Some("/repo/.git"), Some("main")),
            1,
        );
        test.add(
            "repo-branch",
            context("/repo/lib", Some("/repo/.git"), Some("main")),
            9,
        );
        test.add(
            "folder-other-branch",
            context("/repo/app", Some("/repo/.git"), Some("dev")),
            8,
        );
        test.add(
            "repo-only",
            context("/repo/docs", Some("/repo/.git"), Some("dev")),
            7,
        );
        test.add("unrelated", context("/elsewhere", None, None), 100);
        for name in [
            "folder-branch",
            "repo-branch",
            "folder-other-branch",
            "repo-only",
            "unrelated",
        ] {
            test.mark_resumable(name);
        }
        let rules = default_rules();
        let candidates = test
            .index
            .suggestion_candidates(&target, &rules, ThenOrder::Recent, "me", 100, 1000)
            .unwrap();
        let ranked = rank_candidates(
            &target,
            &rules,
            ThenOrder::Recent,
            candidates,
            &["directory".to_owned(), "git_branch".to_owned()],
        );
        let order: Vec<(String, Option<usize>)> = ranked
            .iter()
            .map(|r| (r.candidate.row.name.clone(), r.tier))
            .collect();
        assert_eq!(
            order,
            vec![
                ("folder-branch".to_owned(), Some(0)),
                ("repo-branch".to_owned(), Some(1)),
                ("folder-other-branch".to_owned(), Some(2)),
                ("repo-only".to_owned(), Some(3)),
                ("unrelated".to_owned(), None),
            ]
        );
        assert_eq!(
            ranked[0].matching_facts,
            vec!["directory".to_owned(), "git_branch".to_owned()]
        );
        assert_eq!(ranked[2].matching_facts, vec!["directory".to_owned()]);
    }

    #[test]
    fn a_subfolder_or_a_sibling_with_a_common_prefix_is_not_a_folder_match() {
        let target = context("/p", None, None);
        assert!(target.fact_matches(&context("/p", None, None), FACT_DIRECTORY));
        assert!(target.fact_matches(&context("/p/", None, None), FACT_DIRECTORY));
        assert!(!target.fact_matches(&context("/p/x", None, None), FACT_DIRECTORY));
        assert!(!target.fact_matches(&context("/pq", None, None), FACT_DIRECTORY));
        assert!(!target.fact_matches(&context("/", None, None), FACT_DIRECTORY));
        let mut test = TestIndex::new();
        test.add("same", context("/p", None, None), 1);
        test.add("below", context("/p/x", None, None), 2);
        test.mark_running("same");
        test.mark_running("below");
        let rules = default_rules();
        let candidates = test
            .index
            .matching_candidates(&target, &rules, "me", 1000)
            .unwrap();
        let names: Vec<String> = candidates.iter().map(|c| c.row.name.clone()).collect();
        assert_eq!(names, vec!["same".to_owned()]);
    }

    #[test]
    fn running_sessions_come_first_within_a_tier() {
        let mut test = TestIndex::new();
        let target = context("/p", None, None);
        test.add("older-running", context("/p", None, None), 1);
        test.add("newer-resumable", context("/p", None, None), 50);
        test.add("newest-resumable", context("/p", None, None), 60);
        test.mark_running("older-running");
        test.mark_resumable("newer-resumable");
        test.mark_resumable("newest-resumable");
        let rules = default_rules();
        let candidates = test
            .index
            .matching_candidates(&target, &rules, "me", 1000)
            .unwrap();
        let ranked = rank_candidates(&target, &rules, ThenOrder::None, candidates, &[]);
        let names: Vec<String> = ranked
            .iter()
            .map(|r| r.candidate.row.name.clone())
            .collect();
        assert_eq!(
            names,
            vec![
                "older-running".to_owned(),
                "newest-resumable".to_owned(),
                "newer-resumable".to_owned()
            ]
        );
    }

    #[test]
    fn sessions_without_a_folder_or_a_branch_are_still_suggested() {
        let mut test = TestIndex::new();
        let target = context("/p", Some("/p/.git"), Some("main"));
        test.mark_running("never-recorded");
        test.add(
            "no-folder",
            SessionContext {
                branch: Some("dev".to_owned()),
                ..Default::default()
            },
            30,
        );
        test.mark_running("no-folder");
        test.add("no-branch", context("/elsewhere", None, None), 20);
        test.mark_running("no-branch");
        test.add("no-context", SessionContext::default(), 10);
        test.mark_resumable("no-context");
        let rules = default_rules();
        let candidates = test
            .index
            .suggestion_candidates(&target, &rules, ThenOrder::Recent, "me", 10, 1000)
            .unwrap();
        let ranked = rank_candidates(&target, &rules, ThenOrder::Recent, candidates, &[]);
        let found: BTreeMap<String, (SessionState, SessionContext)> = ranked
            .into_iter()
            .map(|r| {
                (
                    r.candidate.row.name.clone(),
                    (r.candidate.state, r.candidate.row.context),
                )
            })
            .collect();
        assert_eq!(found.len(), 4, "{:?}", found.keys());
        let never_recorded = &found["never-recorded"];
        assert_eq!(never_recorded.0, SessionState::Running);
        assert_eq!(never_recorded.1, SessionContext::default());
        let no_folder = &found["no-folder"];
        assert_eq!(no_folder.0, SessionState::Running);
        assert_eq!(no_folder.1.folder, None);
        assert_eq!(no_folder.1.branch.as_deref(), Some("dev"));
        let no_branch = &found["no-branch"];
        assert_eq!(no_branch.0, SessionState::Running);
        assert_eq!(no_branch.1.branch, None);
        let no_context = &found["no-context"];
        assert_eq!(no_context.0, SessionState::Resumable);
        assert_eq!(no_context.1.folder, None);
        assert_eq!(no_context.1.branch, None);
    }

    #[test]
    fn outside_git_only_folder_tiers_apply() {
        let target = context("/p", None, None);
        let candidate = context("/p", None, None);
        let rules = default_rules();
        assert!(!target.matches_rule(&candidate, &rules[0]));
        assert!(!target.matches_rule(&candidate, &rules[1]));
        assert!(target.matches_rule(&candidate, &rules[2]));
        assert!(!target.matches_rule(&candidate, &rules[3]));
    }

    #[test]
    fn custom_fact_rules_use_the_fact_index() {
        let mut test = TestIndex::new();
        let mut facts = BTreeMap::new();
        facts.insert("project".to_owned(), "zellij".to_owned());
        let ctx = SessionContext {
            facts: facts.clone(),
            ..Default::default()
        };
        test.add("proj", ctx.clone(), 1);
        test.add("other", SessionContext::default(), 1);
        let rule = MatchRule {
            facts: vec!["project".to_owned()],
        };
        let rows = test.index.rows_matching_rule(&ctx, &rule).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].name, "proj");
    }

    #[test]
    fn backfill_reads_existing_saved_sessions_once() {
        let root = tempfile::tempdir().unwrap();
        let cache_dir = root.path().join("cache");
        let saved = cache_dir.join("saved");
        std::fs::create_dir_all(&saved).unwrap();
        std::fs::write(
            saved.join(SESSION_LAYOUT_FILE_NAME),
            "layout {\n    cwd \"/home/me/project\"\n    tab {\n        pane\n    }\n}\n",
        )
        .unwrap();
        let index = SessionIndex::open(
            &root.path().join("index.sqlite"),
            root.path().join("sock"),
            cache_dir.clone(),
        )
        .unwrap();
        let row = index.get("saved").unwrap().unwrap();
        assert_eq!(row.context.folder, Some(PathBuf::from("/home/me/project/")));
        drop(index);
        let later = cache_dir.join("later");
        std::fs::create_dir_all(&later).unwrap();
        std::fs::write(later.join(SESSION_LAYOUT_FILE_NAME), "layout {\n}\n").unwrap();
        let index = SessionIndex::open(
            &root.path().join("index.sqlite"),
            root.path().join("sock"),
            cache_dir,
        )
        .unwrap();
        assert!(index.get("later").unwrap().is_none());
    }

    #[test]
    fn git_rev_parse_output_is_parsed() {
        let dir = tempfile::tempdir().unwrap();
        let git_dir = dir.path().join(".git");
        std::fs::create_dir_all(&git_dir).unwrap();
        let stdout = format!("{}\n.git\nfeature/x\n", dir.path().display());
        let info = parse_git_rev_parse(dir.path(), &stdout).unwrap();
        assert_eq!(info.toplevel, dir.path().display().to_string());
        assert_eq!(
            info.common_dir,
            std::fs::canonicalize(&git_dir)
                .unwrap()
                .display()
                .to_string()
        );
        assert_eq!(info.branch, Some("feature/x".to_owned()));
        let detached = format!("{}\n{}\nHEAD\n", dir.path().display(), git_dir.display());
        assert_eq!(
            parse_git_rev_parse(dir.path(), &detached).unwrap().branch,
            None
        );
        assert_eq!(parse_git_rev_parse(dir.path(), ""), None);
        assert_eq!(parse_git_rev_parse(dir.path(), "/only/one/line\n"), None);
    }

    #[test]
    fn ranking_script_output_is_parsed() {
        let parsed = parse_ranking_script_output("b\tbest match\na\n\nb\tduplicate\nc\t \n");
        assert_eq!(
            parsed,
            vec![
                ("b".to_owned(), Some("best match".to_owned())),
                ("a".to_owned(), None),
                ("c".to_owned(), None),
            ]
        );
        assert!(parse_ranking_script_output("").is_empty());
    }

    fn ranked(name: &str) -> RankedCandidate {
        RankedCandidate {
            candidate: Candidate {
                row: SessionRow {
                    name: name.to_owned(),
                    context: SessionContext::default(),
                    created_at: 0,
                    last_used_at: 0,
                },
                state: SessionState::Running,
            },
            tier: None,
            matching_facts: vec![],
            script_label: None,
        }
    }

    #[test]
    fn script_order_puts_listed_sessions_first() {
        let builtin = vec![ranked("a"), ranked("b"), ranked("c")];
        let output = vec![
            ("c".to_owned(), Some("first".to_owned())),
            ("unknown".to_owned(), None),
            ("a".to_owned(), None),
        ];
        let ordered = apply_script_order(builtin.clone(), &output, false);
        let names: Vec<&str> = ordered
            .iter()
            .map(|r| r.candidate.row.name.as_str())
            .collect();
        assert_eq!(names, vec!["c", "a", "b"]);
        assert_eq!(ordered[0].script_label, Some("first".to_owned()));
        let hidden = apply_script_order(builtin, &output, true);
        assert_eq!(hidden.len(), 2);
    }

    #[test]
    fn fact_lines_are_parsed() {
        let facts = parse_fact_lines("project\tzellij\nteam=core\nbroken\nempty=\n");
        assert_eq!(facts.get("project"), Some(&"zellij".to_owned()));
        assert_eq!(facts.get("team"), Some(&"core".to_owned()));
        assert_eq!(facts.len(), 2);
    }

    #[test]
    fn layout_cwd_is_read_from_the_layout_node() {
        assert_eq!(
            layout_cwd("layout {\n cwd \"/x/y\"\n}\n"),
            Some(PathBuf::from("/x/y"))
        );
        assert_eq!(layout_cwd("layout {\n}\n"), None);
        assert_eq!(layout_cwd("not kdl {"), None);
    }
}
