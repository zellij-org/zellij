use zellij_utils::errors::{ContextType, PtyWriteContext};

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

pub(crate) fn pty_writer_main(bus: Bus<PtyWriteInstruction>) {
    loop {
        let (event, mut err_ctx) = bus.recv().expect("failed to receive event on channel");
        err_ctx.add_call(ContextType::PtyWrite((&event).into()));
        let os_input = bus.os_input.clone().unwrap();
        match event {
            PtyWriteInstruction::Write(bytes, terminal_id) => {
                if let Err(e) = os_input.write_to_tty_stdin(terminal_id, &bytes) {
                    log::error!("failed to write to terminal: {}", e);
                }
                if let Err(e) = os_input.tcdrain(terminal_id) {
                    log::error!("failed to drain terminal: {}", e);
                };
            },
            PtyWriteInstruction::Exit => {
                break;
            },
        }
    }
}
