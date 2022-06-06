use crate::os_input_output::ClientOsApi;
use crate::InputInstruction;
use zellij_utils::channels::SenderWithContext;
use zellij_utils::termwiz::input::{InputEvent, InputParser, MouseButtons};

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
        let mut events = vec![];
        input_parser.parse(
            &buf,
            |input_event: InputEvent| {
                events.push(input_event);
            },
            maybe_more,
        );

        let event_count = events.len();
        for (i, input_event) in events.into_iter().enumerate() {
            if holding_mouse && is_mouse_press_or_hold(&input_event) && i == event_count - 1 {
                let mut poller = os_input.stdin_poller();
                loop {
                    if poller.ready() {
                        break;
                    }
                    send_input_instructions
                        .send(InputInstruction::KeyEvent(
                            input_event.clone(),
                            current_buffer.clone(),
                        ))
                        .unwrap();
                }
            }

            holding_mouse = is_mouse_press_or_hold(&input_event);

            send_input_instructions
                .send(InputInstruction::KeyEvent(
                    input_event,
                    current_buffer.drain(..).collect(),
                ))
                .unwrap();
        }
    }
}

fn is_mouse_press_or_hold(input_event: &InputEvent) -> bool {
    if let InputEvent::Mouse(mouse_event) = input_event {
        if mouse_event.mouse_buttons.contains(MouseButtons::LEFT)
            || mouse_event.mouse_buttons.contains(MouseButtons::RIGHT)
        {
            return true;
        }
    }
    false
}
