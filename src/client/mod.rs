pub mod boundaries;
pub mod layout;
pub mod pane_resizer;
pub mod panes;
pub mod tab;

use std::sync::mpsc;
use std::thread;
use std::{collections::HashMap, fs};
use std::{
    collections::HashSet,
    io::Write,
    str::FromStr,
    sync::{Arc, Mutex},
};

use directories_next::ProjectDirs;
use serde::{Deserialize, Serialize};
use wasmer::{ChainableNamedResolver, Instance, Module, Store, Value};
use wasmer_wasi::{Pipe, WasiState};
use zellij_tile::data::{EventType, InputMode};

use crate::cli::CliArgs;
use crate::common::{
    command_is_executing::CommandIsExecuting,
    errors::{AppContext, ContextType, PluginContext, ScreenContext},
    input::handler::input_loop,
    os_input_output::{ClientOsApi, ServerOsApiInstruction},
    pty_bus::PtyInstruction,
    screen::{Screen, ScreenInstruction},
    wasm_vm::{wasi_stdout, wasi_write_string, zellij_imports, PluginEnv, PluginInstruction},
    ChannelWithContext, SenderType, SenderWithContext, SyncChannelWithContext, OPENCALLS,
};
use crate::layout::Layout;
use crate::server::ServerInstruction;

/// Instructions sent from server to client
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ClientInstruction {
    ToScreen(ScreenInstruction),
    ClosePluginPane(u32),
    Error(String),
    DoneClosingPane,
    Exit,
}

/// Instructions related to the client-side application.
#[derive(Clone)]
pub enum AppInstruction {
    Exit,
    Error(String),
    ToPty(PtyInstruction),
    ToScreen(ScreenInstruction),
    ToPlugin(PluginInstruction),
    OsApi(ServerOsApiInstruction),
    DoneClosingPane,
}

impl From<ClientInstruction> for AppInstruction {
    fn from(item: ClientInstruction) -> Self {
        match item {
            ClientInstruction::ToScreen(s) => AppInstruction::ToScreen(s),
            ClientInstruction::Error(e) => AppInstruction::Error(e),
            ClientInstruction::ClosePluginPane(p) => {
                AppInstruction::ToPlugin(PluginInstruction::Unload(p))
            }
            ClientInstruction::DoneClosingPane => AppInstruction::DoneClosingPane,
            ClientInstruction::Exit => AppInstruction::Exit,
        }
    }
}

pub fn start_client(mut os_input: Box<dyn ClientOsApi>, opts: CliArgs) {
    let take_snapshot = "\u{1b}[?1049h";
    os_input.unset_raw_mode(0);
    let _ = os_input
        .get_stdout_writer()
        .write(take_snapshot.as_bytes())
        .unwrap();

    let mut command_is_executing = CommandIsExecuting::new();

    let full_screen_ws = os_input.get_terminal_size_using_fd(0);
    os_input.set_raw_mode(0);
    let (send_screen_instructions, receive_screen_instructions): ChannelWithContext<
        ScreenInstruction,
    > = mpsc::channel();
    let err_ctx = OPENCALLS.with(|ctx| *ctx.borrow());
    let mut send_screen_instructions =
        SenderWithContext::new(err_ctx, SenderType::Sender(send_screen_instructions));

    let (send_plugin_instructions, receive_plugin_instructions): ChannelWithContext<
        PluginInstruction,
    > = mpsc::channel();
    let send_plugin_instructions =
        SenderWithContext::new(err_ctx, SenderType::Sender(send_plugin_instructions));

    let (send_app_instructions, receive_app_instructions): SyncChannelWithContext<AppInstruction> =
        mpsc::sync_channel(500);
    let mut send_app_instructions =
        SenderWithContext::new(err_ctx, SenderType::SyncSender(send_app_instructions));

    os_input.connect_to_server();

    #[cfg(not(test))]
    std::panic::set_hook({
        use crate::errors::handle_panic;
        let send_app_instructions = send_app_instructions.clone();
        Box::new(move |info| {
            handle_panic(info, &send_app_instructions);
        })
    });

    let screen_thread = thread::Builder::new()
        .name("screen".to_string())
        .spawn({
            let mut command_is_executing = command_is_executing.clone();
            let os_input = os_input.clone();
            let send_plugin_instructions = send_plugin_instructions.clone();
            let send_app_instructions = send_app_instructions.clone();
            let max_panes = opts.max_panes;

            move || {
                let mut screen = Screen::new(
                    receive_screen_instructions,
                    send_plugin_instructions,
                    send_app_instructions,
                    &full_screen_ws,
                    os_input,
                    max_panes,
                    InputMode::Normal,
                );
                loop {
                    let (event, mut err_ctx) = screen
                        .receiver
                        .recv()
                        .expect("failed to receive event on channel");
                    err_ctx.add_call(ContextType::Screen(ScreenContext::from(&event)));
                    screen.send_app_instructions.update(err_ctx);
                    match event {
                        ScreenInstruction::Pty(pid, vte_event) => {
                            screen
                                .get_active_tab_mut()
                                .unwrap()
                                .handle_pty_event(pid, vte_event);
                        }
                        ScreenInstruction::Render => {
                            screen.render();
                        }
                        ScreenInstruction::NewPane(pid) => {
                            screen.get_active_tab_mut().unwrap().new_pane(pid);
                            command_is_executing.done_opening_new_pane();
                        }
                        ScreenInstruction::HorizontalSplit(pid) => {
                            screen.get_active_tab_mut().unwrap().horizontal_split(pid);
                            command_is_executing.done_opening_new_pane();
                        }
                        ScreenInstruction::VerticalSplit(pid) => {
                            screen.get_active_tab_mut().unwrap().vertical_split(pid);
                            command_is_executing.done_opening_new_pane();
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
                        ScreenInstruction::MoveFocus => {
                            screen.get_active_tab_mut().unwrap().move_focus();
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
                            command_is_executing.done_updating_tabs();
                        }
                        ScreenInstruction::SwitchTabNext => {
                            screen.switch_tab_next();
                            command_is_executing.done_updating_tabs();
                        }
                        ScreenInstruction::SwitchTabPrev => {
                            screen.switch_tab_prev();
                            command_is_executing.done_updating_tabs();
                        }
                        ScreenInstruction::CloseTab => {
                            screen.close_tab();
                            command_is_executing.done_updating_tabs();
                        }
                        ScreenInstruction::ApplyLayout((layout, new_pane_pids)) => {
                            screen.apply_layout(Layout::new(layout), new_pane_pids);
                            command_is_executing.done_updating_tabs();
                        }
                        ScreenInstruction::GoToTab(tab_index) => {
                            screen.go_to_tab(tab_index as usize);
                            command_is_executing.done_updating_tabs();
                        }
                        ScreenInstruction::UpdateTabName(c) => {
                            screen.update_active_tab_name(c);
                            command_is_executing.done_updating_tabs();
                        }
                        ScreenInstruction::ChangeInputMode(input_mode) => {
                            screen.change_input_mode(input_mode);
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
            let mut send_screen_instructions = send_screen_instructions.clone();
            let mut send_app_instructions = send_app_instructions.clone();

            let store = Store::default();
            let mut plugin_id = 0;
            let mut plugin_map = HashMap::new();
            move || loop {
                let (event, mut err_ctx) = receive_plugin_instructions
                    .recv()
                    .expect("failed to receive event on channel");
                err_ctx.add_call(ContextType::Plugin(PluginContext::from(&event)));
                send_screen_instructions.update(err_ctx);
                send_app_instructions.update(err_ctx);
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
                            send_app_instructions: send_app_instructions.clone(),
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

    let _stdin_thread = thread::Builder::new()
        .name("stdin_handler".to_string())
        .spawn({
            let send_screen_instructions = send_screen_instructions.clone();
            let send_plugin_instructions = send_plugin_instructions.clone();
            let send_app_instructions = send_app_instructions.clone();
            let command_is_executing = command_is_executing.clone();
            let os_input = os_input.clone();
            move || {
                input_loop(
                    os_input,
                    command_is_executing,
                    send_screen_instructions,
                    send_plugin_instructions,
                    send_app_instructions,
                )
            }
        });

    let router_thread = thread::Builder::new()
        .name("router".to_string())
        .spawn({
            let os_input = os_input.clone();
            move || loop {
                let (instruction, err_ctx) = os_input.client_recv();
                send_app_instructions.update(err_ctx);
                match instruction {
                    ClientInstruction::Exit => break,
                    _ => {
                        send_app_instructions
                            .send(AppInstruction::from(instruction))
                            .unwrap();
                    }
                }
            }
        })
        .unwrap();

    #[warn(clippy::never_loop)]
    loop {
        let (app_instruction, mut err_ctx) = receive_app_instructions
            .recv()
            .expect("failed to receive app instruction on channel");

        err_ctx.add_call(ContextType::App(AppContext::from(&app_instruction)));
        send_screen_instructions.update(err_ctx);
        os_input.update_senders(err_ctx);
        match app_instruction {
            AppInstruction::Exit => break,
            AppInstruction::Error(backtrace) => {
                let _ = os_input.send_to_server(ServerInstruction::ClientExit);
                let _ = send_screen_instructions.send(ScreenInstruction::Exit);
                let _ = send_plugin_instructions.send(PluginInstruction::Exit);
                let _ = screen_thread.join();
                let _ = wasm_thread.join();
                os_input.unset_raw_mode(0);
                let goto_start_of_last_line = format!("\u{1b}[{};{}H", full_screen_ws.rows, 1);
                let error = format!("{}\n{}", goto_start_of_last_line, backtrace);
                let _ = os_input
                    .get_stdout_writer()
                    .write(error.as_bytes())
                    .unwrap();
                std::process::exit(1);
            }
            AppInstruction::ToScreen(instruction) => {
                send_screen_instructions.send(instruction).unwrap();
            }
            AppInstruction::ToPlugin(instruction) => {
                send_plugin_instructions.send(instruction).unwrap();
            }
            AppInstruction::ToPty(instruction) => {
                let _ = os_input.send_to_server(ServerInstruction::ToPty(instruction));
            }
            AppInstruction::OsApi(instruction) => {
                let _ = os_input.send_to_server(ServerInstruction::OsApi(instruction));
            }
            AppInstruction::DoneClosingPane => command_is_executing.done_closing_pane(),
        }
    }

    let _ = os_input.send_to_server(ServerInstruction::ClientExit);
    let _ = send_screen_instructions.send(ScreenInstruction::Exit);
    let _ = send_plugin_instructions.send(PluginInstruction::Exit);
    screen_thread.join().unwrap();
    wasm_thread.join().unwrap();
    router_thread.join().unwrap();

    // cleanup();
    let reset_style = "\u{1b}[m";
    let show_cursor = "\u{1b}[?25h";
    let restore_snapshot = "\u{1b}[?1049l";
    let goto_start_of_last_line = format!("\u{1b}[{};{}H", full_screen_ws.rows, 1);
    let goodbye_message = format!(
        "{}\n{}{}{}Bye from Zellij!\n",
        goto_start_of_last_line, restore_snapshot, reset_style, show_cursor
    );

    os_input.unset_raw_mode(0);
    let _ = os_input
        .get_stdout_writer()
        .write(goodbye_message.as_bytes())
        .unwrap();
    os_input.get_stdout_writer().flush().unwrap();
}
