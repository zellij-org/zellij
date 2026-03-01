use crate::os_input_output::SignalEvent;

use async_trait::async_trait;
use signal_hook::consts::signal::*;
use signal_hook::iterator::Signals;
use tokio::signal::unix::{signal, SignalKind};

use anyhow::{Context, Result};
use std::io;
use std::io::Write;
use std::path::Path;
use zellij_utils::ipc::{IpcReceiverWithContext, IpcSenderWithContext};

/// Async signal listener that maps Unix signals to `SignalEvent` variants.
pub(crate) struct AsyncSignalListener {
    sigwinch: tokio::signal::unix::Signal,
    sigterm: tokio::signal::unix::Signal,
    sigint: tokio::signal::unix::Signal,
    sigquit: tokio::signal::unix::Signal,
    sighup: tokio::signal::unix::Signal,
}

impl AsyncSignalListener {
    pub fn new() -> io::Result<Self> {
        Ok(Self {
            sigwinch: signal(SignalKind::window_change())?,
            sigterm: signal(SignalKind::terminate())?,
            sigint: signal(SignalKind::interrupt())?,
            sigquit: signal(SignalKind::quit())?,
            sighup: signal(SignalKind::hangup())?,
        })
    }
}

#[async_trait]
impl crate::os_input_output::AsyncSignals for AsyncSignalListener {
    async fn recv(&mut self) -> Option<SignalEvent> {
        tokio::select! {
            result = self.sigwinch.recv() => result.map(|_| SignalEvent::Resize),
            result = self.sigterm.recv() => result.map(|_| SignalEvent::Quit),
            result = self.sigint.recv() => result.map(|_| SignalEvent::Quit),
            result = self.sigquit.recv() => result.map(|_| SignalEvent::Quit),
            result = self.sighup.recv() => result.map(|_| SignalEvent::Quit),
        }
    }
}

/// Blocking signal iterator that maps Unix signals to `SignalEvent` variants.
/// Used by `handle_signals()` on a dedicated thread.
pub(crate) struct BlockingSignalIterator {
    signals: Signals,
}

impl BlockingSignalIterator {
    pub fn new(
        _resize_receiver: Option<std::sync::mpsc::Receiver<()>>,
    ) -> io::Result<Self> {
        let signals = Signals::new([SIGWINCH, SIGTERM, SIGINT, SIGQUIT, SIGHUP])?;
        Ok(Self { signals })
    }
}

impl Iterator for BlockingSignalIterator {
    type Item = SignalEvent;

    fn next(&mut self) -> Option<SignalEvent> {
        for signal in self.signals.forever() {
            match signal {
                SIGWINCH => return Some(SignalEvent::Resize),
                SIGTERM | SIGINT | SIGQUIT | SIGHUP => return Some(SignalEvent::Quit),
                _ => {},
            }
        }
        None
    }
}

/// Set up client IPC channels from a connected socket.
///
/// On Unix a single socket is cloned for both send and receive directions.
pub(crate) fn setup_ipc(
    socket: interprocess::local_socket::Stream,
    _path: &Path,
) -> (
    IpcSenderWithContext<zellij_utils::ipc::ClientToServerMsg>,
    IpcReceiverWithContext<zellij_utils::ipc::ServerToClientMsg>,
) {
    let sender = IpcSenderWithContext::new(socket);
    let receiver = sender.get_receiver();
    (sender, receiver)
}

pub(crate) fn enable_mouse_support(stdout: &mut dyn Write) -> Result<()> {
    let err_context = "failed to enable mouse mode";
    stdout
        .write_all(super::os_input_output::ENABLE_MOUSE_SUPPORT.as_bytes())
        .context(err_context)?;
    stdout.flush().context(err_context)?;
    Ok(())
}

pub(crate) fn disable_mouse_support(stdout: &mut dyn Write) -> Result<()> {
    let err_context = "failed to disable mouse mode";
    stdout
        .write_all(super::os_input_output::DISABLE_MOUSE_SUPPORT.as_bytes())
        .context(err_context)?;
    stdout.flush().context(err_context)?;
    Ok(())
}
