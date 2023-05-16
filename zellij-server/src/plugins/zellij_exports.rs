use super::PluginInstruction;
use crate::plugins::plugin_map::{PluginEnv, Subscriptions};
use log::{debug, warn};
use serde::{de::DeserializeOwned, Serialize};
use std::{
    collections::HashSet,
    path::PathBuf,
    process,
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};
use wasmer::{imports, Function, ImportObject, Store, WasmerEnv};
use wasmer_wasi::WasiEnv;

use crate::{
    panes::PaneId,
    pty::{ClientOrTabIndex, PtyInstruction},
    screen::ScreenInstruction,
};

use zellij_utils::{
    consts::VERSION,
    data::{Event, EventType, PluginIds},
    errors::prelude::*,
    input::{command::TerminalAction, plugins::PluginType},
    serde,
};

pub fn zellij_exports(
    store: &Store,
    plugin_env: &PluginEnv,
    subscriptions: &Arc<Mutex<Subscriptions>>,
) -> ImportObject {
    macro_rules! zellij_export {
        ($($host_function:ident),+ $(,)?) => {
            imports! {
                "zellij" => {
                    $(stringify!($host_function) =>
                        Function::new_native_with_env(store, ForeignFunctionEnv::new(plugin_env, subscriptions), $host_function),)+
                }
            }
        }
    }

    zellij_export! {
        host_subscribe,
        host_unsubscribe,
        host_set_selectable,
        host_get_plugin_ids,
        host_get_zellij_version,
        host_open_file,
        host_open_file_with_line,
        host_switch_tab_to,
        host_set_timeout,
        host_exec_cmd,
        host_report_panic,
        host_post_message_to,
        host_post_message_to_plugin,
    }
}

#[derive(WasmerEnv, Clone)]
pub struct ForeignFunctionEnv {
    pub plugin_env: PluginEnv,
    pub subscriptions: Arc<Mutex<Subscriptions>>,
}

impl ForeignFunctionEnv {
    pub fn new(plugin_env: &PluginEnv, subscriptions: &Arc<Mutex<Subscriptions>>) -> Self {
        ForeignFunctionEnv {
            plugin_env: plugin_env.clone(),
            subscriptions: subscriptions.clone(),
        }
    }
}

fn host_subscribe(env: &ForeignFunctionEnv) {
    wasi_read_object::<HashSet<EventType>>(&env.plugin_env.wasi_env)
        .and_then(|new| {
            env.subscriptions.lock().to_anyhow()?.extend(new);
            Ok(())
        })
        .with_context(|| format!("failed to subscribe for plugin {}", env.plugin_env.name()))
        .fatal();
}

fn host_unsubscribe(env: &ForeignFunctionEnv) {
    wasi_read_object::<HashSet<EventType>>(&env.plugin_env.wasi_env)
        .and_then(|old| {
            env.subscriptions
                .lock()
                .to_anyhow()?
                .retain(|k| !old.contains(k));
            Ok(())
        })
        .with_context(|| format!("failed to unsubscribe for plugin {}", env.plugin_env.name()))
        .fatal();
}

fn host_set_selectable(env: &ForeignFunctionEnv, selectable: i32) {
    match env.plugin_env.plugin.run {
        PluginType::Pane(Some(tab_index)) => {
            let selectable = selectable != 0;
            env.plugin_env
                .senders
                .send_to_screen(ScreenInstruction::SetSelectable(
                    PaneId::Plugin(env.plugin_env.plugin_id),
                    selectable,
                    tab_index,
                ))
                .with_context(|| {
                    format!(
                        "failed to set plugin {} selectable from plugin {}",
                        selectable,
                        env.plugin_env.name()
                    )
                })
                .non_fatal();
        },
        _ => {
            debug!(
                "{} - Calling method 'host_set_selectable' does nothing for headless plugins",
                env.plugin_env.plugin.location
            )
        },
    }
}

fn host_get_plugin_ids(env: &ForeignFunctionEnv) {
    let ids = PluginIds {
        plugin_id: env.plugin_env.plugin_id,
        zellij_pid: process::id(),
    };
    wasi_write_object(&env.plugin_env.wasi_env, &ids)
        .with_context(|| {
            format!(
                "failed to query plugin IDs from host for plugin {}",
                env.plugin_env.name()
            )
        })
        .non_fatal();
}

fn host_get_zellij_version(env: &ForeignFunctionEnv) {
    wasi_write_object(&env.plugin_env.wasi_env, VERSION)
        .with_context(|| {
            format!(
                "failed to request zellij version from host for plugin {}",
                env.plugin_env.name()
            )
        })
        .non_fatal();
}

fn host_open_file(env: &ForeignFunctionEnv) {
    wasi_read_object::<PathBuf>(&env.plugin_env.wasi_env)
        .and_then(|path| {
            env.plugin_env
                .senders
                .send_to_pty(PtyInstruction::SpawnTerminal(
                    Some(TerminalAction::OpenFile(path, None, None)),
                    None,
                    None,
                    ClientOrTabIndex::TabIndex(env.plugin_env.tab_index),
                ))
        })
        .with_context(|| {
            format!(
                "failed to open file on host from plugin {}",
                env.plugin_env.name()
            )
        })
        .non_fatal();
}

fn host_open_file_with_line(env: &ForeignFunctionEnv) {
    wasi_read_object::<(PathBuf, usize)>(&env.plugin_env.wasi_env)
        .and_then(|(path, line)| {
            env.plugin_env
                .senders
                .send_to_pty(PtyInstruction::SpawnTerminal(
                    Some(TerminalAction::OpenFile(path, Some(line), None)), // TODO: add cwd
                    None,
                    None,
                    ClientOrTabIndex::TabIndex(env.plugin_env.tab_index),
                ))
        })
        .with_context(|| {
            format!(
                "failed to open file on host from plugin {}",
                env.plugin_env.name()
            )
        })
        .non_fatal();
}

fn host_switch_tab_to(env: &ForeignFunctionEnv, tab_idx: u32) {
    env.plugin_env
        .senders
        .send_to_screen(ScreenInstruction::GoToTab(
            tab_idx,
            Some(env.plugin_env.client_id),
        ))
        .with_context(|| {
            format!(
                "failed to switch host to tab {tab_idx} from plugin {}",
                env.plugin_env.name()
            )
        })
        .non_fatal();
}

fn host_set_timeout(env: &ForeignFunctionEnv, secs: f64) {
    // There is a fancy, high-performance way to do this with zero additional threads:
    // If the plugin thread keeps a BinaryHeap of timer structs, it can manage multiple and easily `.peek()` at the
    // next time to trigger in O(1) time. Once the wake-up time is known, the `wasm` thread can use `recv_timeout()`
    // to wait for an event with the timeout set to be the time of the next wake up. If events come in in the meantime,
    // they are handled, but if the timeout triggers, we replace the event from `recv()` with an
    // `Update(pid, TimerEvent)` and pop the timer from the Heap (or reschedule it). No additional threads for as many
    // timers as we'd like.
    //
    // But that's a lot of code, and this is a few lines:
    let send_plugin_instructions = env.plugin_env.senders.to_plugin.clone();
    let update_target = Some(env.plugin_env.plugin_id);
    let client_id = env.plugin_env.client_id;
    let plugin_name = env.plugin_env.name();
    // TODO: we should really use an async task for this
    thread::spawn(move || {
        let start_time = Instant::now();
        thread::sleep(Duration::from_secs_f64(secs));
        // FIXME: The way that elapsed time is being calculated here is not exact; it doesn't take into account the
        // time it takes an event to actually reach the plugin after it's sent to the `wasm` thread.
        let elapsed_time = Instant::now().duration_since(start_time).as_secs_f64();

        send_plugin_instructions
            .ok_or(anyhow!("found no sender to send plugin instruction to"))
            .and_then(|sender| {
                sender
                    .send(PluginInstruction::Update(vec![(
                        update_target,
                        Some(client_id),
                        Event::Timer(elapsed_time),
                    )]))
                    .to_anyhow()
            })
            .with_context(|| {
                format!(
                    "failed to set host timeout of {secs} s for plugin {}",
                    plugin_name
                )
            })
            .non_fatal();
    });
}

fn host_exec_cmd(env: &ForeignFunctionEnv) {
    let err_context = || {
        format!(
            "failed to execute command on host for plugin '{}'",
            env.plugin_env.name()
        )
    };

    let mut cmdline: Vec<String> = wasi_read_object(&env.plugin_env.wasi_env)
        .with_context(err_context)
        .fatal();
    let command = cmdline.remove(0);

    // Bail out if we're forbidden to run command
    if !env.plugin_env.plugin._allow_exec_host_cmd {
        warn!("This plugin isn't allow to run command in host side, skip running this command: '{cmd} {args}'.",
        	cmd = command, args = cmdline.join(" "));
        return;
    }

    // Here, we don't wait the command to finish
    process::Command::new(command)
        .args(cmdline)
        .spawn()
        .with_context(err_context)
        .non_fatal();
}

fn host_post_message_to(env: &ForeignFunctionEnv) {
    wasi_read_object::<(String, String, String)>(&env.plugin_env.wasi_env)
        .and_then(|(worker_name, message, payload)| {
            env.plugin_env
                .senders
                .send_to_plugin(PluginInstruction::PostMessagesToPluginWorker(
                    env.plugin_env.plugin_id,
                    env.plugin_env.client_id,
                    worker_name,
                    vec![(message, payload)],
                ))
        })
        .with_context(|| format!("failed to post message to worker {}", env.plugin_env.name()))
        .fatal();
}

fn host_post_message_to_plugin(env: &ForeignFunctionEnv) {
    wasi_read_object::<(String, String)>(&env.plugin_env.wasi_env)
        .and_then(|(message, payload)| {
            env.plugin_env
                .senders
                .send_to_plugin(PluginInstruction::PostMessageToPlugin(
                    env.plugin_env.plugin_id,
                    env.plugin_env.client_id,
                    message,
                    payload,
                ))
        })
        .with_context(|| format!("failed to post message to plugin {}", env.plugin_env.name()))
        .fatal();
}

// Custom panic handler for plugins.
//
// This is called when a panic occurs in a plugin. Since most panics will likely originate in the
// code trying to deserialize an `Event` upon a plugin state update, we read some panic message,
// formatted as string from the plugin.
fn host_report_panic(env: &ForeignFunctionEnv) {
    let msg = wasi_read_string(&env.plugin_env.wasi_env)
        .with_context(|| {
            format!(
                "failed to report panic for plugin '{}'",
                env.plugin_env.name()
            )
        })
        .fatal();
    panic!("{}", msg);
}

// Helper Functions ---------------------------------------------------------------------------------------------------

pub fn wasi_read_string(wasi_env: &WasiEnv) -> Result<String> {
    let err_context = || format!("failed to read string from WASI env '{wasi_env:?}'");

    let mut buf = vec![];
    wasi_env
        .state()
        .fs
        .stdout_mut()
        .map_err(anyError::new)
        .and_then(|stdout| {
            stdout
                .as_mut()
                .ok_or(anyhow!("failed to get mutable reference to stdout"))
        })
        .and_then(|wasi_file| wasi_file.read_to_end(&mut buf).map_err(anyError::new))
        .with_context(err_context)?;
    let buf = String::from_utf8_lossy(&buf);
    // https://stackoverflow.com/questions/66450942/in-rust-is-there-a-way-to-make-literal-newlines-in-r-using-windows-c
    Ok(buf.replace("\n", "\n\r"))
}

pub fn wasi_write_string(wasi_env: &WasiEnv, buf: &str) -> Result<()> {
    wasi_env
        .state()
        .fs
        .stdin_mut()
        .map_err(anyError::new)
        .and_then(|stdin| {
            stdin
                .as_mut()
                .ok_or(anyhow!("failed to get mutable reference to stdin"))
        })
        .and_then(|stdin| writeln!(stdin, "{}\r", buf).map_err(anyError::new))
        .with_context(|| format!("failed to write string to WASI env '{wasi_env:?}'"))
}

pub fn wasi_write_object(wasi_env: &WasiEnv, object: &(impl Serialize + ?Sized)) -> Result<()> {
    serde_json::to_string(&object)
        .map_err(anyError::new)
        .and_then(|string| wasi_write_string(wasi_env, &string))
        .with_context(|| format!("failed to serialize object for WASI env '{wasi_env:?}'"))
}

pub fn wasi_read_object<T: DeserializeOwned>(wasi_env: &WasiEnv) -> Result<T> {
    wasi_read_string(wasi_env)
        .and_then(|string| serde_json::from_str(&string).map_err(anyError::new))
        .with_context(|| format!("failed to deserialize object from WASI env '{wasi_env:?}'"))
}
