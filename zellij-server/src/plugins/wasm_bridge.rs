use super::PluginInstruction;
use highway::{HighwayHash, PortableHash};
use log::{debug, info, warn};
use semver::Version;
use serde::{de::DeserializeOwned, Serialize};
use zellij_utils::async_std::task::{self, JoinHandle};
use std::{
    collections::{HashMap, HashSet},
    fmt, fs,
    path::PathBuf,
    process,
    str::FromStr,
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};
use url::Url;
use wasmer::{
    imports, ChainableNamedResolver, Function, ImportObject, Instance, Module, Store, Value,
    WasmerEnv,
};
use wasmer_wasi::{Pipe, WasiEnv, WasiState};

use crate::{
    logging_pipe::LoggingPipe,
    panes::PaneId,
    pty::{ClientOrTabIndex, PtyInstruction},
    screen::ScreenInstruction,
    thread_bus::ThreadSenders,
    ClientId,
};

use zellij_utils::{
    consts::{VERSION, ZELLIJ_CACHE_DIR, ZELLIJ_TMP_DIR},
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
    plugin_own_data_dir: PathBuf,
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

type PluginMap = HashMap<(u32, ClientId), (Instance, PluginEnv, (usize, usize))>; // u32 =>
                                                                                  // plugin_id,
                                                                                  // (usize, usize)
                                                                                  // => (rows,
                                                                                  // columns)

pub struct WasmBridge {
    connected_clients: Arc<Mutex<Vec<ClientId>>>,
    plugins: PluginsConfig,
    senders: ThreadSenders,
    store: Arc<Mutex<Store>>,
    plugin_dir: PathBuf,
    plugin_cache: Arc<Mutex<HashMap<PathBuf, Module>>>,
    plugin_map: Arc<Mutex<PluginMap>>,
    next_plugin_id: u32,
    cached_events_for_pending_plugins: HashMap<(u32, ClientId), Vec<Event>>, // u32 is the plugin id
    cached_resizes_for_pending_plugins: HashMap<(u32, ClientId), (usize, usize)>, // (rows, columns)
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
        let plugin_cache: Arc<Mutex<HashMap<PathBuf, Module>>> = Arc::new(Mutex::new(HashMap::new()));
        let store = Arc::new(Mutex::new(store));
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

        self.next_plugin_id += 1;
        self.cached_events_for_pending_plugins.insert((plugin_id, client_id), vec![]);
        self.cached_resizes_for_pending_plugins.insert((plugin_id, client_id), (0, 0));
        task::spawn({
            let plugin_dir = self.plugin_dir.clone();
            let plugin_cache = self.plugin_cache.clone();
            let senders = self.senders.clone();
            let store = self.store.clone();
            let plugin_map = self.plugin_map.clone();
            let connected_clients = self.connected_clients.clone();
            log::info!("calling start_plugin_async for plugin_id: {:?}", plugin_id);
            async move {
                let _ = senders.send_to_screen(ScreenInstruction::PluginBytes(vec![(plugin_id, client_id, format!("Starting plugin {plugin_id}...").as_bytes().to_vec())]));
                let _ = start_plugin_async(
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
                        connected_clients,
                    ).with_context(err_context);
                log::info!("done loading plugin {:?}!", plugin_id);
                let _ = senders.send_to_screen(ScreenInstruction::PluginBytes(vec![(plugin_id, client_id, format!("Done loading plugin {plugin_id}").as_bytes().to_vec())]));
                let _ = senders.send_to_plugin(PluginInstruction::ApplyCachedEvents(plugin_id, client_id));
            }
            // TODO: error handling
        });
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
            let zellij = zellij_exports(&*self.store.lock().unwrap(), &new_plugin_env);
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
        for ((plugin_id, client_id), (current_rows, current_columns)) in self.cached_resizes_for_pending_plugins.iter_mut() {
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
//         for (pid, client_id, event) in updates.iter() {
//             log::info!("update_plugins, pid: {:?}, client_id: {:?}", pid, client_id);
//             log::info!("event: {:?}", event);
//         }
        let err_context = || "failed to update plugin state".to_string();

        let mut plugin_map = self.plugin_map.lock().unwrap();
        let mut plugin_bytes = vec![];
        for (pid, cid, event) in updates.drain(..) {
            for (&(plugin_id, client_id), (instance, plugin_env, (rows, columns))) in
                &*plugin_map
            {
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
                    apply_event_to_plugin(plugin_id, client_id, &instance, &plugin_env, &event, *rows, *columns, &mut plugin_bytes)?;
                }
            }
            for ((plugin_id, client_id), mut cached_events) in self.cached_events_for_pending_plugins.iter_mut() {
                    if pid.is_none() && cid.is_none()
                        || (pid.is_none() && cid.as_ref() == Some(client_id))
                        || (cid.is_none() && pid.as_ref() == Some(plugin_id))
                        || cid.as_ref() == Some(client_id) && pid.as_ref() == Some(plugin_id)
                    {
                        cached_events.push(event.clone());
                    }
            }
        }
       let _ = self
            .senders
            .send_to_screen(ScreenInstruction::PluginBytes(plugin_bytes));
        Ok(())
    }
    pub fn apply_cached_events(&mut self, plugin_id: u32, client_id: ClientId) -> Result<()> {
        let err_context = || format!("Failed to apply cached events to plugin {plugin_id}");
        if let Some(mut events) = self.cached_events_for_pending_plugins.remove(&(plugin_id, client_id)) {
            let mut plugin_map = self.plugin_map.lock().unwrap();
            let mut plugin_bytes = vec![];
            if let Some((instance, plugin_env, (rows, columns))) = plugin_map.get_mut(&(plugin_id, client_id)) {
                let subs = plugin_env
                   .subscriptions
                   .lock()
                   .to_anyhow()
                   .with_context(err_context)?;
                for event in events {
                    let event_type =
                        EventType::from_str(&event.to_string()).with_context(err_context)?;
                    if !subs.contains(&event_type) {
                        continue;
                    }
                    apply_event_to_plugin(plugin_id, client_id, &instance, &plugin_env, &event, *rows, *columns, &mut plugin_bytes)?;
                }
                let _ = self
                    .senders
                    .send_to_screen(ScreenInstruction::PluginBytes(plugin_bytes));
            }
        }
        if let Some((rows, columns)) = self.cached_resizes_for_pending_plugins.remove(&(plugin_id, client_id)) {
            self.resize_plugin(plugin_id, columns, rows);
        }
        Ok(())
    }
    pub fn remove_client(&mut self, client_id: ClientId) {
        self.connected_clients.lock().unwrap().retain(|c| c != &client_id);
    }
}

// Returns `Ok` if the plugin version matches the zellij version.
// Returns an `Err` otherwise.
fn assert_plugin_version(instance: &Instance, plugin_env: &PluginEnv) -> Result<()> {
    let err_context = || {
        format!(
            "failed to determine plugin version for plugin {}",
            plugin_env.plugin.path.display()
        )
    };

    let plugin_version_func = match instance.exports.get_function("plugin_version") {
        Ok(val) => val,
        Err(_) => {
            return Err(anyError::new(VersionMismatchError::new(
                VERSION,
                "Unavailable",
                &plugin_env.plugin.path,
                plugin_env.plugin.is_builtin(),
            )))
        },
    };

    let plugin_version = plugin_version_func
        .call(&[])
        .map_err(anyError::new)
        .and_then(|_| wasi_read_string(&plugin_env.wasi_env))
        .and_then(|string| Version::parse(&string).context("failed to parse plugin version"))
        .with_context(err_context)?;
    let zellij_version = Version::parse(VERSION)
        .context("failed to parse zellij version")
        .with_context(err_context)?;
    if plugin_version != zellij_version {
        return Err(anyError::new(VersionMismatchError::new(
            VERSION,
            &plugin_version.to_string(),
            &plugin_env.plugin.path,
            plugin_env.plugin.is_builtin(),
        )));
    }

    Ok(())
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

fn start_plugin_async(
    plugin_id: u32,
    client_id: ClientId,
    plugin: &PluginConfig,
    tab_index: usize,
    plugin_dir: PathBuf,
    plugin_cache: Arc<Mutex<HashMap<PathBuf, Module>>>,
    senders: ThreadSenders,
    store: Arc<Mutex<Store>>,
    plugin_map: Arc<Mutex<PluginMap>>,
    size: Size,
    connected_clients: Arc<Mutex<Vec<ClientId>>>,
) -> Result<()> {
    // TODO: CONTINUE HERE - keep improving the plugin loading messages, and then try and see if we
    // can solve some of the deadlocks (store?)
    let err_context = || format!("failed to start plugin {plugin:#?} for client {client_id}");
    let mut loading_messages = String::new();
    let plugin_own_data_dir = ZELLIJ_CACHE_DIR.join(Url::from(&plugin.location).to_string());
    create_plugin_fs_entries(&plugin_own_data_dir)?;

    loading_messages.push_str(&format!("Attempting to load plugin {plugin_id} from memory... "));
    let _ = senders.send_to_screen(ScreenInstruction::PluginBytes(vec![(plugin_id, client_id, loading_messages.as_bytes().to_vec())]));
    let (module, cache_hit) = load_module_from_memory(&mut *plugin_cache.lock().unwrap(), &plugin.path);

    let module = match module {
        Some(module) => {
            loading_messages.push_str(&format!("SUCCESS\n\r"));
            let _ = senders.send_to_screen(ScreenInstruction::PluginBytes(vec![(plugin_id, client_id, loading_messages.as_bytes().to_vec())]));
            module
        }
        None => {
            loading_messages.push_str(&format!("NOT FOUND\n\r"));
            loading_messages.push_str(&format!("Attempting to load plugin {plugin_id} from cache... "));
            let _ = senders.send_to_screen(ScreenInstruction::PluginBytes(vec![(plugin_id, client_id, loading_messages.as_bytes().to_vec())]));

            let (wasm_bytes, cached_path) = plugin_bytes_and_cache_path(&plugin, &plugin_dir);
            let timer = std::time::Instant::now();
            let mut store = store.lock().unwrap();
            match load_module_from_hd_cache(&mut store, &plugin.path, &timer, &cached_path) {
                Ok(module) => {
                    loading_messages.push_str(&format!("SUCCESS\n\r"));
                    let _ = senders.send_to_screen(ScreenInstruction::PluginBytes(vec![(plugin_id, client_id, loading_messages.as_bytes().to_vec())]));
                    module
                },
                Err(_e) => {
                    loading_messages.push_str(&format!("NOT FOUND\n\r"));
                    loading_messages.push_str(&format!("Compiling plugin {plugin_id}... "));
                    let _ = senders.send_to_screen(ScreenInstruction::PluginBytes(vec![(plugin_id, client_id, loading_messages.as_bytes().to_vec())]));
                    let module = compile_module(&mut store, &plugin.path, &timer, &cached_path, wasm_bytes)?;
                    loading_messages.push_str(&format!("DONE\n\r"));
                    let _ = senders.send_to_screen(ScreenInstruction::PluginBytes(vec![(plugin_id, client_id, loading_messages.as_bytes().to_vec())]));
                    module
                }
            }
        }
    };

    let (instance, plugin_env) = create_plugin_instance_and_environment(
        plugin_id,
        client_id,
        plugin,
        &module,
        tab_index,
        plugin_own_data_dir,
        senders.clone(),
        &mut *store.lock().unwrap()
    )?;

    if !cache_hit {
        // Check plugin version
        // TODO: TEST THIS!
        assert_plugin_version(&instance, &plugin_env).with_context(err_context)?;
    }

    // Only do an insert when everything went well!
    let cloned_plugin = plugin.clone();
    plugin_cache.lock().unwrap().insert(cloned_plugin.path, module);

    let mut main_user_instance = instance.clone();
    let main_user_env = plugin_env.clone();
    loading_messages.push_str(&format!("Starting plugin {plugin_id}... "));
    let _ = senders.send_to_screen(ScreenInstruction::PluginBytes(vec![(plugin_id, client_id, loading_messages.as_bytes().to_vec())]));
    load_plugin_instance(&mut main_user_instance).with_context(err_context)?;
    loading_messages.push_str(&format!("DONE\n\r"));
    loading_messages.push_str(&format!("Writing plugin {plugin_id} to cache... "));
    let _ = senders.send_to_screen(ScreenInstruction::PluginBytes(vec![(plugin_id, client_id, loading_messages.as_bytes().to_vec())]));

    plugin_map.lock().unwrap().insert(
        (plugin_id, client_id),
        (main_user_instance, main_user_env, (size.rows, size.cols)),
    );

    loading_messages.push_str(&format!("DONE\n\r"));
    let _ = senders.send_to_screen(ScreenInstruction::PluginBytes(vec![(plugin_id, client_id, loading_messages.as_bytes().to_vec())]));

    // clone plugins for the rest of the client ids if they exist
    let connected_clients = connected_clients.lock().unwrap();
    if !connected_clients.is_empty() {
        loading_messages.push_str(&format!("Cloning plugin {plugin_id} for other connected clients... "));
        let _ = senders.send_to_screen(ScreenInstruction::PluginBytes(vec![(plugin_id, client_id, loading_messages.as_bytes().to_vec())]));
    }
    for client_id in connected_clients.iter() {
        let (instance, new_plugin_env) = clone_plugin_for_client(
            &plugin_env,
            *client_id,
            &instance,
            &mut *store.lock().unwrap()
        )?;
        plugin_map.lock().unwrap().insert(
            (plugin_id, *client_id),
            (instance, new_plugin_env, (size.rows, size.cols)),
        );
    };
    if !connected_clients.is_empty() {
        loading_messages.push_str(&format!("DONE\n\r"));
        let _ = senders.send_to_screen(ScreenInstruction::PluginBytes(vec![(plugin_id, client_id, loading_messages.as_bytes().to_vec())]));
    }
    Ok(())
}

fn apply_event_to_plugin(plugin_id: u32, client_id: ClientId, instance: &Instance, plugin_env: &PluginEnv, event: &Event, rows: usize, columns: usize, plugin_bytes: &mut Vec<(u32, ClientId, Vec<u8>)>) -> Result<()> {
    let err_context = || format!("Failed to apply event to plugin {plugin_id}");
    let update = instance
        .exports
        .get_function("update")
        .with_context(err_context)?;
    wasi_write_object(&plugin_env.wasi_env, &event).with_context(err_context)?;
    let update_return = update.call(&[]).or_else::<anyError, _>(|e| {
        match e.downcast::<serde_json::Error>() {
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
        }
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
        plugin_bytes.push((
            plugin_id,
            client_id,
            rendered_bytes.as_bytes().to_vec(),
        ));
    }
    Ok(())
}

fn create_plugin_fs_entries(plugin_own_data_dir: &PathBuf) -> Result<()> {
    let err_context = || "failed to create plugin fs entries";
    // Create filesystem entries mounted into WASM.
    // We create them here to get expressive error messages in case they fail.
    fs::create_dir_all(&plugin_own_data_dir)
        .with_context(|| format!("failed to create datadir in {plugin_own_data_dir:?}"))
        .with_context(err_context)?;
    fs::create_dir_all(ZELLIJ_TMP_DIR.as_path())
        .with_context(|| format!("failed to create tmpdir at {:?}", &ZELLIJ_TMP_DIR.as_path()))
        .with_context(err_context)?;
    Ok(())
}

fn compile_module(store: &mut Store, plugin_path: &PathBuf, timer: &Instant, cached_path: &PathBuf, wasm_bytes: Vec<u8>) -> Result<Module> {
    let err_context = || "failed to recover cache dir";
    fs::create_dir_all(ZELLIJ_CACHE_DIR.to_owned())
        .map_err(anyError::new)
        .and_then(|_| {
            // compile module
            Module::new(&*store, &wasm_bytes).map_err(anyError::new)
        })
        .map(|m| {
            // serialize module to HD cache for faster loading in the future
            m.serialize_to_file(&cached_path).map_err(anyError::new)?;
            log::info!(
                "Compiled plugin '{}' in {:?}",
                plugin_path.display(),
                timer.elapsed()
            );
            Ok(m)
        })
        .with_context(err_context)?
}

fn load_module_from_hd_cache(store: &mut Store, plugin_path: &PathBuf, timer: &Instant, cached_path: &PathBuf) -> Result<Module> {
    let module = unsafe { Module::deserialize_from_file(&*store, &cached_path)? };
    log::info!(
        "Loaded plugin '{}' from cache folder at '{}' in {:?}",
        plugin_path.display(),
        ZELLIJ_CACHE_DIR.display(),
        timer.elapsed(),
    );
    Ok(module)
}

fn plugin_bytes_and_cache_path(plugin: &PluginConfig, plugin_dir: &PathBuf) -> (Vec<u8>, PathBuf) {
    let err_context = || "failed to get plugin bytes and cached path";
    // Populate plugin module cache for this plugin!
    // Is it in the cache folder already?
    if plugin._allow_exec_host_cmd {
        info!(
            "Plugin({:?}) is able to run any host command, this may lead to some security issues!",
            plugin.path
        );
    }
    // The plugins blob as stored on the filesystem
    let wasm_bytes = plugin
        .resolve_wasm_bytes(&plugin_dir)
        .with_context(err_context)
        .fatal();
    let hash: String = PortableHash::default()
        .hash256(&wasm_bytes)
        .iter()
        .map(ToString::to_string)
        .collect();
    let cached_path = ZELLIJ_CACHE_DIR.join(&hash);
    (wasm_bytes, cached_path)
}

fn load_module_from_memory(plugin_cache: &mut HashMap<PathBuf, Module>, plugin_path: &PathBuf) -> (Option<Module>, bool) {
    let module = plugin_cache.remove(plugin_path);
    let mut cache_hit = false;
    if module.is_some() {
        cache_hit = true;
        log::debug!(
            "Loaded plugin '{}' from plugin cache",
            plugin_path.display()
        );
    }
    (module, cache_hit)
}

fn load_module_from_hd_or_compile_module(plugin: &PluginConfig, plugin_dir: &PathBuf, store: &mut Store) -> Result<Module> {
    let (wasm_bytes, cached_path) = plugin_bytes_and_cache_path(&plugin, &plugin_dir);
    let timer = std::time::Instant::now();
    load_module_from_hd_cache(store, &plugin.path, &timer, &cached_path)
        .or_else(|e| compile_module(&mut *store, &plugin.path, &timer, &cached_path, wasm_bytes))
}

fn create_plugin_instance_and_environment(
    plugin_id: u32,
    client_id: ClientId,
    plugin: &PluginConfig,
    module: &Module,
    tab_index: usize,
    plugin_own_data_dir: PathBuf,
    senders: ThreadSenders,
    store: &mut Store,
) -> Result<(Instance, PluginEnv)> {
    let err_context = || format!("Failed to create instance and plugin env for plugin {plugin_id}");
    let mut wasi_env = WasiState::new("Zellij")
        .env("CLICOLOR_FORCE", "1")
        .map_dir("/host", ".")
        .and_then(|wasi| wasi.map_dir("/data", &plugin_own_data_dir))
        .and_then(|wasi| wasi.map_dir("/tmp", ZELLIJ_TMP_DIR.as_path()))
        .and_then(|wasi| {
            wasi.stdin(Box::new(Pipe::new()))
                .stdout(Box::new(Pipe::new()))
                .stderr(Box::new(LoggingPipe::new(
                    &plugin.location.to_string(),
                    plugin_id,
                )))
                .finalize()
        })
        .with_context(err_context)?;
    let wasi = wasi_env.import_object(&module).with_context(err_context)?;

    let mut mut_plugin = plugin.clone();
    mut_plugin.set_tab_index(tab_index);
    let plugin_env = PluginEnv {
        plugin_id,
        client_id,
        plugin: mut_plugin,
        senders: senders.clone(),
        wasi_env,
        subscriptions: Arc::new(Mutex::new(HashSet::new())),
        plugin_own_data_dir,
        tab_index,
    };
    // need: wasi, plugin_env

    let zellij = zellij_exports(&store, &plugin_env);
    let instance =
        Instance::new(&module, &zellij.chain_back(wasi)).with_context(err_context)?;
    Ok((instance, plugin_env))
}

fn clone_plugin_for_client(
    plugin_env: &PluginEnv,
    client_id: ClientId,
    instance: &Instance,
    store: &Store
) -> Result<(Instance, PluginEnv)> {
    let err_context = || format!("Failed to clone plugin for client {client_id}");
    let mut new_plugin_env = plugin_env.clone();
    new_plugin_env.client_id = client_id;
    let module = instance.module().clone();
    let wasi = new_plugin_env
        .wasi_env
        .import_object(&module)
        .with_context(err_context)?;
    let start = Instant::now();
    let zellij = zellij_exports(store, &new_plugin_env);
    let mut instance =
        Instance::new(&module, &zellij.chain_back(wasi)).with_context(err_context)?;
    load_plugin_instance(&mut instance).with_context(err_context)?;
    Ok((instance, new_plugin_env))
}
