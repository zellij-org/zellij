pub mod control;
pub mod desktop;
pub mod policy;
pub mod probe;

use control::instance;
use control::standalone;
use desktop::tray;
use policy::engine;
use policy::state;

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use zellij_utils::app_control::{AppReply, AppRequest, OpenWindowRequest, PopupView};
use zellij_utils::data::PaneId;
use zellij_utils::input::attention::AttentionConfig;
use zellij_utils::input::config::Config;

use crate::app::control::watcher::{Snapshot, Watcher};
use crate::app::desktop::notifier::{Notifier, SinkEvent};
use crate::app::desktop::tray::{MenuAction, Tray, TrayEvent};
use crate::app::policy::engine::{Engine, Policy};
use crate::app::policy::model::{tray_state, BadgeSettings, MenuState, TrayState};
use crate::app::policy::state::CloseNotice;

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
        result: Result<crate::session::connection::Connection, String>,
    },
    Terminate,
    Describe(std::sync::mpsc::Sender<zellij_utils::app_control::AppState>),
    RaiseWindows {
        pane: Option<PaneId>,
    },
    DetachWindows,
    Handled {
        ticket: u64,
    },
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
    DetachWindows,
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

pub fn save_setting(
    path: Option<&Path>,
    key: zellij_utils::data::SettingKey,
    value: Option<String>,
) -> Result<(), String> {
    use zellij_utils::input::config_file_edit::{apply_edits, read_config_file, ConfigEdit};
    let path = path.ok_or_else(|| "there is no configuration file to write to".to_owned())?;
    let edit = match value.as_deref().and_then(|value| key.kdl_values(value)) {
        Some(values) => ConfigEdit::SetSetting { key, values },
        None => ConfigEdit::RemoveSetting(key),
    };
    let current = read_config_file(path).map_err(|e| e.to_string())?;
    let written = apply_edits(current.as_deref(), &[edit], &Config::default())?;
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
            _main_menu: None,
        }
    }

    pub fn start_on_main_thread(&mut self) {
        if let Some(events) = self.tray_events.clone() {
            #[cfg(target_os = "macos")]
            tray::install_reopen_handler(events.clone());
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
            done_after_ms: self.config.done_threshold_secs() * 1000,
            details: self.config.notification_details(),
            paused: self.paused(),
            do_not_disturb: false,
        }
    }

    pub fn tray_visible(&self) -> bool {
        self.tray_override
            .unwrap_or(self.config.tray_icon() && !self.hidden_until_restart)
    }

    fn menu_state(&self) -> MenuState {
        MenuState {
            paused: self.paused(),
            popups: self.config.desktop_notifications(),
            badge: BadgeSettings::of(&self.config),
            attention: self.config.enabled(),
        }
    }

    pub fn adjust_intent(&self, intent: Intent) -> Intent {
        if self.config.enabled() {
            return intent;
        }
        let without_inbox = |view: PopupView| match view {
            PopupView::Inbox => PopupView::Sessions,
            view => view,
        };
        match intent {
            Intent::Popup { view, at, token } => Intent::Popup {
                view: view.map(without_inbox),
                at,
                token,
            },
            Intent::TogglePanel { view, token } => Intent::TogglePanel {
                view: without_inbox(view),
                token,
            },
            intent => intent,
        }
    }

    fn tray_state(&self) -> TrayState {
        tray_state(&self.snapshot.global, self.menu_state())
    }

    fn save(&self, key: Option<zellij_utils::data::SettingKey>, value: Option<String>) {
        let Some(key) = key else {
            return;
        };
        if let Err(e) = save_setting(self.config_path.as_deref(), key, value) {
            report!("the notification setting could not be saved: {}", e);
        }
    }

    fn settings_changed(&mut self, now: Instant) {
        let policy = self.policy();
        let commands = self.engine.set_policy(policy, now);
        self.notifier.apply(commands);
        self.refresh_tray();
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
        request_intents(request)
            .1
            .into_iter()
            .map(|intent| self.adjust_intent(intent))
            .collect()
    }

    pub fn on_tray(&mut self, event: TrayEvent, now: Instant) -> Vec<Intent> {
        match event {
            TrayEvent::Activate { at, token } => vec![Intent::Popup {
                view: None,
                at,
                token,
            }],
            TrayEvent::Menu(action) => self
                .on_menu(action, now)
                .into_iter()
                .map(|intent| self.adjust_intent(intent))
                .collect(),
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
            MenuAction::DetachAllWindows => {
                standalone::detach_all_soon();
                vec![Intent::DetachWindows]
            },
            MenuAction::DetachEverywhere => {
                standalone::detach_all_soon();
                detach_everywhere(self.live_sessions());
                vec![]
            },
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
            MenuAction::TogglePopup(status) => {
                use zellij_utils::input::config_settings::popup_setting_key;
                let on = !self.config.desktop_notifications().includes(status);
                self.config.desktop_notifications.set(status, Some(on));
                self.save(popup_setting_key(status), Some(on.to_string()));
                self.settings_changed(now);
                vec![]
            },
            MenuAction::ToggleBadge(status) => {
                use zellij_utils::input::config_settings::badge_setting_key;
                let on = !self.config.tray_badge().includes(status);
                self.config.tray_badge.set(status, Some(on));
                self.save(badge_setting_key(status), Some(on.to_string()));
                self.settings_changed(now);
                vec![]
            },
            MenuAction::BadgeColor(status, colour) => {
                use zellij_utils::input::attention::colour_text;
                use zellij_utils::input::config_settings::badge_colour_setting_key;
                self.config.tray_badge_colors.set(status, colour);
                self.save(badge_colour_setting_key(status), colour.map(colour_text));
                self.settings_changed(now);
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
                    this_session: false,
                };
                if let Err(e) =
                    zellij_utils::session_link::send_actions_to_session(&name, vec![action])
                {
                    report!("the items of {} could not be cleared: {}", name, e);
                }
            }
        });
}

pub fn detach_everywhere(names: Vec<String>) {
    let _ = std::thread::Builder::new()
        .name("app_detach_everywhere".to_owned())
        .spawn(move || {
            for name in names
                .into_iter()
                .filter(|name| !zellij_utils::consts::is_app_socket_name(name))
            {
                if let Err(e) = zellij_utils::session_link::detach_all_clients(&name) {
                    report!("the clients of {} could not be detached: {}", name, e);
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
mod tests;
