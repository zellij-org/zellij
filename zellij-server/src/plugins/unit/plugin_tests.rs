use super::plugin_thread_main;
use crate::screen::ScreenInstruction;
use crate::{channels::SenderWithContext, thread_bus::Bus, ServerInstruction};
use insta::assert_snapshot;
use std::path::PathBuf;
use tempfile::tempdir;
use wasmer::Store;
use zellij_utils::data::Event;
use zellij_utils::errors::ErrorContext;
use zellij_utils::input::layout::{Layout, RunPlugin, RunPluginLocation};
use zellij_utils::input::plugins::PluginsConfig;
use zellij_utils::lazy_static::lazy_static;
use zellij_utils::pane_size::Size;

use crate::background_jobs::BackgroundJob;
use crate::pty_writer::PtyWriteInstruction;
use std::env::set_var;
use std::sync::{Arc, Mutex};

use crate::{plugins::PluginInstruction, pty::PtyInstruction};

use zellij_utils::channels::{self, ChannelWithContext, Receiver};

macro_rules! log_actions_in_thread {
    ( $arc_mutex_log:expr, $exit_event:path, $receiver:expr, $exit_after_count:expr ) => {
        std::thread::Builder::new()
            .name("logger thread".to_string())
            .spawn({
                let log = $arc_mutex_log.clone();
                let mut exit_event_count = 0;
                move || loop {
                    let (event, _err_ctx) = $receiver
                        .recv()
                        .expect("failed to receive event on channel");
                    eprintln!("received event: {:#?}", event);
                    match event {
                        $exit_event(..) => {
                            exit_event_count += 1;
                            log.lock().unwrap().push(event);
                            if exit_event_count == $exit_after_count {
                                break;
                            }
                        },
                        _ => {
                            log.lock().unwrap().push(event);
                        },
                    }
                }
            })
            .unwrap()
    };
}

fn create_plugin_thread() -> (
    SenderWithContext<PluginInstruction>,
    Receiver<(ScreenInstruction, ErrorContext)>,
    Box<dyn FnMut()>,
) {
    let (to_server, _server_receiver): ChannelWithContext<ServerInstruction> =
        channels::bounded(50);
    let to_server = SenderWithContext::new(to_server);

    let (to_screen, screen_receiver): ChannelWithContext<ScreenInstruction> = channels::unbounded();
    let to_screen = SenderWithContext::new(to_screen);

    let (to_plugin, plugin_receiver): ChannelWithContext<PluginInstruction> = channels::unbounded();
    let to_plugin = SenderWithContext::new(to_plugin);
    let (to_pty, _pty_receiver): ChannelWithContext<PtyInstruction> = channels::unbounded();
    let to_pty = SenderWithContext::new(to_pty);

    let (to_pty_writer, _pty_writer_receiver): ChannelWithContext<PtyWriteInstruction> =
        channels::unbounded();
    let to_pty_writer = SenderWithContext::new(to_pty_writer);

    let (to_background_jobs, _background_jobs_receiver): ChannelWithContext<BackgroundJob> =
        channels::unbounded();
    let to_background_jobs = SenderWithContext::new(to_background_jobs);

    let plugin_bus = Bus::new(
        vec![plugin_receiver],
        Some(&to_screen),
        Some(&to_pty),
        Some(&to_plugin),
        Some(&to_server),
        Some(&to_pty_writer),
        Some(&to_background_jobs),
        None,
    )
    .should_silently_fail();
    let store = Store::new(&wasmer::Universal::new(wasmer::Singlepass::default()).engine());
    let data_dir = PathBuf::from(tempdir().unwrap().path());
    let _plugin_thread = std::thread::Builder::new()
        .name("plugin_thread".to_string())
        .spawn(move || {
            set_var("ZELLIJ_SESSION_NAME", "zellij-test");
            plugin_thread_main(
                plugin_bus,
                store,
                data_dir,
                PluginsConfig::default(),
                Box::new(Layout::default()),
            )
            .expect("TEST")
        })
        .unwrap();
    let teardown = {
        let to_plugin = to_plugin.clone();
        move || {
            let _ = to_pty.send(PtyInstruction::Exit);
            let _ = to_pty_writer.send(PtyWriteInstruction::Exit);
            let _ = to_screen.send(ScreenInstruction::Exit);
            let _ = to_server.send(ServerInstruction::KillSession);
            let _ = to_plugin.send(PluginInstruction::Exit);
        }
    };
    (to_plugin, screen_receiver, Box::new(teardown))
}

lazy_static! {
    static ref PLUGIN_FIXTURE: String = format!(
        "{}/../target/wasm32-wasi/debug/fixture-plugin-for-tests.wasm",
        std::env::var_os("CARGO_MANIFEST_DIR")
            .unwrap()
            .to_string_lossy()
    );
}

#[test]
#[ignore]
pub fn load_new_plugin_from_hd() {
    // here we load our fixture plugin into the plugin thread, and then send it an update message
    // expecting tha thte plugin will log the received event and render it later after the update
    // message (this is what the fixture plugin does)
    // we then listen on our mock screen receiver to make sure we got a PluginBytes instruction
    // that contains said render, and assert against it
    eprintln!("load_new_plugin_from_hd debug 1");
    let (plugin_thread_sender, screen_receiver, mut teardown) = create_plugin_thread();
    eprintln!("load_new_plugin_from_hd debug 2");
    let plugin_should_float = Some(false);
    let plugin_title = Some("test_plugin".to_owned());
    let run_plugin = RunPlugin {
        _allow_exec_host_cmd: false,
        location: RunPluginLocation::File(PathBuf::from(&*PLUGIN_FIXTURE)),
    };
    let tab_index = 1;
    let client_id = 1;
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let received_screen_instructions = Arc::new(Mutex::new(vec![]));
    let screen_thread = log_actions_in_thread!(
        received_screen_instructions,
        ScreenInstruction::PluginBytes,
        screen_receiver,
        2
    );
    eprintln!("load_new_plugin_from_hd debug 3");

    let _ = plugin_thread_sender.send(PluginInstruction::AddClient(client_id));
    eprintln!("load_new_plugin_from_hd debug 4");
    let _ = plugin_thread_sender.send(PluginInstruction::Load(
        plugin_should_float,
        plugin_title,
        run_plugin,
        tab_index,
        client_id,
        size,
    ));
    eprintln!("load_new_plugin_from_hd debug 5");
    let _ = plugin_thread_sender.send(PluginInstruction::Update(vec![(
        None,
        Some(client_id),
        Event::InputReceived,
    )])); // will be cached and sent to the plugin once it's loaded
    eprintln!("load_new_plugin_from_hd debug 6");
    screen_thread.join().unwrap(); // this might take a while if the cache is cold
    eprintln!("load_new_plugin_from_hd debug 7");
    teardown();
    eprintln!("load_new_plugin_from_hd debug 8");
    let plugin_bytes_event = received_screen_instructions
        .lock()
        .unwrap()
        .iter()
        .find_map(|i| {
            if let ScreenInstruction::PluginBytes(plugin_bytes) = i {
                for (plugin_id, client_id, plugin_bytes) in plugin_bytes {
                    let plugin_bytes = String::from_utf8_lossy(plugin_bytes).to_string();
                    if plugin_bytes.contains("InputReceived") {
                        return Some((*plugin_id, *client_id, plugin_bytes));
                    }
                }
            }
            None
        });
    eprintln!("load_new_plugin_from_hd debug 9");
    assert_snapshot!(format!("{:#?}", plugin_bytes_event));
    eprintln!("load_new_plugin_from_hd debug 10");
}

#[test]
#[ignore]
pub fn plugin_workers() {
    let (plugin_thread_sender, screen_receiver, mut teardown) = create_plugin_thread();
    let plugin_should_float = Some(false);
    let plugin_title = Some("test_plugin".to_owned());
    let run_plugin = RunPlugin {
        _allow_exec_host_cmd: false,
        location: RunPluginLocation::File(PathBuf::from(&*PLUGIN_FIXTURE)),
    };
    let tab_index = 1;
    let client_id = 1;
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let received_screen_instructions = Arc::new(Mutex::new(vec![]));
    let screen_thread = log_actions_in_thread!(
        received_screen_instructions,
        ScreenInstruction::PluginBytes,
        screen_receiver,
        3
    );

    let _ = plugin_thread_sender.send(PluginInstruction::AddClient(client_id));
    let _ = plugin_thread_sender.send(PluginInstruction::Load(
        plugin_should_float,
        plugin_title,
        run_plugin,
        tab_index,
        client_id,
        size,
    ));
    // we send a SystemClipboardFailure to trigger the custom handler in the fixture plugin that
    // will send a message to the worker and in turn back to the plugin to be rendered, so we know
    // that this cycle is working
    let _ = plugin_thread_sender.send(PluginInstruction::Update(vec![(
        None,
        Some(client_id),
        Event::SystemClipboardFailure,
    )])); // will be cached and sent to the plugin once it's loaded
    screen_thread.join().unwrap(); // this might take a while if the cache is cold
    teardown();
    let plugin_bytes_event = received_screen_instructions
        .lock()
        .unwrap()
        .iter()
        .find_map(|i| {
            if let ScreenInstruction::PluginBytes(plugin_bytes) = i {
                for (plugin_id, client_id, plugin_bytes) in plugin_bytes {
                    let plugin_bytes = String::from_utf8_lossy(plugin_bytes).to_string();
                    if plugin_bytes.contains("Payload from worker") {
                        return Some((*plugin_id, *client_id, plugin_bytes));
                    }
                }
            }
            None
        });
    assert_snapshot!(format!("{:#?}", plugin_bytes_event));
}
