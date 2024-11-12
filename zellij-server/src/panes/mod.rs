pub mod alacritty_functions;
pub mod grid;
pub mod link_handler;
pub mod selection;
pub mod sixel;
pub mod terminal_character;

mod active_panes;
pub mod floating_panes;
mod plugin_pane;
mod search;
mod terminal_pane;
mod tiled_panes;

use std::ops::Deref;

pub use active_panes::*;
pub use alacritty_functions::*;
pub use floating_panes::*;
pub use grid::*;
pub use link_handler::*;
pub(crate) use plugin_pane::*;
pub use sixel::*;
pub(crate) use terminal_character::*;
pub use terminal_pane::*;
pub use tiled_panes::*;

use crate::tab::PaneTrait;

enum Pane {
    TerminalPane(TerminalPane),
    PluginPane(PluginPane),
}

// Deref trait for Lazy use of common methods as defined in the trait
impl Deref for Pane {
    type Target = impl PaneTrait;

    fn deref(&self) -> &Self::Target {
        match self {
            Pane::TerminalPane(term) => term,
            Pane::PluginPane(plugin) => plugin,
        }
    }
}
