use crate::errors::{ContextType, ErrorContext, PtyContext};
use crate::os_input_output::{daemonize, OsApi};
use crate::pty_bus::{PtyBus, PtyInstruction};
use crate::screen::ScreenInstruction;
use crate::utils::consts::MOSAIC_IPC_PIPE;
use crate::{AppInstruction, ClientId, Opt, SenderWithContext, OPENCALLS};
use ipc_channel::ipc::{channel, IpcReceiver, IpcSender};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::os::unix::net::UnixListener;
use std::path::PathBuf;
use std::thread;

#[derive(Serialize, Deserialize, Debug)]
pub enum ServerInstruction {
    OpenFile(PathBuf),
    Error(String),
    ClientExit(ClientId),
    NewClient(String),
}

pub fn start_daemon(os_input: Box<dyn OsApi>, opts: Opt) {
    std::fs::remove_file(MOSAIC_IPC_PIPE).ok();
    let listener = UnixListener::bind(MOSAIC_IPC_PIPE).expect("could not listen on ipc socket");
    #[cfg(not(test))]
    std::panic::set_hook({
        use crate::errors::handle_panic;
        Box::new(move |info| {
            handle_panic(info, None);
        })
    });

    daemonize(true, true);

    let err_ctx = OPENCALLS.with(|ctx| *ctx.borrow());
    let (send_pty_instructions, receive_pty_instructions): (
        IpcSender<(ClientId, PtyInstruction, ErrorContext)>,
        IpcReceiver<(ClientId, PtyInstruction, ErrorContext)>,
    ) = channel().unwrap();
    let send_pty_instructions = SenderWithContext::new(0, err_ctx, send_pty_instructions);

    let mut pty_bus = PtyBus::new(receive_pty_instructions, os_input.clone(), opts.debug);

    let pty_thread_handle = thread::Builder::new()
        .name("pty".to_string())
        .spawn({
            move || loop {
                let (client_id, event, mut err_ctx) = pty_bus
                    .receive_pty_instructions
                    .recv()
                    .expect("failed to receive event on channel");
                let ctx = PtyContext::from(&event);
                err_ctx.add_call(ContextType::Pty(ctx));
                if client_id != 0 && ctx != PtyContext::NewScreen {
                    pty_bus.get_screen_sender_mut(client_id).update_ctx(err_ctx);
                }
                match event {
                    PtyInstruction::NewScreen(screen_sender) => {
                        pty_bus.add_screen(client_id, screen_sender);
                    }
                    PtyInstruction::RemoveScreen(client_id) => {
                        // This message should have come from server
                        assert_eq!(client_id, 0);
                        pty_bus.remove_screen(client_id);
                    }
                    PtyInstruction::SpawnLayout(layout) => pty_bus.spawn_terminals_for_layout(
                        layout,
                        pty_bus.get_screen_sender(client_id).clone(),
                    ),
                    PtyInstruction::SpawnTerminal(file_to_open) => {
                        let screen_sender = pty_bus.get_screen_sender(client_id).clone();
                        let pid = pty_bus.spawn_terminal(file_to_open, &screen_sender);
                        screen_sender.send(ScreenInstruction::NewPane(pid)).unwrap();
                    }
                    PtyInstruction::SpawnTerminalVertically(file_to_open) => {
                        let screen_sender = pty_bus.get_screen_sender(client_id).clone();
                        let pid = pty_bus.spawn_terminal(file_to_open, &screen_sender);
                        screen_sender
                            .send(ScreenInstruction::VerticalSplit(pid))
                            .unwrap();
                    }
                    PtyInstruction::SpawnTerminalHorizontally(file_to_open) => {
                        let screen_sender = pty_bus.get_screen_sender(client_id).clone();
                        let pid = pty_bus.spawn_terminal(file_to_open, &screen_sender);
                        screen_sender
                            .send(ScreenInstruction::HorizontalSplit(pid))
                            .unwrap();
                    }
                    PtyInstruction::NewTab => {
                        let screen_sender = pty_bus.get_screen_sender(client_id).clone();
                        let pid = pty_bus.spawn_terminal(None, &screen_sender);
                        screen_sender.send(ScreenInstruction::NewTab(pid)).unwrap();
                    }
                    PtyInstruction::ClosePane(id) => pty_bus.close_pane(id),
                    PtyInstruction::CloseTab(ids) => pty_bus.close_tab(ids),
                    PtyInstruction::Quit => break,
                }
            }
        })
        .unwrap();

    let mut clients = BTreeMap::new();
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let decoded: ServerInstruction =
                    bincode::deserialize_from(stream).expect("failed to deserialize ipc message");
                match decoded {
                    ServerInstruction::NewClient(sender_token) => {
                        //file.write_all("NewClient\n".as_bytes()).unwrap();
                        let client_id = if let Some(id) = clients.keys().last() {
                            *id + 1
                        } else {
                            1
                        };
                        let client_sender = SenderWithContext::new(
                            client_id,
                            ErrorContext::new(),
                            IpcSender::connect(sender_token).unwrap(),
                        );
                        client_sender
                            .send(AppInstruction::InitClient {
                                client_id,
                                pty_sender: send_pty_instructions.clone(),
                                app_sender: client_sender.clone(),
                            })
                            .unwrap();
                        clients.insert(client_id, client_sender);
                        //file.write_all("Added NewClient\n".as_bytes()).unwrap();
                    }
                    ServerInstruction::Error(backtrace) => {
                        // Send backtrace to all clients and stop
                        clients.values().for_each(|c| {
                            c.send(AppInstruction::Error(backtrace.clone())).unwrap()
                        });
                        pty_thread_handle.join().unwrap();
                        std::process::exit(1);
                    }
                    ServerInstruction::ClientExit(client_id) => {
                        send_pty_instructions
                            .send(PtyInstruction::RemoveScreen(client_id))
                            .unwrap();
                        clients.remove(&client_id).unwrap();
                        if clients.len() == 0 {
                            send_pty_instructions.send(PtyInstruction::Quit).unwrap();
                            break;
                        }
                    }
                    ServerInstruction::OpenFile(_) => unimplemented!(),
                }
            }
            Err(err) => {
                panic!("err {:?}", err);
            }
        }
    }
    pty_thread_handle.join().unwrap();
}
