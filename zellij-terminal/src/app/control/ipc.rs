use std::path::Path;

use anyhow::{Context, Result};
use interprocess::local_socket::{Listener, Stream};

#[cfg_attr(windows, path = "ipc/windows.rs")]
#[cfg_attr(not(windows), path = "ipc/unix.rs")]
mod os;

pub use os::{connect, remove_stale};

pub struct Listeners {
    listener: Listener,
    replies: os::Replies,
}

impl Listeners {
    pub fn bind(path: &Path) -> Result<Self> {
        let listener = zellij_utils::consts::ipc_bind(path)
            .with_context(|| format!("failed to listen on {}", path.display()))?;
        let replies = os::Replies::bind(path)?;
        Ok(Self { listener, replies })
    }

    /// Each connection that comes in, with the stream its answer goes back on.
    pub fn connections(self) -> impl Iterator<Item = (Stream, Stream)> {
        os::connections(self.listener, self.replies)
    }

    /// Each connection that comes in, with its own reply stream where the platform needs one.
    pub fn pushes(self) -> impl Iterator<Item = (Stream, Option<Stream>)> {
        os::pushes(self.listener, self.replies)
    }
}
