use crate::keyboard_parser::{KittyKeyboardParser, KittyParseOutcome};
use crate::os_input_output::ClientOsApi;
use crate::stdin_ansi_parser::{PendingPartial, StdinAnsiParser};
#[cfg(windows)]
use crate::stdin_handler_windows::enable_vt_input;
use crate::InputInstruction;
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

const LONE_ESC_FLUSH_INTERVAL: Duration = Duration::from_millis(50);
const PARTIAL_REPLY_FLUSH_GUARD: Duration = Duration::from_millis(1000);
use zellij_utils::{
    channels::SenderWithContext,
    vendored::termwiz::input::{InputEvent, InputParser},
};

pub(crate) fn stdin_loop(
    mut os_input: Box<dyn ClientOsApi>,
    send_input_instructions: SenderWithContext<InputInstruction>,
    stdin_ansi_parser: Arc<Mutex<StdinAnsiParser>>,
    explicitly_disable_kitty_keyboard_protocol: bool,
    resize_sender: Option<std::sync::mpsc::Sender<()>>,
) {
    // On Windows, choose between two input strategies early — we need this
    // decision before the startup ANSI query below.
    //
    // 1. Native console (no TERM env var): Use crossterm's event::read() which
    //    reads INPUT_RECORDs via ReadConsoleInput. Works in cmd.exe, PowerShell,
    //    and Windows Terminal where ALT is reported as a modifier flag.
    //
    // 2. Terminal emulator (TERM is set, e.g. Alacritty): Enable
    //    ENABLE_VIRTUAL_TERMINAL_INPUT so ReadFile on stdin returns raw VT bytes,
    //    bypassing conpty's lossy VT→INPUT_RECORD translation. Then use the
    //    termwiz byte parser (same as Unix) which understands ESC-prefixed ALT.
    #[cfg(windows)]
    let use_vt_reader = std::env::var("TERM").is_ok() && enable_vt_input();

    // Send the startup host query string so the host terminal replies
    // with its live pixel dimensions, fg/bg, sync-output support, and
    // palette registers. These replies will be classified by the
    // continuous parser as they arrive and routed via `InputInstruction::
    // AnsiStdinInstructions` — no deadline, no cache, no loading gate.
    {
        // On Windows native console, the crossterm event::read() loop
        // reads INPUT_RECORDs via ReadConsoleInput — not raw bytes — so
        // ANSI query responses can never be read on that path.
        #[cfg(windows)]
        let can_query_terminal = use_vt_reader;
        #[cfg(not(windows))]
        let can_query_terminal = true;

        if can_query_terminal {
            let query_string = build_startup_query_string();
            let _ = os_input
                .get_stdout_writer()
                .write(query_string.as_bytes())
                .unwrap();
        }
    }

    #[cfg(windows)]
    if !use_vt_reader {
        crate::stdin_handler_windows::native_console_stdin_loop(
            send_input_instructions,
            resize_sender,
        );
        return;
    }

    // Drop the resize sender so the signal handler thread falls back to
    // polling. Only the Windows native console path (above) keeps it alive;
    // the VT reader path and Unix don't produce crossterm resize events.
    drop(resize_sender);

    // Byte reader + termwiz/kitty parser path.
    // Used on Unix always, and on Windows inside terminal emulators (Alacritty,
    // etc.) with ENABLE_VIRTUAL_TERMINAL_INPUT enabled so stdin delivers raw VT
    // byte sequences.
    let mut input_parser = InputParser::new();
    // Kitty keyboard parser is long-lived so a Kitty CSI sequence split
    // across stdin reads still resolves on a follow-up chunk instead of
    // silently degrading to a legacy CSI form (and losing modifier
    // metadata).
    let mut kitty_parser = KittyKeyboardParser::new();
    let mut current_buffer = vec![];
    let (stdin_tx, stdin_rx) = mpsc::sync_channel(32);
    let _stdin_pump = std::thread::Builder::new()
        .name("stdin_pump".to_string())
        .spawn({
            move || loop {
                match os_input.read_from_stdin() {
                    Ok(buf) => {
                        if stdin_tx.send(Ok(buf)).is_err() {
                            break; // receiver dropped
                        }
                    },
                    Err(e) => {
                        let _ = stdin_tx.send(Err(e));
                        break;
                    },
                }
            }
        });
    let mut needs_finalization = false;
    let mut reply_in_progress_since: Option<Instant> = None;
    loop {
        match if needs_finalization {
            stdin_rx.recv_timeout(LONE_ESC_FLUSH_INTERVAL)
        } else {
            stdin_rx
                .recv()
                .map_err(|_| mpsc::RecvTimeoutError::Disconnected)
        } {
            Ok(result) => {
                match result {
                    Ok(buf) => {
                        // Strip + classify any host-reply sequences
                        // continuously. The residue is the byte stream
                        // the keyboard parser should see.
                        let parse_output = {
                            let mut p = stdin_ansi_parser.lock().unwrap();
                            p.feed(&buf)
                        };
                        if !parse_output.replies.is_empty() {
                            let _ = send_input_instructions.send(
                                InputInstruction::AnsiStdinInstructions(parse_output.replies),
                            );
                        }
                        if let Some((token, reply_bytes)) = parse_output.completed_forward {
                            let _ = send_input_instructions.send(
                                InputInstruction::ForwardedReplyFromHostComplete {
                                    token,
                                    reply_bytes,
                                },
                            );
                        }
                        for payload in parse_output.desktop_notifications {
                            let _ = send_input_instructions
                                .send(InputInstruction::DesktopNotificationResponse(payload));
                        }
                        let residue = parse_output.residue;
                        if residue.is_empty() {
                            schedule_finalization(
                                &stdin_ansi_parser,
                                false,
                                &mut needs_finalization,
                                &mut reply_in_progress_since,
                            );
                            continue;
                        }
                        current_buffer.append(&mut residue.clone());

                        if !explicitly_disable_kitty_keyboard_protocol {
                            // first we try to parse with the KittyKeyboardParser
                            // if we fail, we try to parse normally.
                            // Incomplete and NoMatch both fall through to the
                            // termwiz parser below; on Incomplete the Kitty
                            // parser keeps its state so the next chunk's
                            // continuation completes the sequence.
                            match kitty_parser.feed(&residue) {
                                KittyParseOutcome::Complete(key_with_modifier) => {
                                    send_input_instructions
                                        .send(InputInstruction::KeyWithModifierEvent(
                                            key_with_modifier,
                                            current_buffer.drain(..).collect(),
                                            true,
                                        ))
                                        .unwrap();
                                    schedule_finalization(
                                        &stdin_ansi_parser,
                                        false,
                                        &mut needs_finalization,
                                        &mut reply_in_progress_since,
                                    );
                                    continue;
                                },
                                KittyParseOutcome::Incomplete | KittyParseOutcome::NoMatch => {},
                            }
                        }

                        // Parse with maybe_more = true - complete events sent immediately
                        //
                        // Ambiguous events (if any) will be finalized later only if 50ms
                        // passes with no new input
                        let maybe_more = true;
                        let mut events = vec![];
                        input_parser.parse(
                            &residue,
                            |input_event: InputEvent| {
                                events.push(input_event);
                            },
                            maybe_more,
                        );

                        // Residue contains no OSC or whitelisted CSI
                        // reports — `StdinAnsiParser::feed` strips both
                        // before the keyboard parser sees the bytes.
                        // Every termwiz event is a key/mouse/paste/etc.
                        for input_event in events.into_iter() {
                            send_input_instructions
                                .send(InputInstruction::KeyEvent(
                                    input_event,
                                    current_buffer.drain(..).collect(),
                                ))
                                .unwrap();
                        }

                        schedule_finalization(
                            &stdin_ansi_parser,
                            true,
                            &mut needs_finalization,
                            &mut reply_in_progress_since,
                        );
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
            },
            Err(mpsc::RecvTimeoutError::Timeout) => {
                let pending = stdin_ansi_parser.lock().unwrap().pending_partial();
                match pending {
                    PendingPartial::ReplyInProgress => {
                        let elapsed = reply_in_progress_since
                            .map(|since| since.elapsed())
                            .unwrap_or_default();
                        if elapsed >= PARTIAL_REPLY_FLUSH_GUARD {
                            let drained = stdin_ansi_parser.lock().unwrap().finalize_force();
                            drain_partial_to_keyboard(
                                &mut input_parser,
                                &mut current_buffer,
                                send_input_instructions.clone(),
                                drained,
                            );
                            needs_finalization = false;
                            reply_in_progress_since = None;
                        } else {
                            needs_finalization = true;
                        }
                    },
                    _ => {
                        let drained = stdin_ansi_parser.lock().unwrap().finalize_lone_esc();
                        drain_partial_to_keyboard(
                            &mut input_parser,
                            &mut current_buffer,
                            send_input_instructions.clone(),
                            drained,
                        );
                        needs_finalization = false;
                        reply_in_progress_since = None;
                    },
                }
            },
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                log::debug!("STDIN pump disconnected");
                let _ = send_input_instructions.send(InputInstruction::Exit);
                break;
            },
        }
    }
}

fn schedule_finalization(
    stdin_ansi_parser: &Arc<Mutex<StdinAnsiParser>>,
    fed_termwiz: bool,
    needs_finalization: &mut bool,
    reply_in_progress_since: &mut Option<Instant>,
) {
    let pending = stdin_ansi_parser.lock().unwrap().pending_partial();
    if fed_termwiz || pending != PendingPartial::None {
        *needs_finalization = true;
    }
    if pending == PendingPartial::ReplyInProgress {
        if reply_in_progress_since.is_none() {
            *reply_in_progress_since = Some(Instant::now());
        }
    } else {
        *reply_in_progress_since = None;
    }
}

fn drain_partial_to_keyboard(
    input_parser: &mut InputParser,
    current_buffer: &mut Vec<u8>,
    send_input_instructions: SenderWithContext<InputInstruction>,
    drained: Vec<u8>,
) {
    if !drained.is_empty() {
        current_buffer.extend_from_slice(&drained);
    }

    let mut events = vec![];
    input_parser.parse(
        &drained,
        |input_event: InputEvent| {
            events.push(input_event);
        },
        false,
    );
    for input_event in events {
        send_input_instructions
            .send(InputInstruction::KeyEvent(
                input_event,
                current_buffer.drain(..).collect(),
            ))
            .unwrap();
    }
}

/// Build the fire-and-forget host-query batch sent at client startup.
/// The host's replies refine `Screen`'s cached state asynchronously as
/// they arrive; the UI does not block on them.
fn build_startup_query_string() -> String {
    // <ESC>[14t => get text area size in pixels,
    // <ESC>[16t => get character cell size in pixels
    // <ESC>]11;?<ESC>\ => get background color
    // <ESC>]10;?<ESC>\ => get foreground color
    // <ESC>[?2026$p => get synchronised output mode
    String::from("\u{1b}[14t\u{1b}[16t\u{1b}]11;?\u{1b}\u{5c}\u{1b}]10;?\u{1b}\u{5c}\u{1b}[?2026$p")
}

#[cfg(test)]
mod tests {
    use super::build_startup_query_string;

    #[test]
    fn startup_query_has_no_palette_register_loop() {
        let query = build_startup_query_string();
        assert_eq!(
            query,
            "\u{1b}[14t\u{1b}[16t\u{1b}]11;?\u{1b}\u{5c}\u{1b}]10;?\u{1b}\u{5c}\u{1b}[?2026$p"
        );
        assert!(
            !query.contains("\u{1b}]4;"),
            "startup query must not contain OSC 4 palette-register probes: {:?}",
            query
        );
    }
}
