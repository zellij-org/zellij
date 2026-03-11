use std::collections::{HashMap, VecDeque};
use std::time::Duration;

use zellij_utils::channels;
use zellij_utils::errors::{prelude::*, ContextType, PtyWriteContext};

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

/// Maximum pending bytes per terminal before dropping the buffer (10 MB).
const MAX_PENDING_BYTES: usize = 10 * 1024 * 1024;

/// When pending writes exist, poll for new instructions with this timeout
/// before retrying the drain. Keeps the thread responsive to new instructions
/// while still retrying stuck terminals.
const PENDING_DRAIN_TIMEOUT: Duration = Duration::from_millis(10);

/// A chunk of bytes waiting to be written to a terminal's stdin.
struct PendingWrite {
    bytes: Vec<u8>,
    offset: usize,
    _completion: Option<NotificationEnd>,
}

pub(crate) fn pty_writer_main(bus: Bus<PtyWriteInstruction>) -> Result<()> {
    let err_context = || "failed to write to pty".to_string();
    let mut pending: HashMap<u32, VecDeque<PendingWrite>> = HashMap::new();

    loop {
        // If we have pending writes, use a short timeout so we can keep
        // draining. Otherwise, block until a new instruction arrives.
        let has_pending = pending.values().any(|q| !q.is_empty());
        let event = if has_pending {
            match bus.recv_timeout(PENDING_DRAIN_TIMEOUT) {
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
                    let queued: usize =
                        queue.iter().map(|w| w.bytes.len() - w.offset).sum();
                    if queued + bytes.len() > MAX_PENDING_BYTES {
                        log::error!(
                            "dropping write buffer for terminal {} \
                             ({} + {} bytes exceeds {} limit)",
                            terminal_id,
                            queued,
                            bytes.len(),
                            MAX_PENDING_BYTES,
                        );
                        queue.clear();
                    } else {
                        queue.push_back(PendingWrite {
                            bytes,
                            offset: 0,
                            _completion: completion,
                        });
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

        // Drain pending writes — one pass across all terminals.
        // For each terminal, write as much as the kernel accepts without
        // blocking, then move on to the next terminal.
        let terminal_ids: Vec<u32> = pending.keys().copied().collect();
        for tid in terminal_ids {
            let queue = match pending.get_mut(&tid) {
                Some(q) if !q.is_empty() => q,
                _ => continue,
            };

            while let Some(front) = queue.front_mut() {
                let remaining = &front.bytes[front.offset..];
                if remaining.is_empty() {
                    queue.pop_front();
                    continue;
                }
                match os_input.write_to_tty_stdin(tid, remaining) {
                    Ok(0) => break, // EAGAIN — move to next terminal
                    Ok(n) => {
                        front.offset += n;
                        if front.offset >= front.bytes.len() {
                            queue.pop_front();
                        }
                    },
                    Err(e) => {
                        // Terminal error (EBADF, EIO, etc) — clear its queue
                        Err::<(), _>(e).with_context(err_context).non_fatal();
                        queue.clear();
                        break;
                    },
                }
            }
        }
        pending.retain(|_, q| !q.is_empty());
    }
}
