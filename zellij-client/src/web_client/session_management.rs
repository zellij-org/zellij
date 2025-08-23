use crate::os_input_output::ClientOsApi;
use crate::spawn_server;

use std::{fs, path::PathBuf};
use zellij_utils::{
    cli::CliArgs,
    data::{ConnectToSession, LayoutInfo, WebSharing},
    envs,
    input::{
        config::{Config, ConfigError},
        layout::Layout,
        options::Options,
        cli_assets::CliAssets,
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

pub fn spawn_session_if_needed(
    session_name: &str,
    path: String,
    client_attributes: ClientAttributes,
    config_file_path: Option<PathBuf>,
    config: &Config,
    config_options: &Options,
    is_web_client: bool,
    os_input: Box<dyn ClientOsApi>,
    requested_layout: Option<LayoutInfo>,
    is_welcome_screen: bool,
) -> (ClientToServerMsg, PathBuf) {
    if session_exists(&session_name).unwrap_or(false) {
        ipc_pipe_and_first_message_for_existing_session(
            path,
            client_attributes,
            config_file_path,
            &config,
            &config_options,
            is_web_client,
        )
    } else {
        let force_run_commands = false;
        let resurrection_layout =
            resurrection_layout(&session_name)
                .ok()
                .flatten()
                .map(|mut resurrection_layout| {
                    if force_run_commands {
                        resurrection_layout.recursively_add_start_suspended(Some(false));
                    }
                    resurrection_layout
                });

        match resurrection_layout {
            Some(resurrection_layout) => spawn_new_session(
                &session_name,
                os_input.clone(),
                config_file_path,
                config.clone(),
                config_options.clone(),
                Some(resurrection_layout),
                client_attributes,
                is_welcome_screen,
            ),
            None => {
                let new_session_layout = layout_for_new_session(&config, requested_layout);

                spawn_new_session(
                    &session_name,
                    os_input.clone(),
                    config_file_path,
                    config.clone(),
                    config_options.clone(),
                    new_session_layout.ok().map(|(l, _c)| l),
                    client_attributes,
                    is_welcome_screen,
                )
            },
        }
    }
}

fn spawn_new_session(
    name: &str,
    mut os_input: Box<dyn ClientOsApi>,
    config_file_path: Option<PathBuf>,
    mut config: Config,
    config_opts: Options,
    layout: Option<Layout>,
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

    // TODO(REFACTOR): THIS IS WRONG!!!111oneoneone
    let cli_assets = CliAssets {
        config_file_path: cli_args.config.clone(),
        config_dir: cli_args.config_dir.clone(),
        should_ignore_config: cli_args.is_setup_clean(),
        explicit_cli_options: cli_args.options(),
        layout: cli_args.layout.clone(),
        terminal_window_size: client_attributes.size,
    };
    config.options.web_server = Some(true);
    config.options.web_sharing = Some(WebSharing::On);
    let is_web_client = true;

    (
        ClientToServerMsg::NewClient(
            cli_assets,
            Box::new(cli_args),
            Box::new(layout.unwrap()),
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
    config_file_path: Option<PathBuf>,
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

    let cli_assets = {
        // TODO(REFACTOR): implement
        unimplemented!()
    };

    let first_message = ClientToServerMsg::AttachClient(
        cli_assets,
        None,
        None,
        is_web_client,
    );
    (first_message, zellij_ipc_pipe)
}
