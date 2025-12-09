use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

/// Result of a fuzzy completion operation
#[derive(Debug, Clone)]
pub struct CompletionResult {
    pub completed_text: String,
    pub score: i64,
    pub is_directory: bool,
    pub is_prefix_completion: bool, // True if this is the shortest of multiple matches with same prefix
}

/// Type of completion
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CompletionType {
    Command,
    Path,
}

/// Fuzzy match executables (commands) from the PATH
pub fn fuzzy_complete_command(
    query: &str,
    executables: &BTreeMap<String, PathBuf>,
) -> Option<CompletionResult> {
    if query.is_empty() {
        return None;
    }

    let matcher = SkimMatcherV2::default().ignore_case();

    // Collect all matches with their scores
    let mut all_matches: Vec<(String, i64)> = Vec::new();

    for executable_name in executables.keys() {
        if let Some((score, _indices)) = matcher.fuzzy_indices(executable_name, query) {
            all_matches.push((executable_name.clone(), score));
        }
    }

    if all_matches.is_empty() {
        return None;
    }

    // Sort by score (highest first), then by length (shortest first)
    all_matches.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.len().cmp(&b.0.len())));

    // Get the best match
    let (best_name, best_score) = &all_matches[0];

    // Check if there are multiple matches that share the same prefix
    let is_prefix_completion = check_for_prefix_matches(&all_matches, best_name);

    Some(CompletionResult {
        completed_text: best_name.clone(),
        score: *best_score,
        is_directory: false, // Commands are not directories
        is_prefix_completion,
    })
}

/// Fuzzy match paths (directories and files) relative to the current working directory
pub fn fuzzy_complete_path(query: &str, cwd: Option<&PathBuf>) -> Option<CompletionResult> {
    if query.is_empty() {
        return None;
    }

    let matcher = SkimMatcherV2::default().ignore_case();

    // Determine the base directory to search in
    let (base_dir, search_prefix) = if query.starts_with('/') {
        // Absolute path
        let parts: Vec<&str> = query.rsplitn(2, '/').collect();
        if parts.len() == 2 {
            // Has a directory component
            let dir = parts[1];
            let prefix = parts[0];
            (PathBuf::from(dir), prefix.to_string())
        } else {
            // Just "/"
            (PathBuf::from("/"), String::new())
        }
    } else if query.starts_with("~/") {
        // Home directory
        let home_dir = std::env::var("HOME").ok()?;
        let query_without_tilde = &query[2..];
        let parts: Vec<&str> = query_without_tilde.rsplitn(2, '/').collect();
        if parts.len() == 2 {
            let dir = PathBuf::from(&home_dir).join(parts[1]);
            let prefix = parts[0];
            (dir, prefix.to_string())
        } else {
            // Just "~/" or "~/something"
            (PathBuf::from(home_dir), query_without_tilde.to_string())
        }
    } else {
        // Relative path
        let base = cwd?;
        let parts: Vec<&str> = query.rsplitn(2, '/').collect();
        if parts.len() == 2 {
            // Has a directory component
            let dir = base.join(parts[1]);
            let prefix = parts[0];
            (dir, prefix.to_string())
        } else {
            // Just a name in the current directory
            (base.clone(), query.to_string())
        }
    };

    // Convert to host path
    let host_base_dir =
        PathBuf::from("/host").join(base_dir.strip_prefix("/").unwrap_or(&base_dir));

    // Read directory entries
    let entries = fs::read_dir(&host_base_dir).ok()?;

    // Collect all matches with their scores and directory status
    let mut all_matches: Vec<(String, i64, bool)> = Vec::new();

    for entry in entries.flatten() {
        if let Some(name) = entry.file_name().to_str() {
            // Skip hidden files unless the query starts with a dot
            if name.starts_with('.') && !search_prefix.starts_with('.') {
                continue;
            }

            if let Some((score, _indices)) = matcher.fuzzy_indices(name, &search_prefix) {
                // Check if this entry is a directory
                let is_dir = entry.metadata().ok().map(|m| m.is_dir()).unwrap_or(false);
                all_matches.push((name.to_string(), score, is_dir));
            }
        }
    }

    if all_matches.is_empty() {
        return None;
    }

    // Sort by score (highest first), then by length (shortest first)
    all_matches.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.len().cmp(&b.0.len())));

    // Get the best match
    let (name, score, is_directory) = all_matches[0].clone();

    // Check if there are multiple matches that share the same prefix
    let name_matches: Vec<(String, i64)> = all_matches
        .iter()
        .map(|(n, s, _)| (n.clone(), *s))
        .collect();
    let is_prefix_completion = check_for_prefix_matches(&name_matches, &name);

    // Reconstruct the full path
    let completed_text = if query.starts_with('/') {
        // Absolute path
        let dir_part = query.rsplitn(2, '/').nth(1).unwrap_or("");
        if dir_part.is_empty() {
            format!("/{}", name)
        } else {
            format!("{}/{}", dir_part, name)
        }
    } else if query.starts_with("~/") {
        // Home directory
        let query_without_tilde = &query[2..];
        let dir_part = query_without_tilde.rsplitn(2, '/').nth(1).unwrap_or("");
        if dir_part.is_empty() {
            format!("~/{}", name)
        } else {
            format!("~/{}/{}", dir_part, name)
        }
    } else {
        // Relative path
        let dir_part = query.rsplitn(2, '/').nth(1).unwrap_or("");
        if dir_part.is_empty() {
            name.clone()
        } else {
            format!("{}/{}", dir_part, name)
        }
    };

    Some(CompletionResult {
        completed_text,
        score,
        is_directory,
        is_prefix_completion,
    })
}

/// Check if there are multiple matches that start with the best match's name
/// Returns true if the best match is a prefix of other matches
fn check_for_prefix_matches(matches: &[(String, i64)], best_match: &str) -> bool {
    if matches.len() <= 1 {
        return false;
    }

    // Count how many matches start with the best_match text
    let prefix_count = matches
        .iter()
        .filter(|(name, _)| name.starts_with(best_match) && name != best_match)
        .count();

    prefix_count > 0
}

/// Perform fuzzy completion on the current input
/// Returns the best completion (command or path) with appropriate suffix (space or slash)
pub fn fuzzy_complete(
    input: &str,
    executables: &BTreeMap<String, PathBuf>,
    cwd: Option<&PathBuf>,
) -> Option<String> {
    // If input is empty, nothing to complete
    if input.is_empty() {
        return None;
    }

    // Extract the last word/token to complete
    // For simplicity, we'll complete the entire input if it's a single word,
    // or the last space-separated token if there are multiple words
    let tokens: Vec<&str> = input.split_whitespace().collect();
    let to_complete = if tokens.is_empty() {
        input
    } else {
        tokens.last().unwrap()
    };

    // Get both command and path completions
    let command_result = fuzzy_complete_command(to_complete, executables);
    let path_result = fuzzy_complete_path(to_complete, cwd);

    // Choose the better match based on score
    match (command_result, path_result) {
        (Some(cmd), Some(path)) => {
            // Both matched, use the higher score
            let (result, completion_type, is_dir, is_prefix) = if cmd.score >= path.score {
                (
                    cmd.completed_text,
                    CompletionType::Command,
                    false,
                    cmd.is_prefix_completion,
                )
            } else {
                (
                    path.completed_text,
                    CompletionType::Path,
                    path.is_directory,
                    path.is_prefix_completion,
                )
            };

            // Add appropriate suffix (skip slash if this is a prefix completion)
            let suffix = get_completion_suffix(completion_type, is_dir, is_prefix);
            let completed = format!("{}{}", result, suffix);

            // Replace the last token with the completion
            Some(replace_last_token(input, &completed))
        },
        (Some(cmd), None) => {
            // Command completion - add space (unless it's a prefix completion)
            let suffix = if cmd.is_prefix_completion { "" } else { " " };
            let completed = format!("{}{}", cmd.completed_text, suffix);
            Some(replace_last_token(input, &completed))
        },
        (None, Some(path)) => {
            // Path completion - add slash if directory (unless it's a prefix completion)
            let suffix = if path.is_prefix_completion {
                ""
            } else if path.is_directory {
                "/"
            } else {
                ""
            };
            let completed = format!("{}{}", path.completed_text, suffix);
            Some(replace_last_token(input, &completed))
        },
        (None, None) => None,
    }
}

/// Get the appropriate suffix for a completion
fn get_completion_suffix(
    completion_type: CompletionType,
    is_directory: bool,
    is_prefix_completion: bool,
) -> &'static str {
    // No suffix for prefix completions
    if is_prefix_completion {
        return "";
    }

    match completion_type {
        CompletionType::Command => " ",
        CompletionType::Path => {
            if is_directory {
                "/"
            } else {
                ""
            }
        },
    }
}

/// Replace the last whitespace-separated token in the input with the completion
fn replace_last_token(input: &str, completion: &str) -> String {
    let tokens: Vec<&str> = input.split_whitespace().collect();
    if tokens.is_empty() {
        return completion.to_string();
    }

    if tokens.len() == 1 {
        completion.to_string()
    } else {
        // Join all tokens except the last one, then add the completion
        let mut result = tokens[..tokens.len() - 1].join(" ");
        result.push(' ');
        result.push_str(completion);
        result
    }
}
