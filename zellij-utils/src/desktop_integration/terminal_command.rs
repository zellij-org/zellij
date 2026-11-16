use std::path::{Path, PathBuf};

const LAUNCHER_SUFFIX: &str = "-terminal";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalCommand {
    pub program: PathBuf,
    pub arguments: &'static str,
}

impl TerminalCommand {
    pub fn uses_launcher(&self) -> bool {
        self.arguments.is_empty()
    }

    pub fn command_line(&self) -> String {
        let program = format!("\"{}\"", self.program.display());
        if self.arguments.is_empty() {
            program
        } else {
            format!("{} {}", program, self.arguments)
        }
    }
}

pub fn terminal_command(exe: &Path) -> TerminalCommand {
    let launcher = launcher_of(exe);
    let launcher_exists = launcher.is_file();
    command_for(exe, launcher, launcher_exists)
}

fn launcher_of(exe: &Path) -> PathBuf {
    let stem = exe
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default();
    exe.with_file_name(format!(
        "{}{}{}",
        stem,
        LAUNCHER_SUFFIX,
        std::env::consts::EXE_SUFFIX
    ))
}

fn command_for(exe: &Path, launcher: PathBuf, launcher_exists: bool) -> TerminalCommand {
    if launcher_exists {
        TerminalCommand {
            program: launcher,
            arguments: "",
        }
    } else {
        TerminalCommand {
            program: exe.to_path_buf(),
            arguments: "terminal",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_launcher_sits_next_to_the_program_it_is_named_after() {
        let dir = Path::new("install");
        assert_eq!(
            launcher_of(&dir.join("zellij.exe")),
            dir.join("zellij-terminal.exe")
        );
        assert_eq!(
            launcher_of(&dir.join("acme.exe")),
            dir.join("acme-terminal.exe")
        );
    }

    #[test]
    fn the_window_starts_through_the_launcher_when_there_is_one() {
        let exe = Path::new("install").join("zellij.exe");
        let launcher = launcher_of(&exe);
        let command = command_for(&exe, launcher.clone(), true);
        assert_eq!(
            command,
            TerminalCommand {
                program: launcher,
                arguments: "",
            }
        );
        assert!(command.uses_launcher());
    }

    #[test]
    fn the_command_line_quotes_the_program_and_adds_the_verb_only_without_a_launcher() {
        let dir = Path::new(r"C:\Program Files\Zellij");
        let launcher = TerminalCommand {
            program: dir.join("zellij-terminal.exe"),
            arguments: "",
        };
        assert_eq!(
            launcher.command_line(),
            r#""C:\Program Files\Zellij\zellij-terminal.exe""#
        );
        let direct = TerminalCommand {
            program: dir.join("zellij.exe"),
            arguments: "terminal",
        };
        assert_eq!(
            direct.command_line(),
            r#""C:\Program Files\Zellij\zellij.exe" terminal"#
        );
    }

    #[test]
    fn without_a_launcher_the_window_verb_is_run_directly() {
        let exe = Path::new("install").join("zellij.exe");
        let command = command_for(&exe, launcher_of(&exe), false);
        assert_eq!(
            command,
            TerminalCommand {
                program: exe,
                arguments: "terminal",
            }
        );
        assert!(!command.uses_launcher());
    }
}
