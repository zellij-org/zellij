use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use zellij_utils::consts::ZELLIJ_CACHE_DIR;

const FILE_NAME: &str = "app-state.json";

pub const CLOSE_NOTICE: &str = "Closing this window keeps your sessions running.";
pub const CLOSE_NOTICE_TRAY: &str = "Find them in the tray.";

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppState {
    #[serde(default)]
    pub close_notice_shown: bool,
}

pub fn path() -> PathBuf {
    ZELLIJ_CACHE_DIR.join(FILE_NAME)
}

pub fn load_from(path: &Path) -> AppState {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

pub fn save_to(path: &Path, state: &AppState) -> Result<()> {
    let text = serde_json::to_string(state).context("failed to describe the app state")?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {:?}", parent))?;
    }
    let temporary = path.with_extension(format!("{}.tmp", std::process::id()));
    std::fs::write(&temporary, text).with_context(|| format!("failed to write {:?}", temporary))?;
    std::fs::rename(&temporary, path).with_context(|| format!("failed to move {:?}", temporary))
}

pub struct CloseNotice {
    path: Option<PathBuf>,
    state: AppState,
}

impl CloseNotice {
    pub fn at(path: Option<PathBuf>) -> Self {
        let state = path.as_deref().map(load_from).unwrap_or_default();
        CloseNotice { path, state }
    }

    pub fn take(&mut self) -> Option<&'static str> {
        if self.state.close_notice_shown {
            return None;
        }
        self.state.close_notice_shown = true;
        if let Some(path) = &self.path {
            if let Err(e) = save_to(path, &self.state) {
                report!(
                    "the app could not remember that it explained closing: {:#}",
                    e
                );
            }
        }
        Some(CLOSE_NOTICE)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_close_notice_is_given_once_and_remembered_across_launches() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join(FILE_NAME);
        let mut notice = CloseNotice::at(Some(path.clone()));
        assert_eq!(notice.take(), Some(CLOSE_NOTICE));
        assert_eq!(notice.take(), None);
        let mut relaunched = CloseNotice::at(Some(path.clone()));
        assert_eq!(relaunched.take(), None);
        assert!(load_from(&path).close_notice_shown);
    }

    #[test]
    fn an_unreadable_state_file_starts_fresh() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join(FILE_NAME);
        std::fs::write(&path, "not json").unwrap();
        assert_eq!(load_from(&path), AppState::default());
    }
}
