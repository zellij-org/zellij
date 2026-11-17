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
    pub fn new(_resize_receiver: Option<std::sync::mpsc::Receiver<()>>) -> io::Result<Self> {
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
/// Connects to the server at `path`, waiting for it to start listening.
pub(crate) fn connect_ipc(
    path: &Path,
    give_up_after: Option<std::time::Duration>,
) -> std::io::Result<(
    IpcSenderWithContext<zellij_utils::ipc::ClientToServerMsg>,
    IpcReceiverWithContext<zellij_utils::ipc::ServerToClientMsg>,
)> {
    let started = std::time::Instant::now();
    let socket = loop {
        match zellij_utils::consts::ipc_connect(path) {
            Ok(socket) => break socket,
            Err(e) => {
                if let Some(limit) = give_up_after {
                    if e.kind() == io::ErrorKind::NotFound || started.elapsed() >= limit {
                        return Err(e);
                    }
                }
                std::thread::sleep(std::time::Duration::from_millis(50))
            },
        }
    };
    let sender = IpcSenderWithContext::new(socket);
    let receiver = sender.get_receiver();
    Ok((sender, receiver))
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn a_session_whose_socket_is_gone_is_reported_at_once() {
        let dir = tempfile::tempdir().unwrap();
        let started = Instant::now();
        let error = connect_ipc(&dir.path().join("gone"), Some(Duration::from_secs(30)))
            .err()
            .expect("there is nothing to connect to");
        assert_eq!(error.kind(), io::ErrorKind::NotFound);
        assert!(started.elapsed() < Duration::from_secs(5));
        assert_eq!(
            crate::connect_error_message(error),
            "Error: the session is not running"
        );
    }

    #[test]
    fn a_socket_nobody_listens_on_is_given_up_on_after_the_limit() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("stale");
        drop(std::os::unix::net::UnixListener::bind(&path).unwrap());
        assert!(path.exists());
        let limit = Duration::from_millis(300);
        let started = Instant::now();
        let error = connect_ipc(&path, Some(limit))
            .err()
            .expect("nobody listens on a stale socket");
        assert_ne!(error.kind(), io::ErrorKind::NotFound);
        assert!(started.elapsed() >= limit);
        assert!(started.elapsed() < Duration::from_secs(5));
        assert!(crate::connect_error_message(error).starts_with("Error: cannot connect"));
    }

    #[test]
    fn a_server_that_is_listening_is_reached_within_the_limit() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("live");
        let _listener = std::os::unix::net::UnixListener::bind(&path).unwrap();
        assert!(connect_ipc(&path, Some(Duration::from_secs(5))).is_ok());
    }
}
