use crate::config::options::Options;
use crate::config::window_state::{Startup, WindowState};
use crate::session::connection::Geometry;
use crate::text::font::FontStack;
use crate::{config, session};
use anyhow::{bail, Result};
use std::path::PathBuf;
use zellij_utils::cli::TerminalArgs;
use zellij_utils::input::terminal::StartupMode;

pub(crate) fn starting_folder(args: &TerminalArgs) -> PathBuf {
    args.working_directory
        .clone()
        .map(|dir| match std::env::current_dir() {
            Ok(cwd) if dir.is_relative() => cwd.join(dir),
            _ => dir,
        })
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."))
}

pub(crate) struct Plan {
    pub(crate) geometry: Geometry,
    pub(crate) capabilities: session::connection::Capabilities,
    pub(crate) fonts: Option<FontStack>,
    pub(crate) startup: Startup,
}

pub(crate) fn remembered(args: &TerminalArgs, options: &Options) -> Option<WindowState> {
    if args.headless || options.startup_mode != StartupMode::Remember {
        return None;
    }
    config::window_state::load_from(&config::window_state::path())
}

pub(crate) fn plan(
    args: &TerminalArgs,
    options: &Options,
    saved: Option<WindowState>,
) -> Result<Plan> {
    let startup = Startup::resolve_with(
        options.startup_mode,
        saved,
        args.rows,
        args.cols,
        config::window_state::InitialCells {
            rows: options.initial_rows,
            cols: options.initial_cols,
        },
    );
    if startup.rows == 0 || startup.cols == 0 {
        bail!("rows and cols must both be greater than zero");
    }
    if args.cell_width == 0 || args.cell_height == 0 {
        bail!("cell dimensions must both be greater than zero");
    }

    let windowed = !args.headless;
    let fonts = if windowed {
        Some(options.font.stack(1.0)?)
    } else {
        None
    };
    let (cell_width, cell_height) = match &fonts {
        Some(fonts) => {
            let metrics = fonts.metrics();
            (metrics.width as usize, metrics.height as usize)
        },
        None => (args.cell_width, args.cell_height),
    };

    Ok(Plan {
        geometry: Geometry {
            rows: startup.rows,
            cols: startup.cols,
            cell_width,
            cell_height,
        },
        capabilities: session::connection::Capabilities {
            local_media: windowed && args.record.is_none(),
            paints: options.paints,
        },
        fonts,
        startup,
    })
}

pub(crate) fn configured(settings: &config::settings::Settings, args: &TerminalArgs) -> Options {
    let mut options = config::options::resolve(settings, None);
    if let Some(mode) = args.startup_mode {
        options.startup_mode = mode;
    }
    options
}
