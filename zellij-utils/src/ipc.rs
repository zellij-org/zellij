//! IPC stuff for starting to split things into a client and server model.
use crate::{
    data::{ClientId, ConnectToSession, HostTerminalThemeMode, KeyWithModifier, PaneId, Style},
    errors::{prelude::*, ErrorContext},
    input::{actions::Action, cli_assets::CliAssets},
    pane_size::{Size, SizeInPixels},
};
use interprocess::local_socket::Stream as LocalSocketStream;
use log::warn;
use serde::{Deserialize, Serialize};
use std::{
    fmt::{Display, Error, Formatter},
    io::{self, Read, Write},
    marker::PhantomData,
};

// Protobuf imports
use crate::client_server_contract::client_server_contract::{
    ClientToServerMsg as ProtoClientToServerMsg, ServerToClientMsg as ProtoServerToClientMsg,
};
use prost::Message;

mod enum_conversions;
mod protobuf_conversion;

#[cfg(test)]
mod tests;

type SessionId = u64;

/// A bidirectional byte stream that supports cloning for simultaneous read/write.
pub trait IpcStream: Read + Write + Send + 'static {
    fn try_clone_stream(&self) -> io::Result<Box<dyn IpcStream>>;
}

impl IpcStream for LocalSocketStream {
    fn try_clone_stream(&self) -> io::Result<Box<dyn IpcStream>> {
        use interprocess::TryClone;
        Ok(Box::new(self.try_clone()?))
    }
}

#[derive(PartialEq, Eq, Serialize, Deserialize, Hash)]
pub struct Session {
    // Unique ID for this session
    id: SessionId,
    // Identifier for the underlying IPC primitive (socket, pipe)
    conn_name: String,
    // User configured alias for the session
    alias: String,
}

// How do we want to connect to a session?
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClientType {
    Reader,
    Writer,
}

#[derive(Default, Serialize, Deserialize, Debug, Clone)]
pub struct ClientAttributes {
    pub size: Size,
    pub style: Style,
}

#[derive(Default, Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub struct PixelDimensions {
    pub text_area_size: Option<SizeInPixels>,
    pub character_cell_size: Option<SizeInPixels>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq, Eq)]
pub struct PaneReference {
    pub pane_id: u32,
    pub is_plugin: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq, Eq)]
pub struct ColorRegister {
    pub index: usize,
    pub color: String,
}

impl PixelDimensions {
    pub fn merge(&mut self, other: PixelDimensions) {
        if let Some(text_area_size) = other.text_area_size {
            self.text_area_size = Some(text_area_size);
        }
        if let Some(character_cell_size) = other.character_cell_size {
            self.character_cell_size = Some(character_cell_size);
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct MobileSizePayload {
    pub cols: usize,
    pub rows: usize,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct MobileActivePanePayload {
    pub pane_id: u32,
    pub is_plugin: bool,
    pub tab_position: usize,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct MobileTabPayload {
    pub position: usize,
    pub name: String,
    pub active: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct MobilePanePayload {
    pub tab_position: usize,
    pub pane_id: u32,
    pub is_plugin: bool,
    pub title: String,
    pub is_floating: bool,
    pub last_activity_secs_ago: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct MobileSessionPayload {
    pub name: String,
    pub web_clients_allowed: bool,
    pub tab_count: usize,
    pub pane_count: usize,
    pub connected_clients: usize,
    pub creation_secs_ago: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct MobileRenderPrefsPayload {
    pub single_pane: bool,
    pub fit: bool,
    pub active_pane_is_fullscreen: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct MobileStatePayload {
    pub session_name: String,
    pub now_secs: u64,
    pub is_welcome_screen: bool,
    pub desktop_client_connected: bool,
    pub desktop_size: Option<MobileSizePayload>,
    pub active_pane: Option<MobileActivePanePayload>,
    pub tabs: Vec<MobileTabPayload>,
    pub panes: Vec<MobilePanePayload>,
    pub sessions: Vec<MobileSessionPayload>,
    pub render_prefs: MobileRenderPrefsPayload,
}

// Types of messages sent from the client to the server
#[allow(clippy::large_enum_variant)]
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum ClientToServerMsg {
    DetachSession {
        client_ids: Vec<ClientId>,
    },
    TerminalPixelDimensions {
        pixel_dimensions: PixelDimensions,
    },
    BackgroundColor {
        color: String,
    },
    ForegroundColor {
        color: String,
    },
    ColorRegisters {
        color_registers: Vec<ColorRegister>,
    },
    TerminalResize {
        new_size: Size,
    },
    FirstClientConnected {
        cli_assets: CliAssets,
        is_web_client: bool,
    },
    AttachClient {
        cli_assets: CliAssets,
        tab_position_to_focus: Option<usize>,
        pane_to_focus: Option<PaneReference>,
        is_web_client: bool,
    },
    AttachWatcherClient {
        terminal_size: Size,
        is_web_client: bool,
    },
    Action {
        action: Action,
        terminal_id: Option<u32>,
        client_id: Option<ClientId>,
        is_cli_client: bool,
    },
    Key {
        key: KeyWithModifier,
        raw_bytes: Vec<u8>,
        is_kitty_keyboard_protocol: bool,
    },
    ClientExited,
    KillSession,
    ConnStatus,
    WebServerStarted {
        base_url: String,
    },
    FailedToStartWebServer {
        error: String,
    },
    SubscribeToPaneRenders {
        pane_ids: Vec<PaneId>,
        scrollback: Option<usize>,
        ansi: bool,
    },
    DesktopNotificationResponse {
        raw_bytes: Vec<u8>,
    },
    ForwardedReplyFromHost {
        token: u32,
        reply_bytes: Vec<u8>,
    },
    HostTerminalThemeChanged {
        mode: HostTerminalThemeMode,
    },
    SoftKeyboardVisibilityChanged {
        visible: bool,
    },
    NestedSessionFrameFromHost {
        payload_bytes: Vec<u8>,
    },
    KittyGraphicsSupport {
        supported: bool,
    },
    SixelSupport {
        supported: bool,
    },
    RequestSessionList,
    SetMobileRenderPreferences {
        single_pane: bool,
        fit: bool,
    },
    HostTerminalFocusChanged {
        focused: bool,
    },
}

// Types of messages sent from the server to the client
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum ServerToClientMsg {
    Render {
        content: String,
    },
    UnblockInputThread,
    Exit {
        exit_reason: ExitReason,
    },
    Connected,
    Log {
        lines: Vec<String>,
    },
    LogError {
        lines: Vec<String>,
    },
    SwitchSession {
        connect_to_session: ConnectToSession,
    },
    UnblockCliPipeInput {
        pipe_name: String,
    },
    CliPipeOutput {
        pipe_name: String,
        output: String,
    },
    QueryTerminalSize,
    SetSoftKeyboard {
        on: bool,
    },
    StartWebServer,
    RenamedSession {
        name: String,
    },
    ConfigFileUpdated,
    PaneRenderUpdate {
        pane_id: PaneId,
        viewport: Vec<String>,
        scrollback: Option<Vec<String>>,
        is_initial: bool,
    },
    SubscribedPaneClosed {
        pane_id: PaneId,
    },
    ForwardQueryToHost {
        token: u32,
        query_bytes: Vec<u8>,
        resolve_async: bool,
    },
    EmitNestedSessionFrame {
        payload_bytes: Vec<u8>,
    },
    MobileState {
        payload: MobileStatePayload,
    },
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum ExitReason {
    Normal,
    NormalDetached,
    ForceDetached,
    CannotAttach,
    Disconnect,
    WebClientsForbidden,
    KickedByHost,
    CustomExitStatus(i32),
    Error(String),
}

impl Display for ExitReason {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        match self {
            Self::Normal => write!(f, "Bye from Zellij!"),
            Self::NormalDetached => write!(f, "Session detached"),
            Self::ForceDetached => write!(
                f,
                "Session was detached from this client (possibly because another client connected)"
            ),
            Self::CannotAttach => write!(
                f,
                "Session attached to another client. Use --force flag to force connect."
            ),
            Self::WebClientsForbidden => write!(
                f,
                "Web clients are not allowed in this session - cannot attach"
            ),
            Self::Disconnect => {
                let session_tip = match crate::envs::get_session_name() {
                    Ok(name) => format!("`zellij attach {}`", name),
                    Err(_) => "see `zellij ls` and `zellij attach`".to_string(),
                };
                write!(
                    f,
                    "
Your zellij client lost connection to the zellij server.

As a safety measure, you have been disconnected from the current zellij session.
However, the session should still exist and none of your data should be lost.

This usually means that your terminal didn't process server messages quick
enough. Maybe your system is currently under high load, or your terminal
isn't performant enough.

There are a few things you can try now:
    - Reattach to your previous session and see if it works out better this
      time: {session_tip}
    - Try using a faster (maybe GPU-accelerated) terminal emulator
    "
                )
            },
            Self::KickedByHost => write!(f, "Disconnected by host"),
            Self::CustomExitStatus(exit_status) => write!(f, "Exit {}", exit_status),
            Self::Error(e) => write!(f, "Error occurred in server:\n{}", e),
        }
    }
}

/// Sends messages on a stream socket, along with an [`ErrorContext`].
pub struct IpcSenderWithContext<T: Serialize> {
    sender: io::BufWriter<Box<dyn IpcStream>>,
    _phantom: PhantomData<T>,
}

impl<T: Serialize> IpcSenderWithContext<T> {
    /// Returns a sender to the given [LocalSocketStream](interprocess::local_socket::LocalSocketStream).
    pub fn new(sender: LocalSocketStream) -> Self {
        Self {
            sender: io::BufWriter::new(Box::new(sender)),
            _phantom: PhantomData,
        }
    }

    fn from_boxed(sender: Box<dyn IpcStream>) -> Self {
        Self {
            sender: io::BufWriter::new(sender),
            _phantom: PhantomData,
        }
    }

    pub fn send_client_msg(&mut self, msg: ClientToServerMsg) -> Result<()> {
        let proto_msg: ProtoClientToServerMsg = msg.into();
        write_protobuf_message(&mut self.sender, &proto_msg)?;
        let _ = self.sender.flush();
        Ok(())
    }

    pub fn send_server_msg(&mut self, msg: ServerToClientMsg) -> Result<()> {
        let proto_msg: ProtoServerToClientMsg = msg.into();
        write_protobuf_message(&mut self.sender, &proto_msg)?;
        let _ = self.sender.flush();
        Ok(())
    }

    /// Returns an [`IpcReceiverWithContext`] with the same socket as this sender.
    pub fn get_receiver<F>(&self) -> IpcReceiverWithContext<F>
    where
        F: for<'de> Deserialize<'de> + Serialize,
    {
        let socket = self.sender.get_ref().try_clone_stream().unwrap();
        IpcReceiverWithContext::from_boxed(socket)
    }
}

/// Receives messages on a stream socket, along with an [`ErrorContext`].
pub struct IpcReceiverWithContext<T> {
    receiver: io::BufReader<Box<dyn IpcStream>>,
    _phantom: PhantomData<T>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IpcReceiveError {
    Disconnected,
    Undecodable,
}

impl Display for IpcReceiveError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::result::Result<(), Error> {
        match self {
            IpcReceiveError::Disconnected => write!(f, "the peer closed the connection"),
            IpcReceiveError::Undecodable => write!(f, "received a message that could not be read"),
        }
    }
}

impl<T> IpcReceiverWithContext<T>
where
    T: for<'de> Deserialize<'de> + Serialize,
{
    /// Returns a receiver to the given [LocalSocketStream](interprocess::local_socket::LocalSocketStream).
    pub fn new(receiver: LocalSocketStream) -> Self {
        Self {
            receiver: io::BufReader::new(Box::new(receiver)),
            _phantom: PhantomData,
        }
    }

    fn from_boxed(receiver: Box<dyn IpcStream>) -> Self {
        Self {
            receiver: io::BufReader::new(receiver),
            _phantom: PhantomData,
        }
    }

    pub fn recv_client_msg(&mut self) -> Option<(ClientToServerMsg, ErrorContext)> {
        self.try_recv_client_msg().ok()
    }

    pub fn recv_server_msg(&mut self) -> Option<(ServerToClientMsg, ErrorContext)> {
        self.try_recv_server_msg().ok()
    }

    pub fn try_recv_client_msg(
        &mut self,
    ) -> std::result::Result<(ClientToServerMsg, ErrorContext), IpcReceiveError> {
        let proto_msg = read_protobuf_message::<ProtoClientToServerMsg>(&mut self.receiver)?;
        match proto_msg.try_into() {
            Ok(rust_msg) => Ok((rust_msg, ErrorContext::default())),
            Err(e) => {
                warn!("Error converting protobuf to ClientToServerMsg: {:?}", e);
                Err(IpcReceiveError::Undecodable)
            },
        }
    }

    pub fn try_recv_server_msg(
        &mut self,
    ) -> std::result::Result<(ServerToClientMsg, ErrorContext), IpcReceiveError> {
        let proto_msg = read_protobuf_message::<ProtoServerToClientMsg>(&mut self.receiver)?;
        match proto_msg.try_into() {
            Ok(rust_msg) => Ok((rust_msg, ErrorContext::default())),
            Err(e) => {
                warn!("Error converting protobuf to ServerToClientMsg: {:?}", e);
                Err(IpcReceiveError::Undecodable)
            },
        }
    }

    /// Returns an [`IpcSenderWithContext`] with the same socket as this receiver.
    pub fn get_sender<F: Serialize>(&self) -> IpcSenderWithContext<F> {
        let socket = self.receiver.get_ref().try_clone_stream().unwrap();
        IpcSenderWithContext::from_boxed(socket)
    }
}

// Protobuf wire format utilities
fn read_protobuf_message<T: Message + Default>(
    reader: &mut impl Read,
) -> std::result::Result<T, IpcReceiveError> {
    // Read length-prefixed protobuf message
    let mut len_bytes = [0u8; 4];
    reader
        .read_exact(&mut len_bytes)
        .map_err(|_| IpcReceiveError::Disconnected)?;
    let len = u32::from_le_bytes(len_bytes) as usize;

    let mut buf = vec![0u8; len];
    reader
        .read_exact(&mut buf)
        .map_err(|_| IpcReceiveError::Disconnected)?;

    T::decode(&buf[..]).map_err(|_| IpcReceiveError::Undecodable)
}

fn write_protobuf_message<T: Message>(writer: &mut impl Write, msg: &T) -> Result<()> {
    let encoded = msg.encode_to_vec();
    let len = encoded.len() as u32;

    // we measure the length of the message and transmit it first so that the reader will be able
    // to first read exactly 4 bytes (representing this length) and then read that amount of bytes
    // as the actual message - this is so that we are able to distinct whole messages over the wire
    // stream
    writer.write_all(&len.to_le_bytes())?;
    writer.write_all(&encoded)?;
    Ok(())
}

// Protobuf helper functions
pub fn send_protobuf_client_to_server(
    sender: &mut IpcSenderWithContext<ClientToServerMsg>,
    msg: ClientToServerMsg,
) -> Result<()> {
    let proto_msg: ProtoClientToServerMsg = msg.into();
    write_protobuf_message(&mut sender.sender, &proto_msg)?;
    let _ = sender.sender.flush();
    Ok(())
}

pub fn send_protobuf_server_to_client(
    sender: &mut IpcSenderWithContext<ServerToClientMsg>,
    msg: ServerToClientMsg,
) -> Result<()> {
    let proto_msg: ProtoServerToClientMsg = msg.into();
    write_protobuf_message(&mut sender.sender, &proto_msg)?;
    let _ = sender.sender.flush();
    Ok(())
}

pub fn recv_protobuf_client_to_server(
    receiver: &mut IpcReceiverWithContext<ClientToServerMsg>,
) -> Option<(ClientToServerMsg, ErrorContext)> {
    receiver.try_recv_client_msg().ok()
}

pub fn recv_protobuf_server_to_client(
    receiver: &mut IpcReceiverWithContext<ServerToClientMsg>,
) -> Option<(ServerToClientMsg, ErrorContext)> {
    receiver.try_recv_server_msg().ok()
}

/// Asynchronously send `ClientToServerMsg::KillSession` to the peer at `path`
/// and wait until the peer's existing shutdown path replies (or its socket
/// closes). Either of those outcomes confirms the kill landed; the caller
/// wraps this in `tokio::time::timeout` to bound the wait against a wedged
/// peer.
///
/// On Unix the local socket is bidirectional, so the same async stream is
/// used for both send and receive. On Windows the named pipe is half-duplex
/// and the existing sync `ipc_connect` / `ipc_connect_reply` flow is
/// dispatched onto a blocking task.
#[cfg(unix)]
pub async fn async_send_kill_and_await(path: &std::path::Path) -> io::Result<()> {
    use interprocess::local_socket::traits::tokio::Stream as _;
    use interprocess::local_socket::{prelude::*, GenericFilePath};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let fs_name = path.to_fs_name::<GenericFilePath>()?;
    let mut stream: interprocess::local_socket::tokio::Stream =
        interprocess::local_socket::tokio::Stream::connect(fs_name).await?;

    let proto_msg: ProtoClientToServerMsg = crate::ipc::ClientToServerMsg::KillSession.into();
    let encoded = proto_msg.encode_to_vec();
    let len_bytes = (encoded.len() as u32).to_le_bytes();

    stream.write_all(&len_bytes).await?;
    stream.write_all(&encoded).await?;
    // Best-effort flush; failing here doesn't mean the kill failed.
    let _ = stream.flush().await;

    // The peer's shutdown path sends `ServerToClientMsg::Exit { Normal }`
    // (zellij-server/src/lib.rs ServerInstruction::KillSession) over this
    // same socket before exiting; if it dies without ACKing, the stream
    // closes. Either outcome -- a successful 4-byte length-prefix read or a
    // read error/EOF -- confirms the kill is no longer in flight.
    let mut len_buf = [0u8; 4];
    let _ = stream.read_exact(&mut len_buf).await;
    Ok(())
}

#[cfg(windows)]
pub async fn async_send_kill_and_await(path: &std::path::Path) -> io::Result<()> {
    let path = path.to_path_buf();
    tokio::task::spawn_blocking(move || {
        use crate::consts::{ipc_connect, ipc_connect_reply};
        let stream = ipc_connect(&path)?;
        let reply = ipc_connect_reply(&path);
        let mut sender = IpcSenderWithContext::<ClientToServerMsg>::new(stream);
        sender
            .send_client_msg(ClientToServerMsg::KillSession)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;
        if let Ok(reply_stream) = reply {
            let mut receiver: IpcReceiverWithContext<ServerToClientMsg> =
                IpcReceiverWithContext::new(reply_stream);
            let _ = receiver.recv_server_msg();
        }
        Ok::<(), io::Error>(())
    })
    .await
    .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?
}
