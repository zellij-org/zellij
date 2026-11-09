use std::collections::{HashMap, VecDeque};
use std::time::Duration;

use zellij_utils::channels;
use zellij_utils::errors::{prelude::*, ContextType, PtyWriteContext};

use crate::diagnostics;
use crate::os_input_output::ServerOsApi;
use crate::route::NotificationEnd;
use crate::thread_bus::Bus;

// we separate these instruction to a different thread because some programs get deadlocked if
// you write into their STDIN while reading from their STDOUT (I'm looking at you, vim)
// while the same has not been observed to happen with resizes, it could conceivably happen and we have this
// here anyway, so
#[derive(Debug, Clone)]
pub enum PtyWriteInstruction {
    Write(Vec<u8>, u32, Option<NotificationEnd>),
    ResizePty(u32, u16, u16, Option<u16>, Option<u16>),
    StartCachingResizes,
    ApplyCachedResizes,
    Exit,
}

impl From<&PtyWriteInstruction> for PtyWriteContext {
    fn from(tty_write_instruction: &PtyWriteInstruction) -> Self {
        match *tty_write_instruction {
            PtyWriteInstruction::Write(..) => PtyWriteContext::Write,
            PtyWriteInstruction::ResizePty(..) => PtyWriteContext::ResizePty,
            PtyWriteInstruction::ApplyCachedResizes => PtyWriteContext::ApplyCachedResizes,
            PtyWriteInstruction::StartCachingResizes => PtyWriteContext::StartCachingResizes,
            PtyWriteInstruction::Exit => PtyWriteContext::Exit,
        }
    }
}

/// Maximum pending bytes per terminal before dropping the buffer.
///
/// Exposed (but not `pub`) so tests in this module can reference it.
#[allow(dead_code)]
const MAX_PENDING_BYTES: usize = 10 * 1024 * 1024;

/// When pending writes exist, poll for new instructions with this timeout
/// before retrying the drain. Keeps the thread responsive to new instructions
/// while still retrying stuck terminals.
const PENDING_DRAIN_TIMEOUT: Duration = Duration::from_millis(10);

/// Maximum bytes one terminal may write in a single drain pass before the
/// next terminal gets a turn.
const MAX_DRAIN_BYTES_PER_TURN: usize = 256 * 1024;

/// Maximum queue steps (write calls, or retired empty writes) one terminal may
/// make in a single drain pass.
/// Bounds the turn even when the consumer accepts only a few bytes per
/// write call.
const MAX_DRAIN_CALLS_PER_TURN: usize = 32;

/// A chunk of bytes waiting to be written to a terminal's stdin.
struct PendingWrite {
    bytes: Vec<u8>,
    offset: usize,
    completion: Option<NotificationEnd>,
}

#[derive(Default)]
struct PendingQueue {
    writes: VecDeque<PendingWrite>,
    remaining_bytes: usize,
}

impl PendingQueue {
    fn is_empty(&self) -> bool {
        self.writes.is_empty()
    }

    fn push_back(&mut self, write: PendingWrite) {
        self.remaining_bytes = self
            .remaining_bytes
            .saturating_add(write.bytes.len().saturating_sub(write.offset));
        self.writes.push_back(write);
    }

    fn front_remaining(&self) -> Option<&[u8]> {
        self.writes
            .front()
            .map(|write| write.bytes.get(write.offset..).unwrap_or_default())
    }

    fn advance(&mut self, written: usize) {
        if let Some(write) = self.writes.front_mut() {
            write.offset = write.offset.saturating_add(written);
            self.remaining_bytes = self.remaining_bytes.saturating_sub(written);
            if write.offset >= write.bytes.len() {
                self.writes.pop_front();
            }
        }
    }

    /// Discard every queued write, reporting the discard as a truthful
    /// failure on each write's completion token instead of the silent
    /// success an unmarked drop would produce.
    fn fail_all(&mut self, reason: &str) {
        for mut write in self.writes.drain(..) {
            if let Some(mut completion) = write.completion.take() {
                completion.set_error_message(reason.to_string());
            }
        }
        self.remaining_bytes = 0;
    }
}

pub(crate) fn pty_writer_main(bus: Bus<PtyWriteInstruction>) -> Result<()> {
    let err_context = || "failed to write to pty".to_string();
    let mut pending: HashMap<u32, PendingQueue> = HashMap::new();
    let mut drain_made_progress = false;

    loop {
        // If we have pending writes, use a short timeout so we can keep
        // draining. Otherwise, block until a new instruction arrives.
        // While the last pass still wrote bytes, poll without waiting so a
        // large burst keeps its throughput; only fall back to the timeout
        // when every pending terminal refused bytes in the previous pass.
        let has_pending = pending.values().any(|q| !q.is_empty());
        let event = if has_pending {
            let timeout = if drain_made_progress {
                Duration::ZERO
            } else {
                PENDING_DRAIN_TIMEOUT
            };
            match bus.recv_timeout(timeout) {
                Ok(pair) => Some(pair),
                Err(channels::RecvTimeoutError::Timeout) => None,
                Err(channels::RecvTimeoutError::Disconnected) => return Ok(()),
            }
        } else {
            Some(bus.recv().with_context(err_context)?)
        };

        let mut os_input = bus
            .os_input
            .clone()
            .context("no OS input API found")
            .with_context(err_context)?;

        // Process the instruction if we received one
        if let Some((event, mut err_ctx)) = event {
            err_ctx.add_call(ContextType::PtyWrite((&event).into()));
            match event {
                PtyWriteInstruction::Write(bytes, terminal_id, completion) => {
                    let queue = pending.entry(terminal_id).or_default();
                    if queue.remaining_bytes.saturating_add(bytes.len()) > MAX_PENDING_BYTES {
                        log::error!(
                            "dropping write buffer for terminal {} \
                             ({} + {} bytes exceeds {} limit)",
                            terminal_id,
                            queue.remaining_bytes,
                            bytes.len(),
                            MAX_PENDING_BYTES,
                        );
                        queue.fail_all(
                            "pty write queue overflowed the pending byte limit; \
                             the write was discarded before reaching the terminal",
                        );
                        if let Some(mut completion) = completion {
                            completion.set_error_message(
                                "pty write rejected: pending byte limit already exceeded"
                                    .to_string(),
                            );
                        }
                    } else {
                        queue.push_back(PendingWrite {
                            bytes,
                            offset: 0,
                            completion,
                        });
                        diagnostics::record_pty_pending_bytes(queue.remaining_bytes);
                    }
                },
                PtyWriteInstruction::ResizePty(
                    terminal_id,
                    columns,
                    rows,
                    width_in_pixels,
                    height_in_pixels,
                ) => {
                    os_input
                        .set_terminal_size_using_terminal_id(
                            terminal_id,
                            columns,
                            rows,
                            width_in_pixels,
                            height_in_pixels,
                        )
                        .with_context(err_context)
                        .non_fatal();
                },
                PtyWriteInstruction::StartCachingResizes => {
                    // we do this because there are some logic traps inside the screen/tab/layout code
                    // the cause multiple resizes to be sent to the pty - while the last one is always
                    // the correct one, many programs and shells debounce those (I guess due to the
                    // trauma of dealing with GUI resizes of the controlling terminal window), and this
                    // then causes glitches and missing redraws
                    // so we do this to play nice and always only send the last resize instruction to
                    // each pane
                    // the logic for this happens in the main Screen event loop
                    os_input.cache_resizes();
                },
                PtyWriteInstruction::ApplyCachedResizes => {
                    os_input.apply_cached_resizes();
                },
                PtyWriteInstruction::Exit => {
                    return Ok(());
                },
            }
        }

        // Drain pending writes — one pass across all terminals. Each
        // terminal gets a bounded turn so a fast consumer with a large
        // queued burst cannot monopolize the writer while other terminals
        // wait; partial-write offsets keep delivery ordered.
        drain_made_progress = false;
        let terminal_ids: Vec<u32> = pending.keys().copied().collect();
        for tid in terminal_ids {
            let queue = match pending.get_mut(&tid) {
                Some(q) if !q.is_empty() => q,
                _ => continue,
            };

            drain_made_progress |= drain_terminal_turn(queue, tid, os_input.as_ref());
        }
        pending.retain(|_, q| !q.is_empty());
    }
}

/// Give one terminal its bounded turn. Returns whether the turn made
/// progress (wrote bytes or retired queued writes). Every queue step, empty
/// writes included, spends one unit of the call budget, so neither a stream
/// of empty writes nor a slow consumer can extend a turn.
///
/// The syscall cost of a turn is bounded as well: at most
/// `MAX_DRAIN_CALLS_PER_TURN` calls, each issuing at most
/// `MAX_WRITE_SYSCALLS_PER_CALL` write(2) calls in the Unix backend.
fn drain_terminal_turn(queue: &mut PendingQueue, tid: u32, os_input: &dyn ServerOsApi) -> bool {
    let err_context = || "failed to write to pty".to_string();
    let mut progress = false;
    let mut turn_bytes = 0usize;
    let mut turn_calls = 0usize;
    while turn_calls < MAX_DRAIN_CALLS_PER_TURN && turn_bytes < MAX_DRAIN_BYTES_PER_TURN {
        let Some(remaining) = queue.front_remaining() else {
            break;
        };
        turn_calls += 1;
        if remaining.is_empty() {
            queue.advance(0);
            progress = true;
            continue;
        }
        let offer = remaining.len().min(MAX_DRAIN_BYTES_PER_TURN - turn_bytes);
        match os_input.write_to_tty_stdin(tid, &remaining[..offer]) {
            Ok(0) => break, // EAGAIN — move to next terminal
            Ok(n) => {
                queue.advance(n);
                turn_bytes += n;
                progress = true;
            },
            Err(e) => {
                // Terminal error (EBADF, EIO, etc) — report every
                // discarded write as failed rather than completed.
                let reason = format!(
                    "pty write failed ({e}); the write was discarded before \
                     reaching the terminal"
                );
                Err::<(), _>(e).with_context(err_context).non_fatal();
                queue.fail_all(&reason);
                break;
            },
        }
    }
    progress
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::oneshot;

    #[test]
    fn discarded_writes_report_failure_and_drained_writes_report_success() {
        // Three queued writes: one drains fully, two are discarded by a
        // failure. Each token must tell the truth about its outcome.
        let mut queue = PendingQueue::default();
        let (ok_tx, ok_rx) = oneshot::channel();
        let (fail_tx1, fail_rx1) = oneshot::channel();
        let (fail_tx2, fail_rx2) = oneshot::channel();
        queue.push_back(PendingWrite {
            bytes: b"done".to_vec(),
            offset: 0,
            completion: Some(NotificationEnd::new(ok_tx)),
        });
        queue.push_back(PendingWrite {
            bytes: b"lost1".to_vec(),
            offset: 0,
            completion: Some(NotificationEnd::new(fail_tx1)),
        });
        queue.push_back(PendingWrite {
            bytes: b"lost2".to_vec(),
            offset: 0,
            completion: Some(NotificationEnd::new(fail_tx2)),
        });

        queue.advance(4);
        let ok_result = ok_rx.blocking_recv().expect("completed write resolves");
        assert_eq!(ok_result.error_message, None);

        queue.fail_all("terminal went away");
        let fail_result1 = fail_rx1.blocking_recv().expect("discarded write resolves");
        let fail_result2 = fail_rx2.blocking_recv().expect("discarded write resolves");
        assert_eq!(
            fail_result1.error_message.as_deref(),
            Some("terminal went away")
        );
        assert_eq!(
            fail_result2.error_message.as_deref(),
            Some("terminal went away")
        );
        assert_eq!(queue.remaining_bytes, 0);
    }

    #[test]
    fn pending_byte_accounting_tracks_partial_drains() {
        let mut queue = PendingQueue::default();
        queue.push_back(PendingWrite {
            bytes: vec![0u8; 10],
            offset: 0,
            completion: None,
        });
        queue.push_back(PendingWrite {
            bytes: vec![0u8; 5],
            offset: 0,
            completion: None,
        });
        assert_eq!(queue.remaining_bytes, 15);
        queue.advance(4);
        assert_eq!(queue.remaining_bytes, 11);
        queue.advance(6);
        assert_eq!(queue.remaining_bytes, 5);
        queue.advance(5);
        assert_eq!(queue.remaining_bytes, 0);
        assert!(queue.is_empty());
    }
}

#[cfg(test)]
#[path = "./unit/pty_writer_tests.rs"]
mod pty_writer_tests;
