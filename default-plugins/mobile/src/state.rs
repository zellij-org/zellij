use crate::fit::Fit;
use crate::frame::Frame;
use crate::input::Input;
use crate::navigation::Navigation;
use crate::screens::{
    ActiveScreen, MenuScreen, NewSessionPromptScreen, PanesScreen, SessionsScreen,
    ViewportScreen,
};
use crate::workspace::Workspace;

#[derive(Default)]
pub struct State {
    pub workspace: Workspace,
    pub fit: Fit,
    pub frame: Frame,
    pub input: Input,
    pub navigation: Navigation,
    pub active: ActiveScreen,
    pub viewport: ViewportScreen,
    pub sessions: SessionsScreen,
    pub panes: PanesScreen,
    pub new_session: NewSessionPromptScreen,
    pub menu: MenuScreen,
}
