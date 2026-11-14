//! IPC stuff for starting to split things into a client and server model.
use crate::{
    data::{ClientId, ConnectToSession, HostTerminalThemeMode, KeyWithModifier, PaneId, Style},
    errors::{prelude::*, ErrorContext},
    input::{actions::Action, cli_assets::CliAssets, terminal::TerminalSettings},
    pane_size::{Size, SizeInPixels},
};
use interprocess::local_socket::Stream as LocalSocketStream;
use log::warn;
use serde::{Deserialize, Serialize};
use std::{
    fmt::{Display, Error, Formatter},
    io::{self, Read, Write},
    marker::PhantomData,
    time::{Duration, Instant},
};

// Protobuf imports
use crate::client_server_contract::client_server_contract::{
    ClientToServerMsg as ProtoClientToServerMsg, ServerToClientMsg as ProtoServerToClientMsg,
};
use prost::Message;

mod attention_conversion;
mod enum_conversions;
mod protobuf_conversion;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod handshake_frame_tests {
    use super::{decode_client_to_server_frame, ClientToServerMsg, ProtoClientToServerMsg};
    use prost::Message;

    #[test]
    fn decode_client_to_server_frame_accepts_a_small_body_and_rejects_garbage() {
        let proto: ProtoClientToServerMsg = ClientToServerMsg::ConnStatus.into();
        let bytes = proto.encode_to_vec();
        let decoded = decode_client_to_server_frame(&bytes).unwrap();
        assert!(matches!(decoded, ClientToServerMsg::ConnStatus));
        assert!(decode_client_to_server_frame(&[0xff, 0xff, 0x00]).is_err());
    }
}

type SessionId = u64;
const MAX_FRAME_BYTES: usize = 256 * 1024 * 1024;
/// Cap for a health-probe or kill-reply frame. `Connected` and `Exit` are tiny.
/// This does not change the generic [`MAX_FRAME_BYTES`] limit used by normal IPC.
pub const PROBE_MAX_FRAME_BYTES: usize = 64 * 1024;

/// A bidirectional byte stream that supports cloning for simultaneous read/write.
pub trait IpcStream: Read + Write + Send + 'static {
    fn try_clone_stream(&self) -> io::Result<Box<dyn IpcStream>>;
    fn set_read_timeout(&self, timeout: Option<Duration>) -> io::Result<()>;
    /// Shut down both directions of the underlying connection, which
    /// fails any read or write blocked on it, on this handle or any clone
    /// of it. Streams that cannot do this do nothing.
    ///
    /// Implemented for Unix sockets and for the server end of a Windows named
    /// pipe. Windows named-pipe clients cannot disconnect the server's end;
    /// the IPC writer uses [`BlockingIoCancel`] for the synchronous I/O
    /// blocked on its thread.
    fn shutdown_stream(&self) -> io::Result<()> {
        Ok(())
    }

    /// Arm `SO_SNDTIMEO` for the next write. `None` restores the default
    /// (block forever). Streams that cannot do this return an error so a
    /// caller does not mistake an unbounded write for a deadline.
    fn set_write_timeout(&self, timeout: Option<Duration>) -> io::Result<()> {
        let _ = timeout;
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "this ipc stream has no write deadline",
        ))
    }
}

/// Cancels synchronous I/O blocked on a thread.
///
/// Windows named-pipe handles used by the IPC layer are synchronous, so
/// `CancelIoEx` cannot interrupt an in-flight `ReadFile` or `WriteFile`.
/// `CancelSynchronousIo` targets the blocked thread instead. Obtain one from
/// the thread that performs the I/O, then call
/// [`Self::cancel_blocking_io`] from the thread that gives up on it.
///
/// This is a no-op outside Windows, where socket timeouts already bound the
/// same operations.
#[derive(Debug)]
pub struct BlockingIoCancel {
    #[cfg(windows)]
    thread: windows_sys::Win32::Foundation::HANDLE,
}

#[cfg(windows)]
unsafe impl Send for BlockingIoCancel {}
#[cfg(windows)]
unsafe impl Sync for BlockingIoCancel {}

impl BlockingIoCancel {
    /// Captures a duplicate of the calling thread's handle.
    pub fn for_current_thread() -> io::Result<Self> {
        #[cfg(windows)]
        {
            use windows_sys::Win32::Foundation::{DuplicateHandle, DUPLICATE_SAME_ACCESS};
            use windows_sys::Win32::System::Threading::{GetCurrentProcess, GetCurrentThread};
            let mut thread = std::ptr::null_mut();
            let ok = unsafe {
                DuplicateHandle(
                    GetCurrentProcess(),
                    GetCurrentThread(),
                    GetCurrentProcess(),
                    &mut thread,
                    0,
                    0,
                    DUPLICATE_SAME_ACCESS,
                )
            };
            if ok == 0 {
                Err(io::Error::last_os_error())
            } else {
                Ok(Self { thread })
            }
        }
        #[cfg(not(windows))]
        {
            Ok(Self {})
        }
    }

    /// Cancels whichever synchronous I/O the captured thread is blocked in.
    /// No pending operation is success: the write may already have finished.
    pub fn cancel_blocking_io(&self) -> io::Result<()> {
        #[cfg(windows)]
        {
            use windows_sys::Win32::Foundation::ERROR_NOT_FOUND;
            use windows_sys::Win32::System::IO::CancelSynchronousIo;
            if unsafe { CancelSynchronousIo(self.thread) } != 0 {
                return Ok(());
            }
            let error = io::Error::last_os_error();
            if error.raw_os_error() == Some(ERROR_NOT_FOUND as i32) {
                Ok(())
            } else {
                Err(error)
            }
        }
        #[cfg(not(windows))]
        {
            Ok(())
        }
    }
}

#[cfg(windows)]
impl Drop for BlockingIoCancel {
    fn drop(&mut self) {
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(self.thread);
        }
    }
}

impl IpcStream for LocalSocketStream {
    fn try_clone_stream(&self) -> io::Result<Box<dyn IpcStream>> {
        use interprocess::TryClone;
        Ok(Box::new(self.try_clone()?))
    }
    fn set_read_timeout(&self, timeout: Option<Duration>) -> io::Result<()> {
        use interprocess::local_socket::traits::Stream;
        self.set_recv_timeout(timeout)
    }
    #[cfg(unix)]
    fn set_write_timeout(&self, timeout: Option<Duration>) -> io::Result<()> {
        use interprocess::local_socket::traits::Stream;
        self.set_send_timeout(timeout)
    }
    #[cfg(unix)]
    fn shutdown_stream(&self) -> io::Result<()> {
        let LocalSocketStream::UdSocket(stream) = self;
        stream.inner().shutdown(std::net::Shutdown::Both)
    }
    #[cfg(windows)]
    fn shutdown_stream(&self) -> io::Result<()> {
        use std::os::windows::io::{AsHandle, AsRawHandle};
        use windows_sys::Win32::Foundation::{ERROR_BROKEN_PIPE, ERROR_PIPE_NOT_CONNECTED};
        use windows_sys::Win32::System::Pipes::DisconnectNamedPipe;

        let LocalSocketStream::NamedPipe(pipe) = self;
        let inner = pipe.inner();
        if !inner.is_server() {
            // A client handle cannot be disconnected. Cancelling a blocked
            // write uses BlockingIoCancel on the writer thread instead.
            return Ok(());
        }
        if unsafe { DisconnectNamedPipe(inner.as_handle().as_raw_handle()) } != 0 {
            return Ok(());
        }
        let error = io::Error::last_os_error();
        match error.raw_os_error() {
            Some(code)
                if code == ERROR_PIPE_NOT_CONNECTED as i32 || code == ERROR_BROKEN_PIPE as i32 =>
            {
                Ok(())
            },
            _ => Err(error),
        }
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

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct MobileRenderPrefsPayload {
    pub single_pane: bool,
    pub fit: bool,
    pub active_pane_is_fullscreen: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
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
        local_media: bool,
    },
    KittyZlibSupport {
        supported: bool,
    },
    SixelSupport {
        supported: bool,
    },
    RequestSessionList,
    RequestSessionPreview {
        tab_index: Option<u32>,
        pane_id: Option<(u32, bool)>,
        anchor: Option<crate::data::PreviewAnchor>,
    },
    RequestSessionAttention {
        include_command_text: bool,
    },
    PeerAttentionChanged {
        session_name: String,
        attention: Option<crate::attention::SessionAttention>,
    },
    SetMobileRenderPreferences {
        single_pane: bool,
        fit: bool,
    },
    HostTerminalFocusChanged {
        focused: bool,
    },
    StructuredRenderSupport {
        supported: bool,
        row_shifts: bool,
    },
    ClipboardPasteSupport {
        supported: bool,
        shortcut: Option<String>,
    },
    RenderFrameAck {
        seq: u64,
    },
    FontFamilies {
        families: Vec<String>,
    },
    PaneMirror {
        payload: Vec<u8>,
    },
    AdoptSessionHistory {
        history: crate::session_history::SessionHistory,
    },
}

// Types of messages sent from the server to the client
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum ServerToClientMsg {
    Render {
        content: String,
    },
    RenderFrame {
        frame: Vec<u8>,
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
        exit_code: Option<i32>,
    },
    CliPipeOutput {
        pipe_name: String,
        output: String,
    },
    QueryTerminalSize,
    SetSoftKeyboard {
        on: bool,
    },
    PasteFromClipboard {
        pane_id: Option<PaneId>,
    },
    StartWebServer,
    RenamedSession {
        name: String,
    },
    ConfigFileUpdated,
    TerminalSettingsChanged {
        settings: Box<TerminalSettings>,
    },
    HostTerminalThemeChanged {
        mode: HostTerminalThemeMode,
    },
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
    HostProgramStatus {
        status: Option<crate::attention::HostProgramStatus>,
    },
    SessionAttention {
        attention: crate::attention::SessionAttention,
    },
    SessionPreview {
        preview: Option<crate::data::SessionPreview>,
    },
    PaneMirror {
        payload: Vec<u8>,
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
    sender: Option<io::BufWriter<Box<dyn IpcStream>>>,
    _phantom: PhantomData<T>,
}

impl<T: Serialize> IpcSenderWithContext<T> {
    /// Returns a sender to the given [LocalSocketStream](interprocess::local_socket::LocalSocketStream).
    pub fn new(sender: LocalSocketStream) -> Self {
        Self {
            sender: Some(io::BufWriter::new(Box::new(sender))),
            _phantom: PhantomData,
        }
    }

    fn from_boxed(sender: Box<dyn IpcStream>) -> Self {
        Self {
            sender: Some(io::BufWriter::new(sender)),
            _phantom: PhantomData,
        }
    }

    pub fn send_client_msg(&mut self, msg: ClientToServerMsg) -> Result<()> {
        let sender = self
            .sender
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("ipc sender is closed"))?;
        let proto_msg: ProtoClientToServerMsg = msg.into();
        write_protobuf_message(sender, &proto_msg)?;
        sender.flush()?;
        Ok(())
    }

    /// Write one client frame, the same bytes as [`Self::send_client_msg`],
    /// but stop at `deadline`. A partial or timed-out write is an error:
    /// the frame is not a delivered message. The `BufWriter` must be empty;
    /// this writes the frame straight to the socket so the deadline covers
    /// every syscall.
    pub fn send_client_msg_until(
        &mut self,
        msg: ClientToServerMsg,
        deadline: Instant,
    ) -> io::Result<()> {
        if Instant::now() >= deadline {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "ipc write deadline exhausted",
            ));
        }
        #[cfg(unix)]
        {
            self.send_client_frame_until(msg, deadline)
        }
        #[cfg(not(unix))]
        {
            // Named-pipe sends cannot take SO_SNDTIMEO. Cancel the
            // synchronous write on this thread when the deadline passes.
            #[cfg(windows)]
            {
                run_with_deadline_cancel(deadline, || {
                    self.send_client_msg(msg)
                        .map_err(|err| io::Error::other(err.to_string()))
                })
            }
            #[cfg(not(windows))]
            {
                self.send_client_msg(msg)
                    .map_err(|err| io::Error::other(err.to_string()))
            }
        }
    }

    #[cfg(unix)]
    fn send_client_frame_until(
        &mut self,
        msg: ClientToServerMsg,
        deadline: Instant,
    ) -> io::Result<()> {
        let frame = encode_client_frame(msg)?;
        let sender = self
            .sender
            .as_mut()
            .ok_or_else(|| io::Error::other("ipc sender is closed"))?;
        if !sender.buffer().is_empty() {
            return Err(io::Error::other("ipc send buffer is not empty"));
        }
        let inner = sender.get_mut();
        write_all_ipc_until_deadline(inner.as_mut(), &frame, deadline)?;
        let _ = inner.set_write_timeout(None);
        Ok(())
    }

    pub fn send_server_msg(&mut self, msg: ServerToClientMsg) -> Result<()> {
        let sender = self
            .sender
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("ipc sender is closed"))?;
        let proto_msg: ProtoServerToClientMsg = msg.into();
        write_protobuf_message(sender, &proto_msg)?;
        sender.flush()?;
        Ok(())
    }

    /// Drop this sender without flushing a frame left buffered by a failed
    /// write. The frame was not delivered, and retrying its remaining bytes
    /// during `BufWriter`'s drop could block indefinitely on Windows.
    pub fn abandon(&mut self) {
        if let Some(sender) = self.sender.take() {
            let (stream, _) = sender.into_parts();
            drop(stream);
        }
    }

    /// Returns another handle to the socket this sender writes to, for
    /// shutting it down from another thread while a write is blocked.
    pub fn try_clone_stream(&self) -> io::Result<Box<dyn IpcStream>> {
        self.sender
            .as_ref()
            .ok_or_else(|| io::Error::other("ipc sender is closed"))?
            .get_ref()
            .try_clone_stream()
    }

    /// Returns an [`IpcReceiverWithContext`] with the same socket as this sender.
    pub fn get_receiver<F>(&self) -> io::Result<IpcReceiverWithContext<F>>
    where
        F: for<'de> Deserialize<'de> + Serialize,
    {
        let socket = self
            .sender
            .as_ref()
            .ok_or_else(|| io::Error::other("ipc sender is closed"))?
            .get_ref()
            .try_clone_stream()?;
        Ok(IpcReceiverWithContext::from_boxed(socket))
    }
}

impl<T: Serialize> Drop for IpcSenderWithContext<T> {
    fn drop(&mut self) {
        self.abandon();
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
    pub fn get_sender<F: Serialize>(&self) -> io::Result<IpcSenderWithContext<F>> {
        let socket = self.receiver.get_ref().try_clone_stream()?;
        Ok(IpcSenderWithContext::from_boxed(socket))
    }

    pub fn set_read_timeout(&self, timeout: Option<Duration>) -> io::Result<()> {
        self.receiver.get_ref().set_read_timeout(timeout)
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
    if len > MAX_FRAME_BYTES {
        // The payload is left unread, so the stream cannot be resynchronized.
        return Err(IpcReceiveError::Disconnected);
    }

    let mut buf = vec![0u8; len];
    reader
        .read_exact(&mut buf)
        .map_err(|_| IpcReceiveError::Disconnected)?;

    T::decode(&buf[..]).map_err(|_| IpcReceiveError::Undecodable)
}

fn write_protobuf_message<T: Message>(writer: &mut impl Write, msg: &T) -> Result<()> {
    let encoded = msg.encode_to_vec();
    validate_frame_length(encoded.len())?;
    let len = encoded.len() as u32;

    // we measure the length of the message and transmit it first so that the reader will be able
    // to first read exactly 4 bytes (representing this length) and then read that amount of bytes
    // as the actual message - this is so that we are able to distinct whole messages over the wire
    // stream
    writer.write_all(&len.to_le_bytes())?;
    writer.write_all(&encoded)?;
    Ok(())
}

fn validate_frame_length(len: usize) -> Result<()> {
    if len > MAX_FRAME_BYTES {
        return Err(anyhow::anyhow!(
            "encoded IPC frame exceeds the maximum frame size"
        ));
    }
    u32::try_from(len).map_err(|_| anyhow::anyhow!("encoded IPC frame exceeds the u32 prefix"))?;
    Ok(())
}

/// What `async_send_kill_and_await` actually observed.
///
/// Process death is a separate fact. This enum never claims the server
/// process has exited; [`KillReply::ShutdownAcknowledged`] only means a
/// `ServerToClientMsg::Exit` frame was decoded. `KillSession` sends
/// `ExitReason::Normal` (`zellij-server` `ServerInstruction::KillSession`);
/// any `Exit` variant counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KillReply {
    /// A length-prefixed `ServerToClientMsg::Exit` was decoded.
    ShutdownAcknowledged,
    /// The kill request was written, but the reply was not a decoded Exit
    /// (EOF, short read, oversized length, decode failure, timeout, unexpected
    /// message, or a later I/O error).
    OutcomeUnknown,
}

#[cfg(unix)]
fn remaining_until(deadline: Instant) -> io::Result<Duration> {
    // Sub-millisecond values round to a zero `timeval`, and Linux treats
    // `SO_RCVTIMEO`/`SO_SNDTIMEO` of zero as "block forever".
    match deadline.checked_duration_since(Instant::now()) {
        Some(remaining) if remaining >= Duration::from_millis(1) => Ok(remaining),
        _ => Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "deadline exhausted",
        )),
    }
}

/// Runs a synchronous operation on this thread, cancelling its blocking I/O
/// when `deadline` passes. The watchdog is joined before returning, so a late
/// cancellation cannot land on a later operation of the same thread.
#[cfg(windows)]
fn run_with_deadline_cancel<T>(
    deadline: Instant,
    operation: impl FnOnce() -> io::Result<T>,
) -> io::Result<T> {
    let remaining = match deadline.checked_duration_since(Instant::now()) {
        Some(remaining) if !remaining.is_zero() => remaining,
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "ipc io deadline exhausted",
            ));
        },
    };
    let cancel = BlockingIoCancel::for_current_thread()?;
    let (done_tx, done_rx) = std::sync::mpsc::channel();
    let watchdog = std::thread::Builder::new()
        .name("ipc_io_deadline".to_owned())
        .spawn(move || {
            if done_rx.recv_timeout(remaining).is_err() {
                let _ = cancel.cancel_blocking_io();
            }
        })?;
    let result = operation();
    let _ = done_tx.send(());
    let _ = watchdog.join();
    match result {
        Err(_) if Instant::now() >= deadline => Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "ipc io deadline exhausted",
        )),
        result => result,
    }
}

fn decode_server_frame(buf: &[u8]) -> io::Result<ServerToClientMsg> {
    let proto = ProtoServerToClientMsg::decode(buf)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
    ServerToClientMsg::try_from(proto)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err.to_string()))
}

/// Socket timeouts are inactivity timers. Callers that need one absolute
/// deadline must refresh them before every syscall.
#[cfg(unix)]
pub(crate) trait DeadlineIo {
    fn refresh_read_timeout(&self, timeout: Option<Duration>) -> io::Result<()>;
    fn refresh_write_timeout(&self, timeout: Option<Duration>) -> io::Result<()>;
    fn wait_readable(&self, deadline: Instant) -> io::Result<()> {
        let _ = deadline;
        Ok(())
    }
}

#[cfg(unix)]
impl DeadlineIo for LocalSocketStream {
    fn refresh_read_timeout(&self, timeout: Option<Duration>) -> io::Result<()> {
        use interprocess::local_socket::traits::Stream;
        self.set_recv_timeout(timeout)
    }
    fn refresh_write_timeout(&self, timeout: Option<Duration>) -> io::Result<()> {
        use interprocess::local_socket::traits::Stream;
        self.set_send_timeout(timeout)
    }
    fn wait_readable(&self, deadline: Instant) -> io::Result<()> {
        use nix::errno::Errno;
        use nix::poll::{poll, PollFd, PollFlags, PollTimeout};
        use std::os::fd::AsFd;

        let interprocess::local_socket::Stream::UdSocket(stream) = self;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "deadline exhausted",
                ));
            }
            let poll_timeout = PollTimeout::try_from(remaining).unwrap_or(PollTimeout::MAX);
            let mut fds = [PollFd::new(stream.as_fd(), PollFlags::POLLIN)];
            match poll(&mut fds, poll_timeout) {
                Err(Errno::EINTR) => continue,
                Err(error) => return Err(error.into()),
                Ok(0) => {
                    return Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        "deadline exhausted",
                    ))
                },
                Ok(_) => return Ok(()),
            }
        }
    }
}

/// Errors that mean "the peer is not ready yet", not "the probe is done".
///
/// macOS `poll` reports `POLLHUP` with `SO_ERROR == 0` for a unix socket that
/// has connected and is sitting in the accept queue. The same poll result is
/// reported when `accept` is already waiting (a later write succeeds) and
/// when `accept` has not entered the kernel yet. In that second window a
/// write or read returns `WouldBlock`, `TimedOut`, `NotConnected`, an
/// immediate EOF, or `BrokenPipe` / `ConnectionReset` / `ConnectionAborted`.
/// Rust ignores `SIGPIPE`, so the broken pipe is an error on this socket
/// rather than a dead process. None of those are a finished exchange: the
/// same file descriptor answers once the server accepts. Retrying stops at
/// the caller's deadline, so a peer that stays reset does not wait longer
/// than the probe was already allowed, and this does not open a new socket.
#[cfg(unix)]
fn peer_not_ready(err: &io::Error) -> bool {
    if matches!(
        err.kind(),
        io::ErrorKind::Interrupted
            | io::ErrorKind::WouldBlock
            | io::ErrorKind::TimedOut
            | io::ErrorKind::NotConnected
            | io::ErrorKind::BrokenPipe
            | io::ErrorKind::ConnectionReset
            | io::ErrorKind::ConnectionAborted
    ) {
        return true;
    }
    // macOS has returned a transient `EPROTOTYPE` for a write to a unix
    // socket whose peer is not fully established. It is not a protocol
    // mismatch the probe can act on; the deadline bounds the retry.
    #[cfg(target_vendor = "apple")]
    if err.raw_os_error() == Some(nix::errno::Errno::EPROTOTYPE as i32) {
        return true;
    }
    // macOS has returned EINVAL while a just-connected Unix socket was not yet
    // ready for the probe read. Retrying is bounded by the caller's deadline.
    #[cfg(target_vendor = "apple")]
    if err.kind() == io::ErrorKind::InvalidInput {
        return true;
    }
    false
}

/// Wait a millisecond, or less when the deadline is closer. A kernel `EAGAIN`
/// must not spin. The wait never extends the deadline.
#[cfg(unix)]
fn pause_until_peer_ready(deadline: Instant) -> io::Result<()> {
    let remaining = remaining_until(deadline)?;
    std::thread::sleep(Duration::from_millis(1).min(remaining));
    Ok(())
}

/// `read_exact` against one absolute deadline. The socket timeout is refreshed
/// from `deadline` before each `read`. `Interrupted`, `WouldBlock`,
/// `TimedOut`, `NotConnected`, `BrokenPipe`, `ConnectionReset`,
/// `ConnectionAborted`, and an immediate EOF retry until the deadline:
/// on macOS those are what a connected socket returns before `accept`.
/// Bytes already stored stay stored. Zero remaining time returns
/// [`io::ErrorKind::TimedOut`] without another blocking call.
#[cfg(unix)]
pub(crate) fn read_exact_until_deadline<R>(
    reader: &mut R,
    buf: &mut [u8],
    deadline: Instant,
) -> io::Result<()>
where
    R: Read + DeadlineIo,
{
    let mut filled = 0;
    let mut traced = false;
    while filled < buf.len() {
        #[cfg(not(target_vendor = "apple"))]
        {
            let remaining = remaining_until(deadline)?;
            reader.refresh_read_timeout(Some(remaining))?;
        }
        #[cfg(target_vendor = "apple")]
        remaining_until(deadline)?;
        reader.wait_readable(deadline)?;
        match reader.read(&mut buf[filled..]) {
            Ok(0) => {
                trace_probe_retry_once(
                    &mut traced,
                    "read",
                    filled,
                    buf.len(),
                    &io::Error::new(io::ErrorKind::UnexpectedEof, "read returned zero"),
                );
                pause_until_peer_ready(deadline)?;
            },
            Ok(n) => filled += n,
            Err(err) if err.kind() == io::ErrorKind::Interrupted => continue,
            Err(err) if peer_not_ready(&err) => {
                trace_probe_retry_once(&mut traced, "read", filled, buf.len(), &err);
                pause_until_peer_ready(deadline)?;
            },
            Err(err) => return Err(err),
        }
    }
    Ok(())
}

#[cfg(unix)]
fn trace_probe_retry_once(
    traced: &mut bool,
    stage: &str,
    completed: usize,
    total: usize,
    error: &io::Error,
) {
    if *traced {
        return;
    }
    *traced = true;
    log::debug!("probe {stage} first retry at {completed}/{total} bytes: {error:?}");
    #[cfg(test)]
    eprintln!("probe {stage} first retry at {completed}/{total} bytes: {error:?}");
}

/// `write_all` + `flush` on an [`IpcStream`], refreshing `SO_SNDTIMEO` from
/// `deadline` before every syscall. A trickle of progress cannot extend the
/// deadline. `WouldBlock` / `TimedOut` from the socket timer retry until the
/// deadline, then return [`io::ErrorKind::TimedOut`].
#[cfg(unix)]
fn write_all_ipc_until_deadline(
    writer: &mut dyn IpcStream,
    buf: &[u8],
    deadline: Instant,
) -> io::Result<()> {
    let mut written = 0;
    while written < buf.len() {
        let remaining = remaining_until(deadline)?;
        writer.set_write_timeout(Some(remaining))?;
        match writer.write(&buf[written..]) {
            Ok(0) => {
                return Err(io::Error::new(
                    io::ErrorKind::WriteZero,
                    "write returned 0 before the deadline",
                ));
            },
            Ok(n) => written += n,
            Err(err) if err.kind() == io::ErrorKind::Interrupted => continue,
            Err(err)
                if err.kind() == io::ErrorKind::WouldBlock
                    || err.kind() == io::ErrorKind::TimedOut =>
            {
                if Instant::now() >= deadline {
                    return Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        "ipc write deadline exhausted",
                    ));
                }
                continue;
            },
            Err(err) => return Err(err),
        }
    }
    let remaining = remaining_until(deadline)?;
    writer.set_write_timeout(Some(remaining))?;
    writer.flush()?;
    Ok(())
}

/// `write_all` against one absolute deadline. The socket timeout is refreshed
/// from `deadline` before each `write`. A zero-length write, `Interrupted`,
/// `WouldBlock`, `TimedOut`, `NotConnected`, `BrokenPipe`, `ConnectionReset`,
/// and `ConnectionAborted` retry until the deadline. A short write keeps the
/// bytes already counted and retries only the tail, so a reset in the middle
/// of the frame is not sent twice. Zero remaining time returns
/// [`io::ErrorKind::TimedOut`] without another blocking call. `flush` uses
/// the same retry.
#[cfg(unix)]
pub(crate) fn write_all_until_deadline<W>(
    writer: &mut W,
    buf: &[u8],
    deadline: Instant,
) -> io::Result<()>
where
    W: Write + DeadlineIo,
{
    let mut written = 0;
    let mut traced = false;
    while written < buf.len() {
        let remaining = remaining_until(deadline)?;
        writer.refresh_write_timeout(Some(remaining))?;
        match writer.write(&buf[written..]) {
            Ok(0) => {
                trace_probe_retry_once(
                    &mut traced,
                    "write",
                    written,
                    buf.len(),
                    &io::Error::new(io::ErrorKind::WriteZero, "write returned zero"),
                );
                pause_until_peer_ready(deadline)?;
            },
            Ok(n) => written += n,
            Err(err) if err.kind() == io::ErrorKind::Interrupted => continue,
            Err(err) if peer_not_ready(&err) => {
                trace_probe_retry_once(&mut traced, "write", written, buf.len(), &err);
                pause_until_peer_ready(deadline)?;
            },
            Err(err) => return Err(err),
        }
    }
    loop {
        let remaining = remaining_until(deadline)?;
        writer.refresh_write_timeout(Some(remaining))?;
        match writer.flush() {
            Ok(()) => return Ok(()),
            Err(err) if err.kind() == io::ErrorKind::Interrupted => continue,
            Err(err) if peer_not_ready(&err) => {
                trace_probe_retry_once(&mut traced, "flush", written, buf.len(), &err);
                pause_until_peer_ready(deadline)?;
            },
            Err(err) => return Err(err),
        }
    }
}

#[cfg(unix)]
pub(crate) fn encode_client_frame(msg: ClientToServerMsg) -> io::Result<Vec<u8>> {
    let proto: ProtoClientToServerMsg = msg.into();
    let encoded = proto.encode_to_vec();
    validate_frame_length(encoded.len())
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err.to_string()))?;
    let len = u32::try_from(encoded.len())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "frame length exceeds u32"))?;
    let mut frame = Vec::with_capacity(4 + encoded.len());
    frame.extend_from_slice(&len.to_le_bytes());
    frame.extend_from_slice(&encoded);
    Ok(frame)
}

/// Read one length-prefixed server frame, refusing to allocate above `max_frame`.
#[cfg(unix)]
pub(crate) fn read_server_frame_until_deadline<R>(
    reader: &mut R,
    deadline: Instant,
    max_frame: usize,
) -> io::Result<ServerToClientMsg>
where
    R: Read + DeadlineIo,
{
    let mut len_bytes = [0u8; 4];
    read_exact_until_deadline(reader, &mut len_bytes, deadline)?;
    let len = u32::from_le_bytes(len_bytes) as usize;
    if len > max_frame {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "frame exceeds the allowed cap",
        ));
    }
    let mut buf = vec![0u8; len];
    read_exact_until_deadline(reader, &mut buf, deadline)?;
    decode_server_frame(&buf)
}

#[cfg(all(test, unix))]
mod probe_io_retry {
    use super::{read_exact_until_deadline, write_all_until_deadline, DeadlineIo};
    use std::io::{self, Read, Write};
    use std::time::{Duration, Instant};

    struct Scripted {
        reads: Vec<io::Result<usize>>,
        writes: Vec<io::Result<usize>>,
        payload: Vec<u8>,
    }

    impl DeadlineIo for Scripted {
        fn refresh_read_timeout(&self, _timeout: Option<Duration>) -> io::Result<()> {
            Ok(())
        }
        fn refresh_write_timeout(&self, _timeout: Option<Duration>) -> io::Result<()> {
            Ok(())
        }
    }

    impl Read for Scripted {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            if self.reads.is_empty() {
                let n = self.payload.len().min(buf.len());
                buf[..n].copy_from_slice(&self.payload[..n]);
                self.payload.drain(..n);
                return Ok(n);
            }
            match self.reads.remove(0) {
                Ok(0) => Ok(0),
                Ok(n) => {
                    let n = n.min(buf.len()).min(self.payload.len());
                    buf[..n].copy_from_slice(&self.payload[..n]);
                    self.payload.drain(..n);
                    Ok(n)
                },
                Err(err) => Err(err),
            }
        }
    }

    impl Write for Scripted {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            if self.writes.is_empty() {
                return Ok(buf.len());
            }
            self.writes.remove(0)
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn an_early_eof_and_would_block_are_retried_until_the_peer_answers() {
        let mut reader = Scripted {
            reads: vec![
                Ok(0),
                Err(io::Error::new(
                    io::ErrorKind::WouldBlock,
                    "waiting for accept",
                )),
            ],
            writes: vec![],
            payload: b"ok".to_vec(),
        };
        let mut buf = [0u8; 2];
        read_exact_until_deadline(
            &mut reader,
            &mut buf,
            Instant::now() + Duration::from_secs(2),
        )
        .expect("the reply arrives after the pre-accept errors");
        assert_eq!(&buf, b"ok");
    }

    #[test]
    fn a_zero_write_before_accept_is_retried() {
        let mut writer = Scripted {
            reads: vec![],
            writes: vec![Ok(0)],
            payload: vec![],
        };
        write_all_until_deadline(
            &mut writer,
            b"ping",
            Instant::now() + Duration::from_secs(2),
        )
        .expect("the write completes once the peer accepts");
    }

    #[test]
    fn a_broken_pipe_before_accept_is_retried_on_the_same_socket() {
        let mut writer = Scripted {
            reads: vec![],
            writes: vec![Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "accept has not run",
            ))],
            payload: vec![],
        };
        write_all_until_deadline(
            &mut writer,
            b"ping",
            Instant::now() + Duration::from_secs(2),
        )
        .expect("the same socket accepts the frame after the reset");
        assert!(
            writer.writes.is_empty(),
            "the retry must consume the broken-pipe result"
        );
    }

    #[test]
    fn a_short_write_then_reset_retries_only_the_unwritten_tail() {
        let mut writer = Scripted {
            reads: vec![],
            writes: vec![
                Ok(1),
                Err(io::Error::new(
                    io::ErrorKind::ConnectionReset,
                    "reset after one byte",
                )),
                Ok(3),
            ],
            payload: vec![],
        };
        write_all_until_deadline(
            &mut writer,
            b"ping",
            Instant::now() + Duration::from_secs(2),
        )
        .expect("the tail is written after the reset");
        assert!(writer.writes.is_empty());
    }

    #[test]
    fn a_connection_reset_before_the_reply_is_retried() {
        let mut reader = Scripted {
            reads: vec![Err(io::Error::new(
                io::ErrorKind::ConnectionAborted,
                "accept has not run",
            ))],
            writes: vec![],
            payload: b"ok".to_vec(),
        };
        let mut buf = [0u8; 2];
        read_exact_until_deadline(
            &mut reader,
            &mut buf,
            Instant::now() + Duration::from_secs(2),
        )
        .expect("the reply arrives after the reset");
        assert_eq!(&buf, b"ok");
    }

    #[test]
    fn a_peer_that_stays_reset_stops_at_the_deadline() {
        struct AlwaysReset;

        impl DeadlineIo for AlwaysReset {
            fn refresh_read_timeout(&self, _timeout: Option<Duration>) -> io::Result<()> {
                Ok(())
            }
            fn refresh_write_timeout(&self, _timeout: Option<Duration>) -> io::Result<()> {
                Ok(())
            }
        }

        impl Write for AlwaysReset {
            fn write(&mut self, _buf: &[u8]) -> io::Result<usize> {
                Err(io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    "peer stays reset",
                ))
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }

        let started = Instant::now();
        let err = write_all_until_deadline(
            &mut AlwaysReset,
            b"ping",
            started + Duration::from_millis(40),
        )
        .expect_err("a peer that stays reset must not succeed");
        let elapsed = started.elapsed();
        assert_eq!(err.kind(), io::ErrorKind::TimedOut);
        assert!(
            elapsed < Duration::from_millis(500),
            "reset retry ran past the deadline: {elapsed:?}"
        );
    }
}

#[cfg(windows)]
fn read_server_frame_capped<R: Read>(
    reader: &mut R,
    max_frame: usize,
) -> io::Result<ServerToClientMsg> {
    let mut len_bytes = [0u8; 4];
    reader.read_exact(&mut len_bytes)?;
    let len = u32::from_le_bytes(len_bytes) as usize;
    if len > max_frame {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "frame exceeds the allowed cap",
        ));
    }
    let mut buf = vec![0u8; len];
    reader.read_exact(&mut buf)?;
    decode_server_frame(&buf)
}

// Protobuf helper functions
pub fn send_protobuf_client_to_server(
    sender: &mut IpcSenderWithContext<ClientToServerMsg>,
    msg: ClientToServerMsg,
) -> Result<()> {
    sender.send_client_msg(msg)
}

pub fn send_protobuf_server_to_client(
    sender: &mut IpcSenderWithContext<ServerToClientMsg>,
    msg: ServerToClientMsg,
) -> Result<()> {
    sender.send_server_msg(msg)
}

pub fn recv_protobuf_client_to_server(
    receiver: &mut IpcReceiverWithContext<ClientToServerMsg>,
) -> Option<(ClientToServerMsg, ErrorContext)> {
    receiver.try_recv_client_msg().ok()
}

/// Decode a client message from an already-buffered frame body.
///
/// The handshake path reads the length prefix itself and refuses oversized
/// claims before allocating the body. This function does not look at a length
/// prefix and does not change the 256 MiB cap used by [`read_protobuf_message`]
/// for later frames on an established connection.
pub fn decode_client_to_server_frame(
    buf: &[u8],
) -> std::result::Result<ClientToServerMsg, IpcReceiveError> {
    let proto_msg =
        ProtoClientToServerMsg::decode(buf).map_err(|_| IpcReceiveError::Undecodable)?;
    proto_msg.try_into().map_err(|error| {
        warn!(
            "Error converting protobuf to ClientToServerMsg: {:?}",
            error
        );
        IpcReceiveError::Undecodable
    })
}

pub fn recv_protobuf_server_to_client(
    receiver: &mut IpcReceiverWithContext<ServerToClientMsg>,
) -> Option<(ServerToClientMsg, ErrorContext)> {
    receiver.try_recv_server_msg().ok()
}

/// Send `ClientToServerMsg::KillSession` and wait up to `budget` for a decoded
/// [`ServerToClientMsg::Exit`].
///
/// Returns [`Err`] if the request is not written (connect or write failure,
/// including the deadline expiring first). After a successful write, EOF, a
/// short read, an oversized frame, a decode failure, a timeout, or any
/// non-Exit message is [`KillReply::OutcomeUnknown`] — not success.
///
/// On Unix the local socket is bidirectional, so the same stream is used for
/// both send and receive. The budget is one absolute deadline over connect,
/// the kill write, and the reply frame. A trickle of bytes cannot extend it.
///
/// On Windows the named pipe is half-duplex and this runs on `spawn_blocking`.
/// The same deadline covers the connect, the write, and the reply read; a
/// blocked synchronous call is cancelled on its thread when the deadline
/// passes. Only a decoded `Exit` is [`KillReply::ShutdownAcknowledged`].
///
/// This function does not observe process death.
#[cfg(unix)]
pub async fn async_send_kill_and_await(
    path: &std::path::Path,
    budget: Duration,
) -> io::Result<KillReply> {
    use interprocess::local_socket::traits::tokio::Stream as _;
    use interprocess::local_socket::{prelude::*, GenericFilePath};
    use tokio::io::AsyncWriteExt;

    if budget.is_zero() {
        return Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "no time to send kill",
        ));
    }
    let deadline = Instant::now() + budget;
    let fs_name = path.to_fs_name::<GenericFilePath>()?;
    let connect_budget = remaining_until(deadline)?;
    let mut stream: interprocess::local_socket::tokio::Stream =
        match tokio::time::timeout(connect_budget, async {
            interprocess::local_socket::tokio::Stream::connect(fs_name).await
        })
        .await
        {
            Err(_) => {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "kill connect timed out",
                ));
            },
            Ok(result) => result?,
        };

    let proto_msg: ProtoClientToServerMsg = ClientToServerMsg::KillSession.into();
    let encoded = proto_msg.encode_to_vec();
    validate_frame_length(encoded.len())
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err.to_string()))?;
    let len = u32::try_from(encoded.len())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "frame length exceeds u32"))?;
    let mut frame = Vec::with_capacity(4 + encoded.len());
    frame.extend_from_slice(&len.to_le_bytes());
    frame.extend_from_slice(&encoded);
    write_all_async_until_deadline(&mut stream, &frame, deadline).await?;
    let _ = stream.flush().await;

    match read_exit_reply(&mut stream, deadline).await {
        Ok(ServerToClientMsg::Exit { .. }) => Ok(KillReply::ShutdownAcknowledged),
        Ok(_) | Err(_) => Ok(KillReply::OutcomeUnknown),
    }
}

#[cfg(unix)]
async fn write_all_async_until_deadline(
    stream: &mut interprocess::local_socket::tokio::Stream,
    buf: &[u8],
    deadline: Instant,
) -> io::Result<()> {
    use tokio::io::AsyncWriteExt;
    let mut written = 0;
    while written < buf.len() {
        let remaining = remaining_until(deadline)?;
        match tokio::time::timeout(remaining, stream.write(&buf[written..])).await {
            Err(_) => {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "kill write deadline exhausted",
                ));
            },
            Ok(Err(err)) if err.kind() == io::ErrorKind::Interrupted => continue,
            Ok(Err(err)) => return Err(err),
            Ok(Ok(0)) => {
                return Err(io::Error::new(
                    io::ErrorKind::WriteZero,
                    "kill write returned 0",
                ));
            },
            Ok(Ok(n)) => written += n,
        }
    }
    Ok(())
}

#[cfg(unix)]
async fn read_exact_async_until_deadline(
    stream: &mut interprocess::local_socket::tokio::Stream,
    buf: &mut [u8],
    deadline: Instant,
) -> io::Result<()> {
    use tokio::io::AsyncReadExt;
    let mut filled = 0;
    while filled < buf.len() {
        let remaining = remaining_until(deadline)?;
        match tokio::time::timeout(remaining, stream.read(&mut buf[filled..])).await {
            Err(_) => {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "kill read deadline exhausted",
                ));
            },
            Ok(Err(err)) if err.kind() == io::ErrorKind::Interrupted => continue,
            Ok(Err(err)) => return Err(err),
            Ok(Ok(0)) => {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "short read before the kill deadline",
                ));
            },
            Ok(Ok(n)) => filled += n,
        }
    }
    Ok(())
}

#[cfg(unix)]
async fn read_exit_reply(
    stream: &mut interprocess::local_socket::tokio::Stream,
    deadline: Instant,
) -> io::Result<ServerToClientMsg> {
    let mut len_bytes = [0u8; 4];
    read_exact_async_until_deadline(stream, &mut len_bytes, deadline).await?;
    let len = u32::from_le_bytes(len_bytes) as usize;
    if len > PROBE_MAX_FRAME_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "kill reply exceeds the probe cap",
        ));
    }
    let mut buf = vec![0u8; len];
    read_exact_async_until_deadline(stream, &mut buf, deadline).await?;
    decode_server_frame(&buf)
}

#[cfg(windows)]
pub async fn async_send_kill_and_await(
    path: &std::path::Path,
    budget: Duration,
) -> io::Result<KillReply> {
    // Windows named pipes have no socket timeout. Bound the connect, write,
    // and reply read with the same deadline, cancelling whichever
    // synchronous call is blocked when it expires.
    if budget.is_zero() {
        return Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "no time to send kill",
        ));
    }
    let deadline = Instant::now() + budget;
    let path = path.to_path_buf();
    tokio::task::spawn_blocking(move || {
        // Both pipes, under the per-process lock. A failed reply connect drops
        // the request instead of leaving a half the server can mis-pair.
        let remaining = deadline.saturating_duration_since(Instant::now());
        let (stream, mut reply_stream) =
            crate::pipe_pairs::connect_pair_with_timeout(&path, remaining)?;
        run_with_deadline_cancel(deadline, || {
            let mut sender = IpcSenderWithContext::<ClientToServerMsg>::new(stream);
            if let Err(error) = sender.send_client_msg(ClientToServerMsg::KillSession) {
                sender.abandon();
                return Err(io::Error::other(error.to_string()));
            }
            match read_server_frame_capped(&mut reply_stream, PROBE_MAX_FRAME_BYTES) {
                Ok(ServerToClientMsg::Exit { .. }) => Ok(KillReply::ShutdownAcknowledged),
                _ => Ok(KillReply::OutcomeUnknown),
            }
        })
    })
    .await
    .map_err(|e| io::Error::other(e.to_string()))?
}
