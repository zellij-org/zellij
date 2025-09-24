use crate::os_input_output::ClientOsApi;
use crate::spawn_server;

use std::{fs, path::PathBuf};
use zellij_utils::{
    consts::session_layout_cache_file_name,
    data::{ConnectToSession, LayoutInfo, WebSharing},
    envs,
    input::{cli_assets::CliAssets, config::Config, options::Options},
    ipc::{ClientAttributes, ClientToServerMsg},
    sessions::{generate_unique_session_name, resurrection_layout, session_exists},
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

pub fn spawn_session_if_needed(
    session_name: &str,
    client_attributes: ClientAttributes,
    config_file_path: Option<PathBuf>,
    config_options: &Options,
    os_input: Box<dyn ClientOsApi>,
    requested_layout: Option<LayoutInfo>,
) -> (ClientToServerMsg, PathBuf) {
    if session_exists(&session_name).unwrap_or(false) {
        ipc_pipe_and_first_message_for_existing_session(
            session_name,
            client_attributes,
            config_file_path,
            config_options.clone(),
        )
    } else {
        // we still parse the resurrection layout here even though we don't use it just in case
        // it's corrupted
        let resurrection_layout = resurrection_layout(&session_name).ok().flatten();

        if resurrection_layout.is_some() {
            spawn_new_session(
                &session_name,
                os_input.clone(),
                config_file_path,
                config_options.clone(),
                Some(LayoutInfo::File(
                    session_layout_cache_file_name(&session_name)
                        .display()
                        .to_string(),
                )),
                client_attributes,
            )
        } else {
            spawn_new_session(
                &session_name,
                os_input.clone(),
                config_file_path,
                config_options.clone(),
                requested_layout,
                client_attributes,
            )
        }
    }
}

fn spawn_new_session(
    name: &str,
    mut os_input: Box<dyn ClientOsApi>,
    config_file_path: Option<PathBuf>,
    mut config_opts: Options,
    layout_info: Option<LayoutInfo>,
    client_attributes: ClientAttributes,
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
    let is_web_client = true;

    (
        ClientToServerMsg::FirstClientConnected {
            cli_assets,
            is_web_client,
        },
        zellij_ipc_pipe,
    )
}

fn ipc_pipe_and_first_message_for_existing_session(
    session_name: &str,
    client_attributes: ClientAttributes,
    config_file_path: Option<PathBuf>,
    mut config_opts: Options,
) -> (ClientToServerMsg, PathBuf) {
    let zellij_ipc_pipe: PathBuf = {
        let mut sock_dir = zellij_utils::consts::ZELLIJ_SOCK_DIR.clone();
        fs::create_dir_all(&sock_dir).unwrap();
        zellij_utils::shared::set_permissions(&sock_dir, 0o700).unwrap();
        sock_dir.push(session_name);
        sock_dir
    };

    config_opts.web_server = Some(true);
    config_opts.web_sharing = Some(WebSharing::On);
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

    let first_message = ClientToServerMsg::AttachClient {
        cli_assets,
        tab_position_to_focus: None,
        pane_to_focus: None,
        is_web_client,
    };
    (first_message, zellij_ipc_pipe)
}
