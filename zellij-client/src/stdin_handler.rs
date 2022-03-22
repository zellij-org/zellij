use crate::os_input_output::ClientOsApi;
use crate::InputInstruction;
use zellij_utils::channels::SenderWithContext;
use zellij_utils::input::mouse::{MouseButton, MouseEvent};

use zellij_utils::termwiz::input::{InputEvent, InputParser};

pub(crate) fn stdin_loop(
    os_input: Box<dyn ClientOsApi>,
    send_input_instructions: SenderWithContext<InputInstruction>,
) {
    let mut holding_mouse = false;
    let mut input_parser = InputParser::new();
    let mut current_buffer = vec![];
    loop {
        let buf = os_input.read_from_stdin();
        current_buffer.append(&mut buf.to_vec());
        let maybe_more = false; // read_from_stdin should (hopefully) always empty the STDIN buffer completely
        let parse_input_event = |input_event: InputEvent| {
            holding_mouse = should_hold_mouse(input_event.clone());
            if holding_mouse {
                 let mut poller = os_input.stdin_poller();
                 loop {
                    let ready = poller.ready();
                    if ready {
                        break;
                    }
                    send_input_instructions
                        .send(InputInstruction::KeyEvent(
                            input_event.clone(),
                            current_buffer.clone(),
                        ))
                        .unwrap();
                 }
            } else {
                send_input_instructions
                    .send(InputInstruction::KeyEvent(
                        input_event,
                        current_buffer.drain(..).collect(),
                    ))
                    .unwrap();
            }
        };
        input_parser.parse(&buf, parse_input_event, maybe_more);
    }
}

fn should_hold_mouse(input_event: InputEvent) -> bool {
    if let InputEvent::Mouse(mouse_event) = input_event {
        let mouse_event = zellij_utils::input::mouse::MouseEvent::from(mouse_event);
        if let MouseEvent::Press(button, _point) = mouse_event {
            match button {
                MouseButton::Left | MouseButton::Right => {
                    return true;
                },
                _ => {}
            }
        }
    }
    false
}
