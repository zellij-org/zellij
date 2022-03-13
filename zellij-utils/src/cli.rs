use crate::setup::Setup;
use crate::{
    consts::{ZELLIJ_CONFIG_DIR_ENV, ZELLIJ_CONFIG_FILE_ENV},
    input::options::CliOptions,
};
use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Parser, Default, Debug, Clone, Serialize, Deserialize)]
#[clap(version, name = "zellij")]
pub struct CliArgs {
    /// Maximum panes on screen, caution: opening more panes will close old ones
    #[clap(long)]
    pub max_panes: Option<usize>,

    /// Change where zellij looks for layouts and plugins
    #[clap(long, parse(from_os_str), overrides_with = "data_dir")]
    pub data_dir: Option<PathBuf>,

    /// Run server listening at the specified socket path
    #[clap(long, parse(from_os_str), hide = true, overrides_with = "server")]
    pub server: Option<PathBuf>,

    /// Specify name of a new session
    #[clap(long, short, overrides_with = "session")]
    pub session: Option<String>,

    /// Name of a layout file in the layout directory
    #[clap(short, long, parse(from_os_str), overrides_with = "layout")]
    pub layout: Option<PathBuf>,

    /// Path to a layout yaml file
    #[clap(long, parse(from_os_str), overrides_with = "layout_path")]
    pub layout_path: Option<PathBuf>,

    /// Change where zellij looks for the configuration file
    #[clap(short, long, overrides_with = "config", env = ZELLIJ_CONFIG_FILE_ENV, parse(from_os_str))]
    pub config: Option<PathBuf>,

    /// Change where zellij looks for the configuration directory
    #[clap(long, overrides_with = "config_dir", env = ZELLIJ_CONFIG_DIR_ENV, parse(from_os_str))]
    pub config_dir: Option<PathBuf>,

    #[clap(subcommand)]
    pub command: Option<Command>,

    #[clap(short, long)]
    pub debug: bool,
}

#[derive(Debug, Subcommand, Clone, Serialize, Deserialize)]
pub enum Command {
    /// Change the behaviour of zellij
    #[clap(name = "options")]
    Options(CliOptions),

    /// Setup zellij and check its configuration
    #[clap(name = "setup")]
    Setup(Setup),

    /// Explore existing zellij sessions
    #[clap(flatten)]
    Sessions(Sessions),
}

#[derive(Debug, Subcommand, Clone, Serialize, Deserialize)]
pub enum SessionCommand {
    /// Change the behaviour of zellij
    #[clap(name = "options")]
    Options(CliOptions),
}

#[derive(Debug, Subcommand, Clone, Serialize, Deserialize)]
pub enum Sessions {
    /// List active sessions
    #[clap(alias = "ls")]
    ListSessions,

    /// Attach to session
    #[clap(alias = "a")]
    Attach {
        /// Name of the session to attach to.
        session_name: Option<String>,

        /// Create a session if one does not exist.
        #[clap(short, long)]
        create: bool,

        /// Number of the session index in the active sessions ordered creation date.
        #[clap(long)]
        index: Option<usize>,

        /// Change the behaviour of zellij
        #[clap(subcommand, name = "options")]
        options: Option<SessionCommand>,
    },

    /// Kill the specific session
    #[clap(alias = "k")]
    KillSession {
        /// Name of target session
        target_session: Option<String>,
    },

    /// Kill all sessions
    #[clap(alias = "ka")]
    KillAllSessions {
        /// Automatic yes to prompts
        #[clap(short, long)]
        yes: bool,
    },
}
