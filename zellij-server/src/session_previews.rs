use std::path::PathBuf;
use std::time::Duration;
use zellij_utils::consts::{session_layout_cache_file_name, ZELLIJ_SOCK_DIR};
use zellij_utils::data::{SavedPanePreview, SavedSessionPreview, SavedTabPreview, SessionPreview};
use zellij_utils::input::layout::{FloatingPaneLayout, Layout, Run, TiledPaneLayout};
use zellij_utils::ipc::{
    ClientToServerMsg, IpcReceiverWithContext, IpcSenderWithContext, ServerToClientMsg,
};
use zellij_utils::session_index::layout_cwd;
use zellij_utils::sessions::{validate_session_folder_name, validate_session_name};

pub const REMOTE_PREVIEW_TIMEOUT: Duration = Duration::from_millis(1000);
pub const LOCAL_PREVIEW_ANSWER_TIMEOUT: Duration = Duration::from_millis(800);

/// How many lines of a saved pane reach the preview event. Matches `PREVIEW_SCROLLBACK_LINES`,
/// which is what running session previews keep.
pub const SAVED_PREVIEW_SCROLLBACK_LINES: usize = 300;

fn preview_error(session_name: &str, tab_index: Option<usize>, error: String) -> SessionPreview {
    SessionPreview {
        session_name: session_name.to_owned(),
        tab_index,
        error: Some(error),
        ..Default::default()
    }
}

fn checked_session_id(session_name: &str) -> Result<String, String> {
    validate_session_name(session_name)?;
    validate_session_folder_name(session_name)?;
    let session_id = zellij_utils::sessions::resolve_session_id(session_name)
        .unwrap_or_else(|| session_name.to_owned());
    validate_session_folder_name(&session_id)?;
    Ok(session_id)
}

pub fn running_session_preview(
    session_name: &str,
    tab_index: Option<usize>,
    pane_id: Option<(u32, bool)>,
    anchor: Option<zellij_utils::data::PreviewAnchor>,
) -> SessionPreview {
    let session_id = match checked_session_id(session_name) {
        Ok(session_id) => session_id,
        Err(e) => return preview_error(session_name, tab_index, e),
    };
    let socket_path = ZELLIJ_SOCK_DIR.join(session_id);
    #[cfg(windows)]
    let (stream, reply_stream) = match zellij_utils::pipe_pairs::connect_pair(&socket_path) {
        Ok(pipes) => pipes,
        Err(e) => return preview_error(session_name, tab_index, e.to_string()),
    };
    #[cfg(not(windows))]
    let stream = match zellij_utils::consts::ipc_connect(&socket_path) {
        Ok(stream) => stream,
        Err(e) => return preview_error(session_name, tab_index, e.to_string()),
    };
    let mut sender = IpcSenderWithContext::<ClientToServerMsg>::new(stream);
    if let Err(e) = sender.send_client_msg(ClientToServerMsg::RequestSessionPreview {
        tab_index: tab_index.map(|t| t as u32),
        pane_id,
        anchor,
    }) {
        return preview_error(session_name, tab_index, e.to_string());
    }
    #[cfg(windows)]
    let mut receiver: IpcReceiverWithContext<ServerToClientMsg> =
        IpcReceiverWithContext::new(reply_stream);
    #[cfg(not(windows))]
    let mut receiver: IpcReceiverWithContext<ServerToClientMsg> = sender.get_receiver();
    let _ = receiver.set_read_timeout(Some(REMOTE_PREVIEW_TIMEOUT));
    let reply = receiver.try_recv_server_msg();
    match reply {
        Ok((ServerToClientMsg::SessionPreview { preview }, _)) => match preview {
            Some(mut preview) => {
                preview.session_name = session_name.to_owned();
                preview
            },
            None => preview_error(
                session_name,
                tab_index,
                "The session could not make a preview in time".to_owned(),
            ),
        },
        Ok(_) => preview_error(
            session_name,
            tab_index,
            "The session did not send a preview".to_owned(),
        ),
        Err(e) => preview_error(session_name, tab_index, e.to_string()),
    }
}

pub fn current_session_preview(
    senders: &crate::thread_bus::ThreadSenders,
    session_name: &str,
    tab_index: Option<usize>,
    pane_id: Option<(u32, bool)>,
    anchor: Option<zellij_utils::data::PreviewAnchor>,
) -> SessionPreview {
    let (response_sender, response_receiver) = crossbeam::channel::bounded(1);
    let sent = senders.send_to_screen(crate::screen::ScreenInstruction::GetSessionPreview {
        tab_index,
        pane_id,
        anchor,
        response_channel: response_sender,
    });
    if sent.is_err() {
        return preview_error(session_name, tab_index, "The session is closing".to_owned());
    }
    match response_receiver.recv_timeout(REMOTE_PREVIEW_TIMEOUT) {
        Ok(mut preview) => {
            preview.session_name = session_name.to_owned();
            preview
        },
        Err(e) => preview_error(session_name, tab_index, e.to_string()),
    }
}

pub fn answer_preview_request(
    senders: Option<&crate::thread_bus::ThreadSenders>,
    tab_index: Option<u32>,
    pane_id: Option<(u32, bool)>,
    anchor: Option<zellij_utils::data::PreviewAnchor>,
) -> Option<SessionPreview> {
    let senders = senders?;
    let (response_sender, response_receiver) = crossbeam::channel::bounded(1);
    let sent = senders.send_to_screen(crate::screen::ScreenInstruction::GetSessionPreview {
        tab_index: tab_index.map(|t| t as usize),
        pane_id,
        anchor,
        response_channel: response_sender,
    });
    if sent.is_err() {
        return None;
    }
    response_receiver
        .recv_timeout(LOCAL_PREVIEW_ANSWER_TIMEOUT)
        .ok()
}

fn command_text(run: &Option<Run>) -> Option<String> {
    match run {
        Some(Run::Command(run_command)) => {
            let mut parts = vec![run_command.command.display().to_string()];
            parts.extend(run_command.args.iter().cloned());
            Some(parts.join(" "))
        },
        Some(Run::EditFile(path, _, _)) => Some(format!("edit {}", path.display())),
        _ => None,
    }
}

fn command_cwd(run: &Option<Run>) -> Option<String> {
    match run {
        Some(Run::Command(run_command)) => run_command
            .cwd
            .as_ref()
            .map(|cwd| cwd.display().to_string()),
        Some(Run::Cwd(cwd)) => Some(cwd.display().to_string()),
        Some(Run::EditFile(_, _, cwd)) => cwd.as_ref().map(|cwd| cwd.display().to_string()),
        _ => None,
    }
}

fn is_plugin(run: &Option<Run>) -> bool {
    matches!(run, Some(Run::Plugin(_)))
}

fn collect_tiled_panes(layout: &TiledPaneLayout, panes: &mut Vec<SavedPanePreview>) {
    if layout.children.is_empty() {
        if is_plugin(&layout.run) {
            return;
        }
        panes.push(SavedPanePreview {
            title: layout.name.clone(),
            command: command_text(&layout.run),
            cwd: command_cwd(&layout.run),
            contents: pane_contents(layout.pane_initial_contents.as_deref()),
            focused: layout.focus.unwrap_or(false),
        });
    } else {
        for child in &layout.children {
            collect_tiled_panes(child, panes);
        }
    }
}

fn collect_floating_panes(layouts: &[FloatingPaneLayout], panes: &mut Vec<SavedPanePreview>) {
    for layout in layouts {
        if is_plugin(&layout.run) {
            continue;
        }
        panes.push(SavedPanePreview {
            title: layout.name.clone(),
            command: command_text(&layout.run),
            cwd: command_cwd(&layout.run),
            contents: pane_contents(layout.pane_initial_contents.as_deref()),
            focused: layout.focus.unwrap_or(false),
        });
    }
}

/// Keep only the last [`SAVED_PREVIEW_SCROLLBACK_LINES`] lines of a saved pane.
///
/// A session saved with `serialize_pane_viewport` keeps every line the user scrolled through, and
/// the preview event is copied into the plugin's memory. A session with a large scrollback can
/// exceed the WASM memory limit and make the plugin fail with "growth operation limited". Running
/// session previews already cut to `PREVIEW_SCROLLBACK_LINES`, so do the same here.
fn pane_contents(contents: Option<&str>) -> Option<String> {
    let contents = contents?;
    let lines: Vec<&str> = contents.lines().collect();
    if lines.len() <= SAVED_PREVIEW_SCROLLBACK_LINES {
        return Some(contents.to_owned());
    }
    let first_kept = lines.len() - SAVED_PREVIEW_SCROLLBACK_LINES;
    let offset: usize = lines[..first_kept].iter().map(|line| line.len() + 1).sum();
    Some(contents[offset..].to_owned())
}

pub fn saved_session_preview_from_layout(
    session_name: &str,
    raw_layout: &str,
    layout_path: Option<PathBuf>,
) -> SavedSessionPreview {
    let folder = layout_cwd(raw_layout).map(|cwd| cwd.display().to_string());
    match Layout::from_kdl(
        raw_layout,
        layout_path.map(|path| path.display().to_string()),
        None,
        None,
    ) {
        Ok(layout) => {
            let focused_tab = layout.focused_tab_index();
            let tabs = layout
                .tabs()
                .into_iter()
                .enumerate()
                .map(|(index, (name, tiled, floating))| {
                    let mut panes = vec![];
                    collect_tiled_panes(&tiled, &mut panes);
                    collect_floating_panes(&floating, &mut panes);
                    SavedTabPreview {
                        name: name.unwrap_or_else(|| format!("Tab #{}", index + 1)),
                        focused: focused_tab == Some(index),
                        panes,
                    }
                })
                .collect();
            SavedSessionPreview {
                session_name: session_name.to_owned(),
                folder,
                tabs,
                error: None,
            }
        },
        Err(e) => SavedSessionPreview {
            session_name: session_name.to_owned(),
            folder,
            tabs: vec![],
            error: Some(format!("{}", e)),
        },
    }
}

pub fn saved_session_preview(session_name: &str) -> SavedSessionPreview {
    let session_id = match checked_session_id(session_name) {
        Ok(session_id) => session_id,
        Err(e) => {
            return SavedSessionPreview {
                session_name: session_name.to_owned(),
                error: Some(e),
                ..Default::default()
            }
        },
    };
    let layout_path = session_layout_cache_file_name(&session_id);
    match std::fs::read_to_string(&layout_path) {
        Ok(raw_layout) => {
            saved_session_preview_from_layout(session_name, &raw_layout, Some(layout_path))
        },
        Err(e) => SavedSessionPreview {
            session_name: session_name.to_owned(),
            error: Some(e.to_string()),
            ..Default::default()
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saved_layout_lists_tabs_panes_and_commands() {
        let folder = tempfile::tempdir().unwrap();
        std::fs::write(folder.path().join("initial_contents_1"), "$ ls\nfile\n").unwrap();
        let raw = r#"
layout {
    cwd "/home/me/project"
    tab name="editor" focus=true {
        pane command="htop" cwd="/tmp"
        pane contents_file="initial_contents_1" focus=true
    }
    tab name="logs" {
        pane command="tail" {
            args "-f" "log.txt"
        }
    }
}
"#;
        let layout_path = folder.path().join("session-layout.kdl");
        let preview = saved_session_preview_from_layout("saved", raw, Some(layout_path));
        assert_eq!(preview.error, None);
        assert_eq!(preview.folder, Some("/home/me/project".to_owned()));
        assert_eq!(preview.tabs.len(), 2);
        assert_eq!(preview.tabs[0].name, "editor");
        assert!(preview.tabs[0].focused);
        assert_eq!(preview.tabs[0].panes[0].command, Some("htop".to_owned()));
        assert_eq!(
            preview.tabs[0].panes[1].contents,
            Some("$ ls\nfile\n".to_owned())
        );
        assert_eq!(
            preview.tabs[1].panes[0].command,
            Some("tail -f log.txt".to_owned())
        );
    }

    #[test]
    fn unreadable_layouts_report_an_error() {
        let preview = saved_session_preview_from_layout("broken", "layout {", None);
        assert!(preview.error.is_some());
    }

    #[test]
    fn saved_preview_keeps_the_last_lines_of_a_large_pane() {
        let folder = tempfile::tempdir().unwrap();
        let total = SAVED_PREVIEW_SCROLLBACK_LINES + 50;
        let contents: String = (0..total).map(|i| format!("line {i}\n")).collect();
        std::fs::write(folder.path().join("initial_contents_1"), &contents).unwrap();
        let raw = r#"
layout {
    tab name="logs" {
        pane contents_file="initial_contents_1"
    }
}
"#;
        let layout_path = folder.path().join("session-layout.kdl");
        let preview = saved_session_preview_from_layout("saved", raw, Some(layout_path));
        assert_eq!(preview.error, None);
        let pane_contents = preview.tabs[0].panes[0].contents.as_deref().unwrap();
        assert_eq!(
            pane_contents.lines().count(),
            SAVED_PREVIEW_SCROLLBACK_LINES,
            "a large pane has to be cut to the preview scrollback limit"
        );
        assert!(
            pane_contents.starts_with("line 50\n"),
            "the oldest lines are the ones to drop, got {:?}",
            &pane_contents[..20]
        );
        assert!(pane_contents.ends_with(&format!("line {}\n", total - 1)));
    }

    #[test]
    fn saved_preview_keeps_a_small_pane_unchanged() {
        assert_eq!(
            pane_contents(Some("$ ls\nfile\n")),
            Some("$ ls\nfile\n".to_owned())
        );
        assert_eq!(pane_contents(None), None);
    }

    #[test]
    fn saved_previews_refuse_names_that_leave_the_cache_folder() {
        for name in [
            "/tmp/zellij-preview-test",
            "../../etc",
            "..",
            ".",
            "a/b",
            "",
        ] {
            let preview = saved_session_preview(name);
            assert!(
                preview
                    .error
                    .as_deref()
                    .map(|e| e.contains("ession name"))
                    .unwrap_or(false),
                "{:?} gave {:?}",
                name,
                preview.error
            );
            assert!(preview.tabs.is_empty());
        }
    }

    #[test]
    fn running_previews_refuse_names_that_leave_the_socket_folder() {
        for name in ["/tmp/zellij-preview-test", "../x", "a/b"] {
            let preview = running_session_preview(name, None, None, None);
            assert!(
                preview
                    .error
                    .as_deref()
                    .map(|e| e.contains("ession name"))
                    .unwrap_or(false),
                "{:?} gave {:?}",
                name,
                preview.error
            );
        }
    }

    #[test]
    fn a_missing_session_reports_an_error() {
        let preview = running_session_preview("no-such-session-for-preview-test", None, None, None);
        assert!(preview.error.is_some());
    }
}
