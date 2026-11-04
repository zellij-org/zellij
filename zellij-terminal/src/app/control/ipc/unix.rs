use std::io;
use std::path::Path;

use anyhow::Result;
use interprocess::local_socket::{Listener, Stream};
use interprocess::TryClone;

use super::Listeners;

pub struct Replies;

impl Replies {
    pub fn bind(_path: &Path) -> Result<Self> {
        Ok(Replies)
    }

    pub fn reply_to(&self, stream: &Stream) -> io::Result<Stream> {
        TryClone::try_clone(stream)
    }

    pub fn push_reply(&self) -> io::Result<Option<Stream>> {
        Ok(None)
    }
}

impl Listeners {
    pub fn of(listener: Listener) -> Self {
        Listeners {
            listener,
            replies: Replies,
        }
    }
}

pub fn remove_stale(path: &Path) -> io::Result<()> {
    std::fs::remove_file(path)
}

pub fn connect_reply(_socket: &Path, stream: &Stream) -> io::Result<Stream> {
    TryClone::try_clone(stream)
}
