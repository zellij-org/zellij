use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use zellij_utils::cli::WindowArgs;
use zellij_utils::consts::ZELLIJ_CACHE_DIR;
use zellij_utils::input::window::StartupMode;

const FILE_NAME: &str = "window-state.json";
const SMALLEST: usize = 2;
const LARGEST: usize = 1000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Shown {
    Windowed,
    Maximized,
    Fullscreen,
}

impl Shown {
    pub fn mode(self) -> StartupMode {
        match self {
            Shown::Windowed => StartupMode::Windowed,
            Shown::Maximized => StartupMode::Maximized,
            Shown::Fullscreen => StartupMode::Fullscreen,
        }
    }

    pub fn of(mode: StartupMode) -> Self {
        match mode {
            StartupMode::Maximized => Shown::Maximized,
            StartupMode::Fullscreen => Shown::Fullscreen,
            StartupMode::Windowed | StartupMode::Remember => Shown::Windowed,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowState {
    pub cols: usize,
    pub rows: usize,
    pub state: Shown,
}

impl WindowState {
    pub fn is_sane(&self) -> bool {
        sane(self.cols) && sane(self.rows)
    }
}

fn sane(cells: usize) -> bool {
    (SMALLEST..=LARGEST).contains(&cells)
}

pub fn path() -> PathBuf {
    ZELLIJ_CACHE_DIR.join(FILE_NAME)
}

pub fn load_from(path: &Path) -> Option<WindowState> {
    let text = std::fs::read_to_string(path).ok()?;
    let state: WindowState = serde_json::from_str(&text).ok()?;
    state.is_sane().then_some(state)
}

pub fn save_to(path: &Path, state: WindowState) -> Result<()> {
    let text = serde_json::to_string(&state).context("failed to describe the window state")?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {:?}", parent))?;
    }
    let file_name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| FILE_NAME.to_owned());
    let temporary = path.with_file_name(format!("{}.{}.tmp", file_name, std::process::id()));
    let written = std::fs::write(&temporary, text)
        .with_context(|| format!("failed to write {:?}", temporary))
        .and_then(|()| {
            std::fs::rename(&temporary, path)
                .with_context(|| format!("failed to move {:?} into place", temporary))
        });
    if written.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    written
}

pub fn store(path: &Path, state: WindowState) {
    if let Err(e) = save_to(path, state) {
        report!("the window size could not be remembered: {:#}", e);
    }
}

pub fn to_remember(state: WindowState, windowed_known: bool, path: &Path) -> WindowState {
    if windowed_known {
        return state;
    }
    match load_from(path) {
        Some(previous) => WindowState {
            cols: previous.cols,
            rows: previous.rows,
            ..state
        },
        None => state,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Startup {
    pub rows: usize,
    pub cols: usize,
    pub mode: StartupMode,
    #[cfg_attr(not(test), allow(dead_code))]
    pub restored: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct InitialCells {
    pub rows: Option<usize>,
    pub cols: Option<usize>,
}

impl Startup {
    pub fn resolve(
        mode: StartupMode,
        saved: Option<WindowState>,
        rows: Option<usize>,
        cols: Option<usize>,
    ) -> Self {
        Self::resolve_with(mode, saved, rows, cols, InitialCells::default())
    }

    pub fn resolve_with(
        mode: StartupMode,
        saved: Option<WindowState>,
        rows: Option<usize>,
        cols: Option<usize>,
        initial: InitialCells,
    ) -> Self {
        let saved = match mode {
            StartupMode::Remember => saved.filter(WindowState::is_sane),
            _ => None,
        };
        let initial = InitialCells {
            rows: initial.rows.filter(|rows| sane(*rows)),
            cols: initial.cols.filter(|cols| sane(*cols)),
        };
        Startup {
            rows: rows
                .or(saved.map(|saved| saved.rows))
                .or(initial.rows)
                .unwrap_or(WindowArgs::DEFAULT_ROWS),
            cols: cols
                .or(saved.map(|saved| saved.cols))
                .or(initial.cols)
                .unwrap_or(WindowArgs::DEFAULT_COLS),
            mode: match mode {
                StartupMode::Remember => saved
                    .map(|saved| saved.state.mode())
                    .unwrap_or(StartupMode::Windowed),
                other => other,
            },
            restored: saved.is_some(),
        }
    }

    #[cfg(test)]
    pub fn windowed(rows: usize, cols: usize) -> Self {
        Startup {
            rows,
            cols,
            mode: StartupMode::Windowed,
            restored: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn state(cols: usize, rows: usize, shown: Shown) -> WindowState {
        WindowState {
            cols,
            rows,
            state: shown,
        }
    }

    #[test]
    fn a_saved_state_loads_back_unchanged() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join(FILE_NAME);
        for saved in [
            state(90, 30, Shown::Windowed),
            state(200, 60, Shown::Maximized),
            state(2, 1000, Shown::Fullscreen),
        ] {
            save_to(&path, saved).unwrap();
            assert_eq!(load_from(&path), Some(saved));
        }
    }

    #[test]
    fn the_file_is_plain_json_naming_the_state() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join(FILE_NAME);
        save_to(&path, state(90, 30, Shown::Maximized)).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        let value: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(value["cols"], 90);
        assert_eq!(value["rows"], 30);
        assert_eq!(value["state"], "maximized");
    }

    #[test]
    fn a_missing_empty_corrupt_or_out_of_range_file_loads_nothing() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join(FILE_NAME);
        assert_eq!(load_from(&path), None, "missing");
        for text in [
            "",
            "{",
            "not json",
            "{\"cols\": 90, \"rows\": 30}",
            "{\"cols\": 90, \"rows\": 30, \"state\": \"minimized\"}",
            "{\"cols\": -1, \"rows\": 30, \"state\": \"windowed\"}",
            "{\"cols\": 1, \"rows\": 30, \"state\": \"windowed\"}",
            "{\"cols\": 90, \"rows\": 0, \"state\": \"windowed\"}",
            "{\"cols\": 1001, \"rows\": 30, \"state\": \"windowed\"}",
            "{\"cols\": 90, \"rows\": 5000, \"state\": \"fullscreen\"}",
        ] {
            std::fs::write(&path, text).unwrap();
            assert_eq!(load_from(&path), None, "{:?}", text);
        }
    }

    #[test]
    fn a_save_leaves_no_temporary_file_behind() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("nested").join(FILE_NAME);
        save_to(&path, state(90, 30, Shown::Windowed)).unwrap();
        save_to(&path, state(91, 31, Shown::Windowed)).unwrap();
        let names: Vec<_> = std::fs::read_dir(path.parent().unwrap())
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, vec![FILE_NAME.to_owned()]);
        assert_eq!(load_from(&path), Some(state(91, 31, Shown::Windowed)));
    }

    #[test]
    fn a_failed_save_is_reported_rather_than_panicking() {
        let dir = TempDir::new().unwrap();
        let blocker = dir.path().join("file");
        std::fs::write(&blocker, "").unwrap();
        let path = blocker.join(FILE_NAME);
        assert!(save_to(&path, state(90, 30, Shown::Windowed)).is_err());
        store(&path, state(90, 30, Shown::Windowed));
    }

    #[test]
    fn remember_uses_the_saved_cells_and_state() {
        let startup = Startup::resolve(
            StartupMode::Remember,
            Some(state(90, 30, Shown::Windowed)),
            None,
            None,
        );
        assert_eq!((startup.cols, startup.rows), (90, 30));
        assert_eq!(startup.mode, StartupMode::Windowed);
        assert!(startup.restored);

        let maximized = Startup::resolve(
            StartupMode::Remember,
            Some(state(90, 30, Shown::Maximized)),
            None,
            None,
        );
        assert_eq!(maximized.mode, StartupMode::Maximized);
        assert_eq!((maximized.cols, maximized.rows), (90, 30));
    }

    #[test]
    fn remember_with_nothing_saved_is_windowed_at_the_defaults() {
        assert_eq!(
            Startup::resolve(StartupMode::Remember, None, None, None),
            Startup::resolve(StartupMode::Windowed, None, None, None)
        );
        assert_eq!(
            Startup::resolve(StartupMode::Remember, None, None, None),
            Startup {
                rows: 40,
                cols: 120,
                mode: StartupMode::Windowed,
                restored: false,
            }
        );
        assert_eq!(
            Startup::resolve(
                StartupMode::Remember,
                Some(state(1, 30, Shown::Maximized)),
                None,
                None
            ),
            Startup::resolve(StartupMode::Windowed, None, None, None),
            "an insane saved state counts as nothing saved"
        );
    }

    #[test]
    fn the_other_modes_ignore_the_saved_state() {
        let saved = Some(state(90, 30, Shown::Fullscreen));
        for mode in [
            StartupMode::Windowed,
            StartupMode::Maximized,
            StartupMode::Fullscreen,
        ] {
            let startup = Startup::resolve(mode, saved, None, None);
            assert_eq!((startup.cols, startup.rows), (120, 40), "{:?}", mode);
            assert_eq!(startup.mode, mode);
            assert!(!startup.restored);
        }
    }

    #[test]
    fn explicit_rows_and_cols_win_over_the_saved_ones() {
        let saved = Some(state(90, 30, Shown::Maximized));
        let startup = Startup::resolve(StartupMode::Remember, saved, Some(24), Some(80));
        assert_eq!((startup.cols, startup.rows), (80, 24));
        assert_eq!(startup.mode, StartupMode::Maximized);
        let rows_only = Startup::resolve(StartupMode::Remember, saved, Some(24), None);
        assert_eq!((rows_only.cols, rows_only.rows), (90, 24));
    }

    #[test]
    fn initial_cells_apply_only_when_nothing_better_is_known() {
        let initial = InitialCells {
            rows: Some(30),
            cols: Some(100),
        };
        let fresh = Startup::resolve_with(StartupMode::Remember, None, None, None, initial);
        assert_eq!((fresh.cols, fresh.rows), (100, 30));
        assert!(!fresh.restored);

        let saved = Some(state(90, 25, Shown::Windowed));
        let remembered = Startup::resolve_with(StartupMode::Remember, saved, None, None, initial);
        assert_eq!((remembered.cols, remembered.rows), (90, 25));

        let insane = Some(state(1, 25, Shown::Windowed));
        let fallen_back = Startup::resolve_with(StartupMode::Remember, insane, None, None, initial);
        assert_eq!((fallen_back.cols, fallen_back.rows), (100, 30));

        let explicit = Startup::resolve_with(StartupMode::Remember, saved, Some(24), None, initial);
        assert_eq!((explicit.cols, explicit.rows), (90, 24));

        let maximized = Startup::resolve_with(StartupMode::Maximized, saved, None, None, initial);
        assert_eq!((maximized.cols, maximized.rows), (100, 30));
        assert_eq!(maximized.mode, StartupMode::Maximized);
    }
}
