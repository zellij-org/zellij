//! IPC stuff for starting to split things into a client and server model.
use crate::{
    data::{ClientId, ConnectToSession, KeyWithModifier, Style},
    errors::{prelude::*, ErrorContext},
    input::{actions::Action, cli_assets::CliAssets},
    pane_size::{Size, SizeInPixels},
};
use interprocess::local_socket::LocalSocketStream;
use log::warn;
use nix::unistd::dup;
use serde::{Deserialize, Serialize};
use std::{
    fmt::{Display, Error, Formatter},
    io::{self, Read, Write},
    marker::PhantomData,
    os::unix::io::{AsRawFd, FromRawFd},
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
    StartWebServer,
    RenamedSession {
        name: String,
    },
    ConfigFileUpdated,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum ExitReason {
    Normal,
    NormalDetached,
    ForceDetached,
    CannotAttach,
    Disconnect,
    WebClientsForbidden,
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
            Self::CustomExitStatus(exit_status) => write!(f, "Exit {}", exit_status),
            Self::Error(e) => write!(f, "Error occurred in server:\n{}", e),
        }
    }
}

/// Sends messages on a stream socket, along with an [`ErrorContext`].
pub struct IpcSenderWithContext<T: Serialize> {
    sender: io::BufWriter<LocalSocketStream>,
    _phantom: PhantomData<T>,
}

impl<T: Serialize> IpcSenderWithContext<T> {
    /// Returns a sender to the given [LocalSocketStream](interprocess::local_socket::LocalSocketStream).
    pub fn new(sender: LocalSocketStream) -> Self {
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
        let sock_fd = self.sender.get_ref().as_raw_fd();
        let dup_sock = dup(sock_fd).unwrap();
        let socket = unsafe { LocalSocketStream::from_raw_fd(dup_sock) };
        IpcReceiverWithContext::new(socket)
    }
}

/// Receives messages on a stream socket, along with an [`ErrorContext`].
pub struct IpcReceiverWithContext<T> {
    receiver: io::BufReader<LocalSocketStream>,
    _phantom: PhantomData<T>,
}

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

    pub fn recv_client_msg(&mut self) -> Option<(ClientToServerMsg, ErrorContext)> {
        match read_protobuf_message::<ProtoClientToServerMsg>(&mut self.receiver) {
            Ok(proto_msg) => match proto_msg.try_into() {
                Ok(rust_msg) => Some((rust_msg, ErrorContext::default())),
                Err(e) => {
                    warn!("Error converting protobuf to ClientToServerMsg: {:?}", e);
                    None
                },
            },
            Err(_e) => None,
        }
    }

    pub fn recv_server_msg(&mut self) -> Option<(ServerToClientMsg, ErrorContext)> {
        match read_protobuf_message::<ProtoServerToClientMsg>(&mut self.receiver) {
            Ok(proto_msg) => match proto_msg.try_into() {
                Ok(rust_msg) => Some((rust_msg, ErrorContext::default())),
                Err(e) => {
                    warn!("Error converting protobuf to ServerToClientMsg: {:?}", e);
                    None
                },
            },
            Err(_e) => None,
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

// Protobuf wire format utilities
fn read_protobuf_message<T: Message + Default>(reader: &mut impl Read) -> Result<T> {
    // Read length-prefixed protobuf message
    let mut len_bytes = [0u8; 4];
    reader.read_exact(&mut len_bytes)?;
    let len = u32::from_le_bytes(len_bytes) as usize;

    let mut buf = vec![0u8; len];
    reader.read_exact(&mut buf)?;

    T::decode(&buf[..]).map_err(Into::into)
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
    match read_protobuf_message::<ProtoClientToServerMsg>(&mut receiver.receiver) {
        Ok(proto_msg) => match proto_msg.try_into() {
            Ok(rust_msg) => Some((rust_msg, ErrorContext::default())),
            Err(e) => {
                warn!("Error converting protobuf message: {:?}", e);
                None
            },
        },
        Err(_e) => None,
    }
}

pub fn recv_protobuf_server_to_client(
    receiver: &mut IpcReceiverWithContext<ServerToClientMsg>,
) -> Option<(ServerToClientMsg, ErrorContext)> {
    match read_protobuf_message::<ProtoServerToClientMsg>(&mut receiver.receiver) {
        Ok(proto_msg) => match proto_msg.try_into() {
            Ok(rust_msg) => Some((rust_msg, ErrorContext::default())),
            Err(e) => {
                warn!("Error converting protobuf message: {:?}", e);
                None
            },
        },
        Err(_e) => None,
    }
}
