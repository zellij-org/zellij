use crate::os_input_output::ClientOsApi;
use crate::spawn_server;
use crate::web_client::types::SessionCreationPolicy;

use std::{fs, path::PathBuf};
use zellij_utils::{
    consts::session_layout_cache_file_name,
    data::{ConnectToSession, LayoutInfo, LayoutMetadata, WebSharing},
    envs,
    input::{cli_assets::CliAssets, config::Config, options::Options},
    ipc::{ClientAttributes, ClientToServerMsg},
    sessions::{generate_unique_session_name, resurrection_layout},
};

pub fn build_initial_connection(
    session_name: Option<String>,
    config: &Config,
) -> Result<Option<ConnectToSession>, &'static str> {
    let should_start_with_welcome_screen = session_name.is_none();
    let default_layout_from_config =
        LayoutInfo::from_config(&config.options.layout_dir, &config.options.default_layout);
    if should_start_with_welcome_screen {
        let Some(initial_session_name) = session_name.clone().or_else(generate_unique_session_name)
        else {
            return Err("Failed to generate unique session name, bailing.");
        };
        Ok(Some(ConnectToSession {
            name: Some(initial_session_name.clone()),
            layout: Some(LayoutInfo::BuiltIn("welcome".to_owned())),
            ..Default::default()
        }))
    } else if let Some(session_name) = session_name {
        Ok(Some(ConnectToSession {
            name: Some(session_name.clone()),
            layout: default_layout_from_config,
            ..Default::default()
        }))
    } else if default_layout_from_config.is_some() {
        Ok(Some(ConnectToSession {
            layout: default_layout_from_config,
            ..Default::default()
        }))
    } else {
        Ok(None)
    }
}

pub fn spawn_new_session(
    session_name: &str,
    mut os_input: Box<dyn ClientOsApi>,
    zellij_ipc_pipe: &PathBuf,
) {
    let debug = false;
    envs::set_session_name(session_name.to_owned());
    os_input.update_session_name(session_name.to_owned());
    spawn_server(&*zellij_ipc_pipe, debug).unwrap();
}

pub fn create_first_message(
    is_read_only: bool,
    config_file_path: Option<PathBuf>,
    client_attributes: ClientAttributes,
    mut config_opts: Options,
    should_create_session: bool,
    session_name: &str,
    initial_layout: Option<LayoutInfo>,
) -> ClientToServerMsg {
    let resurrection_layout = resurrection_layout(&session_name).ok().flatten();

    let layout_info = if resurrection_layout.is_some() {
        Some(LayoutInfo::File(
            session_layout_cache_file_name(&session_name)
                .display()
                .to_string(),
            LayoutMetadata::default(),
        ))
    } else {
        initial_layout
    };

    config_opts.web_server = Some(true);
    config_opts.web_sharing = Some(WebSharing::On);

    let is_web_client = true;
    if is_read_only {
        // read only clients attach as watchers
        ClientToServerMsg::AttachWatcherClient {
            terminal_size: client_attributes.size,
            is_web_client,
        }
    } else if should_create_session {
        config_opts.web_server = Some(true);
        config_opts.web_sharing = Some(WebSharing::On);
        let cli_assets = CliAssets {
            config_file_path,
            config_dir: None,
            should_ignore_config: false,
            configuration_options: Some(config_opts),
            layout: layout_info,
            terminal_window_size: client_attributes.size,
            data_dir: None,
            is_debug: false,
            max_panes: None,
            force_run_layout_commands: false,
            cwd: None,
        };

        ClientToServerMsg::FirstClientConnected {
            cli_assets,
            is_web_client,
        }
    } else {
        let cli_assets = CliAssets {
            config_file_path,
            config_dir: None,
            should_ignore_config: false,
            configuration_options: Some(config_opts),
            layout: None,
            terminal_window_size: client_attributes.size,
            data_dir: None,
            is_debug: false,
            max_panes: None,
            force_run_layout_commands: false,
            cwd: None,
        };
        let is_web_client = true;

        ClientToServerMsg::AttachClient {
            cli_assets,
            tab_position_to_focus: None,
            pane_to_focus: None,
            is_web_client,
        }
    }
}

pub fn create_ipc_pipe(session_name: &str) -> PathBuf {
    let zellij_ipc_pipe: PathBuf = {
        let mut sock_dir = zellij_utils::consts::ZELLIJ_SOCK_DIR.clone();
        fs::create_dir_all(&sock_dir).unwrap();
        zellij_utils::shared::set_permissions(&sock_dir, 0o700).unwrap();
        sock_dir.push(session_name);
        sock_dir
    };
    zellij_ipc_pipe
}

/// How a terminal socket request may treat the session it names.
///
/// LOCAL PATCH (web session ownership, llmos#15114). Two requests reach this
/// socket: an UNNAMED one (`/ws/terminal` — the browser asked for a terminal and
/// the server picks the name) and a NAMED one (`/ws/terminal/<name>` — the
/// browser asked for a session it remembers). Only the unnamed request may
/// create: a name the browser merely remembers is not evidence that anything
/// still wants that session, and a Zellient tab left open on a lane re-minted
/// the lane husk the moment the janitor deleted it. A named request that
/// genuinely wants a new session says so (`?create=1`).
pub fn session_creation_policy(
    requested_session_name: Option<&str>,
    explicit_create: bool,
) -> SessionCreationPolicy {
    match requested_session_name {
        None => SessionCreationPolicy::CreateIfMissing,
        Some(_) if explicit_create => SessionCreationPolicy::CreateIfMissing,
        Some(_) => SessionCreationPolicy::AttachOnly,
    }
}

/// A session name the orchestrator owns. Every lane runs in
/// `work-<digits>-<hex>`, is minted by the orchestrator (`zellij attach
/// --create-background`) and is reaped by `zellij_session_janitor` when its
/// queue row ends. A browser is never a lane's owner, so it may watch a live
/// lane but must never bring one back — that resurrection is the husk
/// (llmos#15114). The recogniser is the janitor's own test (`name.startswith
/// ("work-")`), deliberately not a tighter regex: a lane name that ever gains a
/// suffix must still be refused.
pub fn is_orchestrator_owned_session_name(name: &str) -> bool {
    name.starts_with("work-")
}

/// What a web connection should do about the session it names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionAction {
    Attach,
    Create,
}

/// The single decision the web attach path makes about a session name.
///
/// Attaching never creates anything. Creating needs BOTH a create policy AND a
/// name no orchestrator owns; everything else is refused with a message the
/// browser can read.
pub fn decide_session_action(
    session_name: &str,
    session_is_live: bool,
    create_policy: SessionCreationPolicy,
) -> Result<SessionAction, String> {
    if session_is_live {
        return Ok(SessionAction::Attach);
    }
    if create_policy == SessionCreationPolicy::AttachOnly {
        return Err(format!(
            "session {:?} is not running — the web client attaches to live sessions only and does not create them",
            session_name
        ));
    }
    if is_orchestrator_owned_session_name(session_name) {
        return Err(format!(
            "session {:?} is orchestrator-owned (work-*); the web client will not create or resurrect it. Start it with `zellij attach --create-background {}`",
            session_name, session_name
        ));
    }
    Ok(SessionAction::Create)
}
