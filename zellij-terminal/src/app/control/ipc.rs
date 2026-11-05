use std::path::Path;

use anyhow::{Context, Result};
use interprocess::local_socket::{Listener, Stream};

#[cfg_attr(windows, path = "ipc/windows.rs")]
#[cfg_attr(not(windows), path = "ipc/unix.rs")]
mod os;

pub use os::{connect_reply, remove_stale};

pub struct Listeners {
    pub listener: Listener,
    replies: os::Replies,
}

impl Listeners {
    pub fn bind(path: &Path) -> Result<Self> {
        let listener = zellij_utils::consts::ipc_bind(path)
            .with_context(|| format!("failed to listen on {}", path.display()))?;
        let replies = os::Replies::bind(path)?;
        Ok(Self { listener, replies })
    }

    pub fn reply_to(&self, stream: &Stream) -> std::io::Result<Stream> {
        self.replies.reply_to(stream)
    }

    pub fn push_reply(&self) -> std::io::Result<Option<Stream>> {
        self.replies.push_reply()
    }
}
