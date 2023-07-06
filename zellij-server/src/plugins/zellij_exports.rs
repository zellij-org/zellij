use super::PluginInstruction;
use crate::plugins::plugin_map::{PluginEnv, Subscriptions};
use crate::plugins::wasm_bridge::handle_plugin_crash;
use crate::route::route_action;
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
use wasmer::{imports, AsStoreMut, Function, FunctionEnv, FunctionEnvMut, Imports, Store};
use wasmer_wasi::WasiEnv;

use url::Url;

use crate::{panes::PaneId, screen::ScreenInstruction};

use zellij_utils::{
    consts::VERSION,
    data::{Direction, Event, EventType, InputMode, PluginIds, Resize},
    errors::prelude::*,
    input::{
        actions::Action,
        command::{RunCommand, RunCommandAction, TerminalAction},
        layout::{Layout, RunPlugin, RunPluginLocation},
        plugins::PluginType,
    },
    serde,
};

macro_rules! apply_action {
    ($action:ident, $error_message:ident, $env: ident) => {
        if let Err(e) = route_action(
            $action,
            $env.plugin_env.client_id,
            $env.plugin_env.senders.clone(),
            $env.plugin_env.capabilities.clone(),
            $env.plugin_env.client_attributes.clone(),
            $env.plugin_env.default_shell.clone(),
            $env.plugin_env.default_layout.clone(),
        ) {
            log::error!("{}: {:?}", $error_message(), e);
        }
    };
}

pub fn zellij_exports(
    store: Arc<Mutex<Store>>,
    plugin_env: &PluginEnv,
    subscriptions: &Arc<Mutex<Subscriptions>>,
) -> Imports {
    let mut store = store.lock().unwrap();
    let function_env = FunctionEnv::new(
        &mut store.as_store_mut(),
        ForeignFunctionEnv::new(plugin_env, subscriptions),
    );

    macro_rules! zellij_export {
        ($($host_function:ident),+ $(,)?) => {
            imports! {
                "zellij" => {
                    $(stringify!($host_function) =>
                        Function::new_typed_with_env(&mut store.as_store_mut(), &function_env, $host_function),)+
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
        host_open_file_floating,
        host_open_file_with_line,
        host_open_file_with_line_floating,
        host_open_terminal,
        host_open_terminal_floating,
        host_open_command_pane,
        host_open_command_pane_floating,
        host_switch_tab_to,
        host_set_timeout,
        host_exec_cmd,
        host_report_panic,
        host_post_message_to,
        host_post_message_to_plugin,
        host_hide_self,
        host_show_self,
        host_switch_to_mode,
        host_new_tabs_with_layout,
        host_new_tab,
        host_go_to_next_tab,
        host_go_to_previous_tab,
        host_resize,
        host_resize_with_direction,
        host_focus_next_pane,
        host_focus_previous_pane,
        host_move_focus,
        host_move_focus_or_tab,
        host_detach,
        host_edit_scrollback,
        host_write,
        host_write_chars,
        host_toggle_tab,
        host_move_pane,
        host_move_pane_with_direction,
        host_clear_screen,
        host_scroll_up,
        host_scroll_down,
        host_scroll_to_top,
        host_scroll_to_bottom,
        host_page_scroll_up,
        host_page_scroll_down,
        host_toggle_focus_fullscreen,
        host_toggle_pane_frames,
        host_toggle_pane_embed_or_eject,
        host_undo_rename_pane,
        host_close_focus,
        host_toggle_active_tab_sync,
        host_close_focused_tab,
        host_undo_rename_tab,
        host_quit_zellij,
        host_previous_swap_layout,
        host_next_swap_layout,
        host_go_to_tab_name,
        host_focus_or_create_tab,
        host_go_to_tab,
        host_start_or_reload_plugin,
        host_close_terminal_pane,
        host_close_plugin_pane,
        host_focus_terminal_pane,
        host_focus_plugin_pane,
        host_rename_terminal_pane,
        host_rename_plugin_pane,
        host_rename_tab,
    }
}

#[derive(Clone)]
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

fn host_subscribe(env: FunctionEnvMut<ForeignFunctionEnv>) {
    let env = env.data();
    wasi_read_object::<HashSet<EventType>>(&env.plugin_env.wasi_env)
        .and_then(|new| {
            env.subscriptions.lock().to_anyhow()?.extend(new.clone());
            Ok(new)
        })
        .and_then(|new| {
            env.plugin_env
                .senders
                .send_to_plugin(PluginInstruction::PluginSubscribedToEvents(
                    env.plugin_env.plugin_id,
                    env.plugin_env.client_id,
                    new,
                ))
        })
        .with_context(|| format!("failed to subscribe for plugin {}", env.plugin_env.name()))
        .fatal();
}

fn host_unsubscribe(env: FunctionEnvMut<ForeignFunctionEnv>) {
    let env = env.data();
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

fn host_set_selectable(env: FunctionEnvMut<ForeignFunctionEnv>, selectable: i32) {
    let env = env.data();
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

fn host_get_plugin_ids(env: FunctionEnvMut<ForeignFunctionEnv>) {
    let env = env.data();
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

fn host_get_zellij_version(env: FunctionEnvMut<ForeignFunctionEnv>) {
    let env = env.data();
    wasi_write_object(&env.plugin_env.wasi_env, VERSION)
        .with_context(|| {
            format!(
                "failed to request zellij version from host for plugin {}",
                env.plugin_env.name()
            )
        })
        .non_fatal();
}

fn host_open_file(env: FunctionEnvMut<ForeignFunctionEnv>) {
    let env = env.data();
    wasi_read_object::<PathBuf>(&env.plugin_env.wasi_env)
        .and_then(|path| {
            let error_msg = || {
                format!(
                    "failed to open floating file in plugin {}",
                    env.plugin_env.name()
                )
            };
            let floating = false;
            let action = Action::EditFile(path, None, None, None, floating); // TODO: add cwd
            apply_action!(action, error_msg, env);
            Ok(())
        })
        .with_context(|| {
            format!(
                "failed to open file on host from plugin {}",
                env.plugin_env.name()
            )
        })
        .non_fatal();
}

fn host_open_file_floating(env: FunctionEnvMut<ForeignFunctionEnv>) {
    let env = env.data();
    wasi_read_object::<PathBuf>(&env.plugin_env.wasi_env)
        .and_then(|path| {
            let error_msg = || format!("failed to open file in plugin {}", env.plugin_env.name());
            let floating = true;
            let action = Action::EditFile(path, None, None, None, floating); // TODO: add cwd
            apply_action!(action, error_msg, env);
            Ok(())
        })
        .with_context(|| {
            format!(
                "failed to open file on host from plugin {}",
                env.plugin_env.name()
            )
        })
        .non_fatal();
}

fn host_open_file_with_line(env: FunctionEnvMut<ForeignFunctionEnv>) {
    let env = env.data();
    wasi_read_object::<(PathBuf, usize)>(&env.plugin_env.wasi_env)
        .and_then(|(path, line)| {
            let error_msg = || format!("failed to open file in plugin {}", env.plugin_env.name());
            let floating = false;
            let action = Action::EditFile(path, Some(line), None, None, floating); // TODO: add cwd
            apply_action!(action, error_msg, env);
            Ok(())
        })
        .with_context(|| {
            format!(
                "failed to open file on host from plugin {}",
                env.plugin_env.name()
            )
        })
        .non_fatal();
}

fn host_open_file_with_line_floating(env: FunctionEnvMut<ForeignFunctionEnv>) {
    let env = env.data();
    wasi_read_object::<(PathBuf, usize)>(&env.plugin_env.wasi_env)
        .and_then(|(path, line)| {
            let error_msg = || format!("failed to open file in plugin {}", env.plugin_env.name());
            let floating = true;
            let action = Action::EditFile(path, Some(line), None, None, floating); // TODO: add cwd
            apply_action!(action, error_msg, env);
            Ok(())
        })
        .with_context(|| {
            format!(
                "failed to open file on host from plugin {}",
                env.plugin_env.name()
            )
        })
        .non_fatal();
}

fn host_open_terminal(env: FunctionEnvMut<ForeignFunctionEnv>) {
    let env = env.data();
    wasi_read_object::<PathBuf>(&env.plugin_env.wasi_env)
        .and_then(|path| {
            let error_msg = || format!("failed to open file in plugin {}", env.plugin_env.name());
            let mut default_shell = env
                .plugin_env
                .default_shell
                .clone()
                .unwrap_or_else(|| TerminalAction::RunCommand(RunCommand::default()));
            default_shell.change_cwd(path);
            let run_command_action: Option<RunCommandAction> = match default_shell {
                TerminalAction::RunCommand(run_command) => Some(run_command.into()),
                _ => None,
            };
            let action = Action::NewTiledPane(None, run_command_action, None);
            apply_action!(action, error_msg, env);
            Ok(())
        })
        .with_context(|| {
            format!(
                "failed to open file on host from plugin {}",
                env.plugin_env.name()
            )
        })
        .non_fatal();
}

fn host_open_terminal_floating(env: FunctionEnvMut<ForeignFunctionEnv>) {
    let env = env.data();
    wasi_read_object::<PathBuf>(&env.plugin_env.wasi_env)
        .and_then(|path| {
            let error_msg = || format!("failed to open file in plugin {}", env.plugin_env.name());
            let mut default_shell = env
                .plugin_env
                .default_shell
                .clone()
                .unwrap_or_else(|| TerminalAction::RunCommand(RunCommand::default()));
            default_shell.change_cwd(path);
            let run_command_action: Option<RunCommandAction> = match default_shell {
                TerminalAction::RunCommand(run_command) => Some(run_command.into()),
                _ => None,
            };
            let action = Action::NewFloatingPane(run_command_action, None);
            apply_action!(action, error_msg, env);
            Ok(())
        })
        .with_context(|| {
            format!(
                "failed to open file on host from plugin {}",
                env.plugin_env.name()
            )
        })
        .non_fatal();
}

fn host_open_command_pane(env: FunctionEnvMut<ForeignFunctionEnv>) {
    let env = env.data();
    let error_msg = || format!("failed to run command in plugin {}", env.plugin_env.name());
    wasi_read_object::<(PathBuf, Vec<String>)>(&env.plugin_env.wasi_env)
        .and_then(|(command, args)| {
            let cwd = None;
            let direction = None;
            let hold_on_close = true;
            let hold_on_start = false;
            let name = None;
            let run_command_action = RunCommandAction {
                command,
                args,
                cwd,
                direction,
                hold_on_close,
                hold_on_start,
            };
            let action = Action::NewTiledPane(direction, Some(run_command_action), name);
            apply_action!(action, error_msg, env);
            Ok(())
        })
        .with_context(error_msg)
        .non_fatal();
}

fn host_open_command_pane_floating(env: FunctionEnvMut<ForeignFunctionEnv>) {
    let env = env.data();
    let error_msg = || format!("failed to run command in plugin {}", env.plugin_env.name());
    wasi_read_object::<(PathBuf, Vec<String>)>(&env.plugin_env.wasi_env)
        .and_then(|(command, args)| {
            let cwd = None;
            let direction = None;
            let hold_on_close = true;
            let hold_on_start = false;
            let name = None;
            let run_command_action = RunCommandAction {
                command,
                args,
                cwd,
                direction,
                hold_on_close,
                hold_on_start,
            };
            let action = Action::NewFloatingPane(Some(run_command_action), name);
            apply_action!(action, error_msg, env);
            Ok(())
        })
        .with_context(error_msg)
        .non_fatal();
}

fn host_switch_tab_to(env: FunctionEnvMut<ForeignFunctionEnv>, tab_idx: u32) {
    let env = env.data();
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

fn host_set_timeout(env: FunctionEnvMut<ForeignFunctionEnv>, secs: f64) {
    // There is a fancy, high-performance way to do this with zero additional threads:
    // If the plugin thread keeps a BinaryHeap of timer structs, it can manage multiple and easily `.peek()` at the
    // next time to trigger in O(1) time. Once the wake-up time is known, the `wasm` thread can use `recv_timeout()`
    // to wait for an event with the timeout set to be the time of the next wake up. If events come in in the meantime,
    // they are handled, but if the timeout triggers, we replace the event from `recv()` with an
    // `Update(pid, TimerEvent)` and pop the timer from the Heap (or reschedule it). No additional threads for as many
    // timers as we'd like.
    //
    // But that's a lot of code, and this is a few lines:
    let env = env.data();
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

fn host_exec_cmd(env: FunctionEnvMut<ForeignFunctionEnv>) {
    let env = env.data();
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

fn host_post_message_to(env: FunctionEnvMut<ForeignFunctionEnv>) {
    let env = env.data();
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

fn host_post_message_to_plugin(env: FunctionEnvMut<ForeignFunctionEnv>) {
    let env = env.data();
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

fn host_hide_self(env: FunctionEnvMut<ForeignFunctionEnv>) {
    let env = env.data();
    env.plugin_env
        .senders
        .send_to_screen(ScreenInstruction::SuppressPane(
            PaneId::Plugin(env.plugin_env.plugin_id),
            env.plugin_env.client_id,
        ))
        .with_context(|| format!("failed to hide self"))
        .fatal();
}

fn host_show_self(env: FunctionEnvMut<ForeignFunctionEnv>, should_float_if_hidden: i32) {
    let env = env.data();
    let should_float_if_hidden = should_float_if_hidden != 0;
    let action = Action::FocusPluginPaneWithId(env.plugin_env.plugin_id, should_float_if_hidden);
    let error_msg = || format!("Failed to show self for plugin");
    apply_action!(action, error_msg, env);
}

fn host_switch_to_mode(env: FunctionEnvMut<ForeignFunctionEnv>) {
    let env = env.data();
    wasi_read_object::<InputMode>(&env.plugin_env.wasi_env)
        .and_then(|input_mode| {
            let action = Action::SwitchToMode(input_mode);
            let error_msg = || {
                format!(
                    "failed to switch to mode in plugin {}",
                    env.plugin_env.name()
                )
            };
            apply_action!(action, error_msg, env);
            Ok(())
        })
        .with_context(|| format!("failed to subscribe for plugin {}", env.plugin_env.name()))
        .fatal();
}

fn host_new_tabs_with_layout(env: FunctionEnvMut<ForeignFunctionEnv>) {
    let env = env.data();
    wasi_read_string(&env.plugin_env.wasi_env)
        .and_then(|raw_layout| {
            Layout::from_str(
                &raw_layout,
                format!("Layout from plugin: {}", env.plugin_env.name()),
                None,
                None,
            )
            .map_err(|e| anyhow!("Failed to parse layout: {:?}", e))
        }) // TODO: cwd?
        .and_then(|layout| {
            let mut tabs_to_open = vec![];
            let tabs = layout.tabs();
            if tabs.is_empty() {
                let swap_tiled_layouts = Some(layout.swap_tiled_layouts.clone());
                let swap_floating_layouts = Some(layout.swap_floating_layouts.clone());
                let action = Action::NewTab(
                    layout.template.as_ref().map(|t| t.0.clone()),
                    layout.template.map(|t| t.1).unwrap_or_default(),
                    swap_tiled_layouts,
                    swap_floating_layouts,
                    None,
                );
                tabs_to_open.push(action);
            } else {
                for (tab_name, tiled_pane_layout, floating_pane_layout) in layout.tabs() {
                    let swap_tiled_layouts = Some(layout.swap_tiled_layouts.clone());
                    let swap_floating_layouts = Some(layout.swap_floating_layouts.clone());
                    let action = Action::NewTab(
                        Some(tiled_pane_layout),
                        floating_pane_layout,
                        swap_tiled_layouts,
                        swap_floating_layouts,
                        tab_name,
                    );
                    tabs_to_open.push(action);
                }
            }
            for action in tabs_to_open {
                let error_msg = || format!("Failed to create layout tab");
                apply_action!(action, error_msg, env);
            }
            Ok(())
        })
        .non_fatal();
}

fn host_new_tab(env: FunctionEnvMut<ForeignFunctionEnv>) {
    let env = env.data();
    let action = Action::NewTab(None, vec![], None, None, None);
    let error_msg = || format!("Failed to open new tab");
    apply_action!(action, error_msg, env);
}

fn host_go_to_next_tab(env: FunctionEnvMut<ForeignFunctionEnv>) {
    let env = env.data();
    let action = Action::GoToNextTab;
    let error_msg = || format!("Failed to go to next tab");
    apply_action!(action, error_msg, env);
}

fn host_go_to_previous_tab(env: FunctionEnvMut<ForeignFunctionEnv>) {
    let env = env.data();
    let action = Action::GoToPreviousTab;
    let error_msg = || format!("Failed to go to previous tab");
    apply_action!(action, error_msg, env);
}

fn host_resize(env: FunctionEnvMut<ForeignFunctionEnv>) {
    let env = env.data();
    let error_msg = || format!("failed to resize in plugin {}", env.plugin_env.name());
    wasi_read_object::<Resize>(&env.plugin_env.wasi_env)
        .and_then(|resize| {
            let action = Action::Resize(resize, None);
            apply_action!(action, error_msg, env);
            Ok(())
        })
        .with_context(error_msg)
        .fatal();
}

fn host_resize_with_direction(env: FunctionEnvMut<ForeignFunctionEnv>) {
    let env = env.data();
    let error_msg = || format!("failed to resize in plugin {}", env.plugin_env.name());
    wasi_read_object::<(Resize, Direction)>(&env.plugin_env.wasi_env)
        .and_then(|(resize, direction)| {
            let action = Action::Resize(resize, Some(direction));
            apply_action!(action, error_msg, env);
            Ok(())
        })
        .with_context(error_msg)
        .fatal();
}

fn host_focus_next_pane(env: FunctionEnvMut<ForeignFunctionEnv>) {
    let env = env.data();
    let action = Action::FocusNextPane;
    let error_msg = || format!("Failed to focus next pane");
    apply_action!(action, error_msg, env);
}

fn host_focus_previous_pane(env: FunctionEnvMut<ForeignFunctionEnv>) {
    let env = env.data();
    let action = Action::FocusPreviousPane;
    let error_msg = || format!("Failed to focus previous pane");
    apply_action!(action, error_msg, env);
}

fn host_move_focus(env: FunctionEnvMut<ForeignFunctionEnv>) {
    let env = env.data();
    let error_msg = || format!("failed to move focus in plugin {}", env.plugin_env.name());
    wasi_read_object::<Direction>(&env.plugin_env.wasi_env)
        .and_then(|direction| {
            let action = Action::MoveFocus(direction);
            apply_action!(action, error_msg, env);
            Ok(())
        })
        .with_context(error_msg)
        .fatal();
}

fn host_move_focus_or_tab(env: FunctionEnvMut<ForeignFunctionEnv>) {
    let env = env.data();
    let error_msg = || format!("failed to move focus in plugin {}", env.plugin_env.name());
    wasi_read_object::<Direction>(&env.plugin_env.wasi_env)
        .and_then(|direction| {
            let action = Action::MoveFocusOrTab(direction);
            apply_action!(action, error_msg, env);
            Ok(())
        })
        .with_context(error_msg)
        .fatal();
}

fn host_detach(env: FunctionEnvMut<ForeignFunctionEnv>) {
    let env = env.data();
    let action = Action::Detach;
    let error_msg = || format!("Failed to detach");
    apply_action!(action, error_msg, env);
}

fn host_edit_scrollback(env: FunctionEnvMut<ForeignFunctionEnv>) {
    let env = env.data();
    let action = Action::EditScrollback;
    let error_msg = || format!("Failed to edit scrollback");
    apply_action!(action, error_msg, env);
}

fn host_write(env: FunctionEnvMut<ForeignFunctionEnv>) {
    let env = env.data();
    let error_msg = || format!("failed to write in plugin {}", env.plugin_env.name());
    wasi_read_object::<Vec<u8>>(&env.plugin_env.wasi_env)
        .and_then(|bytes| {
            let action = Action::Write(bytes);
            apply_action!(action, error_msg, env);
            Ok(())
        })
        .with_context(error_msg)
        .fatal();
}

fn host_write_chars(env: FunctionEnvMut<ForeignFunctionEnv>) {
    let env = env.data();
    let error_msg = || format!("failed to write in plugin {}", env.plugin_env.name());
    wasi_read_string(&env.plugin_env.wasi_env)
        .and_then(|chars_to_write| {
            let action = Action::WriteChars(chars_to_write);
            apply_action!(action, error_msg, env);
            Ok(())
        })
        .with_context(error_msg)
        .fatal();
}

fn host_toggle_tab(env: FunctionEnvMut<ForeignFunctionEnv>) {
    let env = env.data();
    let action = Action::ToggleTab;
    let error_msg = || format!("Failed to toggle tab");
    apply_action!(action, error_msg, env);
}

fn host_move_pane(env: FunctionEnvMut<ForeignFunctionEnv>) {
    let env = env.data();
    let error_msg = || format!("failed to move pane in plugin {}", env.plugin_env.name());
    let action = Action::MovePane(None);
    apply_action!(action, error_msg, env);
}

fn host_move_pane_with_direction(env: FunctionEnvMut<ForeignFunctionEnv>) {
    let env = env.data();
    let error_msg = || format!("failed to move pane in plugin {}", env.plugin_env.name());
    wasi_read_object::<Direction>(&env.plugin_env.wasi_env)
        .and_then(|direction| {
            let action = Action::MovePane(Some(direction));
            apply_action!(action, error_msg, env);
            Ok(())
        })
        .with_context(error_msg)
        .fatal();
}

fn host_clear_screen(env: FunctionEnvMut<ForeignFunctionEnv>) {
    let env = env.data();
    let error_msg = || format!("failed to clear screen in plugin {}", env.plugin_env.name());
    let action = Action::ClearScreen;
    apply_action!(action, error_msg, env);
}
fn host_scroll_up(env: FunctionEnvMut<ForeignFunctionEnv>) {
    let env = env.data();
    let error_msg = || format!("failed to scroll up in plugin {}", env.plugin_env.name());
    let action = Action::ScrollUp;
    apply_action!(action, error_msg, env);
}

fn host_scroll_down(env: FunctionEnvMut<ForeignFunctionEnv>) {
    let env = env.data();
    let error_msg = || format!("failed to scroll down in plugin {}", env.plugin_env.name());
    let action = Action::ScrollDown;
    apply_action!(action, error_msg, env);
}

fn host_scroll_to_top(env: FunctionEnvMut<ForeignFunctionEnv>) {
    let env = env.data();
    let error_msg = || format!("failed to scroll in plugin {}", env.plugin_env.name());
    let action = Action::ScrollToTop;
    apply_action!(action, error_msg, env);
}

fn host_scroll_to_bottom(env: FunctionEnvMut<ForeignFunctionEnv>) {
    let env = env.data();
    let error_msg = || format!("failed to scroll in plugin {}", env.plugin_env.name());
    let action = Action::ScrollToBottom;
    apply_action!(action, error_msg, env);
}

fn host_page_scroll_up(env: FunctionEnvMut<ForeignFunctionEnv>) {
    let env = env.data();
    let error_msg = || format!("failed to scroll in plugin {}", env.plugin_env.name());
    let action = Action::PageScrollUp;
    apply_action!(action, error_msg, env);
}

fn host_page_scroll_down(env: FunctionEnvMut<ForeignFunctionEnv>) {
    let env = env.data();
    let error_msg = || format!("failed to scroll in plugin {}", env.plugin_env.name());
    let action = Action::PageScrollDown;
    apply_action!(action, error_msg, env);
}

fn host_toggle_focus_fullscreen(env: FunctionEnvMut<ForeignFunctionEnv>) {
    let env = env.data();
    let error_msg = || {
        format!(
            "failed to toggle full screen in plugin {}",
            env.plugin_env.name()
        )
    };
    let action = Action::ToggleFocusFullscreen;
    apply_action!(action, error_msg, env);
}

fn host_toggle_pane_frames(env: FunctionEnvMut<ForeignFunctionEnv>) {
    let env = env.data();
    let error_msg = || {
        format!(
            "failed to toggle full screen in plugin {}",
            env.plugin_env.name()
        )
    };
    let action = Action::TogglePaneFrames;
    apply_action!(action, error_msg, env);
}

fn host_toggle_pane_embed_or_eject(env: FunctionEnvMut<ForeignFunctionEnv>) {
    let env = env.data();
    let error_msg = || {
        format!(
            "failed to toggle pane embed or eject in plugin {}",
            env.plugin_env.name()
        )
    };
    let action = Action::TogglePaneEmbedOrFloating;
    apply_action!(action, error_msg, env);
}

fn host_undo_rename_pane(env: FunctionEnvMut<ForeignFunctionEnv>) {
    let env = env.data();
    let error_msg = || {
        format!(
            "failed to undo rename pane in plugin {}",
            env.plugin_env.name()
        )
    };
    let action = Action::UndoRenamePane;
    apply_action!(action, error_msg, env);
}

fn host_close_focus(env: FunctionEnvMut<ForeignFunctionEnv>) {
    let env = env.data();
    let error_msg = || {
        format!(
            "failed to close focused pane in plugin {}",
            env.plugin_env.name()
        )
    };
    let action = Action::CloseFocus;
    apply_action!(action, error_msg, env);
}

fn host_toggle_active_tab_sync(env: FunctionEnvMut<ForeignFunctionEnv>) {
    let env = env.data();
    let error_msg = || {
        format!(
            "failed to toggle active tab sync in plugin {}",
            env.plugin_env.name()
        )
    };
    let action = Action::ToggleActiveSyncTab;
    apply_action!(action, error_msg, env);
}

fn host_close_focused_tab(env: FunctionEnvMut<ForeignFunctionEnv>) {
    let env = env.data();
    let error_msg = || {
        format!(
            "failed to close active tab in plugin {}",
            env.plugin_env.name()
        )
    };
    let action = Action::CloseTab;
    apply_action!(action, error_msg, env);
}

fn host_undo_rename_tab(env: FunctionEnvMut<ForeignFunctionEnv>) {
    let env = env.data();
    let error_msg = || {
        format!(
            "failed to undo rename tab in plugin {}",
            env.plugin_env.name()
        )
    };
    let action = Action::UndoRenameTab;
    apply_action!(action, error_msg, env);
}

fn host_quit_zellij(env: FunctionEnvMut<ForeignFunctionEnv>) {
    let env = env.data();
    let error_msg = || format!("failed to quit zellij in plugin {}", env.plugin_env.name());
    let action = Action::Quit;
    apply_action!(action, error_msg, env);
}

fn host_previous_swap_layout(env: FunctionEnvMut<ForeignFunctionEnv>) {
    let env = env.data();
    let error_msg = || {
        format!(
            "failed to switch swap layout in plugin {}",
            env.plugin_env.name()
        )
    };
    let action = Action::PreviousSwapLayout;
    apply_action!(action, error_msg, env);
}

fn host_next_swap_layout(env: FunctionEnvMut<ForeignFunctionEnv>) {
    let env = env.data();
    let error_msg = || {
        format!(
            "failed to switch swap layout in plugin {}",
            env.plugin_env.name()
        )
    };
    let action = Action::NextSwapLayout;
    apply_action!(action, error_msg, env);
}

fn host_go_to_tab_name(env: FunctionEnvMut<ForeignFunctionEnv>) {
    let env = env.data();
    let error_msg = || format!("failed to change tab in plugin {}", env.plugin_env.name());
    wasi_read_string(&env.plugin_env.wasi_env)
        .and_then(|tab_name| {
            let create = false;
            let action = Action::GoToTabName(tab_name, create);
            apply_action!(action, error_msg, env);
            Ok(())
        })
        .with_context(error_msg)
        .fatal();
}

fn host_focus_or_create_tab(env: FunctionEnvMut<ForeignFunctionEnv>) {
    let env = env.data();
    let error_msg = || {
        format!(
            "failed to change or create tab in plugin {}",
            env.plugin_env.name()
        )
    };
    wasi_read_string(&env.plugin_env.wasi_env)
        .and_then(|tab_name| {
            let create = true;
            let action = Action::GoToTabName(tab_name, create);
            apply_action!(action, error_msg, env);
            Ok(())
        })
        .with_context(error_msg)
        .fatal();
}

fn host_go_to_tab(env: FunctionEnvMut<ForeignFunctionEnv>, tab_index: i32) {
    let env = env.data();
    let error_msg = || {
        format!(
            "failed to change tab focus in plugin {}",
            env.plugin_env.name()
        )
    };
    let action = Action::GoToTab(tab_index as u32);
    apply_action!(action, error_msg, env);
}

fn host_start_or_reload_plugin(env: FunctionEnvMut<ForeignFunctionEnv>) {
    let env = env.data();
    let error_msg = || {
        format!(
            "failed to start or reload plugin in plugin {}",
            env.plugin_env.name()
        )
    };
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    wasi_read_string(&env.plugin_env.wasi_env)
        .and_then(|url| Url::parse(&url).map_err(|e| anyhow!("Failed to parse url: {}", e)))
        .and_then(|url| {
            RunPluginLocation::parse(url.as_str(), Some(cwd))
                .map_err(|e| anyhow!("Failed to parse plugin location: {}", e))
        })
        .and_then(|run_plugin_location| {
            let run_plugin = RunPlugin {
                location: run_plugin_location,
                _allow_exec_host_cmd: false,
            };
            let action = Action::StartOrReloadPlugin(run_plugin);
            apply_action!(action, error_msg, env);
            Ok(())
        })
        .with_context(error_msg)
        .fatal();
}

fn host_close_terminal_pane(env: FunctionEnvMut<ForeignFunctionEnv>, terminal_pane_id: i32) {
    let env = env.data();
    let error_msg = || {
        format!(
            "failed to change tab focus in plugin {}",
            env.plugin_env.name()
        )
    };
    let action = Action::CloseTerminalPane(terminal_pane_id as u32);
    apply_action!(action, error_msg, env);
}

fn host_close_plugin_pane(env: FunctionEnvMut<ForeignFunctionEnv>, plugin_pane_id: i32) {
    let env = env.data();
    let error_msg = || {
        format!(
            "failed to change tab focus in plugin {}",
            env.plugin_env.name()
        )
    };
    let action = Action::ClosePluginPane(plugin_pane_id as u32);
    apply_action!(action, error_msg, env);
}

fn host_focus_terminal_pane(
    env: FunctionEnvMut<ForeignFunctionEnv>,
    terminal_pane_id: i32,
    should_float_if_hidden: i32,
) {
    let env = env.data();
    let should_float_if_hidden = should_float_if_hidden != 0;
    let action = Action::FocusTerminalPaneWithId(terminal_pane_id as u32, should_float_if_hidden);
    let error_msg = || format!("Failed to focus terminal pane");
    apply_action!(action, error_msg, env);
}

fn host_focus_plugin_pane(
    env: FunctionEnvMut<ForeignFunctionEnv>,
    plugin_pane_id: i32,
    should_float_if_hidden: i32,
) {
    let env = env.data();
    let should_float_if_hidden = should_float_if_hidden != 0;
    let action = Action::FocusPluginPaneWithId(plugin_pane_id as u32, should_float_if_hidden);
    let error_msg = || format!("Failed to focus plugin pane");
    apply_action!(action, error_msg, env);
}

fn host_rename_terminal_pane(env: FunctionEnvMut<ForeignFunctionEnv>) {
    let env = env.data();
    let error_msg = || format!("Failed to rename terminal pane");
    wasi_read_object::<(u32, String)>(&env.plugin_env.wasi_env)
        .and_then(|(terminal_pane_id, new_name)| {
            let rename_pane_action =
                Action::RenameTerminalPane(terminal_pane_id, new_name.as_bytes().to_vec());
            apply_action!(rename_pane_action, error_msg, env);
            Ok(())
        })
        .with_context(error_msg)
        .fatal();
}

fn host_rename_plugin_pane(env: FunctionEnvMut<ForeignFunctionEnv>) {
    let env = env.data();
    let error_msg = || format!("Failed to rename plugin pane");
    wasi_read_object::<(u32, String)>(&env.plugin_env.wasi_env)
        .and_then(|(plugin_pane_id, new_name)| {
            let rename_pane_action =
                Action::RenamePluginPane(plugin_pane_id, new_name.as_bytes().to_vec());
            apply_action!(rename_pane_action, error_msg, env);
            Ok(())
        })
        .with_context(error_msg)
        .fatal();
}

fn host_rename_tab(env: FunctionEnvMut<ForeignFunctionEnv>) {
    let env = env.data();
    let error_msg = || format!("Failed to rename tab");
    wasi_read_object::<(u32, String)>(&env.plugin_env.wasi_env)
        .and_then(|(tab_index, new_name)| {
            let rename_tab_action = Action::RenameTab(tab_index, new_name.as_bytes().to_vec());
            apply_action!(rename_tab_action, error_msg, env);
            Ok(())
        })
        .with_context(error_msg)
        .fatal();
}

// Custom panic handler for plugins.
//
// This is called when a panic occurs in a plugin. Since most panics will likely originate in the
// code trying to deserialize an `Event` upon a plugin state update, we read some panic message,
// formatted as string from the plugin.
fn host_report_panic(env: FunctionEnvMut<ForeignFunctionEnv>) {
    let env = env.data();
    let msg = wasi_read_string(&env.plugin_env.wasi_env)
        .with_context(|| {
            format!(
                "failed to report panic for plugin '{}'",
                env.plugin_env.name()
            )
        })
        .fatal();
    log::error!("PANIC IN PLUGIN! {}", msg);
    handle_plugin_crash(
        env.plugin_env.plugin_id,
        msg,
        env.plugin_env.senders.clone(),
    );
}

// Helper Functions ---------------------------------------------------------------------------------------------------

pub fn wasi_read_string(wasi_env: &WasiEnv) -> Result<String> {
    log::info!("wasi_read_string");
    let err_context = || format!("failed to read string from WASI env '{wasi_env:?}'");

    let mut buf = vec![];
    wasi_env
        .state()
        .stdout()
        .map_err(anyError::new)
        .and_then(|stdout| {
            log::info!("got stdout");
            stdout.ok_or(anyhow!("failed to get mutable reference to stdout"))
        })
        .and_then(|mut wasi_file| wasi_file.read_to_end(&mut buf).map_err(anyError::new))
        .with_context(err_context)?;
    let buf = String::from_utf8_lossy(&buf);
    log::info!("buf: {:?}", buf);
    // https://stackoverflow.com/questions/66450942/in-rust-is-there-a-way-to-make-literal-newlines-in-r-using-windows-c
    Ok(buf.replace("\n", "\n\r"))
}

pub fn wasi_write_string(wasi_env: &WasiEnv, buf: &str) -> Result<()> {
    wasi_env
        .state()
        .stdin()
        .map_err(anyError::new)
        .and_then(|stdin| stdin.ok_or(anyhow!("failed to get mutable reference to stdin")))
        .and_then(|mut stdin| writeln!(stdin, "{}\r", buf).map_err(anyError::new))
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
