//! IPC stuff for starting to split things into a client and server model.

use crate::cli::CliArgs;
use crate::common::{
    errors::{get_current_ctx, ErrorContext},
    input::actions::Action,
};
use crate::panes::PositionAndSize;
use interprocess::local_socket::LocalSocketStream;
use nix::unistd::dup;
use serde::{Deserialize, Serialize};
use std::io::{self, Write};
use std::marker::PhantomData;
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

// Types of messages sent from the client to the server
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ClientToServerMsg {
    /*// List which sessions are available
    ListSessions,
    // Create a new session
    CreateSession,
    // Attach to a running session
    AttachToSession(SessionId, ClientType),
    // Force detach
    DetachSession(SessionId),
    // Disconnect from the session we're connected to
    DisconnectFromSession,*/
    ClientExit,
    TerminalResize(PositionAndSize),
    NewClient(PositionAndSize, CliArgs),
    Action(Action),
}

// Types of messages sent from the server to the client
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ServerToClientMsg {
    /*// Info about a particular session
    SessionInfo(Session),
    // A list of sessions
    SessionList(HashSet<Session>),*/
    Render(Option<String>),
    UnblockInputThread,
    Exit,
    ServerError(String),
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
    pub fn send(&mut self, msg: T) {
        let err_ctx = get_current_ctx();
        bincode::serialize_into(&mut self.sender, &(msg, err_ctx)).unwrap();
        self.sender.flush().unwrap();
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
    pub fn recv(&mut self) -> (T, ErrorContext) {
        bincode::deserialize_from(&mut self.receiver).unwrap()
    }

    /// Returns an [`IpcSenderWithContext`] with the same socket as this receiver.
    pub fn get_sender<F: Serialize>(&self) -> IpcSenderWithContext<F> {
        let sock_fd = self.receiver.get_ref().as_raw_fd();
        let dup_sock = dup(sock_fd).unwrap();
        let socket = unsafe { LocalSocketStream::from_raw_fd(dup_sock) };
        IpcSenderWithContext::new(socket)
    }
}
