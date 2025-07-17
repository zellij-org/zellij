//! IPC stuff for starting to split things into a client and server model.
use crate::{
    cli::CliArgs,
    data::{ClientId, ConnectToSession, KeyWithModifier, Style},
    errors::{get_current_ctx, prelude::*, ErrorContext},
    input::config::Config,
    input::{actions::Action, layout::Layout, options::Options, plugins::PluginAliases},
    pane_size::{Size, SizeInPixels},
};
use interprocess::local_socket::LocalSocketStream;
use log::warn;
use nix::unistd::dup;
use serde::{Deserialize, Serialize};
use std::{
    fmt::{Display, Error, Formatter},
    io::{self, Write},
    marker::PhantomData,
    os::unix::io::{AsRawFd, FromRawFd},
    path::PathBuf,
};

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
        Box<Config>,  // represents the saved configuration
        Box<Options>, // represents the runtime configuration
        Box<Layout>,
        Box<PluginAliases>,
        bool, // should launch setup wizard
        bool, // is_web_client
        bool, // layout_is_welcome_screen
    ),
    AttachClient(
        ClientAttributes,
        Config,              // represents the saved configuration
        Options,             // represents the runtime configuration
        Option<usize>,       // tab position to focus
        Option<(u32, bool)>, // (pane_id, is_plugin) => pane id to focus
        bool,                // is_web_client
    ),
    Action(Action, Option<u32>, Option<ClientId>), // u32 is the terminal id
    Key(KeyWithModifier, Vec<u8>, bool),           // key, raw_bytes, is_kitty_keyboard_protocol
    ClientExited,
    KillSession,
    ConnStatus,
    ConfigWrittenToDisk(Config),
    FailedToWriteConfigToDisk(Option<PathBuf>),
    WebServerStarted(String), // String -> base_url
    FailedToStartWebServer(String),
}

// Types of messages sent from the server to the client
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ServerToClientMsg {
    Render(String),
    UnblockInputThread,
    Exit(ExitReason),
    Connected,
    Log(Vec<String>),
    LogError(Vec<String>),
    SwitchSession(ConnectToSession),
    UnblockCliPipeInput(String),   // String -> pipe name
    CliPipeOutput(String, String), // String -> pipe name, String -> Output
    QueryTerminalSize,
    WriteConfigToDisk { config: String },
    StartWebServer,
    RenamedSession(String), // String -> new session name
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ExitReason {
    Normal,
    NormalDetached,
    ForceDetached,
    CannotAttach,
    Disconnect,
    WebClientsForbidden,
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

    /// Sends an event, along with the current [`ErrorContext`], on this [`IpcSenderWithContext`]'s socket.
    pub fn send(&mut self, msg: T) -> Result<()> {
        let err_ctx = get_current_ctx();
        if rmp_serde::encode::write(&mut self.sender, &(msg, err_ctx)).is_err() {
            Err(anyhow!("failed to send message to client"))
        } else {
            if let Err(e) = self.sender.flush() {
                log::error!("Failed to flush ipc sender: {}", e);
            }
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
