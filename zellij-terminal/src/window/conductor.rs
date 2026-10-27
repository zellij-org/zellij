use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, Instant};

use zellij_utils::app_control::{OpenWindowRequest, PopupView};
use zellij_utils::consts::{APP_PANEL_SESSION, APP_QUIT_REQUEST};
use zellij_utils::data::{ConnectToSession, PaneId};
use zellij_utils::input::actions::Action;
use zellij_utils::session_history::SessionHistory;

use super::Handover;
use crate::app::route::{pane_focus, route, AttachedWindow, Route};
use crate::app::{panel_toggle, Intent, PanelToggle};

pub(crate) const POPUP_REOPEN_GUARD: Duration = Duration::from_millis(400);

pub(crate) trait Facts {
    fn session_of(&self, key: u64) -> Option<String>;
    fn has_session(&self, name: &str) -> bool;
    fn live_sessions(&self) -> Vec<String>;
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Input {
    Started {
        first: Option<Intent>,
    },
    Intents(Vec<Intent>),
    OpenAbandoned {
        ticket: u64,
    },
    Connected {
        ticket: u64,
        error: Option<String>,
    },
    WindowFocused(u64),
    WindowClosed {
        key: u64,
        by_user: bool,
    },
    PanelFocused,
    PanelFocusLost,
    PanelClosedByUser,
    PanelGone,
    PanelEnded,
    PanelSwitched {
        to: ConnectToSession,
        token: Option<String>,
    },
    HandedOver {
        handover: Handover,
        from: u64,
    },
    QuitRequested {
        stop_sessions: bool,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Effect {
    OpenWindow {
        ticket: u64,
        request: OpenWindowRequest,
        token: Option<String>,
    },
    OpenPanel {
        ticket: u64,
        view: Option<PopupView>,
        at: Option<(i32, i32)>,
        token: Option<String>,
    },
    ShowWindow {
        ticket: u64,
    },
    ShowPanel {
        ticket: u64,
        view: Option<PopupView>,
    },
    Discard {
        ticket: u64,
    },
    ClosePanel,
    ReleasePanel,
    SwitchPanelView {
        view: PopupView,
    },
    BringPanelForward {
        token: Option<String>,
    },
    RaiseWindow {
        key: u64,
        pane: Option<PaneId>,
        tab: Option<usize>,
        history: Option<SessionHistory>,
        token: Option<String>,
        requester: Option<u64>,
    },
    SendAction {
        key: u64,
        action: Action,
    },
    Notify {
        text: String,
    },
    CloseNotice,
    SetAccessory(bool),
    Quit {
        stop: Vec<String>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PendingPanel {
    ticket: u64,
    view: Option<PopupView>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ShownPanel {
    view: Option<PopupView>,
    focused: bool,
}

pub(crate) struct Conductor {
    hosted: bool,
    windows: BTreeMap<u64, u64>,
    clock: u64,
    next_key: u64,
    pending_windows: BTreeSet<u64>,
    panel: Option<ShownPanel>,
    panel_pending: Option<PendingPanel>,
    lost_focus_at: Option<Instant>,
    last_panel_view: Option<PopupView>,
    started: bool,
    quitting: bool,
}

impl Conductor {
    pub(crate) fn new(hosted: bool) -> Self {
        Conductor {
            hosted,
            windows: BTreeMap::new(),
            clock: 0,
            next_key: 1,
            pending_windows: BTreeSet::new(),
            panel: None,
            panel_pending: None,
            lost_focus_at: None,
            last_panel_view: None,
            started: false,
            quitting: false,
        }
    }

    pub(crate) fn adopt(&mut self, key: u64) {
        self.next_key = self.next_key.max(key + 1);
        self.windows.insert(key, 0);
    }

    #[cfg(test)]
    fn panel_view(&self) -> Option<Option<PopupView>> {
        self.panel.map(|panel| panel.view)
    }

    pub(crate) fn panel_state(&self) -> Option<zellij_utils::app_control::PanelState> {
        match (self.panel, self.panel_pending) {
            (Some(panel), _) => Some(zellij_utils::app_control::PanelState {
                view: panel.view,
                shown: true,
            }),
            (None, Some(pending)) => Some(zellij_utils::app_control::PanelState {
                view: pending.view,
                shown: false,
            }),
            (None, None) => None,
        }
    }

    pub(crate) fn started(&self) -> bool {
        self.started
    }

    pub(crate) fn should_exit(&self) -> bool {
        self.quitting || (!self.hosted && self.started && self.windows.is_empty())
    }

    pub(crate) fn handle(&mut self, input: Input, now: Instant, facts: &dyn Facts) -> Vec<Effect> {
        let mut effects = vec![];
        match input {
            Input::Started { first } => {
                if self.started {
                    return effects;
                }
                self.started = true;
                if let Some(intent) = first {
                    self.intents(vec![intent], now, facts, &mut effects);
                }
                self.follow_window_count(&mut effects);
            },
            Input::Intents(intents) => self.intents(intents, now, facts, &mut effects),
            Input::OpenAbandoned { ticket } => {
                self.pending_windows.remove(&ticket);
                if self.panel_pending.map(|p| p.ticket) == Some(ticket) {
                    self.panel_pending = None;
                }
            },
            Input::Connected { ticket, error } => self.connected(ticket, error, &mut effects),
            Input::WindowFocused(key) => self.note_focus(key),
            Input::WindowClosed { key, by_user } => {
                if self.windows.remove(&key).is_none() {
                    return effects;
                }
                if self.hosted && by_user {
                    effects.push(Effect::CloseNotice);
                }
                self.follow_window_count(&mut effects);
            },
            Input::PanelFocused => {
                if let Some(panel) = self.panel.as_mut() {
                    panel.focused = true;
                }
            },
            Input::PanelFocusLost => {
                if self.panel.map(|panel| panel.focused).unwrap_or(false) {
                    self.lost_focus_at = Some(now);
                    self.close_panel(&mut effects);
                }
            },
            Input::PanelClosedByUser => self.close_panel(&mut effects),
            Input::PanelGone => self.panel = None,
            Input::PanelEnded => {
                if self.panel.take().is_some() {
                    effects.push(Effect::ReleasePanel);
                }
            },
            Input::PanelSwitched { to, token } => {
                self.panel_switched(to, token, now, facts, &mut effects)
            },
            Input::HandedOver { handover, from } => {
                if self.windows.contains_key(&handover.window) {
                    effects.push(Effect::RaiseWindow {
                        key: handover.window,
                        pane: handover.pane,
                        tab: handover.tab,
                        history: handover.history,
                        token: None,
                        requester: Some(from),
                    });
                    self.note_focus(handover.window);
                }
            },
            Input::QuitRequested { stop_sessions } => self.quit(stop_sessions, facts, &mut effects),
        }
        effects
    }

    fn take_key(&mut self) -> u64 {
        let key = self.next_key;
        self.next_key += 1;
        key
    }

    fn note_focus(&mut self, key: u64) {
        if let Some(at) = self.windows.get_mut(&key) {
            self.clock += 1;
            *at = self.clock;
        }
    }

    fn follow_window_count(&self, effects: &mut Vec<Effect>) {
        if self.hosted {
            effects.push(Effect::SetAccessory(self.windows.is_empty()));
        }
    }

    fn attached(&self, facts: &dyn Facts) -> Vec<AttachedWindow> {
        self.windows
            .iter()
            .map(|(key, focused_at)| AttachedWindow {
                key: *key,
                session: facts.session_of(*key),
                focused_at: *focused_at,
            })
            .collect()
    }

    pub(crate) fn window_showing(
        &self,
        session: Option<&str>,
        switching: u64,
        facts: &dyn Facts,
    ) -> Option<u64> {
        let session = session?;
        if !self.hosted {
            return None;
        }
        let others: Vec<AttachedWindow> = self
            .attached(facts)
            .into_iter()
            .filter(|window| window.key != switching)
            .collect();
        match route(session, None, &others) {
            Route::Raise { window, .. } => Some(window),
            Route::Open { .. } => None,
        }
    }

    pub(crate) fn most_recent_window(&self) -> Option<u64> {
        self.most_recent()
    }

    fn most_recent(&self) -> Option<u64> {
        self.windows
            .iter()
            .max_by_key(|(key, focused_at)| (**focused_at, **key))
            .map(|(key, _)| *key)
    }

    fn intents(
        &mut self,
        intents: Vec<Intent>,
        now: Instant,
        facts: &dyn Facts,
        effects: &mut Vec<Effect>,
    ) {
        for intent in intents {
            match intent {
                Intent::Open { request, token } => self.open(request, token, effects),
                Intent::Route {
                    session,
                    pane,
                    token,
                } => match route(&session, pane, &self.attached(facts)) {
                    Route::Raise { window, pane } => {
                        effects.push(Effect::RaiseWindow {
                            key: window,
                            pane,
                            tab: None,
                            history: None,
                            token,
                            requester: None,
                        });
                        self.note_focus(window);
                    },
                    Route::Open { session, pane } => self.open(
                        OpenWindowRequest {
                            session: Some(session),
                            pane_id: pane_focus(pane),
                            ..Default::default()
                        },
                        token,
                        effects,
                    ),
                },
                Intent::Popup { view, at, token } => {
                    self.toggle_panel(view, at, token, now, effects)
                },
                Intent::TogglePanel { view, token } => {
                    self.toggle_panel_view(view, token, now, effects)
                },
                Intent::ConfirmQuitAll => {
                    self.toggle_panel(Some(PopupView::Quit), None, None, now, effects)
                },
                Intent::Back => {
                    if let Some(key) = self.most_recent() {
                        effects.push(Effect::SendAction {
                            key,
                            action: Action::ToggleSession,
                        });
                    }
                },
                Intent::QuitApp => self.quit(false, facts, effects),
                Intent::DetachWindows => {
                    for key in self.windows.keys() {
                        effects.push(Effect::SendAction {
                            key: *key,
                            action: Action::Detach,
                        });
                    }
                },
            }
        }
    }

    fn open(
        &mut self,
        request: OpenWindowRequest,
        token: Option<String>,
        effects: &mut Vec<Effect>,
    ) {
        let ticket = self.take_key();
        if !self.hosted {
            return;
        }
        self.pending_windows.insert(ticket);
        effects.push(Effect::OpenWindow {
            ticket,
            request,
            token,
        });
    }

    fn connected(&mut self, ticket: u64, error: Option<String>, effects: &mut Vec<Effect>) {
        if self.panel_pending.map(|p| p.ticket) == Some(ticket) {
            let pending = self.panel_pending.take();
            match error {
                Some(text) => effects.push(Effect::Notify {
                    text: format!("The inbox could not be opened: {}", text),
                }),
                None => {
                    let view = pending.and_then(|p| p.view);
                    self.panel = Some(ShownPanel {
                        view,
                        focused: false,
                    });
                    effects.push(Effect::ShowPanel { ticket, view });
                },
            }
            return;
        }
        if !self.pending_windows.remove(&ticket) {
            effects.push(Effect::Discard { ticket });
            return;
        }
        match error {
            Some(text) => effects.push(Effect::Notify {
                text: format!("A window could not be opened: {}", text),
            }),
            None => {
                self.windows.insert(ticket, 0);
                self.note_focus(ticket);
                effects.push(Effect::ShowWindow { ticket });
                self.follow_window_count(effects);
            },
        }
    }

    fn close_panel(&mut self, effects: &mut Vec<Effect>) {
        self.panel_pending = None;
        if let Some(panel) = self.panel.take() {
            self.last_panel_view = panel.view;
            effects.push(Effect::ClosePanel);
        }
    }

    fn just_closed(&self, now: Instant) -> bool {
        self.lost_focus_at
            .map(|at| now.saturating_duration_since(at) < POPUP_REOPEN_GUARD)
            .unwrap_or(false)
    }

    fn toggle_panel(
        &mut self,
        view: Option<PopupView>,
        at: Option<(i32, i32)>,
        token: Option<String>,
        now: Instant,
        effects: &mut Vec<Effect>,
    ) {
        let toggling = view.is_none();
        if self.panel.is_some() || self.panel_pending.is_some() {
            let pending_only = self.panel.is_none();
            self.close_panel(effects);
            if toggling || pending_only {
                return;
            }
        }
        if toggling && self.just_closed(now) {
            self.lost_focus_at = None;
            return;
        }
        if !self.hosted {
            return;
        }
        let ticket = self.take_key();
        self.panel_pending = Some(PendingPanel { ticket, view });
        effects.push(Effect::OpenPanel {
            ticket,
            view,
            at,
            token,
        });
    }

    fn toggle_panel_view(
        &mut self,
        view: PopupView,
        token: Option<String>,
        now: Instant,
        effects: &mut Vec<Effect>,
    ) {
        let just_closed = if self.just_closed(now) {
            self.last_panel_view
        } else {
            None
        };
        let choice = match self.panel.map(|panel| panel.view) {
            Some(None) => PanelToggle::Switch,
            shown => panel_toggle(
                shown.flatten(),
                self.panel_pending.is_some(),
                view,
                just_closed,
            ),
        };
        match choice {
            PanelToggle::Close => self.close_panel(effects),
            PanelToggle::Ignore => self.lost_focus_at = None,
            PanelToggle::Open => self.toggle_panel(Some(view), None, token, now, effects),
            PanelToggle::Switch => {
                if let Some(panel) = self.panel.as_mut() {
                    panel.view = Some(view);
                    effects.push(Effect::SwitchPanelView { view });
                }
                effects.push(Effect::BringPanelForward { token });
            },
        }
    }

    fn panel_switched(
        &mut self,
        to: ConnectToSession,
        token: Option<String>,
        now: Instant,
        facts: &dyn Facts,
        effects: &mut Vec<Effect>,
    ) {
        if to.name.as_deref() == Some(APP_QUIT_REQUEST) {
            self.quit(true, facts, effects);
            return;
        }
        if self.panel.take().is_some() {
            effects.push(Effect::ReleasePanel);
        }
        let pane = to.pane_id.map(|(id, plugin)| {
            if plugin {
                PaneId::Plugin(id)
            } else {
                PaneId::Terminal(id)
            }
        });
        let live = self.hosted
            && to
                .name
                .as_deref()
                .map(|name| facts.has_session(name))
                .unwrap_or(false);
        let intent = match to.name.clone() {
            Some(session) if live => Intent::Route {
                session,
                pane,
                token,
            },
            name => Intent::Open {
                request: OpenWindowRequest {
                    session: name,
                    create: true,
                    pane_id: to.pane_id,
                    cwd: to.cwd.clone(),
                    layout: to.layout.clone(),
                    ..Default::default()
                },
                token,
            },
        };
        self.intents(vec![intent], now, facts, effects);
    }

    fn quit(&mut self, stop_sessions: bool, facts: &dyn Facts, effects: &mut Vec<Effect>) {
        self.close_panel(effects);
        let mut stop = if self.hosted && stop_sessions {
            facts.live_sessions()
        } else {
            vec![]
        };
        if self.hosted {
            stop.push(APP_PANEL_SESSION.to_owned());
        }
        self.quitting = true;
        effects.push(Effect::Quit { stop });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::path::PathBuf;

    #[derive(Default)]
    struct World {
        sessions: HashMap<u64, String>,
        live: Vec<String>,
    }

    impl Facts for World {
        fn session_of(&self, key: u64) -> Option<String> {
            self.sessions.get(&key).cloned()
        }
        fn has_session(&self, name: &str) -> bool {
            self.live.iter().any(|live| live == name)
        }
        fn live_sessions(&self) -> Vec<String> {
            self.live.clone()
        }
    }

    struct Rig {
        conductor: Conductor,
        world: World,
        start: Instant,
    }

    impl Rig {
        fn hosted() -> Rig {
            Rig {
                conductor: Conductor::new(true),
                world: World::default(),
                start: Instant::now(),
            }
        }

        fn single() -> Rig {
            Rig {
                conductor: Conductor::new(false),
                ..Rig::hosted()
            }
        }

        fn at(&mut self, millis: u64, input: Input) -> Vec<Effect> {
            let now = self.start + Duration::from_millis(millis);
            self.conductor.handle(input, now, &self.world)
        }

        fn feed(&mut self, input: Input) -> Vec<Effect> {
            self.at(0, input)
        }

        fn intent(&mut self, millis: u64, intent: Intent) -> Vec<Effect> {
            self.at(millis, Input::Intents(vec![intent]))
        }

        fn window_on(&mut self, session: &str) -> u64 {
            let effects = self.intent(
                0,
                Intent::Open {
                    request: OpenWindowRequest::default(),
                    token: None,
                },
            );
            let ticket = opened_window(&effects);
            self.feed(Input::Connected {
                ticket,
                error: None,
            });
            self.world.sessions.insert(ticket, session.to_owned());
            ticket
        }

        fn open_panel(&mut self, millis: u64, view: PopupView) -> u64 {
            let effects = self.intent(millis, Intent::TogglePanel { view, token: None });
            let ticket = opened_panel(&effects).expect("the panel opens");
            self.at(
                millis,
                Input::Connected {
                    ticket,
                    error: None,
                },
            );
            self.at(millis, Input::PanelFocused);
            ticket
        }
    }

    fn opened_window(effects: &[Effect]) -> u64 {
        effects
            .iter()
            .find_map(|effect| match effect {
                Effect::OpenWindow { ticket, .. } => Some(*ticket),
                _ => None,
            })
            .expect("a window is opened")
    }

    fn opened_panel(effects: &[Effect]) -> Option<u64> {
        effects.iter().find_map(|effect| match effect {
            Effect::OpenPanel { ticket, .. } => Some(*ticket),
            _ => None,
        })
    }

    fn toggle(view: PopupView) -> Intent {
        Intent::TogglePanel { view, token: None }
    }

    fn tray_click() -> Intent {
        Intent::Popup {
            view: None,
            at: Some((10, 20)),
            token: Some("click".to_owned()),
        }
    }

    fn switch_to(name: &str) -> ConnectToSession {
        ConnectToSession {
            name: Some(name.to_owned()),
            ..Default::default()
        }
    }

    #[test]
    fn the_panel_is_described_as_connecting_then_shown_then_gone() {
        use zellij_utils::app_control::PanelState;
        let mut rig = Rig::hosted();
        assert_eq!(rig.conductor.panel_state(), None);
        let ticket = opened_panel(&rig.intent(0, toggle(PopupView::Sessions))).unwrap();
        assert_eq!(
            rig.conductor.panel_state(),
            Some(PanelState {
                view: Some(PopupView::Sessions),
                shown: false
            })
        );
        rig.feed(Input::Connected {
            ticket,
            error: None,
        });
        assert_eq!(
            rig.conductor.panel_state(),
            Some(PanelState {
                view: Some(PopupView::Sessions),
                shown: true
            })
        );
        rig.feed(Input::PanelClosedByUser);
        assert_eq!(rig.conductor.panel_state(), None);
    }

    #[test]
    fn a_panel_shortcut_opens_closes_and_switches_the_panel() {
        let mut rig = Rig::hosted();
        rig.open_panel(0, PopupView::Inbox);
        assert_eq!(rig.conductor.panel_view(), Some(Some(PopupView::Inbox)));
        assert_eq!(
            rig.intent(
                10,
                Intent::TogglePanel {
                    view: PopupView::Sessions,
                    token: Some("t".to_owned())
                }
            ),
            vec![
                Effect::SwitchPanelView {
                    view: PopupView::Sessions
                },
                Effect::BringPanelForward {
                    token: Some("t".to_owned())
                }
            ]
        );
        assert_eq!(rig.conductor.panel_view(), Some(Some(PopupView::Sessions)));
        assert_eq!(
            rig.intent(20, toggle(PopupView::Sessions)),
            vec![Effect::ClosePanel]
        );
        assert_eq!(rig.conductor.panel_view(), None);
    }

    #[test]
    fn a_panel_opened_on_the_automatic_view_is_switched_by_any_shortcut() {
        let mut rig = Rig::hosted();
        let effects = rig.intent(0, tray_click());
        let ticket = opened_panel(&effects).unwrap();
        rig.feed(Input::Connected {
            ticket,
            error: None,
        });
        assert_eq!(rig.conductor.panel_view(), Some(None));
        assert_eq!(
            rig.intent(5, toggle(PopupView::Inbox)),
            vec![
                Effect::SwitchPanelView {
                    view: PopupView::Inbox
                },
                Effect::BringPanelForward { token: None }
            ]
        );
    }

    #[test]
    fn shortcuts_are_ignored_while_the_panel_is_still_connecting() {
        let mut rig = Rig::hosted();
        let effects = rig.intent(0, toggle(PopupView::Inbox));
        assert!(opened_panel(&effects).is_some());
        assert!(rig.intent(5, toggle(PopupView::Inbox)).is_empty());
        assert!(rig.intent(5, toggle(PopupView::Sessions)).is_empty());
    }

    #[test]
    fn a_shortcut_right_after_a_focus_loss_close_of_its_view_does_not_reopen_it() {
        let mut rig = Rig::hosted();
        rig.open_panel(0, PopupView::Inbox);
        assert_eq!(rig.at(100, Input::PanelFocusLost), vec![Effect::ClosePanel]);
        assert!(rig.intent(300, toggle(PopupView::Inbox)).is_empty());
        assert!(
            opened_panel(&rig.intent(310, toggle(PopupView::Inbox))).is_some(),
            "the guard is used up by the first shortcut"
        );
    }

    #[test]
    fn the_reopen_guard_expires_and_does_not_hold_back_another_view() {
        let mut rig = Rig::hosted();
        rig.open_panel(0, PopupView::Inbox);
        rig.at(100, Input::PanelFocusLost);
        assert!(opened_panel(&rig.intent(200, toggle(PopupView::Sessions))).is_some());
        let mut rig = Rig::hosted();
        rig.open_panel(0, PopupView::Inbox);
        rig.at(100, Input::PanelFocusLost);
        assert!(opened_panel(&rig.intent(100 + 400, toggle(PopupView::Inbox))).is_some());
    }

    #[test]
    fn a_tray_click_toggles_the_panel_and_honours_the_reopen_guard() {
        let mut rig = Rig::hosted();
        let effects = rig.intent(0, tray_click());
        assert_eq!(
            effects,
            vec![Effect::OpenPanel {
                ticket: 1,
                view: None,
                at: Some((10, 20)),
                token: Some("click".to_owned())
            }]
        );
        rig.feed(Input::Connected {
            ticket: 1,
            error: None,
        });
        rig.feed(Input::PanelFocused);
        assert_eq!(rig.intent(50, tray_click()), vec![Effect::ClosePanel]);
        let ticket = opened_panel(&rig.intent(60, tray_click())).unwrap();
        rig.at(
            60,
            Input::Connected {
                ticket,
                error: None,
            },
        );
        rig.at(60, Input::PanelFocused);
        assert_eq!(rig.at(70, Input::PanelFocusLost), vec![Effect::ClosePanel]);
        assert!(
            rig.intent(200, tray_click()).is_empty(),
            "the click that took focus away closes the panel rather than reopening it"
        );
        assert!(opened_panel(&rig.intent(210, tray_click())).is_some());
    }

    #[test]
    fn a_menu_item_switches_an_open_panel_to_its_view_by_reopening_it() {
        let mut rig = Rig::hosted();
        rig.open_panel(0, PopupView::Inbox);
        let effects = rig.intent(
            10,
            Intent::Popup {
                view: Some(PopupView::Sessions),
                at: None,
                token: None,
            },
        );
        assert_eq!(effects[0], Effect::ClosePanel);
        assert!(matches!(
            effects[1],
            Effect::OpenPanel {
                view: Some(PopupView::Sessions),
                ..
            }
        ));
    }

    #[test]
    fn a_tray_click_while_the_panel_connects_cancels_it_and_its_connection_is_discarded() {
        let mut rig = Rig::hosted();
        let ticket = opened_panel(&rig.intent(0, tray_click())).unwrap();
        assert!(rig.intent(10, tray_click()).is_empty());
        assert_eq!(
            rig.feed(Input::Connected {
                ticket,
                error: None
            }),
            vec![Effect::Discard { ticket }]
        );
        assert_eq!(rig.conductor.panel_view(), None);
    }

    #[test]
    fn focus_loss_closes_the_panel_only_after_it_had_focus() {
        let mut rig = Rig::hosted();
        let ticket = opened_panel(&rig.intent(0, toggle(PopupView::Inbox))).unwrap();
        rig.feed(Input::Connected {
            ticket,
            error: None,
        });
        assert!(rig.at(10, Input::PanelFocusLost).is_empty());
        assert!(rig.conductor.panel_view().is_some());
        rig.at(20, Input::PanelFocused);
        assert_eq!(rig.at(30, Input::PanelFocusLost), vec![Effect::ClosePanel]);
    }

    #[test]
    fn a_panel_that_fails_to_connect_says_so_and_leaves_nothing_open() {
        let mut rig = Rig::hosted();
        let ticket = opened_panel(&rig.intent(0, toggle(PopupView::Inbox))).unwrap();
        assert_eq!(
            rig.feed(Input::Connected {
                ticket,
                error: Some("boom".to_owned())
            }),
            vec![Effect::Notify {
                text: "The inbox could not be opened: boom".to_owned()
            }]
        );
        assert_eq!(rig.conductor.panel_view(), None);
        assert!(opened_panel(&rig.intent(10, toggle(PopupView::Inbox))).is_some());
    }

    #[test]
    fn choosing_a_live_session_in_the_panel_brings_its_window_forward_with_the_pane() {
        let mut rig = Rig::hosted();
        rig.world.live = vec!["api".to_owned()];
        let window = rig.window_on("api");
        rig.open_panel(0, PopupView::Inbox);
        let effects = rig.feed(Input::PanelSwitched {
            to: ConnectToSession {
                name: Some("api".to_owned()),
                pane_id: Some((3, false)),
                ..Default::default()
            },
            token: Some("t".to_owned()),
        });
        assert_eq!(
            effects,
            vec![
                Effect::ReleasePanel,
                Effect::RaiseWindow {
                    key: window,
                    pane: Some(PaneId::Terminal(3)),
                    tab: None,
                    history: None,
                    token: Some("t".to_owned()),
                    requester: None
                }
            ]
        );
        assert_eq!(rig.conductor.panel_view(), None);
    }

    #[test]
    fn choosing_a_live_session_without_a_window_opens_one_on_it() {
        let mut rig = Rig::hosted();
        rig.world.live = vec!["api".to_owned()];
        rig.window_on("docs");
        rig.open_panel(0, PopupView::Sessions);
        let effects = rig.feed(Input::PanelSwitched {
            to: ConnectToSession {
                name: Some("api".to_owned()),
                pane_id: Some((2, true)),
                ..Default::default()
            },
            token: None,
        });
        assert_eq!(effects[0], Effect::ReleasePanel);
        match &effects[1] {
            Effect::OpenWindow { request, .. } => {
                assert_eq!(request.session.as_deref(), Some("api"));
                assert_eq!(request.pane_id, Some((2, true)));
                assert!(!request.create);
            },
            other => panic!("expected a new window, got {:?}", other),
        }
    }

    #[test]
    fn choosing_a_new_or_exited_session_opens_a_window_that_creates_it() {
        let mut rig = Rig::hosted();
        rig.open_panel(0, PopupView::Sessions);
        let to = ConnectToSession {
            name: Some("fresh".to_owned()),
            cwd: Some(PathBuf::from("/tmp/somewhere")),
            pane_id: Some((1, false)),
            ..Default::default()
        };
        let effects = rig.feed(Input::PanelSwitched {
            to: to.clone(),
            token: Some("t".to_owned()),
        });
        assert_eq!(
            effects,
            vec![
                Effect::ReleasePanel,
                Effect::OpenWindow {
                    ticket: 2,
                    request: OpenWindowRequest {
                        session: Some("fresh".to_owned()),
                        create: true,
                        pane_id: Some((1, false)),
                        cwd: Some(PathBuf::from("/tmp/somewhere")),
                        layout: None,
                        ..Default::default()
                    },
                    token: Some("t".to_owned())
                }
            ]
        );
    }

    #[test]
    fn confirming_quit_in_the_panel_stops_every_live_session_and_the_panel_session() {
        let mut rig = Rig::hosted();
        rig.world.live = vec!["a".to_owned(), "b".to_owned()];
        rig.window_on("a");
        rig.intent(0, Intent::ConfirmQuitAll);
        assert_eq!(
            rig.conductor.panel_view(),
            None,
            "the quit screen is still connecting"
        );
        let ticket = 2;
        rig.feed(Input::Connected {
            ticket,
            error: None,
        });
        assert_eq!(rig.conductor.panel_view(), Some(Some(PopupView::Quit)));
        let effects = rig.feed(Input::PanelSwitched {
            to: switch_to(APP_QUIT_REQUEST),
            token: None,
        });
        assert_eq!(
            effects,
            vec![
                Effect::ClosePanel,
                Effect::Quit {
                    stop: vec!["a".to_owned(), "b".to_owned(), APP_PANEL_SESSION.to_owned()]
                }
            ]
        );
        assert!(rig.conductor.should_exit());
    }

    #[test]
    fn quitting_the_app_from_the_menu_keeps_sessions_but_stops_the_panel_session() {
        let mut rig = Rig::hosted();
        rig.world.live = vec!["a".to_owned()];
        assert_eq!(
            rig.intent(0, Intent::QuitApp),
            vec![Effect::Quit {
                stop: vec![APP_PANEL_SESSION.to_owned()]
            }]
        );
        let mut rig = Rig::single();
        assert_eq!(
            rig.feed(Input::QuitRequested {
                stop_sessions: true
            }),
            vec![Effect::Quit { stop: vec![] }]
        );
    }

    #[test]
    fn a_routed_click_raises_the_most_recently_focused_window_on_the_session() {
        let mut rig = Rig::hosted();
        let first = rig.window_on("api");
        let second = rig.window_on("api");
        rig.feed(Input::WindowFocused(first));
        assert_eq!(
            rig.intent(
                0,
                Intent::Route {
                    session: "api".to_owned(),
                    pane: None,
                    token: None
                }
            ),
            vec![Effect::RaiseWindow {
                key: first,
                pane: None,
                tab: None,
                history: None,
                token: None,
                requester: None
            }]
        );
        rig.feed(Input::WindowFocused(second));
        assert!(matches!(
            rig.intent(
                0,
                Intent::Route {
                    session: "api".to_owned(),
                    pane: None,
                    token: None
                }
            )[..],
            [Effect::RaiseWindow { key, .. }] if key == second
        ));
    }

    #[test]
    fn a_routed_click_without_a_window_on_the_session_opens_one_and_keeps_its_token() {
        let mut rig = Rig::hosted();
        rig.window_on("docs");
        let effects = rig.intent(
            0,
            Intent::Route {
                session: "api".to_owned(),
                pane: Some(PaneId::Terminal(7)),
                token: Some("t".to_owned()),
            },
        );
        assert_eq!(
            effects,
            vec![Effect::OpenWindow {
                ticket: 2,
                request: OpenWindowRequest {
                    session: Some("api".to_owned()),
                    pane_id: Some((7, false)),
                    ..Default::default()
                },
                token: Some("t".to_owned())
            }]
        );
    }

    #[test]
    fn the_first_window_opens_at_start_and_only_once() {
        let mut rig = Rig::hosted();
        let first = Intent::Open {
            request: OpenWindowRequest::default(),
            token: Some("launch".to_owned()),
        };
        let effects = rig.feed(Input::Started {
            first: Some(first.clone()),
        });
        assert_eq!(
            effects,
            vec![
                Effect::OpenWindow {
                    ticket: 1,
                    request: OpenWindowRequest::default(),
                    token: Some("launch".to_owned())
                },
                Effect::SetAccessory(true)
            ]
        );
        assert!(rig.feed(Input::Started { first: Some(first) }).is_empty());
        assert_eq!(
            rig.feed(Input::Connected {
                ticket: 1,
                error: None
            }),
            vec![
                Effect::ShowWindow { ticket: 1 },
                Effect::SetAccessory(false)
            ]
        );
    }

    #[test]
    fn starting_with_a_panel_shortcut_opens_only_the_panel_with_the_launch_token() {
        let mut rig = Rig::hosted();
        let effects = rig.feed(Input::Started {
            first: Some(Intent::TogglePanel {
                view: PopupView::Inbox,
                token: Some("launch".to_owned()),
            }),
        });
        assert_eq!(
            effects,
            vec![
                Effect::OpenPanel {
                    ticket: 1,
                    view: Some(PopupView::Inbox),
                    at: None,
                    token: Some("launch".to_owned())
                },
                Effect::SetAccessory(true)
            ]
        );
        assert_eq!(
            rig.feed(Input::Connected {
                ticket: 1,
                error: None
            }),
            vec![Effect::ShowPanel {
                ticket: 1,
                view: Some(PopupView::Inbox)
            }]
        );
    }

    #[test]
    fn a_window_that_fails_to_connect_is_reported_and_not_counted() {
        let mut rig = Rig::hosted();
        let ticket = opened_window(&rig.intent(
            0,
            Intent::Open {
                request: OpenWindowRequest::default(),
                token: None,
            },
        ));
        assert_eq!(
            rig.feed(Input::Connected {
                ticket,
                error: Some("gone".to_owned())
            }),
            vec![Effect::Notify {
                text: "A window could not be opened: gone".to_owned()
            }]
        );
        assert_eq!(rig.intent(0, Intent::Back), vec![]);
    }

    #[test]
    fn the_close_notice_is_asked_for_only_when_the_user_closes_a_window() {
        let mut rig = Rig::hosted();
        let first = rig.window_on("a");
        let second = rig.window_on("b");
        assert_eq!(
            rig.feed(Input::WindowClosed {
                key: first,
                by_user: false
            }),
            vec![Effect::SetAccessory(false)]
        );
        assert_eq!(
            rig.feed(Input::WindowClosed {
                key: second,
                by_user: true
            }),
            vec![Effect::CloseNotice, Effect::SetAccessory(true)]
        );
        assert!(rig
            .feed(Input::WindowClosed {
                key: second,
                by_user: true
            })
            .is_empty());
        assert!(!rig.conductor.should_exit());
    }

    #[test]
    fn a_single_window_exits_when_its_window_closes_and_never_notices_or_hides() {
        let mut rig = Rig::single();
        rig.conductor.adopt(1);
        assert!(rig.feed(Input::Started { first: None }).is_empty());
        assert!(!rig.conductor.should_exit());
        assert!(rig
            .intent(
                0,
                Intent::Open {
                    request: OpenWindowRequest::default(),
                    token: None
                }
            )
            .is_empty());
        assert!(rig.intent(0, toggle(PopupView::Inbox)).is_empty());
        assert!(rig
            .feed(Input::WindowClosed {
                key: 1,
                by_user: true
            })
            .is_empty());
        assert!(rig.conductor.should_exit());
    }

    #[test]
    fn closing_all_windows_detaches_each_of_them() {
        let mut rig = Rig::hosted();
        let first = rig.window_on("a");
        let second = rig.window_on("b");
        assert_eq!(
            rig.intent(0, Intent::DetachWindows),
            vec![
                Effect::SendAction {
                    key: first,
                    action: Action::Detach
                },
                Effect::SendAction {
                    key: second,
                    action: Action::Detach
                },
            ]
        );
    }

    #[test]
    fn going_back_toggles_the_session_of_the_most_recently_focused_window() {
        let mut rig = Rig::hosted();
        let first = rig.window_on("a");
        rig.window_on("b");
        rig.feed(Input::WindowFocused(first));
        assert_eq!(
            rig.intent(0, Intent::Back),
            vec![Effect::SendAction {
                key: first,
                action: Action::ToggleSession
            }]
        );
    }

    #[test]
    fn a_switch_finds_another_app_window_on_the_session_but_never_the_switching_one() {
        let mut rig = Rig::hosted();
        let docs = rig.window_on("docs");
        let api = rig.window_on("api");
        let second_api = rig.window_on("api");
        rig.feed(Input::WindowFocused(api));
        assert_eq!(
            rig.conductor.window_showing(Some("api"), docs, &rig.world),
            Some(api)
        );
        assert_eq!(
            rig.conductor.window_showing(Some("api"), api, &rig.world),
            Some(second_api)
        );
        assert_eq!(
            rig.conductor.window_showing(Some("docs"), docs, &rig.world),
            None,
            "the only window on the session is the one switching"
        );
        assert_eq!(
            rig.conductor
                .window_showing(Some("elsewhere"), docs, &rig.world),
            None,
            "no window shows the session"
        );
        assert_eq!(rig.conductor.window_showing(None, docs, &rig.world), None);
    }

    #[test]
    fn a_session_shown_only_outside_the_app_is_never_a_window_to_raise() {
        let mut rig = Rig::hosted();
        rig.world.live = vec!["api".to_owned(), "docs".to_owned()];
        let docs = rig.window_on("docs");
        assert_eq!(
            rig.conductor.window_showing(Some("api"), docs, &rig.world),
            None,
            "a standalone window on the session is not one of the app's windows"
        );
        let mut rig = Rig::single();
        rig.conductor.adopt(1);
        rig.world.sessions.insert(1, "docs".to_owned());
        rig.conductor.adopt(2);
        rig.world.sessions.insert(2, "api".to_owned());
        assert_eq!(
            rig.conductor.window_showing(Some("api"), 1, &rig.world),
            None,
            "a standalone window never hands over"
        );
    }

    #[test]
    fn a_handed_over_switch_raises_the_window_with_its_tab_pane_and_history() {
        let mut rig = Rig::hosted();
        let docs = rig.window_on("docs");
        let api = rig.window_on("api");
        rig.feed(Input::WindowFocused(docs));
        let history = SessionHistory {
            back: vec!["docs".to_owned()],
            forward: vec![],
            last: Some("docs".to_owned()),
        };
        let effects = rig.feed(Input::HandedOver {
            handover: Handover {
                window: api,
                tab: Some(2),
                pane: Some(PaneId::Terminal(4)),
                history: Some(history.clone()),
            },
            from: docs,
        });
        assert_eq!(
            effects,
            vec![Effect::RaiseWindow {
                key: api,
                pane: Some(PaneId::Terminal(4)),
                tab: Some(2),
                history: Some(history),
                token: None,
                requester: Some(docs)
            }]
        );
        assert_eq!(rig.conductor.most_recent_window(), Some(api));
        assert!(rig
            .feed(Input::HandedOver {
                handover: Handover {
                    window: 99,
                    tab: None,
                    pane: None,
                    history: None,
                },
                from: docs,
            })
            .is_empty());
    }

    #[test]
    fn closing_the_switching_window_after_a_handover_never_quits_the_app() {
        let mut rig = Rig::hosted();
        let docs = rig.window_on("docs");
        let api = rig.window_on("api");
        rig.feed(Input::HandedOver {
            handover: Handover {
                window: api,
                tab: None,
                pane: None,
                history: None,
            },
            from: docs,
        });
        assert_eq!(
            rig.feed(Input::WindowClosed {
                key: docs,
                by_user: false
            }),
            vec![Effect::SetAccessory(false)]
        );
        assert!(!rig.conductor.should_exit());
    }

    #[test]
    fn a_panel_that_ends_on_its_own_is_released_without_arming_the_guard() {
        let mut rig = Rig::hosted();
        rig.open_panel(0, PopupView::Inbox);
        assert_eq!(rig.feed(Input::PanelEnded), vec![Effect::ReleasePanel]);
        assert!(opened_panel(&rig.intent(10, toggle(PopupView::Inbox))).is_some());
        let mut rig = Rig::hosted();
        rig.open_panel(0, PopupView::Inbox);
        assert_eq!(rig.feed(Input::PanelClosedByUser), vec![Effect::ClosePanel]);
        assert!(opened_panel(&rig.intent(10, toggle(PopupView::Inbox))).is_some());
    }
}
