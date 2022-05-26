use crate::os_input_output::ClientOsApi;
use crate::InputInstruction;
use crate::stdin_ansi_parser::StdinAnsiParser;
use zellij_utils::channels::SenderWithContext;
use zellij_utils::termwiz::input::{InputEvent, InputParser, MouseButtons};
use std::sync::{Arc, Mutex};

pub(crate) fn stdin_loop(
    os_input: Box<dyn ClientOsApi>,
    send_input_instructions: SenderWithContext<InputInstruction>,
    stdin_ansi_parser: Arc<Mutex<StdinAnsiParser>>,
) {
    let mut holding_mouse = false;
    let mut input_parser = InputParser::new();
    let mut current_buffer = vec![];
    let terminal_emulator_query_string = stdin_ansi_parser.lock().unwrap().terminal_emulator_query_string();
    let _ = os_input
        .get_stdout_writer()
        .write(terminal_emulator_query_string.as_bytes())
        .unwrap();
    loop {
        let buf = os_input.read_from_stdin();
        {
            // here we check if we need to parse specialized ANSI instructions sent over STDIN
            // this happens either on startup when we query the terminal emulator for various
            // information (eg. pixel cell size and color registers)
            // as well as on SIGWINCH
            //
            // if we need to parse them, we do so with an internal timeout - anything else we
            // receive on STDIN during that timeout is unceremoniously dropped
            let mut stdin_ansi_parser = stdin_ansi_parser.lock().unwrap();
            if stdin_ansi_parser.should_parse() {
                let events = stdin_ansi_parser.parse(buf);
                if !events.is_empty() {
                    let _ = send_input_instructions.send(InputInstruction::AnsiStdinInstructions(events));
                }
                continue;
            }
        }
        current_buffer.append(&mut buf.to_vec());
        let maybe_more = false; // read_from_stdin should (hopefully) always empty the STDIN buffer completely
        let parse_input_event = |input_event: InputEvent| {
            if holding_mouse && is_mouse_press_or_hold(&input_event) {
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
            }

            holding_mouse = is_mouse_press_or_hold(&input_event);

            send_input_instructions
                .send(InputInstruction::KeyEvent(
                    input_event,
                    current_buffer.drain(..).collect(),
                ))
                .unwrap();
        };
        input_parser.parse(&buf, parse_input_event, maybe_more);
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
