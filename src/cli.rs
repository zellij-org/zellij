use std::path::PathBuf;
use structopt::StructOpt;

#[derive(StructOpt, Debug)]
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

    /// Path to a layout yaml file
    #[structopt(short, long)]
    pub layout: Option<PathBuf>,

    #[structopt(subcommand)]
    pub config: Option<ConfigCli>,

    #[structopt(short, long)]
    pub debug: bool,
}

#[derive(Debug, StructOpt)]
pub enum ConfigCli {
    /// Path to the configuration yaml file
    Config {
        path: Option<PathBuf>,
        #[structopt(long)]
        /// Disables loading of configuration file at default location
        clean: bool,
    },
}
