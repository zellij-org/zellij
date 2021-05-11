use super::common::utils::consts::{ZELLIJ_CONFIG_DIR_ENV, ZELLIJ_CONFIG_FILE_ENV};
use crate::common::input::options::Options;
use crate::common::setup::Setup;
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
    #[structopt(long)]
    pub data_dir: Option<PathBuf>,

    #[structopt(long)]
    pub server: Option<PathBuf>,

    /// Path to a layout yaml file
    #[structopt(short, long)]
    pub layout: Option<PathBuf>,

    /// Change where zellij looks for the configuration
    #[structopt(short, long, env=ZELLIJ_CONFIG_FILE_ENV)]
    pub config: Option<PathBuf>,

    /// Change where zellij looks for the configuration
    #[structopt(long, env=ZELLIJ_CONFIG_DIR_ENV)]
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
