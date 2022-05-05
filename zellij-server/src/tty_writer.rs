use zellij_utils::errors::{ContextType, TtyWriteContext};

use crate::thread_bus::Bus;

#[derive(Debug, Clone)]
pub(crate) enum TtyWriteInstruction {
    Write(Vec<u8>, i32),
}

impl From<&TtyWriteInstruction> for TtyWriteContext {
    fn from(tty_write_instruction: &TtyWriteInstruction) -> Self {
        match *tty_write_instruction {
            TtyWriteInstruction::Write(..) => TtyWriteContext::Write,
        }
    }
}

pub(crate) fn tty_writer_main(bus: Bus<TtyWriteInstruction>) {
    loop {
        let (event, mut err_ctx) = bus.recv().expect("failed to receive event on channel");
        err_ctx.add_call(ContextType::TtyWrite((&event).into()));
        let os_input = bus.os_input.clone().unwrap();
        match event {
            TtyWriteInstruction::Write(bytes, terminal_id) => {
                if let Err(e) = os_input.write_to_tty_stdin(terminal_id, &bytes) {
                    log::error!("failed to write to terminal: {}", e);
                }
                if let Err(e) = os_input.tcdrain(terminal_id) {
                    log::error!("failed to drain terminal: {}", e);
                };
            }
        }
    }
}
