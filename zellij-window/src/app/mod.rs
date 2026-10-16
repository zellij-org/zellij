pub mod bundle;
pub mod engine;
pub mod icon;
pub mod instance;
pub mod model;
pub mod notifier;
#[cfg(windows)]
pub mod platform_windows;
pub mod probe;
pub mod route;
pub mod state;
pub mod tray;
pub mod watcher;

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use zellij_utils::app_control::{AppReply, AppRequest, OpenWindowRequest, PopupView};
use zellij_utils::data::PaneId;
use zellij_utils::input::attention::AttentionConfig;
use zellij_utils::input::config::Config;

use crate::app::engine::{Engine, Policy};
use crate::app::model::{tray_state, TrayState};
use crate::app::notifier::{Notifier, SinkEvent};
use crate::app::state::CloseNotice;
use crate::app::tray::{MenuAction, Tray, TrayEvent};
use crate::app::watcher::{Snapshot, Watcher};

pub const PANEL_VIEW_PIPE: &str = "zellij-app-panel-view";
pub const CONFIG_POLL: Duration = Duration::from_secs(1);
pub const APPEARANCE_POLL: Duration = Duration::from_secs(5);
pub const PAUSE_HOUR: Duration = Duration::from_secs(3600);

pub enum AppEvent {
    Request(AppRequest),
    Snapshot(Box<Snapshot>),
    Tray(TrayEvent),
    Notification(SinkEvent),
    Config(Box<AttentionConfig>),
    Connected {
        ticket: u64,
        result: Result<crate::connection::Connection, String>,
    },
    #[cfg_attr(not(unix), allow(dead_code))]
    Terminate,
    Describe(std::sync::mpsc::Sender<zellij_utils::app_control::AppState>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Intent {
    Open {
        request: OpenWindowRequest,
        token: Option<String>,
    },
    Route {
        session: String,
        pane: Option<PaneId>,
        token: Option<String>,
    },
    Popup {
        view: Option<PopupView>,
        at: Option<(i32, i32)>,
        token: Option<String>,
    },
    TogglePanel {
        view: PopupView,
        token: Option<String>,
    },
    ConfirmQuitAll,
    Back,
    QuitApp,
}

pub fn request_intents(request: AppRequest) -> (AppReply, Vec<Intent>) {
    match request {
        AppRequest::Ping | AppRequest::Describe => (AppReply::Done, vec![]),
        AppRequest::OpenWindow(request) => match request.session.clone() {
            Some(session) if !request.create && request.command.is_empty() => (
                AppReply::Done,
                vec![Intent::Route {
                    session,
                    pane: request.pane_id.map(|(id, plugin)| {
                        if plugin {
                            PaneId::Plugin(id)
                        } else {
                            PaneId::Terminal(id)
                        }
                    }),
                    token: None,
                }],
            ),
            _ => (
                AppReply::Done,
                vec![Intent::Open {
                    request,
                    token: None,
                }],
            ),
        },
        AppRequest::ToggleTrayIcon => (AppReply::Done, vec![]),
        AppRequest::TogglePanel { view, token } => {
            (AppReply::Done, vec![Intent::TogglePanel { view, token }])
        },
        AppRequest::ShowPopup(view) => (
            AppReply::Done,
            vec![Intent::Popup {
                view: Some(view),
                at: None,
                token: None,
            }],
        ),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelToggle {
    Open,
    Switch,
    Close,
    Ignore,
}

pub fn panel_toggle(
    shown: Option<PopupView>,
    opening: bool,
    wanted: PopupView,
    just_closed: Option<PopupView>,
) -> PanelToggle {
    match shown {
        Some(view) if view == wanted => PanelToggle::Close,
        Some(_) => PanelToggle::Switch,
        None if opening => PanelToggle::Ignore,
        None if just_closed == Some(wanted) => PanelToggle::Ignore,
        None => PanelToggle::Open,
    }
}

pub fn launch_token() -> Option<String> {
    let token = ["XDG_ACTIVATION_TOKEN", "DESKTOP_STARTUP_ID"]
        .iter()
        .find_map(|name| std::env::var(name).ok().filter(|v| !v.is_empty()));
    std::env::remove_var("XDG_ACTIVATION_TOKEN");
    std::env::remove_var("DESKTOP_STARTUP_ID");
    token
}

pub fn attention_config(path: Option<&Path>) -> AttentionConfig {
    let Some(path) = path else {
        return AttentionConfig::default();
    };
    if !path.exists() {
        return AttentionConfig::default();
    }
    match Config::from_path(&path.to_path_buf(), None) {
        Ok(config) => config.attention,
        Err(e) => {
            report!("the attention settings could not be read: {}", e);
            AttentionConfig::default()
        },
    }
}

pub fn watch_config(path: PathBuf, post: impl Fn(AttentionConfig) + Send + 'static) {
    let _ = std::thread::Builder::new()
        .name("app_config".to_owned())
        .spawn(move || {
            let modified = |path: &Path| std::fs::metadata(path).and_then(|m| m.modified()).ok();
            let mut last = modified(&path);
            loop {
                std::thread::sleep(CONFIG_POLL);
                let now = modified(&path);
                if now != last {
                    last = now;
                    post(attention_config(Some(&path)));
                }
            }
        });
}

pub fn hide_icon_permanently(path: Option<&Path>) -> Result<(), String> {
    use zellij_utils::input::config_file_edit::{apply_edits, edits_between, read_config_file};
    let path = path.ok_or_else(|| "there is no configuration file to write to".to_owned())?;
    let base = Config::default();
    let mut target = Config::default();
    target.attention.tray_icon = Some(false);
    let edits = edits_between(&base, &target);
    let current = read_config_file(path).map_err(|e| e.to_string())?;
    let written = apply_edits(current.as_deref(), &edits, &target)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(path, written).map_err(|e| e.to_string())
}

pub fn tomorrow() -> SystemTime {
    use chrono::{Duration as ChronoDuration, Local, TimeZone};
    let now = Local::now();
    let midnight = (now.date_naive() + ChronoDuration::days(1))
        .and_hms_opt(0, 0, 0)
        .and_then(|naive| Local.from_local_datetime(&naive).earliest());
    match midnight {
        Some(at) => SystemTime::from(at),
        None => SystemTime::now() + Duration::from_secs(12 * 3600),
    }
}

pub struct Background {
    pub config: AttentionConfig,
    pub config_path: Option<PathBuf>,
    pub snapshot: Snapshot,
    engine: Engine,
    notifier: Notifier,
    watcher: Option<Watcher>,
    tray: Option<Box<dyn Tray>>,
    tray_events: Option<tray::TrayEvents>,
    tray_override: Option<bool>,
    hidden_until_restart: bool,
    paused_until: Option<SystemTime>,
    close_notice: CloseNotice,
    last_tray: Option<TrayState>,
    next_appearance: Instant,
    _holder: Option<instance::Holder>,
    #[cfg(target_os = "macos")]
    _main_menu: Option<tray::MainMenu>,
}

impl Background {
    pub fn new(
        config_path: Option<PathBuf>,
        notifier: Notifier,
        watcher: Option<Watcher>,
        holder: Option<instance::Holder>,
        tray_events: Option<tray::TrayEvents>,
        close_notice: CloseNotice,
    ) -> Background {
        let config = attention_config(config_path.as_deref());
        Background {
            config,
            config_path,
            snapshot: Snapshot::default(),
            engine: Engine::new(),
            notifier,
            watcher,
            tray: None,
            tray_events,
            tray_override: None,
            hidden_until_restart: false,
            paused_until: None,
            close_notice,
            last_tray: None,
            next_appearance: Instant::now() + APPEARANCE_POLL,
            _holder: holder,
            #[cfg(target_os = "macos")]
            _main_menu: None,
        }
    }

    pub fn start_on_main_thread(&mut self) {
        #[cfg(target_os = "macos")]
        if let Some(events) = self.tray_events.clone() {
            self._main_menu = tray::install_main_menu(events);
        }
        self.apply_tray_visibility();
    }

    fn paused(&self) -> bool {
        self.paused_until
            .map(|until| SystemTime::now() < until)
            .unwrap_or(false)
    }

    pub fn policy(&self) -> Policy {
        Policy {
            statuses: self.config.desktop_notifications(),
            details: self.config.notification_details(),
            paused: self.paused(),
            do_not_disturb: false,
        }
    }

    pub fn tray_visible(&self) -> bool {
        self.tray_override
            .unwrap_or(self.config.tray_icon() && !self.hidden_until_restart)
    }

    fn tray_state(&self) -> TrayState {
        tray_state(&self.snapshot.global, self.paused())
    }

    fn apply_tray_visibility(&mut self) {
        let visible = self.tray_visible();
        if visible && self.tray.is_none() {
            if let Some(events) = self.tray_events.clone() {
                let state = self.tray_state();
                self.tray = tray::open(events, &state);
                self.last_tray = Some(state);
            }
            return;
        }
        if let Some(tray) = self.tray.as_mut() {
            tray.set_visible(visible);
        }
    }

    fn refresh_tray(&mut self) {
        let state = self.tray_state();
        if self.last_tray.as_ref() == Some(&state) {
            return;
        }
        if let Some(tray) = self.tray.as_mut() {
            tray.update(&state);
        }
        self.last_tray = Some(state);
    }

    pub fn on_snapshot(&mut self, snapshot: Snapshot, now: Instant) {
        self.snapshot = snapshot;
        let policy = self.policy();
        let commands = self.engine.update(&self.snapshot.global, policy, now);
        self.notifier.apply(commands);
        self.refresh_tray();
    }

    pub fn on_config(&mut self, config: AttentionConfig, now: Instant) {
        if config.tray_icon != self.config.tray_icon {
            self.tray_override = None;
        }
        self.config = config;
        if let Some(watcher) = &self.watcher {
            watcher.set_details(self.config.notification_details());
        }
        let policy = self.policy();
        let commands = self.engine.set_policy(policy, now);
        self.notifier.apply(commands);
        self.apply_tray_visibility();
        self.refresh_tray();
    }

    pub fn on_request(&mut self, request: AppRequest) -> Vec<Intent> {
        if request == AppRequest::ToggleTrayIcon {
            self.tray_override = Some(!self.tray_visible());
            self.apply_tray_visibility();
            return vec![];
        }
        request_intents(request).1
    }

    pub fn on_tray(&mut self, event: TrayEvent, now: Instant) -> Vec<Intent> {
        match event {
            TrayEvent::Activate { at, token } => vec![Intent::Popup {
                view: None,
                at,
                token,
            }],
            TrayEvent::Menu(action) => self.on_menu(action, now),
        }
    }

    fn on_menu(&mut self, action: MenuAction, now: Instant) -> Vec<Intent> {
        match action {
            MenuAction::ShowInbox => vec![Intent::Popup {
                view: Some(PopupView::Inbox),
                at: None,
                token: None,
            }],
            MenuAction::ShowSessions => vec![Intent::Popup {
                view: Some(PopupView::Sessions),
                at: None,
                token: None,
            }],
            MenuAction::NewWindow => vec![Intent::Open {
                request: OpenWindowRequest::default(),
                token: None,
            }],
            MenuAction::ClearAll => {
                clear_all_sessions(self.live_sessions());
                vec![]
            },
            MenuAction::PauseHour => {
                self.pause(Some(SystemTime::now() + PAUSE_HOUR), now);
                vec![]
            },
            MenuAction::PauseUntilTomorrow => {
                self.pause(Some(tomorrow()), now);
                vec![]
            },
            MenuAction::Resume => {
                self.pause(None, now);
                vec![]
            },
            MenuAction::HideUntilRestart => {
                self.hidden_until_restart = true;
                self.tray_override = Some(false);
                self.apply_tray_visibility();
                vec![]
            },
            MenuAction::HidePermanently => {
                if let Err(e) = hide_icon_permanently(self.config_path.as_deref()) {
                    report!("the tray icon setting could not be saved: {}", e);
                }
                self.tray_override = Some(false);
                self.apply_tray_visibility();
                vec![]
            },
            MenuAction::QuitAll => vec![Intent::ConfirmQuitAll],
            MenuAction::Back => vec![Intent::Back],
            MenuAction::QuitApp => vec![Intent::QuitApp],
        }
    }

    fn pause(&mut self, until: Option<SystemTime>, now: Instant) {
        self.paused_until = until;
        let policy = self.policy();
        let commands = self.engine.set_policy(policy, now);
        self.notifier.apply(commands);
        self.refresh_tray();
    }

    pub fn on_notification(&mut self, event: SinkEvent) -> Vec<Intent> {
        match event {
            SinkEvent::Clicked { id, token } => {
                let (session, pane) = engine::target_of(&id, &self.snapshot.global);
                self.engine.forget_dismissed(&id);
                self.notifier
                    .apply(vec![engine::Command::Withdraw { id: id.clone() }]);
                vec![Intent::Route {
                    session,
                    pane,
                    token,
                }]
            },
            SinkEvent::Dismissed(id) => {
                self.engine.forget_dismissed(&id);
                vec![]
            },
        }
    }

    pub fn window_closed_by_user(&mut self) {
        if let Some(text) = self.close_notice.take() {
            let text = if self.tray_visible() {
                format!("{} {}", text, state::CLOSE_NOTICE_TRAY)
            } else {
                text.to_owned()
            };
            self.notifier.simple("Zellij", &text);
        }
    }

    pub fn say(&self, title: &str, body: &str) {
        self.notifier.simple(title, body);
    }

    pub fn deadline(&self) -> Option<Instant> {
        let pause = self.paused_until.map(|until| {
            let left = until
                .duration_since(SystemTime::now())
                .unwrap_or(Duration::ZERO);
            Instant::now() + left
        });
        let appearance = self.tray.as_ref().map(|_| self.next_appearance);
        [self.engine.next_deadline(), pause, appearance]
            .into_iter()
            .flatten()
            .min()
    }

    pub fn tick(&mut self, now: Instant) {
        if self.paused_until.is_some() && !self.paused() {
            self.paused_until = None;
            let policy = self.policy();
            let commands = self.engine.set_policy(policy, now);
            self.notifier.apply(commands);
            self.refresh_tray();
        }
        let commands = self.engine.tick(now);
        self.notifier.apply(commands);
        if now >= self.next_appearance {
            self.next_appearance = now + APPEARANCE_POLL;
            if let Some(tray) = self.tray.as_mut() {
                tray.refresh_appearance();
            }
        }
    }

    pub fn tray_anchor(&self) -> Option<tray::Anchor> {
        self.tray.as_ref().and_then(|tray| tray.anchor())
    }

    pub fn live_sessions(&self) -> Vec<String> {
        match &self.watcher {
            Some(watcher) => watcher.live_sessions(),
            None => self
                .snapshot
                .global
                .sessions
                .iter()
                .map(|s| s.name.clone())
                .collect(),
        }
    }
}

pub fn clear_all_sessions(names: Vec<String>) {
    let _ = std::thread::Builder::new()
        .name("app_clear_all".to_owned())
        .spawn(move || {
            for name in names {
                let action = zellij_utils::input::actions::Action::MarkAllSeen {
                    session: Some(name.clone()),
                };
                if let Err(e) =
                    zellij_utils::session_link::send_actions_to_session(&name, vec![action])
                {
                    report!("the items of {} could not be cleared: {}", name, e);
                }
            }
        });
}

pub fn stop_all_sessions(names: Vec<String>) {
    let handles: Vec<_> = names
        .into_iter()
        .map(|name| {
            std::thread::spawn(move || {
                if let Err(e) =
                    zellij_utils::session_link::stop_session(&name, Duration::from_secs(2))
                {
                    report!("the session {} could not be stopped: {}", name, e);
                }
            })
        })
        .collect();
    for handle in handles {
        let _ = handle.join();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::model::fixtures::*;
    use crate::app::notifier::recording::{Recorded, Recorder};
    use zellij_utils::attention::AttentionStatus;

    fn background() -> (Background, Recorder) {
        let recorder = Recorder::default();
        let sink = recorder.clone();
        let notifier =
            Notifier::spawn_with(move || Some(Box::new(sink) as Box<dyn notifier::Sink>));
        let dir = tempfile::TempDir::new().unwrap();
        let notice = CloseNotice::at(Some(dir.path().join("state.json")));
        std::mem::forget(dir);
        (
            Background::new(None, notifier, None, None, None, notice),
            recorder,
        )
    }

    fn wait_for(recorder: &Recorder, count: usize) -> Vec<Recorded> {
        let deadline = Instant::now() + Duration::from_secs(2);
        let mut seen = vec![];
        while seen.len() < count && Instant::now() < deadline {
            seen.extend(recorder.taken());
            std::thread::sleep(Duration::from_millis(5));
        }
        seen
    }

    fn snapshot(items: Vec<zellij_utils::attention::AttentionItem>) -> Snapshot {
        Snapshot {
            global: global(vec![("b", items)]),
            last_used: Default::default(),
        }
    }

    #[test]
    fn a_click_on_a_notification_routes_to_its_session_and_pane() {
        let (mut background, _) = background();
        background.on_snapshot(
            snapshot(vec![item(4, AttentionStatus::NeedsYou, "x", 1)]),
            Instant::now(),
        );
        let id = engine::NotifyId::Item(engine::ItemKey {
            session: "b".to_owned(),
            pane: PaneId::Terminal(4),
        });
        assert_eq!(
            background.on_notification(SinkEvent::Clicked {
                id: id.clone(),
                token: Some("t".to_owned())
            }),
            vec![Intent::Route {
                session: "b".to_owned(),
                pane: Some(PaneId::Terminal(4)),
                token: Some("t".to_owned())
            }]
        );
        assert!(background
            .on_notification(SinkEvent::Dismissed(id))
            .is_empty());
    }

    #[test]
    fn a_clicked_notification_is_withdrawn_and_not_shown_again_for_the_same_state() {
        let (mut background, recorder) = background();
        let now = Instant::now();
        let items = || snapshot(vec![item(4, AttentionStatus::NeedsYou, "x", 1)]);
        background.on_snapshot(snapshot(vec![]), now);
        background.on_snapshot(items(), now);
        background.tick(now + engine::FIRST_SHOW_DELAY);
        let shown = wait_for(&recorder, 1);
        let id = match &shown[..] {
            [Recorded::Show(id, _)] => id.clone(),
            other => panic!("expected one notification, got {:?}", other),
        };
        background.on_notification(SinkEvent::Clicked {
            id: id.clone(),
            token: None,
        });
        assert_eq!(wait_for(&recorder, 1), vec![Recorded::Withdraw(id)]);
        let later = now + engine::FIRST_SHOW_DELAY * 4;
        background.on_snapshot(items(), later);
        background.tick(later + engine::FIRST_SHOW_DELAY);
        std::thread::sleep(Duration::from_millis(100));
        assert_eq!(
            recorder.taken(),
            vec![],
            "the same unchanged item is not notified again"
        );
    }

    #[test]
    fn the_tray_menu_turns_into_intents_and_pause_dims_the_icon() {
        let (mut background, _) = background();
        let now = Instant::now();
        assert_eq!(
            background.on_tray(
                TrayEvent::Activate {
                    at: Some((3, 4)),
                    token: None
                },
                now
            ),
            vec![Intent::Popup {
                view: None,
                at: Some((3, 4)),
                token: None
            }]
        );
        assert_eq!(
            background.on_tray(TrayEvent::Menu(MenuAction::NewWindow), now),
            vec![Intent::Open {
                request: OpenWindowRequest::default(),
                token: None
            }]
        );
        assert_eq!(
            background.on_tray(TrayEvent::Menu(MenuAction::QuitAll), now),
            vec![Intent::ConfirmQuitAll]
        );
        background.on_tray(TrayEvent::Menu(MenuAction::PauseHour), now);
        assert!(background.policy().paused);
        assert!(background.tray_state().look.dimmed);
        assert!(background.deadline().is_some());
        background.on_tray(TrayEvent::Menu(MenuAction::Resume), now);
        assert!(!background.policy().paused);
    }

    #[test]
    fn hiding_the_icon_never_stops_notifications() {
        let (mut background, recorder) = background();
        let now = Instant::now();
        assert!(background.tray_visible());
        background.on_tray(TrayEvent::Menu(MenuAction::HideUntilRestart), now);
        assert!(!background.tray_visible());
        background.on_request(AppRequest::ToggleTrayIcon);
        assert!(background.tray_visible());
        background.on_request(AppRequest::ToggleTrayIcon);
        assert!(!background.tray_visible());
        background.on_snapshot(snapshot(vec![]), now);
        background.on_snapshot(
            snapshot(vec![item(1, AttentionStatus::NeedsYou, "x", 1)]),
            now,
        );
        background.tick(now + engine::FIRST_SHOW_DELAY);
        assert!(matches!(wait_for(&recorder, 1)[..], [Recorded::Show(..)]));
    }

    #[test]
    fn the_close_notice_is_sent_once() {
        let (mut background, recorder) = background();
        background.window_closed_by_user();
        background.window_closed_by_user();
        let seen = wait_for(&recorder, 2);
        assert_eq!(
            seen,
            vec![Recorded::Simple(
                "Zellij".to_owned(),
                "Closing this window keeps your sessions running. Find them in the tray."
                    .to_owned()
            )]
        );
    }

    #[test]
    fn a_second_launch_naming_a_session_routes_and_a_plain_one_opens() {
        let (reply, intents) = request_intents(AppRequest::OpenWindow(OpenWindowRequest {
            session: Some("api".to_owned()),
            ..Default::default()
        }));
        assert_eq!(reply, AppReply::Done);
        assert_eq!(
            intents,
            vec![Intent::Route {
                session: "api".to_owned(),
                pane: None,
                token: None
            }]
        );
        let (_, intents) = request_intents(AppRequest::OpenWindow(Default::default()));
        assert!(matches!(intents[..], [Intent::Open { .. }]));
        let (_, intents) = request_intents(AppRequest::Ping);
        assert!(intents.is_empty());
    }

    #[test]
    fn a_panel_shortcut_toggles_its_own_view_and_switches_another() {
        use PopupView::*;
        assert_eq!(panel_toggle(None, false, Inbox, None), PanelToggle::Open);
        assert_eq!(
            panel_toggle(Some(Inbox), false, Inbox, None),
            PanelToggle::Close
        );
        assert_eq!(
            panel_toggle(Some(Sessions), false, Inbox, None),
            PanelToggle::Switch
        );
        assert_eq!(panel_toggle(None, true, Inbox, None), PanelToggle::Ignore);
        assert_eq!(
            panel_toggle(None, false, Inbox, Some(Inbox)),
            PanelToggle::Ignore,
            "the shortcut that took focus from the panel closes it rather than reopening it"
        );
        assert_eq!(
            panel_toggle(None, false, Inbox, Some(Sessions)),
            PanelToggle::Open
        );
        assert_eq!(
            request_intents(AppRequest::TogglePanel {
                view: Sessions,
                token: Some("t".to_owned())
            })
            .1,
            vec![Intent::TogglePanel {
                view: Sessions,
                token: Some("t".to_owned())
            }]
        );
    }

    #[test]
    fn the_launch_token_is_taken_once_and_kept_from_child_processes() {
        std::env::set_var("XDG_ACTIVATION_TOKEN", "wayland-token");
        std::env::set_var("DESKTOP_STARTUP_ID", "x11-id");
        assert_eq!(launch_token().as_deref(), Some("wayland-token"));
        assert!(std::env::var_os("XDG_ACTIVATION_TOKEN").is_none());
        assert!(std::env::var_os("DESKTOP_STARTUP_ID").is_none());
        assert_eq!(launch_token(), None);
    }

    #[test]
    fn hiding_permanently_writes_the_setting_into_the_config_file() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("config.kdl");
        std::fs::write(&path, "default_mode \"locked\"\n").unwrap();
        hide_icon_permanently(Some(&path)).unwrap();
        let config = attention_config(Some(&path));
        assert_eq!(config.tray_icon, Some(false));
        let written = std::fs::read_to_string(&path).unwrap();
        assert!(written.contains("default_mode \"locked\""), "{}", written);
        assert!(hide_icon_permanently(None).is_err());
    }

    #[test]
    fn tomorrow_is_in_the_future_and_within_a_day() {
        let until = tomorrow();
        let left = until.duration_since(SystemTime::now()).unwrap();
        assert!(left <= Duration::from_secs(25 * 3600));
    }
}
