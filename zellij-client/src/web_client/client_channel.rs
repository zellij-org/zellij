//! Bounded channels carrying websocket messages to a single web client.
//!
//! The zellij server produces output as fast as it likes, a browser consumes it
//! as fast as its connection allows. These channels sit in between and have a
//! hard limit, both in the number of queued messages and in the number of
//! queued bytes. Bytes stay reserved through dequeue until the websocket send
//! finishes or fails. A sender that hits the limit gets `SendFailure::Full` and
//! is expected to disconnect the client instead of buffering without bound.

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

/// Byte budget held from accept until the websocket write settles.
///
/// Drop releases the reservation, including when the send fails or the task is
/// cancelled. Dropping the [`Message`] alone does not.
#[must_use = "dropping the reservation releases the byte budget"]
pub struct ByteReservation {
    queued_bytes: Arc<AtomicUsize>,
    size: usize,
}

impl Drop for ByteReservation {
    fn drop(&mut self) {
        release_queued_bytes(&self.queued_bytes, self.size);
        self.size = 0;
    }
}

/// One dequeued websocket message whose bytes are still in the budget.
#[must_use = "hold the reservation until the websocket send finishes or fails"]
pub struct ReservedMessage {
    message: Message,
    reservation: ByteReservation,
}

impl ReservedMessage {
    pub fn into_parts(self) -> (Message, ByteReservation) {
        (self.message, self.reservation)
    }
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

fn release_queued_bytes(queued_bytes: &AtomicUsize, size: usize) {
    if size > 0 {
        queued_bytes.fetch_sub(size, Ordering::AcqRel);
    }
}

fn reserve_message(queued_bytes: &Arc<AtomicUsize>, message: Message) -> ReservedMessage {
    let size = message_size(&message);
    ReservedMessage {
        message,
        reservation: ByteReservation {
            queued_bytes: queued_bytes.clone(),
            size,
        },
    }
}

impl ClientSender {
    pub fn try_send(&self, message: Message) -> Result<(), SendFailure> {
        let size = message_size(&message);
        // A frame bigger than the cap can never be sent, including when the
        // queue is empty. Letting it through would blow the byte budget.
        if size > self.max_bytes {
            return Err(SendFailure::Full);
        }
        let queued_before = self.queued_bytes.fetch_add(size, Ordering::AcqRel);
        if queued_before.saturating_add(size) > self.max_bytes {
            release_queued_bytes(&self.queued_bytes, size);
            return Err(SendFailure::Full);
        }
        match self.tx.try_send(message) {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(_)) => {
                release_queued_bytes(&self.queued_bytes, size);
                Err(SendFailure::Full)
            },
            Err(TrySendError::Closed(_)) => {
                release_queued_bytes(&self.queued_bytes, size);
                Err(SendFailure::Closed)
            },
        }
    }

    #[cfg(test)]
    fn reserved_bytes_for_test(&self) -> usize {
        self.queued_bytes.load(Ordering::Acquire)
    }
}

impl ClientReceiver {
    pub async fn recv(&mut self) -> Option<ReservedMessage> {
        let message = self.rx.recv().await?;
        Some(reserve_message(&self.queued_bytes, message))
    }

    #[cfg(test)]
    pub fn try_recv(&mut self) -> Result<ReservedMessage, mpsc::error::TryRecvError> {
        let message = self.rx.try_recv()?;
        Ok(reserve_message(&self.queued_bytes, message))
    }
}

impl Drop for ClientReceiver {
    fn drop(&mut self) {
        // Messages never handed to a sender task are not in flight.
        while let Ok(message) = self.rx.try_recv() {
            release_queued_bytes(&self.queued_bytes, message_size(&message));
        }
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
        // a free queue slot comes back as the receiver drains
        let _released = rx.try_recv().unwrap();
        drop(_released);
        assert_eq!(tx.try_send(text(1)), Ok(()));
    }

    #[test]
    fn refuses_messages_beyond_the_byte_budget() {
        let (tx, mut rx) = client_channel(100, 10);
        assert_eq!(tx.try_send(text(6)), Ok(()));
        assert_eq!(tx.try_send(text(4)), Ok(()));
        assert_eq!(tx.try_send(text(1)), Err(SendFailure::Full));
        let in_flight = rx.try_recv().unwrap();
        assert_eq!(
            tx.try_send(text(1)),
            Err(SendFailure::Full),
            "dequeued bytes stay reserved until the send finishes"
        );
        drop(in_flight);
        assert_eq!(tx.try_send(text(1)), Ok(()));
    }

    #[test]
    fn rejects_an_oversized_frame_even_when_the_queue_is_empty() {
        let (tx, mut rx) = client_channel(100, 10);
        assert_eq!(tx.try_send(text(11)), Err(SendFailure::Full));
        assert_eq!(tx.try_send(text(10)), Ok(()));
        let in_flight = rx.try_recv().unwrap();
        assert_eq!(tx.try_send(text(1)), Err(SendFailure::Full));
        drop(in_flight);
        assert_eq!(tx.try_send(text(1)), Ok(()));
    }

    #[test]
    fn a_failed_send_releases_the_byte_reservation() {
        let (tx, mut rx) = client_channel(100, 10);
        assert_eq!(tx.try_send(text(10)), Ok(()));
        let in_flight = rx.try_recv().unwrap();
        assert_eq!(tx.try_send(text(1)), Err(SendFailure::Full));
        // The writer drops the reservation when the websocket write fails.
        drop(in_flight);
        assert_eq!(tx.try_send(text(10)), Ok(()));
    }

    #[test]
    fn a_dropped_receiver_is_reported_as_closed() {
        let (tx, rx) = client_channel(4, 1024);
        drop(rx);
        assert_eq!(tx.try_send(text(1)), Err(SendFailure::Closed));
    }

    #[tokio::test]
    async fn bytes_stay_reserved_until_the_send_settles() {
        let (tx, mut rx) = client_channel(100, 10);
        for _ in 0..100 {
            assert_eq!(tx.try_send(text(10)), Ok(()));
            let reserved = rx.recv().await.expect("queued message");
            assert_eq!(tx.try_send(text(1)), Err(SendFailure::Full));
            let (message, reservation) = reserved.into_parts();
            drop(message);
            assert_eq!(tx.try_send(text(1)), Err(SendFailure::Full));
            drop(reservation);
            assert_eq!(tx.reserved_bytes_for_test(), 0);
        }
    }
}
