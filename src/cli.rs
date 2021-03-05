use std::path::PathBuf;
use structopt::StructOpt;

#[derive(StructOpt, Debug, Default)]
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

    /// Path to the configuration yaml file
    #[structopt(short, long)]
    pub config: Option<PathBuf>,

    #[structopt(short, long)]
    pub debug: bool,
}
