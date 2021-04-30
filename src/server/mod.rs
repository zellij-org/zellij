use directories_next::ProjectDirs;
use interprocess::local_socket::LocalSocketListener;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::mpsc::channel;
use std::thread;
use std::{collections::HashMap, fs};
use std::{
    collections::HashSet,
    str::FromStr,
    sync::{Arc, Mutex, RwLock},
};
use wasmer::{ChainableNamedResolver, Instance, Module, Store, Value};
use wasmer_wasi::{Pipe, WasiState};
use zellij_tile::data::{Event, EventType, ModeInfo};

use crate::cli::CliArgs;
use crate::client::ClientInstruction;
use crate::common::ZELLIJ_IPC_PIPE;
use crate::common::{
    errors::{ContextType, PluginContext, PtyContext, ScreenContext, ServerContext},
    input::actions::{Action, Direction},
    input::handler::get_mode_info,
    os_input_output::ServerOsApi,
    pty_bus::{PtyBus, PtyInstruction},
    screen::{Screen, ScreenInstruction},
    wasm_vm::{wasi_stdout, wasi_write_string, zellij_imports, PluginEnv, PluginInstruction},
    ChannelWithContext, SenderType, SenderWithContext,
};
use crate::layout::Layout;
use crate::panes::PaneId;
use crate::panes::PositionAndSize;

/// Instructions related to server-side application including the
/// ones sent by client to server
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ServerInstruction {
    TerminalResize(PositionAndSize),
    NewClient(PositionAndSize),
    Action(Action),
    Render(Option<String>),
    UnblockInputThread,
    ClientExit,
}

struct SessionMetaData {
    pub send_pty_instructions: SenderWithContext<PtyInstruction>,
    pub send_screen_instructions: SenderWithContext<ScreenInstruction>,
    pub send_plugin_instructions: SenderWithContext<PluginInstruction>,
    screen_thread: Option<thread::JoinHandle<()>>,
    pty_thread: Option<thread::JoinHandle<()>>,
    wasm_thread: Option<thread::JoinHandle<()>>,
}

impl Drop for SessionMetaData {
    fn drop(&mut self) {
        let _ = self.send_pty_instructions.send(PtyInstruction::Exit);
        let _ = self.send_screen_instructions.send(ScreenInstruction::Exit);
        let _ = self.send_plugin_instructions.send(PluginInstruction::Exit);
        let _ = self.screen_thread.take().unwrap().join();
        let _ = self.pty_thread.take().unwrap().join();
        let _ = self.wasm_thread.take().unwrap().join();
    }
}

pub fn start_server(os_input: Box<dyn ServerOsApi>, opts: CliArgs) -> thread::JoinHandle<()> {
    let (send_server_instructions, receive_server_instructions): ChannelWithContext<
        ServerInstruction,
    > = channel();
    let send_server_instructions =
        SenderWithContext::new(SenderType::Sender(send_server_instructions));
    let sessions: Arc<RwLock<Option<SessionMetaData>>> = Arc::new(RwLock::new(None));

    #[cfg(test)]
    handle_client(
        sessions.clone(),
        os_input.clone(),
        send_server_instructions.clone(),
    );
    #[cfg(not(test))]
    let _ = thread::Builder::new()
        .name("server_listener".to_string())
        .spawn({
            let os_input = os_input.clone();
            let sessions = sessions.clone();
            let send_server_instructions = send_server_instructions.clone();
            move || {
                drop(std::fs::remove_file(ZELLIJ_IPC_PIPE.clone()));
                let listener = LocalSocketListener::bind(ZELLIJ_IPC_PIPE.clone()).unwrap();
                for stream in listener.incoming() {
                    match stream {
                        Ok(stream) => {
                            let mut os_input = os_input.clone();
                            os_input.update_receiver(stream);
                            let sessions = sessions.clone();
                            let send_server_instructions = send_server_instructions.clone();
                            handle_client(sessions, os_input, send_server_instructions);
                        }
                        Err(err) => {
                            panic!("err {:?}", err);
                        }
                    }
                }
            }
        });

    thread::Builder::new()
        .name("server_thread".to_string())
        .spawn({
            move || loop {
                let (instruction, mut err_ctx) = receive_server_instructions.recv().unwrap();
                err_ctx.add_call(ContextType::IPCServer(ServerContext::from(&instruction)));
                match instruction {
                    ServerInstruction::NewClient(full_screen_ws) => {
                        let session_data = init_session(
                            os_input.clone(),
                            opts.clone(),
                            send_server_instructions.clone(),
                            full_screen_ws,
                        );
                        *sessions.write().unwrap() = Some(session_data);
                        sessions
                            .read()
                            .unwrap()
                            .as_ref()
                            .unwrap()
                            .send_pty_instructions
                            .send(PtyInstruction::NewTab)
                            .unwrap();
                    }
                    ServerInstruction::UnblockInputThread => {
                        os_input.send_to_client(ClientInstruction::UnblockInputThread);
                    }
                    ServerInstruction::ClientExit => {
                        *sessions.write().unwrap() = None;
                        os_input.send_to_client(ClientInstruction::Exit);
                        drop(std::fs::remove_file(ZELLIJ_IPC_PIPE.clone()));
                        break;
                    }
                    ServerInstruction::Render(output) => {
                        os_input.send_to_client(ClientInstruction::Render(output))
                    }
                    _ => panic!("Received unexpected instruction."),
                }
            }
        })
        .unwrap()
}

fn handle_client(
    sessions: Arc<RwLock<Option<SessionMetaData>>>,
    mut os_input: Box<dyn ServerOsApi>,
    send_server_instructions: SenderWithContext<ServerInstruction>,
) {
    thread::Builder::new()
        .name("server_router".to_string())
        .spawn(move || loop {
            let (instruction, mut err_ctx) = os_input.recv_from_client();
            err_ctx.add_call(ContextType::IPCServer(ServerContext::from(&instruction)));
            let rlocked_sessions = sessions.read().unwrap();
            match instruction {
                ServerInstruction::ClientExit => {
                    send_server_instructions.send(instruction).unwrap();
                    break;
                }
                ServerInstruction::Action(action) => {
                    route_action(action, rlocked_sessions.as_ref().unwrap());
                }
                ServerInstruction::TerminalResize(new_size) => {
                    rlocked_sessions
                        .as_ref()
                        .unwrap()
                        .send_screen_instructions
                        .send(ScreenInstruction::TerminalResize(new_size))
                        .unwrap();
                }
                ServerInstruction::NewClient(_) => {
                    os_input.add_client_sender();
                    send_server_instructions.send(instruction).unwrap();
                }
                _ => {
                    send_server_instructions.send(instruction).unwrap();
                }
            }
        })
        .unwrap();
}

fn init_session(
    os_input: Box<dyn ServerOsApi>,
    opts: CliArgs,
    send_server_instructions: SenderWithContext<ServerInstruction>,
    full_screen_ws: PositionAndSize,
) -> SessionMetaData {
    let (send_screen_instructions, receive_screen_instructions): ChannelWithContext<
        ScreenInstruction,
    > = channel();
    let send_screen_instructions =
        SenderWithContext::new(SenderType::Sender(send_screen_instructions));

    let (send_plugin_instructions, receive_plugin_instructions): ChannelWithContext<
        PluginInstruction,
    > = channel();
    let send_plugin_instructions =
        SenderWithContext::new(SenderType::Sender(send_plugin_instructions));
    let (send_pty_instructions, receive_pty_instructions): ChannelWithContext<PtyInstruction> =
        channel();
    let send_pty_instructions = SenderWithContext::new(SenderType::Sender(send_pty_instructions));

    // Don't use default layouts in tests, but do everywhere else
    #[cfg(not(test))]
    let default_layout = Some(PathBuf::from("default"));
    #[cfg(test)]
    let default_layout = None;
    let maybe_layout = opts.layout.or(default_layout);

    let mut pty_bus = PtyBus::new(
        receive_pty_instructions,
        os_input.clone(),
        send_screen_instructions.clone(),
        send_plugin_instructions.clone(),
        opts.debug,
    );

    let pty_thread = thread::Builder::new()
        .name("pty".to_string())
        .spawn({
            let send_server_instructions = send_server_instructions.clone();
            move || loop {
                let (event, mut err_ctx) = pty_bus
                    .receive_pty_instructions
                    .recv()
                    .expect("failed to receive event on channel");
                err_ctx.add_call(ContextType::Pty(PtyContext::from(&event)));
                match event {
                    PtyInstruction::SpawnTerminal(file_to_open) => {
                        let pid = pty_bus.spawn_terminal(file_to_open);
                        pty_bus
                            .send_screen_instructions
                            .send(ScreenInstruction::NewPane(PaneId::Terminal(pid)))
                            .unwrap();
                    }
                    PtyInstruction::SpawnTerminalVertically(file_to_open) => {
                        let pid = pty_bus.spawn_terminal(file_to_open);
                        pty_bus
                            .send_screen_instructions
                            .send(ScreenInstruction::VerticalSplit(PaneId::Terminal(pid)))
                            .unwrap();
                    }
                    PtyInstruction::SpawnTerminalHorizontally(file_to_open) => {
                        let pid = pty_bus.spawn_terminal(file_to_open);
                        pty_bus
                            .send_screen_instructions
                            .send(ScreenInstruction::HorizontalSplit(PaneId::Terminal(pid)))
                            .unwrap();
                    }
                    PtyInstruction::NewTab => {
                        if let Some(layout) = maybe_layout.clone() {
                            pty_bus.spawn_terminals_for_layout(layout);
                        } else {
                            let pid = pty_bus.spawn_terminal(None);
                            pty_bus
                                .send_screen_instructions
                                .send(ScreenInstruction::NewTab(pid))
                                .unwrap();
                        }
                    }
                    PtyInstruction::ClosePane(id) => {
                        pty_bus.close_pane(id);
                        send_server_instructions
                            .send(ServerInstruction::UnblockInputThread)
                            .unwrap();
                    }
                    PtyInstruction::CloseTab(ids) => {
                        pty_bus.close_tab(ids);
                        send_server_instructions
                            .send(ServerInstruction::UnblockInputThread)
                            .unwrap();
                    }
                    PtyInstruction::Exit => {
                        break;
                    }
                }
            }
        })
        .unwrap();

    let screen_thread = thread::Builder::new()
        .name("screen".to_string())
        .spawn({
            let os_input = os_input.clone();
            let send_plugin_instructions = send_plugin_instructions.clone();
            let send_pty_instructions = send_pty_instructions.clone();
            let send_server_instructions = send_server_instructions;
            let max_panes = opts.max_panes;

            move || {
                let mut screen = Screen::new(
                    receive_screen_instructions,
                    send_plugin_instructions,
                    send_pty_instructions,
                    send_server_instructions,
                    &full_screen_ws,
                    os_input,
                    max_panes,
                    ModeInfo::default(),
                );
                loop {
                    let (event, mut err_ctx) = screen
                        .receiver
                        .recv()
                        .expect("failed to receive event on channel");
                    err_ctx.add_call(ContextType::Screen(ScreenContext::from(&event)));
                    match event {
                        ScreenInstruction::PtyBytes(pid, vte_bytes) => {
                            let active_tab = screen.get_active_tab_mut().unwrap();
                            if active_tab.has_terminal_pid(pid) {
                                // it's most likely that this event is directed at the active tab
                                // look there first
                                active_tab.handle_pty_bytes(pid, vte_bytes);
                            } else {
                                // if this event wasn't directed at the active tab, start looking
                                // in other tabs
                                let all_tabs = screen.get_tabs_mut();
                                for tab in all_tabs.values_mut() {
                                    if tab.has_terminal_pid(pid) {
                                        tab.handle_pty_bytes(pid, vte_bytes);
                                        break;
                                    }
                                }
                            }
                        }
                        ScreenInstruction::Render => {
                            screen.render();
                        }
                        ScreenInstruction::NewPane(pid) => {
                            screen.get_active_tab_mut().unwrap().new_pane(pid);
                            screen
                                .send_server_instructions
                                .send(ServerInstruction::UnblockInputThread)
                                .unwrap();
                        }
                        ScreenInstruction::HorizontalSplit(pid) => {
                            screen.get_active_tab_mut().unwrap().horizontal_split(pid);
                            screen
                                .send_server_instructions
                                .send(ServerInstruction::UnblockInputThread)
                                .unwrap();
                        }
                        ScreenInstruction::VerticalSplit(pid) => {
                            screen.get_active_tab_mut().unwrap().vertical_split(pid);
                            screen
                                .send_server_instructions
                                .send(ServerInstruction::UnblockInputThread)
                                .unwrap();
                        }
                        ScreenInstruction::WriteCharacter(bytes) => {
                            screen
                                .get_active_tab_mut()
                                .unwrap()
                                .write_to_active_terminal(bytes);
                        }
                        ScreenInstruction::ResizeLeft => {
                            screen.get_active_tab_mut().unwrap().resize_left();
                        }
                        ScreenInstruction::ResizeRight => {
                            screen.get_active_tab_mut().unwrap().resize_right();
                        }
                        ScreenInstruction::ResizeDown => {
                            screen.get_active_tab_mut().unwrap().resize_down();
                        }
                        ScreenInstruction::ResizeUp => {
                            screen.get_active_tab_mut().unwrap().resize_up();
                        }
                        ScreenInstruction::SwitchFocus => {
                            screen.get_active_tab_mut().unwrap().move_focus();
                        }
                        ScreenInstruction::FocusNextPane => {
                            screen.get_active_tab_mut().unwrap().focus_next_pane();
                        }
                        ScreenInstruction::FocusPreviousPane => {
                            screen.get_active_tab_mut().unwrap().focus_previous_pane();
                        }
                        ScreenInstruction::MoveFocusLeft => {
                            screen.get_active_tab_mut().unwrap().move_focus_left();
                        }
                        ScreenInstruction::MoveFocusDown => {
                            screen.get_active_tab_mut().unwrap().move_focus_down();
                        }
                        ScreenInstruction::MoveFocusRight => {
                            screen.get_active_tab_mut().unwrap().move_focus_right();
                        }
                        ScreenInstruction::MoveFocusUp => {
                            screen.get_active_tab_mut().unwrap().move_focus_up();
                        }
                        ScreenInstruction::ScrollUp => {
                            screen
                                .get_active_tab_mut()
                                .unwrap()
                                .scroll_active_terminal_up();
                        }
                        ScreenInstruction::ScrollDown => {
                            screen
                                .get_active_tab_mut()
                                .unwrap()
                                .scroll_active_terminal_down();
                        }
                        ScreenInstruction::ClearScroll => {
                            screen
                                .get_active_tab_mut()
                                .unwrap()
                                .clear_active_terminal_scroll();
                        }
                        ScreenInstruction::CloseFocusedPane => {
                            screen.get_active_tab_mut().unwrap().close_focused_pane();
                            screen.render();
                        }
                        ScreenInstruction::SetSelectable(id, selectable) => {
                            screen
                                .get_active_tab_mut()
                                .unwrap()
                                .set_pane_selectable(id, selectable);
                        }
                        ScreenInstruction::SetMaxHeight(id, max_height) => {
                            screen
                                .get_active_tab_mut()
                                .unwrap()
                                .set_pane_max_height(id, max_height);
                        }
                        ScreenInstruction::SetInvisibleBorders(id, invisible_borders) => {
                            screen
                                .get_active_tab_mut()
                                .unwrap()
                                .set_pane_invisible_borders(id, invisible_borders);
                            screen.render();
                        }
                        ScreenInstruction::ClosePane(id) => {
                            screen.get_active_tab_mut().unwrap().close_pane(id);
                            screen.render();
                        }
                        ScreenInstruction::ToggleActiveTerminalFullscreen => {
                            screen
                                .get_active_tab_mut()
                                .unwrap()
                                .toggle_active_pane_fullscreen();
                        }
                        ScreenInstruction::NewTab(pane_id) => {
                            screen.new_tab(pane_id);
                            screen
                                .send_server_instructions
                                .send(ServerInstruction::UnblockInputThread)
                                .unwrap();
                        }
                        ScreenInstruction::SwitchTabNext => {
                            screen.switch_tab_next();
                            screen
                                .send_server_instructions
                                .send(ServerInstruction::UnblockInputThread)
                                .unwrap();
                        }
                        ScreenInstruction::SwitchTabPrev => {
                            screen.switch_tab_prev();
                            screen
                                .send_server_instructions
                                .send(ServerInstruction::UnblockInputThread)
                                .unwrap();
                        }
                        ScreenInstruction::CloseTab => {
                            screen.close_tab();
                            screen
                                .send_server_instructions
                                .send(ServerInstruction::UnblockInputThread)
                                .unwrap();
                        }
                        ScreenInstruction::ApplyLayout(layout, new_pane_pids) => {
                            screen.apply_layout(Layout::new(layout), new_pane_pids);
                            screen
                                .send_server_instructions
                                .send(ServerInstruction::UnblockInputThread)
                                .unwrap();
                        }
                        ScreenInstruction::GoToTab(tab_index) => {
                            screen.go_to_tab(tab_index as usize);
                            screen
                                .send_server_instructions
                                .send(ServerInstruction::UnblockInputThread)
                                .unwrap();
                        }
                        ScreenInstruction::UpdateTabName(c) => {
                            screen.update_active_tab_name(c);
                            screen
                                .send_server_instructions
                                .send(ServerInstruction::UnblockInputThread)
                                .unwrap();
                        }
                        ScreenInstruction::ChangeMode(mode_info) => {
                            screen.change_mode(mode_info);
                        }
                        ScreenInstruction::TerminalResize(new_size) => {
                            screen.resize_to_screen(new_size);
                        }
                        ScreenInstruction::Exit => {
                            break;
                        }
                    }
                }
            }
        })
        .unwrap();

    let wasm_thread = thread::Builder::new()
        .name("wasm".to_string())
        .spawn({
            let send_screen_instructions = send_screen_instructions.clone();
            let send_pty_instructions = send_pty_instructions.clone();

            let store = Store::default();
            let mut plugin_id = 0;
            let mut plugin_map = HashMap::new();
            move || loop {
                let (event, mut err_ctx) = receive_plugin_instructions
                    .recv()
                    .expect("failed to receive event on channel");
                err_ctx.add_call(ContextType::Plugin(PluginContext::from(&event)));
                match event {
                    PluginInstruction::Load(pid_tx, path) => {
                        let project_dirs =
                            ProjectDirs::from("org", "Zellij Contributors", "Zellij").unwrap();
                        let plugin_dir = project_dirs.data_dir().join("plugins/");
                        let wasm_bytes = fs::read(&path)
                            .or_else(|_| fs::read(&path.with_extension("wasm")))
                            .or_else(|_| fs::read(&plugin_dir.join(&path).with_extension("wasm")))
                            .unwrap_or_else(|_| panic!("cannot find plugin {}", &path.display()));

                        // FIXME: Cache this compiled module on disk. I could use `(de)serialize_to_file()` for that
                        let module = Module::new(&store, &wasm_bytes).unwrap();

                        let output = Pipe::new();
                        let input = Pipe::new();
                        let mut wasi_env = WasiState::new("Zellij")
                            .env("CLICOLOR_FORCE", "1")
                            .preopen(|p| {
                                p.directory(".") // FIXME: Change this to a more meaningful dir
                                    .alias(".")
                                    .read(true)
                                    .write(true)
                                    .create(true)
                            })
                            .unwrap()
                            .stdin(Box::new(input))
                            .stdout(Box::new(output))
                            .finalize()
                            .unwrap();

                        let wasi = wasi_env.import_object(&module).unwrap();

                        let plugin_env = PluginEnv {
                            plugin_id,
                            send_screen_instructions: send_screen_instructions.clone(),
                            send_pty_instructions: send_pty_instructions.clone(),
                            wasi_env,
                            subscriptions: Arc::new(Mutex::new(HashSet::new())),
                        };

                        let zellij = zellij_imports(&store, &plugin_env);
                        let instance = Instance::new(&module, &zellij.chain_back(wasi)).unwrap();

                        let start = instance.exports.get_function("_start").unwrap();

                        // This eventually calls the `.init()` method
                        start.call(&[]).unwrap();

                        plugin_map.insert(plugin_id, (instance, plugin_env));
                        pid_tx.send(plugin_id).unwrap();
                        plugin_id += 1;
                    }
                    PluginInstruction::Update(pid, event) => {
                        for (&i, (instance, plugin_env)) in &plugin_map {
                            let subs = plugin_env.subscriptions.lock().unwrap();
                            // FIXME: This is very janky... Maybe I should write my own macro for Event -> EventType?
                            let event_type = EventType::from_str(&event.to_string()).unwrap();
                            if (pid.is_none() || pid == Some(i)) && subs.contains(&event_type) {
                                let update = instance.exports.get_function("update").unwrap();
                                wasi_write_string(
                                    &plugin_env.wasi_env,
                                    &serde_json::to_string(&event).unwrap(),
                                );
                                update.call(&[]).unwrap();
                            }
                        }
                        drop(send_screen_instructions.send(ScreenInstruction::Render));
                    }
                    PluginInstruction::Render(buf_tx, pid, rows, cols) => {
                        let (instance, plugin_env) = plugin_map.get(&pid).unwrap();

                        let render = instance.exports.get_function("render").unwrap();

                        render
                            .call(&[Value::I32(rows as i32), Value::I32(cols as i32)])
                            .unwrap();

                        buf_tx.send(wasi_stdout(&plugin_env.wasi_env)).unwrap();
                    }
                    PluginInstruction::Unload(pid) => drop(plugin_map.remove(&pid)),
                    PluginInstruction::Exit => break,
                }
            }
        })
        .unwrap();
    SessionMetaData {
        send_plugin_instructions,
        send_screen_instructions,
        send_pty_instructions,
        screen_thread: Some(screen_thread),
        pty_thread: Some(pty_thread),
        wasm_thread: Some(wasm_thread),
    }
}

fn route_action(action: Action, session: &SessionMetaData) {
    match action {
        Action::Write(val) => {
            session
                .send_screen_instructions
                .send(ScreenInstruction::ClearScroll)
                .unwrap();
            session
                .send_screen_instructions
                .send(ScreenInstruction::WriteCharacter(val))
                .unwrap();
        }
        Action::SwitchToMode(mode) => {
            session
                .send_plugin_instructions
                .send(PluginInstruction::Update(
                    None,
                    Event::ModeUpdate(get_mode_info(mode)),
                ))
                .unwrap();
            session
                .send_screen_instructions
                .send(ScreenInstruction::ChangeMode(get_mode_info(mode)))
                .unwrap();
            session
                .send_screen_instructions
                .send(ScreenInstruction::Render)
                .unwrap();
        }
        Action::Resize(direction) => {
            let screen_instr = match direction {
                Direction::Left => ScreenInstruction::ResizeLeft,
                Direction::Right => ScreenInstruction::ResizeRight,
                Direction::Up => ScreenInstruction::ResizeUp,
                Direction::Down => ScreenInstruction::ResizeDown,
            };
            session.send_screen_instructions.send(screen_instr).unwrap();
        }
        Action::SwitchFocus => {
            session
                .send_screen_instructions
                .send(ScreenInstruction::SwitchFocus)
                .unwrap();
        }
        Action::FocusNextPane => {
            session
                .send_screen_instructions
                .send(ScreenInstruction::FocusNextPane)
                .unwrap();
        }
        Action::FocusPreviousPane => {
            session
                .send_screen_instructions
                .send(ScreenInstruction::FocusPreviousPane)
                .unwrap();
        }
        Action::MoveFocus(direction) => {
            let screen_instr = match direction {
                Direction::Left => ScreenInstruction::MoveFocusLeft,
                Direction::Right => ScreenInstruction::MoveFocusRight,
                Direction::Up => ScreenInstruction::MoveFocusUp,
                Direction::Down => ScreenInstruction::MoveFocusDown,
            };
            session.send_screen_instructions.send(screen_instr).unwrap();
        }
        Action::ScrollUp => {
            session
                .send_screen_instructions
                .send(ScreenInstruction::ScrollUp)
                .unwrap();
        }
        Action::ScrollDown => {
            session
                .send_screen_instructions
                .send(ScreenInstruction::ScrollDown)
                .unwrap();
        }
        Action::ToggleFocusFullscreen => {
            session
                .send_screen_instructions
                .send(ScreenInstruction::ToggleActiveTerminalFullscreen)
                .unwrap();
        }
        Action::NewPane(direction) => {
            let pty_instr = match direction {
                Some(Direction::Left) => PtyInstruction::SpawnTerminalVertically(None),
                Some(Direction::Right) => PtyInstruction::SpawnTerminalVertically(None),
                Some(Direction::Up) => PtyInstruction::SpawnTerminalHorizontally(None),
                Some(Direction::Down) => PtyInstruction::SpawnTerminalHorizontally(None),
                // No direction specified - try to put it in the biggest available spot
                None => PtyInstruction::SpawnTerminal(None),
            };
            session.send_pty_instructions.send(pty_instr).unwrap();
        }
        Action::CloseFocus => {
            session
                .send_screen_instructions
                .send(ScreenInstruction::CloseFocusedPane)
                .unwrap();
        }
        Action::NewTab => {
            session
                .send_pty_instructions
                .send(PtyInstruction::NewTab)
                .unwrap();
        }
        Action::GoToNextTab => {
            session
                .send_screen_instructions
                .send(ScreenInstruction::SwitchTabNext)
                .unwrap();
        }
        Action::GoToPreviousTab => {
            session
                .send_screen_instructions
                .send(ScreenInstruction::SwitchTabPrev)
                .unwrap();
        }
        Action::CloseTab => {
            session
                .send_screen_instructions
                .send(ScreenInstruction::CloseTab)
                .unwrap();
        }
        Action::GoToTab(i) => {
            session
                .send_screen_instructions
                .send(ScreenInstruction::GoToTab(i))
                .unwrap();
        }
        Action::TabNameInput(c) => {
            session
                .send_screen_instructions
                .send(ScreenInstruction::UpdateTabName(c))
                .unwrap();
        }
        Action::NoOp => {}
        Action::Quit => panic!("Received unexpected action"),
    }
}
