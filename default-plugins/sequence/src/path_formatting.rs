use std::path::PathBuf;

/// Expand a path string to an absolute PathBuf
/// Handles ~, ./, .., relative and absolute paths
pub fn expand_path(path_str: &str, current_cwd: Option<&PathBuf>) -> Option<PathBuf> {
    let expanded = if path_str.starts_with("~/") || path_str == "~" {
        let home_dir = std::env::var("HOME").ok()?;
        if path_str == "~" {
            PathBuf::from(home_dir)
        } else {
            PathBuf::from(home_dir).join(&path_str[2..])
        }
    } else if path_str.starts_with('/') {
        PathBuf::from(path_str)
    } else {
        let base = current_cwd?;
        base.join(path_str)
    };

    let mut normalized = PathBuf::new();
    for component in expanded.components() {
        match component {
            std::path::Component::ParentDir => {
                normalized.pop();
            },
            std::path::Component::CurDir => {},
            _ => {
                normalized.push(component);
            },
        }
    }

    Some(normalized)
}

/// Format a path for display in the prompt
/// Example: /home/user/some/long/path -> ~/s/l/path
pub fn format_cwd(cwd: &PathBuf) -> String {
    let path_str = cwd.to_string_lossy().to_string();

    let home_dir = std::env::var("HOME").unwrap_or_default();
    let path_str = if !home_dir.is_empty() && path_str.starts_with(&home_dir) {
        path_str.replacen(&home_dir, "~", 1)
    } else {
        path_str
    };

    let parts: Vec<&str> = path_str.split('/').collect();

    if parts.len() <= 1 {
        return path_str;
    }

    let mut formatted_parts = Vec::new();
    for (i, part) in parts.iter().enumerate() {
        if i == parts.len() - 1 {
            formatted_parts.push(part.to_string());
        } else if part.is_empty() {
            formatted_parts.push(String::new());
        } else {
            formatted_parts.push(part.chars().next().unwrap_or_default().to_string());
        }
    }

    formatted_parts.join("/")
}

/// Alias for expand_path for backwards compatibility
pub fn resolve_path(current_cwd: Option<&PathBuf>, path_str: &str) -> Option<PathBuf> {
    expand_path(path_str, current_cwd)
}
