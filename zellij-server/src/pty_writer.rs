use zellij_utils::errors::{prelude::*, ContextType, PtyWriteContext};

use crate::route::NotificationEnd;
use crate::thread_bus::Bus;

// we separate these instruction to a different thread because some programs get deadlocked if
// you write into their STDIN while reading from their STDOUT (I'm looking at you, vim)
// while the same has not been observed to happen with resizes, it could conceivably happen and we have this
// here anyway, so
#[derive(Debug, Clone)]
pub enum PtyWriteInstruction {
    /// Write bytes to a terminal's stdin.
    /// Fields: bytes, terminal_id, completion notification, retry count.
    /// The retry count tracks consecutive zero-progress attempts (EAGAIN with
    /// no bytes written). It is reset whenever forward progress is made.
    Write(Vec<u8>, u32, Option<NotificationEnd>, usize),
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

/// Maximum number of consecutive zero-progress retries before dropping bytes.
const MAX_WRITE_RETRIES: usize = 50;

pub(crate) fn pty_writer_main(bus: Bus<PtyWriteInstruction>) -> Result<()> {
    let err_context = || "failed to write to pty".to_string();

    loop {
        let (event, mut err_ctx) = bus.recv().with_context(err_context)?;
        err_ctx.add_call(ContextType::PtyWrite((&event).into()));
        let mut os_input = bus
            .os_input
            .clone()
            .context("no OS input API found")
            .with_context(err_context)?;
        match event {
            PtyWriteInstruction::Write(bytes, terminal_id, completion, retries) => {
                match os_input.write_to_tty_stdin(terminal_id, &bytes) {
                    Ok(written) if written >= bytes.len() => {
                        // Full write succeeded — drain the fd
                        os_input
                            .tcdrain(terminal_id)
                            .with_context(err_context)
                            .non_fatal();
                    },
                    Ok(0) => {
                        // No progress (EAGAIN) — yield so other terminals' writes
                        // (and their readers) get a chance to run before we retry.
                        std::thread::yield_now();
                        // Re-queue with incremented retry count
                        if retries >= MAX_WRITE_RETRIES {
                            log::error!(
                                "dropping {} bytes for terminal {}: \
                                 no write progress after {} retries",
                                bytes.len(),
                                terminal_id,
                                retries,
                            );
                        } else {
                            bus.senders
                                .send_to_pty_writer(PtyWriteInstruction::Write(
                                    bytes,
                                    terminal_id,
                                    completion,
                                    retries + 1,
                                ))
                                .with_context(err_context)
                                .non_fatal();
                        }
                    },
                    Ok(written) => {
                        // Partial write — re-queue remainder, reset retry count
                        bus.senders
                            .send_to_pty_writer(PtyWriteInstruction::Write(
                                bytes[written..].to_vec(),
                                terminal_id,
                                completion,
                                0,
                            ))
                            .with_context(err_context)
                            .non_fatal();
                    },
                    Err(e) => {
                        Err::<(), _>(e).with_context(err_context).non_fatal();
                    },
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
}