use crate::cli::Command;
use crate::data::{InputMode, WebSharing};
use clap::{ArgEnum, Args};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::str::FromStr;
use crate::input::options::Options;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CliAssets {
    pub config_file_path: Option<PathBuf>,
    pub config_dir: Option<PathBuf>,
    pub should_ignore_config: bool,
    pub explicit_cli_options: Option<Options>,
    pub layout: Option<PathBuf>,
}
