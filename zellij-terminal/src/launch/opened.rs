//! The window for a file opened with the app. Not macOS-specific, but only macOS hands files to
//! the app so far.

use std::path::Path;

use kdl::KdlValue;
use zellij_utils::app_control::OpenWindowRequest;
use zellij_utils::data::LayoutInfo;
use zellij_utils::input::options::{Options, PaneFrameStyle};

pub(crate) fn opened_request(path: &Path, shell: Option<&str>) -> OpenWindowRequest {
    if path.is_dir() {
        OpenWindowRequest {
            cwd: Some(path.to_path_buf()),
            ..Default::default()
        }
    } else {
        script_request(path, shell)
    }
}

/// A fixed layout, so the script's pane is focused whatever the default layout.
fn script_request(script: &Path, shell: Option<&str>) -> OpenWindowRequest {
    OpenWindowRequest {
        layout: Some(LayoutInfo::Stringified(script_layout(script, shell))),
        cwd: script.parent().map(Path::to_path_buf),
        options: Some(script_options()),
        ..Default::default()
    }
}

/// A full frame, even around a single pane, shows the exit code and the Enter/Esc hints.
fn script_options() -> Options {
    Options {
        pane_frame_style: Some(PaneFrameStyle::Full),
        show_startup_tips: Some(false),
        show_release_notes: Some(false),
        ..Default::default()
    }
}

fn script_layout(script: &Path, shell: Option<&str>) -> String {
    let kdl = |text: &str| KdlValue::String(text.to_owned()).to_string();
    // The layout parser expands variables in `command` and `cwd`; `$$` is a literal `$`.
    let kdl_path = |text: &str| kdl(&text.replace('$', "$$"));
    let script_path = script.to_string_lossy();
    let (program, args) = match shell.and_then(|shell| Some((shell, shell_args(shell)?))) {
        Some((shell, args)) => {
            let args: Vec<String> = args.iter().map(|arg| kdl(arg)).collect();
            (
                shell,
                format!("args {} {}", args.join(" "), kdl(&script_path)),
            )
        },
        None => (&*script_path, String::new()),
    };
    format!(
        "layout {{\n    pane name={} command={} cwd={} {{\n        {}\n    }}\n}}\n",
        kdl(&script.file_name().unwrap_or_default().to_string_lossy()),
        kdl_path(program),
        kdl_path(&script.parent().unwrap_or(Path::new("")).to_string_lossy()),
        args
    )
}

/// Runs the script in an interactive shell, so it sees what the user's rc files put on PATH (the
/// app's environment comes from a login shell, which skips `.zshrc`). The path is passed as an
/// argument, so it needs no quoting. Other shells run the script directly.
fn shell_args(shell: &str) -> Option<&'static [&'static str]> {
    match Path::new(shell).file_name()?.to_str()? {
        "zsh" | "bash" | "sh" | "dash" | "ksh" => Some(&["-i", "-c", "\"$@\"", "zellij"]),
        "fish" => Some(&["-i", "-c", "$argv"]),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use zellij_utils::input::command::RunCommand;
    use zellij_utils::input::layout::{Layout, Run, TiledPaneLayout};

    fn only_pane(request: &OpenWindowRequest) -> TiledPaneLayout {
        let Some(LayoutInfo::Stringified(raw)) = &request.layout else {
            panic!("no layout in {:?}", request);
        };
        let layout = Layout::from_str(raw, "script".to_owned(), None, None)
            .unwrap_or_else(|e| panic!("{:?} in\n{}", e, raw));
        let mut pane = layout.new_tab().0;
        while pane.children.len() == 1 {
            pane = pane.children.remove(0);
        }
        assert!(pane.children.is_empty(), "more than one pane in\n{}", raw);
        pane
    }

    fn run_command(script: &str, shell: Option<&str>) -> RunCommand {
        match only_pane(&script_request(Path::new(script), shell)).run {
            Some(Run::Command(command)) => command,
            other => panic!("not a command pane: {:?}", other),
        }
    }

    #[test]
    fn a_folder_opens_a_plain_window_there() {
        let folder = std::env::temp_dir();
        let request = opened_request(&folder, Some("/bin/zsh"));
        assert_eq!(request.cwd.as_deref(), Some(folder.as_path()));
        assert_eq!(request.layout, None);
        assert_eq!(request.options, None);
        assert!(request.command.is_empty());
    }

    #[test]
    fn the_script_runs_in_its_own_folder_in_a_new_session() {
        let request = script_request(Path::new("/x/y/s.command"), Some("/bin/zsh"));
        assert_eq!(request.cwd.as_deref(), Some(Path::new("/x/y")));
        assert_eq!(request.session, None);
        assert_eq!(only_pane(&request).name.as_deref(), Some("s.command"));
        let command = run_command("/x/y/s.command", Some("/bin/zsh"));
        assert_eq!(command.cwd.as_deref(), Some(Path::new("/x/y")));
        assert!(command.hold_on_close, "the pane must stay after the script");
    }

    #[test]
    fn a_posix_shell_runs_the_script_as_an_argument() {
        for shell in ["/bin/zsh", "/opt/homebrew/bin/bash"] {
            let command = run_command("/x/a b.command", Some(shell));
            assert_eq!(command.command, Path::new(shell));
            assert_eq!(
                command.args,
                ["-i", "-c", "\"$@\"", "zellij", "/x/a b.command"]
            );
        }
    }

    #[test]
    fn fish_gets_the_script_through_argv() {
        let command = run_command("/x/s.command", Some("/opt/homebrew/bin/fish"));
        assert_eq!(command.command, Path::new("/opt/homebrew/bin/fish"));
        assert_eq!(command.args, ["-i", "-c", "$argv", "/x/s.command"]);
    }

    #[test]
    fn an_unknown_or_missing_shell_runs_the_script_directly() {
        for shell in [None, Some(""), Some("/opt/homebrew/bin/nu")] {
            let command = run_command("/x/s.command", shell);
            assert_eq!(command.command, Path::new("/x/s.command"), "{:?}", shell);
            assert!(command.args.is_empty(), "{:?}", shell);
        }
    }

    #[test]
    fn the_script_window_shows_its_frame_and_no_tips() {
        let options = script_request(Path::new("/x/s.command"), None)
            .options
            .expect("options");
        assert_eq!(options.pane_frame_style, Some(PaneFrameStyle::Full));
        assert_eq!(options.show_startup_tips, Some(false));
        assert_eq!(options.show_release_notes, Some(false));
        assert_eq!(
            options.pane_frames, None,
            "the CLI merge flips a boolean set on both sides"
        );
    }

    #[test]
    fn any_path_reaches_the_pane_unchanged() {
        let folder = "/x/a \"b\" \\c\n{d}; e";
        let script = format!("{}/r\"un\\ me\n.command", folder);
        let request = script_request(Path::new(&script), Some("/bin/zsh"));
        assert_eq!(
            only_pane(&request).name.as_deref(),
            Some("r\"un\\ me\n.command")
        );
        let command = run_command(&script, Some("/bin/zsh"));
        assert_eq!(command.args.last(), Some(&script));
        assert_eq!(command.cwd.as_deref(), Some(Path::new(folder)));
    }

    #[test]
    fn dollar_signs_in_paths_are_not_expanded() {
        let folder = "/x/$HOME/my$no_such_var/${HOME}$$";
        let script = format!("{}/r$u.command", folder);
        for shell in [Some("/opt/$HOME/bin/zsh"), None] {
            let command = run_command(&script, shell);
            assert_eq!(
                command.cwd.as_deref(),
                Some(Path::new(folder)),
                "{:?}",
                shell
            );
            let program = shell
                .map(PathBuf::from)
                .unwrap_or_else(|| script.clone().into());
            assert_eq!(command.command, program, "{:?}", shell);
        }
    }
}
