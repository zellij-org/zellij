use crate::cli::CliArgs;
use crate::common::{
    ChannelWithContext, ClientInstruction, IpcSenderWithContext, SenderType, SenderWithContext,
    ServerInstruction,
};
use crate::errors::{ContextType, ErrorContext, OsContext, PtyContext};
use crate::os_input_output::{OsApi, OsApiInstruction};
use crate::panes::PaneId;
use crate::pty_bus::{PtyBus, PtyInstruction};
use crate::screen::ScreenInstruction;
use crate::utils::consts::ZELLIJ_IPC_PIPE;
use ipmpsc::{Receiver as IpcReceiver, SharedRingBuffer};
use std::path::PathBuf;
use std::sync::mpsc::channel;
use std::thread;

pub fn start_server(os_input: Box<dyn OsApi>, opts: CliArgs) -> (thread::JoinHandle<()>, String) {
    let (send_pty_instructions, receive_pty_instructions): ChannelWithContext<PtyInstruction> =
        channel();
    let mut send_pty_instructions = SenderWithContext::new(
        ErrorContext::new(),
        SenderType::Sender(send_pty_instructions),
    );

    #[cfg(not(test))]
    let (server_name, server_buffer) = (
        String::from(ZELLIJ_IPC_PIPE),
        SharedRingBuffer::create(ZELLIJ_IPC_PIPE, 8192).unwrap(),
    );
    #[cfg(test)]
    let (server_name, server_buffer) = SharedRingBuffer::create_temp(8192).unwrap();

    let (send_os_instructions, receive_os_instructions): ChannelWithContext<OsApiInstruction> =
        channel();
    let mut send_os_instructions = SenderWithContext::new(
        ErrorContext::new(),
        SenderType::Sender(send_os_instructions),
    );

    // Don't use default layouts in tests, but do everywhere else
    #[cfg(not(test))]
    let default_layout = Some(PathBuf::from("default"));
    #[cfg(test)]
    let default_layout = None;
    let maybe_layout = opts.layout.or(default_layout);

    let send_server_instructions = IpcSenderWithContext::new(server_buffer.clone());

    let mut pty_bus = PtyBus::new(
        receive_pty_instructions,
        os_input.clone(),
        send_server_instructions,
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
                    OsApiInstruction::SpawnTerminal(file_to_open) => {
                        os_input.spawn_terminal(file_to_open);
                    }
                    OsApiInstruction::GetTerminalSizeUsingFd(fd) => {
                        os_input.get_terminal_size_using_fd(fd);
                    }
                    OsApiInstruction::SetTerminalSizeUsingFd(fd, cols, rows) => {
                        os_input.set_terminal_size_using_fd(fd, cols, rows);
                    }
                    OsApiInstruction::SetRawMode(fd) => {
                        os_input.set_raw_mode(fd);
                    }
                    OsApiInstruction::UnsetRawMode(fd) => {
                        os_input.unset_raw_mode(fd);
                    }
                    OsApiInstruction::ReadFromTtyStdout(fd, mut buf) => {
                        let slice = buf.as_mut_slice();
                        os_input.read_from_tty_stdout(fd, slice).unwrap();
                    }
                    OsApiInstruction::WriteToTtyStdin(fd, mut buf) => {
                        let slice = buf.as_mut_slice();
                        os_input.write_to_tty_stdin(fd, slice).unwrap();
                    }
                    OsApiInstruction::TcDrain(fd) => {
                        os_input.tcdrain(fd).unwrap();
                    }
                    OsApiInstruction::Kill(pid) => {
                        os_input.kill(pid).unwrap();
                    }
                    OsApiInstruction::ReadFromStdin => {
                        os_input.read_from_stdin();
                    }
                    OsApiInstruction::GetStdoutWriter => {
                        os_input.get_stdout_writer();
                    }
                    OsApiInstruction::BoxClone => {
                        os_input.box_clone();
                    }
                }
            }
        })
        .unwrap();

    let join_handle = thread::Builder::new()
        .name("ipc_server".to_string())
        .spawn({
            let recv_server_instructions = IpcReceiver::new(server_buffer);
            // Fixme: We cannot use uninitialised sender, therefore this Vec.
            // For now, We make sure that the first message is `NewClient` so there are no out of bound panics.
            let mut send_client_instructions: Vec<IpcSenderWithContext> = Vec::with_capacity(1);
            move || loop {
                let (mut err_ctx, instruction): (ErrorContext, ServerInstruction) =
                    recv_server_instructions.recv().unwrap();
                err_ctx.add_call(ContextType::IPCServer);
                send_pty_instructions.update(err_ctx);
                if send_client_instructions.len() == 1 {
                    send_client_instructions[0].update(err_ctx);
                }

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
                        send_client_instructions[0]
                            .send(ClientInstruction::ToScreen(ScreenInstruction::MoveFocus))
                            .unwrap();
                    }
                    ServerInstruction::NewClient(buffer_path) => {
                        send_pty_instructions.send(PtyInstruction::NewTab).unwrap();
                        send_client_instructions.push(IpcSenderWithContext::new(
                            SharedRingBuffer::open(&buffer_path).unwrap(),
                        ));
                    }
                    ServerInstruction::ToPty(instr) => {
                        send_pty_instructions.send(instr).unwrap();
                    }
                    ServerInstruction::ToScreen(instr) => {
                        send_client_instructions[0]
                            .send(ClientInstruction::ToScreen(instr))
                            .unwrap();
                    }
                    ServerInstruction::OsApi(instr) => {
                        send_os_instructions.send(instr).unwrap();
                    }
                    ServerInstruction::DoneClosingPane => {
                        send_client_instructions[0]
                            .send(ClientInstruction::DoneClosingPane)
                            .unwrap();
                    }
                    ServerInstruction::ClosePluginPane(pid) => {
                        send_client_instructions[0]
                            .send(ClientInstruction::ClosePluginPane(pid))
                            .unwrap();
                    }
                    ServerInstruction::Exit => {
                        let _ = send_pty_instructions.send(PtyInstruction::Exit);
                        let _ = pty_thread.join();
                        let _ = os_thread.join();
                        let _ = send_client_instructions[0].send(ClientInstruction::Exit);
                        break;
                    }
                }
            }
        })
        .unwrap();
    (join_handle, server_name)
}
