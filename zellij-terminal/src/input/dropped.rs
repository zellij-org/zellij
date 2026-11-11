//! The text pasted for a dropped file: its shell-escaped path and a space.

use std::path::Path;

pub fn dropped_file_text(path: &Path) -> String {
    format!("{} ", escape(&path.to_string_lossy()))
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

    #[cfg(not(windows))]
    #[test]
    fn what_a_shell_would_split_or_expand_is_backslash_escaped() {
        for (path, pasted) in [
            ("/tmp/Screenshot 1.png", r"/tmp/Screenshot\ 1.png "),
            (
                "/tmp/it's $HOME (1)&[x].txt",
                r"/tmp/it\'s\ \$HOME\ \(1\)\&\[x\].txt ",
            ),
            ("/tmp/x^y~z.png", r"/tmp/x\^y\~z.png "),
            // A control character becomes a `?` glob that still matches it.
            ("/tmp/a\nrm -rf ~", r"/tmp/a?rm\ -rf\ \~ "),
        ] {
            assert_eq!(dropped_file_text(Path::new(path)), pasted, "{:?}", path);
        }
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

    #[cfg(windows)]
    #[test]
    fn a_windows_path_with_spaces_is_quoted_not_escaped() {
        assert_eq!(
            dropped_file_text(Path::new(r"C:\Users\me\My Pictures\shot.png")),
            "\"C:\\Users\\me\\My Pictures\\shot.png\" "
        );
    }
}
