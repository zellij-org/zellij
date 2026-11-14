use crate::config::options::Options;
use crate::config::window_state::Startup;
use crate::launch::configured;
use crate::launch::connect;
use crate::launch::plan;
use crate::launch::remembered;
use crate::launch::resolve;
use crate::launch::switch;
use crate::launch::Plan;
use crate::launch::Refusal;
use crate::session::client_loop::{detach_after, detach_on_signal, LoopOptions};
use crate::session::connection::{Detacher, Geometry};
use crate::text::font::FontStack;
use crate::{config, remote, session, text, window};
use anyhow::{bail, Result};
use std::time::Duration;
use zellij_utils::cli::{CliArgs, TerminalArgs};
use zellij_utils::data::ConnectToSession;
use zellij_utils::input::options::Options as SessionOptions;
use zellij_utils::ipc::ExitReason;

pub(crate) fn open_alone(
    args: TerminalArgs,
    opts: CliArgs,
    config_options: SessionOptions,
    settings: config::settings::Settings,
) -> Result<()> {
    let windowed = !args.headless;
    let options = configured(&settings, &args);
    let saved = remembered(&args, &options);

    let Plan {
        geometry,
        capabilities,
        fonts,
        startup,
    } = match plan(&args, &options, saved) {
        Ok(plan) => plan,
        Err(e) => return refuse(Refusal::new(format!("{:#}", e)), &args, None),
    };

    let target = match resolve(&args, &config_options, capabilities) {
        Ok(target) => target,
        Err(refusal) => return refuse(refusal, &args, fonts.map(|fonts| (fonts, options))),
    };
    let local_config = if windowed {
        remote::watched_config(&opts)
    } else {
        Default::default()
    };
    let connection = match connect(
        &target,
        &opts,
        &config_options,
        geometry,
        (None, None),
        local_config,
        None,
    ) {
        Ok(connection) => connection,
        Err(e) => {
            return refuse(
                Refusal::new(format!("{:#}", e)),
                &args,
                fonts.map(|fonts| (fonts, options)),
            )
        },
    };
    drive(
        connection,
        args,
        fonts,
        options,
        settings,
        opts,
        config_options,
        startup,
    )
}

pub(crate) fn refuse(
    refusal: Refusal,
    args: &TerminalArgs,
    surface: Option<(FontStack, Options)>,
) -> Result<()> {
    report!("{}", refusal.text);
    if !args.headless {
        match surface.map(Ok).unwrap_or_else(fallback_surface) {
            Ok((fonts, options)) => crate::window::notice::show(&refusal.text, fonts, &options)?,
            Err(e) => report!("the message could not be shown in a window: {:#}", e),
        }
    }
    bail!("{}", refusal.text)
}

pub(crate) fn fallback_surface() -> Result<(FontStack, Options)> {
    let options = config::options::resolve(&config::settings::Settings::default(), None);
    let fonts = options.font.stack(1.0)?;
    Ok((fonts, options))
}

pub(crate) fn installed_font_families() -> Vec<String> {
    text::discovery::Discovery::open()
        .map(|discovery| discovery.monospace_families())
        .unwrap_or_default()
}

pub(crate) fn drive(
    connection: session::connection::Connection,
    args: TerminalArgs,
    fonts: Option<FontStack>,
    options: Options,
    settings: config::settings::Settings,
    opts: CliArgs,
    config_options: SessionOptions,
    startup: Startup,
) -> Result<()> {
    let windowed = fonts.is_some();
    let detacher = Detacher::new(connection.sender.clone(), connection.role);
    detach_on_signal(detacher.clone());
    if let Some(seconds) = args.duration_secs {
        detach_after(detacher.clone(), Duration::from_secs(seconds));
    }

    let loop_options = LoopOptions {
        record_path: args.record.clone(),
        settings: settings.clone(),
        font_families: windowed.then(installed_font_families),
        allow_remote_clipboard_read: options.allow_remote_clipboard_read,
        ..Default::default()
    };
    match fonts {
        Some(fonts) => {
            let capabilities = session::connection::Capabilities {
                local_media: args.record.is_none(),
                paints: options.paints,
            };
            let window = window::WindowOptions {
                options,
                settings,
                detacher,
                startup,
            };
            let switching = move |to: &ConnectToSession, geometry: Geometry| {
                switch(to, &opts, &config_options, geometry, capabilities)
            };
            window::run(
                connection,
                loop_options,
                window,
                fonts,
                Box::new(switching),
                !args.watch,
            )?;
        },
        None => {
            let outcome = session::client_loop::run(connection, loop_options)?;
            if outcome.switch_to.is_some() {
                report!(
                    "the session asked to switch to another one, \
                     which a window that was never opened cannot follow"
                );
            }
            if let Some(ExitReason::CustomExitStatus(status)) = outcome.exit_reason {
                if status != 0 {
                    bail!("{}", crate::window::notice::exit_status(status));
                }
            }
        },
    }
    Ok(())
}
