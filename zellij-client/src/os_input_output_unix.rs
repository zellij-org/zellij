use crate::os_input_output::SignalEvent;

use async_trait::async_trait;
use signal_hook::consts::signal::*;
use signal_hook::iterator::Signals;
use tokio::signal::unix::{signal, SignalKind};

use std::io;

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
    pub fn new() -> io::Result<Self> {
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
