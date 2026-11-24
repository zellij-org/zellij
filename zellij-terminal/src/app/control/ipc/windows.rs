use std::io;
use std::path::Path;

use anyhow::{Context, Result};
use interprocess::local_socket::{Listener, Stream};
use zellij_utils::pipe_pairs;

pub struct Replies(Listener);

impl Replies {
    pub fn bind(path: &Path) -> Result<Self> {
        zellij_utils::consts::ipc_bind_reply(path)
            .map(Replies)
            .with_context(|| format!("failed to listen on {}-reply", path.display()))
    }
}

/// A connection's request and reply pipes are accepted separately, and paired up by the client
/// process that connected them.
pub fn connections(listener: Listener, replies: Replies) -> impl Iterator<Item = (Stream, Stream)> {
    pipe_pairs::accept_pairs(listener, replies.0).into_iter()
}

pub fn pushes(
    listener: Listener,
    replies: Replies,
) -> impl Iterator<Item = (Stream, Option<Stream>)> {
    connections(listener, replies).map(|(stream, reply)| (stream, Some(reply)))
}

pub fn remove_stale(_path: &Path) -> io::Result<()> {
    Ok(())
}

pub fn connect(socket: &Path) -> io::Result<(Stream, Stream)> {
    pipe_pairs::connect_pair(socket)
}
