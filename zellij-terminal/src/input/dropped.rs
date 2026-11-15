//! The text pasted for a dropped file: its shell-escaped path and a space.

use std::path::Path;

pub fn dropped_file_text(path: &Path) -> String {
    format!("{} ", escape(&path.to_string_lossy()))
}

/// Backslash-escapes what a shell (zsh's EXTENDED_GLOB included) would split or
/// expand. Control characters become an escaped literal `?`, which cannot act
/// as a keystroke or glob to another file.
#[cfg(not(windows))]
fn escape(path: &str) -> String {
    escape_posix(path)
}

fn escape_posix(path: &str) -> String {
    const SPECIAL: &[char] = &[
        ' ', '\\', '"', '\'', '`', '$', '!', '#', '&', ';', '|', '*', '?', '(', ')', '[', ']', '{',
        '}', '<', '>', '^', '~',
    ];
    let mut escaped = String::with_capacity(path.len());
    for c in path.chars() {
        if c.is_control() {
            escaped.push_str("\\?");
            continue;
        }
        if SPECIAL.contains(&c) {
            escaped.push('\\');
        }
        escaped.push(c);
    }
    escaped
}

#[cfg(any(windows, test))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WindowsShell {
    Posix,
    PowerShell,
    Cmd,
}

#[cfg(any(windows, test))]
fn windows_shell_from(shell: Option<&str>, comspec: Option<&str>) -> WindowsShell {
    fn program_name(program: &str) -> String {
        let name = program
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or(program)
            .to_ascii_lowercase();
        name.strip_suffix(".exe").unwrap_or(&name).to_owned()
    }
    if let Some(shell) = shell.filter(|shell| !shell.is_empty()) {
        let name = program_name(shell);
        if name.contains("powershell") || name == "pwsh" {
            return WindowsShell::PowerShell;
        }
        if name == "cmd" {
            return WindowsShell::Cmd;
        }
        return WindowsShell::Posix;
    }
    match comspec.map(program_name).as_deref() {
        Some("powershell") | Some("pwsh") => WindowsShell::PowerShell,
        _ => WindowsShell::Cmd,
    }
}

#[cfg(any(windows, test))]
fn escape_for(shell: WindowsShell, path: &str) -> String {
    let path: String = path
        .chars()
        .map(|c| if c.is_control() { '?' } else { c })
        .collect();
    match shell {
        WindowsShell::Posix => escape_posix(&path),
        WindowsShell::PowerShell => format!("'{}'", path.replace('\'', "''")),
        WindowsShell::Cmd => {
            // A caret inside the variable name prevents `%NAME%`/`!NAME!`
            // expansion; cmd removes it before handing the argument to the
            // program. The path is always quoted for spaces and shell operators.
            let escaped = path.replace('%', "%^").replace('!', "!^");
            format!("\"{}\"", escaped)
        },
    }
}

/// Windows file names cannot contain quotes, and a command's shell determines
/// which characters expand inside an argument.
#[cfg(windows)]
fn escape(path: &str) -> String {
    let shell = std::env::var("SHELL").ok();
    let comspec = std::env::var("COMSPEC").ok();
    escape_for(
        windows_shell_from(shell.as_deref(), comspec.as_deref()),
        path,
    )
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
            // A control character becomes a literal `?`, never a glob.
            ("/tmp/a\nrm -rf ~", r"/tmp/a\?rm\ -rf\ \~ "),
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

    #[test]
    fn each_windows_shell_gets_literal_quoting() {
        let path = r"C:\Users\me\My Pictures\it's $HOME `%TEMP%`.png";
        assert_eq!(
            escape_for(WindowsShell::PowerShell, path),
            r"'C:\Users\me\My Pictures\it''s $HOME `%TEMP%`.png'"
        );
        assert_eq!(
            escape_for(WindowsShell::Cmd, path),
            r#""C:\Users\me\My Pictures\it's $HOME `%^TEMP%^`.png""#
        );
        assert_eq!(
            escape_for(WindowsShell::Posix, path),
            r"C:\\Users\\me\\My\ Pictures\\it\'s\ \$HOME\ \`%TEMP%\`.png"
        );
    }

    #[test]
    fn windows_default_shell_detection_matches_the_shell_zellij_starts() {
        assert_eq!(
            windows_shell_from(Some(r"C:\Program Files\Git\bin\bash.exe"), None),
            WindowsShell::Posix
        );
        assert_eq!(
            windows_shell_from(Some(r"C:\Program Files\PowerShell\7\pwsh.exe"), None),
            WindowsShell::PowerShell
        );
        assert_eq!(
            windows_shell_from(Some(r"C:\Windows\System32\cmd.exe"), None),
            WindowsShell::Cmd
        );
        assert_eq!(
            windows_shell_from(None, Some(r"C:\Windows\System32\cmd.exe")),
            WindowsShell::Cmd
        );
    }
}
