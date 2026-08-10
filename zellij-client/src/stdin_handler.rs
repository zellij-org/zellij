use crate::keyboard_parser::{KittyKeyboardParser, KittyParseOutcome};
use crate::os_input_output::ClientOsApi;
#[cfg(windows)]
use crate::os_input_output_windows::use_vt_path;
use crate::stdin_ansi_parser::{HostReply, PendingPartial, StdinAnsiParser};
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
    support_kitty_graphics_protocol: bool,
    resize_sender: Option<std::sync::mpsc::Sender<()>>,
) {
    // On Windows we choose between the VT byte path (termwiz/kitty parsing)
    // and the native-console path (crossterm INPUT_RECORDs) early, before the
    // startup ANSI query below. See `use_vt_path()` for the trigger conditions.
    #[cfg(windows)]
    let use_vt_reader = use_vt_path() && enable_vt_input();

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
            let query_string = build_startup_query_string(support_kitty_graphics_protocol);
            let _ = os_input
                .get_stdout_writer()
                .write(query_string.as_bytes())
                .unwrap();
            if support_kitty_graphics_protocol {
                stdin_ansi_parser.lock().unwrap().expect_kitty_probe_reply();
            } else {
                let _ =
                    send_input_instructions.send(InputInstruction::AnsiStdinInstructions(vec![
                        HostReply::KittyGraphicsSupport(false),
                    ]));
            }
        } else {
            let _ = send_input_instructions.send(InputInstruction::AnsiStdinInstructions(vec![
                HostReply::KittyGraphicsSupport(false),
                HostReply::SixelSupport(false),
            ]));
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
    'stdin: loop {
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
                        for payload_bytes in parse_output.nested_frames {
                            let _ = send_input_instructions
                                .send(InputInstruction::NestedSessionFrameFromHost(payload_bytes));
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
                                    if send_input_instructions
                                        .send(InputInstruction::KeyWithModifierEvent(
                                            key_with_modifier,
                                            current_buffer.drain(..).collect(),
                                            true,
                                        ))
                                        .is_err()
                                    {
                                        break 'stdin;
                                    }
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
                        let mut events: Vec<(InputEvent, usize)> = vec![];
                        input_parser.parse_with_consumed(
                            &residue,
                            |input_event: InputEvent, consumed: usize| {
                                events.push((input_event, consumed));
                            },
                            maybe_more,
                        );

                        // Residue contains no OSC or whitelisted CSI
                        // reports — `StdinAnsiParser::feed` strips both
                        // before the keyboard parser sees the bytes.
                        // Every termwiz event is a key/mouse/paste/etc.
                        // Each event is forwarded with exactly the bytes
                        // that produced it, never bytes belonging to other
                        // events decoded from the same read.
                        for (input_event, consumed) in events.into_iter() {
                            let take = consumed.min(current_buffer.len());
                            let raw_bytes: Vec<u8> = current_buffer.drain(..take).collect();
                            if send_input_instructions
                                .send(InputInstruction::KeyEvent(input_event, raw_bytes))
                                .is_err()
                            {
                                break 'stdin;
                            }
                        }
                        realign_current_buffer(&mut current_buffer, &input_parser);

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

    let mut events: Vec<(InputEvent, usize)> = vec![];
    input_parser.parse_with_consumed(
        &drained,
        |input_event: InputEvent, consumed: usize| {
            events.push((input_event, consumed));
        },
        false,
    );
    for (input_event, consumed) in events {
        let take = consumed.min(current_buffer.len());
        let raw_bytes: Vec<u8> = current_buffer.drain(..take).collect();
        send_input_instructions
            .send(InputInstruction::KeyEvent(input_event, raw_bytes))
            .unwrap();
    }
    realign_current_buffer(current_buffer, input_parser);
}

/// Trim `current_buffer` to the parser's own buffered length so it can
/// never drift from the parser's internal state: a trailing incomplete
/// sequence is held by the parser (and mirrored here) until the next
/// read completes it, while bytes already decoded into events are dropped.
fn realign_current_buffer(current_buffer: &mut Vec<u8>, input_parser: &InputParser) {
    let buffered = input_parser.buffered_len();
    let excess = current_buffer.len().saturating_sub(buffered);
    if excess > 0 {
        current_buffer.drain(..excess);
    }
}

/// Build the fire-and-forget host-query batch sent at client startup.
/// The host's replies refine `Screen`'s cached state asynchronously as
/// they arrive; the UI does not block on them.
fn build_startup_query_string(support_kitty_graphics_protocol: bool) -> String {
    // <ESC>[14t => get text area size in pixels,
    // <ESC>[16t => get character cell size in pixels
    // <ESC>]11;?<ESC>\ => get background color
    // <ESC>]10;?<ESC>\ => get foreground color
    // <ESC>[?2026$p => get synchronised output mode
    // <ESC>_Ga=q,...<ESC>\ => probe kitty graphics support (omitted when the
    // protocol is disabled), answered by capable terminals only; the trailing
    // Primary DA is the barrier that resolves the probe negatively when it
    // goes unanswered
    let kitty_graphics_probe = if support_kitty_graphics_protocol {
        "\u{1b}_Ga=q,i=31,s=1,v=1,t=d,f=24;AAAA\u{1b}\u{5c}"
    } else {
        ""
    };
    format!(
        "{}\u{1b}]11;?\u{1b}\u{5c}\u{1b}]10;?\u{1b}\u{5c}\u{1b}[?2026$p{}\u{1b}[c",
        PIXEL_SIZE_QUERY, kitty_graphics_probe
    )
}

pub(crate) const PIXEL_SIZE_QUERY: &str = "\u{1b}[14t\u{1b}[16t";

#[cfg(test)]
mod tests {
    use super::{
        build_startup_query_string, realign_current_buffer, InputParser, PIXEL_SIZE_QUERY,
    };

    #[test]
    fn realign_after_lone_paste_start_empties_the_buffer() {
        let mut parser = InputParser::new();
        parser.parse_with_consumed(b"\x1b[200~", |_, _| {}, true);
        let mut current_buffer = b"\x1b[200~".to_vec();
        realign_current_buffer(&mut current_buffer, &parser);
        assert!(
            current_buffer.is_empty(),
            "the silently consumed paste-start bytes must not linger: {:?}",
            current_buffer
        );
    }

    #[test]
    fn realign_drops_stale_bytes_from_the_front_and_keeps_the_pending_tail() {
        let mut parser = InputParser::new();
        parser.parse_with_consumed(b"\x1b[200~hel", |_, _| {}, true);
        let mut current_buffer = b"\x1b[200~hel".to_vec();
        realign_current_buffer(&mut current_buffer, &parser);
        assert_eq!(
            current_buffer, b"hel",
            "the retained bytes must be the parser's pending tail, not the stale front"
        );
    }

    #[test]
    fn realign_is_a_no_op_when_nothing_was_consumed() {
        let mut parser = InputParser::new();
        parser.parse_with_consumed(b"\x1b[1;2", |_, _| {}, true);
        let mut current_buffer = b"\x1b[1;2".to_vec();
        realign_current_buffer(&mut current_buffer, &parser);
        assert_eq!(current_buffer, b"\x1b[1;2");
    }

    #[test]
    fn realign_tolerates_a_shorter_mirror_buffer() {
        let mut parser = InputParser::new();
        parser.parse_with_consumed(b"\x1b[1;2", |_, _| {}, true);
        let mut current_buffer = b";2".to_vec();
        realign_current_buffer(&mut current_buffer, &parser);
        assert_eq!(current_buffer, b";2");
    }

    #[test]
    fn pixel_size_query_probes_text_area_and_character_cell() {
        assert_eq!(PIXEL_SIZE_QUERY, "\u{1b}[14t\u{1b}[16t");
    }

    #[test]
    fn startup_query_has_no_palette_register_loop() {
        let query = build_startup_query_string(true);
        assert_eq!(
            query,
            "\u{1b}[14t\u{1b}[16t\u{1b}]11;?\u{1b}\u{5c}\u{1b}]10;?\u{1b}\u{5c}\u{1b}[?2026$p\u{1b}_Ga=q,i=31,s=1,v=1,t=d,f=24;AAAA\u{1b}\u{5c}\u{1b}[c"
        );
        assert!(
            !query.contains("\u{1b}]4;"),
            "startup query must not contain OSC 4 palette-register probes: {:?}",
            query
        );
    }

    #[test]
    fn startup_query_contains_kitty_probe_before_barrier() {
        let query = build_startup_query_string(true);
        assert!(query.contains("\u{1b}_Ga=q,i=31,s=1,v=1,t=d,f=24;AAAA\u{1b}\u{5c}"));
        let probe_pos = query.find("\u{1b}_Ga=q,i=31,").unwrap();
        let barrier_pos = query.find("\u{1b}[c").unwrap();
        assert!(probe_pos < barrier_pos);
    }

    #[test]
    fn startup_query_omits_kitty_probe_when_the_protocol_is_disabled() {
        let query = build_startup_query_string(false);
        assert!(!query.contains("\u{1b}_G"));
        assert!(query.ends_with("\u{1b}[c"));
        assert!(query.starts_with("\u{1b}[14t\u{1b}[16t"));
    }
}
