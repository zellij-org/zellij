//! IPC stuff for starting to split things into a client and server model.

#[cfg(windows)]
use crate::windows_utils::named_pipe::{PipeStream, Pipe};
use crate::{
    cli::CliArgs,
    data::{ClientId, ConnectToSession, InputMode, Style},
    errors::{get_current_ctx, prelude::*, ErrorContext},
    input::{actions::Action, keybinds::Keybinds, layout::Layout, options::Options, plugins::PluginsConfig},
    pane_size::{Size, SizeInPixels},
};
use interprocess::{local_socket::LocalSocketStream, os::windows::named_pipe::DuplexBytePipeStream};
use log::warn;

#[cfg(unix)]
use nix::unistd::dup;

use serde::{Deserialize, Serialize};
use std::{
    fmt::{Display, Error, Formatter},
    io::{self, Read, Write},
    marker::PhantomData, path::Path,
};

#[cfg(unix)]
use std::os::unix::io::{AsRawFd, FromRawFd};

type SessionId = u64;

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
    pub keybinds: Keybinds,
}

#[derive(Default, Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub struct PixelDimensions {
    pub text_area_size: Option<SizeInPixels>,
    pub character_cell_size: Option<SizeInPixels>,
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

// Types of messages sent from the client to the server
#[allow(clippy::large_enum_variant)]
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ClientToServerMsg {
    DetachSession(Vec<ClientId>),
    TerminalPixelDimensions(PixelDimensions),
    BackgroundColor(String),
    ForegroundColor(String),
    ColorRegisters(Vec<(usize, String)>),
    TerminalResize(Size),
    NewClient(
        ClientAttributes,
        Box<CliArgs>,
        Box<Options>,
        Box<Layout>,
        Option<PluginsConfig>,
    ),
    AttachClient(
        ClientAttributes,
        Options,
        Option<usize>,       // tab position to focus
        Option<(u32, bool)>, // (pane_id, is_plugin) => pane id to focus
    ),
    Action(Action, Option<u32>, Option<ClientId>), // u32 is the terminal id
    ClientExited,
    KillSession,
    ConnStatus,
    ListClients,
}

impl Display for ClientToServerMsg {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            ClientToServerMsg::DetachSession(_) => write!(f, "ClientToServerMsg::DetachSession"),
            ClientToServerMsg::TerminalPixelDimensions(_) => {
                write!(f, "ClientToServerMsg::TerminalPixelDimensions")
            },
            ClientToServerMsg::BackgroundColor(_) => {
                write!(f, "ClientToServerMsg::BackgroundColor")
            },
            ClientToServerMsg::ForegroundColor(_) => {
                write!(f, "ClientToServerMsg::ForegroundColor")
            },
            ClientToServerMsg::ColorRegisters(_) => write!(f, "ClientToServerMsg::ColorRegisters"),
            ClientToServerMsg::TerminalResize(_) => write!(f, "ClientToServerMsg::TerminalResize"),
            ClientToServerMsg::NewClient(_, _, _, _, _) => {
                write!(f, "ClientToServerMsg::NewClient")
            },
            ClientToServerMsg::AttachClient(_, _, _, _) => {
                write!(f, "ClientToServerMsg::AttachClient")
            },
            ClientToServerMsg::Action(action, _, _) => {
                write!(f, "ClientToServerMsg::Action({:?})", action)
            },
            ClientToServerMsg::ClientExited => write!(f, "ClientToServerMsg::ClientExited"),
            ClientToServerMsg::KillSession => write!(f, "ClientToServerMsg::KillSession"),
            ClientToServerMsg::ConnStatus => write!(f, "ClientToServerMsg::ConnStatus"),
            ClientToServerMsg::ListClients => write!(f, "ClientToServerMsg::ListClients"),
        }
    }
}

// Types of messages sent from the server to the client
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ServerToClientMsg {
    Render(String),
    UnblockInputThread,
    Exit(ExitReason),
    SwitchToMode(InputMode),
    Connected,
    ActiveClients(Vec<ClientId>),
    Log(Vec<String>),
    LogError(Vec<String>),
    SwitchSession(ConnectToSession),
}

impl Display for ServerToClientMsg {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            ServerToClientMsg::Render(_) => write!(f, "ServerToClientMsg::Render"),
            ServerToClientMsg::UnblockInputThread => {
                write!(f, "ServerToClientMsg::UnblockInputThread")
            },
            ServerToClientMsg::Exit(_) => write!(f, "ServerToClientMsg::Exit"),
            ServerToClientMsg::SwitchToMode(_) => write!(f, "ServerToClientMsg::SwitchToMode"),
            ServerToClientMsg::Connected => write!(f, "ServerToClientMsg::Connected"),
            ServerToClientMsg::ActiveClients(_) => write!(f, "ServerToClientMsg::ActiveClients"),
            ServerToClientMsg::Log(_) => write!(f, "ServerToClientMsg::Log"),
            ServerToClientMsg::LogError(_) => write!(f, "ServerToClientMsg::LogError"),
            ServerToClientMsg::SwitchSession(_) => write!(f, "ServerToClientMsg::SwitchSession"),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ExitReason {
    Normal,
    NormalDetached,
    ForceDetached,
    CannotAttach,
    Disconnect,
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
            Self::Error(e) => write!(f, "Error occurred in server:\n{}", e),
        }
    }
}

#[cfg(windows)]
pub type IpcSocketStream = PipeStream;
#[cfg(unix)]
pub type IpcSocketStream = LocalSocketStream;

/// Sends messages on a stream socket, along with an [`ErrorContext`].
pub struct IpcSenderWithContext<T: Serialize> {
    sender: io::BufWriter<IpcSocketStream>,
    _phantom: PhantomData<T>,
}

#[cfg(unix)]
impl<T: Serialize> IpcSenderWithContext<T> {
    /// Returns a sender to the given [LocalSocketStream](interprocess::local_socket::LocalSocketStream).
    pub fn new(sender: LocalSocketStream) -> Self {
        Self {
            sender: io::BufWriter::new(sender),
            _phantom: PhantomData,
        }
    }

    /// Sends an event, along with the current [`ErrorContext`], on this [`IpcSenderWithContext`]'s socket.
    pub fn send(&mut self, msg: T) -> Result<()> {
        let err_ctx = get_current_ctx();
        if rmp_serde::encode::write(&mut self.sender, &(msg, err_ctx)).is_err() {
            Err(anyhow!("failed to send message to client"))
        } else {
            // TODO: unwrapping here can cause issues when the server disconnects which we don't mind
            // do we need to handle errors here in other cases?
            let _ = self.sender.flush();
            Ok(())
        }
    }

    /// Returns an [`IpcReceiverWithContext`] with the same socket as this sender.
    pub fn get_receiver<F>(&self) -> IpcReceiverWithContext<F>
    where
        F: for<'de> Deserialize<'de> + Serialize,
    {
        let sock_fd = self.sender.get_ref().as_raw_fd();
        let dup_sock = dup(sock_fd).unwrap();
        let socket = unsafe { LocalSocketStream::from_raw_fd(dup_sock) };
        IpcReceiverWithContext::new(socket)
    }

}

#[cfg(windows)]
impl<T: Serialize> IpcSenderWithContext<T> {
    /// Returns a sender to the given [PipeStream](zellij_utils::ipc::named_pipe::PipeStream).
    pub fn new(sender: PipeStream) -> Self {
        Self {
            sender: io::BufWriter::new(sender),
            _phantom: PhantomData,
        }
    }

    ///Returns an [`IpcReceiverWithContext`] with the same socket as this sender.
    pub fn get_receiver<F>(&self) -> IpcReceiverWithContext<F>
    where
        F: for<'de> Deserialize<'de> + Serialize,
    {
        let socket = self
            .sender
            .get_ref()
            .try_clone()
            .expect("Failed to duplicate pipe to obtain receiver");
        IpcReceiverWithContext::new(socket)
    }

    /// Sends an event, along with the current [`ErrorContext`], on this [`IpcSenderWithContext`]'s socket.
    pub fn send(&mut self, msg: T) -> Result<()> {
        log::debug!("Sending message");
        let err_ctx = get_current_ctx();
        let result = if rmp_serde::encode::write(&mut self.sender, &(msg, err_ctx)).is_err() {
            Err(anyhow!("failed to send message to client"))
        } else {
            // TODO: unwrapping here can cause issues when the server disconnects which we don't mind
            // do we need to handle errors here in other cases?
            let _ = self.sender.flush();
            Ok(())
        };
        log::debug!("Message sent with {:?}", &result);
        result
    }
}

/// Receives messages on a stream socket, along with an [`ErrorContext`].
pub struct IpcReceiverWithContext<T> {

    #[cfg(unix)]
    receiver: io::BufReader<IpcSocketStream>,
    #[cfg(windows)]
    receiver: IpcSocketStream,
    _phantom: PhantomData<T>,
}

#[cfg(unix)]
impl<T> IpcReceiverWithContext<T>
where
    T: for<'de> Deserialize<'de> + Serialize,
{
    /// Returns a receiver to the given [LocalSocketStream](interprocess::local_socket::LocalSocketStream).
    pub fn new(receiver: LocalSocketStream) -> Self {
        Self {
            receiver: io::BufReader::new(receiver),
            _phantom: PhantomData,
        }
    }

    /// Receives an event, along with the current [`ErrorContext`], on this [`IpcReceiverWithContext`]'s socket.
    pub fn recv(&mut self) -> Option<(T, ErrorContext)> {
        match rmp_serde::decode::from_read(&mut self.receiver) {
            Ok(msg) => Some(msg),
            Err(e) => {
                warn!("Error in IpcReceiver.recv(): {:?}", e);
                None
            },
        }
    }

    /// Returns an [`IpcSenderWithContext`] with the same socket as this receiver.
    pub fn get_sender<F: Serialize>(&self) -> IpcSenderWithContext<F> {
        let sock_fd = self.receiver.get_ref().as_raw_fd();
        let dup_sock = dup(sock_fd).unwrap();
        let socket = unsafe { LocalSocketStream::from_raw_fd(dup_sock) };
        IpcSenderWithContext::new(socket)
    }
}

#[cfg(windows)]
impl<T> IpcReceiverWithContext<T>
where
    T: for<'de> Deserialize<'de> + Serialize,
{
    /// Returns a receiver to the given [PipeStream](zellij_utils::ipc::named_pipe::PipeStream).
    pub fn new(receiver: PipeStream) -> Self {
        Self {
            receiver,
            _phantom: PhantomData,
        }
    }

    /// Returns an [`IpcSenderWithContext`] with the same socket as this receiver.
    pub fn get_sender<F: Serialize>(&self) -> IpcSenderWithContext<F> {
        let socket = self
            .receiver
            .try_clone()
            .expect("Failed to duplicate pipe to obtain sender");
        IpcSenderWithContext::new(socket)
    }

    /// Receives an event, along with the current [`ErrorContext`], on this [`IpcReceiverWithContext`]'s socket.
    pub fn recv(&mut self) -> Option<(T, ErrorContext)> {
        let mut buf = Vec::with_capacity(512);
        let mut counter = 0;
        loop {
            let remaining_buffer = buf.spare_capacity_mut();
            for element in remaining_buffer.iter_mut() {
                element.write(0);
            }
            let remaining_buffer: &mut [u8] = unsafe { std::mem::transmute(remaining_buffer) };

            log::info!("Reading from pipe");
            let consumed = self.receiver.read(remaining_buffer).unwrap();
            unsafe { buf.set_len(buf.len() + consumed) };
            let input = &buf;
            if consumed > 0 {
                counter = 0;
            }
            match rmp_serde::decode::from_slice(input) {
                Ok(msg) => break Some(msg),
                Err(e) => {
                    match e {
                        rmp_serde::decode::Error::LengthMismatch(expected) => {
                            let expected = expected as usize;
                            if expected > input.len() {
                                // we need more bytes
                                if expected > buf.capacity() {
                                    buf.reserve_exact(expected - buf.len())
                                }
                                continue;
                            } else if expected < input.len() {
                                // we have more than one message
                                todo!("Decode first message and figure out what to do with the remainder")
                            }
                        },
                        (rmp_serde::decode::Error::InvalidDataRead(io_error)
                        | rmp_serde::decode::Error::InvalidMarkerRead(io_error))
                            if io_error.kind() == std::io::ErrorKind::UnexpectedEof =>
                        {
                            counter += 1;
                            if counter > 5 {
                                return None;
                            }
                            log::warn!("Missing some content in {:x?}", input);
                            buf.reserve(1);
                            continue;
                        },
                        e => {
                            return None;
                        },
                    }
                },
            }
        }
    }
}

#[cfg(unix)]
pub fn bind_server(name: &Path) -> Result<LocalSocketListener> {
    let socket_path = name;
    drop(std::fs::remove_file(&socket_path));
    let listener = LocalSocketListener::bind(&*socket_path)?;
    // set the sticky bit to avoid the socket file being potentially cleaned up
    // https://specifications.freedesktop.org/basedir-spec/basedir-spec-latest.html states that for XDG_RUNTIME_DIR:
    // "To ensure that your files are not removed, they should have their access time timestamp modified at least once every 6 hours of monotonic time or the 'sticky' bit should be set on the file. "
    // It is not guaranteed that all platforms allow setting the sticky bit on sockets!
    drop(set_permissions(&socket_path, 0o1700));

    return Ok(listener);
}

#[cfg(windows)]
pub fn bind_server(name: &Path) -> Result<Pipe> {
    let pipe = Pipe::new(name);

    Ok(pipe)
}
