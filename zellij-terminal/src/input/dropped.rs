//! The text pasted for a dropped file: its shell-escaped path and a space.

use std::path::Path;

pub fn dropped_file_text(path: &Path) -> String {
    let mut text = escape(&path.to_string_lossy());
    text.push(' ');
    text
}

/// Backslash-escapes what a shell (zsh's EXTENDED_GLOB included) would split or
/// expand. Control characters become a bare `?`, which still globs to them but
/// cannot act as a keystroke.
#[cfg(not(windows))]
fn escape(path: &str) -> String {
    const SPECIAL: &[char] = &[
        ' ', '\\', '"', '\'', '`', '$', '!', '#', '&', ';', '|', '*', '?', '(', ')', '[', ']', '{',
        '}', '<', '>', '^', '~',
    ];
    let mut escaped = String::with_capacity(path.len());
    for c in path.chars() {
        if c.is_control() {
            escaped.push('?');
            continue;
        }
        if SPECIAL.contains(&c) {
            escaped.push('\\');
        }
        escaped.push(c);
    }
    escaped
}

/// Backslashes are separators here, so the path is quoted instead.
#[cfg(windows)]
fn escape(path: &str) -> String {
    const SPECIAL: &[char] = &[' ', '&', '(', ')', '^', ';', ',', '\'', '`', '$'];
    let path: String = path
        .chars()
        .map(|c| if c.is_control() { '?' } else { c })
        .collect();
    if path.contains(SPECIAL) {
        format!("\"{}\"", path)
    } else {
        path
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

    #[cfg(not(windows))]
    #[test]
    fn zsh_extended_glob_operators_are_escaped() {
        assert_eq!(
            dropped_file_text(Path::new("/tmp/x^y~z.png")),
            "/tmp/x\\^y\\~z.png "
        );
    }

    #[test]
    fn control_characters_never_reach_the_pane_as_keystrokes() {
        let text = dropped_file_text(Path::new("/tmp/a\nrm -rf ~\r\tb\u{1b}[c\u{7f}d\u{9b}201~e"));
        assert!(
            !text.chars().any(char::is_control),
            "a control character would act as a keystroke: {:?}",
            text
        );
    }

    #[cfg(not(windows))]
    #[test]
    fn a_newline_in_a_name_becomes_a_glob_that_still_matches_it() {
        assert_eq!(
            dropped_file_text(Path::new("/tmp/a\nrm -rf ~")),
            "/tmp/a?rm\\ -rf\\ \\~ "
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
