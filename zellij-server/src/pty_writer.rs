use zellij_utils::errors::{prelude::*, ContextType, PtyWriteContext};

use crate::thread_bus::Bus;

// we separate these instruction to a different thread because some programs get deadlocked if
// you write into their STDIN while reading from their STDOUT (I'm looking at you, vim)
// while the same has not been observed to happen with resizes, it could conceivably happen and we have this
// here anyway, so
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum PtyWriteInstruction {
    Write(Vec<u8>, u32),
    ResizePty(u32, u16, u16, Option<u16>, Option<u16>), // terminal_id, columns, rows, pixel width, pixel height
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
            PtyWriteInstruction::Write(bytes, terminal_id) => {
                log::info!("write pty isntruction {:?}", bytes);
                os_input
                    .write_to_tty_stdin(terminal_id, &bytes)
                    .with_context(err_context)
                    .non_fatal();
                os_input
                    .tcdrain(terminal_id)
                    .with_context(err_context)
                    .non_fatal();
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
