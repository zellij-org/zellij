use std::path::PathBuf;
use structopt::StructOpt;

#[derive(StructOpt, Debug, Default)]
#[structopt(name = "mosaic")]
pub struct Opt {
    /// Send "split (direction h == horizontal / v == vertical)" to active mosaic session
    #[structopt(short, long)]
    pub split: Option<char>,

    /// Send "move focused pane" to active mosaic session
    #[structopt(short, long)]
    pub move_focus: bool,

    /// Send "open file in new pane" to active mosaic session
    #[structopt(short, long)]
    pub open_file: Option<PathBuf>,

    /// Maximum panes on screen, caution: opening more panes will close old ones
    #[structopt(long)]
    pub max_panes: Option<usize>,

    /// Path to a layout yaml file
    #[structopt(short, long)]
    pub layout: Option<PathBuf>,

    #[structopt(short, long)]
    pub debug: bool,
}
