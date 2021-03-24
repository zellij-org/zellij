use crate::cli::CliArgs;
use crate::common::{ChannelWithContext, ClientInstruction, SenderType, SenderWithContext};
use crate::errors::{ContextType, ErrorContext, OsContext, PtyContext, ServerContext};
use crate::os_input_output::{ServerOsApi, ServerOsApiInstruction};
use crate::panes::PaneId;
use crate::pty_bus::{PtyBus, PtyInstruction};
use crate::screen::ScreenInstruction;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::mpsc::channel;
use std::thread;

/// Instructions related to server-side application including the
/// ones sent by client to server
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ServerInstruction {
    OpenFile(PathBuf),
    SplitHorizontally,
    SplitVertically,
    MoveFocus,
    NewClient(String),
    ToPty(PtyInstruction),
    ToScreen(ScreenInstruction),
    OsApi(ServerOsApiInstruction),
    DoneClosingPane,
    ClosePluginPane(u32),
    ClientExit,
    Exit,
}

pub fn start_server(mut os_input: Box<dyn ServerOsApi>, opts: CliArgs) -> thread::JoinHandle<()> {
    let (send_pty_instructions, receive_pty_instructions): ChannelWithContext<PtyInstruction> =
        channel();
    let mut send_pty_instructions = SenderWithContext::new(
        ErrorContext::new(),
        SenderType::Sender(send_pty_instructions),
    );

    let (send_os_instructions, receive_os_instructions): ChannelWithContext<
        ServerOsApiInstruction,
    > = channel();
    let mut send_os_instructions = SenderWithContext::new(
        ErrorContext::new(),
        SenderType::Sender(send_os_instructions),
    );

    let (send_server_instructions, receive_server_instructions): ChannelWithContext<
        ServerInstruction,
    > = channel();
    let mut send_server_instructions = SenderWithContext::new(
        ErrorContext::new(),
        SenderType::Sender(send_server_instructions),
    );

    // Don't use default layouts in tests, but do everywhere else
    #[cfg(not(test))]
    let default_layout = Some(PathBuf::from("default"));
    #[cfg(test)]
    let default_layout = None;
    let maybe_layout = opts.layout.or(default_layout);

    let mut pty_bus = PtyBus::new(
        receive_pty_instructions,
        os_input.clone(),
        send_server_instructions.clone(),
        opts.debug,
    );

    let pty_thread = thread::Builder::new()
        .name("pty".to_string())
        .spawn(move || loop {
            let (event, mut err_ctx) = pty_bus
                .receive_pty_instructions
                .recv()
                .expect("failed to receive event on channel");
            err_ctx.add_call(ContextType::Pty(PtyContext::from(&event)));
            match event {
                PtyInstruction::SpawnTerminal(file_to_open) => {
                    let pid = pty_bus.spawn_terminal(file_to_open);
                    pty_bus
                        .send_server_instructions
                        .send(ServerInstruction::ToScreen(ScreenInstruction::NewPane(
                            PaneId::Terminal(pid),
                        )))
                        .unwrap();
                }
                PtyInstruction::SpawnTerminalVertically(file_to_open) => {
                    let pid = pty_bus.spawn_terminal(file_to_open);
                    pty_bus
                        .send_server_instructions
                        .send(ServerInstruction::ToScreen(
                            ScreenInstruction::VerticalSplit(PaneId::Terminal(pid)),
                        ))
                        .unwrap();
                }
                PtyInstruction::SpawnTerminalHorizontally(file_to_open) => {
                    let pid = pty_bus.spawn_terminal(file_to_open);
                    pty_bus
                        .send_server_instructions
                        .send(ServerInstruction::ToScreen(
                            ScreenInstruction::HorizontalSplit(PaneId::Terminal(pid)),
                        ))
                        .unwrap();
                }
                PtyInstruction::NewTab => {
                    if let Some(layout) = maybe_layout.clone() {
                        pty_bus.spawn_terminals_for_layout(layout);
                    } else {
                        let pid = pty_bus.spawn_terminal(None);
                        pty_bus
                            .send_server_instructions
                            .send(ServerInstruction::ToScreen(ScreenInstruction::NewTab(pid)))
                            .unwrap();
                    }
                }
                PtyInstruction::ClosePane(id) => {
                    pty_bus.close_pane(id);
                    pty_bus
                        .send_server_instructions
                        .send(ServerInstruction::DoneClosingPane)
                        .unwrap();
                }
                PtyInstruction::CloseTab(ids) => {
                    pty_bus.close_tab(ids);
                    pty_bus
                        .send_server_instructions
                        .send(ServerInstruction::DoneClosingPane)
                        .unwrap();
                }
                PtyInstruction::Exit => {
                    break;
                }
            }
        })
        .unwrap();

    let os_thread = thread::Builder::new()
        .name("os".to_string())
        .spawn({
            let mut os_input = os_input.clone();
            move || loop {
                let (event, mut err_ctx) = receive_os_instructions
                    .recv()
                    .expect("failed to receive an event on the channel");
                err_ctx.add_call(ContextType::Os(OsContext::from(&event)));
                match event {
                    ServerOsApiInstruction::SetTerminalSizeUsingFd(fd, cols, rows) => {
                        os_input.set_terminal_size_using_fd(fd, cols, rows);
                    }
                    ServerOsApiInstruction::WriteToTtyStdin(fd, mut buf) => {
                        let slice = buf.as_mut_slice();
                        os_input.write_to_tty_stdin(fd, slice).unwrap();
                    }
                    ServerOsApiInstruction::TcDrain(fd) => {
                        os_input.tcdrain(fd).unwrap();
                    }
                    ServerOsApiInstruction::Exit => break,
                }
            }
        })
        .unwrap();

    let router_thread = thread::Builder::new()
        .name("server_router".to_string())
        .spawn({
            let os_input = os_input.clone();
            move || loop {
                let (instruction, err_ctx) = os_input.server_recv();
                send_server_instructions.update(err_ctx);
                match instruction {
                    ServerInstruction::Exit => break,
                    _ => {
                        send_server_instructions.send(instruction).unwrap();
                    }
                }
            }
        })
        .unwrap();

    thread::Builder::new()
        .name("ipc_server".to_string())
        .spawn({
            move || loop {
                let (instruction, mut err_ctx) = receive_server_instructions.recv().unwrap();
                err_ctx.add_call(ContextType::IPCServer(ServerContext::from(&instruction)));
                send_pty_instructions.update(err_ctx);
                send_os_instructions.update(err_ctx);
                os_input.update_senders(err_ctx);
                match instruction {
                    ServerInstruction::OpenFile(file_name) => {
                        let path = PathBuf::from(file_name);
                        send_pty_instructions
                            .send(PtyInstruction::SpawnTerminal(Some(path)))
                            .unwrap();
                    }
                    ServerInstruction::SplitHorizontally => {
                        send_pty_instructions
                            .send(PtyInstruction::SpawnTerminalHorizontally(None))
                            .unwrap();
                    }
                    ServerInstruction::SplitVertically => {
                        send_pty_instructions
                            .send(PtyInstruction::SpawnTerminalVertically(None))
                            .unwrap();
                    }
                    ServerInstruction::MoveFocus => {
                        os_input.send_to_client(ClientInstruction::ToScreen(
                            ScreenInstruction::MoveFocus,
                        ));
                    }
                    ServerInstruction::NewClient(buffer_path) => {
                        send_pty_instructions.send(PtyInstruction::NewTab).unwrap();
                        os_input.add_client_sender(buffer_path);
                    }
                    ServerInstruction::ToPty(instr) => {
                        send_pty_instructions.send(instr).unwrap();
                    }
                    ServerInstruction::ToScreen(instr) => {
                        os_input.send_to_client(ClientInstruction::ToScreen(instr));
                    }
                    ServerInstruction::OsApi(instr) => {
                        send_os_instructions.send(instr).unwrap();
                    }
                    ServerInstruction::DoneClosingPane => {
                        os_input.send_to_client(ClientInstruction::DoneClosingPane);
                    }
                    ServerInstruction::ClosePluginPane(pid) => {
                        os_input.send_to_client(ClientInstruction::ClosePluginPane(pid));
                    }
                    ServerInstruction::ClientExit => {
                        let _ = send_pty_instructions.send(PtyInstruction::Exit);
                        let _ = send_os_instructions.send(ServerOsApiInstruction::Exit);
                        os_input.server_exit();
                        let _ = pty_thread.join();
                        let _ = os_thread.join();
                        let _ = router_thread.join();
                        let _ = os_input.send_to_client(ClientInstruction::Exit);
                        break;
                    }
                    _ => {}
                }
            }
        })
        .unwrap()
}
