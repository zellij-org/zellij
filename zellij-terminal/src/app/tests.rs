use super::*;
use crate::app::desktop::notifier;
use crate::app::desktop::notifier::recording::{Recorded, Recorder};
use crate::app::policy::model::fixtures::*;
use zellij_utils::attention::AttentionStatus;

fn background() -> (Background, Recorder) {
    let recorder = Recorder::default();
    let sink = recorder.clone();
    let notifier = Notifier::spawn_with(move || Some(Box::new(sink) as Box<dyn notifier::Sink>));
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
fn with_attention_off_every_way_to_the_inbox_shows_the_sessions() {
    let (mut background, _) = background();
    let attention_off = AttentionConfig {
        enabled: Some(false),
        ..Default::default()
    };
    background.on_config(attention_off, Instant::now());
    assert_eq!(
        background.on_request(AppRequest::TogglePanel {
            view: PopupView::Inbox,
            token: None
        }),
        vec![Intent::TogglePanel {
            view: PopupView::Sessions,
            token: None
        }]
    );
    assert_eq!(
        background.on_request(AppRequest::ShowPopup(PopupView::Inbox)),
        vec![Intent::Popup {
            view: Some(PopupView::Sessions),
            at: None,
            token: None
        }]
    );
    assert_eq!(
        background.on_tray(TrayEvent::Menu(MenuAction::ShowInbox), Instant::now()),
        vec![Intent::Popup {
            view: Some(PopupView::Sessions),
            at: None,
            token: None
        }]
    );
    assert_eq!(
        background.adjust_intent(Intent::Popup {
            view: Some(PopupView::Quit),
            at: None,
            token: None
        }),
        Intent::Popup {
            view: Some(PopupView::Quit),
            at: None,
            token: None
        }
    );
    assert!(!background.menu_state().attention);
}

#[test]
fn with_attention_on_the_inbox_is_shown() {
    let (mut background, _) = background();
    assert_eq!(
        background.on_request(AppRequest::ShowPopup(PopupView::Inbox)),
        vec![Intent::Popup {
            view: Some(PopupView::Inbox),
            at: None,
            token: None
        }]
    );
    assert!(background.menu_state().attention);
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
            "Closing this window keeps your sessions running. Find them in the tray.".to_owned()
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
fn the_notifications_menu_writes_its_choices_into_the_config_file() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("config.kdl");
    std::fs::write(&path, "default_mode \"locked\"\n").unwrap();
    let (mut background, _) = background();
    background.config_path = Some(path.clone());
    let now = Instant::now();
    background.on_tray(
        TrayEvent::Menu(MenuAction::TogglePopup(AttentionStatus::Done)),
        now,
    );
    background.on_tray(
        TrayEvent::Menu(MenuAction::ToggleBadge(AttentionStatus::Quiet)),
        now,
    );
    background.on_tray(
        TrayEvent::Menu(MenuAction::BadgeColor(
            AttentionStatus::Failed,
            Some([1, 2, 3]),
        )),
        now,
    );
    assert!(!background.policy().statuses.includes(AttentionStatus::Done));
    assert!(background
        .menu_state()
        .badge
        .statuses
        .includes(AttentionStatus::Quiet));
    let config = attention_config(Some(&path));
    assert!(!config
        .desktop_notifications()
        .includes(AttentionStatus::Done));
    assert!(config
        .desktop_notifications()
        .includes(AttentionStatus::Failed));
    assert!(config.tray_badge().includes(AttentionStatus::Quiet));
    assert_eq!(
        config.tray_badge_color(AttentionStatus::Failed),
        Some([1, 2, 3])
    );
    background.on_tray(
        TrayEvent::Menu(MenuAction::BadgeColor(AttentionStatus::Failed, None)),
        now,
    );
    let config = attention_config(Some(&path));
    assert_eq!(config.tray_badge_color(AttentionStatus::Failed), None);
    let written = std::fs::read_to_string(&path).unwrap();
    assert!(written.contains("default_mode \"locked\""), "{}", written);
}

#[test]
fn tomorrow_is_in_the_future_and_within_a_day() {
    let until = tomorrow();
    let left = until.duration_since(SystemTime::now()).unwrap();
    assert!(left <= Duration::from_secs(25 * 3600));
}
