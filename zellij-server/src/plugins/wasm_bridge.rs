use super::PluginInstruction;
use crate::plugins::start_plugin::start_plugin;
use log::{debug, info, warn};
use serde::{de::DeserializeOwned, Serialize};
use std::{
    collections::{HashMap, HashSet},
    fmt,
    path::PathBuf,
    process,
    str::FromStr,
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};
use wasmer::{
    imports, ChainableNamedResolver, Function, ImportObject, Instance, Module, Store, Value,
    WasmerEnv,
};
use wasmer_wasi::WasiEnv;
use zellij_utils::async_std::task::{self, JoinHandle};

use crate::{
    background_jobs::BackgroundJob,
    panes::PaneId,
    pty::{ClientOrTabIndex, PtyInstruction},
    screen::ScreenInstruction,
    thread_bus::ThreadSenders,
    ui::loading_indication::LoadingIndication,
    ClientId,
};

use zellij_utils::{
    consts::VERSION,
    data::{Event, EventType, PluginIds},
    errors::prelude::*,
    input::{
        command::TerminalAction,
        layout::RunPlugin,
        plugins::{PluginConfig, PluginType, PluginsConfig},
    },
    pane_size::Size,
    serde,
};

/// Custom error for plugin version mismatch.
///
/// This is thrown when, during starting a plugin, it is detected that the plugin version doesn't
/// match the zellij version. This is treated as a fatal error and leads to instantaneous
/// termination.
#[derive(Debug)]
pub struct VersionMismatchError {
    zellij_version: String,
    plugin_version: String,
    plugin_path: PathBuf,
    // true for builtin plugins
    builtin: bool,
}

impl std::error::Error for VersionMismatchError {}

impl VersionMismatchError {
    pub fn new(
        zellij_version: &str,
        plugin_version: &str,
        plugin_path: &PathBuf,
        builtin: bool,
    ) -> Self {
        VersionMismatchError {
            zellij_version: zellij_version.to_owned(),
            plugin_version: plugin_version.to_owned(),
            plugin_path: plugin_path.to_owned(),
            builtin,
        }
    }
}

impl fmt::Display for VersionMismatchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let first_line = if self.builtin {
            "It seems your version of zellij was built with outdated core plugins."
        } else {
            "If you're seeing this error a plugin version doesn't match the current
zellij version."
        };

        write!(
            f,
            "{}
Detected versions:

- Plugin version: {}
- Zellij version: {}
- Offending plugin: {}

If you're a user:
    Please contact the distributor of your zellij version and report this error
    to them.

If you're a developer:
    Please run zellij with updated plugins. The easiest way to achieve this
    is to build zellij with `cargo xtask install`. Also refer to the docs:
    https://github.com/zellij-org/zellij/blob/main/CONTRIBUTING.md#building
",
            first_line,
            self.plugin_version.trim_end(),
            self.zellij_version.trim_end(),
            self.plugin_path.display()
        )
    }
}

#[derive(WasmerEnv, Clone)]
pub struct PluginEnv {
    pub plugin_id: u32,
    pub plugin: PluginConfig,
    pub senders: ThreadSenders,
    pub wasi_env: WasiEnv,
    pub subscriptions: Arc<Mutex<HashSet<EventType>>>,
    pub tab_index: usize,
    pub client_id: ClientId,
    #[allow(dead_code)]
    pub plugin_own_data_dir: PathBuf,
}

impl PluginEnv {
    // Get the name (path) of the containing plugin
    pub fn name(&self) -> String {
        format!(
            "{} (ID {})",
            self.plugin.path.display().to_string(),
            self.plugin_id
        )
    }
}

pub type PluginMap = HashMap<(u32, ClientId), (Instance, PluginEnv, (usize, usize))>; // u32 =>
                                                                                      // plugin_id,
                                                                                      // (usize, usize)
                                                                                      // => (rows,
                                                                                      // columns)

pub struct WasmBridge {
    connected_clients: Arc<Mutex<Vec<ClientId>>>,
    plugins: PluginsConfig,
    senders: ThreadSenders,
    store: Store,
    plugin_dir: PathBuf,
    plugin_cache: Arc<Mutex<HashMap<PathBuf, Module>>>,
    plugin_map: Arc<Mutex<PluginMap>>,
    next_plugin_id: u32,
    cached_events_for_pending_plugins: HashMap<u32, Vec<Event>>, // u32 is the plugin id
    cached_resizes_for_pending_plugins: HashMap<u32, (usize, usize)>, // (rows, columns)
    loading_plugins: HashMap<u32, JoinHandle<()>>,               // plugin_id to join-handle
}

impl WasmBridge {
    pub fn new(
        plugins: PluginsConfig,
        senders: ThreadSenders,
        store: Store,
        plugin_dir: PathBuf,
    ) -> Self {
        let plugin_map = Arc::new(Mutex::new(HashMap::new()));
        let connected_clients: Arc<Mutex<Vec<ClientId>>> = Arc::new(Mutex::new(vec![]));
        let plugin_cache: Arc<Mutex<HashMap<PathBuf, Module>>> =
            Arc::new(Mutex::new(HashMap::new()));
        WasmBridge {
            connected_clients,
            plugins,
            senders,
            store,
            plugin_dir,
            plugin_cache,
            plugin_map,
            next_plugin_id: 0,
            cached_events_for_pending_plugins: HashMap::new(),
            cached_resizes_for_pending_plugins: HashMap::new(),
            loading_plugins: HashMap::new(),
        }
    }
    pub fn load_plugin(
        &mut self,
        run: &RunPlugin,
        tab_index: usize,
        size: Size,
        client_id: ClientId,
    ) -> Result<u32> {
        // returns the plugin id
        let err_context = move || format!("failed to load plugin for client {client_id}");
        let plugin_id = self.next_plugin_id;

        let plugin = self
            .plugins
            .get(run)
            .with_context(|| format!("failed to resolve plugin {run:?}"))
            .with_context(err_context)
            .fatal();
        let plugin_name = run.location.to_string();

        self.next_plugin_id += 1;

        self.cached_events_for_pending_plugins
            .insert(plugin_id, vec![]);
        self.cached_resizes_for_pending_plugins
            .insert(plugin_id, (0, 0));

        let load_plugin_task = task::spawn({
            let plugin_dir = self.plugin_dir.clone();
            let plugin_cache = self.plugin_cache.clone();
            let senders = self.senders.clone();
            let store = self.store.clone();
            let plugin_map = self.plugin_map.clone();
            let connected_clients = self.connected_clients.clone();
            async move {
                let _ =
                    senders.send_to_background_jobs(BackgroundJob::AnimatePluginLoading(plugin_id));
                let mut loading_indication = LoadingIndication::new(plugin_name.clone());
                match start_plugin(
                    plugin_id,
                    client_id,
                    &plugin,
                    tab_index,
                    plugin_dir,
                    plugin_cache,
                    senders.clone(),
                    store,
                    plugin_map,
                    size,
                    connected_clients.clone(),
                    &mut loading_indication,
                ) {
                    Ok(_) => {
                        let _ = senders.send_to_background_jobs(
                            BackgroundJob::StopPluginLoadingAnimation(plugin_id),
                        );
                        let _ =
                            senders.send_to_plugin(PluginInstruction::ApplyCachedEvents(plugin_id));
                    },
                    Err(e) => {
                        let _ = senders.send_to_background_jobs(
                            BackgroundJob::StopPluginLoadingAnimation(plugin_id),
                        );
                        let _ =
                            senders.send_to_plugin(PluginInstruction::ApplyCachedEvents(plugin_id));
                        loading_indication.indicate_loading_error(e.to_string());
                        let _ =
                            senders.send_to_screen(ScreenInstruction::UpdatePluginLoadingStage(
                                plugin_id,
                                loading_indication.clone(),
                            ));
                    },
                }
            }
        });
        self.loading_plugins.insert(plugin_id, load_plugin_task);
        self.next_plugin_id += 1;
        Ok(plugin_id)
    }
    pub fn unload_plugin(&mut self, pid: u32) -> Result<()> {
        info!("Bye from plugin {}", &pid);
        // TODO: remove plugin's own data directory
        let mut plugin_map = self.plugin_map.lock().unwrap();
        let ids_in_plugin_map: Vec<(u32, ClientId)> = plugin_map.keys().copied().collect();
        for (plugin_id, client_id) in ids_in_plugin_map {
            if pid == plugin_id {
                drop(plugin_map.remove(&(plugin_id, client_id)));
            }
        }
        Ok(())
    }
    pub fn add_client(&mut self, client_id: ClientId) -> Result<()> {
        let err_context = || format!("failed to add plugins for client {client_id}");

        self.connected_clients.lock().unwrap().push(client_id);

        let mut seen = HashSet::new();
        let mut new_plugins = HashMap::new();
        let mut plugin_map = self.plugin_map.lock().unwrap();
        for (&(plugin_id, _), (instance, plugin_env, (rows, columns))) in &*plugin_map {
            if seen.contains(&plugin_id) {
                continue;
            }
            seen.insert(plugin_id);
            let mut new_plugin_env = plugin_env.clone();

            new_plugin_env.client_id = client_id;
            new_plugins.insert(
                plugin_id,
                (instance.module().clone(), new_plugin_env, (*rows, *columns)),
            );
        }
        for (plugin_id, (module, mut new_plugin_env, (rows, columns))) in new_plugins.drain() {
            let wasi = new_plugin_env
                .wasi_env
                .import_object(&module)
                .with_context(err_context)?;
            let zellij = zellij_exports(&self.store, &new_plugin_env);
            let mut instance =
                Instance::new(&module, &zellij.chain_back(wasi)).with_context(err_context)?;
            load_plugin_instance(&mut instance).with_context(err_context)?;
            plugin_map.insert(
                (plugin_id, client_id),
                (instance, new_plugin_env, (rows, columns)),
            );
        }
        Ok(())
    }
    pub fn resize_plugin(&mut self, pid: u32, new_columns: usize, new_rows: usize) -> Result<()> {
        let err_context = || format!("failed to resize plugin {pid}");
        let mut plugin_bytes = vec![];
        let mut plugin_map = self.plugin_map.lock().unwrap();
        for ((plugin_id, client_id), (instance, plugin_env, (current_rows, current_columns))) in
            plugin_map.iter_mut()
        {
            if *plugin_id == pid {
                *current_rows = new_rows;
                *current_columns = new_columns;

                // TODO: consolidate with above render function
                let rendered_bytes = instance
                    .exports
                    .get_function("render")
                    .map_err(anyError::new)
                    .and_then(|render| {
                        render
                            .call(&[
                                Value::I32(*current_rows as i32),
                                Value::I32(*current_columns as i32),
                            ])
                            .map_err(anyError::new)
                    })
                    .and_then(|_| wasi_read_string(&plugin_env.wasi_env))
                    .with_context(err_context)?;

                plugin_bytes.push((*plugin_id, *client_id, rendered_bytes.as_bytes().to_vec()));
            }
        }
        for (plugin_id, (current_rows, current_columns)) in
            self.cached_resizes_for_pending_plugins.iter_mut()
        {
            if *plugin_id == pid {
                *current_rows = new_rows;
                *current_columns = new_columns;
            }
        }
        let _ = self
            .senders
            .send_to_screen(ScreenInstruction::PluginBytes(plugin_bytes));
        Ok(())
    }
    pub fn update_plugins(
        &mut self,
        mut updates: Vec<(Option<u32>, Option<ClientId>, Event)>,
    ) -> Result<()> {
        let err_context = || "failed to update plugin state".to_string();

        let plugin_map = self.plugin_map.lock().unwrap();
        let mut plugin_bytes = vec![];
        for (pid, cid, event) in updates.drain(..) {
            for (&(plugin_id, client_id), (instance, plugin_env, (rows, columns))) in &*plugin_map {
                let subs = plugin_env
                    .subscriptions
                    .lock()
                    .to_anyhow()
                    .with_context(err_context)?;
                // FIXME: This is very janky... Maybe I should write my own macro for Event -> EventType?
                let event_type =
                    EventType::from_str(&event.to_string()).with_context(err_context)?;
                if subs.contains(&event_type)
                    && ((pid.is_none() && cid.is_none())
                        || (pid.is_none() && cid == Some(client_id))
                        || (cid.is_none() && pid == Some(plugin_id))
                        || (cid == Some(client_id) && pid == Some(plugin_id)))
                {
                    apply_event_to_plugin(
                        plugin_id,
                        client_id,
                        &instance,
                        &plugin_env,
                        &event,
                        *rows,
                        *columns,
                        &mut plugin_bytes,
                    )?;
                }
            }
            for (plugin_id, cached_events) in self.cached_events_for_pending_plugins.iter_mut() {
                if pid.is_none() || pid.as_ref() == Some(plugin_id) {
                    cached_events.push(event.clone());
                }
            }
        }
        let _ = self
            .senders
            .send_to_screen(ScreenInstruction::PluginBytes(plugin_bytes));
        Ok(())
    }
    pub fn apply_cached_events(&mut self, plugin_id: u32) -> Result<()> {
        let err_context = || format!("Failed to apply cached events to plugin {plugin_id}");
        if let Some(events) = self.cached_events_for_pending_plugins.remove(&plugin_id) {
            let mut plugin_map = self.plugin_map.lock().unwrap();
            let all_connected_clients: Vec<ClientId> = self
                .connected_clients
                .lock()
                .unwrap()
                .iter()
                .copied()
                .collect();
            for client_id in all_connected_clients {
                let mut plugin_bytes = vec![];
                if let Some((instance, plugin_env, (rows, columns))) =
                    plugin_map.get_mut(&(plugin_id, client_id))
                {
                    let subs = plugin_env
                        .subscriptions
                        .lock()
                        .to_anyhow()
                        .with_context(err_context)?;
                    for event in events.clone() {
                        let event_type =
                            EventType::from_str(&event.to_string()).with_context(err_context)?;
                        if !subs.contains(&event_type) {
                            continue;
                        }
                        apply_event_to_plugin(
                            plugin_id,
                            client_id,
                            &instance,
                            &plugin_env,
                            &event,
                            *rows,
                            *columns,
                            &mut plugin_bytes,
                        )?;
                    }
                    let _ = self
                        .senders
                        .send_to_screen(ScreenInstruction::PluginBytes(plugin_bytes));
                }
            }
        }
        if let Some((rows, columns)) = self.cached_resizes_for_pending_plugins.remove(&plugin_id) {
            self.resize_plugin(plugin_id, columns, rows)?;
        }
        self.loading_plugins.remove(&plugin_id);
        Ok(())
    }
    pub fn remove_client(&mut self, client_id: ClientId) {
        self.connected_clients
            .lock()
            .unwrap()
            .retain(|c| c != &client_id);
    }
    pub fn cleanup(&mut self) {
        for (_plugin_id, loading_plugin_task) in self.loading_plugins.drain() {
            drop(loading_plugin_task.cancel());
        }
    }
}

fn load_plugin_instance(instance: &mut Instance) -> Result<()> {
    let err_context = || format!("failed to load plugin from instance {instance:#?}");

    let load_function = instance
        .exports
        .get_function("_start")
        .with_context(err_context)?;
    // This eventually calls the `.load()` method
    load_function.call(&[]).with_context(err_context)?;
    Ok(())
}

pub(crate) fn zellij_exports(store: &Store, plugin_env: &PluginEnv) -> ImportObject {
    macro_rules! zellij_export {
        ($($host_function:ident),+ $(,)?) => {
            imports! {
                "zellij" => {
                    $(stringify!($host_function) =>
                        Function::new_native_with_env(store, plugin_env.clone(), $host_function),)+
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
        host_switch_tab_to,
        host_set_timeout,
        host_exec_cmd,
        host_report_panic,
    }
}

fn host_subscribe(plugin_env: &PluginEnv) {
    wasi_read_object::<HashSet<EventType>>(&plugin_env.wasi_env)
        .and_then(|new| {
            plugin_env.subscriptions.lock().to_anyhow()?.extend(new);
            Ok(())
        })
        .with_context(|| format!("failed to subscribe for plugin {}", plugin_env.name()))
        .fatal();
}

fn host_unsubscribe(plugin_env: &PluginEnv) {
    wasi_read_object::<HashSet<EventType>>(&plugin_env.wasi_env)
        .and_then(|old| {
            plugin_env
                .subscriptions
                .lock()
                .to_anyhow()?
                .retain(|k| !old.contains(k));
            Ok(())
        })
        .with_context(|| format!("failed to unsubscribe for plugin {}", plugin_env.name()))
        .fatal();
}

fn host_set_selectable(plugin_env: &PluginEnv, selectable: i32) {
    match plugin_env.plugin.run {
        PluginType::Pane(Some(tab_index)) => {
            let selectable = selectable != 0;
            plugin_env
                .senders
                .send_to_screen(ScreenInstruction::SetSelectable(
                    PaneId::Plugin(plugin_env.plugin_id),
                    selectable,
                    tab_index,
                ))
                .with_context(|| {
                    format!(
                        "failed to set plugin {} selectable from plugin {}",
                        selectable,
                        plugin_env.name()
                    )
                })
                .non_fatal();
        },
        _ => {
            debug!(
                "{} - Calling method 'host_set_selectable' does nothing for headless plugins",
                plugin_env.plugin.location
            )
        },
    }
}

fn host_get_plugin_ids(plugin_env: &PluginEnv) {
    let ids = PluginIds {
        plugin_id: plugin_env.plugin_id,
        zellij_pid: process::id(),
    };
    wasi_write_object(&plugin_env.wasi_env, &ids)
        .with_context(|| {
            format!(
                "failed to query plugin IDs from host for plugin {}",
                plugin_env.name()
            )
        })
        .non_fatal();
}

fn host_get_zellij_version(plugin_env: &PluginEnv) {
    wasi_write_object(&plugin_env.wasi_env, VERSION)
        .with_context(|| {
            format!(
                "failed to request zellij version from host for plugin {}",
                plugin_env.name()
            )
        })
        .non_fatal();
}

fn host_open_file(plugin_env: &PluginEnv) {
    wasi_read_object::<PathBuf>(&plugin_env.wasi_env)
        .and_then(|path| {
            plugin_env
                .senders
                .send_to_pty(PtyInstruction::SpawnTerminal(
                    Some(TerminalAction::OpenFile(path, None, None)),
                    None,
                    None,
                    ClientOrTabIndex::TabIndex(plugin_env.tab_index),
                ))
        })
        .with_context(|| {
            format!(
                "failed to open file on host from plugin {}",
                plugin_env.name()
            )
        })
        .non_fatal();
}

fn host_switch_tab_to(plugin_env: &PluginEnv, tab_idx: u32) {
    plugin_env
        .senders
        .send_to_screen(ScreenInstruction::GoToTab(
            tab_idx,
            Some(plugin_env.client_id),
        ))
        .with_context(|| {
            format!(
                "failed to switch host to tab {tab_idx} from plugin {}",
                plugin_env.name()
            )
        })
        .non_fatal();
}

fn host_set_timeout(plugin_env: &PluginEnv, secs: f64) {
    // There is a fancy, high-performance way to do this with zero additional threads:
    // If the plugin thread keeps a BinaryHeap of timer structs, it can manage multiple and easily `.peek()` at the
    // next time to trigger in O(1) time. Once the wake-up time is known, the `wasm` thread can use `recv_timeout()`
    // to wait for an event with the timeout set to be the time of the next wake up. If events come in in the meantime,
    // they are handled, but if the timeout triggers, we replace the event from `recv()` with an
    // `Update(pid, TimerEvent)` and pop the timer from the Heap (or reschedule it). No additional threads for as many
    // timers as we'd like.
    //
    // But that's a lot of code, and this is a few lines:
    let send_plugin_instructions = plugin_env.senders.to_plugin.clone();
    let update_target = Some(plugin_env.plugin_id);
    let client_id = plugin_env.client_id;
    let plugin_name = plugin_env.name();
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

fn host_exec_cmd(plugin_env: &PluginEnv) {
    let err_context = || {
        format!(
            "failed to execute command on host for plugin '{}'",
            plugin_env.name()
        )
    };

    let mut cmdline: Vec<String> = wasi_read_object(&plugin_env.wasi_env)
        .with_context(err_context)
        .fatal();
    let command = cmdline.remove(0);

    // Bail out if we're forbidden to run command
    if !plugin_env.plugin._allow_exec_host_cmd {
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

// Custom panic handler for plugins.
//
// This is called when a panic occurs in a plugin. Since most panics will likely originate in the
// code trying to deserialize an `Event` upon a plugin state update, we read some panic message,
// formatted as string from the plugin.
fn host_report_panic(plugin_env: &PluginEnv) {
    let msg = wasi_read_string(&plugin_env.wasi_env)
        .with_context(|| format!("failed to report panic for plugin '{}'", plugin_env.name()))
        .fatal();
    panic!("{}", msg);
}

// Helper Functions ---------------------------------------------------------------------------------------------------

pub fn wasi_read_string(wasi_env: &WasiEnv) -> Result<String> {
    let err_context = || format!("failed to read string from WASI env '{wasi_env:?}'");

    let mut buf = String::new();
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
        .and_then(|wasi_file| wasi_file.read_to_string(&mut buf).map_err(anyError::new))
        .with_context(err_context)?;
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

pub fn apply_event_to_plugin(
    plugin_id: u32,
    client_id: ClientId,
    instance: &Instance,
    plugin_env: &PluginEnv,
    event: &Event,
    rows: usize,
    columns: usize,
    plugin_bytes: &mut Vec<(u32, ClientId, Vec<u8>)>,
) -> Result<()> {
    let err_context = || format!("Failed to apply event to plugin {plugin_id}");
    let update = instance
        .exports
        .get_function("update")
        .with_context(err_context)?;
    wasi_write_object(&plugin_env.wasi_env, &event).with_context(err_context)?;
    let update_return =
        update
            .call(&[])
            .or_else::<anyError, _>(|e| match e.downcast::<serde_json::Error>() {
                Ok(_) => panic!(
                    "{}",
                    anyError::new(VersionMismatchError::new(
                        VERSION,
                        "Unavailable",
                        &plugin_env.plugin.path,
                        plugin_env.plugin.is_builtin(),
                    ))
                ),
                Err(e) => Err(e).with_context(err_context),
            })?;
    let should_render = match update_return.get(0) {
        Some(Value::I32(n)) => *n == 1,
        _ => false,
    };

    if rows > 0 && columns > 0 && should_render {
        let rendered_bytes = instance
            .exports
            .get_function("render")
            .map_err(anyError::new)
            .and_then(|render| {
                render
                    .call(&[Value::I32(rows as i32), Value::I32(columns as i32)])
                    .map_err(anyError::new)
            })
            .and_then(|_| wasi_read_string(&plugin_env.wasi_env))
            .with_context(err_context)?;
        plugin_bytes.push((plugin_id, client_id, rendered_bytes.as_bytes().to_vec()));
    }
    Ok(())
}
