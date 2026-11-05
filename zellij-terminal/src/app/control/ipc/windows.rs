use std::io;
use std::path::Path;

use anyhow::{Context, Result};
use interprocess::local_socket::prelude::*;
use interprocess::local_socket::{Listener, Stream};

pub struct Replies(Listener);

impl Replies {
    pub fn bind(path: &Path) -> Result<Self> {
        zellij_utils::consts::ipc_bind_reply(path)
            .map(Replies)
            .with_context(|| format!("failed to listen on {}-reply", path.display()))
    }

    pub fn reply_to(&self, _stream: &Stream) -> io::Result<Stream> {
        self.0.accept()
    }

    pub fn push_reply(&self) -> io::Result<Option<Stream>> {
        self.0.accept().map(Some)
    }
}

pub fn remove_stale(_path: &Path) -> io::Result<()> {
    Ok(())
}

pub fn connect_reply(socket: &Path, _stream: &Stream) -> io::Result<Stream> {
    zellij_utils::consts::ipc_connect_reply(socket)
}
