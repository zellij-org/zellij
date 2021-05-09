//! Definitions and helpers for sending and receiving messages between threads.

use async_std::task_local;
use std::cell::RefCell;
use std::sync::mpsc;

use crate::common::pty::PtyInstruction;
use crate::common::ServerInstruction;
use crate::errors::{get_current_ctx, ErrorContext};
use crate::os_input_output::ServerOsApi;
use crate::screen::ScreenInstruction;
use crate::wasm_vm::PluginInstruction;

/// An [MPSC](mpsc) asynchronous channel with added error context.
pub type ChannelWithContext<T> = (
    mpsc::Sender<(T, ErrorContext)>,
    mpsc::Receiver<(T, ErrorContext)>,
);
/// An [MPSC](mpsc) synchronous channel with added error context.
pub type SyncChannelWithContext<T> = (
    mpsc::SyncSender<(T, ErrorContext)>,
    mpsc::Receiver<(T, ErrorContext)>,
);

/// Wrappers around the two standard [MPSC](mpsc) sender types, [`mpsc::Sender`] and [`mpsc::SyncSender`], with an additional [`ErrorContext`].
#[derive(Clone)]
pub enum SenderType<T: Clone> {
    /// A wrapper around an [`mpsc::Sender`], adding an [`ErrorContext`].
    Sender(mpsc::Sender<(T, ErrorContext)>),
    /// A wrapper around an [`mpsc::SyncSender`], adding an [`ErrorContext`].
    SyncSender(mpsc::SyncSender<(T, ErrorContext)>),
}

/// Sends messages on an [MPSC](std::sync::mpsc) channel, along with an [`ErrorContext`],
/// synchronously or asynchronously depending on the underlying [`SenderType`].
#[derive(Clone)]
pub struct SenderWithContext<T: Clone> {
    sender: SenderType<T>,
}

impl<T: Clone> SenderWithContext<T> {
    pub fn new(sender: SenderType<T>) -> Self {
        Self { sender }
    }

    /// Sends an event, along with the current [`ErrorContext`], on this
    /// [`SenderWithContext`]'s channel.
    pub fn send(&self, event: T) -> Result<(), mpsc::SendError<(T, ErrorContext)>> {
        let err_ctx = get_current_ctx();
        match self.sender {
            SenderType::Sender(ref s) => s.send((event, err_ctx)),
            SenderType::SyncSender(ref s) => s.send((event, err_ctx)),
        }
    }
}

unsafe impl<T: Clone> Send for SenderWithContext<T> {}
unsafe impl<T: Clone> Sync for SenderWithContext<T> {}

thread_local!(
    /// A key to some thread local storage (TLS) that holds a representation of the thread's call
    /// stack in the form of an [`ErrorContext`].
    pub static OPENCALLS: RefCell<ErrorContext> = RefCell::default()
);

task_local! {
    /// A key to some task local storage that holds a representation of the task's call
    /// stack in the form of an [`ErrorContext`].
    pub static ASYNCOPENCALLS: RefCell<ErrorContext> = RefCell::default()
}

/// A container for senders to the different threads in zellij on the server side
#[derive(Clone)]
pub struct ThreadSenders {
    pub to_screen: Option<SenderWithContext<ScreenInstruction>>,
    pub to_pty: Option<SenderWithContext<PtyInstruction>>,
    pub to_plugin: Option<SenderWithContext<PluginInstruction>>,
    pub to_server: Option<SenderWithContext<ServerInstruction>>,
}

impl ThreadSenders {
    pub fn send_to_screen(
        &self,
        instruction: ScreenInstruction,
    ) -> Result<(), mpsc::SendError<(ScreenInstruction, ErrorContext)>> {
        self.to_screen.as_ref().unwrap().send(instruction)
    }

    pub fn send_to_pty(
        &self,
        instruction: PtyInstruction,
    ) -> Result<(), mpsc::SendError<(PtyInstruction, ErrorContext)>> {
        self.to_pty.as_ref().unwrap().send(instruction)
    }

    pub fn send_to_plugin(
        &self,
        instruction: PluginInstruction,
    ) -> Result<(), mpsc::SendError<(PluginInstruction, ErrorContext)>> {
        self.to_plugin.as_ref().unwrap().send(instruction)
    }

    pub fn send_to_server(
        &self,
        instruction: ServerInstruction,
    ) -> Result<(), mpsc::SendError<(ServerInstruction, ErrorContext)>> {
        self.to_server.as_ref().unwrap().send(instruction)
    }
}

/// A container for a receiver, OS input and the senders to a given thread
pub struct Bus<T> {
    pub receiver: mpsc::Receiver<(T, ErrorContext)>,
    pub senders: ThreadSenders,
    pub os_input: Option<Box<dyn ServerOsApi>>,
}

impl<T> Bus<T> {
    pub fn new(
        receiver: mpsc::Receiver<(T, ErrorContext)>,
        to_screen: Option<&SenderWithContext<ScreenInstruction>>,
        to_pty: Option<&SenderWithContext<PtyInstruction>>,
        to_plugin: Option<&SenderWithContext<PluginInstruction>>,
        to_server: Option<&SenderWithContext<ServerInstruction>>,
        os_input: Option<Box<dyn ServerOsApi>>,
    ) -> Self {
        Bus {
            receiver,
            senders: ThreadSenders {
                to_screen: to_screen.cloned(),
                to_pty: to_pty.cloned(),
                to_plugin: to_plugin.cloned(),
                to_server: to_server.cloned(),
            },
            os_input: os_input.clone(),
        }
    }

    pub fn recv(&self) -> Result<(T, ErrorContext), mpsc::RecvError> {
        self.receiver.recv()
    }
}
