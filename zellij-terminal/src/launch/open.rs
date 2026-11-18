use crate::config;
use crate::launch::launch_app;
use crate::launch::open_alone;
use crate::launch::refuse;
use crate::launch::starting_folder;
use crate::launch::Refusal;
use anyhow::Result;
use zellij_client::session_resolution;
use zellij_client::ClientInfo;
use zellij_utils::app_control::OpenWindowRequest;
use zellij_utils::cli::{CliArgs, Command, SessionCommand, TerminalArgs};
use zellij_utils::input::config::Config;
use zellij_utils::input::options::Options as SessionOptions;
use zellij_utils::setup::Setup;

pub(crate) type Loaded = (SessionOptions, config::settings::Settings);

pub(crate) fn load(args: &TerminalArgs, opts: &CliArgs) -> std::result::Result<Loaded, Result<()>> {
    let config_options = match Setup::from_cli_args(opts) {
        Ok((_, _, config_options, _, _)) => config_options,
        Err(e) => {
            return Err(refuse(
                Refusal::new(crate::window::notice::described(&e)),
                args,
                None,
            ))
        },
    };
    let settings = if args.headless {
        config::settings::Settings::default()
    } else {
        match config::settings::load(
            Config::config_file_path(opts).as_deref(),
            opts.config_dir.as_deref(),
        ) {
            Ok(settings) => settings,
            Err(e) => return Err(refuse(Refusal::new(format!("{:#}", e)), args, None)),
        }
    };
    Ok((config_options, settings))
}

pub(crate) fn open(args: TerminalArgs, opts: CliArgs) -> Result<()> {
    if !args.headless {
        crate::platform::static_build::ensure_windows()?;
    }
    if panel_view(&args).is_some() {
        return launch_app(args, opts, None);
    }
    let (config_options, settings) = match load(&args, &opts) {
        Ok(loaded) => loaded,
        Err(done) => return done,
    };
    if hosted_by_the_app(&args) {
        let args = match with_the_index_resolved(args, &config_options) {
            Ok(args) => args,
            Err(done) => return done,
        };
        let loaded = args
            .attach
            .options
            .is_none()
            .then_some((config_options, settings));
        return launch_app(args, opts, loaded);
    }
    open_alone(args, opts, config_options, settings)
}

fn with_the_index_resolved(
    mut args: TerminalArgs,
    config_options: &SessionOptions,
) -> std::result::Result<TerminalArgs, Result<()>> {
    if args.attach.index.is_none() || args.attach.remote_url().is_some() {
        return Ok(args);
    }
    let resolved =
        match session_resolution::resolve_attach(&args.attach, config_options.clone(), false, None)
        {
            Ok(resolved) => resolved,
            Err(failure) => return Err(refuse(Refusal::from(failure), &args, None)),
        };
    args.attach.session_name = Some(resolved.client.get_session_name().to_owned());
    args.attach.create = matches!(resolved.client, ClientInfo::New(..));
    args.attach.index = None;
    Ok(args)
}

pub(crate) fn without_session_options(
    args: &TerminalArgs,
    opts: &CliArgs,
) -> (TerminalArgs, CliArgs) {
    let mut args = args.clone();
    args.attach.options = None;
    let mut opts = opts.clone();
    if let Some(Command::Terminal(terminal_args)) = opts.command.as_mut() {
        terminal_args.attach.options = None;
    }
    (args, opts)
}

pub(crate) fn panel_view(args: &TerminalArgs) -> Option<zellij_utils::app_control::PopupView> {
    use zellij_utils::app_control::PopupView;
    if args.inbox {
        Some(PopupView::Inbox)
    } else if args.sessions {
        Some(PopupView::Sessions)
    } else {
        None
    }
}

pub(crate) fn hosted_by_the_app(args: &TerminalArgs) -> bool {
    hosted_by_the_app_with(
        args,
        std::env::var_os("ZELLIJ_TERMINAL_STANDALONE").is_some(),
    )
}

pub(crate) fn hosted_by_the_app_with(args: &TerminalArgs, standalone_env: bool) -> bool {
    if panel_view(args).is_some() {
        return true;
    }
    !args.standalone
        && !standalone_env
        && !args.headless
        && !args.watch
        && args.record.is_none()
        && args.duration_secs.is_none()
}

pub(crate) fn window_request(args: &TerminalArgs) -> OpenWindowRequest {
    OpenWindowRequest {
        session: args.attach.session_name.clone(),
        create: args.attach.create,
        pane_id: None,
        command: args.command().to_vec(),
        cwd: Some(starting_folder(args)),
        close_on_exit: args.closes_command_on_exit(),
        start_suspended: args.attach.start_suspended,
        force_run_commands: args.attach.force_run_commands,
        startup_mode: args.startup_mode,
        layout: None,
        rows: args.rows,
        cols: args.cols,
        options: args
            .attach
            .options
            .as_deref()
            .map(|SessionCommand::Options(options)| options.clone()),
        token: args.attach.token.clone(),
        remember: args.attach.remember,
        forget: args.attach.forget,
        ca_cert: args.attach.ca_cert.clone(),
        insecure: args.attach.insecure,
        env: Default::default(),
    }
}
