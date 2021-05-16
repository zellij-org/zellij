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
    #[structopt(long, parse(from_os_str))]
    pub server: Option<PathBuf>,

    /// Path to a layout yaml file
    #[structopt(short, long, parse(from_os_str))]
    pub layout: Option<PathBuf>,

    /// Change where zellij looks for the configuration
    #[structopt(short, long, env=ZELLIJ_CONFIG_FILE_ENV, parse(from_os_str))]
    pub config: Option<PathBuf>,

    /// Change where zellij looks for the configuration
    #[structopt(long, env=ZELLIJ_CONFIG_DIR_ENV, parse(from_os_str))]
    pub config_dir: Option<PathBuf>,

    #[structopt(subcommand)]
    pub option: Option<ConfigCli>,

    #[structopt(short, long)]
    pub debug: bool,
}

#[derive(Debug, StructOpt, Clone, Serialize, Deserialize)]
pub enum ConfigCli {
    /// Change the behaviour of zellij
    #[structopt(name = "options")]
    Options(Options),

    /// Setup zellij and check its configuration
    #[structopt(name = "setup")]
    Setup(Setup),
}
