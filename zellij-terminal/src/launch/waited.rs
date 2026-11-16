use crate::launch::hosted_by_the_app;
use crate::launch::open;
use crate::launch::run_app;
use crate::launch::starting_folder;
use crate::{app, config, session};
use anyhow::{anyhow, bail, Result};
use zellij_utils::app_control::{AppRequest, OpenWindowRequest};
use zellij_utils::cli::{CliArgs, TerminalArgs};
use zellij_utils::input::config::Config;
use zellij_utils::sessions::generate_unique_session_name;
use zellij_utils::setup::Setup;

pub(crate) fn terminal_requests(opts: &CliArgs) -> zellij_utils::input::terminal::TerminalRequests {
    config::settings::load(Config::config_file_path(opts).as_deref())
        .ok()
        .and_then(|settings| settings.section.terminal_requests)
        .unwrap_or_default()
}

pub(crate) fn run_waited(args: TerminalArgs, opts: CliArgs) -> Result<i32> {
    let waited = session::terminal_request::Waited {
        command: args.execute.clone(),
        cwd: starting_folder(&args),
        hold: args.hold,
    };
    if hosted_by_the_app(&args) && session::terminal_request::ensure_app_running() {
        let request = OpenWindowRequest {
            startup_mode: args.startup_mode,
            force_run_commands: args.attach.force_run_commands,
            ..Default::default()
        };
        match session::terminal_request::run_in_app(&waited, terminal_requests(&opts), request) {
            Ok(code) => return Ok(code),
            Err(e) => report!(
                "the desktop app could not open the command, so this window runs on its own: {:#}",
                e
            ),
        }
    }
    run_waited_alone(args, opts, waited)
}

pub(crate) fn run_waited_alone(
    args: TerminalArgs,
    opts: CliArgs,
    waited: session::terminal_request::Waited,
) -> Result<i32> {
    let name = generate_unique_session_name()
        .ok_or_else(|| anyhow!("Failed to generate a unique session name, giving up"))?;
    let mut args = args;
    args.attach.session_name = Some(name.clone());
    args.attach.create = true;
    args.attach.initial_command = waited.command.clone();
    args.attach.close_on_exit = false;
    args.execute.clear();
    args.standalone = true;
    let waiter = {
        let waited = waited.clone();
        std::thread::Builder::new()
            .name("waited_command".to_owned())
            .spawn(move || {
                session::terminal_request::wait_in_new_session(&name, &waited).unwrap_or_else(|e| {
                    report!("the command could not be followed: {:#}", e);
                    session::terminal_request::FAILURE
                })
            })?
    };
    let opened = open(args, opts);
    let code = waiter.join().unwrap_or(session::terminal_request::FAILURE);
    if let Err(e) = opened {
        report!("{:#}", e);
    }
    Ok(code)
}

pub(crate) fn run_in_background(args: TerminalArgs, opts: CliArgs) -> Result<()> {
    if zellij_utils::app_control::app_is_running() {
        return Ok(());
    }
    let config_options = match Setup::from_cli_args(&opts) {
        Ok((_, _, config_options, _, _)) => config_options,
        Err(e) => bail!("{}", crate::window::notice::described(&e)),
    };
    let settings = config::settings::load(Config::config_file_path(&opts).as_deref())?;
    let dir = zellij_utils::consts::app_dir();
    match app::control::instance::claim(
        &dir,
        &AppRequest::Ping,
        app::control::instance::HANDOFF_WAIT,
    )? {
        app::control::instance::Claim::Handed(_) => Ok(()),
        app::control::instance::Claim::Primary(instance) => {
            run_app(instance, None, args, opts, config_options, settings)
        },
    }
}
