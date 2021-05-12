pub mod route;

use interprocess::local_socket::LocalSocketListener;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, RwLock};
use std::thread;
use std::{path::PathBuf, sync::mpsc::channel};
use wasmer::Store;

use crate::cli::CliArgs;
use crate::client::ClientInstruction;
use crate::common::thread_bus::{Bus, ThreadSenders};
use crate::common::{
    errors::{ContextType, ServerContext},
    input::{actions::Action, options::ConfigOptions},
    os_input_output::{set_permissions, ServerOsApi},
    pty::{pty_thread_main, Pty, PtyInstruction},
    screen::{screen_thread_main, ScreenInstruction},
    setup::install::populate_data_dir,
    thread_bus::{ChannelWithContext, SenderType, SenderWithContext},
    utils::consts::{ZELLIJ_IPC_PIPE, ZELLIJ_PROJ_DIR},
    wasm_vm::{wasm_thread_main, PluginInstruction},
};
use crate::layout::Layout;
use crate::panes::PositionAndSize;
use route::route_thread_main;

/// Instructions related to server-side application including the
/// ones sent by client to server
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ServerInstruction {
    TerminalResize(PositionAndSize),
    NewClient(PositionAndSize, CliArgs, ConfigOptions),
    Action(Action),
    Render(Option<String>),
    UnblockInputThread,
    ClientExit,
}

pub struct SessionMetaData {
    pub senders: ThreadSenders,
    screen_thread: Option<thread::JoinHandle<()>>,
    pty_thread: Option<thread::JoinHandle<()>>,
    wasm_thread: Option<thread::JoinHandle<()>>,
}

impl Drop for SessionMetaData {
    fn drop(&mut self) {
        let _ = self.senders.send_to_pty(PtyInstruction::Exit);
        let _ = self.senders.send_to_screen(ScreenInstruction::Exit);
        let _ = self.senders.send_to_plugin(PluginInstruction::Exit);
        let _ = self.screen_thread.take().unwrap().join();
        let _ = self.pty_thread.take().unwrap().join();
        let _ = self.wasm_thread.take().unwrap().join();
    }
}

pub fn start_server(os_input: Box<dyn ServerOsApi>) -> thread::JoinHandle<()> {
    let (to_server, server_receiver): ChannelWithContext<ServerInstruction> = channel();
    let to_server = SenderWithContext::new(SenderType::Sender(to_server));
    let sessions: Arc<RwLock<Option<SessionMetaData>>> = Arc::new(RwLock::new(None));

    #[cfg(test)]
    thread::Builder::new()
        .name("server_router".to_string())
        .spawn({
            let sessions = sessions.clone();
            let os_input = os_input.clone();
            let to_server = to_server.clone();

            move || route_thread_main(sessions, os_input, to_server)
        })
        .unwrap();
    #[cfg(not(test))]
    let _ = thread::Builder::new()
        .name("server_listener".to_string())
        .spawn({
            let os_input = os_input.clone();
            let sessions = sessions.clone();
            let to_server = to_server.clone();
            move || {
                drop(std::fs::remove_file(&*ZELLIJ_IPC_PIPE));
                let listener = LocalSocketListener::bind(&**ZELLIJ_IPC_PIPE).unwrap();
                set_permissions(&*ZELLIJ_IPC_PIPE).unwrap();
                for stream in listener.incoming() {
                    match stream {
                        Ok(stream) => {
                            let mut os_input = os_input.clone();
                            os_input.update_receiver(stream);
                            let sessions = sessions.clone();
                            let to_server = to_server.clone();
                            thread::Builder::new()
                                .name("server_router".to_string())
                                .spawn({
                                    let sessions = sessions.clone();
                                    let os_input = os_input.clone();
                                    let to_server = to_server.clone();

                                    move || route_thread_main(sessions, os_input, to_server)
                                })
                                .unwrap();
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
                let (instruction, mut err_ctx) = server_receiver.recv().unwrap();
                err_ctx.add_call(ContextType::IPCServer(ServerContext::from(&instruction)));
                match instruction {
                    ServerInstruction::NewClient(full_screen_ws, opts, config_options) => {
                        let session_data = init_session(
                            os_input.clone(),
                            opts,
                            config_options,
                            to_server.clone(),
                            full_screen_ws,
                        );
                        *sessions.write().unwrap() = Some(session_data);
                        sessions
                            .read()
                            .unwrap()
                            .as_ref()
                            .unwrap()
                            .senders
                            .send_to_pty(PtyInstruction::NewTab)
                            .unwrap();
                    }
                    ServerInstruction::UnblockInputThread => {
                        os_input.send_to_client(ClientInstruction::UnblockInputThread);
                    }
                    ServerInstruction::ClientExit => {
                        *sessions.write().unwrap() = None;
                        os_input.send_to_client(ClientInstruction::Exit);
                        drop(std::fs::remove_file(&*ZELLIJ_IPC_PIPE));
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

fn init_session(
    os_input: Box<dyn ServerOsApi>,
    opts: CliArgs,
    config_options: ConfigOptions,
    to_server: SenderWithContext<ServerInstruction>,
    full_screen_ws: PositionAndSize,
) -> SessionMetaData {
    let (to_screen, screen_receiver): ChannelWithContext<ScreenInstruction> = channel();
    let to_screen = SenderWithContext::new(SenderType::Sender(to_screen));

    let (to_plugin, plugin_receiver): ChannelWithContext<PluginInstruction> = channel();
    let to_plugin = SenderWithContext::new(SenderType::Sender(to_plugin));
    let (to_pty, pty_receiver): ChannelWithContext<PtyInstruction> = channel();
    let to_pty = SenderWithContext::new(SenderType::Sender(to_pty));

    // Determine and initialize the data directory
    let data_dir = opts
        .data_dir
        .unwrap_or_else(|| ZELLIJ_PROJ_DIR.data_dir().to_path_buf());
    populate_data_dir(&data_dir);

    // Don't use default layouts in tests, but do everywhere else
    #[cfg(not(test))]
    let default_layout = Some(PathBuf::from("default"));
    #[cfg(test)]
    let default_layout = None;
    let maybe_layout = opts
        .layout
        .map(|p| Layout::new(&p, &data_dir))
        .or_else(|| default_layout.map(|p| Layout::from_defaults(&p, &data_dir)));

    let pty_thread = thread::Builder::new()
        .name("pty".to_string())
        .spawn({
            let pty = Pty::new(
                Bus::new(
                    pty_receiver,
                    Some(&to_screen),
                    None,
                    Some(&to_plugin),
                    Some(&to_server),
                    Some(os_input.clone()),
                ),
                opts.debug,
            );

            move || pty_thread_main(pty, maybe_layout)
        })
        .unwrap();

    let screen_thread = thread::Builder::new()
        .name("screen".to_string())
        .spawn({
            let screen_bus = Bus::new(
                screen_receiver,
                None,
                Some(&to_pty),
                Some(&to_plugin),
                Some(&to_server),
                Some(os_input.clone()),
            );
            let max_panes = opts.max_panes;

            move || {
                screen_thread_main(screen_bus, max_panes, full_screen_ws, config_options);
            }
        })
        .unwrap();

    let wasm_thread = thread::Builder::new()
        .name("wasm".to_string())
        .spawn({
            let plugin_bus = Bus::new(
                plugin_receiver,
                Some(&to_screen),
                Some(&to_pty),
                None,
                None,
                None,
            );
            let store = Store::default();

            move || wasm_thread_main(plugin_bus, store, data_dir)
        })
        .unwrap();
    SessionMetaData {
        senders: ThreadSenders {
            to_screen: Some(to_screen),
            to_pty: Some(to_pty),
            to_plugin: Some(to_plugin),
            to_server: None,
        },
        screen_thread: Some(screen_thread),
        pty_thread: Some(pty_thread),
        wasm_thread: Some(wasm_thread),
    }
}
