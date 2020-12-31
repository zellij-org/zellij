use crate::os_input_output::{daemonize, OsApi};
use crate::utils::consts::MOSAIC_IPC_PIPE;
use crate::ApiCommand;
use std::io::Read;
use std::os::unix::net::UnixListener;

pub fn start_daemon(mut _os_input: Box<dyn OsApi>) {
    #[cfg(not(test))]
    std::panic::set_hook({
        use crate::errors::handle_panic;
        Box::new(move |info| {
            handle_panic(info, None);
        })
    });

    daemonize(false, false);
    std::fs::remove_file(MOSAIC_IPC_PIPE).ok();
    let listener = UnixListener::bind(MOSAIC_IPC_PIPE).expect("could not listen on ipc socket");
    for stream in listener.incoming() {
        match stream {
            Ok(mut stream) => {
                let mut buffer = [0; 65535]; // TODO: more accurate
                let _ = stream
                    .read(&mut buffer)
                    .expect("failed to parse ipc message");
                let decoded: ApiCommand =
                    bincode::deserialize(&buffer).expect("failed to deserialize ipc message");
                match &decoded {
                    /*ApiCommand::OpenFile(file_name) => {
                        let path = PathBuf::from(file_name);
                        send_pty_instructions
                            .send(PtyInstruction::SpawnTerminal(Some(path)))
                            .unwrap();
                    }
                    ApiCommand::SplitHorizontally => {
                        send_pty_instructions
                            .send(PtyInstruction::SpawnTerminalHorizontally(None))
                            .unwrap();
                    }
                    ApiCommand::SplitVertically => {
                        send_pty_instructions
                            .send(PtyInstruction::SpawnTerminalVertically(None))
                            .unwrap();
                    }
                    ApiCommand::MoveFocus => {
                        send_screen_instructions
                            .send(ScreenInstruction::MoveFocus)
                            .unwrap();
                    }*/
                    ApiCommand::Error(_backtrace) => {
                        // Send backtrace to all clients and stop
                    }
                    ApiCommand::Exit => {
                        // Server should not exit when a client closes. It should ideally just remove the client.
                        // Server should close only when there are no active clients and no sessions to store.
                        // But we should break until we add the check for above.
                        break;
                    }
                    _ => {}
                }
            }
            Err(err) => {
                panic!("err {:?}", err);
            }
        }
    }
}
