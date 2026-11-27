use crate::config;
use crate::launch::launch_app;
use crate::launch::open_alone;
use crate::launch::refuse;
use crate::launch::starting_folder;
use crate::launch::Refusal;
use anyhow::Result;
use zellij_utils::app_control::OpenWindowRequest;
use zellij_utils::cli::{CliArgs, TerminalArgs};
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
        match config::settings::load(Config::config_file_path(opts).as_deref()) {
            Ok(settings) => settings,
            Err(e) => return Err(refuse(Refusal::new(format!("{:#}", e)), args, None)),
        }
    };
    Ok((config_options, settings))
}

pub(crate) fn open(args: TerminalArgs, opts: CliArgs) -> Result<()> {
    if panel_view(&args).is_some() {
        return launch_app(args, opts, None);
    }
    let (config_options, settings) = match load(&args, &opts) {
        Ok(loaded) => loaded,
        Err(done) => return done,
    };
    if hosted_by_the_app(&args) {
        return launch_app(args, opts, Some((config_options, settings)));
    }
    open_alone(args, opts, config_options, settings)
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
        && args.rows.is_none()
        && args.cols.is_none()
        && args.attach.remote_url().is_none()
        && args.attach.index.is_none()
        && args.attach.options.is_none()
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
    }
}
