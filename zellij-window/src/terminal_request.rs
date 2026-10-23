use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Result};
use zellij_utils::app_control::{
    app_is_running, describe_app, send_app_request, AppReply, AppRequest, OpenWindowRequest,
};
use zellij_utils::data::PaneId;
use zellij_utils::input::actions::{initial_panes_from_cli, Action};
use zellij_utils::input::window::TerminalRequests;
use zellij_utils::ipc::{ClientToServerMsg, ExitReason, ServerToClientMsg};
use zellij_utils::session_link::{session_answers, SessionLink};

pub const FAILURE: i32 = 1;
const SESSION_START_WAIT: Duration = Duration::from_secs(30);
const PANE_APPEAR_WAIT: Duration = Duration::from_secs(10);
const APP_START_WAIT: Duration = Duration::from_secs(10);
const POLL: Duration = Duration::from_millis(100);
const ANSWER_WAIT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Waited {
    pub command: Vec<String>,
    pub cwd: PathBuf,
    pub hold: bool,
}

pub fn ensure_app_running() -> bool {
    if app_is_running() {
        return true;
    }
    let Ok(program) = std::env::current_exe() else {
        return false;
    };
    let mut command = std::process::Command::new(program);
    command
        .args(["window", "--background"])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        command.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
    }
    if command.spawn().is_err() {
        return false;
    }
    let deadline = Instant::now() + APP_START_WAIT;
    while Instant::now() < deadline {
        if app_is_running() {
            return true;
        }
        std::thread::sleep(POLL);
    }
    false
}

pub fn run_in_app(
    waited: &Waited,
    mode: TerminalRequests,
    request: OpenWindowRequest,
) -> Result<i32> {
    if mode == TerminalRequests::ExistingSession {
        if let Some(session) = focused_session() {
            match open_tab(&session, waited) {
                Ok(pane) => {
                    let raise = OpenWindowRequest {
                        session: Some(session.clone()),
                        create: false,
                        pane_id: Some((pane, false)),
                        ..Default::default()
                    };
                    if let Err(e) = handed(&raise) {
                        report!("the window for {} could not be raised: {}", session, e);
                    }
                    return Ok(wait_for_pane(&session, pane, waited.hold));
                },
                Err(e) => report!(
                    "the command could not open in {}, so it gets a session of its own: {:#}",
                    session,
                    e
                ),
            }
        }
    }
    let name = zellij_utils::sessions::generate_unique_session_name()
        .ok_or_else(|| anyhow!("Failed to generate a unique session name, giving up"))?;
    let open = OpenWindowRequest {
        session: Some(name.clone()),
        create: true,
        command: waited.command.clone(),
        cwd: Some(waited.cwd.clone()),
        close_on_exit: false,
        ..request
    };
    handed(&open).map_err(|e| anyhow!("{}", e))?;
    wait_in_new_session(&name, waited)
}

pub fn wait_in_new_session(name: &str, waited: &Waited) -> Result<i32> {
    if !wait_for_session(name, SESSION_START_WAIT) {
        bail!("The session {} did not start", name);
    }
    let pane = find_command_pane(name, &waited.command, None, PANE_APPEAR_WAIT)?;
    Ok(wait_for_pane(name, pane, waited.hold))
}

fn handed(request: &OpenWindowRequest) -> Result<(), String> {
    match send_app_request(&AppRequest::OpenWindow(request.clone()))? {
        AppReply::Refused(reason) => Err(reason),
        _ => Ok(()),
    }
}

fn focused_session() -> Option<String> {
    let state = describe_app().ok()?;
    state
        .focused_session
        .or_else(|| state.windows.iter().find_map(|w| w.session.clone()))
        .filter(|session| session_answers(session, ANSWER_WAIT))
}

pub fn wait_for_session(name: &str, limit: Duration) -> bool {
    let deadline = Instant::now() + limit;
    while Instant::now() < deadline {
        if session_answers(name, Duration::from_millis(500)) {
            return true;
        }
        std::thread::sleep(POLL);
    }
    false
}

fn send_and_collect(session: &str, action: Action) -> Result<Vec<String>> {
    let mut link = SessionLink::open(session).map_err(|e| anyhow!("{}", e))?;
    link.send(cli_message(action))
        .map_err(|e| anyhow!("{}", e))?;
    let mut lines = vec![];
    loop {
        match link.recv(ANSWER_WAIT).map_err(|e| anyhow!("{}", e))? {
            ServerToClientMsg::Log { lines: more } => lines.extend(more),
            ServerToClientMsg::LogError { lines } => bail!("{}", lines.join("\n")),
            ServerToClientMsg::UnblockInputThread | ServerToClientMsg::Exit { .. } => break,
            _ => {},
        }
    }
    let _ = link.send(ClientToServerMsg::ClientExited);
    Ok(lines)
}

fn cli_message(action: Action) -> ClientToServerMsg {
    ClientToServerMsg::Action {
        action,
        terminal_id: None,
        client_id: None,
        is_cli_client: true,
    }
}

fn open_tab(session: &str, waited: &Waited) -> Result<u32> {
    let initial_panes = initial_panes_from_cli(
        waited.command.clone(),
        None,
        Some(waited.cwd.clone()),
        waited.cwd.clone(),
        false,
        false,
    );
    let lines = send_and_collect(
        session,
        Action::NewTab {
            tiled_layout: None,
            floating_layouts: vec![],
            swap_tiled_layouts: None,
            swap_floating_layouts: None,
            tab_name: None,
            should_change_focus_to_new_tab: true,
            cwd: Some(waited.cwd.clone()),
            initial_panes,
            first_pane_unblock_condition: None,
            env: Default::default(),
        },
    )?;
    let tab_id = lines
        .iter()
        .find_map(|line| line.trim().parse::<usize>().ok());
    find_command_pane(session, &waited.command, tab_id, PANE_APPEAR_WAIT)
}

fn list_panes(session: &str) -> Result<Vec<serde_json::Value>> {
    let lines = send_and_collect(
        session,
        Action::ListPanes {
            show_tab: true,
            show_command: true,
            show_state: true,
            show_geometry: false,
            show_all: false,
            output_json: true,
        },
    )?;
    let text = lines.join("\n");
    let parsed: serde_json::Value = serde_json::from_str(&text)
        .map_err(|e| anyhow!("the pane list could not be read: {}", e))?;
    Ok(parsed.as_array().cloned().unwrap_or_default())
}

pub fn command_pane_in(
    panes: &[serde_json::Value],
    command: &[String],
    tab_id: Option<usize>,
) -> Option<u32> {
    let program = command
        .first()
        .map(|c| {
            Path::new(c)
                .file_name()
                .map(|f| f.to_string_lossy().into_owned())
        })
        .flatten()
        .unwrap_or_default();
    panes
        .iter()
        .filter(|pane| !pane["is_plugin"].as_bool().unwrap_or(true))
        .filter(|pane| {
            tab_id
                .map(|id| pane["tab_id"].as_u64() == Some(id as u64))
                .unwrap_or(true)
        })
        .filter(|pane| {
            pane["terminal_command"]
                .as_str()
                .map(|text| text.contains(&program))
                .unwrap_or(false)
        })
        .filter_map(|pane| pane["id"].as_u64().map(|id| id as u32))
        .min()
}

fn find_command_pane(
    session: &str,
    command: &[String],
    tab_id: Option<usize>,
    limit: Duration,
) -> Result<u32> {
    let deadline = Instant::now() + limit;
    loop {
        if let Ok(panes) = list_panes(session) {
            if let Some(pane) = command_pane_in(&panes, command, tab_id) {
                return Ok(pane);
            }
        }
        if Instant::now() >= deadline {
            bail!("The command's pane did not appear in {}", session);
        }
        std::thread::sleep(POLL);
    }
}

pub fn wait_for_pane(session: &str, pane: u32, hold: bool) -> i32 {
    let code = exit_code_of(session, pane).unwrap_or_else(|e| {
        report!("the command's pane could not be followed: {:#}", e);
        FAILURE
    });
    if !hold {
        let _ = send_and_collect(
            session,
            Action::CloseFocusByPaneId {
                pane_id: PaneId::Terminal(pane),
            },
        );
    }
    code
}

fn exit_code_of(session: &str, pane: u32) -> Result<i32> {
    let mut link = SessionLink::open(session).map_err(|e| anyhow!("{}", e))?;
    link.send(cli_message(Action::WaitForPaneExit {
        pane_id: PaneId::Terminal(pane),
    }))
    .map_err(|e| anyhow!("{}", e))?;
    loop {
        match link.recv_blocking() {
            Ok(ServerToClientMsg::Exit { exit_reason }) => {
                let _ = link.send(ClientToServerMsg::ClientExited);
                return Ok(match exit_reason {
                    ExitReason::CustomExitStatus(code) => code,
                    _ => FAILURE,
                });
            },
            Ok(ServerToClientMsg::LogError { lines }) => {
                let _ = link.send(ClientToServerMsg::ClientExited);
                bail!("{}", lines.join("\n"));
            },
            Ok(_) => {},
            Err(_) => return Ok(FAILURE),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_command_pane_is_found_by_its_program_and_tab() {
        let panes = vec![
            json!({"id": 0, "is_plugin": true, "tab_id": 0, "terminal_command": null}),
            json!({"id": 0, "is_plugin": false, "tab_id": 0, "terminal_command": null}),
            json!({"id": 3, "is_plugin": false, "tab_id": 1, "terminal_command": "/usr/bin/htop -d 5"}),
            json!({"id": 5, "is_plugin": false, "tab_id": 2, "terminal_command": "htop"}),
        ];
        let command = vec!["htop".to_owned(), "-d".to_owned(), "5".to_owned()];
        assert_eq!(command_pane_in(&panes, &command, None), Some(3));
        assert_eq!(command_pane_in(&panes, &command, Some(2)), Some(5));
        assert_eq!(
            command_pane_in(&panes, &["vim".to_owned()], None),
            None,
            "a shell pane is never mistaken for the command"
        );
    }
}
