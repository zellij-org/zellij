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
///
/// A control character (a newline, say) cannot be escaped this way: pasted into a
/// pane without bracketed paste, it would run the line typed so far. It becomes a
/// bare `?` instead, which a shell glob still matches against that one character.
#[cfg(not(windows))]
fn escape(path: &str) -> String {
    const SPECIAL: &[char] = &[
        ' ', '\\', '"', '\'', '`', '$', '!', '#', '&', ';', '|', '*', '?', '(', ')', '[', ']', '{',
        '}', '<', '>',
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

/// Backslashes separate Windows paths, so a path that needs protecting is
/// wrapped in double quotes instead, which cmd and PowerShell both accept.
/// Windows forbids control characters in file names, but should one arrive it
/// becomes `?` rather than a keystroke, as on other platforms.
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

    #[test]
    fn control_characters_never_reach_the_pane_as_keystrokes() {
        let text = dropped_file_text(Path::new("/tmp/a\nrm -rf ~\r\tb\u{1b}[c"));
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
            "/tmp/a?rm\\ -rf\\ ~ "
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
