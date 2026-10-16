use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

pub const APP_REQUEST_TIMEOUT: Duration = Duration::from_millis(2000);
const MAX_LINE: u64 = 64 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PopupView {
    Inbox,
    Sessions,
    Quit,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenWindowRequest {
    pub session: Option<String>,
    pub create: bool,
    pub pane_id: Option<(u32, bool)>,
    pub command: Vec<String>,
    pub cwd: Option<PathBuf>,
    pub close_on_exit: bool,
    pub start_suspended: bool,
    pub force_run_commands: bool,
    pub startup_mode: Option<crate::input::window::StartupMode>,
    #[serde(default)]
    pub layout: Option<crate::data::LayoutInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AppRequest {
    OpenWindow(OpenWindowRequest),
    ToggleTrayIcon,
    ShowPopup(PopupView),
    TogglePanel {
        view: PopupView,
        token: Option<String>,
    },
    Ping,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AppReply {
    Done,
    Refused(String),
}

pub fn encode_line<T: Serialize>(value: &T) -> Result<Vec<u8>, String> {
    let mut line = serde_json::to_vec(value).map_err(|e| e.to_string())?;
    line.push(b'\n');
    Ok(line)
}

pub fn read_line<T: for<'de> Deserialize<'de>>(reader: impl std::io::Read) -> Result<T, String> {
    let mut line = String::new();
    BufReader::new(std::io::Read::take(reader, MAX_LINE))
        .read_line(&mut line)
        .map_err(|e| e.to_string())?;
    if line.trim().is_empty() {
        return Err("The app closed the connection without an answer".to_owned());
    }
    serde_json::from_str(line.trim()).map_err(|e| e.to_string())
}

pub fn send_app_request(request: &AppRequest) -> Result<AppReply, String> {
    send_app_request_to(&crate::consts::app_control_socket(), request)
}

pub fn send_app_request_to(socket: &Path, request: &AppRequest) -> Result<AppReply, String> {
    use interprocess::local_socket::traits::Stream as _;
    let mut stream = crate::consts::ipc_connect(socket).map_err(|e| e.to_string())?;
    #[cfg(windows)]
    let reply_stream = crate::consts::ipc_connect_reply(socket).map_err(|e| e.to_string())?;
    #[cfg(not(windows))]
    let reply_stream = interprocess::TryClone::try_clone(&stream).map_err(|e| e.to_string())?;
    let _ = stream.set_recv_timeout(Some(APP_REQUEST_TIMEOUT));
    let _ = reply_stream.set_recv_timeout(Some(APP_REQUEST_TIMEOUT));
    stream
        .write_all(&encode_line(request)?)
        .map_err(|e| e.to_string())?;
    stream.flush().map_err(|e| e.to_string())?;
    read_line(reply_stream)
}

pub fn app_is_running() -> bool {
    matches!(send_app_request(&AppRequest::Ping), Ok(AppReply::Done))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_and_replies_survive_a_line() {
        let requests = vec![
            AppRequest::OpenWindow(OpenWindowRequest {
                session: Some("api".to_owned()),
                create: false,
                pane_id: Some((3, false)),
                command: vec!["htop".to_owned()],
                cwd: Some(PathBuf::from("/tmp")),
                close_on_exit: true,
                start_suspended: false,
                force_run_commands: true,
                startup_mode: Some(crate::input::window::StartupMode::Maximized),
                layout: Some(crate::data::LayoutInfo::BuiltIn("compact".to_owned())),
            }),
            AppRequest::ToggleTrayIcon,
            AppRequest::ShowPopup(PopupView::Sessions),
            AppRequest::TogglePanel {
                view: PopupView::Inbox,
                token: Some("token".to_owned()),
            },
            AppRequest::Ping,
        ];
        for request in requests {
            let line = encode_line(&request).unwrap();
            assert_eq!(line.last(), Some(&b'\n'));
            let back: AppRequest = read_line(&line[..]).unwrap();
            assert_eq!(back, request);
        }
        let reply: AppReply =
            read_line(&encode_line(&AppReply::Refused("no".into())).unwrap()[..]).unwrap();
        assert_eq!(reply, AppReply::Refused("no".to_owned()));
    }

    #[test]
    fn an_empty_answer_is_an_error() {
        assert!(read_line::<AppReply>(&b""[..]).is_err());
    }
}
