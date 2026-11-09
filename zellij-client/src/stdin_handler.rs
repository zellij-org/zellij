use crate::keyboard_parser::{KittyKeyboardParser, KittyParseOutcome};
use crate::os_input_output::ClientOsApi;
#[cfg(windows)]
use crate::os_input_output_windows::use_vt_path;
use crate::stdin_ansi_parser::{HostReply, PendingPartial, StdinAnsiParser};
#[cfg(windows)]
use crate::stdin_handler_windows::enable_vt_input;
use crate::InputInstruction;
use std::ops::Range;
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
    query_host_theme: bool,
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
            let query_string =
                build_startup_query_string(support_kitty_graphics_protocol, query_host_theme);
            {
                let mut stdin_ansi_parser = stdin_ansi_parser.lock().unwrap();
                stdin_ansi_parser.open_own_query_batch();
                stdin_ansi_parser.expect_grapheme_cluster_mode_reply();
                if support_kitty_graphics_protocol {
                    stdin_ansi_parser.expect_kitty_probe_reply();
                    stdin_ansi_parser.expect_kitty_zlib_probe_reply();
                }
                let mut stdout = os_input.get_stdout_writer();
                let _ = stdout.write_all(query_string.as_bytes());
                let _ = stdout.flush();
            }
            if !support_kitty_graphics_protocol {
                let _ =
                    send_input_instructions.send(InputInstruction::AnsiStdinInstructions(vec![
                        HostReply::KittyGraphicsSupport(false),
                        HostReply::KittyZlibSupport(false),
                    ]));
            }
        } else {
            let _ = send_input_instructions.send(InputInstruction::AnsiStdinInstructions(vec![
                HostReply::KittyGraphicsSupport(false),
                HostReply::KittyZlibSupport(false),
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
                        let (parse_output, exit_draining) = {
                            let mut p = stdin_ansi_parser.lock().unwrap();
                            let parse_output = p.feed(&buf);
                            (parse_output, p.is_exit_draining())
                        };
                        if exit_draining {
                            current_buffer.clear();
                            needs_finalization = false;
                            reply_in_progress_since = None;
                            continue;
                        }
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
                        if let Some((token, reply_bytes)) = parse_output.completed_clipboard_forward
                        {
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
                        let paste_spans = parse_output.paste_spans;
                        let (residue, focus_changes, paste_spans) =
                            extract_focus_reports(parse_output.residue, &paste_spans);
                        for focused in focus_changes {
                            let _ = send_input_instructions
                                .send(InputInstruction::HostTerminalFocusChanged(focused));
                        }
                        if residue.is_empty() {
                            schedule_finalization(
                                &stdin_ansi_parser,
                                false,
                                &mut needs_finalization,
                                &mut reply_in_progress_since,
                            );
                            continue;
                        }
                        current_buffer.extend_from_slice(&residue);

                        let fed_termwiz = match parse_residue(
                            &residue,
                            &paste_spans,
                            &mut current_buffer,
                            &mut input_parser,
                            &mut kitty_parser,
                            !explicitly_disable_kitty_keyboard_protocol,
                            |instruction| send_input_instructions.send(instruction).is_ok(),
                        ) {
                            Some(fed_termwiz) => fed_termwiz,
                            None => break 'stdin,
                        };

                        schedule_finalization(
                            &stdin_ansi_parser,
                            fed_termwiz,
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
                if stdin_ansi_parser.lock().unwrap().is_exit_draining() {
                    needs_finalization = false;
                    reply_in_progress_since = None;
                    continue;
                }
                let pending = stdin_ansi_parser.lock().unwrap().pending_partial();
                if pending.uses_reply_flush_guard() {
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
                } else {
                    let drained = stdin_ansi_parser.lock().unwrap().finalize_fast_partial();
                    drain_partial_to_keyboard(
                        &mut input_parser,
                        &mut current_buffer,
                        send_input_instructions.clone(),
                        drained,
                    );
                    needs_finalization = false;
                    reply_in_progress_since = None;
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
    if pending.uses_reply_flush_guard() {
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
        let _ = send_input_instructions
            .send(InputInstruction::KeyEvent(input_event, raw_bytes));
    }
    realign_current_buffer(current_buffer, input_parser);
}

/// Decode one residue chunk in input order. Paste spans go to termwiz
/// verbatim; non-paste spans are first offered to the Kitty parser and then
/// to termwiz. Returns whether termwiz saw bytes, or None when the output
/// channel closed.
fn parse_residue<F>(
    residue: &[u8],
    paste_spans: &[Range<usize>],
    current_buffer: &mut Vec<u8>,
    input_parser: &mut InputParser,
    kitty_parser: &mut KittyKeyboardParser,
    kitty_enabled: bool,
    mut send: F,
) -> Option<bool>
where
    F: FnMut(InputInstruction) -> bool,
{
    let mut fed_termwiz = false;
    let mut cursor = 0;
    for span in paste_spans {
        let start = span.start.min(residue.len()).max(cursor);
        let end = span.end.min(residue.len()).max(start);
        if cursor < start {
            fed_termwiz |= parse_normal_span(
                residue,
                cursor,
                start,
                current_buffer,
                input_parser,
                kitty_parser,
                kitty_enabled,
                &mut send,
            )?;
        }
        if start < end {
            // A key sequence cannot validly continue across literal paste
            // bytes, even when the two spans arrive in the same read.
            *kitty_parser = KittyKeyboardParser::new();
            feed_termwiz(
                &residue[start..end],
                current_buffer,
                input_parser,
                &mut send,
            )?;
            fed_termwiz = true;
        }
        cursor = end;
    }
    if cursor < residue.len() {
        fed_termwiz |= parse_normal_span(
            residue,
            cursor,
            residue.len(),
            current_buffer,
            input_parser,
            kitty_parser,
            kitty_enabled,
            &mut send,
        )?;
    }
    if fed_termwiz {
        realign_current_buffer(current_buffer, input_parser);
    }
    Some(fed_termwiz)
}

fn parse_normal_span<F>(
    residue: &[u8],
    start: usize,
    end: usize,
    current_buffer: &mut Vec<u8>,
    input_parser: &mut InputParser,
    kitty_parser: &mut KittyKeyboardParser,
    kitty_enabled: bool,
    send: &mut F,
) -> Option<bool>
where
    F: FnMut(InputInstruction) -> bool,
{
    let mut kitty_processed = 0;
    if kitty_enabled {
        loop {
            let remaining = &residue[start + kitty_processed..end];
            if remaining.is_empty() {
                break;
            }
            match kitty_parser.feed_with_consumed(remaining) {
                (KittyParseOutcome::Complete(key_with_modifier), consumed) => {
                    let consumed = consumed.min(remaining.len());
                    if consumed == 0 {
                        break;
                    }
                    let offset = start + kitty_processed;
                    let raw_bytes =
                        drain_kitty_bytes(current_buffer, residue.len(), offset, consumed);
                    kitty_processed += consumed;
                    if !send(InputInstruction::KeyWithModifierEvent(
                        key_with_modifier,
                        raw_bytes,
                        true,
                    )) {
                        return None;
                    }
                },
                (KittyParseOutcome::Incomplete | KittyParseOutcome::NoMatch, _) => break,
            }
        }
    }

    let termwiz_bytes = &residue[start + kitty_processed..end];
    if termwiz_bytes.is_empty() {
        return Some(false);
    }
    feed_termwiz(termwiz_bytes, current_buffer, input_parser, send)?;
    Some(true)
}

fn drain_kitty_bytes(
    current_buffer: &mut Vec<u8>,
    residue_len: usize,
    offset: usize,
    consumed: usize,
) -> Vec<u8> {
    // `current_buffer` also contains any bytes pending from previous
    // reads. Drain those, the residue preceding this sequence, and the
    // sequence itself.
    let remaining_len = residue_len.saturating_sub(offset);
    let pending_before_residue = current_buffer.len().saturating_sub(remaining_len);
    let take = pending_before_residue
        .saturating_add(consumed)
        .min(current_buffer.len());
    current_buffer.drain(..take).collect()
}

fn feed_termwiz<F>(
    bytes: &[u8],
    current_buffer: &mut Vec<u8>,
    input_parser: &mut InputParser,
    send: &mut F,
) -> Option<()>
where
    F: FnMut(InputInstruction) -> bool,
{
    let mut events: Vec<(InputEvent, usize)> = vec![];
    input_parser.parse_with_consumed(
        bytes,
        |input_event: InputEvent, consumed: usize| {
            events.push((input_event, consumed));
        },
        true,
    );
    for (input_event, consumed) in events {
        let take = consumed.min(current_buffer.len());
        let raw_bytes: Vec<u8> = current_buffer.drain(..take).collect();
        if !send(InputInstruction::KeyEvent(input_event, raw_bytes)) {
            return None;
        }
    }
    Some(())
}

fn extract_focus_reports(
    residue: Vec<u8>,
    paste_spans: &[Range<usize>],
) -> (Vec<u8>, Vec<bool>, Vec<Range<usize>>) {
    let mut remaining = Vec::with_capacity(residue.len());
    let mut focus_changes = vec![];
    let mut remaining_paste_spans = vec![];
    let mut cursor = 0;
    for span in paste_spans {
        let start = span.start.min(residue.len()).max(cursor);
        let end = span.end.min(residue.len()).max(start);
        if cursor < start {
            let (cleaned, focus) = extract_focus_reports_from(&residue[cursor..start]);
            remaining.extend_from_slice(&cleaned);
            focus_changes.extend(focus);
        }
        let out_start = remaining.len();
        remaining.extend_from_slice(&residue[start..end]);
        if remaining.len() > out_start {
            remaining_paste_spans.push(out_start..remaining.len());
        }
        cursor = end;
    }
    if cursor < residue.len() {
        let (cleaned, focus) = extract_focus_reports_from(&residue[cursor..]);
        remaining.extend_from_slice(&cleaned);
        focus_changes.extend(focus);
    }
    (remaining, focus_changes, remaining_paste_spans)
}

fn extract_focus_reports_from(residue: &[u8]) -> (Vec<u8>, Vec<bool>) {
    const FOCUS_GAINED: &[u8] = b"\x1b[I";
    const FOCUS_LOST: &[u8] = b"\x1b[O";
    if residue.len() < FOCUS_GAINED.len() {
        return (residue.to_vec(), vec![]);
    }
    let mut remaining = Vec::with_capacity(residue.len());
    let mut focus_changes = vec![];
    let mut index = 0;
    while index < residue.len() {
        let rest = &residue[index..];
        if rest.starts_with(FOCUS_GAINED) {
            focus_changes.push(true);
            index += FOCUS_GAINED.len();
        } else if rest.starts_with(FOCUS_LOST) {
            focus_changes.push(false);
            index += FOCUS_LOST.len();
        } else {
            remaining.push(residue[index]);
            index += 1;
        }
    }
    (remaining, focus_changes)
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
fn build_startup_query_string(
    support_kitty_graphics_protocol: bool,
    query_host_theme: bool,
) -> String {
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
        "\u{1b}_Ga=q,i=31,s=1,v=1,t=d,f=24;AAAA\u{1b}\u{5c}\u{1b}_Ga=q,i=32,s=4,v=4,t=d,f=24,o=z;eJxjYCANAAAAMAAB\u{1b}\u{5c}"
    } else {
        ""
    };
    let host_theme_query = if query_host_theme {
        crate::QUERY_HOST_THEME
    } else {
        ""
    };
    format!(
        "{}{}\u{1b}]11;?\u{1b}\u{5c}\u{1b}]10;?\u{1b}\u{5c}\u{1b}[?2026$p{}{}\u{1b}[?2027$p\u{1b}[c",
        host_theme_query, PIXEL_SIZE_QUERY, kitty_graphics_probe, PROGRAM_STATUS_QUERY
    )
}

pub(crate) const PIXEL_SIZE_QUERY: &str = "\u{1b}[14t\u{1b}[16t";
pub(crate) const PROGRAM_STATUS_QUERY: &str = "\u{1b}]7501;?\u{1b}\u{5c}";

#[cfg(test)]
mod tests {
    use super::{
        build_startup_query_string, extract_focus_reports, parse_residue, realign_current_buffer,
        InputEvent, InputInstruction, InputParser, PIXEL_SIZE_QUERY,
    };
    use crate::keyboard_parser::KittyKeyboardParser;
    use crate::stdin_ansi_parser::StdinAnsiParser;
    use zellij_utils::data::{BareKey, KeyWithModifier};

    fn decode_chunks(chunks: &[&[u8]]) -> (Vec<InputInstruction>, Vec<bool>) {
        let mut ansi_parser = StdinAnsiParser::new();
        let mut input_parser = InputParser::new();
        let mut kitty_parser = KittyKeyboardParser::new();
        let mut current_buffer = vec![];
        let mut instructions = vec![];
        let mut focus_changes = vec![];
        for chunk in chunks {
            let output = ansi_parser.feed(chunk);
            let (residue, focus, paste_spans) =
                extract_focus_reports(output.residue, &output.paste_spans);
            focus_changes.extend(focus);
            current_buffer.extend_from_slice(&residue);
            let result = parse_residue(
                &residue,
                &paste_spans,
                &mut current_buffer,
                &mut input_parser,
                &mut kitty_parser,
                true,
                |instruction| {
                    instructions.push(instruction);
                    true
                },
            );
            assert!(result.is_some(), "test channel unexpectedly closed");
        }
        assert!(
            current_buffer.is_empty(),
            "all complete input should be drained: {:?}",
            current_buffer
        );
        (instructions, focus_changes)
    }

    fn assert_kitty_key(
        instruction: &InputInstruction,
        expected: KeyWithModifier,
        expected_raw: &[u8],
    ) {
        match instruction {
            InputInstruction::KeyWithModifierEvent(key, raw, true) => {
                assert_eq!(key, &expected);
                assert_eq!(raw, expected_raw);
            },
            other => panic!("expected Kitty key event, got {:?}", other),
        }
    }

    fn assert_paste(instruction: &InputInstruction, expected: &str, expected_raw: &[u8]) {
        match instruction {
            InputInstruction::KeyEvent(InputEvent::Paste(pasted), raw) => {
                assert_eq!(pasted, expected);
                assert_eq!(raw, expected_raw);
            },
            other => panic!("expected paste event, got {:?}", other),
        }
    }

    fn assert_plain_key(instruction: &InputInstruction, expected: char) {
        match instruction {
            InputInstruction::KeyEvent(
                InputEvent::Key(zellij_utils::vendored::termwiz::input::KeyEvent {
                    key: zellij_utils::vendored::termwiz::input::KeyCode::Char(key),
                    ..
                }),
                raw,
            ) => {
                assert_eq!(*key, expected);
                assert_eq!(raw, &[expected as u8]);
            },
            other => panic!("expected plain key event, got {:?}", other),
        }
    }

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
        let query = build_startup_query_string(true, false);
        assert_eq!(
            query,
            "\u{1b}[14t\u{1b}[16t\u{1b}]11;?\u{1b}\u{5c}\u{1b}]10;?\u{1b}\u{5c}\u{1b}[?2026$p\u{1b}_Ga=q,i=31,s=1,v=1,t=d,f=24;AAAA\u{1b}\u{5c}\u{1b}_Ga=q,i=32,s=4,v=4,t=d,f=24,o=z;eJxjYCANAAAAMAAB\u{1b}\u{5c}\u{1b}]7501;?\u{1b}\u{5c}\u{1b}[?2027$p\u{1b}[c"
        );
        assert!(
            !query.contains("\u{1b}]4;"),
            "startup query must not contain OSC 4 palette-register probes: {:?}",
            query
        );
    }

    #[test]
    fn focus_reports_are_extracted_from_the_byte_stream() {
        let (residue, focus_changes, _) = extract_focus_reports(b"a\x1b[Ib\x1b[Oc".to_vec(), &[]);
        assert_eq!(residue, b"abc".to_vec());
        assert_eq!(focus_changes, vec![true, false]);
    }

    #[test]
    fn a_stream_without_focus_reports_is_left_untouched() {
        let (residue, focus_changes, _) = extract_focus_reports(b"\x1b[A\x1b[B".to_vec(), &[]);
        assert_eq!(residue, b"\x1b[A\x1b[B".to_vec());
        assert!(focus_changes.is_empty());
    }

    #[test]
    fn focus_like_bytes_inside_a_paste_are_preserved_verbatim() {
        let pasted = b"\x1b[200~a\x1b[Ib\x1b[Oc\x1b[201~";
        let (residue, focus_changes, paste_spans) =
            extract_focus_reports(pasted.to_vec(), &[0..pasted.len()]);
        assert_eq!(residue, pasted.to_vec());
        assert!(focus_changes.is_empty());
        assert_eq!(paste_spans, vec![0..pasted.len()]);
    }

    #[test]
    fn paste_followed_by_a_kitty_key_in_one_read_keeps_both() {
        let (events, focus) = decode_chunks(&[b"\x1b[200~pasted\x1b[201~\x1b[97u"]);
        assert!(focus.is_empty());
        assert_eq!(events.len(), 2);
        assert_paste(&events[0], "pasted", b"\x1b[200~pasted\x1b[201~");
        assert_kitty_key(
            &events[1],
            KeyWithModifier::new(BareKey::Char('a')),
            b"\x1b[97u",
        );
    }

    #[test]
    fn kitty_key_followed_by_paste_in_one_read_keeps_both() {
        let (events, focus) = decode_chunks(&[b"\x1b[97u\x1b[200~pasted\x1b[201~"]);
        assert!(focus.is_empty());
        assert_eq!(events.len(), 2);
        assert_kitty_key(
            &events[0],
            KeyWithModifier::new(BareKey::Char('a')),
            b"\x1b[97u",
        );
        assert_paste(&events[1], "pasted", b"\x1b[200~pasted\x1b[201~");
    }

    #[test]
    fn paste_content_that_looks_like_keys_and_focus_is_not_decoded() {
        let (events, focus) =
            decode_chunks(&[b"\x1b[200~before\x1b[97u\x1b[I\x1b[Oafter\x1b[201~"]);
        assert!(focus.is_empty());
        assert_eq!(events.len(), 1);
        assert_paste(
            &events[0],
            "before\x1b[97u\x1b[I\x1b[Oafter",
            b"\x1b[200~before\x1b[97u\x1b[I\x1b[Oafter\x1b[201~",
        );
    }

    #[test]
    fn focus_reports_outside_a_paste_still_work_alongside_a_kitty_key() {
        let (events, focus) = decode_chunks(&[b"\x1b[I\x1b[200~P\x1b[201~\x1b[O\x1b[97u"]);
        assert_eq!(focus, vec![true, false]);
        assert_eq!(events.len(), 2);
        assert_paste(&events[0], "P", b"\x1b[200~P\x1b[201~");
        assert_kitty_key(
            &events[1],
            KeyWithModifier::new(BareKey::Char('a')),
            b"\x1b[97u",
        );
    }

    #[test]
    fn a_paste_fragmented_across_reads_keeps_its_escape_content_literal() {
        let (events, focus) = decode_chunks(&[
            b"\x1b[200~hel",
            b"lo \x1b[97u\x1b[I\x1b[O",
            b" world\x1b[201~",
        ]);
        assert!(focus.is_empty());
        assert_eq!(events.len(), 1);
        // The paste-start marker was already consumed by termwiz during the
        // first read, so only the pending body and end marker remain here.
        assert_paste(
            &events[0],
            "hello \x1b[97u\x1b[I\x1b[O world",
            b"hello \x1b[97u\x1b[I\x1b[O world\x1b[201~",
        );
    }

    #[test]
    fn ordinary_keys_around_a_paste_keep_their_order() {
        let (events, focus) = decode_chunks(&[b"a\x1b[200~P\x1b[201~b"]);
        assert!(focus.is_empty());
        assert_eq!(events.len(), 3);
        assert_plain_key(&events[0], 'a');
        assert_paste(&events[1], "P", b"\x1b[200~P\x1b[201~");
        assert_plain_key(&events[2], 'b');
    }

    #[test]
    fn startup_query_leads_with_the_host_theme_query_when_requested() {
        let query = build_startup_query_string(false, true);
        assert!(query.starts_with("\u{1b}[?996n\u{1b}[14t\u{1b}[16t"));
        assert!(query.ends_with("\u{1b}[c"));
    }

    #[test]
    fn startup_query_contains_kitty_probe_before_barrier() {
        let query = build_startup_query_string(true, false);
        assert!(query.contains("\u{1b}_Ga=q,i=31,s=1,v=1,t=d,f=24;AAAA\u{1b}\u{5c}"));
        let probe_pos = query.find("\u{1b}_Ga=q,i=31,").unwrap();
        let barrier_pos = query.find("\u{1b}[c").unwrap();
        assert!(probe_pos < barrier_pos);
    }

    #[test]
    fn startup_query_contains_zlib_probe_between_kitty_probe_and_barrier() {
        let query = build_startup_query_string(true, false);
        let probe_pos = query.find("\u{1b}_Ga=q,i=31,").unwrap();
        let zlib_probe_pos = query
            .find("\u{1b}_Ga=q,i=32,s=4,v=4,t=d,f=24,o=z;eJxjYCANAAAAMAAB\u{1b}\u{5c}")
            .unwrap();
        let barrier_pos = query.find("\u{1b}[c").unwrap();
        assert!(probe_pos < zlib_probe_pos);
        assert!(zlib_probe_pos < barrier_pos);
    }

    #[test]
    fn startup_query_asks_for_grapheme_cluster_mode_right_before_the_barrier() {
        for (kitty, theme) in [(true, true), (false, false)] {
            let query = build_startup_query_string(kitty, theme);
            assert!(query.ends_with("\u{1b}[?2027$p\u{1b}[c"), "{:?}", query);
            assert_eq!(query.matches("\u{1b}[?2027$p").count(), 1);
        }
    }

    #[test]
    fn startup_query_omits_kitty_probe_when_the_protocol_is_disabled() {
        let query = build_startup_query_string(false, false);
        assert!(!query.contains("\u{1b}_G"));
        assert!(query.ends_with("\u{1b}[c"));
        assert!(query.starts_with("\u{1b}[14t\u{1b}[16t"));
    }
}
