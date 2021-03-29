//! IPC stuff for starting to split things into a client and server model.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;

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
#[derive(Serialize, Deserialize)]
pub enum _ClientToServerMsg {
    // List which sessions are available
    ListSessions,
    // Create a new session
    CreateSession,
    // Attach to a running session
    AttachToSession(SessionId, ClientType),
    // Force detach
    DetachSession(SessionId),
    // Disconnect from the session we're connected to
    DisconnectFromSession,
}

// Types of messages sent from the server to the client
// @@@ Implement Serialize and Deserialize for this...
pub enum _ServerToClientMsg {
    // Info about a particular session
    SessionInfo(Session),
    // A list of sessions
    SessionList(HashSet<Session>),
}
