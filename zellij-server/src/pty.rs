use crate::background_jobs::BackgroundJob;
use crate::terminal_bytes::TerminalBytes;
use crate::{
    panes::PaneId,
    plugins::{PluginId, PluginInstruction},
    screen::ScreenInstruction,
    session_layout_metadata::SessionLayoutMetadata,
    thread_bus::{Bus, ThreadSenders},
    ClientId, ServerInstruction,
};
use async_std::task::{self, JoinHandle};
#[cfg(target_os = "linux")]
use std::ops::RangeInclusive;
#[cfg(target_os = "linux")]
use std::os::unix::fs::MetadataExt;
use std::path::Path;
use std::sync::Arc;
use std::{collections::HashMap, os::unix::io::RawFd, path::PathBuf};
use zellij_utils::data::PaneManifest;
use zellij_utils::nix::unistd::Pid;
use zellij_utils::{
    async_std,
    data::{Event, FloatingPaneCoordinates, OriginatingPlugin},
    errors::prelude::*,
    errors::{ContextType, PtyContext},
    input::{
        command::{OpenFilePayload, RunCommand, TerminalAction},
        layout::{FloatingPaneLayout, Layout, Run, RunPluginOrAlias, TiledPaneLayout},
    },
    pane_size::Size,
    session_serialization,
};

pub type VteBytes = Vec<u8>;
pub type TabIndex = u32;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClientTabIndexOrPaneId {
    ClientId(ClientId),
    TabIndex(usize),
    PaneId(PaneId),
}

/// Instructions related to PTYs (pseudoterminals).
#[derive(Clone, Debug)]
pub enum PtyInstruction {
    SpawnTerminal(
        Option<TerminalAction>,
        Option<bool>,
        Option<String>,
        Option<FloatingPaneCoordinates>,
        bool, // start suppressed
        ClientTabIndexOrPaneId,
    ), // bool (if Some) is
    // should_float, String is an optional pane name
    OpenInPlaceEditor(PathBuf, Option<usize>, ClientTabIndexOrPaneId), // Option<usize> is the optional line number
    SpawnTerminalVertically(Option<TerminalAction>, Option<String>, ClientId), // String is an
    // optional pane
    // name
    // bool is start_suppressed
    SpawnTerminalHorizontally(Option<TerminalAction>, Option<String>, ClientId), // String is an
    // optional pane
    // name
    UpdateActivePane(Option<PaneId>, ClientId),
    GoToTab(TabIndex, ClientId),
    NewTab(
        Option<PathBuf>,
        Option<TerminalAction>,
        Option<TiledPaneLayout>,
        Vec<FloatingPaneLayout>,
        usize,                               // tab_index
        HashMap<RunPluginOrAlias, Vec<u32>>, // plugin_ids
        bool,                                // should change focus to new tab
        ClientId,
    ), // the String is the tab name
    ClosePane(PaneId),
    CloseTab(Vec<PaneId>),
    ReRunCommandInPane(PaneId, RunCommand),
    DropToShellInPane {
        pane_id: PaneId,
        shell: Option<PathBuf>,
        working_dir: Option<PathBuf>,
    },
    SpawnInPlaceTerminal(
        Option<TerminalAction>,
        Option<String>,
        ClientTabIndexOrPaneId,
    ), // String is an optional pane name
    DumpLayout(SessionLayoutMetadata, ClientId),
    DumpLayoutToPlugin(SessionLayoutMetadata, PluginId),
    LogLayoutToHd(SessionLayoutMetadata),
    FillPluginCwd(
        Option<bool>,   // should float
        bool,           // should be opened in place
        Option<String>, // pane title
        RunPluginOrAlias,
        usize,          // tab index
        Option<PaneId>, // pane id to replace if this is to be opened "in-place"
        ClientId,
        Size,
        bool,            // skip cache
        Option<PathBuf>, // if Some, will not fill cwd but just forward the message
        Option<FloatingPaneCoordinates>,
    ),
    ListClientsMetadata(SessionLayoutMetadata, ClientId),
    Reconfigure {
        client_id: ClientId,
        default_editor: Option<PathBuf>,
    },
    ListClientsToPlugin(SessionLayoutMetadata, PluginId, ClientId),
    UpdatePaneInfo(PaneManifest),
    Exit,
}

impl From<&PtyInstruction> for PtyContext {
    fn from(pty_instruction: &PtyInstruction) -> Self {
        match *pty_instruction {
            PtyInstruction::SpawnTerminal(..) => PtyContext::SpawnTerminal,
            PtyInstruction::OpenInPlaceEditor(..) => PtyContext::OpenInPlaceEditor,
            PtyInstruction::SpawnTerminalVertically(..) => PtyContext::SpawnTerminalVertically,
            PtyInstruction::SpawnTerminalHorizontally(..) => PtyContext::SpawnTerminalHorizontally,
            PtyInstruction::UpdateActivePane(..) => PtyContext::UpdateActivePane,
            PtyInstruction::GoToTab(..) => PtyContext::GoToTab,
            PtyInstruction::ClosePane(_) => PtyContext::ClosePane,
            PtyInstruction::CloseTab(_) => PtyContext::CloseTab,
            PtyInstruction::NewTab(..) => PtyContext::NewTab,
            PtyInstruction::ReRunCommandInPane(..) => PtyContext::ReRunCommandInPane,
            PtyInstruction::DropToShellInPane { .. } => PtyContext::DropToShellInPane,
            PtyInstruction::SpawnInPlaceTerminal(..) => PtyContext::SpawnInPlaceTerminal,
            PtyInstruction::DumpLayout(..) => PtyContext::DumpLayout,
            PtyInstruction::DumpLayoutToPlugin(..) => PtyContext::DumpLayoutToPlugin,
            PtyInstruction::LogLayoutToHd(..) => PtyContext::LogLayoutToHd,
            PtyInstruction::FillPluginCwd(..) => PtyContext::FillPluginCwd,
            PtyInstruction::ListClientsMetadata(..) => PtyContext::ListClientsMetadata,
            PtyInstruction::Reconfigure { .. } => PtyContext::Reconfigure,
            PtyInstruction::ListClientsToPlugin(..) => PtyContext::ListClientsToPlugin,
            PtyInstruction::UpdatePaneInfo(..) => PtyContext::UpdatePaneInfo,
            PtyInstruction::Exit => PtyContext::Exit,
        }
    }
}

pub(crate) struct Pty {
    pub active_panes: HashMap<ClientId, PaneId>,
    pub bus: Bus<PtyInstruction>,
    pub id_to_child_pid: HashMap<u32, RawFd>, // terminal_id => child raw fd
    originating_plugins: HashMap<u32, OriginatingPlugin>,
    debug_to_file: bool,
    task_handles: HashMap<u32, JoinHandle<()>>, // terminal_id to join-handle
    default_editor: Option<PathBuf>,
}

pub(crate) fn pty_thread_main(mut pty: Pty, layout: Box<Layout>) -> Result<()> {
    loop {
        let (event, mut err_ctx) = pty.bus.recv().expect("failed to receive event on channel");
        err_ctx.add_call(ContextType::Pty((&event).into()));
        match event {
            PtyInstruction::SpawnTerminal(
                terminal_action,
                should_float,
                name,
                floating_pane_coordinates,
                start_suppressed,
                client_or_tab_index,
            ) => {
                let err_context =
                    || format!("failed to spawn terminal for {:?}", client_or_tab_index);

                let (hold_on_close, run_command, pane_title, open_file_payload) =
                    match &terminal_action {
                        Some(TerminalAction::RunCommand(run_command)) => (
                            run_command.hold_on_close,
                            Some(run_command.clone()),
                            Some(name.unwrap_or_else(|| run_command.to_string())),
                            None,
                        ),
                        Some(TerminalAction::OpenFile(open_file_payload)) => {
                            (false, None, name, Some(open_file_payload.clone()))
                        },
                        _ => (false, None, name, None),
                    };
                let invoked_with = match &terminal_action {
                    Some(TerminalAction::RunCommand(run_command)) => {
                        Some(Run::Command(run_command.clone()))
                    },
                    Some(TerminalAction::OpenFile(payload)) => Some(Run::EditFile(
                        payload.path.clone(),
                        payload.line_number.clone(),
                        payload.cwd.clone(),
                    )),
                    _ => None,
                };
                match pty
                    .spawn_terminal(terminal_action, client_or_tab_index)
                    .with_context(err_context)
                {
                    Ok((pid, starts_held)) => {
                        let hold_for_command = if starts_held {
                            run_command.clone()
                        } else {
                            None
                        };

                        // if this command originated in a plugin, we send the plugin back an event
                        // to let it know the command started and which pane_id it has
                        if let Some(originating_plugin) =
                            run_command.and_then(|r| r.originating_plugin)
                        {
                            pty.originating_plugins
                                .insert(pid, originating_plugin.clone());
                            let update_event =
                                Event::CommandPaneOpened(pid, originating_plugin.context.clone());
                            pty.bus
                                .senders
                                .send_to_plugin(PluginInstruction::Update(vec![(
                                    Some(originating_plugin.plugin_id),
                                    Some(originating_plugin.client_id),
                                    update_event,
                                )]))
                                .with_context(err_context)?;
                        }
                        if let Some(originating_plugin) =
                            open_file_payload.and_then(|o| o.originating_plugin)
                        {
                            let update_event =
                                Event::EditPaneOpened(pid, originating_plugin.context.clone());
                            pty.bus
                                .senders
                                .send_to_plugin(PluginInstruction::Update(vec![(
                                    Some(originating_plugin.plugin_id),
                                    Some(originating_plugin.client_id),
                                    update_event,
                                )]))
                                .with_context(err_context)?;
                        }

                        pty.bus
                            .senders
                            .send_to_screen(ScreenInstruction::NewPane(
                                PaneId::Terminal(pid),
                                pane_title,
                                should_float,
                                hold_for_command,
                                invoked_with,
                                floating_pane_coordinates,
                                start_suppressed,
                                client_or_tab_index,
                            ))
                            .with_context(err_context)?;
                    },
                    Err(err) => match err.downcast_ref::<ZellijError>() {
                        Some(ZellijError::CommandNotFound { terminal_id, .. }) => {
                            if hold_on_close {
                                let hold_for_command = None; // we do not hold an "error" pane
                                pty.bus
                                    .senders
                                    .send_to_screen(ScreenInstruction::NewPane(
                                        PaneId::Terminal(*terminal_id),
                                        pane_title,
                                        should_float,
                                        hold_for_command,
                                        invoked_with,
                                        floating_pane_coordinates,
                                        start_suppressed,
                                        client_or_tab_index,
                                    ))
                                    .with_context(err_context)?;
                                if let Some(run_command) = run_command {
                                    send_command_not_found_to_screen(
                                        pty.bus.senders.clone(),
                                        *terminal_id,
                                        run_command.clone(),
                                    )
                                    .with_context(err_context)?;
                                }
                            } else {
                                log::error!("Failed to spawn terminal: {:?}", err);
                                pty.close_pane(PaneId::Terminal(*terminal_id))
                                    .with_context(err_context)?;
                            }
                        },
                        _ => Err::<(), _>(err).non_fatal(),
                    },
                }
            },
            PtyInstruction::SpawnInPlaceTerminal(
                terminal_action,
                name,
                client_id_tab_index_or_pane_id,
            ) => {
                let err_context = || {
                    format!(
                        "failed to spawn terminal for {:?}",
                        client_id_tab_index_or_pane_id
                    )
                };
                let (hold_on_close, run_command, pane_title) = match &terminal_action {
                    Some(TerminalAction::RunCommand(run_command)) => (
                        run_command.hold_on_close,
                        Some(run_command.clone()),
                        Some(name.unwrap_or_else(|| run_command.to_string())),
                    ),
                    _ => (false, None, name),
                };
                let invoked_with = match &terminal_action {
                    Some(TerminalAction::RunCommand(run_command)) => {
                        Some(Run::Command(run_command.clone()))
                    },
                    Some(TerminalAction::OpenFile(payload)) => Some(Run::EditFile(
                        payload.path.clone(),
                        payload.line_number.clone(),
                        payload.cwd.clone(),
                    )),
                    _ => None,
                };
                match pty
                    .spawn_terminal(terminal_action, client_id_tab_index_or_pane_id)
                    .with_context(err_context)
                {
                    Ok((pid, starts_held)) => {
                        let hold_for_command = if starts_held { run_command } else { None };
                        pty.bus
                            .senders
                            .send_to_screen(ScreenInstruction::ReplacePane(
                                PaneId::Terminal(pid),
                                hold_for_command,
                                pane_title,
                                invoked_with,
                                client_id_tab_index_or_pane_id,
                            ))
                            .with_context(err_context)?;
                    },
                    Err(err) => match err.downcast_ref::<ZellijError>() {
                        Some(ZellijError::CommandNotFound { terminal_id, .. }) => {
                            if hold_on_close {
                                let hold_for_command = None; // we do not hold an "error" pane
                                pty.bus
                                    .senders
                                    .send_to_screen(ScreenInstruction::ReplacePane(
                                        PaneId::Terminal(*terminal_id),
                                        hold_for_command,
                                        pane_title,
                                        invoked_with,
                                        client_id_tab_index_or_pane_id,
                                    ))
                                    .with_context(err_context)?;
                                if let Some(run_command) = run_command {
                                    send_command_not_found_to_screen(
                                        pty.bus.senders.clone(),
                                        *terminal_id,
                                        run_command.clone(),
                                    )
                                    .with_context(err_context)?;
                                }
                            } else {
                                log::error!("Failed to spawn terminal: {:?}", err);
                                pty.close_pane(PaneId::Terminal(*terminal_id))
                                    .with_context(err_context)?;
                            }
                        },
                        _ => Err::<(), _>(err).non_fatal(),
                    },
                }
            },
            PtyInstruction::OpenInPlaceEditor(
                temp_file,
                line_number,
                client_tab_index_or_pane_id,
            ) => {
                let err_context = || format!("failed to open in-place editor for client");

                match pty.spawn_terminal(
                    Some(TerminalAction::OpenFile(OpenFilePayload::new(
                        temp_file,
                        line_number,
                        None,
                    ))),
                    client_tab_index_or_pane_id,
                ) {
                    Ok((pid, _starts_held)) => {
                        pty.bus
                            .senders
                            .send_to_screen(ScreenInstruction::OpenInPlaceEditor(
                                PaneId::Terminal(pid),
                                client_tab_index_or_pane_id,
                            ))
                            .with_context(err_context)?;
                    },
                    Err(e) => {
                        Err::<(), _>(e).with_context(err_context).non_fatal();
                    },
                }
            },
            PtyInstruction::SpawnTerminalVertically(terminal_action, name, client_id) => {
                let err_context =
                    || format!("failed to spawn terminal vertically for client {client_id}");

                let (hold_on_close, run_command, pane_title) = match &terminal_action {
                    Some(TerminalAction::RunCommand(run_command)) => (
                        run_command.hold_on_close,
                        Some(run_command.clone()),
                        Some(name.unwrap_or_else(|| run_command.to_string())),
                    ),
                    _ => (false, None, name),
                };
                match pty
                    .spawn_terminal(terminal_action, ClientTabIndexOrPaneId::ClientId(client_id))
                    .with_context(err_context)
                {
                    Ok((pid, starts_held)) => {
                        let hold_for_command = if starts_held { run_command } else { None };
                        pty.bus
                            .senders
                            .send_to_screen(ScreenInstruction::VerticalSplit(
                                PaneId::Terminal(pid),
                                pane_title,
                                hold_for_command,
                                client_id,
                            ))
                            .with_context(err_context)?;
                    },
                    Err(err) => match err.downcast_ref::<ZellijError>() {
                        Some(ZellijError::CommandNotFound { terminal_id, .. }) => {
                            let hold_for_command = None; // we do not hold an "error" pane
                            if hold_on_close {
                                pty.bus
                                    .senders
                                    .send_to_screen(ScreenInstruction::VerticalSplit(
                                        PaneId::Terminal(*terminal_id),
                                        pane_title,
                                        hold_for_command,
                                        client_id,
                                    ))
                                    .with_context(err_context)?;
                                if let Some(run_command) = run_command {
                                    pty.bus
                                        .senders
                                        .send_to_screen(ScreenInstruction::PtyBytes(
                                            *terminal_id,
                                            format!(
                                                "Command not found: {}",
                                                run_command.command.display()
                                            )
                                            .as_bytes()
                                            .to_vec(),
                                        ))
                                        .with_context(err_context)?;
                                    pty.bus
                                        .senders
                                        .send_to_screen(ScreenInstruction::HoldPane(
                                            PaneId::Terminal(*terminal_id),
                                            Some(2), // exit status
                                            run_command,
                                        ))
                                        .with_context(err_context)?;
                                }
                            }
                        },
                        _ => Err::<(), _>(err).non_fatal(),
                    },
                }
            },
            PtyInstruction::SpawnTerminalHorizontally(terminal_action, name, client_id) => {
                let err_context =
                    || format!("failed to spawn terminal horizontally for client {client_id}");

                let (hold_on_close, run_command, pane_title) = match &terminal_action {
                    Some(TerminalAction::RunCommand(run_command)) => (
                        run_command.hold_on_close,
                        Some(run_command.clone()),
                        Some(name.unwrap_or_else(|| run_command.to_string())),
                    ),
                    _ => (false, None, name),
                };
                match pty
                    .spawn_terminal(terminal_action, ClientTabIndexOrPaneId::ClientId(client_id))
                    .with_context(err_context)
                {
                    Ok((pid, starts_held)) => {
                        let hold_for_command = if starts_held { run_command } else { None };
                        pty.bus
                            .senders
                            .send_to_screen(ScreenInstruction::HorizontalSplit(
                                PaneId::Terminal(pid),
                                pane_title,
                                hold_for_command,
                                client_id,
                            ))
                            .with_context(err_context)?;
                    },
                    Err(err) => match err.downcast_ref::<ZellijError>() {
                        Some(ZellijError::CommandNotFound { terminal_id, .. }) => {
                            if hold_on_close {
                                let hold_for_command = None; // we do not hold an "error" pane
                                pty.bus
                                    .senders
                                    .send_to_screen(ScreenInstruction::HorizontalSplit(
                                        PaneId::Terminal(*terminal_id),
                                        pane_title,
                                        hold_for_command,
                                        client_id,
                                    ))
                                    .with_context(err_context)?;
                                if let Some(run_command) = run_command {
                                    pty.bus
                                        .senders
                                        .send_to_screen(ScreenInstruction::PtyBytes(
                                            *terminal_id,
                                            format!(
                                                "Command not found: {}",
                                                run_command.command.display()
                                            )
                                            .as_bytes()
                                            .to_vec(),
                                        ))
                                        .with_context(err_context)?;
                                    pty.bus
                                        .senders
                                        .send_to_screen(ScreenInstruction::HoldPane(
                                            PaneId::Terminal(*terminal_id),
                                            Some(2), // exit status
                                            run_command,
                                        ))
                                        .with_context(err_context)?;
                                }
                            }
                        },
                        _ => Err::<(), _>(err).non_fatal(),
                    },
                }
            },
            PtyInstruction::UpdateActivePane(pane_id, client_id) => {
                pty.set_active_pane(pane_id, client_id);
            },
            PtyInstruction::GoToTab(tab_index, client_id) => {
                pty.bus
                    .senders
                    .send_to_screen(ScreenInstruction::GoToTab(tab_index, Some(client_id)))
                    .with_context(|| {
                        format!("failed to move client {} to tab {}", client_id, tab_index)
                    })?;
            },
            PtyInstruction::NewTab(
                cwd,
                terminal_action,
                tab_layout,
                floating_panes_layout,
                tab_index,
                plugin_ids,
                should_change_focus_to_new_tab,
                client_id,
            ) => {
                let err_context = || format!("failed to open new tab for client {}", client_id);

                let floating_panes_layout = if floating_panes_layout.is_empty() {
                    layout.new_tab().1
                } else {
                    floating_panes_layout
                };
                pty.spawn_terminals_for_layout(
                    cwd,
                    tab_layout.unwrap_or_else(|| layout.new_tab().0),
                    floating_panes_layout,
                    terminal_action.clone(),
                    plugin_ids,
                    tab_index,
                    should_change_focus_to_new_tab,
                    client_id,
                )
                .with_context(err_context)?;
            },
            PtyInstruction::ClosePane(id) => {
                pty.close_pane(id)
                    .and_then(|_| {
                        pty.bus
                            .senders
                            .send_to_server(ServerInstruction::UnblockInputThread)
                    })
                    .with_context(|| format!("failed to close pane {:?}", id))?;
            },
            PtyInstruction::CloseTab(ids) => {
                pty.close_tab(ids)
                    .and_then(|_| {
                        pty.bus
                            .senders
                            .send_to_server(ServerInstruction::UnblockInputThread)
                    })
                    .context("failed to close tabs")?;
            },
            PtyInstruction::ReRunCommandInPane(pane_id, run_command) => {
                let err_context = || format!("failed to rerun command in pane {:?}", pane_id);

                match pty
                    .rerun_command_in_pane(pane_id, run_command.clone())
                    .with_context(err_context)
                {
                    Ok(..) => {},
                    Err(err) => match err.downcast_ref::<ZellijError>() {
                        Some(ZellijError::CommandNotFound { terminal_id, .. }) => {
                            if run_command.hold_on_close {
                                pty.bus
                                    .senders
                                    .send_to_screen(ScreenInstruction::PtyBytes(
                                        *terminal_id,
                                        format!(
                                            "Command not found: {}",
                                            run_command.command.display()
                                        )
                                        .as_bytes()
                                        .to_vec(),
                                    ))
                                    .with_context(err_context)?;
                                pty.bus
                                    .senders
                                    .send_to_screen(ScreenInstruction::HoldPane(
                                        PaneId::Terminal(*terminal_id),
                                        Some(2), // exit status
                                        run_command,
                                    ))
                                    .with_context(err_context)?;
                            }
                        },
                        _ => Err::<(), _>(err).non_fatal(),
                    },
                }
            },
            PtyInstruction::DropToShellInPane {
                pane_id,
                shell,
                working_dir,
            } => {
                let err_context = || format!("failed to rerun command in pane {:?}", pane_id);

                // TODO: get configured default_shell from screen/tab as an option and default to
                // this otherwise (also look for a place that turns get_default_shell into a
                // RunCommand, we might have done this before)
                let run_command = RunCommand {
                    command: shell.unwrap_or_else(|| get_default_shell()),
                    hold_on_close: false,
                    hold_on_start: false,
                    cwd: working_dir,
                    ..Default::default()
                };
                match pty
                    .rerun_command_in_pane(pane_id, run_command.clone())
                    .with_context(err_context)
                {
                    Ok(..) => {},
                    Err(err) => match err.downcast_ref::<ZellijError>() {
                        Some(ZellijError::CommandNotFound { terminal_id, .. }) => {
                            if run_command.hold_on_close {
                                pty.bus
                                    .senders
                                    .send_to_screen(ScreenInstruction::PtyBytes(
                                        *terminal_id,
                                        format!(
                                            "Command not found: {}",
                                            run_command.command.display()
                                        )
                                        .as_bytes()
                                        .to_vec(),
                                    ))
                                    .with_context(err_context)?;
                                pty.bus
                                    .senders
                                    .send_to_screen(ScreenInstruction::HoldPane(
                                        PaneId::Terminal(*terminal_id),
                                        Some(2), // exit status
                                        run_command,
                                    ))
                                    .with_context(err_context)?;
                            }
                        },
                        _ => Err::<(), _>(err).non_fatal(),
                    },
                }
            },
            PtyInstruction::DumpLayout(mut session_layout_metadata, client_id) => {
                let err_context = || format!("Failed to dump layout");
                pty.populate_session_layout_metadata(&mut session_layout_metadata);
                match session_serialization::serialize_session_layout(
                    session_layout_metadata.into(),
                ) {
                    Ok((kdl_layout, _pane_contents)) => {
                        pty.bus
                            .senders
                            .send_to_server(ServerInstruction::Log(vec![kdl_layout], client_id))
                            .with_context(err_context)
                            .non_fatal();
                    },
                    Err(e) => {
                        pty.bus
                            .senders
                            .send_to_server(ServerInstruction::Log(vec![e.to_owned()], client_id))
                            .with_context(err_context)
                            .non_fatal();
                    },
                }
            },
            PtyInstruction::ListClientsMetadata(mut session_layout_metadata, client_id) => {
                let err_context = || format!("Failed to dump layout");
                pty.populate_session_layout_metadata(&mut session_layout_metadata);
                pty.bus
                    .senders
                    .send_to_server(ServerInstruction::Log(
                        vec![format!(
                            "{}",
                            session_layout_metadata.list_clients_metadata()
                        )],
                        client_id,
                    ))
                    .with_context(err_context)
                    .non_fatal();
            },
            PtyInstruction::DumpLayoutToPlugin(mut session_layout_metadata, plugin_id) => {
                let err_context = || format!("Failed to dump layout");
                pty.populate_session_layout_metadata(&mut session_layout_metadata);
                pty.bus
                    .senders
                    .send_to_plugin(PluginInstruction::DumpLayoutToPlugin(
                        session_layout_metadata,
                        plugin_id,
                    ))
                    .with_context(err_context)
                    .non_fatal();
            },
            PtyInstruction::ListClientsToPlugin(
                mut session_layout_metadata,
                plugin_id,
                client_id,
            ) => {
                let err_context = || format!("Failed to dump layout");
                pty.populate_session_layout_metadata(&mut session_layout_metadata);
                pty.bus
                    .senders
                    .send_to_plugin(PluginInstruction::ListClientsToPlugin(
                        session_layout_metadata,
                        plugin_id,
                        client_id,
                    ))
                    .with_context(err_context)
                    .non_fatal();
            },
            PtyInstruction::LogLayoutToHd(mut session_layout_metadata) => {
                let err_context = || format!("Failed to dump layout");
                pty.populate_session_layout_metadata(&mut session_layout_metadata);
                if session_layout_metadata.is_dirty() {
                    match session_serialization::serialize_session_layout(
                        session_layout_metadata.into(),
                    ) {
                        Ok(kdl_layout_and_pane_contents) => {
                            pty.bus
                                .senders
                                .send_to_background_jobs(BackgroundJob::ReportLayoutInfo(
                                    kdl_layout_and_pane_contents,
                                ))
                                .with_context(err_context)?;
                        },
                        Err(e) => {
                            log::error!("Failed to log layout to HD: {}", e);
                        },
                    }
                }
            },
            PtyInstruction::FillPluginCwd(
                should_float,
                should_be_open_in_place,
                pane_title,
                run,
                tab_index,
                pane_id_to_replace,
                client_id,
                size,
                skip_cache,
                cwd,
                floating_pane_coordinates,
            ) => {
                pty.fill_plugin_cwd(
                    should_float,
                    should_be_open_in_place,
                    pane_title,
                    run,
                    tab_index,
                    pane_id_to_replace,
                    client_id,
                    size,
                    skip_cache,
                    cwd,
                    floating_pane_coordinates,
                )?;
            },
            PtyInstruction::Reconfigure {
                default_editor,
                client_id: _,
            } => {
                pty.reconfigure(default_editor);
            },
            PtyInstruction::UpdatePaneInfo(mut manifest) => {
                for infos in manifest.panes.values_mut() {
                    for pane_info in infos.as_mut_slice() {
                        let pid = pty.get_child_pid(pane_info.id);
                        pane_info.pid = pid;
                        pane_info.cwd = pty
                            .bus
                            .os_input
                            .as_ref()
                            .map(|os_input| {
                                os_input
                                    .get_cwd(Pid::from_raw(pid))
                                    .map(|x| x.to_string_lossy().to_string())
                            })
                            .unwrap_or_default();

                        // Currently, only available on Linux, as it
                        // depends on the /proc-filesystem
                        #[cfg(target_os = "linux")]
                        {
                            // We can't send PathBufs via protobuf, so we have to convert to String
                            pane_info.tty = TtyDriver::find_tty_for_pid(pid)
                                .map(|x| x.to_string_lossy().to_string());
                        }
                    }
                }
                pty.bus
                    .senders
                    .send_to_plugin(PluginInstruction::Update(vec![(
                        None,
                        None,
                        Event::PaneUpdate(manifest),
                    )]))
                    .context("failed to update tabs")?;
            },
            PtyInstruction::Exit => break,
        }
    }
    Ok(())
}

impl Pty {
    pub fn new(
        bus: Bus<PtyInstruction>,
        debug_to_file: bool,
        default_editor: Option<PathBuf>,
    ) -> Self {
        Pty {
            active_panes: HashMap::new(),
            bus,
            id_to_child_pid: HashMap::new(),
            debug_to_file,
            task_handles: HashMap::new(),
            default_editor,
            originating_plugins: HashMap::new(),
        }
    }
    pub fn get_default_terminal(
        &self,
        cwd: Option<PathBuf>,
        default_shell: Option<TerminalAction>,
    ) -> TerminalAction {
        match default_shell {
            Some(mut default_shell) => {
                if let Some(cwd) = cwd {
                    match default_shell {
                        TerminalAction::RunCommand(ref mut command) => {
                            command.cwd = Some(cwd);
                        },
                        TerminalAction::OpenFile(ref mut payload) => {
                            match payload.cwd.as_mut() {
                                Some(edit_cwd) => {
                                    *edit_cwd = cwd.join(&edit_cwd);
                                },
                                None => {
                                    let _ = payload.cwd.insert(cwd.clone());
                                },
                            };
                        },
                    }
                }
                default_shell
            },
            None => {
                let shell = get_default_shell();
                TerminalAction::RunCommand(RunCommand {
                    args: vec![],
                    command: shell,
                    cwd, // note: this might also be filled by the calling function, eg. spawn_terminal
                    hold_on_close: false,
                    hold_on_start: false,
                    ..Default::default()
                })
            },
        }
    }
    fn fill_cwd(&self, terminal_action: &mut TerminalAction, client_id: ClientId) {
        if let TerminalAction::RunCommand(run_command) = terminal_action {
            if run_command.cwd.is_none() {
                run_command.cwd = self
                    .active_panes
                    .get(&client_id)
                    .and_then(|pane| match pane {
                        PaneId::Plugin(..) => None,
                        PaneId::Terminal(id) => self.id_to_child_pid.get(id),
                    })
                    .and_then(|&id| {
                        self.bus
                            .os_input
                            .as_ref()
                            .and_then(|input| input.get_cwd(Pid::from_raw(id)))
                    });
            };
        };
    }
    fn fill_cwd_from_pane_id(&self, terminal_action: &mut TerminalAction, pane_id: &u32) {
        if let TerminalAction::RunCommand(run_command) = terminal_action {
            if run_command.cwd.is_none() {
                run_command.cwd = self.id_to_child_pid.get(pane_id).and_then(|&id| {
                    self.bus
                        .os_input
                        .as_ref()
                        .and_then(|input| input.get_cwd(Pid::from_raw(id)))
                });
            };
        };
    }
    pub fn spawn_terminal(
        &mut self,
        terminal_action: Option<TerminalAction>,
        client_or_tab_index: ClientTabIndexOrPaneId,
    ) -> Result<(u32, bool)> {
        // bool is starts_held
        let err_context = || format!("failed to spawn terminal for {:?}", client_or_tab_index);

        // returns the terminal id
        let terminal_action = match client_or_tab_index {
            ClientTabIndexOrPaneId::ClientId(client_id) => {
                let mut terminal_action =
                    terminal_action.unwrap_or_else(|| self.get_default_terminal(None, None));
                self.fill_cwd(&mut terminal_action, client_id);
                terminal_action
            },
            ClientTabIndexOrPaneId::TabIndex(_) => {
                terminal_action.unwrap_or_else(|| self.get_default_terminal(None, None))
            },
            ClientTabIndexOrPaneId::PaneId(pane_id) => {
                let mut terminal_action =
                    terminal_action.unwrap_or_else(|| self.get_default_terminal(None, None));
                if let PaneId::Terminal(terminal_pane_id) = pane_id {
                    self.fill_cwd_from_pane_id(&mut terminal_action, &terminal_pane_id);
                }
                terminal_action
            },
        };
        let (hold_on_start, hold_on_close, originating_command_plugin, originating_edit_plugin) =
            match &terminal_action {
                TerminalAction::RunCommand(run_command) => (
                    run_command.hold_on_start,
                    run_command.hold_on_close,
                    run_command.originating_plugin.clone(),
                    None,
                ),
                TerminalAction::OpenFile(open_file_payload) => (
                    false,
                    false,
                    None,
                    open_file_payload.originating_plugin.clone(),
                ),
            };

        if hold_on_start {
            // we don't actually open a terminal in this case, just wait for the user to run it
            let starts_held = hold_on_start;
            let terminal_id = self
                .bus
                .os_input
                .as_mut()
                .context("couldn't get mutable reference to OS interface")
                .and_then(|os_input| os_input.reserve_terminal_id())
                .with_context(err_context)?;
            return Ok((terminal_id, starts_held));
        }

        let originating_command_plugin = Arc::new(originating_command_plugin.clone());
        let originating_edit_plugin = Arc::new(originating_edit_plugin.clone());
        let quit_cb = Box::new({
            let senders = self.bus.senders.clone();
            move |pane_id, exit_status, command| {
                // if this command originated in a plugin, we send the plugin an event letting it
                // know the command exited and some other useful information
                if let PaneId::Terminal(pane_id) = pane_id {
                    if let Some(originating_command_plugin) = originating_command_plugin.as_ref() {
                        let update_event = Event::CommandPaneExited(
                            pane_id,
                            exit_status,
                            originating_command_plugin.context.clone(),
                        );
                        let _ = senders.send_to_plugin(PluginInstruction::Update(vec![(
                            Some(originating_command_plugin.plugin_id),
                            Some(originating_command_plugin.client_id),
                            update_event,
                        )]));
                    }
                    if let Some(originating_edit_plugin) = originating_edit_plugin.as_ref() {
                        let update_event = Event::EditPaneExited(
                            pane_id,
                            exit_status,
                            originating_edit_plugin.context.clone(),
                        );
                        let _ = senders.send_to_plugin(PluginInstruction::Update(vec![(
                            Some(originating_edit_plugin.plugin_id),
                            Some(originating_edit_plugin.client_id),
                            update_event,
                        )]));
                    }
                }

                if hold_on_close {
                    let _ = senders.send_to_screen(ScreenInstruction::HoldPane(
                        pane_id,
                        exit_status,
                        command,
                    ));
                } else {
                    let _ = senders.send_to_screen(ScreenInstruction::ClosePane(pane_id, None));
                }
            }
        });
        let (terminal_id, pid_primary, child_fd): (u32, RawFd, RawFd) = self
            .bus
            .os_input
            .as_mut()
            .context("no OS I/O interface found")
            .and_then(|os_input| {
                os_input.spawn_terminal(terminal_action, quit_cb, self.default_editor.clone())
            })
            .with_context(err_context)?;
        let terminal_bytes = task::spawn({
            let err_context =
                |terminal_id: u32| format!("failed to run async task for terminal {terminal_id}");
            let senders = self.bus.senders.clone();
            let os_input = self
                .bus
                .os_input
                .as_ref()
                .with_context(|| err_context(terminal_id))
                .fatal()
                .clone();
            let debug_to_file = self.debug_to_file;
            async move {
                TerminalBytes::new(pid_primary, senders, os_input, debug_to_file, terminal_id)
                    .listen()
                    .await
                    .with_context(|| err_context(terminal_id))
                    .fatal();
            }
        });

        self.task_handles.insert(terminal_id, terminal_bytes);
        self.id_to_child_pid.insert(terminal_id, child_fd);
        let starts_held = false;
        Ok((terminal_id, starts_held))
    }
    pub fn spawn_terminals_for_layout(
        &mut self,
        cwd: Option<PathBuf>,
        layout: TiledPaneLayout,
        floating_panes_layout: Vec<FloatingPaneLayout>,
        default_shell: Option<TerminalAction>,
        plugin_ids: HashMap<RunPluginOrAlias, Vec<u32>>,
        tab_index: usize,
        should_change_focus_to_new_tab: bool,
        client_id: ClientId,
    ) -> Result<()> {
        let err_context = || format!("failed to spawn terminals for layout for client {client_id}");

        let mut default_shell =
            default_shell.unwrap_or_else(|| self.get_default_terminal(cwd, None));
        self.fill_cwd(&mut default_shell, client_id);
        let extracted_run_instructions = layout.extract_run_instructions();
        let extracted_floating_run_instructions = floating_panes_layout
            .iter()
            .filter(|f| !f.already_running)
            .map(|f| f.run.clone());
        let mut new_pane_pids: Vec<(u32, bool, Option<RunCommand>, Result<RawFd>)> = vec![]; // (terminal_id,
                                                                                             // starts_held,
                                                                                             // run_command,
                                                                                             // file_descriptor)
        let mut new_floating_panes_pids: Vec<(u32, bool, Option<RunCommand>, Result<RawFd>)> =
            vec![]; // same
                    // as
                    // new_pane_pids
        for run_instruction in extracted_run_instructions {
            if let Some(new_pane_data) =
                self.apply_run_instruction(run_instruction, default_shell.clone())?
            {
                new_pane_pids.push(new_pane_data);
            }
        }
        for run_instruction in extracted_floating_run_instructions {
            if let Some(new_pane_data) =
                self.apply_run_instruction(run_instruction, default_shell.clone())?
            {
                new_floating_panes_pids.push(new_pane_data);
            }
        }
        // Option<RunCommand> should only be Some if the pane starts held
        let new_tab_pane_ids: Vec<(u32, Option<RunCommand>)> = new_pane_pids
            .iter()
            .map(|(terminal_id, starts_held, run_command, _)| {
                if *starts_held {
                    (*terminal_id, run_command.clone())
                } else {
                    (*terminal_id, None)
                }
            })
            .collect();
        let new_tab_floating_pane_ids: Vec<(u32, Option<RunCommand>)> = new_floating_panes_pids
            .iter()
            .map(|(terminal_id, starts_held, run_command, _)| {
                if *starts_held {
                    (*terminal_id, run_command.clone())
                } else {
                    (*terminal_id, None)
                }
            })
            .collect();
        self.bus
            .senders
            .send_to_screen(ScreenInstruction::ApplyLayout(
                layout,
                floating_panes_layout,
                new_tab_pane_ids,
                new_tab_floating_pane_ids,
                plugin_ids,
                tab_index,
                should_change_focus_to_new_tab,
                client_id,
            ))
            .with_context(err_context)?;
        let mut terminals_to_start = vec![];
        terminals_to_start.append(&mut new_pane_pids);
        terminals_to_start.append(&mut new_floating_panes_pids);
        for (terminal_id, starts_held, run_command, pid_primary) in terminals_to_start {
            if starts_held {
                // we do not run a command or start listening for bytes on held panes
                continue;
            }
            match pid_primary {
                Ok(pid_primary) => {
                    let terminal_bytes = task::spawn({
                        let senders = self.bus.senders.clone();
                        let os_input = self
                            .bus
                            .os_input
                            .as_ref()
                            .with_context(err_context)?
                            .clone();
                        let debug_to_file = self.debug_to_file;
                        async move {
                            TerminalBytes::new(
                                pid_primary,
                                senders,
                                os_input,
                                debug_to_file,
                                terminal_id,
                            )
                            .listen()
                            .await
                            .context("failed to spawn terminals for layout")
                            .fatal();
                        }
                    });
                    self.task_handles.insert(terminal_id, terminal_bytes);
                },
                _ => match run_command {
                    Some(run_command) => {
                        if run_command.hold_on_close {
                            send_command_not_found_to_screen(
                                self.bus.senders.clone(),
                                terminal_id,
                                run_command.clone(),
                            )
                            .with_context(err_context)?;
                        } else {
                            self.close_pane(PaneId::Terminal(terminal_id))
                                .with_context(err_context)?;
                        }
                    },
                    None => {
                        self.close_pane(PaneId::Terminal(terminal_id))
                            .with_context(err_context)?;
                    },
                },
            }
        }
        Ok(())
    }
    fn apply_run_instruction(
        &mut self,
        run_instruction: Option<Run>,
        default_shell: TerminalAction,
    ) -> Result<Option<(u32, bool, Option<RunCommand>, Result<i32>)>> {
        // terminal_id,
        // starts_held,
        // command
        // successfully opened
        let err_context = || format!("failed to apply run instruction");
        let quit_cb = Box::new({
            let senders = self.bus.senders.clone();
            move |pane_id, _exit_status, _command| {
                let _ = senders.send_to_screen(ScreenInstruction::ClosePane(pane_id, None));
            }
        });
        match run_instruction {
            Some(Run::Command(mut command)) => {
                let starts_held = command.hold_on_start;
                let hold_on_close = command.hold_on_close;
                let quit_cb = Box::new({
                    let senders = self.bus.senders.clone();
                    move |pane_id, exit_status, command| {
                        if hold_on_close {
                            let _ = senders.send_to_screen(ScreenInstruction::HoldPane(
                                pane_id,
                                exit_status,
                                command,
                            ));
                        } else {
                            let _ =
                                senders.send_to_screen(ScreenInstruction::ClosePane(pane_id, None));
                        }
                    }
                });
                if command.cwd.is_none() {
                    if let TerminalAction::RunCommand(cmd) = default_shell {
                        command.cwd = cmd.cwd;
                    }
                }
                let cmd = TerminalAction::RunCommand(command.clone());
                if starts_held {
                    // we don't actually open a terminal in this case, just wait for the user to run it
                    match self
                        .bus
                        .os_input
                        .as_mut()
                        .context("no OS I/O interface found")
                        .with_context(err_context)?
                        .reserve_terminal_id()
                    {
                        Ok(terminal_id) => {
                            Ok(Some((
                                terminal_id,
                                starts_held,
                                Some(command.clone()),
                                Ok(terminal_id as i32), // this is not actually correct but gets
                                                        // stripped later
                            )))
                        },
                        Err(e) => Err(e),
                    }
                } else {
                    match self
                        .bus
                        .os_input
                        .as_mut()
                        .context("no OS I/O interface found")
                        .with_context(err_context)?
                        .spawn_terminal(cmd, quit_cb, self.default_editor.clone())
                        .with_context(err_context)
                    {
                        Ok((terminal_id, pid_primary, child_fd)) => {
                            self.id_to_child_pid.insert(terminal_id, child_fd);
                            Ok(Some((
                                terminal_id,
                                starts_held,
                                Some(command.clone()),
                                Ok(pid_primary),
                            )))
                        },
                        Err(err) => {
                            match err.downcast_ref::<ZellijError>() {
                                Some(ZellijError::CommandNotFound { terminal_id, .. }) => Ok(Some(
                                    (*terminal_id, starts_held, Some(command.clone()), Err(err)),
                                )),
                                _ => Err(err),
                            }
                        },
                    }
                }
            },
            Some(Run::Cwd(cwd)) => {
                let starts_held = false; // we do not hold Cwd panes
                let shell = self.get_default_terminal(Some(cwd), Some(default_shell.clone()));
                match self
                    .bus
                    .os_input
                    .as_mut()
                    .context("no OS I/O interface found")
                    .with_context(err_context)?
                    .spawn_terminal(shell, quit_cb, self.default_editor.clone())
                    .with_context(err_context)
                {
                    Ok((terminal_id, pid_primary, child_fd)) => {
                        self.id_to_child_pid.insert(terminal_id, child_fd);
                        Ok(Some((terminal_id, starts_held, None, Ok(pid_primary))))
                    },
                    Err(err) => match err.downcast_ref::<ZellijError>() {
                        Some(ZellijError::CommandNotFound { terminal_id, .. }) => {
                            Ok(Some((*terminal_id, starts_held, None, Err(err))))
                        },
                        _ => Err(err),
                    },
                }
            },
            Some(Run::EditFile(path_to_file, line_number, cwd)) => {
                let starts_held = false; // we do not hold edit panes (for now?)
                match self
                    .bus
                    .os_input
                    .as_mut()
                    .context("no OS I/O interface found")
                    .with_context(err_context)?
                    .spawn_terminal(
                        TerminalAction::OpenFile(OpenFilePayload::new(
                            path_to_file,
                            line_number,
                            cwd,
                        )),
                        quit_cb,
                        self.default_editor.clone(),
                    )
                    .with_context(err_context)
                {
                    Ok((terminal_id, pid_primary, child_fd)) => {
                        self.id_to_child_pid.insert(terminal_id, child_fd);
                        Ok(Some((terminal_id, starts_held, None, Ok(pid_primary))))
                    },
                    Err(err) => match err.downcast_ref::<ZellijError>() {
                        Some(ZellijError::CommandNotFound { terminal_id, .. }) => {
                            Ok(Some((*terminal_id, starts_held, None, Err(err))))
                        },
                        _ => Err(err),
                    },
                }
            },
            None => {
                let starts_held = false;
                match self
                    .bus
                    .os_input
                    .as_mut()
                    .context("no OS I/O interface found")
                    .with_context(err_context)?
                    .spawn_terminal(default_shell.clone(), quit_cb, self.default_editor.clone())
                    .with_context(err_context)
                {
                    Ok((terminal_id, pid_primary, child_fd)) => {
                        self.id_to_child_pid.insert(terminal_id, child_fd);
                        Ok(Some((terminal_id, starts_held, None, Ok(pid_primary))))
                    },
                    Err(err) => match err.downcast_ref::<ZellijError>() {
                        Some(ZellijError::CommandNotFound { terminal_id, .. }) => {
                            Ok(Some((*terminal_id, starts_held, None, Err(err))))
                        },
                        _ => Err(err),
                    },
                }
            },
            // Investigate moving plugin loading to here.
            Some(Run::Plugin(_)) => Ok(None),
        }
    }
    pub fn close_pane(&mut self, id: PaneId) -> Result<()> {
        let err_context = || format!("failed to close for pane {id:?}");
        match id {
            PaneId::Terminal(id) => {
                self.task_handles.remove(&id);
                if let Some(child_fd) = self.id_to_child_pid.remove(&id) {
                    task::block_on(async {
                        let err_context = || format!("failed to run async task for pane {id}");
                        self.bus
                            .os_input
                            .as_mut()
                            .with_context(err_context)
                            .fatal()
                            .kill(Pid::from_raw(child_fd))
                            .with_context(err_context)
                            .fatal();
                    });
                }
                self.bus
                    .os_input
                    .as_ref()
                    .context("no OS I/O interface found")
                    .and_then(|os_input| os_input.clear_terminal_id(id))
                    .with_context(err_context)?;
            },
            PaneId::Plugin(pid) => drop(
                self.bus
                    .senders
                    .send_to_plugin(PluginInstruction::Unload(pid)),
            ),
        }
        Ok(())
    }
    pub fn close_tab(&mut self, ids: Vec<PaneId>) -> Result<()> {
        for id in ids {
            self.close_pane(id)
                .with_context(|| format!("failed to close tab for pane {id:?}"))?;
        }
        Ok(())
    }
    pub fn set_active_pane(&mut self, pane_id: Option<PaneId>, client_id: ClientId) {
        if let Some(pane_id) = pane_id {
            self.active_panes.insert(client_id, pane_id);
        }
    }
    pub fn rerun_command_in_pane(
        &mut self,
        pane_id: PaneId,
        mut run_command: RunCommand,
    ) -> Result<()> {
        let err_context = || format!("failed to rerun command in pane {:?}", pane_id);

        match pane_id {
            PaneId::Terminal(id) => {
                if let Some(originating_plugins) = self.originating_plugins.get(&id) {
                    run_command.originating_plugin = Some(originating_plugins.clone());
                }
                let _ = self.task_handles.remove(&id); // if all is well, this shouldn't be here
                let _ = self.id_to_child_pid.remove(&id); // if all is wlel, this shouldn't be here

                let hold_on_close = run_command.hold_on_close;
                let originating_plugin = Arc::new(run_command.originating_plugin.clone());
                let quit_cb = Box::new({
                    let senders = self.bus.senders.clone();
                    move |pane_id, exit_status, command| {
                        if let PaneId::Terminal(pane_id) = pane_id {
                            if let Some(originating_plugin) = originating_plugin.as_ref() {
                                let update_event = Event::CommandPaneExited(
                                    pane_id,
                                    exit_status,
                                    originating_plugin.context.clone(),
                                );
                                let _ = senders.send_to_plugin(PluginInstruction::Update(vec![(
                                    Some(originating_plugin.plugin_id),
                                    Some(originating_plugin.client_id),
                                    update_event,
                                )]));
                            }
                        }
                        if hold_on_close {
                            let _ = senders.send_to_screen(ScreenInstruction::HoldPane(
                                pane_id,
                                exit_status,
                                command,
                            ));
                        } else {
                            let _ =
                                senders.send_to_screen(ScreenInstruction::ClosePane(pane_id, None));
                        }
                    }
                });
                let (pid_primary, child_fd): (RawFd, RawFd) = self
                    .bus
                    .os_input
                    .as_mut()
                    .context("no OS I/O interface found")
                    .and_then(|os_input| {
                        os_input.re_run_command_in_terminal(id, run_command, quit_cb)
                    })
                    .with_context(err_context)?;
                let terminal_bytes = task::spawn({
                    let err_context =
                        |pane_id| format!("failed to run async task for pane {pane_id:?}");
                    let senders = self.bus.senders.clone();
                    let os_input = self
                        .bus
                        .os_input
                        .as_ref()
                        .with_context(|| err_context(pane_id))
                        .fatal()
                        .clone();
                    let debug_to_file = self.debug_to_file;
                    async move {
                        TerminalBytes::new(pid_primary, senders, os_input, debug_to_file, id)
                            .listen()
                            .await
                            .with_context(|| err_context(pane_id))
                            .fatal();
                    }
                });

                self.task_handles.insert(id, terminal_bytes);
                self.id_to_child_pid.insert(id, child_fd);
                if let Some(originating_plugin) = self.originating_plugins.get(&id) {
                    self.bus
                        .senders
                        .send_to_plugin(PluginInstruction::Update(vec![(
                            Some(originating_plugin.plugin_id),
                            Some(originating_plugin.client_id),
                            Event::CommandPaneReRun(id, originating_plugin.context.clone()),
                        )]))
                        .with_context(err_context)?;
                }
                Ok(())
            },
            _ => Err(anyhow!("cannot respawn plugin panes")).with_context(err_context),
        }
    }
    pub fn populate_session_layout_metadata(
        &self,
        session_layout_metadata: &mut SessionLayoutMetadata,
    ) {
        let terminal_ids = session_layout_metadata.all_terminal_ids();
        let mut terminal_ids_to_commands: HashMap<u32, Vec<String>> = HashMap::new();
        let mut terminal_ids_to_cwds: HashMap<u32, PathBuf> = HashMap::new();

        let pids: Vec<_> = terminal_ids
            .iter()
            .filter_map(|id| self.id_to_child_pid.get(&id))
            .map(|pid| Pid::from_raw(*pid))
            .collect();
        let pids_to_cwds = self
            .bus
            .os_input
            .as_ref()
            .map(|os_input| os_input.get_cwds(pids))
            .unwrap_or_default();
        let ppids_to_cmds = self
            .bus
            .os_input
            .as_ref()
            .map(|os_input| os_input.get_all_cmds_by_ppid())
            .unwrap_or_default();

        for terminal_id in terminal_ids {
            let process_id = self.id_to_child_pid.get(&terminal_id);
            let cwd = process_id
                .as_ref()
                .and_then(|pid| pids_to_cwds.get(&Pid::from_raw(**pid)));
            let cmd = process_id
                .as_ref()
                .and_then(|pid| ppids_to_cmds.get(&format!("{}", pid)));
            if let Some(cmd) = cmd {
                terminal_ids_to_commands.insert(terminal_id, cmd.clone());
            }
            if let Some(cwd) = cwd {
                terminal_ids_to_cwds.insert(terminal_id, cwd.clone());
            }
        }
        session_layout_metadata.update_default_shell(get_default_shell());
        session_layout_metadata.update_terminal_commands(terminal_ids_to_commands);
        session_layout_metadata.update_terminal_cwds(terminal_ids_to_cwds);
        session_layout_metadata.update_default_editor(&self.default_editor)
    }
    pub fn fill_plugin_cwd(
        &self,
        should_float: Option<bool>,
        should_open_in_place: bool, // should be opened in place
        pane_title: Option<String>, // pane title
        mut run: RunPluginOrAlias,
        tab_index: usize,                   // tab index
        pane_id_to_replace: Option<PaneId>, // pane id to replace if this is to be opened "in-place"
        client_id: ClientId,
        size: Size,
        skip_cache: bool,
        cwd: Option<PathBuf>,
        // left here for historical and potential future reasons since we might change the ordering
        // of the pipeline between threads and end up needing to forward this
        _floating_pane_coordinates: Option<FloatingPaneCoordinates>,
    ) -> Result<()> {
        let get_focused_cwd = || {
            self.active_panes
                .get(&client_id)
                .and_then(|pane| match pane {
                    PaneId::Plugin(..) => None,
                    PaneId::Terminal(id) => self.id_to_child_pid.get(id),
                })
                .and_then(|&id| {
                    self.bus
                        .os_input
                        .as_ref()
                        .and_then(|input| input.get_cwd(Pid::from_raw(id)))
                })
        };

        let cwd = cwd.or_else(get_focused_cwd);

        if let RunPluginOrAlias::Alias(alias) = &mut run {
            let cwd = get_focused_cwd();
            alias.set_caller_cwd_if_not_set(cwd);
        }
        self.bus.senders.send_to_plugin(PluginInstruction::Load(
            should_float,
            should_open_in_place,
            pane_title,
            run,
            Some(tab_index),
            pane_id_to_replace,
            client_id,
            size,
            cwd,
            skip_cache,
        ))?;
        Ok(())
    }
    pub fn reconfigure(&mut self, default_editor: Option<PathBuf>) {
        self.default_editor = default_editor;
    }

    pub fn get_child_pid(&self, terminal_id: u32) -> i32 {
        *self.id_to_child_pid.get(&terminal_id).unwrap_or(&-1)
    }
}

impl Drop for Pty {
    fn drop(&mut self) {
        let child_ids: Vec<u32> = self.id_to_child_pid.keys().copied().collect();
        for id in child_ids {
            self.close_pane(PaneId::Terminal(id))
                .with_context(|| format!("failed to close pane for pid {id}"))
                .fatal();
        }
    }
}

fn send_command_not_found_to_screen(
    senders: ThreadSenders,
    terminal_id: u32,
    run_command: RunCommand,
) -> Result<()> {
    let err_context = || format!("failed to send command_not_fount for terminal {terminal_id}");
    senders
        .send_to_screen(ScreenInstruction::PtyBytes(
            terminal_id,
            format!("Command not found: {}\n\rIf you were including arguments as part of the command, try including them as 'args' instead.", run_command.command.display())
                .as_bytes()
                .to_vec(),
        ))
        .with_context(err_context)?;
    senders
        .send_to_screen(ScreenInstruction::HoldPane(
            PaneId::Terminal(terminal_id),
            Some(2),
            run_command.clone(),
        ))
        .with_context(err_context)?;
    Ok(())
}

pub fn get_default_shell() -> PathBuf {
    PathBuf::from(std::env::var("SHELL").unwrap_or_else(|_| {
        log::warn!("Cannot read SHELL env, falling back to use /bin/sh");
        "/bin/sh".to_string()
    }))
}

#[cfg(target_os = "linux")]
#[derive(Debug, Clone)]
struct TtyDriver {
    path: PathBuf,
    major: i32,
    minor_range: RangeInclusive<i32>,
}

#[cfg(target_os = "linux")]
impl TtyDriver {
    /// Trys to find the TTY for a given process ID.
    /// This is unfortunately not straight forward. We have to do:
    /// 1. Read the tty_nr from /proc/<PID>/stat and do some bit-magic to get major and minor
    /// 2. Read /proc/tty/drivers to see which path corresponds to which major number and minor range
    /// 3. Match those 2 together and find a fitting driver
    /// 4. 'Guess' the resulting path (e.g. either /dev/tty/2 or /dev/tty2)
    /// 5. Verify the guess is correct by stat-ing the result and comparing major and minor
    pub(crate) fn find_tty_for_pid(pid: i32) -> Option<PathBuf> {
        // 1. Parse major and minor tty_nr
        let (tty_major, tty_minor) = TtyDriver::get_tty_nr_for_pid(pid)?;
        // 2. Parse /proc/tty/drivers
        let drivers = TtyDriver::parse_tty_drivers();
        // 3. Find a match
        let driver = TtyDriver::match_drivers_to_tty_nr(drivers, tty_major, tty_minor)?;
        // 4. and 5. Guess and verify path
        let path = TtyDriver::guess_tty_path(&driver.path, tty_major, tty_minor)?;
        Some(path)
    }

    fn verify_tty_path(path: &Path, tty_major: i32, tty_minor: i32) -> bool {
        if let Ok(metadata) = path.metadata() {
            let rdev = metadata.rdev() as i32;
            let dev_major = rdev >> 8;
            let dev_minor = rdev & 0xff;
            if dev_major == tty_major && dev_minor == tty_minor {
                return true;
            }
        }
        false
    }

    fn guess_tty_path(path: &Path, tty_major: i32, tty_minor: i32) -> Option<PathBuf> {
        // First, guess seperated by slash: (e.g. /dev/tty/2)
        let mut res = path.join(format!("{tty_minor}"));
        if res.exists() && TtyDriver::verify_tty_path(&res, tty_major, tty_minor) {
            return Some(res);
        }

        // Otherwise, guess seperated by directly appending the number: (e.g. /dev/tty2)
        let mut second_try = path.as_os_str().to_os_string();
        second_try.push(format!("{tty_minor}"));
        res = PathBuf::from(second_try);
        if res.exists() && TtyDriver::verify_tty_path(&res, tty_major, tty_minor) {
            return Some(res);
        }

        // No luck
        None
    }

    fn match_drivers_to_tty_nr(
        drivers: Vec<TtyDriver>,
        tty_major: i32,
        tty_minor: i32,
    ) -> Option<TtyDriver> {
        drivers
            .into_iter()
            .find(|driver| driver.major == tty_major && driver.minor_range.contains(&tty_minor))
    }

    fn parse_tty_drivers() -> Vec<TtyDriver> {
        let mut drivers = Vec::new();

        // example output:
        // /dev/tty             /dev/tty        5       0 system:/dev/tty
        // /dev/console         /dev/console    5       1 system:console
        // /dev/ptmx            /dev/ptmx       5       2 system
        // /dev/vc/0            /dev/vc/0       4       0 system:vtmaster
        // rfcomm               /dev/rfcomm   216 0-255 serial
        // serial               /dev/ttyS       4 64-95 serial
        // pty_slave            /dev/pts      136 0-1048575 pty:slave
        // pty_master           /dev/ptm      128 0-1048575 pty:master
        // unknown              /dev/tty        4 1-63 console
        let drivers_raw = match std::fs::read_to_string(PathBuf::from("/proc/tty/drivers")) {
            Ok(x) => x,
            Err(_) => {
                return drivers;
            },
        };

        for line in drivers_raw.lines() {
            let parts: Vec<_> = line.split_whitespace().collect();
            if parts.len() < 4 {
                // Something is wrong. Silently ignore this entry
                continue;
            }
            let path = PathBuf::from(parts[1]);
            let major = match parts[2].parse::<i32>() {
                Ok(maj) => maj,
                Err(_) => continue,
            };
            let tty_minor = parts[3];
            let minor_range = match TtyDriver::parse_minor_range(tty_minor) {
                Some(x) => x,
                None => {
                    continue;
                },
            };
            let driver = TtyDriver {
                path,
                major,
                minor_range,
            };
            drivers.push(driver);
        }
        drivers
    }

    // Getting either "3" or "3-10" and parsing a Range from that
    fn parse_minor_range(tty_minor: &str) -> Option<RangeInclusive<i32>> {
        let minor_range: Vec<_> = tty_minor.split('-').collect();
        if minor_range.len() == 1 {
            let start = minor_range[0].parse::<i32>().ok()?;
            Some(start..=start)
        } else if minor_range.len() == 2 {
            let start = minor_range[0].parse::<i32>().ok()?;
            let end = minor_range[1].parse::<i32>().ok()?;
            Some(start..=end)
        } else {
            None
        }
    }

    fn get_tty_nr_for_pid(pid: i32) -> Option<(i32, i32)> {
        if pid == -1 {
            return None;
        }
        let stat = std::fs::read_to_string(PathBuf::from(format!("/proc/{pid}/stat"))).ok()?;

        let tty_nr = stat
            .split_whitespace()
            .nth(6)
            .and_then(|s| s.parse::<i32>().ok())?;
        // from /usr/include/linux/kdev_t.h
        // #define MAJOR(dev)	((dev)>>8)
        // #define MINOR(dev)	((dev) & 0xff)
        let tty_major = tty_nr >> 8;
        let tty_minor = tty_nr & 0xff;

        Some((tty_major, tty_minor))
    }
}
