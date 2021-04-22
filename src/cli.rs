use std::path::PathBuf;
use structopt::StructOpt;

// TODO add to consts.rs
const ZELLIJ_CONFIG_ENV: &str = "ZELLIJ_CONFIG";

#[derive(StructOpt, Default, Debug)]
#[structopt(name = "zellij")]
pub struct CliArgs {
    /// Send "split (direction h == horizontal / v == vertical)" to active zellij session
    #[structopt(short, long)]
    pub split: Option<char>,

    /// Send "move focused pane" to active zellij session
    #[structopt(short, long)]
    pub move_focus: bool,

    /// Send "open file in new pane" to active zellij session
    #[structopt(short, long)]
    pub open_file: Option<PathBuf>,

    /// Maximum panes on screen, caution: opening more panes will close old ones
    #[structopt(long)]
    pub max_panes: Option<usize>,

    /// Change where zellij looks for layouts and plugins
    #[structopt(long)]
    pub data_dir: Option<PathBuf>,

    /// Path to a layout yaml file
    #[structopt(short, long)]
    pub layout: Option<PathBuf>,

    /// Change where zellij looks for the configuration
    #[structopt(short, long, env=ZELLIJ_CONFIG_ENV)]
    pub config: Option<PathBuf>,

    #[structopt(subcommand)]
    pub option: Option<ConfigCli>,

    #[structopt(short, long)]
    pub debug: bool,
}

#[derive(Debug, StructOpt)]
pub enum ConfigCli {
    /// Change the behaviour of zellij
    #[structopt(name = "option")]
    Config {
        #[structopt(long)]
        /// Disables loading of configuration file at default location
        clean: bool,
    },
}
