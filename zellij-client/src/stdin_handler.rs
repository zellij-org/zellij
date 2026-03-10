use crate::keyboard_parser::KittyKeyboardParser;
use crate::os_input_output::ClientOsApi;
use crate::stdin_ansi_parser::StdinAnsiParser;
use crate::InputInstruction;
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;
use zellij_utils::{
    channels::SenderWithContext,
    vendored::termwiz::input::{InputEvent, InputParser},
};

#[cfg(windows)]
use std::sync::OnceLock;
#[cfg(windows)]
use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
#[cfg(windows)]
use windows_sys::Win32::System::Console::{
    GetConsoleMode, GetStdHandle, SetConsoleMode, ENABLE_EXTENDED_FLAGS, ENABLE_MOUSE_INPUT,
    ENABLE_VIRTUAL_TERMINAL_INPUT, ENABLE_WINDOW_INPUT, STD_INPUT_HANDLE,
};

/// Saved console input mode from before `enable_vt_input()` modified it.
/// Used by `restore_vt_input()` to put the console back the way the shell
/// left it, clearing flags like ENABLE_MOUSE_INPUT that crossterm's
/// disable_raw_mode() does not touch.
#[cfg(windows)]
static ORIGINAL_CONSOLE_MODE: OnceLock<u32> = OnceLock::new();

fn send_done_parsing_after_query_timeout(
    send_input_instructions: SenderWithContext<InputInstruction>,
    query_duration: u64,
) {
    std::thread::spawn({
        move || {
            std::thread::sleep(Duration::from_millis(query_duration));
            send_input_instructions
                .send(InputInstruction::DoneParsing)
                .unwrap();
        }
    });
}

/// On Windows, set the stdin console mode for raw VT input.
///
/// Instead of just ORing in ENABLE_VIRTUAL_TERMINAL_INPUT on top of whatever
/// the current mode happens to be, we explicitly set the exact mode we need.
/// This avoids a TOCTOU race with crossterm's EnableMouseCapture (which also
/// does GetConsoleMode/SetConsoleMode) and ensures flags like
/// ENABLE_QUICK_EDIT_MODE are always cleared — that flag intercepts mouse
/// events at the console level, breaking application mouse support.
#[cfg(windows)]
fn enable_vt_input() -> bool {
    unsafe {
        let handle = GetStdHandle(STD_INPUT_HANDLE);
        if handle == 0 || handle == INVALID_HANDLE_VALUE {
            return false;
        }
        let mut mode: u32 = 0;
        if GetConsoleMode(handle, &mut mode) == 0 {
            return false;
        }
        // Save the original mode so we can restore it on exit.
        let _ = ORIGINAL_CONSOLE_MODE.set(mode);
        // Explicitly set the mode we need rather than read-modify-write.
        // This eliminates the race with crossterm's EnableMouseCapture which
        // also calls GetConsoleMode/SetConsoleMode concurrently.
        //
        // Flags we set:
        //   ENABLE_WINDOW_INPUT           (0x0008) - receive window resize events
        //   ENABLE_MOUSE_INPUT            (0x0010) - receive mouse events; on ConPTY
        //                                            this signals the terminal emulator
        //                                            to capture and forward mouse input
        //   ENABLE_EXTENDED_FLAGS         (0x0080) - required to clear QUICK_EDIT
        //   ENABLE_VIRTUAL_TERMINAL_INPUT (0x0200) - stdin returns raw VT bytes
        //
        // Flags we deliberately clear:
        //   ENABLE_PROCESSED_INPUT  (0x0001) - let VT sequences through raw
        //   ENABLE_LINE_INPUT       (0x0002) - no line buffering
        //   ENABLE_ECHO_INPUT       (0x0004) - no echo
        //   ENABLE_QUICK_EDIT_MODE  (0x0040) - would intercept mouse events
        let new_mode = ENABLE_WINDOW_INPUT
            | ENABLE_MOUSE_INPUT
            | ENABLE_EXTENDED_FLAGS
            | ENABLE_VIRTUAL_TERMINAL_INPUT;
        if SetConsoleMode(handle, new_mode) == 0 {
            return false;
        }
        true
    }
}

/// Restore the console input mode that was saved by `enable_vt_input()`.
///
/// `crossterm::terminal::disable_raw_mode()` only adds back LINE_INPUT,
/// ECHO_INPUT and PROCESSED_INPUT — it never clears ENABLE_MOUSE_INPUT or
/// ENABLE_VIRTUAL_TERMINAL_INPUT.  If those flags are left set after Zellij
/// exits, ConPTY continues to deliver mouse events as VT escape sequences
/// into the shell's stdin, causing visible garbage like `[555;99;32M`.
#[cfg(windows)]
pub(crate) fn restore_vt_input() {
    if let Some(&original_mode) = ORIGINAL_CONSOLE_MODE.get() {
        unsafe {
            let handle = GetStdHandle(STD_INPUT_HANDLE);
            if handle != 0 && handle != INVALID_HANDLE_VALUE {
                SetConsoleMode(handle, original_mode);
            }
        }
    }
}

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
                // On Windows native console, the crossterm event::read() loop
                // reads INPUT_RECORDs via ReadConsoleInput — not raw bytes — so
                // ANSI query responses can never be read on that path. On the
                // VT reader path (TERM is set), fill_buf() reads raw VT bytes
                // just like Unix, so terminal queries work normally.
                #[cfg(windows)]
                let can_query_terminal = use_vt_reader;
                #[cfg(not(windows))]
                let can_query_terminal = true;

                if can_query_terminal {
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
                } else {
                    let _ = send_input_instructions.send(InputInstruction::DoneParsing);
                }
            },
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
    let mut current_buffer = vec![];
    let mut ansi_stdin_events = vec![];
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
    loop {
        match if needs_finalization {
            stdin_rx.recv_timeout(Duration::from_millis(50))
        } else {
            stdin_rx
                .recv()
                .map_err(|_| mpsc::RecvTimeoutError::Disconnected)
        } {
            Ok(result) => {
                match result {
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

                        if !explicitly_disable_kitty_keyboard_protocol {
                            // first we try to parse with the KittyKeyboardParser
                            // if we fail, we try to parse normally
                            match KittyKeyboardParser::new().parse(&buf) {
                                Some(key_with_modifier) => {
                                    send_input_instructions
                                        .send(InputInstruction::KeyWithModifierEvent(
                                            key_with_modifier,
                                            current_buffer.drain(..).collect(),
                                            true,
                                        ))
                                        .unwrap();
                                    continue;
                                },
                                None => {},
                            }
                        }

                        // Parse with maybe_more = true - complete events sent immediately
                        //
                        // Ambiguous events (if any) will be finalized later only if 50ms
                        // passes with no new input
                        let maybe_more = true;
                        let mut events = vec![];
                        input_parser.parse(
                            &buf,
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

                        needs_finalization = true;
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
                finalize_events(
                    &mut input_parser,
                    &mut current_buffer,
                    send_input_instructions.clone(),
                );
                needs_finalization = false;
            },
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                log::debug!("STDIN pump disconnected");
                let _ = send_input_instructions.send(InputInstruction::Exit);
                break;
            },
        }
    }
}

fn finalize_events(
    input_parser: &mut InputParser,
    current_buffer: &mut Vec<u8>,
    send_input_instructions: SenderWithContext<InputInstruction>,
) {
    let mut events = vec![];
    input_parser.parse(
        &[],
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
