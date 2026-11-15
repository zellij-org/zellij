//! Bound accepted sockets that have not yet produced a client message.
//!
//! The listener used to spawn a router thread per `accept` with no cap. Each
//! router blocks in the generic IPC reader, which allocates the frame body
//! from the 4-byte length prefix (up to 256 MiB) before the message is
//! decoded. These helpers keep that window finite and let the first frame be
//! rejected before that allocation.

use std::io::{self, Read};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

/// Simultaneous accepted connections that may still be reading their first
/// client message. Each one occupies a router thread for up to the
/// pre-handshake read timeout. 64 is enough for a burst of attaches and
/// status probes, and small enough that a connect flood cannot create an
/// unbounded number of threads before any message is decoded.
pub const PRE_HANDSHAKE_CONNECTION_CEILING: usize = 64;

/// Largest frame accepted as the first message on a new connection.
///
/// Handshakes (`ConnStatus`, attach, the first client instruction) are small
/// protobufs. 1 MiB is far above those and far below the 256 MiB cap used for
/// later, already-established frames (layout serialization and similar), so
/// this does not shrink legitimate traffic after the first message.
pub const HANDSHAKE_FRAME_BYTES: usize = 1024 * 1024;

/// Why a router was not started for an accepted stream.
#[derive(Debug)]
pub enum RouterSpawnError {
    /// `PRE_HANDSHAKE_CONNECTION_CEILING` (or the test ceiling) is already in use.
    CeilingReached,
    /// The OS refused to create the thread. The admission permit is released.
    Spawn(io::Error),
}

/// Counts in-flight pre-handshake connections.
#[derive(Debug)]
pub struct PreHandshakeLimiter {
    in_flight: Arc<AtomicUsize>,
    ceiling: usize,
}

impl PreHandshakeLimiter {
    pub fn new(ceiling: usize) -> Self {
        Self {
            in_flight: Arc::new(AtomicUsize::new(0)),
            ceiling,
        }
    }

    pub fn production() -> Self {
        Self::new(PRE_HANDSHAKE_CONNECTION_CEILING)
    }

    pub fn in_flight(&self) -> usize {
        self.in_flight.load(Ordering::SeqCst)
    }

    /// Reserve one pre-handshake slot. `None` means the caller must drop the
    /// accepted stream and must not spawn a router.
    pub fn try_admit(&self) -> Option<PreHandshakePermit> {
        let mut current = self.in_flight.load(Ordering::SeqCst);
        loop {
            if current >= self.ceiling {
                return None;
            }
            match self.in_flight.compare_exchange(
                current,
                current + 1,
                Ordering::SeqCst,
                Ordering::SeqCst,
            ) {
                Ok(_) => {
                    return Some(PreHandshakePermit {
                        in_flight: Arc::clone(&self.in_flight),
                        armed: true,
                    });
                },
                Err(observed) => current = observed,
            }
        }
    }

    /// Admit one connection and start its router.
    ///
    /// `body` owns the permit. Dropping the permit (router exit or failure
    /// before the first decoded client message) releases the slot. On a
    /// failed `thread::Builder::spawn` the permit is released here: the
    /// closure never runs, and the slot is cleared explicitly so a runtime
    /// that dropped the closure without running it cannot leak the count.
    pub fn spawn_router<F>(
        &self,
        build: impl FnOnce() -> thread::Builder,
        body: F,
    ) -> Result<(), RouterSpawnError>
    where
        F: FnOnce(PreHandshakePermit) + Send + 'static,
    {
        let Some(permit) = self.try_admit() else {
            return Err(RouterSpawnError::CeilingReached);
        };
        let slot = Arc::new(Mutex::new(Some(permit)));
        let slot_for_thread = Arc::clone(&slot);
        match build().spawn(move || {
            let permit = slot_for_thread
                .lock()
                .unwrap_or_else(|poison| poison.into_inner())
                .take()
                .expect("pre-handshake permit");
            body(permit);
        }) {
            Ok(_handle) => Ok(()),
            Err(error) => {
                drop(
                    slot.lock()
                        .unwrap_or_else(|poison| poison.into_inner())
                        .take(),
                );
                Err(RouterSpawnError::Spawn(error))
            },
        }
    }
}

/// Held from accept until the first client message is decoded, or until the
/// router exits or fails before that.
pub struct PreHandshakePermit {
    in_flight: Arc<AtomicUsize>,
    armed: bool,
}

impl PreHandshakePermit {
    /// The first client message was decoded. The connection is no longer
    /// pre-handshake, so it must not keep occupying the ceiling.
    pub fn release_after_first_message(mut self) {
        self.disarm();
    }

    fn disarm(&mut self) {
        if self.armed {
            self.in_flight.fetch_sub(1, Ordering::SeqCst);
            self.armed = false;
        }
    }
}

impl Drop for PreHandshakePermit {
    fn drop(&mut self) {
        self.disarm();
    }
}

/// Failure reading a length-prefixed frame without allocating an oversized body.
#[derive(Debug, PartialEq, Eq)]
pub enum CappedFrameError {
    /// The peer closed the stream or the read failed before a full frame.
    Disconnected,
    /// `claimed` is the length prefix. The body was not read and not allocated.
    Oversized { claimed: usize },
}

/// Read one length-prefixed frame, refusing to allocate when `claimed > cap`.
///
/// The length is a little-endian `u32`, matching Zellij IPC. Callers that
/// still need the generic 256 MiB cap pass that cap; the handshake path
/// passes [`HANDSHAKE_FRAME_BYTES`].
pub fn read_capped_frame(reader: &mut impl Read, cap: usize) -> Result<Vec<u8>, CappedFrameError> {
    let mut len_bytes = [0u8; 4];
    if reader.read_exact(&mut len_bytes).is_err() {
        return Err(CappedFrameError::Disconnected);
    }
    let claimed = u32::from_le_bytes(len_bytes) as usize;
    if claimed > cap {
        return Err(CappedFrameError::Oversized { claimed });
    }
    let mut buf = vec![0u8; claimed];
    if reader.read_exact(&mut buf).is_err() {
        return Err(CappedFrameError::Disconnected);
    }
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Barrier;
    use std::time::Duration;

    struct PrefixThenTrap {
        bytes: Vec<u8>,
        pos: usize,
        extra_reads: Arc<AtomicUsize>,
    }

    impl Read for PrefixThenTrap {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            if self.pos >= self.bytes.len() {
                self.extra_reads.fetch_add(1, Ordering::SeqCst);
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "body must not be read",
                ));
            }
            let n = (self.bytes.len() - self.pos).min(buf.len());
            buf[..n].copy_from_slice(&self.bytes[self.pos..self.pos + n]);
            self.pos += n;
            Ok(n)
        }
    }

    #[test]
    fn ceiling_rejects_further_admits_without_growing_the_permit() {
        let limiter = PreHandshakeLimiter::new(1);
        let started = Arc::new(AtomicUsize::new(0));
        let release = Arc::new(Barrier::new(2));
        let started_gate = Arc::new(Barrier::new(2));
        let running = Arc::clone(&started);
        let release_for_thread = Arc::clone(&release);
        let started_for_thread = Arc::clone(&started_gate);
        limiter
            .spawn_router(
                || thread::Builder::new(),
                move |_permit| {
                    running.fetch_add(1, Ordering::SeqCst);
                    started_for_thread.wait();
                    release_for_thread.wait();
                },
            )
            .expect("first router");
        started_gate.wait();
        assert_eq!(limiter.in_flight(), 1);

        let started_if_spawned = Arc::clone(&started);
        let rejected = limiter.spawn_router(
            || thread::Builder::new(),
            move |_| {
                started_if_spawned.fetch_add(1, Ordering::SeqCst);
            },
        );
        assert!(matches!(rejected, Err(RouterSpawnError::CeilingReached)));
        assert_eq!(
            started.load(Ordering::SeqCst),
            1,
            "rejected accept spawned work"
        );
        assert_eq!(limiter.in_flight(), 1);

        release.wait();
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        while limiter.in_flight() != 0 {
            if std::time::Instant::now() > deadline {
                panic!("permit was not released when the connection ended");
            }
            thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn failed_thread_spawn_releases_the_permit() {
        let limiter = PreHandshakeLimiter::new(1);
        let error = limiter
            .spawn_router(
                || thread::Builder::new().stack_size(usize::MAX),
                |_permit| {},
            )
            .expect_err("usize::MAX stack must not spawn");
        assert!(matches!(error, RouterSpawnError::Spawn(_)));
        assert_eq!(limiter.in_flight(), 0);
        let again = limiter.try_admit();
        assert!(again.is_some(), "a failed spawn leaked the only permit");
    }

    #[test]
    fn explicit_handshake_release_frees_the_slot_before_the_thread_exits() {
        let limiter = PreHandshakeLimiter::new(1);
        let released = Arc::new(Barrier::new(2));
        let released_for_thread = Arc::clone(&released);
        limiter
            .spawn_router(
                || thread::Builder::new(),
                move |permit| {
                    permit.release_after_first_message();
                    released_for_thread.wait();
                },
            )
            .unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        while limiter.in_flight() != 0 {
            if std::time::Instant::now() > deadline {
                panic!("handshake release did not free the permit");
            }
            thread::sleep(Duration::from_millis(10));
        }
        let second = limiter.try_admit();
        assert!(second.is_some());
        drop(second);
        released.wait();
    }

    #[test]
    fn oversized_length_prefix_does_not_read_or_allocate_the_body() {
        let claimed: u32 = (HANDSHAKE_FRAME_BYTES as u32).saturating_add(1);
        let extra_reads = Arc::new(AtomicUsize::new(0));
        let mut reader = PrefixThenTrap {
            bytes: claimed.to_le_bytes().to_vec(),
            pos: 0,
            extra_reads: Arc::clone(&extra_reads),
        };
        let error = read_capped_frame(&mut reader, HANDSHAKE_FRAME_BYTES).unwrap_err();
        assert_eq!(
            error,
            CappedFrameError::Oversized {
                claimed: claimed as usize
            }
        );
        assert_eq!(extra_reads.load(Ordering::SeqCst), 0);
        assert!(HANDSHAKE_FRAME_BYTES < 256 * 1024 * 1024);
        assert_eq!(HANDSHAKE_FRAME_BYTES, 1024 * 1024);
    }

    #[test]
    fn a_small_first_frame_is_admitted() {
        let body = b"ping";
        let mut bytes = (body.len() as u32).to_le_bytes().to_vec();
        bytes.extend_from_slice(body);
        let extra_reads = Arc::new(AtomicUsize::new(0));
        let mut reader = PrefixThenTrap {
            bytes,
            pos: 0,
            extra_reads: Arc::clone(&extra_reads),
        };
        let frame = read_capped_frame(&mut reader, HANDSHAKE_FRAME_BYTES).unwrap();
        assert_eq!(frame, body);
        assert_eq!(extra_reads.load(Ordering::SeqCst), 0);
    }
}
