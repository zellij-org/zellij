use crate::os_input_output::ClientOsApi;
use crate::spawn_server;
use crate::web_client::types::SessionCreationPolicy;

use std::{fs, path::PathBuf};
use zellij_utils::{
    cli::CliArgs,
    data::{ConnectToSession, LayoutInfo, WebSharing},
    envs,
    input::{
        config::{Config, ConfigError},
        layout::Layout,
        options::Options,
    },
    ipc::{ClientAttributes, ClientToServerMsg},
    sessions::{generate_unique_session_name, resurrection_layout, session_exists},
    setup::{find_default_config_dir, get_layout_dir},
};

pub fn build_initial_connection(
    session_name: Option<String>,
    config: &Config,
) -> Result<(Option<ConnectToSession>, bool), &'static str> {
    // bool -> is_welcome_screen
    let should_start_with_welcome_screen = session_name.is_none();
    let default_layout_from_config =
        LayoutInfo::from_config(&config.options.layout_dir, &config.options.default_layout);
    if should_start_with_welcome_screen {
        let Some(initial_session_name) = session_name.clone().or_else(generate_unique_session_name)
        else {
            return Err("Failed to generate unique session name, bailing.");
        };
        Ok((
            Some(ConnectToSession {
                name: Some(initial_session_name.clone()),
                layout: Some(LayoutInfo::BuiltIn("welcome".to_owned())),
                ..Default::default()
            }),
            should_start_with_welcome_screen,
        ))
    } else if let Some(session_name) = session_name {
        Ok((
            Some(ConnectToSession {
                name: Some(session_name.clone()),
                layout: default_layout_from_config,
                ..Default::default()
            }),
            should_start_with_welcome_screen,
        ))
    } else if default_layout_from_config.is_some() {
        Ok((
            Some(ConnectToSession {
                layout: default_layout_from_config,
                ..Default::default()
            }),
            should_start_with_welcome_screen,
        ))
    } else {
        Ok((None, should_start_with_welcome_screen))
    }
}

fn layout_for_new_session(
    config: &Config,
    requested_layout: Option<LayoutInfo>,
) -> Result<(Layout, Config), ConfigError> {
    let layout_dir = config
        .options
        .layout_dir
        .clone()
        .or_else(|| get_layout_dir(find_default_config_dir()));
    match requested_layout {
        Some(LayoutInfo::BuiltIn(layout_name)) => Layout::from_default_assets(
            &PathBuf::from(layout_name),
            layout_dir.clone(),
            config.clone(),
        ),
        Some(LayoutInfo::File(layout_name)) => Layout::from_path_or_default(
            Some(&PathBuf::from(layout_name)),
            layout_dir.clone(),
            config.clone(),
        ),
        Some(LayoutInfo::Url(url)) => Layout::from_url(&url, config.clone()),
        Some(LayoutInfo::Stringified(stringified_layout)) => {
            Layout::from_stringified_layout(&stringified_layout, config.clone())
        },
        None => Layout::from_default_assets(
            &PathBuf::from("default"),
            layout_dir.clone(),
            config.clone(),
        ),
    }
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

pub fn spawn_session_if_needed(
    session_name: &str,
    path: String,
    client_attributes: ClientAttributes,
    config: &Config,
    config_options: &Options,
    is_web_client: bool,
    os_input: Box<dyn ClientOsApi>,
    requested_layout: Option<LayoutInfo>,
    is_welcome_screen: bool,
    create_policy: SessionCreationPolicy,
) -> Result<(ClientToServerMsg, PathBuf), String> {
    // LOCAL PATCH (web-daemon panic fix, 2026-07-15): this returns Result so a
    // layout that fails to resolve fails THIS connection instead of panicking
    // the shared web daemon. The old code swallowed the ConfigError with
    // `.ok()` and then `layout.unwrap()`ed a None inside spawn_new_session —
    // dmesg-correlated with every wholesale session death Jul 11-15.
    //
    // LOCAL PATCH (web session ownership, llmos#15114): attach-vs-create is a
    // decision (`decide_session_action`), not a side effect of a terminal
    // socket opening for a name.
    let session_is_live = session_exists(session_name).unwrap_or(false);
    match decide_session_action(session_name, session_is_live, create_policy)? {
        SessionAction::Attach => Ok(ipc_pipe_and_first_message_for_existing_session(
            path,
            client_attributes,
            &config,
            &config_options,
            is_web_client,
        )),
        SessionAction::Create => {
            let force_run_commands = false;
            let resurrection_layout = resurrection_layout(session_name)
                .ok()
                .flatten()
                .map(|mut resurrection_layout| {
                    if force_run_commands {
                        resurrection_layout.recursively_add_start_suspended(Some(false));
                    }
                    resurrection_layout
                });

            // Resolve the layout BEFORE spawning the session server so a bad
            // layout cannot leave an orphaned, clientless server behind.
            let layout = match resurrection_layout {
                Some(resurrection_layout) => resurrection_layout,
                None => layout_for_new_session(&config, requested_layout)
                    .map(|(layout, _config)| layout)
                    .map_err(|e| {
                        format!(
                            "Failed to resolve layout for new session {:?}: {}",
                            session_name, e
                        )
                    })?,
            };
            Ok(spawn_new_session(
                session_name,
                os_input.clone(),
                config.clone(),
                config_options.clone(),
                layout,
                client_attributes,
                is_welcome_screen,
            ))
        },
    }
}

fn spawn_new_session(
    name: &str,
    mut os_input: Box<dyn ClientOsApi>,
    mut config: Config,
    config_opts: Options,
    layout: Layout,
    client_attributes: ClientAttributes,
    is_welcome_screen: bool,
) -> (ClientToServerMsg, PathBuf) {
    let debug = false;
    envs::set_session_name(name.to_owned());
    os_input.update_session_name(name.to_owned());

    let zellij_ipc_pipe: PathBuf = {
        let mut sock_dir = zellij_utils::consts::ZELLIJ_SOCK_DIR.clone();
        fs::create_dir_all(&sock_dir).unwrap();
        zellij_utils::shared::set_permissions(&sock_dir, 0o700).unwrap();
        sock_dir.push(name);
        sock_dir
    };

    spawn_server(&*zellij_ipc_pipe, debug).unwrap();

    let successfully_written_config = Config::write_config_to_disk_if_it_does_not_exist(
        config.to_string(true),
        &Default::default(),
    );
    let should_launch_setup_wizard = successfully_written_config;
    let cli_args = CliArgs::default();
    config.options.web_server = Some(true);
    config.options.web_sharing = Some(WebSharing::On);
    let is_web_client = true;

    (
        ClientToServerMsg::NewClient(
            client_attributes,
            Box::new(cli_args),
            Box::new(config.clone()),
            Box::new(config_opts.clone()),
            Box::new(layout),
            Box::new(config.plugins.clone()),
            should_launch_setup_wizard,
            is_web_client,
            is_welcome_screen,
        ),
        zellij_ipc_pipe,
    )
}

fn ipc_pipe_and_first_message_for_existing_session(
    session_name: String,
    client_attributes: ClientAttributes,
    config: &Config,
    config_options: &Options,
    is_web_client: bool,
) -> (ClientToServerMsg, PathBuf) {
    let zellij_ipc_pipe: PathBuf = {
        let mut sock_dir = zellij_utils::consts::ZELLIJ_SOCK_DIR.clone();
        fs::create_dir_all(&sock_dir).unwrap();
        zellij_utils::shared::set_permissions(&sock_dir, 0o700).unwrap();
        sock_dir.push(session_name);
        sock_dir
    };
    let first_message = ClientToServerMsg::AttachClient(
        client_attributes,
        config.clone(),
        config_options.clone(),
        None,
        None,
        is_web_client,
    );
    (first_message, zellij_ipc_pipe)
}
