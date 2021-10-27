use crate::os_input_output::ClientOsApi;
use crate::InputInstruction;
use termion::input::TermReadEventsAndRaw;
use zellij_utils::channels::SenderWithContext;
use zellij_utils::input::mouse::MouseEvent;
use zellij_utils::termion;

fn bracketed_paste_end_position(stdin_buffer: &[u8]) -> Option<usize> {
    let bracketed_paste_end = vec![27, 91, 50, 48, 49, 126]; // \u{1b}[201
    let mut bp_position = 0;
    let mut position = None;
    for (i, byte) in stdin_buffer.iter().enumerate() {
        if Some(byte) == bracketed_paste_end.get(bp_position) {
            position = Some(i);
            bp_position += 1;
            if bp_position == bracketed_paste_end.len() - 1 {
                break;
            }
        } else {
            bp_position = 0;
            position = None;
        }
    }
    if bp_position == bracketed_paste_end.len() - 1 {
        position
    } else {
        None
    }
}

pub(crate) fn stdin_loop(
    os_input: Box<dyn ClientOsApi>,
    send_input_instructions: SenderWithContext<InputInstruction>,
) {
    let mut pasting = false;
    let bracketed_paste_start = vec![27, 91, 50, 48, 48, 126]; // \u{1b}[200~
    loop {
        let mut stdin_buffer = os_input.read_from_stdin();
        if pasting
            || (stdin_buffer.len() > bracketed_paste_start.len()
                && stdin_buffer
                    .iter()
                    .take(bracketed_paste_start.len())
                    .eq(bracketed_paste_start.iter()))
        {
            match bracketed_paste_end_position(&stdin_buffer) {
                Some(paste_end_position) => {
                    let pasted_input = stdin_buffer.drain(..=paste_end_position).collect();
                    send_input_instructions
                        .send(InputInstruction::PastedText(pasted_input))
                        .unwrap();
                    pasting = false;
                }
                None => {
                    send_input_instructions
                        .send(InputInstruction::PastedText(stdin_buffer))
                        .unwrap();
                    pasting = true;
                    continue;
                }
            }
        }
        if stdin_buffer.is_empty() {
            continue;
        }
        for key_result in stdin_buffer.events_and_raw() {
            let (key_event, raw_bytes) = key_result.unwrap();
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
            send_input_instructions
                .send(InputInstruction::KeyEvent(key_event, raw_bytes))
                .unwrap();
        }
    }
}
