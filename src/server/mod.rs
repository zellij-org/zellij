use crate::client::ClientInstruction;
use crate::common::{ChannelWithContext, SenderType, SenderWithContext};
use crate::errors::{ContextType, ErrorContext, OsContext, PtyContext, ServerContext};
use crate::os_input_output::{ServerOsApi, ServerOsApiInstruction};
use crate::panes::PaneId;
use crate::pty_bus::{PtyBus, PtyInstruction};
use crate::screen::ScreenInstruction;
use crate::{cli::CliArgs, common::pty_bus::VteEvent};
use serde::{Deserialize, Serialize};
use std::os::unix::io::RawFd;
use std::path::PathBuf;
use std::sync::mpsc::channel;
use std::thread;
use zellij_tile::prelude::InputMode;

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
impl ServerInstruction {
    // ToPty
    pub fn spawn_terminal(path: Option<PathBuf>) -> Self {
        Self::ToPty(PtyInstruction::SpawnTerminal(path))
    }
    pub fn spawn_terminal_vertically(path: Option<PathBuf>) -> Self {
        Self::ToPty(PtyInstruction::SpawnTerminalVertically(path))
    }
    pub fn spawn_terminal_horizontally(path: Option<PathBuf>) -> Self {
        Self::ToPty(PtyInstruction::SpawnTerminalHorizontally(path))
    }
    pub fn pty_new_tab() -> Self {
        Self::ToPty(PtyInstruction::NewTab)
    }
    pub fn pty_close_pane(id: PaneId) -> Self {
        Self::ToPty(PtyInstruction::ClosePane(id))
    }
    pub fn pty_close_tab(ids: Vec<PaneId>) -> Self {
        Self::ToPty(PtyInstruction::CloseTab(ids))
    }
    pub fn pty_exit() -> Self {
        Self::ToPty(PtyInstruction::Exit)
    }

    // ToScreen
    pub fn render() -> Self {
        Self::ToScreen(ScreenInstruction::Render)
    }
    pub fn new_pane(id: PaneId) -> Self {
        Self::ToScreen(ScreenInstruction::NewPane(id))
    }
    pub fn horizontal_split(id: PaneId) -> Self {
        Self::ToScreen(ScreenInstruction::HorizontalSplit(id))
    }
    pub fn vertical_split(id: PaneId) -> Self {
        Self::ToScreen(ScreenInstruction::VerticalSplit(id))
    }
    pub fn write_character(chars: Vec<u8>) -> Self {
        Self::ToScreen(ScreenInstruction::WriteCharacter(chars))
    }
    pub fn resize_left() -> Self {
        Self::ToScreen(ScreenInstruction::ResizeLeft)
    }
    pub fn resize_right() -> Self {
        Self::ToScreen(ScreenInstruction::ResizeRight)
    }
    pub fn resize_down() -> Self {
        Self::ToScreen(ScreenInstruction::ResizeDown)
    }
    pub fn resize_up() -> Self {
        Self::ToScreen(ScreenInstruction::ResizeUp)
    }
    pub fn move_focus() -> Self {
        Self::ToScreen(ScreenInstruction::MoveFocus)
    }
    pub fn move_focus_left() -> Self {
        Self::ToScreen(ScreenInstruction::MoveFocusLeft)
    }
    pub fn move_focus_right() -> Self {
        Self::ToScreen(ScreenInstruction::MoveFocusRight)
    }
    pub fn move_focus_down() -> Self {
        Self::ToScreen(ScreenInstruction::MoveFocusDown)
    }
    pub fn move_focus_up() -> Self {
        Self::ToScreen(ScreenInstruction::MoveFocusUp)
    }
    pub fn screen_exit() -> Self {
        Self::ToScreen(ScreenInstruction::Exit)
    }
    pub fn scroll_up() -> Self {
        Self::ToScreen(ScreenInstruction::ScrollUp)
    }
    pub fn scroll_down() -> Self {
        Self::ToScreen(ScreenInstruction::ScrollDown)
    }
    pub fn clear_scroll() -> Self {
        Self::ToScreen(ScreenInstruction::ClearScroll)
    }
    pub fn close_focused_pane() -> Self {
        Self::ToScreen(ScreenInstruction::CloseFocusedPane)
    }
    pub fn toggle_active_terminal_fullscreen() -> Self {
        Self::ToScreen(ScreenInstruction::ToggleActiveTerminalFullscreen)
    }
    pub fn set_selectable(pane_id: PaneId, value: bool) -> Self {
        Self::ToScreen(ScreenInstruction::SetSelectable(pane_id, value))
    }
    pub fn set_max_height(pane_id: PaneId, max_height: usize) -> Self {
        Self::ToScreen(ScreenInstruction::SetMaxHeight(pane_id, max_height))
    }
    pub fn set_invisible_borders(pane_id: PaneId, value: bool) -> Self {
        Self::ToScreen(ScreenInstruction::SetInvisibleBorders(pane_id, value))
    }
    pub fn screen_close_pane(pane_id: PaneId) -> Self {
        Self::ToScreen(ScreenInstruction::ClosePane(pane_id))
    }
    pub fn apply_layout(layout: (PathBuf, Vec<RawFd>)) -> Self {
        Self::ToScreen(ScreenInstruction::ApplyLayout(layout))
    }
    pub fn screen_new_tab(fd: RawFd) -> Self {
        Self::ToScreen(ScreenInstruction::NewTab(fd))
    }
    pub fn switch_tab_prev() -> Self {
        Self::ToScreen(ScreenInstruction::SwitchTabPrev)
    }
    pub fn switch_tab_next() -> Self {
        Self::ToScreen(ScreenInstruction::SwitchTabPrev)
    }
    pub fn screen_close_tab() -> Self {
        Self::ToScreen(ScreenInstruction::CloseTab)
    }
    pub fn go_to_tab(tab_id: u32) -> Self {
        Self::ToScreen(ScreenInstruction::GoToTab(tab_id))
    }
    pub fn update_tab_name(tab_ids: Vec<u8>) -> Self {
        Self::ToScreen(ScreenInstruction::UpdateTabName(tab_ids))
    }
    pub fn change_input_mode(input_mode: InputMode) -> Self {
        Self::ToScreen(ScreenInstruction::ChangeInputMode(input_mode))
    }
    pub fn pty(fd: RawFd, event: VteEvent) -> Self {
        Self::ToScreen(ScreenInstruction::Pty(fd, event))
    }

    // OsApi
    pub fn set_terminal_size_using_fd(fd: RawFd, cols: u16, rows: u16) -> Self {
        Self::OsApi(ServerOsApiInstruction::SetTerminalSizeUsingFd(
            fd, cols, rows,
        ))
    }
    pub fn write_to_tty_stdin(fd: RawFd, buf: Vec<u8>) -> Self {
        Self::OsApi(ServerOsApiInstruction::WriteToTtyStdin(fd, buf))
    }
    pub fn tc_drain(fd: RawFd) -> Self {
        Self::OsApi(ServerOsApiInstruction::TcDrain(fd))
    }
    pub fn os_exit() -> Self {
        Self::OsApi(ServerOsApiInstruction::Exit)
    }
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
                        .send(ServerInstruction::new_pane(PaneId::Terminal(pid)))
                        .unwrap();
                }
                PtyInstruction::SpawnTerminalVertically(file_to_open) => {
                    let pid = pty_bus.spawn_terminal(file_to_open);
                    pty_bus
                        .send_server_instructions
                        .send(ServerInstruction::vertical_split(PaneId::Terminal(pid)))
                        .unwrap();
                }
                PtyInstruction::SpawnTerminalHorizontally(file_to_open) => {
                    let pid = pty_bus.spawn_terminal(file_to_open);
                    pty_bus
                        .send_server_instructions
                        .send(ServerInstruction::horizontal_split(PaneId::Terminal(pid)))
                        .unwrap();
                }
                PtyInstruction::NewTab => {
                    if let Some(layout) = maybe_layout.clone() {
                        pty_bus.spawn_terminals_for_layout(layout);
                    } else {
                        let pid = pty_bus.spawn_terminal(None);
                        pty_bus
                            .send_server_instructions
                            .send(ServerInstruction::screen_new_tab(pid))
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
            let mut send_os_instructions = send_os_instructions.clone();
            let mut send_pty_instructions = send_pty_instructions.clone();
            move || loop {
                let (instruction, err_ctx) = os_input.server_recv();
                send_server_instructions.update(err_ctx);
                send_pty_instructions.update(err_ctx);
                send_os_instructions.update(err_ctx);
                match instruction {
                    ServerInstruction::Exit => break,
                    ServerInstruction::ToPty(instruction) => {
                        send_pty_instructions.send(instruction).unwrap();
                    }
                    ServerInstruction::OsApi(instruction) => {
                        send_os_instructions.send(instruction).unwrap();
                    }
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
                    ServerInstruction::ToScreen(instr) => {
                        os_input.send_to_client(ClientInstruction::ToScreen(instr));
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
