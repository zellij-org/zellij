use crate::keyboard_parser::KittyKeyboardParser;
use crate::os_input_output::ClientOsApi;
use crate::stdin_ansi_parser::StdinAnsiParser;
use crate::InputInstruction;
use std::sync::{Arc, Mutex};
use termwiz::input::{InputEvent, InputParser};
use zellij_utils::channels::SenderWithContext;

pub(crate) fn stdin_loop(
    mut os_input: Box<dyn ClientOsApi>,
    send_input_instructions: SenderWithContext<InputInstruction>,
    stdin_ansi_parser: Arc<Mutex<StdinAnsiParser>>,
    explicitly_disable_kitty_keyboard_protocol: bool,
) {
    let mut input_parser = InputParser::new();
    let mut current_buffer = vec![];
    let mut pending_buffer = vec![]; // Buffer for incomplete sequences
    let mut pending_since = None::<std::time::Instant>; // Track when we started buffering

    {
        // on startup we send a query to the terminal emulator for stuff like the pixel size and colors
        // we get a response through STDIN, so it makes sense to do this here
        let mut stdin_ansi_parser = stdin_ansi_parser.lock().unwrap();
        match stdin_ansi_parser.read_cache() {
            Some(events) => {
                let _ =
                    send_input_instructions.send(InputInstruction::AnsiStdinInstructions(events));
                let _ = send_input_instructions
                    .send(InputInstruction::DoneParsing)
                    .unwrap();
            },
            None => {
                send_input_instructions
                    .send(InputInstruction::StartedParsing)
                    .unwrap();
                let terminal_emulator_query_string =
                    stdin_ansi_parser.terminal_emulator_query_string();
                let _ = os_input
                    .get_stdout_writer()
                    .write(terminal_emulator_query_string.as_bytes())
                    .unwrap();
                let query_duration = stdin_ansi_parser.startup_query_duration();
                send_done_parsing_after_query_timeout(
                    send_input_instructions.clone(),
                    query_duration,
                );
            },
        }
    }
    let mut ansi_stdin_events = vec![];
    loop {
        match os_input.read_from_stdin() {
            Ok(buf) => {
                {
                    // here we check if we need to parse specialized ANSI instructions sent over STDIN
                    // this happens either on startup (see above) or on SIGWINCH
                    //
                    // if we need to parse them, we do so with an internal timeout - anything else we
                    // receive on STDIN during that timeout is unceremoniously dropped
                    let mut stdin_ansi_parser = stdin_ansi_parser.lock().unwrap();
                    if stdin_ansi_parser.should_parse() {
                        let events = stdin_ansi_parser.parse(buf);
                        if !events.is_empty() {
                            ansi_stdin_events.append(&mut events.clone());
                            let _ = send_input_instructions
                                .send(InputInstruction::AnsiStdinInstructions(events));
                        }
                        continue;
                    }
                }
                if !ansi_stdin_events.is_empty() {
                    stdin_ansi_parser
                        .lock()
                        .unwrap()
                        .write_cache(ansi_stdin_events.drain(..).collect());
                }
                current_buffer.append(&mut buf.to_vec());

                // Combine with any pending incomplete data
                pending_buffer.extend_from_slice(&buf);

                // Check for timeout on pending data (50ms is reasonable)
                let should_flush_timeout = pending_since
                    .map(|start| start.elapsed() > std::time::Duration::from_millis(50))
                    .unwrap_or(false);

                if should_flush_timeout {
                    // Process whatever we have, even if incomplete

                    match KittyKeyboardParser::new().parse(&pending_buffer) {
                        Some(key_with_modifier) => {
                            send_input_instructions
                                .send(InputInstruction::KeyWithModifierEvent(
                                    key_with_modifier,
                                    current_buffer.drain(..).collect(),
                                ))
                                .unwrap();
                            pending_buffer.clear();
                            pending_since = None;
                            continue;
                        },
                        None => {},
                    }

                    let maybe_more = false;
                    let mut events = vec![];
                    input_parser.parse(
                        &pending_buffer,
                        |input_event: InputEvent| {
                            events.push(input_event);
                        },
                        maybe_more,
                    );

                    for input_event in events.into_iter() {
                        send_input_instructions
                            .send(InputInstruction::KeyEvent(
                                input_event,
                                current_buffer.drain(..).collect(),
                            ))
                            .unwrap();
                    }

                    pending_buffer.clear();
                    pending_since = None;
                    continue;
                }

                if !explicitly_disable_kitty_keyboard_protocol {
                    match KittyKeyboardParser::new().parse(&pending_buffer) {
                        Some(key_with_modifier) => {
                            send_input_instructions
                                .send(InputInstruction::KeyWithModifierEvent(
                                    key_with_modifier,
                                    current_buffer.drain(..).collect(),
                                ))
                                .unwrap();
                            pending_buffer.clear();
                            pending_since = None;
                            continue;
                        },
                        None => {},
                    }
                }

                if might_have_more_data(&pending_buffer) {
                    if pending_since.is_none() {
                        pending_since = Some(std::time::Instant::now());
                    }
                    continue; // Don't parse yet, wait for more data
                }

                let maybe_more = false;
                let mut events = vec![];
                input_parser.parse(
                    &pending_buffer,
                    |input_event: InputEvent| {
                        events.push(input_event);
                    },
                    maybe_more,
                );

                pending_buffer.clear();
                pending_since = None;

                for input_event in events.into_iter() {
                    send_input_instructions
                        .send(InputInstruction::KeyEvent(
                            input_event,
                            current_buffer.drain(..).collect(),
                        ))
                        .unwrap();
                }
            },
            Err(e) => {
                if e == "Session ended" {
                    log::debug!("Switched sessions, signing this thread off...");
                } else {
                    log::error!("Failed to read from STDIN: {}", e);
                }
                let _ = send_input_instructions.send(InputInstruction::Exit);
                break;
            },
        }
    }
}

fn send_done_parsing_after_query_timeout(
    send_input_instructions: SenderWithContext<InputInstruction>,
    query_duration: u64,
) {
    std::thread::spawn({
        move || {
            std::thread::sleep(std::time::Duration::from_millis(query_duration));
            send_input_instructions
                .send(InputInstruction::DoneParsing)
                .unwrap();
        }
    });
}

pub fn might_have_more_data(buf: &[u8]) -> bool {
    if buf.is_empty() {
        return false;
    }

    let len = buf.len();
    // Check if buffer ends with potential incomplete ANSI sequence
    if len >= 1 && buf[len - 1] == 0x1b {
        return true; // Ends with ESC
    }

    // Special case: ESC[ could be Alt+[ (complete) or start of ANSI sequence
    if len == 2 && buf == b"\x1b[" {
        return false; // Treat as complete Alt+[ key sequence
    }

    if len >= 2 && buf[len - 2] == 0x1b && buf[len - 1] == b'[' {
        return true; // Ends with ESC[
    }

    // Look for incomplete CSI sequences at the end
    // Only check the last 20 bytes for performance
    let start_pos = len.saturating_sub(20);
    for i in start_pos..len {
        if buf[i] == 0x1b && i + 1 < len && buf[i + 1] == b'[' {
            let remaining = &buf[i + 2..];
            // 0x40-0x7E -> this range covers all valid CSI final bytes
            if remaining.is_empty() || !remaining.iter().any(|&b| b >= 0x40 && b <= 0x7E) {
                return true;
            }
        }
    }

    false
}

#[cfg(test)]
#[path = "./unit/stdin_handler_tests.rs"]
mod stdin_handler_tests;
