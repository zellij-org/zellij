use std::io;
use std::path::Path;

use anyhow::Result;
use interprocess::local_socket::traits::Listener as _;
use interprocess::local_socket::{Listener, Stream};
use interprocess::TryClone;

use super::Listeners;

pub struct Replies;

impl Replies {
    pub fn bind(_path: &Path) -> Result<Self> {
        Ok(Replies)
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

/// A socket is two-way, so a connection answers on itself.
pub fn connections(
    listener: Listener,
    _replies: Replies,
) -> impl Iterator<Item = (Stream, Stream)> {
    pushes(listener, Replies).filter_map(|(stream, _)| {
        let reply = TryClone::try_clone(&stream).ok()?;
        Some((stream, reply))
    })
}

pub fn pushes(
    listener: Listener,
    _replies: Replies,
) -> impl Iterator<Item = (Stream, Option<Stream>)> {
    std::iter::from_fn(move || loop {
        if let Ok(stream) = listener.accept() {
            return Some((stream, None));
        }
    })
}

pub fn remove_stale(path: &Path) -> io::Result<()> {
    std::fs::remove_file(path)
}

pub fn connect(socket: &Path) -> io::Result<(Stream, Stream)> {
    let stream = zellij_utils::consts::ipc_connect(socket)?;
    let reply = TryClone::try_clone(&stream)?;
    Ok((stream, reply))
}
