//! Definitions and helpers for sending and receiving messages between threads.

use async_std::task_local;
use std::cell::RefCell;

use crate::errors::{get_current_ctx, ErrorContext};
pub use crossbeam::channel::{bounded, unbounded, Receiver, RecvError, SendError, Sender};

/// An [MPSC](mpsc) asynchronous channel with added error context.
pub type ChannelWithContext<T> = (Sender<(T, ErrorContext)>, Receiver<(T, ErrorContext)>);

/// Wrappers around the two standard [MPSC](mpsc) sender types, [`mpsc::Sender`] and [`mpsc::SyncSender`], with an additional [`ErrorContext`].
#[derive(Clone)]
pub enum SenderType<T: Clone> {
    /// A wrapper around an [`mpsc::Sender`], adding an [`ErrorContext`].
    Sender(Sender<(T, ErrorContext)>),
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
    pub fn send(&self, event: T) -> Result<(), SendError<(T, ErrorContext)>> {
        let err_ctx = get_current_ctx();
        match self.sender {
            SenderType::Sender(ref s) => s.send((event, err_ctx)),
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
