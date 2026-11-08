use std::collections::{HashMap, VecDeque};
use std::time::Duration;

use zellij_utils::channels;
use zellij_utils::errors::{prelude::*, ContextType, PtyWriteContext};

use crate::diagnostics;
use crate::os_input_output::{ServerOsApi, TerminalRunChangedBeforeWrite};
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
    /// The screen adopted `generation` as this terminal's current process.
    /// The first adoption keeps writes that were queued before any run was
    /// known. A later, different generation drops the queue: those bytes
    /// were for the previous process.
    AdoptRun(u32, u64),
    /// The terminal is gone. Drop its adopted-run entry so the map does not
    /// grow with every pane ever opened in the session.
    ForgetRun(u32),
    Exit,
}

impl From<&PtyWriteInstruction> for PtyWriteContext {
    fn from(tty_write_instruction: &PtyWriteInstruction) -> Self {
        match *tty_write_instruction {
            PtyWriteInstruction::Write(..) => PtyWriteContext::Write,
            PtyWriteInstruction::ResizePty(..) => PtyWriteContext::ResizePty,
            PtyWriteInstruction::ApplyCachedResizes => PtyWriteContext::ApplyCachedResizes,
            PtyWriteInstruction::StartCachingResizes => PtyWriteContext::StartCachingResizes,
            PtyWriteInstruction::AdoptRun(..) => PtyWriteContext::Write,
            PtyWriteInstruction::ForgetRun(..) => PtyWriteContext::Write,
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
    /// `None` until a run has been adopted. Those writes are keystrokes from
    /// before the screen processed `TerminalRunStarted` and are kept across
    /// the first adoption.
    run_generation: Option<u64>,
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

    /// Drop the front write when it was stamped for a different run than the
    /// one this terminal has adopted. Unstamped writes are keystrokes from
    /// before the first adoption and stay in the queue.
    fn reject_other_run(&mut self, adopted: Option<u64>) -> bool {
        let stamp = match self.writes.front() {
            Some(write) => write.run_generation,
            None => return false,
        };
        let Some(stamp) = stamp else {
            return false;
        };
        if adopted == Some(stamp) {
            return false;
        }
        let mut write = match self.writes.pop_front() {
            Some(write) => write,
            None => return false,
        };
        let leftover = write.bytes.len().saturating_sub(write.offset);
        self.remaining_bytes = self.remaining_bytes.saturating_sub(leftover);
        if let Some(mut completion) = write.completion.take() {
            completion.set_error_message(Self::failure_message(
                "the pane started another command before this input was written",
                write.offset,
            ));
        }
        true
    }

    /// Discard every queued write, reporting the discard as a truthful
    /// failure on each write's completion token instead of the silent
    /// success an unmarked drop would produce.
    fn fail_all(&mut self, reason: &str) {
        for mut write in self.writes.drain(..) {
            if let Some(mut completion) = write.completion.take() {
                completion.set_error_message(Self::failure_message(reason, write.offset));
            }
        }
        self.remaining_bytes = 0;
    }

    /// A prefix that already reached the terminal is not "discarded before
    /// reaching" it. Offset zero keeps `reason` unchanged.
    fn failure_message(reason: &str, offset: usize) -> String {
        if offset == 0 {
            return reason.to_string();
        }
        let reason = reason
            .replace(
                "the write was discarded before reaching the terminal",
                "the rest of the write was not written",
            )
            .replace(
                "before this input was written",
                "before the rest of this input was written",
            );
        format!("{reason} ({offset} bytes of this write were already written)")
    }

    fn fail_after_write_error(&mut self, error: &str) {
        for mut write in self.writes.drain(..) {
            if let Some(mut completion) = write.completion.take() {
                let message = if write.offset > 0 {
                    format!(
                        "pty write failed ({error}); {} bytes of this write were already written",
                        write.offset
                    )
                } else {
                    format!(
                        "pty write failed ({error}); the write was discarded before reaching the terminal"
                    )
                };
                completion.set_error_message(message);
            }
        }
        self.remaining_bytes = 0;
    }
}

/// Adopt `generation` for `terminal_id`. The first adoption keeps the queue.
/// A different generation fails every queued write, including ones that were
/// not stamped with a generation yet.
fn adopt_run(
    adopted_runs: &mut HashMap<u32, u64>,
    pending: &mut HashMap<u32, PendingQueue>,
    terminal_id: u32,
    generation: u64,
) {
    let previous = adopted_runs.insert(terminal_id, generation);
    if matches!(previous, Some(current) if current != generation) {
        if let Some(queue) = pending.get_mut(&terminal_id) {
            queue.fail_all("the pane started another command before this input was written");
        }
    } else if previous.is_none() {
        // Keystrokes queued before any run was known belong to this first
        // run. Stamping them means a later endpoint replacement rejects
        // them instead of delivering them to the next process.
        if let Some(queue) = pending.get_mut(&terminal_id) {
            for write in queue.writes.iter_mut() {
                if write.run_generation.is_none() {
                    write.run_generation = Some(generation);
                }
            }
        }
    }
}

fn fail_pending_writes(pending: &mut HashMap<u32, PendingQueue>, reason: &str) {
    for queue in pending.values_mut() {
        queue.fail_all(reason);
    }
}

pub(crate) fn pty_writer_main(bus: Bus<PtyWriteInstruction>) -> Result<()> {
    let err_context = || "failed to write to pty".to_string();
    let mut pending: HashMap<u32, PendingQueue> = HashMap::new();
    let mut adopted_runs: HashMap<u32, u64> = HashMap::new();
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
                Err(channels::RecvTimeoutError::Disconnected) => {
                    fail_pending_writes(
                        &mut pending,
                        "the pty writer disconnected before this input was written",
                    );
                    return Ok(());
                },
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
                    let run_generation = adopted_runs.get(&terminal_id).copied();
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
                            run_generation,
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
                PtyWriteInstruction::AdoptRun(terminal_id, generation) => {
                    adopt_run(&mut adopted_runs, &mut pending, terminal_id, generation);
                },
                PtyWriteInstruction::ForgetRun(terminal_id) => {
                    adopted_runs.remove(&terminal_id);
                },
                PtyWriteInstruction::Exit => {
                    fail_pending_writes(
                        &mut pending,
                        "the pty writer shut down before this input was written",
                    );
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

            let adopted = adopted_runs.get(&tid).copied();
            drain_made_progress |= drain_terminal_turn(queue, tid, adopted, os_input.as_ref());
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
fn drain_terminal_turn(
    queue: &mut PendingQueue,
    tid: u32,
    adopted: Option<u64>,
    os_input: &dyn ServerOsApi,
) -> bool {
    let err_context = || "failed to write to pty".to_string();
    let mut progress = false;
    let mut turn_bytes = 0usize;
    let mut turn_calls = 0usize;
    while turn_calls < MAX_DRAIN_CALLS_PER_TURN && turn_bytes < MAX_DRAIN_BYTES_PER_TURN {
        if queue.reject_other_run(adopted) {
            turn_calls += 1;
            progress = true;
            continue;
        }
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
        let stamp = queue.writes.front().and_then(|write| write.run_generation);
        let (written, result) =
            os_input.write_to_tty_stdin_for_run_with_progress(tid, stamp, &remaining[..offer]);
        if written > 0 {
            queue.advance(written);
            turn_bytes += written;
            progress = true;
        }
        match result {
            Ok(()) if written == 0 => break, // EAGAIN — move to next terminal
            Ok(()) => {},
            Err(error)
                if error
                    .downcast_ref::<TerminalRunChangedBeforeWrite>()
                    .is_some() =>
            {
                // The endpoint is being replaced. Keep the queue; the
                // matching AdoptRun (or ForgetRun) settles it.
                break;
            },
            Err(error) => {
                // Terminal error. A prefix already written is reported; the
                // rest is not described as never sent.
                let message = error.to_string();
                Err::<(), _>(error).with_context(err_context).non_fatal();
                queue.fail_after_write_error(&message);
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
            run_generation: None,
        });
        queue.push_back(PendingWrite {
            bytes: b"lost1".to_vec(),
            offset: 0,
            completion: Some(NotificationEnd::new(fail_tx1)),
            run_generation: None,
        });
        queue.push_back(PendingWrite {
            bytes: b"lost2".to_vec(),
            offset: 0,
            completion: Some(NotificationEnd::new(fail_tx2)),
            run_generation: None,
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
            run_generation: None,
        });
        queue.push_back(PendingWrite {
            bytes: vec![0u8; 5],
            offset: 0,
            completion: None,
            run_generation: None,
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

    #[test]
    fn the_first_adoption_keeps_unstamped_writes_and_the_next_run_drops_them() {
        let mut adopted = HashMap::new();
        let mut pending = HashMap::new();
        let mut queue = PendingQueue::default();
        let (keep_tx, mut keep_rx) = oneshot::channel();
        queue.push_back(PendingWrite {
            bytes: b"typed-early".to_vec(),
            offset: 0,
            completion: Some(NotificationEnd::new(keep_tx)),
            run_generation: None,
        });
        pending.insert(1, queue);

        adopt_run(&mut adopted, &mut pending, 1, 7);
        assert_eq!(pending.get(&1).map(|queue| queue.writes.len()), Some(1));
        assert_eq!(
            pending.get(&1).unwrap().writes[0].run_generation,
            Some(7),
            "the first adoption stamps writes that had no run yet"
        );
        assert!(keep_rx.try_recv().is_err());

        adopt_run(&mut adopted, &mut pending, 1, 7);
        assert_eq!(pending.get(&1).map(|queue| queue.writes.len()), Some(1));

        adopt_run(&mut adopted, &mut pending, 1, 8);
        let discarded = keep_rx.blocking_recv().expect("discarded write resolves");
        assert!(discarded.error_message.is_some());
        assert!(pending.get(&1).unwrap().is_empty());
    }

    #[test]
    fn a_stamped_write_is_not_delivered_to_a_different_run() {
        let mut queue = PendingQueue::default();
        let (stale_tx, stale_rx) = oneshot::channel();
        let (live_tx, mut live_rx) = oneshot::channel();
        queue.push_back(PendingWrite {
            bytes: b"stale".to_vec(),
            offset: 0,
            completion: Some(NotificationEnd::new(stale_tx)),
            run_generation: Some(1),
        });
        queue.push_back(PendingWrite {
            bytes: b"live".to_vec(),
            offset: 0,
            completion: Some(NotificationEnd::new(live_tx)),
            run_generation: Some(2),
        });
        assert!(queue.reject_other_run(Some(2)));
        assert_eq!(queue.front_remaining(), Some(&b"live"[..]));
        let discarded = stale_rx.blocking_recv().expect("stale write resolves");
        assert!(discarded.error_message.is_some());
        assert!(!queue.reject_other_run(Some(2)));
        assert!(live_rx.try_recv().is_err());
    }

    #[test]
    fn a_rejected_partial_write_reports_the_bytes_already_written() {
        let mut queue = PendingQueue::default();
        let (partial_tx, partial_rx) = oneshot::channel();
        queue.push_back(PendingWrite {
            bytes: b"abcdef".to_vec(),
            offset: 2,
            completion: Some(NotificationEnd::new(partial_tx)),
            run_generation: Some(1),
        });
        assert!(queue.reject_other_run(Some(2)));
        let message = partial_rx
            .blocking_recv()
            .expect("rejected write resolves")
            .error_message
            .unwrap();
        assert!(
            message.contains("2 bytes of this write were already written"),
            "{message}"
        );
        assert!(!message.contains("discarded before reaching"), "{message}");
    }

    #[test]
    fn a_partial_overflow_does_not_claim_the_prefix_was_never_sent() {
        let mut queue = PendingQueue::default();
        let (partial_tx, partial_rx) = oneshot::channel();
        queue.push_back(PendingWrite {
            bytes: b"abcdef".to_vec(),
            offset: 2,
            completion: Some(NotificationEnd::new(partial_tx)),
            run_generation: None,
        });
        queue.fail_all(
            "pty write queue overflowed the pending byte limit; \
             the write was discarded before reaching the terminal",
        );
        let message = partial_rx
            .blocking_recv()
            .expect("overflowed write resolves")
            .error_message
            .unwrap();
        assert!(
            message.contains("2 bytes of this write were already written"),
            "{message}"
        );
        assert!(!message.contains("discarded before reaching"), "{message}");
        assert!(
            message.contains("the rest of the write was not written"),
            "{message}"
        );
    }

    #[test]
    fn a_partial_write_failure_reports_the_bytes_already_written() {
        let mut queue = PendingQueue::default();
        let (partial_tx, partial_rx) = oneshot::channel();
        let (rest_tx, rest_rx) = oneshot::channel();
        queue.push_back(PendingWrite {
            bytes: b"abcdefgh".to_vec(),
            offset: 3,
            completion: Some(NotificationEnd::new(partial_tx)),
            run_generation: Some(1),
        });
        queue.push_back(PendingWrite {
            bytes: b"later".to_vec(),
            offset: 0,
            completion: Some(NotificationEnd::new(rest_tx)),
            run_generation: Some(1),
        });
        queue.fail_after_write_error("broken pipe");
        let partial = partial_rx.blocking_recv().expect("partial write resolves");
        let rest = rest_rx.blocking_recv().expect("later write resolves");
        let partial_message = partial.error_message.unwrap();
        assert!(
            partial_message.contains("3 bytes of this write were already written"),
            "{partial_message}"
        );
        assert!(
            !partial_message.contains("discarded before reaching"),
            "{partial_message}"
        );
        let rest_message = rest.error_message.unwrap();
        assert!(rest_message.contains("discarded before reaching the terminal"));
        assert!(!rest_message.contains("already written"));
    }
}

#[cfg(test)]
#[path = "./unit/pty_writer_tests.rs"]
mod pty_writer_tests;
