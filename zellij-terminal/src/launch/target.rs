use crate::launch::starting_folder;
use crate::session::connection::{Capabilities, Geometry, Opening};
use crate::{remote, session};
use anyhow::{anyhow, Result};
use std::path::PathBuf;
use zellij_client::session_resolution::{self, ResolutionFailure};
use zellij_client::{first_message, ClientInfo};
use zellij_utils::cli::{AttachArgs, CliArgs, TerminalArgs};
use zellij_utils::data::ConnectToSession;
use zellij_utils::input::actions::initial_panes_from_cli;
use zellij_utils::input::options::Options as SessionOptions;
use zellij_utils::sessions::generate_unique_session_name;

pub(crate) enum Reach {
    Local(ClientInfo),
    #[cfg_attr(not(feature = "web_server_capability"), allow(dead_code))]
    Remote(Box<AttachArgs>),
}

pub(crate) struct Target {
    pub(crate) reach: Reach,
    pub(crate) opening: Opening,
}

pub(crate) struct Refusal {
    pub(crate) text: String,
}

impl Refusal {
    pub(crate) fn new(text: impl Into<String>) -> Self {
        Refusal { text: text.into() }
    }
}

impl From<ResolutionFailure> for Refusal {
    fn from(failure: ResolutionFailure) -> Self {
        Refusal {
            text: failure.text(),
        }
    }
}

pub(crate) fn resolve(
    args: &TerminalArgs,
    config_options: &SessionOptions,
    capabilities: Capabilities,
) -> Result<Target, Refusal> {
    let attach = &args.attach;
    if attach.remote_url().is_some() {
        return remote::target(args, capabilities);
    }
    if args.watch {
        let info =
            session_resolution::resolve_watch(attach.session_name.clone(), config_options.clone())?;
        return Ok(Target {
            reach: Reach::Local(info),
            opening: Opening::Watcher,
        });
    }
    let current_dir = starting_folder(args);
    let chosen_folder = args.working_directory.as_ref().map(|_| current_dir.clone());
    let initial_panes = initial_panes_from_cli(
        attach.initial_command.clone(),
        None,
        Some(current_dir.clone()),
        current_dir,
        attach.close_on_exit,
        attach.start_suspended,
    );
    let info = if attach.session_name.is_none() && attach.index.is_none() {
        let Some(name) = generate_unique_session_name() else {
            return Err(Refusal::new(
                "Failed to generate a unique session name, giving up",
            ));
        };
        ClientInfo::New(name, None, chosen_folder, initial_panes)
    } else {
        let mut resolved = session_resolution::resolve_attach(
            attach,
            config_options.clone(),
            false,
            chosen_folder.clone(),
        )?;
        if let Some(initial_panes) = initial_panes {
            resolved.client.set_initial_panes(initial_panes);
        }
        if let (Some(folder), ClientInfo::New(..)) = (chosen_folder, &resolved.client) {
            resolved.client.set_cwd(folder);
        }
        resolved.client
    };
    Ok(Target {
        reach: Reach::Local(info),
        opening: Opening::Participant(capabilities),
    })
}

pub(crate) fn connect(
    target: &Target,
    opts: &CliArgs,
    config_options: &SessionOptions,
    geometry: Geometry,
    focus: (Option<usize>, Option<(u32, bool)>),
    local_config: remote::LocalConfig,
    switch_context: Option<zellij_utils::session_history::SwitchContext>,
) -> Result<session::connection::Connection> {
    let info = match &target.reach {
        Reach::Local(info) => info,
        Reach::Remote(attach) => return remote::connect(attach, geometry, local_config),
    };
    let handshake = first_message(
        info,
        opts,
        config_options,
        geometry.size(),
        session::spawn::advertised_host_terminal_env(),
        focus.0,
        focus.1,
        switch_context,
    );
    let session_name = info.get_session_name().to_owned();
    let socket_path = if handshake.starts_server {
        start_server(&session_name)?
    } else {
        session::connection::socket_path(&session_name)?
    };
    session::connection::open_at(
        &socket_path,
        &session_name,
        geometry,
        target.opening,
        handshake.message,
        handshake.starts_server,
    )
}

pub(crate) fn switch_target(
    to: &ConnectToSession,
    config_options: &SessionOptions,
    capabilities: Capabilities,
) -> Result<(Target, SessionOptions)> {
    let name = match to.name.clone() {
        Some(name) => name,
        None => generate_unique_session_name()
            .ok_or_else(|| anyhow!("Failed to generate a unique session name, giving up"))?,
    };
    let args = AttachArgs {
        session_name: Some(name),
        create: true,
        ..AttachArgs::default()
    };
    let mut resolved =
        session_resolution::resolve_attach(&args, config_options.clone(), false, to.cwd.clone())
            .map_err(|failure| anyhow!("{}", failure.text()))?;
    if let Some(layout) = &to.layout {
        resolved.client.set_layout_info(layout.clone());
    }
    if let Some(cwd) = &to.cwd {
        resolved.client.set_cwd(cwd.clone());
    }
    Ok((
        Target {
            reach: Reach::Local(resolved.client),
            opening: Opening::Participant(capabilities),
        },
        resolved.config_options,
    ))
}

pub(crate) fn switch(
    to: &ConnectToSession,
    opts: &CliArgs,
    config_options: &SessionOptions,
    geometry: Geometry,
    capabilities: Capabilities,
) -> Result<session::connection::Connection> {
    let (target, resolved_options) = switch_target(to, config_options, capabilities)?;
    connect(
        &target,
        opts,
        &resolved_options,
        geometry,
        (to.tab_position, to.pane_id),
        Default::default(),
        to.switch_context.clone(),
    )
}

// Returns the new session's socket path, which is named by a freshly registered session id.
pub(crate) fn start_server(session_name: &str) -> Result<PathBuf> {
    let cwd = session::spawn::resolve_cwd(None)?;
    let program = session::spawn::resolve_program()?;
    session::spawn::prepare_socket_dir()?;
    let socket_path = session::connection::new_session_socket_path(session_name)?;
    if let Some(parent) = socket_path.parent() {
        zellij_utils::consts::prepare_private_dir(parent)
            .map_err(|e| anyhow!("failed to create {:?}: {}", parent, e))?;
    }
    session::spawn::spawn(&session::spawn::server_command(
        program,
        &socket_path,
        session_name,
        &cwd,
    ))?;
    Ok(socket_path)
}
