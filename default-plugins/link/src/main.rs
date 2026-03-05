use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use zellij_tile::prelude::*;

const FILE_PATH_REGEX: &str = r#"(?:(?:\./|\.\./|/)[A-Za-z0-9_./\-+@%,#=~!\$\{\}\[\]]+|~/[A-Za-z0-9_./\-+@%,#=~!\$\{\}\[\]]+|\$\{?[A-Za-z_][A-Za-z0-9_]*\}?/[A-Za-z0-9_./\-+@%,#=~!\$\{\}\[\]]+|[a-zA-Z0-9_][a-zA-Z0-9_.\-]*/[A-Za-z0-9_./\-+@%,#=~!\$\{\}\[\]]+)(?::\d+(?::\d+)?)?"#;

const CWD_CONTEXT_KEY: &str = "cwd";

#[derive(Default)]
struct State {
    known_terminal_panes: HashSet<PaneId>,
    /// Tracks the current CWD for each terminal pane.
    pane_cwds: HashMap<PaneId, PathBuf>,
    /// Tracks the directory entry names highlighted for each pane,
    /// so they can be removed when the CWD changes.
    pane_dir_entries: HashMap<PaneId, Vec<String>>,
    /// Session environment variables, fetched once on load.
    /// Used for `~` and `$VAR` expansion in clicked paths.
    env_vars: BTreeMap<String, String>,
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
        self.env_vars = get_session_environment_variables();
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
        let expanded = expand_path(path_str, &self.env_vars);
        let path_str = expanded.as_str();

        // Resolve to a fully qualified path: if relative, join with the
        // pane CWD stored in the highlight context.
        let absolute_path = if path_str.starts_with('/') {
            PathBuf::from(path_str)
        } else if let Some(cwd) = context.get(CWD_CONTEXT_KEY) {
            PathBuf::from(cwd).join(path_str)
        } else {
            PathBuf::from(path_str)
        };

        // Validate the path exists via the /host/ filesystem mapping
        // established at load time. This guards against regex false
        // positives that match non-path text in terminal output.
        let host_path = Path::new("/host").join(
            absolute_path.strip_prefix("/").unwrap_or(&absolute_path),
        );
        let metadata = match std::fs::metadata(&host_path) {
            Ok(m) => m,
            Err(_) => return, // path does not exist — silently ignore
        };

        if metadata.is_dir() {
            let mut args = BTreeMap::new();
            let mut configuration = BTreeMap::new();
            args.insert("open_directly".to_owned(), "true".to_owned());
            configuration.insert("caller_cwd".to_owned(), absolute_path.display().to_string());
            pipe_message_to_plugin(
                MessageToPlugin::new("filepicker")
                    .with_plugin_url("filepicker")
                    .new_plugin_instance_should_have_pane_title(
                        &format!("Browse: {}", absolute_path.display()),
                    )
                    .new_plugin_instance_should_be_focused()
                    .new_plugin_instance_should_have_cwd(absolute_path)
                    .with_args(args)
                    .with_plugin_config(configuration),
            );
        } else {
            let mut file_to_open = FileToOpen::new(&absolute_path);
            if let Some(line) = line_number {
                file_to_open = file_to_open.with_line_number(line);
            }
            open_file_floating(file_to_open, None, BTreeMap::new());
        }
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

/// Expand `~` and `$VAR` / `${VAR}` references in a path string.
///
/// - `~` at the start (followed by `/` or at end-of-string) is replaced with
///   the value of `HOME` from `env_vars`.
/// - `$VARNAME` and `${VARNAME}` anywhere in the string are replaced with the
///   corresponding value from `env_vars`.
/// - Unrecognized variables and a missing `HOME` are left as-is.
fn expand_path(path: &str, env_vars: &BTreeMap<String, String>) -> String {
    // Step 1: tilde expansion (only leading ~)
    let after_tilde = if path == "~" {
        match env_vars.get("HOME") {
            Some(home) => home.clone(),
            None => path.to_owned(),
        }
    } else if let Some(rest) = path.strip_prefix("~/") {
        match env_vars.get("HOME") {
            Some(home) => format!("{}/{}", home, rest),
            None => path.to_owned(),
        }
    } else {
        path.to_owned()
    };

    // Step 2: environment variable expansion ($VAR and ${VAR})
    let bytes = after_tilde.as_bytes();
    let len = bytes.len();
    let mut result = String::with_capacity(len);
    let mut i = 0;

    while i < len {
        if bytes[i] == b'$' && i + 1 < len {
            let (var_name, end_idx) = if bytes[i + 1] == b'{' {
                // ${VAR} form
                if let Some(close) = after_tilde[i + 2..].find('}') {
                    let name = &after_tilde[i + 2..i + 2 + close];
                    (name, i + 2 + close + 1)
                } else {
                    // No closing brace — not a valid variable reference
                    result.push('$');
                    i += 1;
                    continue;
                }
            } else {
                // $VAR form — variable name is [A-Za-z_][A-Za-z0-9_]*
                let start = i + 1;
                if start < len
                    && (bytes[start].is_ascii_alphabetic() || bytes[start] == b'_')
                {
                    let mut end = start + 1;
                    while end < len
                        && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_')
                    {
                        end += 1;
                    }
                    (&after_tilde[start..end], end)
                } else {
                    result.push('$');
                    i += 1;
                    continue;
                }
            };

            if let Some(value) = env_vars.get(var_name) {
                result.push_str(value);
            } else {
                // Unknown variable — preserve the original text
                result.push_str(&after_tilde[i..end_idx]);
            }
            i = end_idx;
        } else {
            result.push(bytes[i] as char);
            i += 1;
        }
    }

    result
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
