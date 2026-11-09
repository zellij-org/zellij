use crate::os_input_output::ClientOsApi;
use crate::spawn_server;

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
    is_welcome_session: bool,
    config: &Config,
) -> Result<Option<ConnectToSession>, &'static str> {
    let default_layout_from_config =
        LayoutInfo::from_config(&config.options.layout_dir, &config.options.default_layout);
    if is_welcome_session {
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
) -> std::io::Result<()> {
    let debug = false;
    envs::set_session_name(session_name.to_owned());
    os_input.update_session_name(session_name.to_owned());
    spawn_server(&*zellij_ipc_pipe, debug)
}

/// Outcome of an exclusive create-or-attach attempt for a session socket.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionEnsureAction {
    /// We hold (or held) the creation claim and should spawn a new server, then send the create message.
    Create,
    /// Another owner holds the socket claim, or a recheck found the session; attach only.
    Attach,
}

/// Take the shared startup claim, recheck liveness, and decide whether this caller may create.
///
/// The claim is released before returning so the server process can acquire the same lock on bind.
/// A lost claim (`AddrInUse`) means another owner already exists — never spawn a replacement.
pub fn decide_session_create_or_attach(
    session_name: &str,
    zellij_ipc_pipe: &PathBuf,
) -> std::io::Result<SessionEnsureAction> {
    use zellij_utils::session_socket_claim::SessionSocketClaim;
    use zellij_utils::sessions::session_exists;

    match SessionSocketClaim::try_acquire(zellij_ipc_pipe) {
        Ok(claim) => {
            let action = match session_exists(session_name) {
                Ok(true) => SessionEnsureAction::Attach,
                Ok(false) => SessionEnsureAction::Create,
                Err(kind) => {
                    claim.release();
                    return Err(std::io::Error::new(
                        kind,
                        format!(
                            "could not recheck session {session_name:?} while holding the creation claim"
                        ),
                    ));
                },
            };
            claim.release();
            Ok(action)
        },
        Err(err) if err.kind() == std::io::ErrorKind::AddrInUse => Ok(SessionEnsureAction::Attach),
        Err(err) => Err(err),
    }
}

pub fn create_first_message(
    is_read_only: bool,
    config_file_path: Option<PathBuf>,
    client_attributes: ClientAttributes,
    mut config_opts: Options,
    should_create_session: bool,
    session_name: &str,
    initial_layout: Option<LayoutInfo>,
    switch_context: Option<zellij_utils::session_history::SwitchContext>,
    focus: (Option<usize>, Option<(u32, bool)>),
) -> ClientToServerMsg {
    let resurrection_layout = resurrection_layout(&session_name).ok().flatten();

    let layout_info = if resurrection_layout.is_some() {
        let id = zellij_utils::sessions::resolve_session_id(session_name)
            .unwrap_or_else(|| session_name.to_string());
        Some(LayoutInfo::File(
            session_layout_cache_file_name(&id).display().to_string(),
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
            host_terminal_env: Default::default(),
            initial_panes: None,
            switch_context,
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
            host_terminal_env: Default::default(),
            initial_panes: None,
            switch_context,
        };
        let is_web_client = true;

        ClientToServerMsg::AttachClient {
            cli_assets,
            tab_position_to_focus: focus.0,
            pane_to_focus: focus
                .1
                .map(|(pane_id, is_plugin)| zellij_utils::ipc::PaneReference {
                    pane_id,
                    is_plugin,
                }),
            is_web_client,
        }
    }
}

pub fn create_ipc_pipe(session_name: &str, session_exists: bool) -> std::io::Result<PathBuf> {
    if let Err(e) = zellij_utils::sessions::validate_session_name(session_name) {
        log::error!("Invalid session name: {}", e);
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("Invalid session name: {}", e),
        ));
    }
    let sock_dir = zellij_utils::consts::ZELLIJ_SOCK_DIR.clone();
    fs::create_dir_all(&sock_dir)?;
    zellij_utils::shared::set_permissions(&sock_dir, 0o700)?;
    let zellij_ipc_pipe: PathBuf = if session_exists {
        // Existing session: resolve its id via the session index.
        zellij_utils::sessions::resolve_session_socket_path(session_name)
            .unwrap_or_else(|| sock_dir.join(session_name))
    } else {
        // New session: register it under a fresh id.
        sock_dir.join(zellij_utils::sessions::register_session(session_name))
    };
    crate::check_ipc_pipe_length(&zellij_ipc_pipe);
    Ok(zellij_ipc_pipe)
}
