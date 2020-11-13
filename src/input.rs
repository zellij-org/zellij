/// Module for handling input
use std::sync::mpsc::Sender;

use crate::os_input_output::OsApi;
use crate::pty_bus::PtyInstruction;
use crate::screen::ScreenInstruction;
use crate::AppInstruction;

pub fn input_loop(
    os_input: Box<dyn OsApi>,
    send_screen_instructions: Sender<ScreenInstruction>,
    send_pty_instructions: Sender<PtyInstruction>,
    send_app_instructions: Sender<AppInstruction>,
) {
    let mut stdin = os_input.get_stdin_reader();
    loop {
        let mut buffer = [0; 10]; // TODO: more accurately
        stdin.read(&mut buffer).expect("failed to read stdin");
        // uncomment this to print the entered character to a log file (/tmp/mosaic-log.txt) for debugging
        // _debug_log_to_file(format!("buffer {:?}", buffer));
        match buffer {
            [10, 0, 0, 0, 0, 0, 0, 0, 0, 0] => {
                // ctrl-j
                send_screen_instructions
                    .send(ScreenInstruction::ResizeDown)
                    .unwrap();
            }
            [11, 0, 0, 0, 0, 0, 0, 0, 0, 0] => {
                // ctrl-k
                send_screen_instructions
                    .send(ScreenInstruction::ResizeUp)
                    .unwrap();
            }
            [16, 0, 0, 0, 0, 0, 0, 0, 0, 0] => {
                // ctrl-p
                send_screen_instructions
                    .send(ScreenInstruction::MoveFocus)
                    .unwrap();
            }
            [8, 0, 0, 0, 0, 0, 0, 0, 0, 0] => {
                // ctrl-h
                send_screen_instructions
                    .send(ScreenInstruction::ResizeLeft)
                    .unwrap();
            }
            [12, 0, 0, 0, 0, 0, 0, 0, 0, 0] => {
                // ctrl-l
                send_screen_instructions
                    .send(ScreenInstruction::ResizeRight)
                    .unwrap();
            }
            [26, 0, 0, 0, 0, 0, 0, 0, 0, 0] => {
                // ctrl-z
                send_pty_instructions
                    .send(PtyInstruction::SpawnTerminal(None))
                    .unwrap();
            }
            [14, 0, 0, 0, 0, 0, 0, 0, 0, 0] => {
                // ctrl-n
                send_pty_instructions
                    .send(PtyInstruction::SpawnTerminalVertically(None))
                    .unwrap();
            }
            [2, 0, 0, 0, 0, 0, 0, 0, 0, 0] => {
                // ctrl-b
                send_pty_instructions
                    .send(PtyInstruction::SpawnTerminalHorizontally(None))
                    .unwrap();
            }
            [17, 0, 0, 0, 0, 0, 0, 0, 0, 0] => {
                // ctrl-q
                let _ = send_screen_instructions.send(ScreenInstruction::Quit);
                let _ = send_pty_instructions.send(PtyInstruction::Quit);
                let _ = send_app_instructions.send(AppInstruction::Exit);
                break;
            }
            [27, 91, 53, 94, 0, 0, 0, 0, 0, 0] => {
                // ctrl-PgUp
                send_screen_instructions
                    .send(ScreenInstruction::ScrollUp)
                    .unwrap();
            }
            [27, 91, 54, 94, 0, 0, 0, 0, 0, 0] => {
                // ctrl-PgDown
                send_screen_instructions
                    .send(ScreenInstruction::ScrollDown)
                    .unwrap();
            }
            [24, 0, 0, 0, 0, 0, 0, 0, 0, 0] => {
                // ctrl-x
                send_screen_instructions
                    .send(ScreenInstruction::CloseFocusedPane)
                    .unwrap();
                // ::std::thread::sleep(::std::time::Duration::from_millis(10));
            }
            [5, 0, 0, 0, 0, 0, 0, 0, 0, 0] => {
                // ctrl-e
                send_screen_instructions
                    .send(ScreenInstruction::ToggleActiveTerminalFullscreen)
                    .unwrap();
            }
            _ => {
                send_screen_instructions
                    .send(ScreenInstruction::ClearScroll)
                    .unwrap();
                send_screen_instructions
                    .send(ScreenInstruction::WriteCharacter(buffer))
                    .unwrap();
            }
        }
    }
}
