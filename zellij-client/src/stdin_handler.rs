use crate::os_input_output::ClientOsApi;
use crate::InputInstruction;
use termion::input::TermReadEventsAndRaw;
use zellij_utils::channels::SenderWithContext;
use zellij_utils::input::mouse::MouseEvent;
use zellij_utils::termion;

pub(crate) fn stdin_loop(
    os_input: Box<dyn ClientOsApi>,
    send_input_instructions: SenderWithContext<InputInstruction>,
) {
    let mut pasting = false;
    let mut pasted_text = vec![];
    let bracketed_paste_start = termion::event::Event::Unsupported(vec![27, 91, 50, 48, 48, 126]); // \u{1b}[200~
    let bracketed_paste_end = termion::event::Event::Unsupported(vec![27, 91, 50, 48, 49, 126]); // \u{1b}[201~
    let csi_mouse_sgr_start = vec![27, 91, 60];
    for key_result in os_input.get_stdin_reader().events_and_raw() {
        let (key_event, mut raw_bytes) = key_result.unwrap();

        if key_event == bracketed_paste_start {
            pasting = true;
            continue;
        } else if pasting && key_event == bracketed_paste_end {
            pasting = false;
            let pasted_text: Vec<u8> = pasted_text.drain(..).collect();
            send_input_instructions
                .send(InputInstruction::PastedText(pasted_text))
                .unwrap();
            continue;
        } else if pasting {
            pasted_text.append(&mut raw_bytes);
            continue;
        }

        if let termion::event::Event::Mouse(me) = key_event {
            let mouse_event = zellij_utils::input::mouse::MouseEvent::from(me);
            if let MouseEvent::Hold(_) = mouse_event {
                // as long as the user is holding the mouse down (no other stdin, eg.
                // MouseRelease) we need to keep sending this instruction to the app,
                // because the app itself doesn't have an event loop in the proper
                // place
                let mut poller = os_input.stdin_poller();
                send_input_instructions
                    .send(InputInstruction::KeyEvent(
                        key_event.clone(),
                        raw_bytes.clone(),
                    ))
                    .unwrap();
                loop {
                    let ready = poller.ready();
                    if ready {
                        break;
                    }
                    send_input_instructions
                        .send(InputInstruction::KeyEvent(
                            key_event.clone(),
                            raw_bytes.clone(),
                        ))
                        .unwrap();
                }
                continue;
            }
        }

        // FIXME: termion does not properly parse some csi sgr mouse sequences
        // like ctrl + click.
        // As a workaround, to avoid writing these sequences to tty stdin,
        // we discard them.
        if let termion::event::Event::Unsupported(_) = key_event {
            if raw_bytes.len() > csi_mouse_sgr_start.len()
                && raw_bytes[0..csi_mouse_sgr_start.len()] == csi_mouse_sgr_start
            {
                continue;
            }
        }

        send_input_instructions
            .send(InputInstruction::KeyEvent(key_event, raw_bytes))
            .unwrap();
    }
}
