use crate::background_jobs::write_session_state_to_disk;
use crate::background_jobs::BackgroundJob;
use crate::global_async_runtime::get_tokio_runtime as async_runtime;
use crate::os_input_output::{env_value, AsyncReader, NullAsyncReader, PaneEnv};
use crate::pty_writer::PtyWriteInstruction;
use crate::route::NotificationEnd;
use crate::session_suggestions::{ContextTrigger, SessionSuggestionsJob};
use crate::terminal_bytes::TerminalBytes;
use crate::{
    panes::PaneId,
    plugins::{DumpSessionLayoutResponse, PluginId, PluginInstruction},
    screen::{ClosePaneSource, ScreenInstruction, TabOverrideResult},
    session_layout_metadata::SessionLayoutMetadata,
    thread_bus::{Bus, ThreadSenders},
    ClientId, ServerInstruction,
};
use std::collections::HashSet;
use std::sync::Arc;
use std::{collections::HashMap, path::PathBuf};
use tokio::task::JoinHandle;
use zellij_utils::{
    data::{
        CommandOrPlugin, Event, FloatingPaneCoordinates, GetPaneCwdResponse, GetPanePidResponse,
        GetPaneRunningCommandResponse, NewPanePlacement, OriginatingPlugin, SessionInfo,
    },
    errors::prelude::*,
    errors::{ContextType, PtyContext},
    input::{
        actions::ResurrectCommand,
        command::{OpenFilePayload, RunCommand, RunCommandAction, TerminalAction},
        layout::{
            FloatingPaneLayout, Layout, Run, RunPluginOrAlias, SwapFloatingLayout, SwapTiledLayout,
            TabLayoutInfo, TiledPaneLayout,
        },
    },
    pane_size::Size,
    session_serialization,
};

pub type VteBytes = Vec<u8>;
pub type TabIndex = u32;

type PreparedTerminal = (
    u32,
    bool,
    Option<RunCommand>,
    Result<Box<dyn AsyncReader>>,
    u64,
);

const ORIGIN_PANE_ID_ENV_VAR: &str = "ZELLIJ_ORIGIN_PANE_ID";
const PANE_ID_ENV_VAR: &str = "ZELLIJ_PANE_ID";

fn pane_env_with_origin(pane_env: &PaneEnv, origin_pane_id: Option<PaneId>) -> PaneEnv {
    let mut pane_env = pane_env.clone();
    pane_env.insert(
        ORIGIN_PANE_ID_ENV_VAR.to_owned(),
        origin_pane_id.map(|id| id.to_string()),
    );
    pane_env
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClientTabIndexOrPaneId {
    ClientId(ClientId),
    ClientIdNoFocus(ClientId),
    TabIndex(usize),
    TabIndexNoFocus(usize),
    PaneId(PaneId),
}

/// Instructions related to PTYs (pseudoterminals).
#[derive(Clone, Debug)]
pub enum PtyInstruction {
    SpawnTerminal(
        Option<TerminalAction>,
        Option<String>,
        NewPanePlacement,
        bool, // start suppressed
        ClientTabIndexOrPaneId,
        Option<NotificationEnd>, // completion signal
        bool,                    // set_blocking
        Option<PaneId>,
    ), // bool (if Some) is
    // should_float, String is an optional pane name
    OpenInPlaceEditor(
        PathBuf,
        Option<usize>,
        ClientTabIndexOrPaneId,
        Option<NotificationEnd>,
    ), // Option<usize> is the optional line number
    UpdateActivePane(Option<PaneId>, ClientId),
    PromptReturned(u32),
    GoToTab(TabIndex, ClientId),
    NewTab(
        Option<PathBuf>,
        Option<TerminalAction>,
        Option<TiledPaneLayout>,
        Vec<FloatingPaneLayout>,
        usize,                               // tab_index
        HashMap<RunPluginOrAlias, Vec<u32>>, // plugin_ids
        Option<Vec<CommandOrPlugin>>,        // initial_panes
        bool,                                // block_on_first_terminal
        bool,                                // should change focus to new tab
        (ClientId, bool),                    // bool -> is_web_client
        Option<NotificationEnd>,             // completion signal
    ), // the String is the tab name
    OverrideLayout(
        Option<PathBuf>,                                           // CWD
        Option<TerminalAction>,                                    // Default Shell
        Vec<(TabLayoutInfo, HashMap<RunPluginOrAlias, Vec<u32>>)>, // (layout, plugin_ids) per tab
        bool,                                                      // retain_existing_terminal_panes
        bool,                                                      // retain_existing_plugin_panes
        ClientId,
        Option<NotificationEnd>,
    ),
    ClosePane(PaneId, Option<NotificationEnd>),
    ClosePaneThatWasNotCreated(PaneId, Option<NotificationEnd>),
    /// Forget the child process of a pane whose command exited and was
    /// reaped, whether the pane is held or about to be closed. The generation
    /// names the run that exited, so a late notification from a replaced run
    /// cannot erase the process of the run that replaced it.
    ClearPaneProcessIdentity(PaneId, u64),
    CloseTab(Vec<PaneId>),
    ReRunCommandInPane(PaneId, RunCommand, Option<NotificationEnd>),
    DropToShellInPane {
        pane_id: PaneId,
        shell: Option<PathBuf>,
        working_dir: Option<PathBuf>,
        completion_tx: Option<NotificationEnd>,
    },
    SpawnInPlaceTerminal(
        Option<TerminalAction>,
        Option<String>,
        bool, // close replaced pane
        ClientTabIndexOrPaneId,
        Option<NotificationEnd>, // completion signal
        Option<PaneId>,
    ), // String is an optional pane name
    DumpLayout(SessionLayoutMetadata, ClientId, Option<NotificationEnd>),
    DumpLayoutToPlugin {
        session_layout_metadata: SessionLayoutMetadata,
        plugin_id: PluginId,
        response_channel: crossbeam::channel::Sender<DumpSessionLayoutResponse>,
    },
    LogLayoutToHd(SessionLayoutMetadata),
    SaveSessionToDisk {
        session_name: String,
        session_info: SessionInfo,
        session_layout_metadata: SessionLayoutMetadata,
        completion_tx: Option<NotificationEnd>,
    },
    FillPluginCwd(
        Option<bool>,   // should float
        bool,           // should be opened in place
        bool,           // close_replaced_pane
        Option<String>, // pane title
        RunPluginOrAlias,
        usize,          // tab index
        Option<PaneId>, // pane id to replace if this is to be opened "in-place"
        ClientId,
        Size,
        bool,            // skip cache
        Option<PathBuf>, // if Some, will not fill cwd but just forward the message
        Option<bool>,    // should focus plugin
        Option<FloatingPaneCoordinates>,
        Option<NotificationEnd>,
    ),
    ListClientsMetadata(SessionLayoutMetadata, ClientId, Option<NotificationEnd>),
    Reconfigure {
        client_id: ClientId,
        default_editor: Option<PathBuf>,
        post_command_discovery_hook: Option<String>,
        pane_env: PaneEnv,
    },
    ListClientsToPlugin(SessionLayoutMetadata, PluginId, ClientId),
    ReportPluginCwd(PluginId, PathBuf),
    SendSigintToPaneId(PaneId),
    SendSigkillToPaneId(PaneId),
    GetPanePid {
        pane_id: PaneId,
        response_channel: crossbeam::channel::Sender<GetPanePidResponse>,
    },
    GetPaneRunningCommand {
        pane_id: PaneId,
        response_channel: crossbeam::channel::Sender<GetPaneRunningCommandResponse>,
    },
    GetPaneCwd {
        pane_id: PaneId,
        response_channel: crossbeam::channel::Sender<GetPaneCwdResponse>,
    },
    UpdateAndReportCwds,
    NotifyCwdFromOsc7(u32, PathBuf),
    RunInBackground {
        command: RunCommandAction,
        client_id: ClientId,
        origin_pane_id: Option<PaneId>,
        completion_tx: Option<NotificationEnd>,
    },
    SetResurrectCommand {
        terminal_id: u32,
        resurrect: Option<ResurrectCommand>,
        completion_tx: Option<NotificationEnd>,
    },
    Exit,
}

impl From<&PtyInstruction> for PtyContext {
    fn from(pty_instruction: &PtyInstruction) -> Self {
        match *pty_instruction {
            PtyInstruction::SpawnTerminal(..) => PtyContext::SpawnTerminal,
            PtyInstruction::OpenInPlaceEditor(..) => PtyContext::OpenInPlaceEditor,
            PtyInstruction::UpdateActivePane(..) => PtyContext::UpdateActivePane,
            PtyInstruction::GoToTab(..) => PtyContext::GoToTab,
            PtyInstruction::ClosePane(..) => PtyContext::ClosePane,
            PtyInstruction::ClosePaneThatWasNotCreated(..) => {
                PtyContext::ClosePaneThatWasNotCreated
            },
            PtyInstruction::ClearPaneProcessIdentity(..) => PtyContext::ClearPaneProcessIdentity,
            PtyInstruction::CloseTab(_) => PtyContext::CloseTab,
            PtyInstruction::NewTab(..) => PtyContext::NewTab,
            PtyInstruction::OverrideLayout(..) => PtyContext::OverrideLayout,
            PtyInstruction::ReRunCommandInPane(..) => PtyContext::ReRunCommandInPane,
            PtyInstruction::DropToShellInPane { .. } => PtyContext::DropToShellInPane,
            PtyInstruction::SpawnInPlaceTerminal(..) => PtyContext::SpawnInPlaceTerminal,
            PtyInstruction::DumpLayout(..) => PtyContext::DumpLayout,
            PtyInstruction::DumpLayoutToPlugin { .. } => PtyContext::DumpLayoutToPlugin,
            PtyInstruction::LogLayoutToHd(..) => PtyContext::LogLayoutToHd,
            PtyInstruction::SaveSessionToDisk { .. } => PtyContext::SaveSessionToDisk,
            PtyInstruction::FillPluginCwd(..) => PtyContext::FillPluginCwd,
            PtyInstruction::ListClientsMetadata(..) => PtyContext::ListClientsMetadata,
            PtyInstruction::Reconfigure { .. } => PtyContext::Reconfigure,
            PtyInstruction::ListClientsToPlugin(..) => PtyContext::ListClientsToPlugin,
            PtyInstruction::ReportPluginCwd(..) => PtyContext::ReportPluginCwd,
            PtyInstruction::SendSigintToPaneId(..) => PtyContext::SendSigintToPaneId,
            PtyInstruction::SendSigkillToPaneId(..) => PtyContext::SendSigkillToPaneId,
            PtyInstruction::GetPanePid { .. } => PtyContext::GetPanePid,
            PtyInstruction::GetPaneRunningCommand { .. } => PtyContext::GetPaneRunningCommand,
            PtyInstruction::GetPaneCwd { .. } => PtyContext::GetPaneCwd,
            PtyInstruction::UpdateAndReportCwds => PtyContext::UpdateAndReportCwds,
            PtyInstruction::NotifyCwdFromOsc7(..) => PtyContext::NotifyCwdFromOsc7,
            PtyInstruction::RunInBackground { .. } => PtyContext::RunInBackground,
            PtyInstruction::SetResurrectCommand { .. } => PtyContext::SetResurrectCommand,
            PtyInstruction::PromptReturned(..) => PtyContext::PromptReturned,
            PtyInstruction::Exit => PtyContext::Exit,
        }
    }
}

pub(crate) struct Pty {
    pub active_panes: HashMap<ClientId, PaneId>,
    pub bus: Bus<PtyInstruction>,
    pub id_to_child_pid: HashMap<u32, u32>, // terminal_id => child pid
    // terminal_id => run generation of the child in id_to_child_pid
    child_run_generations: HashMap<u32, u64>,
    next_run_generation: u64,
    session_id: String,
    originating_plugins: HashMap<u32, OriginatingPlugin>,
    origin_pane_ids: HashMap<u32, PaneId>,
    debug_to_file: bool,
    task_handles: HashMap<u32, JoinHandle<()>>, // terminal_id to join-handle
    default_editor: Option<PathBuf>,
    post_command_discovery_hook: Option<String>,
    pane_env: PaneEnv,
    plugin_cwds: HashMap<u32, PathBuf>,   // plugin_id -> cwd
    terminal_cwds: HashMap<u32, PathBuf>, // terminal_id -> cwd
    pane_activity_flags: HashMap<u32, std::sync::Arc<std::sync::atomic::AtomicBool>>,
    terminal_cmds: HashMap<u32, Vec<String>>,
    terminal_foreground_cmds: HashMap<u32, Vec<String>>,
    session_context_pane: Option<PaneId>,
    terminals_with_prompt_markers: HashSet<u32>,
    terminals_with_shell_cwd: HashSet<u32>,
    resurrect_commands: HashMap<u32, ResurrectCommand>,
}

pub(crate) fn pty_thread_main(mut pty: Pty, layout: Box<Layout>, session_id: String) -> Result<()> {
    pty.session_id = session_id.clone();
    loop {
        let (event, mut err_ctx) = pty.bus.recv().expect("failed to receive event on channel");
        err_ctx.add_call(ContextType::Pty((&event).into()));
        match event {
            PtyInstruction::SpawnTerminal(
                terminal_action,
                name,
                new_pane_placement,
                start_suppressed,
                client_or_tab_index,
                completion_tx,
                set_blocking,
                origin_pane_id,
            ) => {
                let err_context =
                    || format!("failed to spawn terminal for {:?}", client_or_tab_index);

                let (hold_on_close, run_command, pane_title, open_file_payload) =
                    match &terminal_action {
                        Some(TerminalAction::RunCommand(run_command)) => (
                            run_command.hold_on_close,
                            Some(run_command.clone()),
                            if name.is_some() {
                                // User explicitly provided a name — use it regardless
                                // of use_terminal_title
                                name
                            } else if run_command.use_terminal_title {
                                None
                            } else {
                                Some(run_command.to_string())
                            },
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
                let run_generation = pty.mint_run_generation();
                match pty
                    .spawn_terminal(
                        terminal_action,
                        client_or_tab_index,
                        origin_pane_id,
                        run_generation,
                    )
                    .with_context(err_context)
                {
                    Ok((pid, starts_held)) => {
                        let hold_for_command = if starts_held {
                            run_command.clone()
                        } else {
                            None
                        };

                        pty.bus
                            .senders
                            .send_to_screen(ScreenInstruction::NewPane(
                                PaneId::Terminal(pid),
                                pane_title,
                                hold_for_command,
                                invoked_with,
                                new_pane_placement,
                                start_suppressed,
                                client_or_tab_index,
                                completion_tx,
                                set_blocking,
                            ))
                            .with_context(err_context)?;

                        // if this command originated in a plugin, we send the plugin back an event
                        // to let it know the command started and which pane_id it has. This
                        // is sent after the pane is queued for the screen: the plugin may
                        // immediately close the pane id it learns about, and that close must
                        // reach the screen after the pane has been created
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
                                        hold_for_command,
                                        invoked_with,
                                        new_pane_placement,
                                        start_suppressed,
                                        client_or_tab_index,
                                        completion_tx,
                                        set_blocking,
                                    ))
                                    .with_context(err_context)?;
                                if let Some(run_command) = run_command {
                                    send_command_not_found_to_screen(
                                        pty.bus.senders.clone(),
                                        *terminal_id,
                                        run_command.clone(),
                                        Some(run_generation),
                                    )
                                    .with_context(err_context)?;
                                }
                            } else {
                                log::error!("Failed to spawn terminal: {:?}", err);
                                if let Some(mut completion_tx) = completion_tx {
                                    completion_tx.set_error_message(format!("{err:#}"));
                                }
                                pty.close_pane(PaneId::Terminal(*terminal_id))
                                    .with_context(err_context)?;
                            }
                        },
                        _ => {
                            if let Some(mut completion_tx) = completion_tx {
                                completion_tx.set_error_message(format!("{err:#}"));
                            }
                            Err::<(), _>(err).non_fatal();
                        },
                    },
                }
            },
            PtyInstruction::SpawnInPlaceTerminal(
                terminal_action,
                name,
                close_replaced_pane,
                client_id_tab_index_or_pane_id,
                completion_tx,
                origin_pane_id,
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
                        if run_command.use_terminal_title {
                            None
                        } else {
                            Some(name.unwrap_or_else(|| run_command.to_string()))
                        },
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
                let run_generation = pty.mint_run_generation();
                match pty
                    .spawn_terminal(
                        terminal_action,
                        client_id_tab_index_or_pane_id,
                        origin_pane_id,
                        run_generation,
                    )
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
                                close_replaced_pane,
                                client_id_tab_index_or_pane_id,
                                completion_tx,
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
                                        close_replaced_pane,
                                        client_id_tab_index_or_pane_id,
                                        completion_tx,
                                    ))
                                    .with_context(err_context)?;
                                if let Some(run_command) = run_command {
                                    send_command_not_found_to_screen(
                                        pty.bus.senders.clone(),
                                        *terminal_id,
                                        run_command.clone(),
                                        Some(run_generation),
                                    )
                                    .with_context(err_context)?;
                                }
                            } else {
                                log::error!("Failed to spawn terminal: {:?}", err);
                                if let Some(mut completion_tx) = completion_tx {
                                    completion_tx.set_error_message(format!("{err:#}"));
                                }
                                pty.close_pane(PaneId::Terminal(*terminal_id))
                                    .with_context(err_context)?;
                            }
                        },
                        _ => {
                            if let Some(mut completion_tx) = completion_tx {
                                completion_tx.set_error_message(format!("{err:#}"));
                            }
                            Err::<(), _>(err).non_fatal();
                        },
                    },
                }
            },
            PtyInstruction::OpenInPlaceEditor(
                temp_file,
                line_number,
                client_tab_index_or_pane_id,
                completion_tx,
            ) => {
                let err_context = || format!("failed to open in-place editor for client");

                let run_generation = pty.mint_run_generation();
                match pty.spawn_terminal(
                    Some(TerminalAction::OpenFile(OpenFilePayload::new(
                        temp_file,
                        line_number,
                        None,
                    ))),
                    client_tab_index_or_pane_id,
                    None,
                    run_generation,
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
                        if let Some(mut completion_tx) = completion_tx {
                            completion_tx.set_error_message(format!("{e:#}"));
                        }
                        Err::<(), _>(e).with_context(err_context).non_fatal();
                    },
                }
            },
            PtyInstruction::UpdateActivePane(pane_id, client_id) => {
                pty.set_active_pane(pane_id, client_id);
            },
            PtyInstruction::GoToTab(tab_index, client_id) => {
                pty.bus
                    .senders
                    .send_to_screen(ScreenInstruction::GoToTab(tab_index, Some(client_id), None))
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
                initial_panes,
                block_on_first_terminal,
                should_change_focus_to_new_tab,
                client_id_and_is_web_client,
                completion_tx,
            ) => {
                let err_context = || "failed to open new tab";
                log::info!(
                    "PtyInstruction::NewTab: spawning terminals for tab {}",
                    tab_index
                );

                let floating_panes_layout = if floating_panes_layout.is_empty() {
                    layout.new_tab().1
                } else {
                    floating_panes_layout
                };
                if let Err(err) = pty.spawn_terminals_for_layout(
                    cwd,
                    tab_layout.unwrap_or_else(|| layout.new_tab().0),
                    floating_panes_layout,
                    terminal_action.clone(),
                    plugin_ids,
                    initial_panes,
                    tab_index,
                    block_on_first_terminal,
                    should_change_focus_to_new_tab,
                    client_id_and_is_web_client,
                    completion_tx,
                ) {
                    Err::<(), _>(err).with_context(err_context).non_fatal();
                }
            },
            PtyInstruction::OverrideLayout(
                cwd,
                default_shell,
                tab_layouts_with_plugin_ids,
                retain_existing_terminal_panes,
                retain_existing_plugin_panes,
                client_id,
                completion_tx,
            ) => {
                let err_context = || "failed to override layout";

                let mut all_tab_results = Vec::new();

                // Process each tab
                for (tab_layout_info, plugin_ids) in tab_layouts_with_plugin_ids {
                    match pty.spawn_terminals_for_layout_override(
                        cwd.clone(),
                        tab_layout_info.tiled_layout,
                        tab_layout_info.floating_layouts,
                        default_shell.clone(),
                        plugin_ids,
                        tab_layout_info.tab_index,
                        tab_layout_info.tab_name,
                        client_id,
                        tab_layout_info.swap_tiled_layouts,
                        tab_layout_info.swap_floating_layouts,
                        retain_existing_terminal_panes,
                        retain_existing_plugin_panes,
                    ) {
                        Ok(tab_result) => all_tab_results.push(tab_result),
                        Err(e) => {
                            log::error!(
                                "Failed to spawn terminals for tab {}: {:?}",
                                tab_layout_info.tab_index,
                                e
                            );
                            // Continue with other tabs (best-effort approach)
                        },
                    }
                }

                // Send all results back to screen in one message
                pty.bus
                    .senders
                    .send_to_screen(ScreenInstruction::OverrideLayoutComplete(
                        all_tab_results,
                        retain_existing_terminal_panes,
                        retain_existing_plugin_panes,
                        client_id,
                        completion_tx,
                    ))
                    .with_context(err_context)?;
            },
            PtyInstruction::ClosePane(id, completion_tx) => {
                let close_result = pty.close_pane(id);
                let action_unblocks_its_own_client = completion_tx.is_some();
                drop(completion_tx);
                close_result
                    .and_then(|_| {
                        if action_unblocks_its_own_client {
                            Ok(())
                        } else {
                            pty.bus
                                .senders
                                .send_to_server(ServerInstruction::UnblockInputThread)
                        }
                    })
                    .with_context(|| format!("failed to close pane {:?}", id))?;
            },
            PtyInstruction::ClosePaneThatWasNotCreated(id, completion_tx) => {
                let close_result = pty.close_pane(id);
                drop(completion_tx);
                close_result.with_context(|| format!("failed to close pane {:?}", id))?;
            },
            PtyInstruction::ClearPaneProcessIdentity(PaneId::Terminal(id), run_generation) => {
                pty.clear_exited_process_identity(id, run_generation);
            },
            PtyInstruction::ClearPaneProcessIdentity(PaneId::Plugin(_), _) => {},
            PtyInstruction::CloseTab(ids) => {
                pty.close_tab(ids)
                    .and_then(|_| {
                        pty.bus
                            .senders
                            .send_to_server(ServerInstruction::UnblockInputThread)
                    })
                    .context("failed to close tabs")?;
            },
            PtyInstruction::ReRunCommandInPane(pane_id, run_command, completion_tx) => {
                let err_context = || format!("failed to rerun command in pane {:?}", pane_id);
                let run_generation = pty.mint_run_generation();

                match pty
                    .rerun_command_in_pane(pane_id, run_command.clone(), run_generation)
                    .with_context(err_context)
                {
                    Ok(..) => drop(completion_tx),
                    Err(err) => match err.downcast_ref::<ZellijError>() {
                        Some(ZellijError::CommandNotFound { terminal_id, .. }) => {
                            if run_command.hold_on_close {
                                pty.bus
                                    .senders
                                    .send_to_screen(ScreenInstruction::PtyRunBytes(
                                        *terminal_id,
                                        run_generation,
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
                                        Some(run_generation),
                                    ))
                                    .with_context(err_context)?;
                                drop(completion_tx);
                            } else if let Some(mut completion_tx) = completion_tx {
                                completion_tx.set_error_message(format!("{err:#}"));
                            }
                        },
                        _ => {
                            if let Some(mut completion_tx) = completion_tx {
                                completion_tx.set_error_message(format!("{err:#}"));
                            }
                            Err::<(), _>(err).non_fatal();
                        },
                    },
                }
            },
            PtyInstruction::DropToShellInPane {
                pane_id,
                shell,
                working_dir,
                completion_tx,
            } => {
                let err_context = || format!("failed to rerun command in pane {:?}", pane_id);
                let run_generation = pty.mint_run_generation();

                // TODO: get configured default_shell from screen/tab as an option and default to
                // this otherwise (also look for a place that turns get_default_shell into a
                // RunCommand, we might have done this before)
                let run_command = RunCommand {
                    command: shell.unwrap_or_else(|| default_shell_from(&pty.pane_env)),
                    hold_on_close: false,
                    hold_on_start: false,
                    cwd: working_dir,
                    ..Default::default()
                };
                match pty
                    .rerun_command_in_pane(pane_id, run_command.clone(), run_generation)
                    .with_context(err_context)
                {
                    Ok(..) => drop(completion_tx),
                    Err(err) => match err.downcast_ref::<ZellijError>() {
                        Some(ZellijError::CommandNotFound { .. }) => {
                            if let Some(mut completion_tx) = completion_tx {
                                completion_tx.set_error_message(format!("{err:#}"));
                            }
                        },
                        _ => {
                            if let Some(mut completion_tx) = completion_tx {
                                completion_tx.set_error_message(format!("{err:#}"));
                            }
                            Err::<(), _>(err).non_fatal();
                        },
                    },
                }
            },
            PtyInstruction::DumpLayout(mut session_layout_metadata, client_id, completion_tx) => {
                let err_context = || format!("Failed to dump layout");
                pty.populate_session_layout_metadata(&mut session_layout_metadata);
                match session_serialization::serialize_session_layout(
                    session_layout_metadata.into(),
                ) {
                    Ok((kdl_layout, _pane_contents)) => {
                        pty.bus
                            .senders
                            .send_to_server(ServerInstruction::Log(
                                vec![kdl_layout],
                                client_id,
                                completion_tx,
                            ))
                            .with_context(err_context)
                            .non_fatal();
                    },
                    Err(e) => {
                        pty.bus
                            .senders
                            .send_to_server(ServerInstruction::Log(
                                vec![e.to_owned()],
                                client_id,
                                completion_tx,
                            ))
                            .with_context(err_context)
                            .non_fatal();
                    },
                }
            },
            PtyInstruction::ListClientsMetadata(
                mut session_layout_metadata,
                client_id,
                completion_tx,
            ) => {
                let err_context = || format!("Failed to dump layout");
                pty.populate_session_layout_metadata(&mut session_layout_metadata);
                pty.bus
                    .senders
                    .send_to_server(ServerInstruction::Log(
                        vec![format!(
                            "{}",
                            session_layout_metadata.list_clients_metadata(),
                        )],
                        client_id,
                        completion_tx,
                    ))
                    .with_context(err_context)
                    .non_fatal();
            },
            PtyInstruction::DumpLayoutToPlugin {
                mut session_layout_metadata,
                plugin_id,
                response_channel,
            } => {
                let err_context = || format!("Failed to dump layout");
                pty.populate_session_layout_metadata(&mut session_layout_metadata);
                pty.bus
                    .senders
                    .send_to_plugin(PluginInstruction::DumpLayoutToPlugin {
                        session_layout_metadata,
                        plugin_id,
                        response_channel,
                    })
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
            PtyInstruction::ReportPluginCwd(plugin_id, cwd) => {
                pty.plugin_cwds.insert(plugin_id, cwd);
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
            PtyInstruction::SaveSessionToDisk {
                // The cache is keyed by the server's own session id; the name in
                // the instruction is no longer needed.
                session_name: _,
                session_info,
                mut session_layout_metadata,
                completion_tx: _completion_tx, // Dropped at end to signal completion
            } => {
                pty.populate_session_layout_metadata(&mut session_layout_metadata);
                match session_serialization::serialize_session_layout(
                    session_layout_metadata.into(),
                ) {
                    Ok(kdl_and_files) => {
                        // Cache is keyed by the stable session id, not the name.
                        match write_session_state_to_disk(
                            session_id.clone(),
                            session_info,
                            kdl_and_files.clone(),
                        ) {
                            Ok(()) => {
                                // Update session save time for plugin query
                                let timestamp_millis = std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .unwrap_or_default()
                                    .as_millis()
                                    as u64;
                                let _ = pty.bus.senders.send_to_plugin(
                                    PluginInstruction::UpdateSessionSaveTime(timestamp_millis),
                                );
                            },
                            Err(e) => {
                                log::error!("Failed to persist session state to disk: {}", e);
                            },
                        }

                        let _ = pty.bus.senders.send_to_background_jobs(
                            BackgroundJob::ReportLayoutInfo(kdl_and_files),
                        );
                    },
                    Err(e) => {
                        log::error!("Failed to serialize layout: {}", e);
                    },
                };
            },
            PtyInstruction::FillPluginCwd(
                should_float,
                should_be_open_in_place,
                close_replaced_pane,
                pane_title,
                run,
                tab_index,
                pane_id_to_replace,
                client_id,
                size,
                skip_cache,
                cwd,
                should_focus_plugin,
                floating_pane_coordinates,
                completion_tx,
            ) => {
                pty.fill_plugin_cwd(
                    should_float,
                    should_be_open_in_place,
                    close_replaced_pane,
                    pane_title,
                    run,
                    tab_index,
                    pane_id_to_replace,
                    client_id,
                    size,
                    skip_cache,
                    cwd,
                    should_focus_plugin,
                    floating_pane_coordinates,
                    completion_tx,
                )?;
            },
            PtyInstruction::Reconfigure {
                default_editor,
                post_command_discovery_hook,
                pane_env,
                client_id: _,
            } => {
                pty.reconfigure(default_editor, post_command_discovery_hook, pane_env);
            },
            PtyInstruction::SendSigintToPaneId(pane_id) => {
                pty.send_sigint_to_pane(pane_id);
            },
            PtyInstruction::SendSigkillToPaneId(pane_id) => {
                pty.send_sigkill_to_pane(pane_id);
            },
            PtyInstruction::GetPanePid {
                pane_id,
                response_channel,
            } => {
                let response = pty.get_pane_pid(pane_id);
                let _ = response_channel.send(response);
            },
            PtyInstruction::GetPaneRunningCommand {
                pane_id,
                response_channel,
            } => {
                let response = pty.get_pane_running_command(pane_id);
                let _ = response_channel.send(response);
            },
            PtyInstruction::GetPaneCwd {
                pane_id,
                response_channel,
            } => {
                let response = pty.get_pane_cwd(pane_id);
                let _ = response_channel.send(response);
            },
            PtyInstruction::UpdateAndReportCwds => {
                pty.update_and_report_cwds();
            },
            PtyInstruction::NotifyCwdFromOsc7(terminal_id, path) => {
                pty.notify_cwd_from_osc7(terminal_id, path);
            },
            PtyInstruction::RunInBackground {
                command,
                client_id,
                origin_pane_id,
                completion_tx,
            } => match pty.run_in_background(command, client_id, origin_pane_id) {
                Ok(()) => drop(completion_tx),
                Err(e) => {
                    if let Some(mut completion_tx) = completion_tx {
                        completion_tx.set_error_message(format!("{e:#}"));
                    }
                    Err::<(), _>(e).non_fatal();
                },
            },
            PtyInstruction::SetResurrectCommand {
                terminal_id,
                resurrect,
                completion_tx,
            } => {
                if let Err(message) = pty.set_resurrect_command(terminal_id, resurrect) {
                    if let Some(mut completion_tx) = completion_tx {
                        completion_tx.set_error_message(message);
                    }
                }
            },
            PtyInstruction::PromptReturned(terminal_id) => {
                pty.terminals_with_prompt_markers.insert(terminal_id);
                if pty.session_context_pane == Some(PaneId::Terminal(terminal_id)) {
                    pty.report_session_context(ContextTrigger::PromptReturned);
                }
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
        post_command_discovery_hook: Option<String>,
        pane_env: PaneEnv,
    ) -> Self {
        Pty {
            active_panes: HashMap::new(),
            bus,
            id_to_child_pid: HashMap::new(),
            child_run_generations: HashMap::new(),
            next_run_generation: 0,
            session_id: String::new(),
            debug_to_file,
            task_handles: HashMap::new(),
            default_editor,
            originating_plugins: HashMap::new(),
            origin_pane_ids: HashMap::new(),
            post_command_discovery_hook,
            pane_env,
            plugin_cwds: HashMap::new(),
            terminal_cwds: HashMap::new(),
            pane_activity_flags: HashMap::new(),
            terminal_cmds: HashMap::new(),
            terminal_foreground_cmds: HashMap::new(),
            session_context_pane: None,
            terminals_with_prompt_markers: HashSet::new(),
            terminals_with_shell_cwd: HashSet::new(),
            resurrect_commands: HashMap::new(),
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
                let shell = default_shell_from(&self.pane_env);
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
    fn cwd_of_pane(&self, pane_id: &PaneId) -> Option<PathBuf> {
        match pane_id {
            PaneId::Terminal(terminal_id) => self
                .id_to_child_pid
                .get(terminal_id)
                .and_then(|&pid| {
                    self.bus
                        .os_input
                        .as_ref()
                        .and_then(|input| input.get_cwd(pid))
                })
                .or_else(|| self.terminal_cwds.get(terminal_id).cloned()),
            PaneId::Plugin(plugin_id) => self.plugin_cwds.get(plugin_id).cloned(),
        }
    }
    fn run_in_background(
        &self,
        command: RunCommandAction,
        client_id: ClientId,
        origin_pane_id: Option<PaneId>,
    ) -> Result<()> {
        let base_cwd = origin_pane_id
            .or_else(|| self.active_panes.get(&client_id).copied())
            .and_then(|pane_id| self.cwd_of_pane(&pane_id));
        let cwd = match (command.cwd, base_cwd) {
            (Some(cwd), Some(base_cwd)) if cwd.is_relative() => Some(base_cwd.join(cwd)),
            (Some(cwd), _) => Some(cwd),
            (None, base_cwd) => base_cwd,
        };
        let mut env = pane_env_with_origin(&self.pane_env, origin_pane_id);
        let pane_id = match origin_pane_id {
            Some(PaneId::Terminal(terminal_id)) => Some(terminal_id.to_string()),
            _ => None,
        };
        env.insert(PANE_ID_ENV_VAR.to_owned(), pane_id);
        self.bus
            .senders
            .send_to_background_jobs(BackgroundJob::RunInBackground {
                command: command.command,
                args: command.args,
                cwd,
                env,
            })
            .with_context(|| format!("failed to run command in background for client {client_id}"))
    }
    fn fill_cwd(&self, terminal_action: &mut TerminalAction, client_id: ClientId) {
        let cwd = match terminal_action {
            TerminalAction::RunCommand(run_command) => &mut run_command.cwd,
            TerminalAction::OpenFile(payload) => &mut payload.cwd,
        };
        if cwd.is_none() {
            *cwd = self
                .active_panes
                .get(&client_id)
                .and_then(|pane| match pane {
                    PaneId::Plugin(plugin_id) => self.plugin_cwds.get(plugin_id).cloned(),
                    PaneId::Terminal(id) => {
                        // Try to get CWD from OS, fall back to cached value
                        self.id_to_child_pid
                            .get(id)
                            .and_then(|&pid| {
                                self.bus
                                    .os_input
                                    .as_ref()
                                    .and_then(|input| input.get_cwd(pid))
                            })
                            .or_else(|| self.terminal_cwds.get(id).cloned())
                    },
                })
        };
    }
    fn fill_cwd_from_pane_id(&self, terminal_action: &mut TerminalAction, pane_id: &PaneId) {
        let cwd = match terminal_action {
            TerminalAction::RunCommand(run_command) => &mut run_command.cwd,
            TerminalAction::OpenFile(payload) => &mut payload.cwd,
        };
        if cwd.is_none() {
            *cwd = match pane_id {
                PaneId::Terminal(terminal_pane_id) => {
                    // Try to get CWD from OS, fall back to cached value
                    self.id_to_child_pid
                        .get(terminal_pane_id)
                        .and_then(|&pid| {
                            self.bus
                                .os_input
                                .as_ref()
                                .and_then(|input| input.get_cwd(pid))
                        })
                        .or_else(|| self.terminal_cwds.get(terminal_pane_id).cloned())
                },
                PaneId::Plugin(plugin_id) => self.plugin_cwds.get(plugin_id).cloned(),
            };
        };
    }
    fn mint_run_generation(&mut self) -> u64 {
        let run_generation = self.next_run_generation;
        self.next_run_generation = self.next_run_generation.wrapping_add(1);
        run_generation
    }
    #[cfg(test)]
    fn record_child_pid(&mut self, terminal_id: u32, child_pid: u32, run_generation: u64) {
        self.remember_run(terminal_id, Some(child_pid), run_generation);
    }
    fn remember_run(&mut self, terminal_id: u32, child_pid: Option<u32>, run_generation: u64) {
        self.child_run_generations
            .insert(terminal_id, run_generation);
        if let Some(child_pid) = child_pid {
            self.id_to_child_pid.insert(terminal_id, child_pid);
        }
        // The endpoint was inserted by spawn, still unbound. Binding it
        // here is what lets a stamped write for this run through, and what
        // makes a write stamped for the previous run fail.
        if let Some(os_input) = self.bus.os_input.as_ref() {
            os_input.bind_terminal_generation(terminal_id, run_generation);
        }
        // Only adopt the run once the endpoint it names is bound. Until then
        // writes keep the previous stamp instead of being rejected against
        // an endpoint that does not represent the new process yet.
        let _ = self
            .bus
            .senders
            .send_to_pty_writer(PtyWriteInstruction::AdoptRun(terminal_id, run_generation));
    }
    fn announce_terminal_run(&self, terminal_id: u32, run_generation: u64) {
        let _ = self
            .bus
            .senders
            .send_to_screen(ScreenInstruction::TerminalRunStarted(
                terminal_id,
                run_generation,
            ));
    }
    fn spawn_terminal_reader(
        &mut self,
        terminal_id: u32,
        reader: Box<dyn AsyncReader>,
        run_generation: u64,
        announce_run: bool,
        context: String,
    ) {
        if announce_run {
            self.announce_terminal_run(terminal_id, run_generation);
        }
        let activity_flag = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let activity_flag_for_task = activity_flag.clone();
        let senders = self.bus.senders.clone();
        let debug_to_file = self.debug_to_file;
        let session_id = self.session_id.clone();
        let terminal_bytes = async_runtime().spawn(async move {
            TerminalBytes::new(
                terminal_id,
                run_generation,
                session_id,
                reader,
                senders,
                debug_to_file,
                activity_flag_for_task,
            )
            .listen()
            .await
            .context(context)
            .fatal();
        });
        self.task_handles.insert(terminal_id, terminal_bytes);
        self.pane_activity_flags.insert(terminal_id, activity_flag);
    }
    fn forget_child_pid(&mut self, terminal_id: u32) -> Option<u32> {
        self.child_run_generations.remove(&terminal_id);
        self.id_to_child_pid.remove(&terminal_id)
    }
    /// A pane's command exited and was reaped, so its PID may now belong to an
    /// unrelated process: forget it, unless the pane has since started another
    /// run whose process must stay addressable.
    fn clear_exited_process_identity(&mut self, terminal_id: u32, run_generation: u64) {
        if self.child_run_generations.get(&terminal_id) == Some(&run_generation) {
            self.forget_child_pid(terminal_id);
        }
    }
    pub fn spawn_terminal(
        &mut self,
        terminal_action: Option<TerminalAction>,
        client_or_tab_index: ClientTabIndexOrPaneId,
        origin_pane_id: Option<PaneId>,
        run_generation: u64,
    ) -> Result<(u32, bool)> {
        // bool is starts_held
        let err_context = || format!("failed to spawn terminal for {:?}", client_or_tab_index);

        // returns the terminal id
        let terminal_action = match client_or_tab_index {
            ClientTabIndexOrPaneId::ClientId(client_id)
            | ClientTabIndexOrPaneId::ClientIdNoFocus(client_id) => {
                let mut terminal_action =
                    terminal_action.unwrap_or_else(|| self.get_default_terminal(None, None));
                self.fill_cwd(&mut terminal_action, client_id);
                terminal_action
            },
            ClientTabIndexOrPaneId::TabIndex(_) | ClientTabIndexOrPaneId::TabIndexNoFocus(_) => {
                terminal_action.unwrap_or_else(|| self.get_default_terminal(None, None))
            },
            ClientTabIndexOrPaneId::PaneId(pane_id) => {
                let mut terminal_action =
                    terminal_action.unwrap_or_else(|| self.get_default_terminal(None, None));
                self.fill_cwd_from_pane_id(&mut terminal_action, &pane_id);
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
            if let Some(origin_pane_id) = origin_pane_id {
                self.origin_pane_ids.insert(terminal_id, origin_pane_id);
            }
            return Ok((terminal_id, starts_held));
        }

        let originating_command_plugin = Arc::new(originating_command_plugin.clone());
        let originating_edit_plugin = Arc::new(originating_edit_plugin.clone());
        let quit_cb = Box::new({
            let senders = self.bus.senders.clone();
            move |pane_id, exit_status, command| {
                // The child has been reaped, so its PID may be reused by an
                // unrelated process: forget it before anything can signal it.
                let _ = senders.send_to_pty(PtyInstruction::ClearPaneProcessIdentity(
                    pane_id,
                    run_generation,
                ));
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

                send_child_run_exited(
                    &senders,
                    pane_id,
                    exit_status,
                    run_generation,
                    if hold_on_close { Some(command) } else { None },
                );
            }
        });
        let pane_env = pane_env_with_origin(&self.pane_env, origin_pane_id);
        let spawn_result = self
            .bus
            .os_input
            .as_mut()
            .context("no OS I/O interface found")
            .and_then(|os_input| {
                os_input.spawn_terminal(
                    terminal_action,
                    quit_cb,
                    self.default_editor.clone(),
                    &pane_env,
                )
            });
        if let Some(origin_pane_id) = origin_pane_id {
            let spawned_terminal_id = match &spawn_result {
                Ok((terminal_id, ..)) => Some(*terminal_id),
                Err(err) => match err.downcast_ref::<ZellijError>() {
                    Some(ZellijError::CommandNotFound { terminal_id, .. }) => Some(*terminal_id),
                    _ => None,
                },
            };
            if let Some(spawned_terminal_id) = spawned_terminal_id {
                self.origin_pane_ids
                    .insert(spawned_terminal_id, origin_pane_id);
            }
        }
        let (terminal_id, reader, child_pid): (u32, Box<dyn AsyncReader>, Option<u32>) =
            spawn_result.with_context(err_context)?;
        self.remember_run(terminal_id, child_pid, run_generation);
        if let Some(child_pid) = child_pid {
            self.capture_initial_cwd(terminal_id, child_pid);
        }
        self.spawn_terminal_reader(
            terminal_id,
            reader,
            run_generation,
            true,
            format!("failed to run async task for terminal {terminal_id}"),
        );

        let starts_held = false;
        Ok((terminal_id, starts_held))
    }
    fn rollback_unapplied_layout_terminals(&mut self, terminal_ids: Vec<u32>) {
        for terminal_id in terminal_ids {
            if let Err(err) = self.close_pane(PaneId::Terminal(terminal_id)) {
                log::error!(
                    "failed to release terminal {terminal_id} while rolling back a layout: {err:#}"
                );
            }
            crate::screen::release_uninstalled_terminal(terminal_id);
            let _ = self
                .bus
                .senders
                .send_to_screen(ScreenInstruction::TerminalClosed(terminal_id));
        }
    }
    fn fail_layout_spawn(
        &mut self,
        error: anyError,
        terminal_ids: Vec<u32>,
        mut completion_tx: Option<NotificationEnd>,
    ) -> Result<()> {
        self.rollback_unapplied_layout_terminals(terminal_ids);
        if let Some(completion_tx) = completion_tx.as_mut() {
            completion_tx.set_error_message(format!("{error:#}"));
        }
        Err(error)
    }
    fn close_unstarted_layout_terminal(&mut self, terminal_id: u32, run_generation: u64) {
        if let Err(err) = self.close_pane(PaneId::Terminal(terminal_id)) {
            log::error!("failed to close unstarted layout terminal {terminal_id}: {err:#}");
        }
        let _ = self
            .bus
            .senders
            .send_to_screen(ScreenInstruction::ClosePane(
                PaneId::Terminal(terminal_id),
                None,
                None,
                Some(2),
                ClosePaneSource::External,
                Some(run_generation),
            ));
    }
    pub fn spawn_terminals_for_layout(
        &mut self,
        cwd: Option<PathBuf>,
        layout: TiledPaneLayout,
        floating_panes_layout: Vec<FloatingPaneLayout>,
        default_shell: Option<TerminalAction>,
        plugin_ids: HashMap<RunPluginOrAlias, Vec<u32>>,
        initial_panes: Option<Vec<CommandOrPlugin>>,
        tab_index: usize,
        block_on_first_terminal: bool,
        should_change_focus_to_new_tab: bool,
        client_id_and_is_web_client: (ClientId, bool),
        completion_tx: Option<NotificationEnd>,
    ) -> Result<()> {
        let err_context = || format!("failed to spawn terminals for layout for");

        let mut default_shell =
            default_shell.unwrap_or_else(|| self.get_default_terminal(cwd, None));
        let (client_id, is_web_client) = client_id_and_is_web_client;
        self.fill_cwd(&mut default_shell, client_id);

        // Match initial_panes commands to empty slots in the layout
        let mut layout = layout;
        if let Some(ref initial_panes_vec) = initial_panes {
            for initial_pane in initial_panes_vec.iter() {
                if let CommandOrPlugin::Command(run_command_action) = initial_pane {
                    let run_command: RunCommand = run_command_action.clone().into();
                    if !layout.replace_next_empty_slot_with_run(Run::Command(run_command)) {
                        log::warn!("More initial_panes provided than empty slots available");
                        break;
                    }
                } else if let CommandOrPlugin::File(file_to_open) = initial_pane {
                    if !layout.replace_next_empty_slot_with_run(Run::EditFile(
                        file_to_open.path.clone(),
                        file_to_open.line_number,
                        file_to_open.cwd.clone(),
                    )) {
                        log::warn!("More initial_panes provided than empty slots available");
                        break;
                    }
                }
                // Skip CommandOrPlugin::Plugin entries (already handled by plugin thread)
            }
        }

        let extracted_run_instructions = layout.extract_run_instructions();
        let extracted_floating_run_instructions = floating_panes_layout
            .iter()
            .filter(|f| !f.already_running)
            .map(|f| f.run.clone());
        let mut new_pane_pids: Vec<PreparedTerminal> = vec![];
        let mut new_floating_panes_pids: Vec<PreparedTerminal> = vec![];

        let mut originating_plugins_to_inform = vec![];
        let mut spawn_error = None;

        for run_instruction in extracted_run_instructions {
            let originating_plugin = run_instruction.as_ref().and_then(|r| {
                if let Run::Command(run_command) = r {
                    run_command.originating_plugin.clone()
                } else {
                    None
                }
            });
            match self.apply_run_instruction(run_instruction, default_shell.clone()) {
                Ok(Some(new_pane_data)) => {
                    let terminal_id = new_pane_data.0;
                    new_pane_pids.push(new_pane_data);
                    if let Some(originating_plugin) = originating_plugin {
                        originating_plugins_to_inform.push((terminal_id, originating_plugin));
                    }
                },
                Ok(None) => {},
                Err(err) => {
                    spawn_error = Some(err);
                    break;
                },
            }
        }
        if spawn_error.is_none() {
            for run_instruction in extracted_floating_run_instructions {
                let originating_plugin = run_instruction.as_ref().and_then(|r| {
                    if let Run::Command(run_command) = r {
                        run_command.originating_plugin.clone()
                    } else {
                        None
                    }
                });
                match self.apply_run_instruction(run_instruction, default_shell.clone()) {
                    Ok(Some(new_pane_data)) => {
                        let terminal_id = new_pane_data.0;
                        new_floating_panes_pids.push(new_pane_data);
                        if let Some(originating_plugin) = originating_plugin {
                            originating_plugins_to_inform.push((terminal_id, originating_plugin));
                        }
                    },
                    Ok(None) => {},
                    Err(err) => {
                        spawn_error = Some(err);
                        break;
                    },
                }
            }
        }
        if let Some(error) = spawn_error {
            let terminal_ids = new_pane_pids
                .iter()
                .chain(new_floating_panes_pids.iter())
                .map(|pane| pane.0)
                .collect();
            return self.fail_layout_spawn(error, terminal_ids, completion_tx);
        }

        // Option<RunCommand> should only be Some if the pane starts held
        let new_tab_pane_ids: Vec<(u32, Option<RunCommand>)> = new_pane_pids
            .iter()
            .map(|(terminal_id, starts_held, run_command, _, _)| {
                if *starts_held {
                    (*terminal_id, run_command.clone())
                } else {
                    (*terminal_id, None)
                }
            })
            .collect();
        let new_tab_floating_pane_ids: Vec<(u32, Option<RunCommand>)> = new_floating_panes_pids
            .iter()
            .map(|(terminal_id, starts_held, run_command, _, _)| {
                if *starts_held {
                    (*terminal_id, run_command.clone())
                } else {
                    (*terminal_id, None)
                }
            })
            .collect();

        // Track the first terminal_id if blocking is requested
        let first_initial_pane_terminal_id = if block_on_first_terminal && !new_pane_pids.is_empty()
        {
            Some(new_pane_pids[0].0)
        } else {
            None
        };

        // Prepare blocking_terminal for ApplyLayout
        let (direct_completion_tx, blocking_terminal) =
            if let Some(terminal_id) = first_initial_pane_terminal_id {
                (None, completion_tx.map(|tx| (terminal_id, tx)))
            } else {
                (completion_tx, None)
            };

        log::info!(
            "spawn_terminals_for_layout: {} tiled + {} floating panes created, sending ApplyLayout",
            new_tab_pane_ids.len(),
            new_tab_floating_pane_ids.len()
        );
        if let Err(error) = self
            .bus
            .senders
            .send_to_screen(ScreenInstruction::ApplyLayout(
                layout,
                floating_panes_layout,
                new_tab_pane_ids.clone(),
                new_tab_floating_pane_ids.clone(),
                plugin_ids,
                tab_index,
                should_change_focus_to_new_tab,
                (client_id, is_web_client),
                direct_completion_tx,
                blocking_terminal,
            ))
        {
            let terminal_ids = new_pane_pids
                .iter()
                .chain(new_floating_panes_pids.iter())
                .map(|pane| pane.0)
                .collect();
            self.rollback_unapplied_layout_terminals(terminal_ids);
            return Err(error).with_context(err_context);
        }
        let mut terminals_to_start = vec![];

        terminals_to_start.append(&mut new_pane_pids);
        terminals_to_start.append(&mut new_floating_panes_pids);
        for (terminal_id, starts_held, run_command, reader_result, run_generation) in
            terminals_to_start
        {
            if starts_held {
                // we do not run a command or start listening for bytes on held panes
                continue;
            }
            match reader_result {
                Ok(reader) => {
                    let Some(run_generation) =
                        self.child_run_generations.get(&terminal_id).copied()
                    else {
                        log::error!(
                            "not reading terminal {terminal_id}: its run generation was not recorded"
                        );
                        continue;
                    };
                    self.spawn_terminal_reader(
                        terminal_id,
                        reader,
                        run_generation,
                        true,
                        "failed to spawn terminals for layout".to_string(),
                    );
                },
                _ => match run_command {
                    Some(run_command) => {
                        if run_command.hold_on_close {
                            send_command_not_found_to_screen(
                                self.bus.senders.clone(),
                                terminal_id,
                                run_command.clone(),
                                Some(run_generation),
                            )
                            .non_fatal();
                        } else {
                            self.close_unstarted_layout_terminal(terminal_id, run_generation);
                        }
                    },
                    None => {
                        self.close_unstarted_layout_terminal(terminal_id, run_generation);
                    },
                },
            }
        }
        for (terminal_id, originating_plugin) in originating_plugins_to_inform {
            self.inform_originating_plugin_of_open(terminal_id, originating_plugin);
        }
        Ok(())
    }
    pub fn spawn_terminals_for_layout_override(
        &mut self,
        cwd: Option<PathBuf>,
        layout: TiledPaneLayout,
        floating_panes_layout: Vec<FloatingPaneLayout>,
        default_shell: Option<TerminalAction>,
        plugin_ids: HashMap<RunPluginOrAlias, Vec<u32>>,
        tab_index: usize,
        tab_name: Option<String>,
        client_id: ClientId,
        swap_tiled_layouts: Option<Vec<SwapTiledLayout>>,
        swap_floating_layouts: Option<Vec<SwapFloatingLayout>>,
        _retain_existing_terminal_panes: bool,
        _retain_existing_plugin_panes: bool,
    ) -> Result<TabOverrideResult> {
        let mut default_shell =
            default_shell.unwrap_or_else(|| self.get_default_terminal(cwd, None));
        self.fill_cwd(&mut default_shell, client_id);

        let extracted_run_instructions = layout.extract_run_instructions();
        let extracted_floating_run_instructions = floating_panes_layout
            .iter()
            .filter(|f| !f.already_running)
            .map(|f| f.run.clone());
        let mut new_pane_pids: Vec<PreparedTerminal> = vec![];
        let mut new_floating_panes_pids: Vec<PreparedTerminal> = vec![];

        let mut originating_plugins_to_inform = vec![];
        let mut spawn_error = None;

        for run_instruction in extracted_run_instructions {
            let originating_plugin = run_instruction.as_ref().and_then(|r| {
                if let Run::Command(run_command) = r {
                    run_command.originating_plugin.clone()
                } else {
                    None
                }
            });
            match self.apply_run_instruction(run_instruction, default_shell.clone()) {
                Ok(Some(new_pane_data)) => {
                    let terminal_id = new_pane_data.0;
                    new_pane_pids.push(new_pane_data);
                    if let Some(originating_plugin) = originating_plugin {
                        originating_plugins_to_inform.push((terminal_id, originating_plugin));
                    }
                },
                Ok(None) => {},
                Err(err) => {
                    spawn_error = Some(err);
                    break;
                },
            }
        }
        if spawn_error.is_none() {
            for run_instruction in extracted_floating_run_instructions {
                let originating_plugin = run_instruction.as_ref().and_then(|r| {
                    if let Run::Command(run_command) = r {
                        run_command.originating_plugin.clone()
                    } else {
                        None
                    }
                });
                match self.apply_run_instruction(run_instruction, default_shell.clone()) {
                    Ok(Some(new_pane_data)) => {
                        let terminal_id = new_pane_data.0;
                        new_floating_panes_pids.push(new_pane_data);
                        if let Some(originating_plugin) = originating_plugin {
                            originating_plugins_to_inform.push((terminal_id, originating_plugin));
                        }
                    },
                    Ok(None) => {},
                    Err(err) => {
                        spawn_error = Some(err);
                        break;
                    },
                }
            }
        }
        if let Some(error) = spawn_error {
            let terminal_ids = new_pane_pids
                .iter()
                .chain(new_floating_panes_pids.iter())
                .map(|pane| pane.0)
                .collect();
            self.rollback_unapplied_layout_terminals(terminal_ids);
            return Err(error);
        }

        // Option<RunCommand> should only be Some if the pane starts held
        let new_tab_pane_ids: Vec<(u32, Option<RunCommand>)> = new_pane_pids
            .iter()
            .map(|(terminal_id, starts_held, run_command, _, _)| {
                if *starts_held {
                    (*terminal_id, run_command.clone())
                } else {
                    (*terminal_id, None)
                }
            })
            .collect();
        let new_tab_floating_pane_ids: Vec<(u32, Option<RunCommand>)> = new_floating_panes_pids
            .iter()
            .map(|(terminal_id, starts_held, run_command, _, _)| {
                if *starts_held {
                    (*terminal_id, run_command.clone())
                } else {
                    (*terminal_id, None)
                }
            })
            .collect();

        let tab_result = TabOverrideResult {
            tab_index,
            tab_name,
            tiled_layout: layout,
            floating_layouts: floating_panes_layout,
            swap_tiled_layouts,
            swap_floating_layouts,
            new_terminal_pids: new_tab_pane_ids,
            new_floating_pane_pids: new_tab_floating_pane_ids,
            plugin_ids,
        };

        let mut terminals_to_start = vec![];

        terminals_to_start.append(&mut new_pane_pids);
        terminals_to_start.append(&mut new_floating_panes_pids);
        for (terminal_id, starts_held, run_command, reader_result, run_generation) in
            terminals_to_start
        {
            if starts_held {
                // we do not run a command or start listening for bytes on held panes
                continue;
            }
            match reader_result {
                Ok(reader) => {
                    let Some(run_generation) =
                        self.child_run_generations.get(&terminal_id).copied()
                    else {
                        log::error!(
                            "not reading terminal {terminal_id}: its run generation was not recorded"
                        );
                        continue;
                    };
                    self.spawn_terminal_reader(
                        terminal_id,
                        reader,
                        run_generation,
                        true,
                        "failed to spawn terminals for layout".to_string(),
                    );
                },
                _ => match run_command {
                    Some(run_command) => {
                        if run_command.hold_on_close {
                            send_command_not_found_to_screen(
                                self.bus.senders.clone(),
                                terminal_id,
                                run_command.clone(),
                                Some(run_generation),
                            )
                            .non_fatal();
                        } else {
                            self.close_unstarted_layout_terminal(terminal_id, run_generation);
                        }
                    },
                    None => {
                        self.close_unstarted_layout_terminal(terminal_id, run_generation);
                    },
                },
            }
        }
        for (terminal_id, originating_plugin) in originating_plugins_to_inform {
            self.inform_originating_plugin_of_open(terminal_id, originating_plugin);
        }
        Ok(tab_result)
    }
    fn inform_originating_plugin_of_open(
        &mut self,
        terminal_id: u32,
        originating_plugin: OriginatingPlugin,
    ) {
        self.originating_plugins
            .insert(terminal_id, originating_plugin.clone());
        let update_event = Event::CommandPaneOpened(terminal_id, originating_plugin.context);
        let _ = self
            .bus
            .senders
            .send_to_plugin(PluginInstruction::Update(vec![(
                Some(originating_plugin.plugin_id),
                Some(originating_plugin.client_id),
                update_event,
            )]));
    }

    fn apply_run_instruction(
        &mut self,
        run_instruction: Option<Run>,
        default_shell: TerminalAction,
    ) -> Result<Option<PreparedTerminal>> {
        // terminal_id,
        // starts_held,
        // command
        // successfully opened
        // run_generation
        let err_context = || format!("failed to apply run instruction");
        let run_generation = self.mint_run_generation();
        let quit_cb = Box::new({
            let senders = self.bus.senders.clone();
            move |pane_id, exit_status, _command| {
                // The child has been reaped, so its PID may be reused by an
                // unrelated process: forget it before anything can signal it.
                let _ = senders.send_to_pty(PtyInstruction::ClearPaneProcessIdentity(
                    pane_id,
                    run_generation,
                ));
                send_child_run_exited(&senders, pane_id, exit_status, run_generation, None);
            }
        });

        let originating_plugin = run_instruction.as_ref().and_then(|r| {
            if let Run::Command(run_command) = r {
                run_command.originating_plugin.clone()
            } else {
                None
            }
        });

        match run_instruction {
            Some(Run::Command(mut command)) => {
                let starts_held = command.hold_on_start;
                let hold_on_close = command.hold_on_close;
                let quit_cb = Box::new({
                    let senders = self.bus.senders.clone();
                    move |pane_id, exit_status, command| {
                        // The child has been reaped, so its PID may be reused by an
                        // unrelated process: forget it before anything can signal it.
                        let _ = senders.send_to_pty(PtyInstruction::ClearPaneProcessIdentity(
                            pane_id,
                            run_generation,
                        ));
                        if let PaneId::Terminal(terminal_pane_id) = pane_id {
                            if let Some(originating_plugin) = originating_plugin.as_ref() {
                                let update_event = Event::CommandPaneExited(
                                    terminal_pane_id,
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

                        send_child_run_exited(
                            &senders,
                            pane_id,
                            exit_status,
                            run_generation,
                            if hold_on_close { Some(command) } else { None },
                        );
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
                                Ok(Box::new(NullAsyncReader) as Box<dyn AsyncReader>), // placeholder, never used for held panes
                                run_generation,
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
                        .spawn_terminal(
                            cmd,
                            quit_cb,
                            self.default_editor.clone(),
                            &pane_env_with_origin(&self.pane_env, None),
                        )
                        .with_context(err_context)
                    {
                        Ok((terminal_id, reader, child_pid)) => {
                            self.remember_run(terminal_id, child_pid, run_generation);
                            if let Some(child_pid) = child_pid {
                                self.capture_initial_cwd(terminal_id, child_pid);
                            }
                            Ok(Some((
                                terminal_id,
                                starts_held,
                                Some(command.clone()),
                                Ok(reader),
                                run_generation,
                            )))
                        },
                        Err(err) => match err.downcast_ref::<ZellijError>() {
                            Some(ZellijError::CommandNotFound { terminal_id, .. }) => Ok(Some((
                                *terminal_id,
                                starts_held,
                                Some(command.clone()),
                                Err(err),
                                run_generation,
                            ))),
                            _ => Err(err),
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
                    .spawn_terminal(
                        shell,
                        quit_cb,
                        self.default_editor.clone(),
                        &pane_env_with_origin(&self.pane_env, None),
                    )
                    .with_context(err_context)
                {
                    Ok((terminal_id, reader, child_pid)) => {
                        self.remember_run(terminal_id, child_pid, run_generation);
                        if let Some(child_pid) = child_pid {
                            self.capture_initial_cwd(terminal_id, child_pid);
                        }
                        Ok(Some((
                            terminal_id,
                            starts_held,
                            None,
                            Ok(reader),
                            run_generation,
                        )))
                    },
                    Err(err) => match err.downcast_ref::<ZellijError>() {
                        Some(ZellijError::CommandNotFound { terminal_id, .. }) => Ok(Some((
                            *terminal_id,
                            starts_held,
                            None,
                            Err(err),
                            run_generation,
                        ))),
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
                        &pane_env_with_origin(&self.pane_env, None),
                    )
                    .with_context(err_context)
                {
                    Ok((terminal_id, reader, child_pid)) => {
                        self.remember_run(terminal_id, child_pid, run_generation);
                        if let Some(child_pid) = child_pid {
                            self.capture_initial_cwd(terminal_id, child_pid);
                        }
                        Ok(Some((
                            terminal_id,
                            starts_held,
                            None,
                            Ok(reader),
                            run_generation,
                        )))
                    },
                    Err(err) => match err.downcast_ref::<ZellijError>() {
                        Some(ZellijError::CommandNotFound { terminal_id, .. }) => Ok(Some((
                            *terminal_id,
                            starts_held,
                            None,
                            Err(err),
                            run_generation,
                        ))),
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
                    .spawn_terminal(
                        default_shell.clone(),
                        quit_cb,
                        self.default_editor.clone(),
                        &pane_env_with_origin(&self.pane_env, None),
                    )
                    .with_context(err_context)
                {
                    Ok((terminal_id, reader, child_pid)) => {
                        self.remember_run(terminal_id, child_pid, run_generation);
                        if let Some(child_pid) = child_pid {
                            self.capture_initial_cwd(terminal_id, child_pid);
                        }
                        Ok(Some((
                            terminal_id,
                            starts_held,
                            None,
                            Ok(reader),
                            run_generation,
                        )))
                    },
                    Err(err) => match err.downcast_ref::<ZellijError>() {
                        Some(ZellijError::CommandNotFound { terminal_id, .. }) => Ok(Some((
                            *terminal_id,
                            starts_held,
                            None,
                            Err(err),
                            run_generation,
                        ))),
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
                let _ = self
                    .bus
                    .senders
                    .send_to_pty_writer(PtyWriteInstruction::ForgetRun(id));
                // Sent before the reader is stopped and the child killed, so
                // the screen hears of the close before any output or exit
                // event that follows it. Only terminals that were actually
                // started are announced: a close for an id that was never
                // spawned must not suppress events of a later pane.
                if let Some(handle) = self.task_handles.remove(&id) {
                    let _ = self
                        .bus
                        .senders
                        .send_to_screen(ScreenInstruction::TerminalClosed(id));
                    handle.abort();
                }
                if let Some(child_pid) = self.forget_child_pid(id) {
                    let err_context = || format!("failed to kill child processes for pane {id}");
                    self.bus
                        .os_input
                        .as_mut()
                        .with_context(err_context)
                        .fatal()
                        .kill(child_pid)
                        .with_context(err_context)
                        .non_fatal();
                }
                self.pane_activity_flags.remove(&id);
                self.terminal_cwds.remove(&id);
                self.terminal_cmds.remove(&id);
                self.terminal_foreground_cmds.remove(&id);
                self.terminals_with_shell_cwd.remove(&id);
                self.terminals_with_prompt_markers.remove(&id);
                self.originating_plugins.remove(&id);
                self.resurrect_commands.remove(&id);
                self.origin_pane_ids.remove(&id);
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
        let _ = self
            .bus
            .senders
            .send_to_background_jobs(BackgroundJob::TrimAllocator);
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
            let previous = self.active_panes.insert(client_id, pane_id);
            if previous != Some(pane_id) && matches!(pane_id, PaneId::Terminal(_)) {
                self.session_context_pane = Some(pane_id);
                self.report_session_context(ContextTrigger::FocusChanged);
            }
        }
    }
    fn report_session_context(&self, trigger: ContextTrigger) {
        let Some(PaneId::Terminal(terminal_id)) = self.session_context_pane else {
            return;
        };
        let folder = self.terminal_cwds.get(&terminal_id).cloned().or_else(|| {
            self.id_to_child_pid.get(&terminal_id).and_then(|pid| {
                self.bus
                    .os_input
                    .as_ref()
                    .and_then(|os_input| os_input.get_cwd(*pid))
            })
        });
        let _ = self
            .bus
            .senders
            .send_to_background_jobs(BackgroundJob::SessionSuggestions(
                SessionSuggestionsJob::FocusedPaneContext { folder, trigger },
            ));
    }
    pub fn rerun_command_in_pane(
        &mut self,
        pane_id: PaneId,
        mut run_command: RunCommand,
        run_generation: u64,
    ) -> Result<()> {
        let err_context = || format!("failed to rerun command in pane {:?}", pane_id);

        match pane_id {
            PaneId::Terminal(id) => {
                if let Some(originating_plugins) = self.originating_plugins.get(&id) {
                    run_command.originating_plugin = Some(originating_plugins.clone());
                }

                let hold_on_close = run_command.hold_on_close;
                let command_text = run_command.to_string();
                let originating_plugin = Arc::new(run_command.originating_plugin.clone());
                let quit_cb = Box::new({
                    let senders = self.bus.senders.clone();
                    move |pane_id, exit_status, command| {
                        // The child has been reaped, so its PID may be reused by an
                        // unrelated process: forget it before anything can signal it.
                        let _ = senders.send_to_pty(PtyInstruction::ClearPaneProcessIdentity(
                            pane_id,
                            run_generation,
                        ));
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
                        send_child_run_exited(
                            &senders,
                            pane_id,
                            exit_status,
                            run_generation,
                            if hold_on_close { Some(command) } else { None },
                        );
                    }
                });
                let pane_env =
                    pane_env_with_origin(&self.pane_env, self.origin_pane_ids.get(&id).copied());
                let (reader, child_pid): (Box<dyn AsyncReader>, Option<u32>) = self
                    .bus
                    .os_input
                    .as_mut()
                    .context("no OS I/O interface found")
                    .and_then(|os_input| {
                        os_input.re_run_command_in_terminal(id, run_command, quit_cb, &pane_env)
                    })
                    .with_context(err_context)?;
                // Only replace the current run after the new process exists.
                // A failed spawn leaves the old generation and reader intact.
                self.announce_terminal_run(id, run_generation);
                if let Some(task) = self.task_handles.remove(&id) {
                    task.abort();
                }
                let _ = self.forget_child_pid(id); // if all is well, this shouldn't be here
                self.remember_run(id, child_pid, run_generation);
                if let Some(child_pid) = child_pid {
                    self.capture_initial_cwd(id, child_pid);
                }
                self.spawn_terminal_reader(
                    id,
                    reader,
                    run_generation,
                    false,
                    format!("failed to run async task for pane {pane_id:?}"),
                );
                let _ = self.bus.senders.send_to_screen(
                    ScreenInstruction::AttentionCommandPaneStarted(
                        PaneId::Terminal(id),
                        Some(command_text),
                    ),
                );
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
    pub fn set_resurrect_command(
        &mut self,
        terminal_id: u32,
        resurrect: Option<ResurrectCommand>,
    ) -> std::result::Result<(), String> {
        // A held pane's exited process is forgotten, but its terminal (and
        // reader task) lives until the pane is closed.
        if !self.id_to_child_pid.contains_key(&terminal_id)
            && !self.task_handles.contains_key(&terminal_id)
        {
            return Err(format!("Terminal pane terminal_{} not found", terminal_id));
        }
        match resurrect {
            Some(resurrect) => {
                self.resurrect_commands.insert(terminal_id, resurrect);
            },
            None => {
                self.resurrect_commands.remove(&terminal_id);
            },
        }
        Ok(())
    }
    pub fn populate_session_layout_metadata(
        &mut self,
        session_layout_metadata: &mut SessionLayoutMetadata,
    ) {
        let terminal_ids = session_layout_metadata.all_terminal_ids();
        let mut terminal_ids_to_commands: HashMap<u32, Vec<String>> = HashMap::new();
        let mut terminal_ids_to_cwds: HashMap<u32, PathBuf> = HashMap::new();

        let pids: Vec<_> = terminal_ids
            .iter()
            .filter_map(|id| self.id_to_child_pid.get(&id))
            .copied()
            .collect();
        let (pids_to_cwds, pids_to_cmds) = self
            .bus
            .os_input
            .as_ref()
            .map(|os_input| os_input.get_cwds(pids))
            .unwrap_or_default();
        let panes: Vec<(u32, u32)> = terminal_ids
            .iter()
            .filter_map(|id| self.id_to_child_pid.get(id).map(|pid| (*id, *pid)))
            .collect();
        let foreground_cmds = self
            .bus
            .os_input
            .as_ref()
            .map(|os_input| os_input.get_foreground_cmds(&panes, &self.post_command_discovery_hook))
            .unwrap_or_default();

        for terminal_id in terminal_ids {
            let process_id = self.id_to_child_pid.get(&terminal_id);
            let cwd = process_id.and_then(|pid| pids_to_cwds.get(pid));
            let cmd_sysinfo = process_id.and_then(|pid| pids_to_cmds.get(pid));
            let cmd_foreground = foreground_cmds.get(&terminal_id);
            if let Some(cmd) = cmd_foreground {
                terminal_ids_to_commands.insert(terminal_id, cmd.clone());
            } else if let Some(cmd) = cmd_sysinfo {
                terminal_ids_to_commands.insert(terminal_id, cmd.clone());
            }
            if let Some(cwd) = cwd {
                terminal_ids_to_cwds.insert(terminal_id, cwd.clone());
            }
            if let Some(resurrect) = self.resurrect_commands.get(&terminal_id) {
                let mut command_line = vec![resurrect.command.to_string_lossy().into_owned()];
                command_line.extend(resurrect.args.iter().cloned());
                terminal_ids_to_commands.insert(terminal_id, command_line);
                if let Some(cwd) = &resurrect.cwd {
                    terminal_ids_to_cwds.insert(terminal_id, cwd.clone());
                }
            }
        }
        session_layout_metadata.update_default_shell(default_shell_from(&self.pane_env));
        session_layout_metadata.update_terminal_commands(terminal_ids_to_commands);
        session_layout_metadata.update_terminal_cwds(terminal_ids_to_cwds);
        session_layout_metadata.update_default_editor(&self.default_editor);
        session_layout_metadata.detect_editor_panes();
    }
    pub fn fill_plugin_cwd(
        &self,
        should_float: Option<bool>,
        should_open_in_place: bool, // should be opened in place
        close_replaced_pane: bool,  // close_replaced_pane
        pane_title: Option<String>, // pane title
        mut run: RunPluginOrAlias,
        tab_index: usize,                   // tab index
        pane_id_to_replace: Option<PaneId>, // pane id to replace if this is to be opened "in-place"
        client_id: ClientId,
        size: Size,
        skip_cache: bool,
        cwd: Option<PathBuf>,
        should_focus_plugin: Option<bool>,
        floating_pane_coordinates: Option<FloatingPaneCoordinates>,
        completion_tx: Option<NotificationEnd>,
    ) -> Result<()> {
        let get_focused_cwd = || {
            self.active_panes
                .get(&client_id)
                .and_then(|pane| match pane {
                    PaneId::Plugin(plugin_id) => self.plugin_cwds.get(plugin_id).cloned(),
                    PaneId::Terminal(id) => {
                        // Try to get CWD from OS, fall back to cached value
                        self.id_to_child_pid
                            .get(id)
                            .and_then(|&pid| {
                                self.bus
                                    .os_input
                                    .as_ref()
                                    .and_then(|input| input.get_cwd(pid))
                            })
                            .or_else(|| self.terminal_cwds.get(id).cloned())
                    },
                })
        };

        let cwd = cwd.or_else(get_focused_cwd);
        let focused_plugin_id = self
            .active_panes
            .get(&client_id)
            .and_then(|pane| match pane {
                PaneId::Plugin(plugin_id) => Some(*plugin_id),
                _ => None,
            });

        if let RunPluginOrAlias::Alias(alias) = &mut run {
            let cwd = get_focused_cwd();
            alias.set_caller_cwd_if_not_set(cwd);
        }
        self.bus.senders.send_to_plugin(PluginInstruction::Load(
            should_float,
            should_open_in_place,
            close_replaced_pane,
            pane_title,
            run,
            Some(tab_index),
            pane_id_to_replace,
            client_id,
            size,
            cwd,
            focused_plugin_id,
            skip_cache,
            should_focus_plugin,
            floating_pane_coordinates,
            completion_tx,
        ))?;
        Ok(())
    }
    fn capture_initial_cwd(&mut self, terminal_id: u32, child_pid: u32) {
        if let Some(os_input) = self.bus.os_input.as_ref() {
            if let Some(cwd) = os_input.get_cwd(child_pid) {
                self.terminal_cwds.insert(terminal_id, cwd);
            }
        }
    }

    pub fn update_and_report_cwds(&mut self) {
        use std::sync::atomic::Ordering;

        let active_terminal_ids: Vec<u32> = self
            .id_to_child_pid
            .keys()
            .copied()
            .filter(|id| {
                self.pane_activity_flags
                    .get(id)
                    .map(|f| f.swap(false, Ordering::Relaxed))
                    .unwrap_or(false)
            })
            .collect();

        if active_terminal_ids.is_empty() {
            return;
        }

        let pids: Vec<_> = active_terminal_ids
            .iter()
            .filter_map(|id| self.id_to_child_pid.get(id))
            .copied()
            .collect();

        let (pids_to_cwds, pids_to_cmds) = self
            .bus
            .os_input
            .as_ref()
            .map(|os_input| os_input.get_cwds(pids))
            .unwrap_or_default();

        for terminal_id in &active_terminal_ids {
            let process_id = self.id_to_child_pid.get(terminal_id);
            let cwd = process_id.and_then(|pid| pids_to_cwds.get(pid));

            let cwd = cwd.filter(|_| !self.terminals_with_shell_cwd.contains(terminal_id));
            if let Some(cwd) = cwd {
                if self.terminal_cwds.get(terminal_id) != Some(cwd) {
                    let pane_id = PaneId::Terminal(*terminal_id);
                    let focused_client_ids: Vec<ClientId> = self
                        .active_panes
                        .iter()
                        .filter(|(_, active_pane)| *active_pane == &pane_id)
                        .map(|(client_id, _)| *client_id)
                        .collect();
                    let _ = self
                        .bus
                        .senders
                        .send_to_plugin(PluginInstruction::Update(vec![(
                            None,
                            None,
                            Event::CwdChanged(pane_id.into(), cwd.clone(), focused_client_ids),
                        )]));
                    self.terminal_cwds.insert(*terminal_id, cwd.clone());
                    if self.session_context_pane == Some(pane_id) {
                        self.report_session_context(ContextTrigger::FolderChanged);
                    }
                }
                self.terminal_cwds.insert(*terminal_id, cwd.clone());
            }

            let cmd = process_id.and_then(|pid| pids_to_cmds.get(pid));
            if let Some(cmd) = cmd {
                self.terminal_cmds.insert(*terminal_id, cmd.clone());
            }
        }

        let panes: Vec<(u32, u32)> = active_terminal_ids
            .iter()
            .filter_map(|id| self.id_to_child_pid.get(id).map(|pid| (*id, *pid)))
            .collect();
        let foreground_cmds = self
            .bus
            .os_input
            .as_ref()
            .map(|os_input| os_input.get_foreground_cmds(&panes, &self.post_command_discovery_hook))
            .unwrap_or_default();

        for terminal_id in &active_terminal_ids {
            let foreground_cmd: Vec<String> = foreground_cmds
                .get(terminal_id)
                .cloned()
                .unwrap_or_default();

            if self.terminal_foreground_cmds.get(terminal_id) != Some(&foreground_cmd) {
                let pane_id = PaneId::Terminal(*terminal_id);
                let focused_client_ids: Vec<ClientId> = self
                    .active_panes
                    .iter()
                    .filter(|(_, active_pane)| *active_pane == &pane_id)
                    .map(|(client_id, _)| *client_id)
                    .collect();
                if foreground_cmd.is_empty()
                    && self.session_context_pane == Some(pane_id)
                    && !self.terminals_with_prompt_markers.contains(terminal_id)
                {
                    self.report_session_context(ContextTrigger::PromptReturned);
                }
                let (command, is_foreground) = if foreground_cmd.is_empty() {
                    let shell_cmd = self
                        .terminal_cmds
                        .get(terminal_id)
                        .cloned()
                        .unwrap_or_default();
                    (shell_cmd, false)
                } else {
                    (foreground_cmd.clone(), true)
                };
                let _ =
                    self.bus
                        .senders
                        .send_to_screen(ScreenInstruction::AttentionForegroundChanged(
                            pane_id,
                            if foreground_cmd.is_empty() {
                                None
                            } else {
                                Some(foreground_cmd.join(" "))
                            },
                        ));
                let _ = self
                    .bus
                    .senders
                    .send_to_plugin(PluginInstruction::Update(vec![(
                        None,
                        None,
                        Event::CommandChanged(
                            pane_id.into(),
                            command,
                            is_foreground,
                            focused_client_ids,
                        ),
                    )]));
                self.terminal_foreground_cmds
                    .insert(*terminal_id, foreground_cmd);
            }
        }
    }

    pub fn reconfigure(
        &mut self,
        default_editor: Option<PathBuf>,
        post_command_discovery_hook: Option<String>,
        pane_env: PaneEnv,
    ) {
        self.default_editor = default_editor;
        self.post_command_discovery_hook = post_command_discovery_hook;
        self.pane_env = pane_env;
    }

    pub fn notify_cwd_from_osc7(&mut self, terminal_id: u32, path: PathBuf) {
        use std::sync::atomic::Ordering;

        self.terminals_with_shell_cwd.insert(terminal_id);

        if self.terminal_cwds.get(&terminal_id) != Some(&path) {
            let pane_id = PaneId::Terminal(terminal_id);
            let focused_client_ids: Vec<ClientId> = self
                .active_panes
                .iter()
                .filter(|(_, active_pane)| *active_pane == &pane_id)
                .map(|(client_id, _)| *client_id)
                .collect();
            let _ = self
                .bus
                .senders
                .send_to_plugin(PluginInstruction::Update(vec![(
                    None,
                    None,
                    Event::CwdChanged(pane_id.into(), path.clone(), focused_client_ids),
                )]));
            self.terminal_cwds.insert(terminal_id, path);
            if self.session_context_pane == Some(pane_id) {
                self.report_session_context(ContextTrigger::FolderChanged);
            }
        }
        if let Some(flag) = self.pane_activity_flags.get(&terminal_id) {
            flag.store(false, Ordering::Relaxed);
        }
    }

    pub fn send_sigint_to_pane(&self, pane_id: PaneId) {
        let err_context = || format!("failed to send SIGINT to pane {:?}", pane_id);

        match pane_id {
            PaneId::Terminal(terminal_id) => {
                if let Some(&child_pid) = self.id_to_child_pid.get(&terminal_id) {
                    self.bus
                        .os_input
                        .as_ref()
                        .context("no OS I/O interface found")
                        .and_then(|os_input| {
                            os_input.send_sigint_to_terminal(terminal_id, child_pid)
                        })
                        .with_context(err_context)
                        .non_fatal();
                } else {
                    log::warn!("Terminal pane {} not found or not running", terminal_id);
                }
            },
            PaneId::Plugin(plugin_id) => {
                log::warn!("Cannot send SIGINT to plugin pane {}", plugin_id);
            },
        }
    }

    pub fn send_sigkill_to_pane(&self, pane_id: PaneId) {
        let err_context = || format!("failed to send SIGKILL to pane {:?}", pane_id);

        match pane_id {
            PaneId::Terminal(terminal_id) => {
                if let Some(&child_pid) = self.id_to_child_pid.get(&terminal_id) {
                    self.bus
                        .os_input
                        .as_ref()
                        .context("no OS I/O interface found")
                        .and_then(|os_input| os_input.force_kill_terminal(terminal_id, child_pid))
                        .with_context(err_context)
                        .non_fatal();
                } else {
                    log::warn!("Terminal pane {} not found or not running", terminal_id);
                }
            },
            PaneId::Plugin(plugin_id) => {
                log::warn!("Cannot send SIGKILL to plugin pane {}", plugin_id);
            },
        }
    }

    pub fn get_pane_pid(&self, pane_id: PaneId) -> GetPanePidResponse {
        match pane_id {
            PaneId::Terminal(terminal_id) => {
                if let Some(&child_pid) = self.id_to_child_pid.get(&terminal_id) {
                    GetPanePidResponse::Ok(child_pid as i32)
                } else {
                    GetPanePidResponse::Err(format!(
                        "Terminal pane {} not found or not running",
                        terminal_id
                    ))
                }
            },
            PaneId::Plugin(plugin_id) => {
                GetPanePidResponse::Err(format!("Cannot get PID for plugin pane {}", plugin_id))
            },
        }
    }
    pub fn get_pane_running_command(&self, pane_id: PaneId) -> GetPaneRunningCommandResponse {
        match pane_id {
            PaneId::Terminal(terminal_id) => {
                if let Some(&child_pid) = self.id_to_child_pid.get(&terminal_id) {
                    // Query OS for current running command
                    if let Some(os_input) = self.bus.os_input.as_ref() {
                        // First, try to get the foreground command (e.g., nvim running in bash)
                        let foreground_cmds = os_input.get_foreground_cmds(
                            &[(terminal_id, child_pid)],
                            &self.post_command_discovery_hook,
                        );
                        let cmd_foreground = foreground_cmds.get(&terminal_id);

                        // If no foreground command, fall back to the shell itself
                        let (_cwds, cmds) = os_input.get_cwds(vec![child_pid]);
                        let cmd_sysinfo = cmds.get(&child_pid);

                        if let Some(command_args) = cmd_foreground {
                            GetPaneRunningCommandResponse::Ok(command_args.clone())
                        } else if let Some(command_args) = cmd_sysinfo {
                            GetPaneRunningCommandResponse::Ok(command_args.clone())
                        } else {
                            GetPaneRunningCommandResponse::Err(format!(
                                "Could not retrieve running command for terminal pane {}",
                                terminal_id
                            ))
                        }
                    } else {
                        GetPaneRunningCommandResponse::Err("OS input not available".to_string())
                    }
                } else {
                    GetPaneRunningCommandResponse::Err(format!(
                        "Terminal pane {} not found or not running",
                        terminal_id
                    ))
                }
            },
            PaneId::Plugin(plugin_id) => GetPaneRunningCommandResponse::Err(format!(
                "Cannot get running command for plugin pane {}",
                plugin_id
            )),
        }
    }
    pub fn get_pane_cwd(&self, pane_id: PaneId) -> GetPaneCwdResponse {
        match pane_id {
            PaneId::Terminal(terminal_id) => {
                if let Some(&child_pid) = self.id_to_child_pid.get(&terminal_id) {
                    // Query OS for current working directory
                    if let Some(os_input) = self.bus.os_input.as_ref() {
                        let (cwds, _cmds) = os_input.get_cwds(vec![child_pid]);
                        if let Some(cwd) = cwds.get(&child_pid) {
                            GetPaneCwdResponse::Ok(cwd.clone())
                        } else {
                            GetPaneCwdResponse::Err(format!(
                                "Could not retrieve CWD for terminal pane {}",
                                terminal_id
                            ))
                        }
                    } else {
                        GetPaneCwdResponse::Err("OS input not available".to_string())
                    }
                } else {
                    GetPaneCwdResponse::Err(format!(
                        "Terminal pane {} not found or not running",
                        terminal_id
                    ))
                }
            },
            PaneId::Plugin(plugin_id) => {
                GetPaneCwdResponse::Err(format!("Cannot get CWD for plugin pane {}", plugin_id))
            },
        }
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

fn send_child_run_exited(
    senders: &ThreadSenders,
    pane_id: PaneId,
    exit_status: Option<i32>,
    run_generation: u64,
    hold_command: Option<RunCommand>,
) {
    let _ = senders.send_to_screen(ScreenInstruction::ChildRunExited {
        pane_id,
        exit_status,
        run_generation,
        hold_command,
    });
}

fn send_command_not_found_to_screen(
    senders: ThreadSenders,
    terminal_id: u32,
    run_command: RunCommand,
    run_generation: Option<u64>,
) -> Result<()> {
    let err_context = || format!("failed to send command_not_fount for terminal {terminal_id}");
    let message = format!("Command not found: {}\n\rIf you were including arguments as part of the command, try including them as 'args' instead.", run_command.command.display())
        .into_bytes();
    match run_generation {
        Some(run_generation) => senders.send_to_screen(ScreenInstruction::PtyRunBytes(
            terminal_id,
            run_generation,
            message,
        )),
        None => senders.send_to_screen(ScreenInstruction::PtyBytes(terminal_id, message)),
    }
    .with_context(err_context)?;
    senders
        .send_to_screen(ScreenInstruction::HoldPane(
            PaneId::Terminal(terminal_id),
            Some(2),
            run_command,
            run_generation,
        ))
        .with_context(err_context)?;
    Ok(())
}

pub fn get_default_shell() -> PathBuf {
    default_shell_from(&PaneEnv::new())
}

#[cfg(not(windows))]
pub fn default_shell_from(pane_env: &PaneEnv) -> PathBuf {
    PathBuf::from(env_value(pane_env, "SHELL").unwrap_or_else(|| {
        log::warn!("Cannot read SHELL env, falling back to use /bin/sh");
        "/bin/sh".to_string()
    }))
}

#[cfg(windows)]
pub fn default_shell_from(pane_env: &PaneEnv) -> PathBuf {
    if let Some(shell) = env_value(pane_env, "SHELL") {
        return PathBuf::from(shell);
    }
    PathBuf::from(env_value(pane_env, "COMSPEC").unwrap_or_else(|| {
        log::warn!("Cannot read SHELL or COMSPEC env, falling back to use cmd.exe");
        "cmd.exe".to_string()
    }))
}

#[cfg(test)]
#[path = "./unit/pty_tests.rs"]
mod pty_tests;
