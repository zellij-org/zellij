use zellij_utils::errors::{prelude::*, ContextType, PtyWriteContext};

use crate::thread_bus::Bus;

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) enum PtyWriteInstruction {
    Write(Vec<u8>, u32),
    Exit,
}

impl From<&PtyWriteInstruction> for PtyWriteContext {
    fn from(tty_write_instruction: &PtyWriteInstruction) -> Self {
        match *tty_write_instruction {
            PtyWriteInstruction::Write(..) => PtyWriteContext::Write,
            PtyWriteInstruction::Exit => PtyWriteContext::Exit,
        }
    }
}

pub(crate) fn pty_writer_main(bus: Bus<PtyWriteInstruction>) -> Result<()> {
    let err_context = || "failed to write to pty".to_string();

    loop {
        let (event, mut err_ctx) = bus.recv().with_context(err_context)?;
        err_ctx.add_call(ContextType::PtyWrite((&event).into()));
        let os_input = bus
            .os_input
            .clone()
            .context("no OS input API found")
            .with_context(err_context)?;
        match event {
            PtyWriteInstruction::Write(bytes, terminal_id) => {
                os_input
                    .write_to_tty_stdin(terminal_id, &bytes)
                    .with_context(err_context)
                    .non_fatal();
                os_input
                    .tcdrain(terminal_id)
                    .with_context(err_context)
                    .non_fatal();
            },
            PtyWriteInstruction::Exit => {
                return Ok(());
            },
        }
    }
}
