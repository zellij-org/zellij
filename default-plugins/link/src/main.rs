use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use zellij_tile::prelude::*;

const FILE_PATH_REGEX: &str = r#"(?:(?:\./|\.\./|/)[^\s:"'`\)\]\}>]+|[a-zA-Z0-9_][a-zA-Z0-9_.\-]*/[^\s:"'`\)\]\}>]+)(?::\d+(?::\d+)?)?"#;

const CWD_CONTEXT_KEY: &str = "cwd";

#[derive(Default)]
struct State {
    known_terminal_panes: HashSet<PaneId>,
    /// Tracks the current CWD for each terminal pane.
    pane_cwds: HashMap<PaneId, PathBuf>,
    /// Tracks the directory entry names highlighted for each pane,
    /// so they can be removed when the CWD changes.
    pane_dir_entries: HashMap<PaneId, Vec<String>>,
}

register_plugin!(State);

impl ZellijPlugin for State {
    fn load(&mut self, _configuration: BTreeMap<String, String>) {
        subscribe(&[
            EventType::PaneUpdate,
            EventType::HighlightClicked,
            EventType::CwdChanged,
        ]);
        // Set host folder to "/" so that /host maps to the real filesystem root,
        // allowing std::fs operations on /host/<absolute_path>.
        change_host_folder(PathBuf::from("/"));
    }

    fn update(&mut self, event: Event) -> bool {
        match event {
            Event::PaneUpdate(pane_manifest) => {
                self.handle_pane_update(pane_manifest);
            },
            Event::HighlightClicked {
                pane_id: _,
                pattern: _,
                matched_string,
                context,
            } => {
                self.handle_highlight_clicked(matched_string, context);
            },
            Event::CwdChanged(pane_id, new_cwd, _focused_client_ids) => {
                self.handle_cwd_changed(pane_id, new_cwd);
            },
            _ => {},
        }
        false // never render — background-only plugin
    }

    fn render(&mut self, _rows: usize, _cols: usize) {
        // Background-only plugin. Never rendered. Intentionally empty.
    }
}

impl State {
    fn handle_pane_update(&mut self, pane_manifest: PaneManifest) {
        let mut current_panes: HashSet<PaneId> = HashSet::new();

        for (_tab_index, panes) in &pane_manifest.panes {
            for pane_info in panes {
                if !pane_info.is_plugin {
                    let pane_id = PaneId::Terminal(pane_info.id);
                    current_panes.insert(pane_id);
                }
            }
        }

        // Set highlights on newly appeared terminal panes
        for &pane_id in &current_panes {
            if !self.known_terminal_panes.contains(&pane_id) {
                // Fetch the pane's current CWD and scan its directory
                if let Ok(cwd) = get_pane_cwd(pane_id) {
                    self.scan_and_store_dir_entries(pane_id, &cwd);
                    self.pane_cwds.insert(pane_id, cwd);
                }
                self.set_all_highlights_for_pane(pane_id);
            }
        }

        // Clear highlights on panes that no longer exist
        for &pane_id in &self.known_terminal_panes {
            if !current_panes.contains(&pane_id) {
                clear_pane_highlights(pane_id); // TODO: why are we doing this if the pane is
                                                // closed?
                self.pane_cwds.remove(&pane_id);
                self.pane_dir_entries.remove(&pane_id);
            }
        }

        self.known_terminal_panes = current_panes;
    }

    fn handle_cwd_changed(&mut self, pane_id: PaneId, new_cwd: PathBuf) {
        let old_cwd = self.pane_cwds.get(&pane_id);
        if old_cwd == Some(&new_cwd) {
            return;
        }

        self.pane_cwds.insert(pane_id, new_cwd.clone());
        self.scan_and_store_dir_entries(pane_id, &new_cwd);

        // clear_pane_highlights removes all highlights, then re-set everything
        // (file-path regex + directory entry patterns)
        clear_pane_highlights(pane_id);
        self.set_all_highlights_for_pane(pane_id);
    }

    fn handle_highlight_clicked(&self, matched_string: String, context: BTreeMap<String, String>) {
        let (path_str, line_number) = parse_path_and_line(&matched_string);
        let path_str = path_str.trim();

        // Resolve to a fully qualified path: if relative, join with the
        // pane CWD stored in the highlight context.
        let absolute_path = if path_str.starts_with('/') {
            PathBuf::from(path_str)
        } else if let Some(cwd) = context.get(CWD_CONTEXT_KEY) {
            PathBuf::from(cwd).join(path_str)
        } else {
            PathBuf::from(path_str)
        };

        let mut file_to_open = FileToOpen::new(&absolute_path);
        if let Some(line) = line_number {
            file_to_open = file_to_open.with_line_number(line);
        }

        open_file_floating(file_to_open, None, BTreeMap::new());
    }

    /// (Re-)set all regex highlights for a pane: the general file-path regex
    /// plus any directory-entry patterns derived from the pane's CWD.
    fn set_all_highlights_for_pane(&self, pane_id: PaneId) {
        let mut highlights = Vec::new();

        // Build context map containing the pane CWD (echoed back on click)
        let context = self.cwd_context_for_pane(pane_id);

        // General file-path regex (always present)
        highlights.push(RegexHighlight {
            pattern: FILE_PATH_REGEX.to_owned(),
            style: HighlightStyle::None,
            context: context.clone(),
            on_hover: true,
            bold: false,
            italic: true,
            underline: true,
        });

        // Directory-entry patterns for the pane's current CWD
        if let Some(entries) = self.pane_dir_entries.get(&pane_id) {
            for entry_name in entries {
                let pattern = regex_escape(entry_name);
                highlights.push(RegexHighlight {
                    pattern,
                    style: HighlightStyle::None,
                    context: context.clone(),
                    on_hover: true,
                    bold: false,
                    italic: true,
                    underline: true,
                });
            }
        }

        set_pane_regex_highlights(pane_id, highlights);
    }

    fn scan_and_store_dir_entries(&mut self, pane_id: PaneId, cwd: &Path) {
        let host_path = Path::new("/host").join(cwd.strip_prefix("/").unwrap_or(cwd));
        let dir_entries = scan_directory(&host_path);
        self.pane_dir_entries.insert(pane_id, dir_entries);
    }

    fn cwd_context_for_pane(&self, pane_id: PaneId) -> BTreeMap<String, String> {
        let mut context = BTreeMap::new();
        if let Some(cwd) = self.pane_cwds.get(&pane_id) {
            context.insert(CWD_CONTEXT_KEY.to_owned(), cwd.display().to_string());
        }
        context
    }
}

/// Scan a directory for first-level file and folder names.
/// Returns an empty vec on any error.
fn scan_directory(path: &Path) -> Vec<String> {
    let mut entries = Vec::new();
    let read_dir = match std::fs::read_dir(path) {
        Ok(rd) => rd,
        Err(_) => return entries,
    };
    for entry in read_dir {
        if let Ok(entry) = entry {
            if let Some(name) = entry.file_name().to_str() {
                entries.push(name.to_owned());
            }
        }
    }
    entries
}

/// Escape a string so it is treated as a literal in a regex pattern.
fn regex_escape(s: &str) -> String {
    let mut escaped = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        match c {
            '\\' | '.' | '+' | '*' | '?' | '(' | ')' | '|' | '[' | ']' | '{' | '}' | '^' | '$' => {
                escaped.push('\\');
                escaped.push(c);
            },
            _ => escaped.push(c),
        }
    }
    escaped
}

fn parse_path_and_line(matched_string: &str) -> (&str, Option<usize>) {
    let mut end = matched_string.len();
    let mut numeric_segments: Vec<(usize, &str)> = Vec::new();

    loop {
        if end == 0 {
            break;
        }
        let search_region = &matched_string[..end];
        if let Some(colon_pos) = search_region.rfind(':') {
            let segment = &matched_string[colon_pos + 1..end];
            if !segment.is_empty() && segment.chars().all(|c| c.is_ascii_digit()) {
                numeric_segments.push((colon_pos, segment));
                end = colon_pos;
            } else {
                break;
            }
        } else {
            break;
        }
    }

    numeric_segments.reverse();

    match numeric_segments.len() {
        0 => (matched_string, None),
        1 => {
            let (colon_pos, line_str) = numeric_segments[0];
            let path = &matched_string[..colon_pos];
            (path, line_str.parse::<usize>().ok())
        },
        _ => {
            let (colon_pos, line_str) = numeric_segments[0];
            let path = &matched_string[..colon_pos];
            (path, line_str.parse::<usize>().ok())
        },
    }
}
