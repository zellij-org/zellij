use crate::consts::{ZELLIJ_CONFIG_DIR_ENV, ZELLIJ_CONFIG_FILE_ENV};
use crate::input::options::Options;
use crate::setup::Setup;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use structopt::StructOpt;

#[derive(StructOpt, Default, Debug, Clone, Serialize, Deserialize)]
#[structopt(name = "zellij")]
pub struct CliArgs {
    /// Maximum panes on screen, caution: opening more panes will close old ones
    #[structopt(long)]
    pub max_panes: Option<usize>,

    /// Change where zellij looks for layouts and plugins
    #[structopt(long, parse(from_os_str))]
    pub data_dir: Option<PathBuf>,

    /// Run server listening at the specified socket path
    #[structopt(long, parse(from_os_str), hidden = true)]
    pub server: Option<PathBuf>,

    /// Specify name of a new session
    #[structopt(long, short)]
    pub session: Option<String>,

    /// Name of a layout file in the layout directory
    #[structopt(short, long, parse(from_os_str))]
    pub layout: Option<PathBuf>,

    /// Path to a layout yaml file
    #[structopt(long, parse(from_os_str))]
    pub layout_path: Option<PathBuf>,

    /// Change where zellij looks for the configuration file
    #[structopt(short, long, env=ZELLIJ_CONFIG_FILE_ENV, parse(from_os_str))]
    pub config: Option<PathBuf>,

    /// Change where zellij looks for the configuration directory
    #[structopt(long, env=ZELLIJ_CONFIG_DIR_ENV, parse(from_os_str))]
    pub config_dir: Option<PathBuf>,

    #[structopt(subcommand)]
    pub command: Option<Command>,

    #[structopt(short, long)]
    pub debug: bool,
}

#[derive(Debug, StructOpt, Clone, Serialize, Deserialize)]
pub enum Command {
    /// Change the behaviour of zellij
    #[structopt(name = "options")]
    Options(Options),

    /// Setup zellij and check its configuration
    #[structopt(name = "setup")]
    Setup(Setup),

    /// Explore existing zellij sessions
    #[structopt(flatten)]
    Sessions(Sessions),
}

#[derive(Debug, StructOpt, Clone, Serialize, Deserialize)]
pub enum Sessions {
    /// List active sessions
    #[structopt(alias = "ls")]
    ListSessions,

    /// Attach to session
    #[structopt(alias = "a")]
    Attach {
        /// Name of the session to attach to.
        session_name: Option<String>,

        /// Force attach- session will detach from the other
        /// zellij client (if any) and attach to this.
        #[structopt(long, short)]
        force: bool,
    },
}
