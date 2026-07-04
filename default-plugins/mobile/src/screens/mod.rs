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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ActiveScreen {
    #[default]
    Viewport,
    Sessions,
    Panes,
    NewSessionPrompt,
}
