use crate::client_server_contract::client_server_contract as proto;
use crate::input::window::StartupMode;
use prost::Message;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

pub const APP_REQUEST_TIMEOUT: Duration = Duration::from_millis(2000);
const MAX_MESSAGE: usize = 4 * 1024 * 1024;
const UNKNOWN_REQUEST: &str =
    "The running Zellij app does not know this request; it is probably older than this command. Restart the app to update it.";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PopupView {
    Inbox,
    Sessions,
    Quit,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OpenWindowRequest {
    pub session: Option<String>,
    pub create: bool,
    pub pane_id: Option<(u32, bool)>,
    pub command: Vec<String>,
    pub cwd: Option<PathBuf>,
    pub close_on_exit: bool,
    pub start_suspended: bool,
    pub force_run_commands: bool,
    pub startup_mode: Option<StartupMode>,
    pub layout: Option<crate::data::LayoutInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppRequest {
    OpenWindow(OpenWindowRequest),
    ToggleTrayIcon,
    ShowPopup(PopupView),
    TogglePanel {
        view: PopupView,
        token: Option<String>,
    },
    Ping,
    Describe,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppReply {
    Done,
    Refused(String),
    State(AppState),
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AppState {
    pub windows: Vec<WindowState>,
    pub panel: Option<PanelState>,
    pub live_sessions: Vec<String>,
    pub tray_icon: bool,
    pub focused_session: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WindowState {
    pub session: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PanelState {
    pub view: Option<PopupView>,
    pub shown: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReadError {
    Closed,
    TooLarge(usize),
    Undecodable(String),
    Unknown,
}

impl std::fmt::Display for ReadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReadError::Closed => write!(f, "The app closed the connection without an answer"),
            ReadError::TooLarge(len) => write!(f, "The message is too large ({} bytes)", len),
            ReadError::Undecodable(e) => write!(f, "The message could not be read: {}", e),
            ReadError::Unknown => write!(f, "{}", UNKNOWN_REQUEST),
        }
    }
}

pub fn describe_app_at(socket: &Path) -> Result<AppState, String> {
    match send_app_request_to(socket, &AppRequest::Describe)? {
        AppReply::State(state) => Ok(state),
        AppReply::Refused(reason) => Err(reason),
        AppReply::Done => Err("The app did not describe itself".to_owned()),
    }
}

pub fn write_request(writer: &mut impl Write, request: &AppRequest) -> std::io::Result<()> {
    write_message(writer, &proto::AppControlRequest::from(request))
}

pub fn read_request(reader: &mut impl Read) -> Result<AppRequest, ReadError> {
    let message: proto::AppControlRequest = read_message(reader)?;
    AppRequest::try_from(message)
}

pub fn write_reply(writer: &mut impl Write, reply: &AppReply) -> std::io::Result<()> {
    write_message(writer, &proto::AppControlReply::from(reply))
}

pub fn read_reply(reader: &mut impl Read) -> Result<AppReply, ReadError> {
    let message: proto::AppControlReply = read_message(reader)?;
    AppReply::try_from(message)
}

pub fn refusal_for(error: &ReadError) -> AppReply {
    AppReply::Refused(match error {
        ReadError::Unknown => UNKNOWN_REQUEST.to_owned(),
        other => format!("The request could not be read: {}", other),
    })
}

fn write_message(writer: &mut impl Write, message: &impl Message) -> std::io::Result<()> {
    let encoded = message.encode_to_vec();
    writer.write_all(&(encoded.len() as u32).to_le_bytes())?;
    writer.write_all(&encoded)?;
    writer.flush()
}

fn read_message<T: Message + Default>(reader: &mut impl Read) -> Result<T, ReadError> {
    let mut len_bytes = [0u8; 4];
    reader
        .read_exact(&mut len_bytes)
        .map_err(|_| ReadError::Closed)?;
    let len = u32::from_le_bytes(len_bytes) as usize;
    if len > MAX_MESSAGE {
        return Err(ReadError::TooLarge(len));
    }
    let mut buf = vec![0u8; len];
    reader.read_exact(&mut buf).map_err(|_| ReadError::Closed)?;
    T::decode(&buf[..]).map_err(|e| ReadError::Undecodable(e.to_string()))
}

pub fn send_app_request(request: &AppRequest) -> Result<AppReply, String> {
    send_app_request_to(&crate::consts::app_control_socket(), request)
}

pub fn send_app_request_to(socket: &Path, request: &AppRequest) -> Result<AppReply, String> {
    use interprocess::local_socket::traits::Stream as _;
    let mut stream = crate::consts::ipc_connect(socket).map_err(|e| e.to_string())?;
    #[cfg(windows)]
    let mut reply_stream = crate::consts::ipc_connect_reply(socket).map_err(|e| e.to_string())?;
    #[cfg(not(windows))]
    let mut reply_stream = interprocess::TryClone::try_clone(&stream).map_err(|e| e.to_string())?;
    let _ = stream.set_recv_timeout(Some(APP_REQUEST_TIMEOUT));
    let _ = reply_stream.set_recv_timeout(Some(APP_REQUEST_TIMEOUT));
    write_request(&mut stream, request).map_err(|e| e.to_string())?;
    read_reply(&mut reply_stream).map_err(|e| e.to_string())
}

pub fn app_is_running() -> bool {
    matches!(send_app_request(&AppRequest::Ping), Ok(AppReply::Done))
}

pub fn describe_app() -> Result<AppState, String> {
    describe_app_at(&crate::consts::app_control_socket())
}

impl From<PopupView> for proto::AppPopupView {
    fn from(view: PopupView) -> Self {
        match view {
            PopupView::Inbox => proto::AppPopupView::Inbox,
            PopupView::Sessions => proto::AppPopupView::Sessions,
            PopupView::Quit => proto::AppPopupView::Quit,
        }
    }
}

fn popup_view(value: i32) -> Option<PopupView> {
    match proto::AppPopupView::try_from(value).ok()? {
        proto::AppPopupView::Inbox => Some(PopupView::Inbox),
        proto::AppPopupView::Sessions => Some(PopupView::Sessions),
        proto::AppPopupView::Quit => Some(PopupView::Quit),
        proto::AppPopupView::Unspecified => None,
    }
}

fn popup_view_value(view: Option<PopupView>) -> i32 {
    view.map(proto::AppPopupView::from)
        .unwrap_or(proto::AppPopupView::Unspecified) as i32
}

fn startup_mode_value(mode: Option<StartupMode>) -> i32 {
    (match mode {
        None => proto::AppStartupMode::Unspecified,
        Some(StartupMode::Windowed) => proto::AppStartupMode::Windowed,
        Some(StartupMode::Maximized) => proto::AppStartupMode::Maximized,
        Some(StartupMode::Fullscreen) => proto::AppStartupMode::Fullscreen,
        Some(StartupMode::Remember) => proto::AppStartupMode::Remember,
    }) as i32
}

fn startup_mode(value: i32) -> Option<StartupMode> {
    match proto::AppStartupMode::try_from(value).ok()? {
        proto::AppStartupMode::Unspecified => None,
        proto::AppStartupMode::Windowed => Some(StartupMode::Windowed),
        proto::AppStartupMode::Maximized => Some(StartupMode::Maximized),
        proto::AppStartupMode::Fullscreen => Some(StartupMode::Fullscreen),
        proto::AppStartupMode::Remember => Some(StartupMode::Remember),
    }
}

impl From<&OpenWindowRequest> for proto::AppOpenWindowRequest {
    fn from(request: &OpenWindowRequest) -> Self {
        proto::AppOpenWindowRequest {
            session: request.session.clone(),
            create: request.create,
            pane_id: request
                .pane_id
                .map(|(pane_id, is_plugin)| proto::PaneIdWithPlugin { pane_id, is_plugin }),
            command: request.command.clone(),
            cwd: request.cwd.as_ref().map(|cwd| cwd.display().to_string()),
            close_on_exit: request.close_on_exit,
            start_suspended: request.start_suspended,
            force_run_commands: request.force_run_commands,
            startup_mode: startup_mode_value(request.startup_mode),
            layout: request.layout.clone().map(Into::into),
        }
    }
}

impl TryFrom<proto::AppOpenWindowRequest> for OpenWindowRequest {
    type Error = ReadError;
    fn try_from(request: proto::AppOpenWindowRequest) -> Result<Self, ReadError> {
        let layout = request
            .layout
            .map(crate::data::LayoutInfo::try_from)
            .transpose()
            .map_err(|e| ReadError::Undecodable(e.to_string()))?;
        Ok(OpenWindowRequest {
            session: request.session,
            create: request.create,
            pane_id: request.pane_id.map(|pane| (pane.pane_id, pane.is_plugin)),
            command: request.command,
            cwd: request.cwd.map(PathBuf::from),
            close_on_exit: request.close_on_exit,
            start_suspended: request.start_suspended,
            force_run_commands: request.force_run_commands,
            startup_mode: startup_mode(request.startup_mode),
            layout,
        })
    }
}

impl From<&AppRequest> for proto::AppControlRequest {
    fn from(request: &AppRequest) -> Self {
        use proto::app_control_request::Request;
        let request = match request {
            AppRequest::OpenWindow(open) => Request::OpenWindow(open.into()),
            AppRequest::ToggleTrayIcon => {
                Request::ToggleTrayIcon(proto::AppToggleTrayIconRequest {})
            },
            AppRequest::ShowPopup(view) => Request::ShowPopup(proto::AppShowPopupRequest {
                view: popup_view_value(Some(*view)),
            }),
            AppRequest::TogglePanel { view, token } => {
                Request::TogglePanel(proto::AppTogglePanelRequest {
                    view: popup_view_value(Some(*view)),
                    token: token.clone(),
                })
            },
            AppRequest::Ping => Request::Ping(proto::AppPingRequest {}),
            AppRequest::Describe => Request::Describe(proto::AppDescribeRequest {}),
        };
        proto::AppControlRequest {
            request: Some(request),
        }
    }
}

impl TryFrom<proto::AppControlRequest> for AppRequest {
    type Error = ReadError;
    fn try_from(message: proto::AppControlRequest) -> Result<Self, ReadError> {
        use proto::app_control_request::Request;
        match message.request.ok_or(ReadError::Unknown)? {
            Request::OpenWindow(open) => Ok(AppRequest::OpenWindow(open.try_into()?)),
            Request::ToggleTrayIcon(_) => Ok(AppRequest::ToggleTrayIcon),
            Request::ShowPopup(show) => popup_view(show.view)
                .map(AppRequest::ShowPopup)
                .ok_or(ReadError::Unknown),
            Request::TogglePanel(toggle) => popup_view(toggle.view)
                .map(|view| AppRequest::TogglePanel {
                    view,
                    token: toggle.token,
                })
                .ok_or(ReadError::Unknown),
            Request::Ping(_) => Ok(AppRequest::Ping),
            Request::Describe(_) => Ok(AppRequest::Describe),
        }
    }
}

impl From<&AppReply> for proto::AppControlReply {
    fn from(reply: &AppReply) -> Self {
        use proto::app_control_reply::Reply;
        let reply = match reply {
            AppReply::Done => Reply::Done(proto::AppDoneReply {}),
            AppReply::Refused(reason) => Reply::Refused(proto::AppRefusedReply {
                reason: reason.clone(),
            }),
            AppReply::State(state) => Reply::State(proto::AppStateReply {
                windows: state
                    .windows
                    .iter()
                    .map(|window| proto::AppWindowState {
                        session: window.session.clone(),
                    })
                    .collect(),
                panel: state.panel.map(|panel| proto::AppPanelState {
                    view: popup_view_value(panel.view),
                    shown: panel.shown,
                }),
                live_sessions: state.live_sessions.clone(),
                tray_icon: state.tray_icon,
                focused_session: state.focused_session.clone(),
            }),
        };
        proto::AppControlReply { reply: Some(reply) }
    }
}

impl TryFrom<proto::AppControlReply> for AppReply {
    type Error = ReadError;
    fn try_from(message: proto::AppControlReply) -> Result<Self, ReadError> {
        use proto::app_control_reply::Reply;
        Ok(match message.reply.ok_or(ReadError::Unknown)? {
            Reply::Done(_) => AppReply::Done,
            Reply::Refused(refused) => AppReply::Refused(refused.reason),
            Reply::State(state) => AppReply::State(AppState {
                windows: state
                    .windows
                    .into_iter()
                    .map(|window| WindowState {
                        session: window.session,
                    })
                    .collect(),
                panel: state.panel.map(|panel| PanelState {
                    view: popup_view(panel.view),
                    shown: panel.shown,
                }),
                live_sessions: state.live_sessions,
                tray_icon: state.tray_icon,
                focused_session: state.focused_session,
            }),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request_roundtrip(request: &AppRequest) -> AppRequest {
        let mut wire = vec![];
        write_request(&mut wire, request).unwrap();
        read_request(&mut &wire[..]).unwrap()
    }

    fn reply_roundtrip(reply: &AppReply) -> AppReply {
        let mut wire = vec![];
        write_reply(&mut wire, reply).unwrap();
        read_reply(&mut &wire[..]).unwrap()
    }

    #[test]
    fn requests_and_replies_survive_the_wire() {
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
                startup_mode: Some(StartupMode::Maximized),
                layout: Some(crate::data::LayoutInfo::BuiltIn("compact".to_owned())),
            }),
            AppRequest::OpenWindow(OpenWindowRequest::default()),
            AppRequest::ToggleTrayIcon,
            AppRequest::ShowPopup(PopupView::Sessions),
            AppRequest::TogglePanel {
                view: PopupView::Inbox,
                token: Some("token".to_owned()),
            },
            AppRequest::TogglePanel {
                view: PopupView::Quit,
                token: None,
            },
            AppRequest::Ping,
            AppRequest::Describe,
        ];
        for request in requests {
            assert_eq!(request_roundtrip(&request), request);
        }
        let replies = vec![
            AppReply::Done,
            AppReply::Refused("no".to_owned()),
            AppReply::State(AppState {
                windows: vec![
                    WindowState {
                        session: Some("api".to_owned()),
                    },
                    WindowState { session: None },
                ],
                panel: Some(PanelState {
                    view: Some(PopupView::Inbox),
                    shown: true,
                }),
                live_sessions: vec!["api".to_owned()],
                tray_icon: false,
                focused_session: Some("api".to_owned()),
            }),
            AppReply::State(AppState {
                panel: Some(PanelState {
                    view: None,
                    shown: false,
                }),
                ..Default::default()
            }),
        ];
        for reply in replies {
            assert_eq!(reply_roundtrip(&reply), reply);
        }
    }

    #[test]
    fn a_closed_connection_or_an_oversized_message_is_an_error() {
        assert_eq!(read_reply(&mut &b""[..]), Err(ReadError::Closed));
        assert_eq!(
            read_reply(&mut &[3, 0, 0, 0, 1][..]),
            Err(ReadError::Closed)
        );
        let huge = (MAX_MESSAGE as u32 + 1).to_le_bytes();
        assert!(matches!(
            read_request(&mut &huge[..]),
            Err(ReadError::TooLarge(_))
        ));
    }

    fn framed(message: &impl Message) -> Vec<u8> {
        let mut wire = vec![];
        write_message(&mut wire, message).unwrap();
        wire
    }

    #[test]
    fn a_request_from_a_newer_client_is_refused_with_a_clear_reason() {
        let unknown_variant = framed(&proto::AppControlRequest { request: None });
        let error = read_request(&mut &unknown_variant[..]).unwrap_err();
        assert_eq!(error, ReadError::Unknown);
        assert_eq!(
            refusal_for(&error),
            AppReply::Refused(UNKNOWN_REQUEST.to_owned())
        );
        let unknown_view = framed(&proto::AppControlRequest {
            request: Some(proto::app_control_request::Request::ShowPopup(
                proto::AppShowPopupRequest { view: 99 },
            )),
        });
        assert_eq!(
            read_request(&mut &unknown_view[..]),
            Err(ReadError::Unknown)
        );
    }

    #[test]
    fn fields_a_newer_peer_adds_are_skipped_and_unknown_values_fall_back() {
        let mut wire = proto::AppControlRequest::from(&AppRequest::Ping).encode_to_vec();
        wire.extend([0xa2, 0x06, 0x03, b'n', b'e', b'w']);
        let mut framed = (wire.len() as u32).to_le_bytes().to_vec();
        framed.extend(wire);
        assert_eq!(read_request(&mut &framed[..]), Ok(AppRequest::Ping));
        let mut open: proto::AppOpenWindowRequest = (&OpenWindowRequest::default()).into();
        open.startup_mode = 42;
        let request = proto::AppControlRequest {
            request: Some(proto::app_control_request::Request::OpenWindow(open)),
        };
        match AppRequest::try_from(request).unwrap() {
            AppRequest::OpenWindow(open) => assert_eq!(open.startup_mode, None),
            other => panic!("{:?}", other),
        }
        let state = proto::AppControlReply {
            reply: Some(proto::app_control_reply::Reply::State(
                proto::AppStateReply {
                    panel: Some(proto::AppPanelState {
                        view: 77,
                        shown: true,
                    }),
                    ..Default::default()
                },
            )),
        };
        match AppReply::try_from(state).unwrap() {
            AppReply::State(state) => assert_eq!(
                state.panel,
                Some(PanelState {
                    view: None,
                    shown: true
                })
            ),
            other => panic!("{:?}", other),
        }
    }
}
