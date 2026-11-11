//! A file dropped onto the window reaches the focused pane as its path, the way
//! other terminals hand drops over: pasted, escaped for a shell, and followed by
//! a space so the next drop or word does not run into it.

use std::path::Path;

pub fn dropped_file_text(path: &Path) -> String {
    let mut text = escape(&path.to_string_lossy());
    text.push(' ');
    text
}

/// Backslash-escapes what a POSIX shell would split on or expand, as Ghostty,
/// iTerm2 and Terminal.app do, so the path survives being pasted at a prompt.
#[cfg(not(windows))]
fn escape(path: &str) -> String {
    const SPECIAL: &[char] = &[
        ' ', '\t', '\\', '"', '\'', '`', '$', '!', '#', '&', ';', '|', '*', '?', '(', ')', '[',
        ']', '{', '}', '<', '>',
    ];
    let mut escaped = String::with_capacity(path.len());
    for c in path.chars() {
        if SPECIAL.contains(&c) {
            escaped.push('\\');
        }
        escaped.push(c);
    }
    escaped
}

/// Backslashes separate Windows paths, so a path that needs protecting is
/// wrapped in double quotes instead, which cmd and PowerShell both accept.
#[cfg(windows)]
fn escape(path: &str) -> String {
    const SPECIAL: &[char] = &[' ', '\t', '&', '(', ')', '^', ';', ',', '\'', '`', '$'];
    if path.contains(SPECIAL) {
        format!("\"{}\"", path)
    } else {
        path.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_plain_path_is_pasted_as_is_with_a_trailing_space() {
        assert_eq!(dropped_file_text(Path::new("plain.png")), "plain.png ");
    }

    #[cfg(not(windows))]
    #[test]
    fn spaces_in_a_path_are_escaped_so_it_stays_one_word() {
        assert_eq!(
            dropped_file_text(Path::new(
                "/Users/me/Desktop/Screenshot 2026-11-11 at 19.02.14.png"
            )),
            "/Users/me/Desktop/Screenshot\\ 2026-11-11\\ at\\ 19.02.14.png "
        );
    }

    #[cfg(not(windows))]
    #[test]
    fn shell_metacharacters_are_escaped() {
        assert_eq!(
            dropped_file_text(Path::new("/tmp/it's $HOME (1)&[x].txt")),
            "/tmp/it\\'s\\ \\$HOME\\ \\(1\\)\\&\\[x\\].txt "
        );
    }

    #[cfg(windows)]
    #[test]
    fn a_windows_path_with_spaces_is_quoted_not_escaped() {
        assert_eq!(
            dropped_file_text(Path::new(r"C:\Users\me\My Pictures\shot.png")),
            "\"C:\\Users\\me\\My Pictures\\shot.png\" "
        );
    }
}
