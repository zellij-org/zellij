//! Per-screen UI units. Each screen is a struct that owns its private
//! state and its key / render / click-action logic; the shared session
//! model and chrome live in the `workspace` / `fit` / `frame` / `input`
//! / `navigation` modules, which each screen borrows as needed.
//!
//! Dispatch is a plain `match` over `ActiveScreen` (see `State::update`
//! / `State::render`) — there is no screen trait. The hamburger menu is
//! an overlay tracked by `MenuScreen::open`, orthogonal to the active
//! body screen (it only ever overlays the Viewport).

pub mod menu;
pub mod new_session;
pub mod panes;
pub mod sessions;
pub mod viewport;

pub use menu::MenuScreen;
pub use new_session::NewSessionPromptScreen;
pub use panes::PanesScreen;
pub use sessions::SessionsScreen;
pub use viewport::ViewportScreen;

/// Which body screen is currently active. Replaces the old
/// `expanded: Option<Selector>` discriminant. The hamburger menu
/// overlay is tracked separately by `MenuScreen::open`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ActiveScreen {
    /// The embedded pane viewport — the default, collapsed view.
    #[default]
    Viewport,
    /// The sessions selector (also the welcome flow).
    Sessions,
    /// The unified pane navigator.
    Panes,
    /// The in-plugin "+ New Session" name-entry prompt.
    NewSessionPrompt,
}
