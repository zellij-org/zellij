use crate::os_input_output::ClientOsApi;
use crate::InputInstruction;
use zellij_utils::channels::SenderWithContext;

use zellij_utils::termwiz::input::{InputEvent, InputParser};

pub(crate) fn stdin_loop(
    os_input: Box<dyn ClientOsApi>,
    send_input_instructions: SenderWithContext<InputInstruction>,
) {
    let mut input_parser = InputParser::new();
    let mut current_buffer = vec![];
    loop {
        let buf = os_input.read_from_stdin();
        current_buffer.append(&mut buf.to_vec());
        let maybe_more = false; // read_from_stdin should (hopefully) always empty the STDIN buffer completely
        let parse_input_event = |input_event: InputEvent| {
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
