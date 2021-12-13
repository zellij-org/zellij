//! Handles cli and configuration options
use crate::cli::Command;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::str::FromStr;
use structopt::StructOpt;
use zellij_tile::data::InputMode;

#[derive(Copy, Clone, Debug, PartialEq, Deserialize, Serialize)]
pub enum OnForceClose {
    #[serde(alias = "quit")]
    Quit,
    #[serde(alias = "detach")]
    Detach,
}

impl Default for OnForceClose {
    fn default() -> Self {
        Self::Detach
    }
}

impl FromStr for OnForceClose {
    type Err = Box<dyn std::error::Error>;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "quit" => Ok(Self::Quit),
            "detach" => Ok(Self::Detach),
            e => Err(e.to_string().into()),
        }
    }
}

#[derive(Clone, Default, Debug, PartialEq, Deserialize, Serialize, StructOpt)]
/// Options that can be set either through the config file,
/// or cli flags - cli flags should take precedence over the config file
/// TODO: In order to correctly parse boolean flags, this is currently split
/// into Options and CliOptions, this could be a good canditate for a macro
pub struct Options {
    /// Allow plugins to use a more simplified layout
    /// that is compatible with more fonts (true or false)
    #[structopt(long)]
    #[serde(default)]
    pub simplified_ui: Option<bool>,
    /// Set the default theme
    #[structopt(long)]
    pub theme: Option<String>,
    /// Set the default mode
    #[structopt(long)]
    pub default_mode: Option<InputMode>,
    /// Set the default shell
    #[structopt(long, parse(from_os_str))]
    pub default_shell: Option<PathBuf>,
    /// Set the layout_dir, defaults to
    /// subdirectory of config dir
    #[structopt(long, parse(from_os_str))]
    pub layout_dir: Option<PathBuf>,
    #[structopt(long)]
    #[serde(default)]
    /// Set the handling of mouse events (true or false)
    /// Can be temporarily bypassed by the [SHIFT] key
    pub mouse_mode: Option<bool>,
    #[structopt(long)]
    #[serde(default)]
    /// Set display of the pane frames (true or false)
    pub pane_frames: Option<bool>,
    /// Set behaviour on force close (quit or detach)
    #[structopt(long)]
    pub on_force_close: Option<OnForceClose>,
    #[structopt(long)]
    pub scroll_buffer_size: Option<usize>,
}

impl Options {
    pub fn from_yaml(from_yaml: Option<Options>) -> Options {
        if let Some(opts) = from_yaml {
            opts
        } else {
            Options::default()
        }
    }

    /// Merges two [`Options`] structs, a `Some` in `other`
    /// will supercede a `Some` in `self`
    // TODO: Maybe a good candidate for a macro?
    pub fn merge(&self, other: Options) -> Options {
        let mouse_mode = other.mouse_mode.or(self.mouse_mode);
        let pane_frames = other.pane_frames.or(self.pane_frames);
        let simplified_ui = other.simplified_ui.or(self.simplified_ui);
        let default_mode = other.default_mode.or(self.default_mode);
        let default_shell = other.default_shell.or_else(|| self.default_shell.clone());
        let layout_dir = other.layout_dir.or_else(|| self.layout_dir.clone());
        let theme = other.theme.or_else(|| self.theme.clone());
        let on_force_close = other.on_force_close.or(self.on_force_close);
        let scroll_buffer_size = other.scroll_buffer_size.or(self.scroll_buffer_size);

        Options {
            simplified_ui,
            theme,
            default_mode,
            default_shell,
            layout_dir,
            mouse_mode,
            pane_frames,
            on_force_close,
            scroll_buffer_size,
        }
    }

    /// Merges two [`Options`] structs,
    /// - `Some` in `other` will supercede a `Some` in `self`
    /// - `Some(bool)` in `other` will toggle a `Some(bool)` in `self`
    // TODO: Maybe a good candidate for a macro?
    pub fn merge_from_cli(&self, other: Options) -> Options {
        let merge_bool = |opt_other: Option<bool>, opt_self: Option<bool>| {
            if opt_other.is_some() ^ opt_self.is_some() {
                opt_other.or(opt_self)
            } else if opt_other.is_some() && opt_self.is_some() {
                Some(opt_other.unwrap() ^ opt_self.unwrap())
            } else {
                None
            }
        };

        let simplified_ui = merge_bool(other.simplified_ui, self.simplified_ui);
        let mouse_mode = merge_bool(other.mouse_mode, self.mouse_mode);
        let pane_frames = merge_bool(other.pane_frames, self.pane_frames);

        let default_mode = other.default_mode.or(self.default_mode);
        let default_shell = other.default_shell.or_else(|| self.default_shell.clone());
        let layout_dir = other.layout_dir.or_else(|| self.layout_dir.clone());
        let theme = other.theme.or_else(|| self.theme.clone());
        let on_force_close = other.on_force_close.or(self.on_force_close);
        let scroll_buffer_size = other.scroll_buffer_size.or(self.scroll_buffer_size);

        Options {
            simplified_ui,
            theme,
            default_mode,
            default_shell,
            layout_dir,
            mouse_mode,
            pane_frames,
            on_force_close,
            scroll_buffer_size,
        }
    }

    pub fn from_cli(&self, other: Option<Command>) -> Options {
        if let Some(Command::Options(options)) = other {
            Options::merge_from_cli(self, options.into())
        } else {
            self.to_owned()
        }
    }
}

#[derive(Clone, Default, Debug, PartialEq, StructOpt, Serialize, Deserialize)]
/// Options that can be set through cli flags
/// boolean flags end up toggling boolean options in `Options`
pub struct CliOptions {
    /// Disable handling of mouse events
    #[structopt(long, conflicts_with("mouse-mode"))]
    pub disable_mouse_mode: bool,
    /// Disable display of pane frames
    #[structopt(long, conflicts_with("pane-frames"))]
    pub no_pane_frames: bool,
    #[structopt(flatten)]
    options: Options,
}

impl From<CliOptions> for Options {
    fn from(cli_options: CliOptions) -> Self {
        let mut opts = cli_options.options;

        if cli_options.no_pane_frames {
            opts.pane_frames = Some(false);
        }
        if cli_options.disable_mouse_mode {
            opts.mouse_mode = Some(false);
        }

        Self {
            simplified_ui: opts.simplified_ui,
            theme: opts.theme,
            default_mode: opts.default_mode,
            default_shell: opts.default_shell,
            layout_dir: opts.layout_dir,
            mouse_mode: opts.mouse_mode,
            pane_frames: opts.pane_frames,
            on_force_close: opts.on_force_close,
            scroll_buffer_size: opts.scroll_buffer_size,
        }
    }
}
