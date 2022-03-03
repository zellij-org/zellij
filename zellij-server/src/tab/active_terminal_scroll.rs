use crate::panes::PaneId;
use crate::tab::{Pane, Tab};
use crate::ClientId;

#[derive(PartialEq)]
enum Scroll {
    Row(usize),
    HalfPage,
    FullPage,
}

#[derive(PartialEq)]
enum Action {
    Up(Scroll),
    Down(Scroll),
    Clear,
}

impl Action {
    fn is_up(&self) -> bool {
        match *self {
            Action::Up(_) => true,
            _ => false,
        }
    }

    fn get_scroll_rows(&self) -> Option<usize> {
        match *self {
            Action::Up(Scroll::Row(count)) | Action::Down(Scroll::Row(count)) => Some(count),
            _ => None,
        }
    }

    fn contains_half_page(&self) -> bool {
        match *self {
            Action::Up(Scroll::HalfPage) | Action::Down(Scroll::HalfPage) => true,
            _ => false,
        }
    }
    fn contains_full_page(&self) -> bool {
        match *self {
            Action::Up(Scroll::FullPage) | Action::Down(Scroll::FullPage) => true,
            _ => false,
        }
    }
}

pub(crate) struct ActiveTerminalScroll<'a> {
    client_id: ClientId,
    tab: Box<&'a mut Tab>,
}

impl<'a> ActiveTerminalScroll<'a> {
    pub(crate) fn new(client_id: ClientId, tab: Box<&'a mut Tab>) -> Self {
        Self { client_id, tab }
    }

    fn proceed_action_impl(action: &Action, active_pane: &mut Box<dyn Pane>, client_id: ClientId) {
        let scroll_rows = if let Some(count) = action.get_scroll_rows() {
            count
        } else if action.contains_half_page() {
            // prevent overflow when row == 0
            (active_pane.rows().max(1) - 1) / 2
        } else if action.contains_full_page() {
            active_pane.get_content_rows()
        } else {
            0 // Action::Clear
        };

        match action {
            Action::Up(_) => active_pane.scroll_up(scroll_rows, client_id),
            Action::Down(_) => active_pane.scroll_down(scroll_rows, client_id),
            Action::Clear => active_pane.clear_scroll(),
        };
    }

    fn proceed_action(&mut self, action: Action) {
        if self.tab.floating_panes.panes_are_visible() && self.tab.floating_panes.has_active_panes()
        {
            if let Some(active_pane) = self.tab.floating_panes.get_active_pane_mut(self.client_id) {
                Self::proceed_action_impl(&action, active_pane, self.client_id);
                if !action.is_up() && !active_pane.is_scrolled() {
                    if let PaneId::Terminal(raw_fd) = active_pane.pid() {
                        self.tab.process_pending_vte_events(raw_fd);
                    }
                }
            }
        } else if let Some(active_pane) = self
            .tab
            .active_panes
            .get(&self.client_id)
            .and_then(|active_pane_id| self.tab.panes.get_mut(active_pane_id))
        {
            Self::proceed_action_impl(&action, active_pane, self.client_id);
            if !action.is_up() && !active_pane.is_scrolled() {
                if let PaneId::Terminal(raw_fd) = active_pane.pid() {
                    self.tab.process_pending_vte_events(raw_fd);
                }
            }
        }
    }

    pub(crate) fn up(&mut self, count: usize) {
        self.proceed_action(Action::Up(Scroll::Row(count)));
    }
    pub(crate) fn up_half_page(&mut self) {
        self.proceed_action(Action::Up(Scroll::HalfPage));
    }
    pub(crate) fn down(&mut self, count: usize) {
        self.proceed_action(Action::Down(Scroll::Row(count)));
    }
    pub(crate) fn down_page(&mut self) {
        self.proceed_action(Action::Down(Scroll::FullPage));
    }
    pub(crate) fn down_half_page(&mut self) {
        self.proceed_action(Action::Down(Scroll::HalfPage));
    }
    pub(crate) fn clear(&mut self) {
        self.proceed_action(Action::Clear);
    }
}
