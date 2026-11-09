//! Bounded channels carrying websocket messages to a single web client.
//!
//! The zellij server produces output as fast as it likes, a browser consumes it
//! as fast as its connection allows. These channels sit in between and have a
//! hard limit, both in the number of queued messages and in the number of
//! queued bytes. A sender that hits the limit gets `SendFailure::Full` and is
//! expected to disconnect the client instead of buffering without bound.

use axum::extract::ws::Message;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use tokio::sync::mpsc::{self, error::TrySendError};

/// Terminal output (screen updates, structured frames) for one client.
pub const OUTPUT_CHANNEL_MAX_MESSAGES: usize = 1024;
pub const OUTPUT_CHANNEL_MAX_BYTES: usize = 8 * 1024 * 1024;
/// Control messages for one client. These are small and infrequent.
pub const CONTROL_CHANNEL_MAX_MESSAGES: usize = 256;
pub const CONTROL_CHANNEL_MAX_BYTES: usize = 1024 * 1024;
/// Keepalive pings that bypass the output queue.
pub const KEEPALIVE_CHANNEL_MAX_MESSAGES: usize = 16;
pub const KEEPALIVE_CHANNEL_MAX_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SendFailure {
    /// The client is not keeping up: the queue is at its limit.
    Full,
    /// The receiving side is gone.
    Closed,
}

#[derive(Debug, Clone)]
pub struct ClientSender {
    tx: mpsc::Sender<Message>,
    queued_bytes: Arc<AtomicUsize>,
    max_bytes: usize,
}

#[derive(Debug)]
pub struct ClientReceiver {
    rx: mpsc::Receiver<Message>,
    queued_bytes: Arc<AtomicUsize>,
}

pub fn client_channel(max_messages: usize, max_bytes: usize) -> (ClientSender, ClientReceiver) {
    let (tx, rx) = mpsc::channel(max_messages);
    let queued_bytes = Arc::new(AtomicUsize::new(0));
    (
        ClientSender {
            tx,
            queued_bytes: queued_bytes.clone(),
            max_bytes,
        },
        ClientReceiver { rx, queued_bytes },
    )
}

fn message_size(message: &Message) -> usize {
    match message {
        Message::Text(text) => text.len(),
        Message::Binary(bytes) | Message::Ping(bytes) | Message::Pong(bytes) => bytes.len(),
        Message::Close(_) => 0,
    }
}

impl ClientSender {
    pub fn try_send(&self, message: Message) -> Result<(), SendFailure> {
        let size = message_size(&message);
        let queued_before = self.queued_bytes.fetch_add(size, Ordering::AcqRel);
        // a single message larger than the whole budget is still let through
        // when nothing else is queued, otherwise it could never be delivered
        if queued_before > 0 && queued_before.saturating_add(size) > self.max_bytes {
            self.queued_bytes.fetch_sub(size, Ordering::AcqRel);
            return Err(SendFailure::Full);
        }
        match self.tx.try_send(message) {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(_)) => {
                self.queued_bytes.fetch_sub(size, Ordering::AcqRel);
                Err(SendFailure::Full)
            },
            Err(TrySendError::Closed(_)) => {
                self.queued_bytes.fetch_sub(size, Ordering::AcqRel);
                Err(SendFailure::Closed)
            },
        }
    }
}

impl ClientReceiver {
    pub async fn recv(&mut self) -> Option<Message> {
        let message = self.rx.recv().await?;
        self.queued_bytes
            .fetch_sub(message_size(&message), Ordering::AcqRel);
        Some(message)
    }

    #[cfg(test)]
    pub fn try_recv(&mut self) -> Result<Message, mpsc::error::TryRecvError> {
        let message = self.rx.try_recv()?;
        self.queued_bytes
            .fetch_sub(message_size(&message), Ordering::AcqRel);
        Ok(message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(len: usize) -> Message {
        Message::Text("x".repeat(len).into())
    }

    #[test]
    fn refuses_messages_beyond_the_message_limit() {
        let (tx, mut rx) = client_channel(3, 1024);
        for _ in 0..3 {
            assert_eq!(tx.try_send(text(1)), Ok(()));
        }
        assert_eq!(tx.try_send(text(1)), Err(SendFailure::Full));
        // room comes back as the receiver drains
        rx.try_recv().unwrap();
        assert_eq!(tx.try_send(text(1)), Ok(()));
    }

    #[test]
    fn refuses_messages_beyond_the_byte_budget() {
        let (tx, mut rx) = client_channel(100, 10);
        assert_eq!(tx.try_send(text(6)), Ok(()));
        assert_eq!(tx.try_send(text(4)), Ok(()));
        assert_eq!(tx.try_send(text(1)), Err(SendFailure::Full));
        rx.try_recv().unwrap();
        assert_eq!(tx.try_send(text(1)), Ok(()));
    }

    #[test]
    fn one_oversized_message_is_delivered_when_the_queue_is_empty() {
        let (tx, mut rx) = client_channel(100, 10);
        assert_eq!(tx.try_send(text(50)), Ok(()));
        assert_eq!(tx.try_send(text(1)), Err(SendFailure::Full));
        rx.try_recv().unwrap();
        assert_eq!(tx.try_send(text(1)), Ok(()));
    }

    #[test]
    fn a_dropped_receiver_is_reported_as_closed() {
        let (tx, rx) = client_channel(4, 1024);
        drop(rx);
        assert_eq!(tx.try_send(text(1)), Err(SendFailure::Closed));
    }

    #[tokio::test]
    async fn bytes_are_released_when_messages_are_received() {
        let (tx, mut rx) = client_channel(100, 10);
        for _ in 0..100 {
            assert_eq!(tx.try_send(text(10)), Ok(()));
            assert!(rx.recv().await.is_some());
        }
    }
}
