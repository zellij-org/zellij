use crate::InputInstruction;
use crossterm::event::{self, Event, KeyEventKind};
use zellij_utils::channels::SenderWithContext;
use zellij_utils::input::{cast_crossterm_key, from_crossterm_mouse};

/// Windows native console event loop.
///
/// Uses crossterm's `event::read()` which reads INPUT_RECORDs via
/// ReadConsoleInput.  Works in cmd.exe, PowerShell, and Windows Terminal
/// where ALT is reported as a modifier flag.
///
/// Resize events are forwarded to the signal handler thread via `resize_sender`.
pub(crate) fn native_console_stdin_loop(
    send_input_instructions: SenderWithContext<InputInstruction>,
    resize_sender: Option<std::sync::mpsc::Sender<()>>,
) {
    loop {
        match event::read() {
            Ok(Event::Key(key_event)) => {
                if key_event.kind != KeyEventKind::Press {
                    continue;
                }
                if let Some((key, bytes)) = cast_crossterm_key(key_event) {
                    if send_input_instructions
                        .send(InputInstruction::KeyWithModifierEvent(key, bytes, false))
                        .is_err()
                    {
                        break;
                    }
                }
            },
            Ok(Event::Mouse(mouse_event)) => {
                let mouse_event = from_crossterm_mouse(mouse_event);
                if send_input_instructions
                    .send(InputInstruction::MouseEvent(mouse_event))
                    .is_err()
                {
                    break;
                }
            },
            Ok(Event::Paste(text)) => {
                let raw_bytes = text.as_bytes().to_vec();
                let paste_event = termwiz::input::InputEvent::Paste(text);
                if send_input_instructions
                    .send(InputInstruction::KeyEvent(paste_event, raw_bytes))
                    .is_err()
                {
                    break;
                }
            },
            Ok(Event::Resize(..)) => {
                if let Some(ref tx) = resize_sender {
                    let _ = tx.send(());
                }
            },
            Ok(_) => {},
            Err(e) => {
                log::error!("Failed to read crossterm event: {}", e);
                let _ = send_input_instructions.send(InputInstruction::Exit);
                break;
            },
        }
    }
}
