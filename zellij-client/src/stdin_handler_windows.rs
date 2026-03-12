use std::sync::OnceLock;

use crossterm::event::{self, Event, KeyEventKind};
use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
use windows_sys::Win32::System::Console::{
    GetConsoleMode, GetStdHandle, SetConsoleMode, ENABLE_EXTENDED_FLAGS, ENABLE_MOUSE_INPUT,
    ENABLE_VIRTUAL_TERMINAL_INPUT, ENABLE_WINDOW_INPUT, STD_INPUT_HANDLE,
};

use crate::InputInstruction;
use zellij_utils::channels::SenderWithContext;
use zellij_utils::input::{cast_crossterm_key, from_crossterm_mouse};
use zellij_utils::vendored::termwiz::input::InputEvent;

/// Saved console input mode from before `enable_vt_input()` modified it.
/// Used by `restore_vt_input()` to put the console back the way the shell
/// left it, clearing flags like ENABLE_MOUSE_INPUT that crossterm's
/// disable_raw_mode() does not touch.
static ORIGINAL_CONSOLE_MODE: OnceLock<u32> = OnceLock::new();

/// Set the stdin console mode for raw VT input.
///
/// Instead of just ORing in ENABLE_VIRTUAL_TERMINAL_INPUT on top of whatever
/// the current mode happens to be, we explicitly set the exact mode we need.
/// This avoids a TOCTOU race with crossterm's EnableMouseCapture (which also
/// does GetConsoleMode/SetConsoleMode) and ensures flags like
/// ENABLE_QUICK_EDIT_MODE are always cleared — that flag intercepts mouse
/// events at the console level, breaking application mouse support.
pub(crate) fn enable_vt_input() -> bool {
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

/// Windows native console event loop.
///
/// Uses crossterm's `event::read()` which reads INPUT_RECORDs via
/// ReadConsoleInput.  Works in cmd.exe, PowerShell, and Windows Terminal
/// where ALT is reported as a modifier flag.
///
/// Resize events are forwarded to the signal handler thread via `resize_sender`.
pub(crate) fn native_console_stdin_loop(
    send_input_instructions: SenderWithContext<InputInstruction>,
    resize_sender: Option<std::sync::mpsc::Sender<()>>,
) {
    loop {
        match event::read() {
            Ok(Event::Key(key_event)) => {
                if key_event.kind != KeyEventKind::Press {
                    continue;
                }
                if let Some((key, bytes)) = cast_crossterm_key(key_event) {
                    if send_input_instructions
                        .send(InputInstruction::KeyWithModifierEvent(key, bytes, false))
                        .is_err()
                    {
                        break;
                    }
                }
            },
            Ok(Event::Mouse(mouse_event)) => {
                let mouse_event = from_crossterm_mouse(mouse_event);
                if send_input_instructions
                    .send(InputInstruction::MouseEvent(mouse_event))
                    .is_err()
                {
                    break;
                }
            },
            Ok(Event::Paste(text)) => {
                let raw_bytes = text.as_bytes().to_vec();
                let paste_event = InputEvent::Paste(text);
                if send_input_instructions
                    .send(InputInstruction::KeyEvent(paste_event, raw_bytes))
                    .is_err()
                {
                    break;
                }
            },
            Ok(Event::Resize(..)) => {
                if let Some(ref tx) = resize_sender {
                    let _ = tx.send(());
                }
            },
            Ok(_) => {},
            Err(e) => {
                log::error!("Failed to read crossterm event: {}", e);
                let _ = send_input_instructions.send(InputInstruction::Exit);
                break;
            },
        }
    }
}
