use super::*;

use winit::dpi::PhysicalPosition;
use winit::event::{MouseButton, MouseScrollDelta};
use winit::keyboard::{Key, NamedKey, SmolStr};
use zellij_utils::input::actions::Action;
use zellij_utils::input::mouse::MouseEventType;
use zellij_utils::input::options::OnForceClose;
use zellij_utils::ipc::{ClientToServerMsg, ExitReason};

use std::sync::atomic::{AtomicUsize, Ordering};

use crate::config::window_state;
use crate::config::window_state::WindowState;
use crate::emulation::color::Paints;
use crate::input::clipboard::Clipboard;
use crate::session::connection::{test_attach_at as attach_at, Capabilities, Connection};
use crate::testing::test_server::FakeServer;
use crate::text::font::test_helpers::pixels_to_points;
use crate::text::font::{FontOptions, DEFAULT_FONT_SIZE, DEFAULT_LIGATURES};
use crate::window::attributes::best_config;
use crate::window::run::guarded;
use crate::window::run::Rendering;
use crate::window::scene::SceneInputs;
use crate::window::scene::Transparency;
use anyhow::anyhow;
use winit::event::ElementState;
use winit::event::Ime;
use winit::event::TouchPhase;
use winit::window::CursorIcon;
use winit::window::WindowAttributes;
use zellij_utils::input::terminal::OptionAsAlt;
use zellij_utils::input::terminal::PaddingColor;
use zellij_utils::input::terminal::StartupMode;
use zellij_utils::input::terminal::{TerminalConfig, TerminalTheme};

const HANDSHAKE: usize = 8;

fn geometry() -> Geometry {
    Geometry {
        rows: 40,
        cols: 120,
        cell_width: 10,
        cell_height: 20,
    }
}

fn test_options(intercept_paste: bool) -> Options {
    Options {
        font: FontOptions {
            family: None,
            size: DEFAULT_FONT_SIZE,
            system_fonts: false,
            ligatures: DEFAULT_LIGATURES,
            ..FontOptions::default()
        },
        paste_keys: if intercept_paste {
            crate::config::options::default_paste_keys()
        } else {
            Vec::new()
        },
        zoom_in_keys: crate::config::options::default_zoom_in_keys(),
        zoom_out_keys: crate::config::options::default_zoom_out_keys(),
        zoom_reset_keys: crate::config::options::default_zoom_reset_keys(),
        middle_click_paste: true,
        open_links: true,
        open_links_with_shift: true,
        shift_drag_selects: true,
        copy_keys: crate::config::options::default_copy_keys(),
        bell: zellij_utils::input::terminal::BellMode::Visual,
        paints: Paints::default(),
        cursor_shape: None,
        cursor_blink: None,
        startup_mode: StartupMode::Windowed,
        transparency: Transparency::OPAQUE,
        blur: false,
        padding: crate::config::options::Padding::default(),
        padding_balance: false,
        padding_color: PaddingColor::Background,
        initial_cols: None,
        initial_rows: None,
        fullscreen_keys: crate::config::options::default_fullscreen_keys(),
        hide_pointer_while_typing: false,
        cursor_unfocused_hollow: true,
        minimum_contrast: 1.0,
        smooth_scrolling: true,
        scroll_animation: Duration::from_millis(100),
        scroll_momentum: true,
        scroll_momentum_friction: 2.0,
        option_as_alt: OptionAsAlt::Left,
        reuse_session_windows: true,
        allow_remote_clipboard_read: false,
    }
}

struct Harness {
    app: App,
    server: FakeServer,
    clipboard: ClipboardHandle,
    _connection: Connection,
}

impl Harness {
    fn new(expected: usize, intercept_paste: bool, clipboard_text: &str) -> Self {
        Self::with(expected, test_options(intercept_paste), clipboard_text)
    }

    fn with(expected: usize, options: Options, clipboard_text: &str) -> Self {
        let total = HANDSHAKE + expected;
        let server = FakeServer::spawn(move |side| side.expect(total));
        let connection = attach_at(
            &server.path,
            "app-test",
            geometry(),
            Capabilities::default(),
        )
        .unwrap();

        let mut owned = Clipboard::in_memory();
        if !clipboard_text.is_empty() {
            owned.set(clipboard_text);
        }
        let clipboard: ClipboardHandle = Arc::new(Mutex::new(owned));

        let fonts = options.font.stack(1.0).unwrap();
        let startup = Startup::windowed(geometry().rows, geometry().cols);
        let app = Rendering::bring_up(&options, fonts, startup).into_app(
            Some(connection.sender.clone()),
            connection.role,
            connection.geometry.clone(),
            "test".to_owned(),
            settings(TerminalConfig::default()),
            clipboard.clone(),
            None,
        );

        Self {
            app,
            server,
            clipboard,
            _connection: connection,
        }
    }

    fn clipboard_text(&self) -> Option<String> {
        self.clipboard.lock().unwrap().get()
    }

    fn press(&mut self, logical: &Key) {
        let modifiers = self.app.modifiers;
        self.app
            .press(keyboard::Press::new(logical.clone(), modifiers));
    }

    fn set_primary(&mut self, text: &str) {
        self.clipboard.lock().unwrap().set_primary(text);
    }

    fn sent(self) -> Vec<ClientToServerMsg> {
        let mut received = self.server.finish();
        received.split_off(HANDSHAKE)
    }

    fn actions(self) -> Vec<Action> {
        self.sent()
            .into_iter()
            .filter_map(|msg| match msg {
                ClientToServerMsg::Action { action, .. } => Some(action),
                other => panic!("expected an action, got {:?}", other),
            })
            .collect()
    }
}

fn character(text: &str) -> Key {
    Key::Character(SmolStr::new(text))
}

fn at(x: f64, y: f64) -> PhysicalPosition<f64> {
    PhysicalPosition::new(x, y)
}

fn margins(left: u32, top: u32, right: u32, bottom: u32) -> Margins {
    Margins {
        left,
        top,
        right,
        bottom,
    }
}

#[test]
fn the_panel_border_is_one_line_on_each_edge_scaled_with_the_display() {
    let ink = [1, 2, 3];
    let rect = |x: i32, y: i32, width: u32, height: u32| scene::Rect {
        x,
        y,
        width,
        height,
        color: ink,
    };
    assert_eq!(
        border_rects(ink, 1.0, (100, 50), (0, 0)),
        [
            rect(0, 0, 100, 1),
            rect(0, 49, 100, 1),
            rect(0, 0, 1, 50),
            rect(99, 0, 1, 50),
        ]
    );
    assert_eq!(
        border_rects(ink, 2.0, (100, 50), (0, 0)),
        [
            rect(0, 0, 100, 2),
            rect(0, 48, 100, 2),
            rect(0, 0, 2, 50),
            rect(98, 0, 2, 50),
        ]
    );
    assert_eq!(
        border_rects(ink, 0.5, (10, 10), (0, 0))[1],
        rect(0, 9, 10, 1)
    );
    assert_eq!(
        border_rects(ink, 1.5, (10, 10), (0, 0))[3],
        rect(8, 0, 2, 10)
    );
    assert_eq!(
        border_rects(ink, 1.0, (100, 50), (8, 10)),
        [
            rect(-8, -10, 100, 1),
            rect(-8, 39, 100, 1),
            rect(-8, -10, 1, 50),
            rect(91, -10, 1, 50),
        ],
        "the rectangles are drawn relative to the grid, so they are moved back to the window edges"
    );
}
#[test]
fn without_padding_the_grid_starts_at_the_corner_and_leftovers_go_right_and_down() {
    let fit = fit_grid((805, 413), (8, 20), Margins::default(), false);
    assert_eq!((fit.cols, fit.rows, fit.origin), (100, 20, (0, 0)));
}

#[test]
fn padding_is_taken_off_the_window_before_the_grid_is_fitted() {
    let fit = fit_grid((820, 430), (8, 20), margins(10, 5, 10, 5), false);
    assert_eq!((fit.cols, fit.rows), (100, 21));
    assert_eq!(fit.origin, (10, 5));
    let fit = fit_grid((819, 430), (8, 20), margins(10, 5, 10, 6), false);
    assert_eq!((fit.cols, fit.rows), (99, 20));
    assert_eq!(fit.origin, (10, 5));
}

#[test]
fn balanced_padding_shares_the_leftover_pixels_on_both_sides() {
    let fit = fit_grid((827, 437), (8, 20), margins(10, 5, 10, 5), true);
    assert_eq!((fit.cols, fit.rows), (100, 21));
    assert_eq!(fit.origin, (10 + 3, 5 + 3));
    let unbalanced = fit_grid((827, 437), (8, 20), margins(10, 5, 10, 5), false);
    assert_eq!(unbalanced.origin, (10, 5));
}

#[test]
fn a_window_too_small_for_padding_and_a_cell_still_holds_one_cell_at_the_padding() {
    for balance in [false, true] {
        let fit = fit_grid((15, 12), (8, 20), margins(10, 10, 10, 10), balance);
        assert_eq!((fit.cols, fit.rows), (1, 1));
        assert_eq!(fit.origin, (10, 10));
        let fit = fit_grid((0, 0), (8, 20), margins(4, 4, 4, 4), balance);
        assert_eq!((fit.cols, fit.rows, fit.origin), (1, 1, (4, 4)));
    }
}

#[test]
fn a_reflow_with_padding_moves_the_pointer_and_reports_the_smaller_grid() {
    let mut options = test_options(true);
    options.padding = crate::config::options::Padding {
        top: 10.0,
        right: 10.0,
        bottom: 10.0,
        left: 20.0,
    };
    let mut harness = Harness::with(3, options, "");
    harness.app.reflow(350, 420);
    assert_eq!(harness.app.origin, (20, 10));
    let geometry = harness.app.geometry.get();
    assert_eq!((geometry.cols, geometry.rows), (40, 20));
    harness
        .app
        .on_cursor_moved(at(20.0 + 8.0 * 3.0 + 1.0, 10.0 + 20.0 + 1.0));
    assert_eq!(harness.app.mouse.pointer.cell(geometry), (3, 1));
    assert_eq!(
        harness.sent()[0],
        ClientToServerMsg::TerminalResize {
            new_size: zellij_utils::pane_size::Size { rows: 20, cols: 40 },
        }
    );
}

fn padded_harness() -> Harness {
    let mut options = test_options(true);
    options.padding = crate::config::options::Padding {
        top: 3.0,
        right: 5.0,
        bottom: 7.0,
        left: 11.0,
    };
    Harness::with(0, options, "")
}

#[test]
fn the_first_window_asks_for_its_cells_plus_padding_as_a_logical_size() {
    let harness = padded_harness();
    let startup = harness.app.placement.startup;
    let expected = winit::dpi::LogicalSize::new(
        (8 * startup.cols + 11 + 5) as f64,
        (20 * startup.rows + 3 + 7) as f64,
    );
    match harness.app.requested_size() {
        winit::dpi::Size::Logical(size) => assert_eq!(size, expected),
        other => panic!("the startup size must be logical, got {:?}", other),
    }
}

#[test]
fn the_exact_size_holds_the_cells_and_the_padding_at_the_display_scale() {
    let mut harness = padded_harness();
    assert_eq!(
        harness.app.exact_size(100, 30),
        winit::dpi::PhysicalSize::new(8 * 100 + 16, 20 * 30 + 10)
    );
    harness.app.scale = 2.0;
    assert_eq!(
        harness.app.exact_size(100, 30),
        winit::dpi::PhysicalSize::new(8 * 100 + 32, 20 * 30 + 20),
        "padding is logical and must double with the display scale"
    );
    let exact = harness.app.exact_size(100, 30);
    let fitted = harness.app.fitted(exact.width, exact.height);
    assert_eq!((fitted.cols, fitted.rows), (100, 30));
}

#[test]
fn a_window_stranded_by_a_failed_switch_can_still_be_closed() {
    let mut harness = Harness::new(0, true, "");
    harness.app.stranded(anyhow!("no session to switch to"));

    assert!(harness.app.sender.is_none());
    assert!(harness.app.failure.is_some(), "the window exits in failure");
    assert!(
        harness.app.closing(),
        "a stranded window must close on the first request"
    );
    assert!(
        harness.app.state.dump().contains("no session to switch to"),
        "the reason is shown in the window"
    );
    assert!(harness.sent().is_empty());
}

fn shown(harness: &Harness) -> String {
    harness
        .app
        .state
        .dump()
        .lines()
        .map(str::trim)
        .collect::<Vec<_>>()
        .join(" ")
}

#[test]
fn a_session_that_ends_normally_closes_the_window() {
    for reason in [
        ExitReason::Normal,
        ExitReason::NormalDetached,
        ExitReason::CustomExitStatus(0),
    ] {
        let mut harness = Harness::new(0, true, "");
        assert!(
            harness.app.session_ended(Ending::Exited(reason.clone())),
            "{:?} must close the window",
            reason
        );
        assert!(harness.app.failure.is_none(), "{:?}", reason);
    }
}

#[test]
fn a_session_that_lets_the_window_go_for_a_reason_keeps_it_open_with_the_reason() {
    let mut harness = Harness::new(0, true, "");
    assert!(
        !harness
            .app
            .session_ended(Ending::Exited(ExitReason::KickedByHost)),
        "the window stays so the reason can be read"
    );
    assert!(shown(&harness).contains("Disconnected by host"));
    assert!(shown(&harness).contains(crate::window::notice::LEAVE));
    assert!(harness.app.sender.is_none());
    assert!(
        harness.app.failure.is_some(),
        "closing it later is a failure"
    );
    assert!(harness.app.closing(), "the first close request closes it");
}

#[test]
fn a_server_error_is_shown_in_the_window() {
    let mut harness = Harness::new(0, true, "");
    harness.app.session_ended(Ending::Exited(ExitReason::Error(
        "the screen thread failed".to_owned(),
    )));
    let shown = shown(&harness);
    assert!(shown.contains("Error occurred in server:"), "{}", shown);
    assert!(shown.contains("the screen thread failed"), "{}", shown);
}

#[test]
fn a_non_zero_exit_status_keeps_the_window_open_and_fails() {
    let mut harness = Harness::new(0, true, "");
    assert!(!harness
        .app
        .session_ended(Ending::Exited(ExitReason::CustomExitStatus(3))));
    assert!(
        shown(&harness).contains("exit status 3"),
        "{}",
        shown(&harness)
    );
    assert!(harness.app.failure.is_some());
}

#[test]
fn a_disconnect_names_the_session_the_window_was_showing() {
    let mut harness = Harness::new(0, true, "");
    harness.app.session_name = "work".to_owned();
    harness
        .app
        .session_ended(Ending::Exited(ExitReason::Disconnect));
    assert!(
        shown(&harness).contains("`zellij attach work`"),
        "{}",
        shown(&harness)
    );
}

#[test]
fn only_enter_or_escape_dismiss_a_message() {
    let mut harness = Harness::new(0, true, "");
    harness.app.session_ended(Ending::Lost);
    for key in [
        Key::Character(SmolStr::new("q")),
        Key::Named(NamedKey::Space),
        Key::Named(NamedKey::ArrowDown),
        Key::Named(NamedKey::Tab),
    ] {
        assert!(!harness.app.notice_key(&key), "{:?}", key);
    }
    assert!(harness.app.notice_key(&Key::Named(NamedKey::Enter)));
    assert!(harness.app.notice_key(&Key::Named(NamedKey::Escape)));
}

#[test]
fn a_long_message_scrolls_with_keys_and_the_wheel() {
    let mut harness = Harness::new(0, true, "");
    let long = (0..100)
        .map(|n| format!("detail {}", n))
        .collect::<Vec<_>>()
        .join("\n");
    harness.app.stranded(anyhow!("{}", long));
    assert!(shown(&harness).contains("detail 0 "));
    assert!(!shown(&harness).contains("detail 99"));
    harness.app.notice_key(&Key::Named(NamedKey::End));
    assert!(shown(&harness).contains("detail 99"), "{}", shown(&harness));
    assert!(!shown(&harness).contains("detail 0 "));
    harness.app.notice_key(&Key::Named(NamedKey::Home));
    assert!(shown(&harness).contains("detail 0 "));
    harness
        .app
        .on_wheel(MouseScrollDelta::LineDelta(0.0, -1.0), TouchPhase::Moved);
    assert_eq!(harness.app.notice.as_ref().unwrap().top(), 3);
    harness.app.notice_key(&Key::Named(NamedKey::ArrowUp));
    assert_eq!(harness.app.notice.as_ref().unwrap().top(), 2);
}

#[test]
fn a_message_is_redrawn_to_fit_a_resized_window() {
    let mut harness = Harness::new(0, true, "");
    harness.app.session_ended(Ending::Lost);
    harness.app.reflow(8 * 30, 20 * 12);
    let size = harness.app.state.size();
    assert_eq!((size.cols, size.rows), (30, 12));
    let shown = shown(&harness);
    assert!(shown.contains("closed without a"), "{}", shown);
    assert!(shown.contains("server may have crashed."), "{}", shown);
}

#[test]
fn a_connection_dropped_without_a_word_is_explained() {
    let mut harness = Harness::new(0, true, "");
    assert!(!harness.app.session_ended(Ending::Lost));
    assert!(
        shown(&harness).contains("closed without a reason"),
        "{}",
        shown(&harness)
    );
}

#[test]
fn a_failure_on_the_window_side_is_shown_in_the_window() {
    let mut harness = Harness::new(0, true, "");
    assert!(!harness.app.session_ended(Ending::Failed(
        "this session's server predates it".to_owned()
    )));
    assert!(shown(&harness).contains("this session's server predates it"));
}

#[test]
fn a_watcher_that_asked_to_leave_closes_however_the_session_ends() {
    let mut harness = Harness::new(1, true, "");
    harness.app.role = Role::Watcher;
    assert!(harness.app.closing());
    assert!(harness.app.session_ended(Ending::Lost));
    assert!(harness.app.failure.is_none());
}

#[test]
fn a_window_that_was_closed_is_not_troubled_by_the_detach_that_follows() {
    let mut harness = Harness::new(1, true, "");
    assert!(harness.app.closing());
    assert!(harness
        .app
        .session_ended(Ending::Exited(ExitReason::NormalDetached)));
    assert!(harness.app.failure.is_none());
    assert_eq!(harness.actions(), vec![Action::Detach]);
}

#[test]
fn a_window_showing_an_error_stays_open_when_the_session_lets_it_go() {
    let mut harness = Harness::new(0, true, "");
    harness.app.session_ended(Ending::Lost);
    assert!(
        !harness
            .app
            .session_ended(Ending::Exited(ExitReason::NormalDetached)),
        "a later normal ending must not hide the message"
    );
    assert!(shown(&harness).contains("closed without a reason"));
}

#[test]
fn an_unreadable_frame_leaves_the_session_and_says_why() {
    let mut harness = Harness::new(1, true, "");
    harness.app.on_frame(&[0xde, 0xad, 0xbe, 0xef]);
    assert!(harness.app.sender.is_none());
    assert!(harness.app.failure.is_some());
    assert!(
        shown(&harness).contains("cannot read"),
        "{}",
        shown(&harness)
    );
    assert!(
        !harness
            .app
            .session_ended(Ending::Exited(ExitReason::NormalDetached)),
        "the detach that follows must leave the message up"
    );
    assert_eq!(harness.actions(), vec![Action::Detach]);
}

#[test]
fn a_crash_in_the_connection_is_reported_and_becomes_an_error() {
    let mut reported = None;
    let outcome: Result<()> = guarded(
        || panic!("the decoder fell over"),
        |message| reported = Some(message),
    );
    let error = outcome.expect_err("a crash must become an error");
    let reported = reported.expect("the crash must be reported");
    assert!(reported.contains("the decoder fell over"), "{}", reported);
    assert_eq!(error.to_string(), reported);
}

#[test]
fn work_that_does_not_crash_passes_its_outcome_through() {
    let mut reported = false;
    assert_eq!(guarded(|| Ok(7), |_| reported = true).unwrap(), 7);
    assert!(guarded::<()>(|| Err(anyhow!("plain failure")), |_| reported = true).is_err());
    assert!(!reported);
}

#[test]
fn closing_a_window_detaches_and_keeps_the_session_running() {
    let mut harness = Harness::new(1, true, "");
    assert!(
        harness.app.closing(),
        "a window closes at once; the session keeps running"
    );
    assert!(harness.app.leaving);
    assert_eq!(harness.actions(), vec![Action::Detach]);
}

#[test]
fn a_switch_reuses_another_app_window_only_when_allowed() {
    assert_eq!(window_to_reuse(false, true, 1, Some(2)), Some(2));
    assert_eq!(
        window_to_reuse(false, false, 1, Some(2)),
        None,
        "with the setting off the window switches in place"
    );
    assert_eq!(
        window_to_reuse(true, true, 1, Some(2)),
        None,
        "a standalone window always switches in place"
    );
    assert_eq!(
        window_to_reuse(false, true, 1, None),
        None,
        "no other window shows the session"
    );
    assert_eq!(
        window_to_reuse(false, true, 1, Some(1)),
        None,
        "the only window on the session is the switching one"
    );
}

#[test]
fn a_handover_carries_the_pane_tab_and_history_of_the_switch() {
    let history = zellij_utils::session_history::SessionHistory {
        back: vec!["s1".to_owned()],
        forward: vec![],
        last: Some("s1".to_owned()),
    };
    let handover = handover_to(
        5,
        ConnectToSession {
            name: Some("s2".to_owned()),
            tab_position: Some(3),
            pane_id: Some((7, true)),
            switch_context: Some(zellij_utils::session_history::SwitchContext {
                history: history.clone(),
                from_session: Some("s1".to_owned()),
                ..Default::default()
            }),
            ..Default::default()
        },
    );
    assert_eq!(
        handover,
        Handover {
            window: 5,
            tab: Some(3),
            pane: Some(zellij_utils::data::PaneId::Plugin(7)),
            history: Some(history),
        }
    );
    let bare = handover_to(
        5,
        ConnectToSession {
            name: Some("s2".to_owned()),
            pane_id: Some((2, false)),
            ..Default::default()
        },
    );
    assert_eq!(bare.pane, Some(zellij_utils::data::PaneId::Terminal(2)));
    assert_eq!(bare.tab, None);
    assert_eq!(bare.history, None);
}

#[test]
fn a_raised_window_hands_the_adopted_history_to_its_session() {
    let harness = Harness::new(1, true, "");
    let history = zellij_utils::session_history::SessionHistory {
        back: vec!["s1".to_owned()],
        forward: vec![],
        last: Some("s1".to_owned()),
    };
    harness.app.adopt_history(history.clone());
    assert_eq!(
        harness.sent(),
        vec![ClientToServerMsg::AdoptSessionHistory { history }]
    );
}

#[test]
fn a_watcher_leaves_without_quitting() {
    let mut harness = Harness::new(1, true, "");
    harness.app.role = Role::Watcher;
    assert!(harness.app.closing());
    assert!(
        matches!(harness.sent()[0], ClientToServerMsg::Key { .. }),
        "a watcher leaves with Esc as before"
    );
}

#[test]
fn with_no_session_a_window_still_closes_itself() {
    let mut harness = Harness::new(0, true, "");
    harness.app.sender = None;
    assert!(harness.app.closing());
    assert!(harness.sent().is_empty());
}

fn f11() -> Key {
    Key::Named(NamedKey::F11)
}

#[test]
fn the_fullscreen_key_toggles_and_is_not_sent_to_the_session() {
    let mut harness = Harness::new(0, true, "");
    harness.app.placement.shown = Shown::Windowed;
    harness.press(&f11());
    assert_eq!(harness.app.placement.shown, Shown::Fullscreen);
    harness.press(&f11());
    assert_eq!(harness.app.placement.shown, Shown::Windowed);
    assert!(harness.sent().is_empty());
}

#[test]
fn leaving_fullscreen_returns_to_maximized_when_that_is_where_it_came_from() {
    let mut harness = Harness::new(0, true, "");
    harness.app.placement.shown = Shown::Maximized;
    harness.press(&f11());
    assert_eq!(harness.app.placement.shown, Shown::Fullscreen);
    harness.press(&f11());
    assert_eq!(harness.app.placement.shown, Shown::Maximized);
}

#[test]
fn the_windowed_size_survives_a_fullscreen_round_trip() {
    let mut harness = Harness::new(4, true, "");
    let dir = remembering(&mut harness);
    harness.app.resized(720, 600);
    harness.press(&f11());
    harness.app.resized(1600, 1000);
    harness.press(&f11());
    harness.app.remember();
    assert_eq!(
        remembered(&dir),
        Some(WindowState {
            cols: 90,
            rows: 30,
            state: Shown::Windowed,
        })
    );
}

#[test]
fn closing_while_fullscreen_keeps_the_windowed_size_and_reopens_fullscreen() {
    let mut harness = Harness::new(4, true, "");
    let dir = remembering(&mut harness);
    harness.app.resized(720, 600);
    harness.press(&f11());
    harness.app.resized(1600, 1000);
    harness.app.remember();
    let saved = remembered(&dir).expect("the state was saved");
    assert_eq!(
        saved,
        WindowState {
            cols: 90,
            rows: 30,
            state: Shown::Fullscreen,
        }
    );
    let startup = Startup::resolve(StartupMode::Remember, Some(saved), None, None);
    assert_eq!(startup.mode, StartupMode::Fullscreen);
    assert_eq!((startup.cols, startup.rows), (90, 30));
    assert!(
        startup_attributes(WindowAttributes::default(), startup.mode)
            .fullscreen
            .is_some()
    );
}

#[test]
fn closing_in_fullscreen_without_a_windowed_size_this_run_keeps_the_saved_one() {
    let mut harness = Harness::new(0, true, "");
    let dir = remembering(&mut harness);
    window_state::store(
        &dir.path().join("window-state.json"),
        WindowState {
            cols: 70,
            rows: 20,
            state: Shown::Windowed,
        },
    );
    harness.app.placement.windowed_known = false;
    harness.app.placement.shown = Shown::Fullscreen;
    harness.app.remember();
    assert_eq!(
        remembered(&dir),
        Some(WindowState {
            cols: 70,
            rows: 20,
            state: Shown::Fullscreen,
        })
    );
}

fn hiding(expected: usize) -> Harness {
    let mut options = test_options(true);
    options.hide_pointer_while_typing = true;
    Harness::with(expected, options, "")
}

#[test]
fn the_pointer_hides_on_a_sent_key_and_returns_when_it_moves() {
    let mut harness = hiding(2);
    harness.press(&character("a"));
    assert!(harness.app.mouse.pointer_hidden);
    harness.app.on_cursor_moved(at(5.0, 5.0));
    assert!(!harness.app.mouse.pointer_hidden);
}

#[test]
fn a_modifier_alone_does_not_hide_the_pointer() {
    let mut harness = hiding(0);
    for named in [
        NamedKey::Shift,
        NamedKey::Control,
        NamedKey::Alt,
        NamedKey::Super,
    ] {
        harness.press(&Key::Named(named));
    }
    assert!(!harness.app.mouse.pointer_hidden);
    assert!(harness.sent().is_empty());
}

#[test]
fn a_window_key_setting_hides_the_pointer_and_every_pointer_event_shows_it() {
    let mut harness = hiding(1);
    harness.press(&f11());
    assert!(harness.app.mouse.pointer_hidden);
    harness
        .app
        .on_wheel(MouseScrollDelta::LineDelta(0.0, 0.0), TouchPhase::Moved);
    assert!(!harness.app.mouse.pointer_hidden);
    harness.press(&f11());
    assert!(harness.app.mouse.pointer_hidden);
    harness.app.on_focus_lost();
    assert!(!harness.app.mouse.pointer_hidden);
    harness.press(&f11());
    harness
        .app
        .on_mouse_input(MouseButton::Right, ElementState::Pressed);
    assert!(!harness.app.mouse.pointer_hidden);
}

#[test]
fn with_the_setting_off_typing_leaves_the_pointer_alone() {
    let mut harness = Harness::new(1, true, "");
    harness.press(&character("a"));
    assert!(!harness.app.mouse.pointer_hidden);
}

#[test]
fn a_focus_change_turns_the_cursor_hollow_and_redraws_its_row() {
    let mut harness = Harness::new(3, true, "");
    assert!(harness.app.focused, "a new window counts as focused");
    assert!(!harness.app.cursor_options().hollow);
    harness.app.refresh_scene();
    harness.app.set_focused(false);
    assert!(harness.app.cursor_options().hollow);
    harness.app.refresh_scene();
    assert_eq!(
        harness.app.retained.rebuilt_rows(),
        &[harness.app.state.cursor_position().0]
    );
    harness.app.set_focused(true);
    assert!(!harness.app.cursor_options().hollow);
    harness.app.options.cursor_unfocused_hollow = false;
    harness.app.set_focused(false);
    assert!(!harness.app.cursor_options().hollow);
    let reported: Vec<bool> = harness
        .sent()
        .into_iter()
        .filter_map(|msg| match msg {
            ClientToServerMsg::HostTerminalFocusChanged { focused } => Some(focused),
            _ => None,
        })
        .collect();
    assert_eq!(reported, vec![false, true, false]);
}

#[test]
fn a_close_request_with_no_session_left_closes_the_window_itself() {
    let mut harness = Harness::new(0, true, "");
    harness.app.sender = None;
    assert!(
        harness.app.closing(),
        "a window with no session must close on its own"
    );
    assert!(harness.sent().is_empty());
}

#[test]
fn a_key_press_is_forwarded_to_the_pane() {
    let mut harness = Harness::new(1, true, "");
    harness.press(&character("a"));
    assert_eq!(
        harness.sent(),
        vec![ClientToServerMsg::Key {
            key: zellij_utils::data::KeyWithModifier::new(zellij_utils::data::BareKey::Char('a')),
            raw_bytes: b"a".to_vec(),
            is_kitty_keyboard_protocol: true,
        }]
    );
}

#[test]
fn ctrl_shift_v_pastes_the_clipboard_rather_than_the_key() {
    let mut harness = Harness::new(1, true, "copied");
    harness.app.modifiers = ModifiersState::CONTROL | ModifiersState::SHIFT;
    harness.press(&character("V"));
    assert_eq!(
        harness.actions(),
        vec![Action::Paste {
            chars: "copied".to_owned(),
            pane_id: None
        }]
    );
}

#[test]
fn shift_insert_pastes_the_clipboard_too() {
    let mut harness = Harness::new(1, true, "copied");
    harness.app.modifiers = ModifiersState::SHIFT;
    harness.press(&Key::Named(NamedKey::Insert));
    assert_eq!(
        harness.actions(),
        vec![Action::Paste {
            chars: "copied".to_owned(),
            pane_id: None
        }]
    );
}

#[test]
fn with_no_paste_key_the_chord_reaches_the_pane_instead() {
    let mut harness = Harness::new(1, false, "copied");
    harness.app.modifiers = ModifiersState::CONTROL | ModifiersState::SHIFT;
    harness.press(&character("V"));
    match &harness.sent()[0] {
        ClientToServerMsg::Key { raw_bytes, .. } => assert_eq!(raw_bytes, b"\x1b[118;6u"),
        other => panic!("expected a key, got {:?}", other),
    }
}

#[test]
fn an_empty_clipboard_sends_no_paste_at_all() {
    let mut harness = Harness::new(1, true, "");
    harness.app.modifiers = ModifiersState::CONTROL | ModifiersState::SHIFT;
    harness.press(&character("V"));
    harness.app.modifiers = ModifiersState::empty();
    harness.press(&character("a"));
    assert_eq!(harness.sent().len(), 1);
}

#[test]
fn a_dropped_file_is_pasted_as_its_path() {
    let mut harness = Harness::new(1, true, "");
    harness
        .app
        .on_window_event(WindowEvent::DroppedFile("/shots/shot.png".into()));
    assert_eq!(
        harness.actions(),
        vec![Action::Paste {
            chars: "/shots/shot.png ".to_owned(),
            pane_id: None
        }]
    );
}

static RINGS_COUNTED: AtomicUsize = AtomicUsize::new(0);

fn count_a_ring() {
    RINGS_COUNTED.fetch_add(1, Ordering::Relaxed);
}

fn frame_with_sideband(rows: usize, cols: usize, seq: u64, sideband: &[u8]) -> Vec<u8> {
    let mut builder =
        zellij_utils::structured_render::FrameBuilder::new(cols as u16, rows as u16, seq);
    builder.sideband_mut().extend_from_slice(sideband);
    builder.finish()
}

#[test]
fn the_startup_mode_reaches_the_attributes_the_window_is_built_with() {
    let windowed = startup_attributes(WindowAttributes::default(), StartupMode::Windowed);
    assert!(!windowed.maximized);
    assert!(windowed.fullscreen.is_none());

    let maximized = startup_attributes(WindowAttributes::default(), StartupMode::Maximized);
    assert!(maximized.maximized);
    assert!(maximized.fullscreen.is_none());

    let fullscreen = startup_attributes(WindowAttributes::default(), StartupMode::Fullscreen);
    assert!(!fullscreen.maximized);
    assert_eq!(
        fullscreen.fullscreen,
        Some(winit::window::Fullscreen::Borderless(None)),
        "a terminal must never change the display's video mode"
    );

    let remembered = startup_attributes(WindowAttributes::default(), StartupMode::Remember);
    assert!(!remembered.maximized);
    assert!(remembered.fullscreen.is_none());
}

fn remembering(harness: &mut Harness) -> tempfile::TempDir {
    let dir = tempfile::TempDir::new().unwrap();
    harness.app.placement.state_path = Some(dir.path().join("window-state.json"));
    dir
}

fn remembered(dir: &tempfile::TempDir) -> Option<WindowState> {
    window_state::load_from(&dir.path().join("window-state.json"))
}

#[cfg(target_os = "macos")]
#[test]
fn a_window_the_system_destroys_still_remembers_its_size() {
    let mut harness = Harness::new(2, true, "");
    let dir = remembering(&mut harness);
    harness.app.resized(720, 600);
    assert_eq!(
        harness.app.on_window_event(WindowEvent::Destroyed),
        Verdict::Close
    );
    assert_eq!(
        remembered(&dir),
        Some(WindowState {
            cols: 90,
            rows: 30,
            state: Shown::Windowed,
        }),
        "macOS closes every window before the app hears it is quitting"
    );
}

#[cfg(target_os = "macos")]
#[test]
fn a_window_the_system_destroys_lets_its_session_go() {
    let mut harness = Harness::new(1, true, "");
    harness.app.on_window_event(WindowEvent::Destroyed);
    assert_eq!(
        harness.actions(),
        vec![Action::Detach],
        "quitting from the Dock lets every session go, as Cmd+Q does"
    );
}

#[cfg(not(target_os = "macos"))]
#[test]
fn a_destroyed_window_is_not_asked_about_itself_off_macos() {
    let mut harness = Harness::new(0, true, "");
    let dir = remembering(&mut harness);
    assert_eq!(
        harness.app.on_window_event(WindowEvent::Destroyed),
        Verdict::Close
    );
    assert_eq!(remembered(&dir), None);
    assert_eq!(harness.actions(), vec![]);
}

#[test]
fn cmd_w_closes_the_panel_on_macos_only() {
    let closes = |platform, modifiers, key: &str| {
        closes_the_panel(platform, modifiers, &Key::Character(key.into()))
    };
    assert!(closes(Platform::MacOs, ModifiersState::SUPER, "w"));
    assert!(closes(Platform::MacOs, ModifiersState::SUPER, "W"));
    assert!(!closes(Platform::MacOs, ModifiersState::empty(), "w"));
    let cmd_ctrl = ModifiersState::SUPER | ModifiersState::CONTROL;
    assert!(!closes(Platform::MacOs, cmd_ctrl, "w"));
    let cmd_shift = ModifiersState::SUPER | ModifiersState::SHIFT;
    assert!(!closes(Platform::MacOs, cmd_shift, "W"));
    let cmd_option = ModifiersState::SUPER | ModifiersState::ALT;
    assert!(!closes(Platform::MacOs, cmd_option, "w"));
    assert!(!closes(Platform::MacOs, ModifiersState::SUPER, "q"));
    assert!(!closes(Platform::Linux, ModifiersState::SUPER, "w"));
    assert!(!closes(Platform::Windows, ModifiersState::SUPER, "w"));
}

#[test]
fn a_window_already_leaving_is_not_remembered_again_when_destroyed() {
    let mut harness = Harness::new(2, true, "");
    let dir = remembering(&mut harness);
    harness.app.resized(720, 600);
    harness.app.leave();
    let left = remembered(&dir);
    harness.app.resized(800, 640);
    harness.app.on_window_event(WindowEvent::Destroyed);
    assert_eq!(
        remembered(&dir),
        left,
        "Cmd+Q remembers every window in turn; macOS then closes them all, in its own order"
    );
}

#[test]
fn closing_after_a_resize_remembers_the_new_size_in_cells() {
    let mut harness = Harness::new(3, true, "");
    let dir = remembering(&mut harness);
    harness.app.resized(720, 600);
    assert!(
        harness.app.close_requested(),
        "a window closes at once and leaves the session running"
    );
    assert_eq!(
        remembered(&dir),
        Some(WindowState {
            cols: 90,
            rows: 30,
            state: Shown::Windowed,
        })
    );
    let sent = harness.sent();
    assert_eq!(
        sent[0],
        ClientToServerMsg::TerminalResize {
            new_size: zellij_utils::pane_size::Size { rows: 30, cols: 90 },
        }
    );
    assert!(matches!(
        sent[2],
        ClientToServerMsg::Action {
            action: Action::Detach,
            ..
        }
    ));
}

#[test]
fn closing_the_window_quits_the_session_when_on_force_close_is_quit() {
    let mut harness = Harness::new(1, true, "");
    harness.app.settings.on_force_close = Some(OnForceClose::Quit);
    assert!(harness.app.close_requested());
    assert_eq!(
        harness.actions(),
        vec![Action::SkipConfirm {
            action: Box::new(Action::Quit)
        }]
    );
}

#[test]
fn a_watcher_closing_the_window_only_stops_watching_even_when_on_force_close_is_quit() {
    let mut harness = Harness::new(1, true, "");
    harness.app.role = Role::Watcher;
    harness.app.settings.on_force_close = Some(OnForceClose::Quit);
    assert!(harness.app.close_requested());
    assert!(matches!(harness.sent()[0], ClientToServerMsg::Key { .. }));
}

#[test]
fn a_maximized_close_keeps_the_windowed_size_already_saved() {
    let mut harness = Harness::new(0, true, "");
    let dir = remembering(&mut harness);
    window_state::save_to(
        harness.app.placement.state_path.as_deref().unwrap(),
        WindowState {
            cols: 70,
            rows: 20,
            state: Shown::Windowed,
        },
    )
    .unwrap();
    harness.app.placement.shown = Shown::Maximized;
    harness.app.placement.windowed_known = false;
    harness.app.remember();
    assert_eq!(
        remembered(&dir),
        Some(WindowState {
            cols: 70,
            rows: 20,
            state: Shown::Maximized,
        })
    );
}

#[test]
fn a_window_without_area_reports_no_size_to_the_session() {
    let mut harness = Harness::new(3, true, "");
    harness.app.resized(720, 600);
    harness.app.resized(0, 0);
    harness.app.resized(720, 0);
    harness.app.relayout(0, 0);
    harness.app.resized(720, 600);
    assert!(harness.app.close_requested());
    let sent = harness.sent();
    assert_eq!(
        sent[0],
        ClientToServerMsg::TerminalResize {
            new_size: zellij_utils::pane_size::Size { rows: 30, cols: 90 },
        }
    );
    assert!(
        matches!(
            sent[2],
            ClientToServerMsg::Action {
                action: Action::Detach,
                ..
            }
        ),
        "a window with no area resized the session: {:?}",
        sent
    );
}

#[test]
fn a_window_without_area_keeps_the_windowed_size_it_remembers() {
    let mut harness = Harness::new(2, true, "");
    let dir = remembering(&mut harness);
    harness.app.resized(720, 600);
    harness.app.resized(0, 0);
    harness.app.remember();
    assert_eq!(
        remembered(&dir),
        Some(WindowState {
            cols: 90,
            rows: 30,
            state: Shown::Windowed,
        }),
        "the size a minimized window reports was remembered as its windowed size"
    );
}

#[cfg(windows)]
#[test]
fn a_resize_keeps_what_a_windows_session_end_would_save_up_to_date() {
    let mut harness = Harness::new(2, true, "");
    let dir = remembering(&mut harness);
    harness.app.resized(720, 600);
    let shared = harness
        .app
        .placement
        .remembered
        .shared
        .lock()
        .unwrap()
        .clone()
        .expect("a resize shared nothing to save at a session end");
    assert_eq!(shared.path, dir.path().join("window-state.json"));
    assert_eq!(
        shared.state,
        WindowState {
            cols: 90,
            rows: 30,
            state: Shown::Windowed,
        }
    );
    assert!(shared.windowed_known);
}

#[test]
fn without_a_state_path_nothing_is_remembered() {
    let mut harness = Harness::new(0, true, "");
    let dir = tempfile::TempDir::new().unwrap();
    harness.app.remember();
    assert!(std::fs::read_dir(dir.path()).unwrap().next().is_none());
    assert!(harness.app.placement.state_path.is_none());
}

#[test]
fn a_frame_carrying_osc_52_in_its_sideband_reaches_the_clipboard() {
    let mut harness = Harness::new(1, true, "");
    let size = harness.app.state.size();
    let frame = frame_with_sideband(size.rows, size.cols, 0, b"\x1b]52;c;Y29waWVk\x1b\\");
    harness.app.on_frame(&frame);
    assert_eq!(harness.clipboard_text().as_deref(), Some("copied"));
}

#[test]
fn a_frame_without_osc_52_leaves_the_clipboard_alone() {
    let mut harness = Harness::new(1, true, "kept");
    let size = harness.app.state.size();
    let frame = frame_with_sideband(size.rows, size.cols, 0, b"");
    harness.app.on_frame(&frame);
    assert_eq!(harness.clipboard_text().as_deref(), Some("kept"));
}

#[test]
fn an_applied_frame_is_acknowledged_by_its_sequence_number() {
    let mut harness = Harness::new(1, true, "");
    let size = harness.app.state.size();
    let frame = frame_with_sideband(size.rows, size.cols, 17, b"");
    harness.app.on_frame(&frame);
    assert_eq!(
        harness.sent(),
        vec![ClientToServerMsg::RenderFrameAck { seq: 17 }]
    );
}

#[test]
fn an_unreadable_frame_detaches_rather_than_being_acknowledged() {
    let mut harness = Harness::new(1, true, "");
    harness.app.on_frame(b"nonsense");
    assert_eq!(
        harness.actions(),
        vec![zellij_utils::input::actions::Action::Detach]
    );
}

#[test]
fn a_frame_for_a_viewport_the_screen_has_moved_past_is_acknowledged_and_dropped() {
    let mut harness = Harness::new(1, true, "");
    let size = harness.app.state.size();
    let mut stale = zellij_utils::structured_render::FrameBuilder::new(size.cols as u16 - 1, 4, 23);
    stale.push_row(0, 0, &[zellij_utils::structured_render::WireCell::BLANK]);
    harness.app.on_frame(&stale.finish());
    assert_eq!(harness.app.state.size(), size);
    assert_eq!(
        harness.sent(),
        vec![ClientToServerMsg::RenderFrameAck { seq: 23 }]
    );
}

#[test]
fn a_reflow_reports_the_new_size_without_reshaping_the_screen_locally() {
    let mut harness = Harness::new(2, true, "");
    let before = harness.app.state.size();
    harness.app.reflow(320, 400);
    assert_eq!(harness.app.state.size(), before);
    assert_eq!(
        harness.sent()[0],
        ClientToServerMsg::TerminalResize {
            new_size: zellij_utils::pane_size::Size { rows: 20, cols: 40 },
        }
    );
}

#[test]
fn a_title_a_bell_and_a_notification_are_all_drained_by_a_frame() {
    let mut harness = Harness::new(1, true, "");
    harness.app.ring = count_a_ring;
    let size = harness.app.state.size();
    let frame = frame_with_sideband(
        size.rows,
        size.cols,
        0,
        b"\x1b]0;zellij - pane\x07\x07\x1b]99;i=1:d=0;body\x1b\\",
    );
    harness.app.on_frame(&frame);
    assert_eq!(harness.app.state.take_title(), None);
    assert_eq!(harness.app.state.take_bells(), 0);
    assert!(harness.app.state.take_notifications().is_empty());
}

#[test]
fn a_completed_notification_is_drained_without_reaching_the_desktop() {
    let mut harness = Harness::new(1, true, "");
    harness.app.ring = count_a_ring;
    let size = harness.app.state.size();
    let frame = frame_with_sideband(size.rows, size.cols, 0, b"\x1b]9;body\x07");
    harness.app.on_frame(&frame);
    assert!(harness.app.state.take_notifications().is_empty());
}

#[test]
fn a_frame_that_signals_nothing_leaves_the_title_untouched() {
    let mut harness = Harness::new(1, true, "");
    let size = harness.app.state.size();
    let frame = frame_with_sideband(size.rows, size.cols, 0, b"");
    harness.app.on_frame(&frame);
    assert_eq!(harness.app.state.take_title(), None);
}

#[test]
fn a_click_reaches_the_server_as_a_mouse_action() {
    let mut harness = Harness::new(2, true, "");
    harness.app.on_cursor_moved(at(35.0, 51.0));
    harness
        .app
        .on_mouse_input(MouseButton::Left, ElementState::Pressed);
    let events: Vec<_> = harness
        .actions()
        .into_iter()
        .map(|action| match action {
            Action::MouseEvent { event } => event,
            other => panic!("expected a mouse event, got {:?}", other),
        })
        .collect();
    assert_eq!(events[0].event_type, MouseEventType::Motion);
    assert_eq!(events[1].event_type, MouseEventType::Press);
    assert!(events[1].left);
    assert_eq!(
        events[1].position,
        zellij_utils::position::Position::new(2, 3)
    );
}

#[test]
fn a_wheel_tick_reaches_the_server() {
    let mut harness = Harness::new(1, true, "");
    harness
        .app
        .on_wheel(MouseScrollDelta::LineDelta(0.0, 1.0), TouchPhase::Moved);
    match &harness.actions()[0] {
        Action::MouseEvent { event } => assert!(event.wheel_up),
        other => panic!("expected a mouse event, got {:?}", other),
    }
}

#[test]
fn a_scale_change_rebuilds_the_cell_metrics() {
    let mut harness = Harness::new(0, true, "");
    assert_eq!(harness.app.metrics.width, 8);
    assert_eq!(harness.app.metrics.height, 20);

    harness.app.rescale(2.0);

    assert_eq!(harness.app.metrics.width, 16);
    assert_eq!(harness.app.metrics.height, 40);
    assert_eq!(harness.app.cache.metrics(), harness.app.metrics);
    assert_eq!(harness.app.scale, 2.0);
}

#[test]
fn an_unusable_scale_leaves_the_previous_one_in_place() {
    let mut harness = Harness::new(0, true, "");
    let before = harness.app.metrics;

    for scale in [0.0, -1.0, f64::NAN, f64::INFINITY, 1000.0] {
        harness.app.rescale(scale);
        assert_eq!(harness.app.metrics, before, "scale {}", scale);
        assert_eq!(harness.app.scale, 1.0, "scale {}", scale);
    }
}

#[test]
fn rescaling_to_the_current_scale_changes_nothing() {
    let mut harness = Harness::new(0, true, "");
    harness.app.rescale(2.0);
    let metrics = harness.app.metrics;
    harness.app.rescale(2.0);
    assert_eq!(harness.app.metrics, metrics);
}

#[test]
fn a_reflow_after_a_scale_change_reports_the_scaled_cell_size() {
    let mut harness = Harness::new(2, true, "");
    harness.app.rescale(2.0);
    harness.app.reflow(320, 400);
    assert_eq!(
        harness.sent(),
        vec![
            ClientToServerMsg::TerminalResize {
                new_size: zellij_utils::pane_size::Size { rows: 10, cols: 20 },
            },
            ClientToServerMsg::TerminalPixelDimensions {
                pixel_dimensions: zellij_utils::ipc::PixelDimensions {
                    text_area_size: Some(zellij_utils::pane_size::SizeInPixels {
                        width: 320,
                        height: 400,
                    }),
                    character_cell_size: Some(zellij_utils::pane_size::SizeInPixels {
                        width: 16,
                        height: 40,
                    }),
                },
            },
        ]
    );
}

fn styling(background: [u8; 3]) -> zellij_utils::input::theme::Theme {
    let [r, g, b] = background;
    zellij_utils::input::theme::Theme {
        sourced_from_external_file: false,
        terminal_colors: None,
        palette: zellij_utils::data::Styling {
            text_unselected: zellij_utils::data::StyleDeclaration {
                background: zellij_utils::data::PaletteColor::Rgb((r, g, b)),
                ..zellij_utils::data::DEFAULT_STYLES.text_unselected
            },
            ..zellij_utils::data::DEFAULT_STYLES
        },
    }
}

fn settings(section: TerminalConfig) -> Settings {
    Settings {
        section: TerminalConfig {
            font_size: section
                .font_size
                .or(Some(pixels_to_points(DEFAULT_FONT_SIZE))),
            ..section
        },
        ..Settings::default()
    }
}

impl Harness {
    fn reconfigure(&mut self, section: TerminalConfig) {
        self.app.reconfigured(settings(section));
    }

    fn push(&mut self, section: TerminalConfig) {
        self.app.settings_pushed(settings(section));
    }
}

#[test]
fn a_panel_keeps_its_grip_margins_when_its_session_pushes_settings() {
    let mut harness = Harness::new(0, true, "");
    harness.app.appearance.look = WindowLook::Popup { position: None };
    crate::launch::panel_look(&mut harness.app.options);
    let margins = harness.app.padding_px();
    assert!(margins.left > 0 && margins.top > 0);
    harness.push(TerminalConfig::default());
    assert_eq!(harness.app.padding_px(), margins);
    assert_eq!(
        harness.app.options.padding_color,
        zellij_utils::input::terminal::PaddingColor::Background
    );
}

#[test]
fn a_reload_that_changes_the_font_size_rebuilds_the_cell_and_reports_it() {
    let mut harness = Harness::new(2, true, "");
    assert_eq!(
        (harness.app.metrics.width, harness.app.metrics.height),
        (8, 20)
    );

    harness.reconfigure(TerminalConfig {
        font_size: Some(pixels_to_points(DEFAULT_FONT_SIZE * 2.0)),
        ..TerminalConfig::default()
    });

    assert_eq!(harness.app.metrics.width, 16);
    assert_eq!(harness.app.metrics.height, 40);
    assert_eq!(
        harness.app.cache.metrics(),
        harness.app.metrics,
        "the glyph cache was not rebuilt with the new font"
    );

    harness.app.reflow(960, 800);
    assert_eq!(
        harness.sent()[0],
        ClientToServerMsg::TerminalResize {
            new_size: zellij_utils::pane_size::Size { rows: 20, cols: 60 },
        },
        "the session was not told the window now holds fewer, larger cells"
    );
}

#[test]
fn a_reload_that_changes_the_colors_re_declares_them_without_touching_the_font() {
    let mut harness = Harness::new(3, true, "");
    let before = harness.app.metrics;

    harness.reconfigure(TerminalConfig {
        theme: Some(TerminalTheme {
            background: Some(zellij_utils::data::PaletteColor::Rgb((4, 5, 6))),
            ..TerminalTheme::default()
        }),
        ..TerminalConfig::default()
    });

    assert_eq!(harness.app.options.paints.background, [4, 5, 6]);
    assert_eq!(harness.app.metrics, before);
    let sent = harness.sent();
    assert_eq!(sent.len(), 3, "{:?}", sent);
    assert_eq!(
        sent[1],
        ClientToServerMsg::BackgroundColor {
            color: crate::emulation::palette::xparse_color([4, 5, 6])
        }
    );
}

#[test]
fn a_reload_that_changes_the_paste_chords_claims_the_new_one() {
    let mut harness = Harness::new(2, true, "copied");
    harness.reconfigure(TerminalConfig {
        paste_keys: Some(vec![zellij_utils::data::KeyWithModifier::new(
            zellij_utils::data::BareKey::F(5),
        )]),
        ..TerminalConfig::default()
    });

    harness.app.modifiers = ModifiersState::CONTROL | ModifiersState::SHIFT;
    harness.press(&character("V"));
    harness.app.modifiers = ModifiersState::empty();
    harness.press(&Key::Named(NamedKey::F5));

    let sent = harness.sent();
    assert!(
        matches!(sent[0], ClientToServerMsg::Key { .. }),
        "the chord the session now owns was still swallowed: {:?}",
        sent[0]
    );
    assert_eq!(
        sent[1],
        ClientToServerMsg::Action {
            action: Action::Paste {
                chars: "copied".to_owned(),
                pane_id: None
            },
            terminal_id: None,
            client_id: None,
            is_cli_client: false,
        }
    );
}

#[test]
fn a_reload_that_changes_nothing_disturbs_nothing() {
    let mut harness = Harness::new(0, true, "");

    let before = harness.app.metrics;
    harness.reconfigure(TerminalConfig::default());
    assert_eq!(harness.app.metrics, before);
    assert_eq!(harness.app.options.font.size, DEFAULT_FONT_SIZE);
    assert_eq!(
        harness.app.options.paste_keys,
        crate::config::options::default_paste_keys()
    );
    assert_eq!(harness.app.options.paints, Paints::default());
}

#[test]
fn a_theme_mode_change_repaints_without_a_configuration_edit() {
    let mut harness = Harness::new(6, true, "");
    harness.app.settings = Settings {
        theme_dark: Some(styling([1, 1, 1])),
        theme_light: Some(styling([2, 2, 2])),
        ..settings(TerminalConfig::default())
    };
    harness.app.reapply(false);
    assert_eq!(harness.app.options.paints.background, [1, 1, 1]);

    harness.app.theme_mode = Some(HostTerminalThemeMode::Light);
    harness.app.reapply(false);
    assert_eq!(harness.app.options.paints.background, [2, 2, 2]);

    let sent = harness.sent();
    assert_eq!(sent.len(), 6, "{:?}", sent);
    assert_eq!(
        sent[1],
        ClientToServerMsg::BackgroundColor {
            color: crate::emulation::palette::xparse_color([1, 1, 1])
        }
    );
    assert_eq!(
        sent[4],
        ClientToServerMsg::BackgroundColor {
            color: crate::emulation::palette::xparse_color([2, 2, 2])
        },
        "the session was not told the colours it answers pane queries with"
    );
}

#[test]
fn a_theme_mode_change_moves_nothing_when_only_one_theme_is_configured() {
    let mut harness = Harness::new(0, true, "");
    harness.app.settings = Settings {
        theme: Some(styling([3, 3, 3])),
        theme_dark: Some(styling([1, 1, 1])),
        ..settings(TerminalConfig::default())
    };
    harness.app.reapply(false);
    let before = harness.app.options.paints;

    harness.app.theme_mode = Some(HostTerminalThemeMode::Light);
    harness.app.reapply(false);

    assert_eq!(harness.app.options.paints, before);
    assert_eq!(harness.app.options.paints.background, [3, 3, 3]);
}

#[test]
fn a_reloaded_cursor_leaves_the_font_and_the_session_alone() {
    let mut harness = Harness::new(0, true, "");
    let before = harness.app.metrics;

    harness.reconfigure(TerminalConfig {
        cursor_style: Some(zellij_utils::input::terminal::CursorStyle::Bar),
        cursor_blink: Some(true),
        ..TerminalConfig::default()
    });

    assert_eq!(
        harness.app.options.cursor_shape,
        Some(crate::emulation::screen_buffer::CursorShape::Beam)
    );
    assert_eq!(harness.app.options.cursor_blink, Some(true));
    assert_eq!(
        harness.app.metrics, before,
        "a cursor change rebuilt the font"
    );
    assert_eq!(
        harness.app.cursor_options(),
        scene::CursorOptions {
            shape: Some(crate::emulation::screen_buffer::CursorShape::Beam),
            blink: Some(true),
            hollow: false,
        },
        "the options the scene is built with did not follow the reload"
    );
}

#[test]
fn a_reload_that_changes_the_opacity_reaches_the_scene_on_the_next_frame() {
    use zellij_utils::input::terminal::OpacityMode;
    let mut harness = Harness::new(0, true, "");
    let before = harness.app.metrics;
    harness.app.refresh_scene();
    let identity = harness.app.retained.identity();
    harness.app.refresh_scene();
    assert!(!harness.app.retained.rebuilt_everything());
    assert_eq!(harness.app.retained.transparency(), Transparency::OPAQUE);

    harness.reconfigure(TerminalConfig {
        opacity: Some(0.8),
        ..TerminalConfig::default()
    });
    let expected = Transparency {
        opacity: 0.8,
        mode: OpacityMode::Background,
    };
    assert_eq!(harness.app.options.transparency, expected);
    assert_eq!(harness.app.effective_transparency(), expected);
    harness.app.refresh_scene();
    assert_eq!(harness.app.retained.transparency(), expected);
    assert!(
        harness.app.retained.rebuilt_everything(),
        "the scene was not rebuilt for the new opacity"
    );

    harness.reconfigure(TerminalConfig {
        opacity: Some(0.8),
        opacity_mode: Some(OpacityMode::Everything),
        ..TerminalConfig::default()
    });
    harness.app.refresh_scene();
    assert_eq!(
        harness.app.retained.transparency(),
        Transparency {
            opacity: 0.8,
            mode: OpacityMode::Everything,
        }
    );
    assert!(harness.app.retained.rebuilt_everything());

    harness.reconfigure(TerminalConfig {
        opacity: Some(0.8),
        opacity_mode: Some(OpacityMode::Everything),
        blur: Some(true),
        ..TerminalConfig::default()
    });
    assert!(harness.app.options.blur);
    harness.app.refresh_scene();
    assert!(
        !harness.app.retained.rebuilt_everything(),
        "a blur change is the compositor's business and must not rebuild the scene"
    );

    assert_eq!(
        harness.app.retained.identity(),
        identity,
        "the scene was replaced rather than updated"
    );
    assert_eq!(
        harness.app.metrics, before,
        "a see-through change rebuilt the font"
    );
}

#[test]
fn only_a_window_drawn_below_full_opacity_is_see_through() {
    let mut harness = Harness::new(0, true, "");
    assert!(!harness.app.see_through());

    harness.reconfigure(TerminalConfig {
        opacity: Some(0.9),
        ..TerminalConfig::default()
    });
    assert!(harness.app.see_through());

    harness.reconfigure(TerminalConfig {
        opacity: Some(1.0),
        blur: Some(true),
        ..TerminalConfig::default()
    });
    assert!(
        !harness.app.see_through(),
        "a blur behind a solid window shows through nothing"
    );

    harness.app.appearance.transparency_available = false;
    harness.reconfigure(TerminalConfig {
        opacity: Some(0.5),
        ..TerminalConfig::default()
    });
    assert!(!harness.app.see_through());
}

#[test]
fn a_display_without_transparency_draws_opaque_and_warns_once() {
    let mut harness = Harness::new(0, true, "");
    harness.app.appearance.transparency_available = false;
    harness.reconfigure(TerminalConfig {
        opacity: Some(0.5),
        ..TerminalConfig::default()
    });
    assert_eq!(harness.app.effective_transparency(), Transparency::OPAQUE);
    assert!(harness.app.appearance.warned_opaque);
    harness.app.refresh_scene();
    assert_eq!(harness.app.retained.transparency(), Transparency::OPAQUE);

    let mut harness = Harness::new(0, true, "");
    harness.app.appearance.transparency_available = false;
    harness.app.warn_if_opaque();
    assert!(
        !harness.app.appearance.warned_opaque,
        "a solid window has nothing to warn about"
    );
}

#[test]
fn a_transparent_config_is_preferred_before_the_fewest_samples() {
    assert_eq!(
        best_config([(Some(false), 0), (Some(true), 4), (None, 0)].into_iter()),
        Some(1)
    );
    assert_eq!(
        best_config([(Some(false), 0), (None, 4), (Some(false), 0)].into_iter()),
        Some(1)
    );
    assert_eq!(
        best_config([(Some(true), 4), (Some(true), 0), (Some(true), 0)].into_iter()),
        Some(1),
        "among equals the fewest samples wins and ties keep the first offered"
    );
    assert_eq!(
        best_config([(Some(false), 2), (Some(false), 0)].into_iter()),
        Some(1)
    );
    assert_eq!(best_config(std::iter::empty()), None);
}

#[test]
fn only_macos_requires_a_transparent_config() {
    assert!(
        !requires_transparent_configs(Platform::Windows),
        "a driver that honors WGL_TRANSPARENT_ARB would offer no config at all"
    );
    assert!(
        !requires_transparent_configs(Platform::Linux),
        "an X11 display without a see-through visual would offer no config at all"
    );
    assert!(requires_transparent_configs(Platform::MacOs));
}

#[test]
fn windows_sees_through_any_config_with_an_alpha_channel() {
    for reported in [Some(false), None, Some(true)] {
        assert!(
            transparency_available(Platform::Windows, reported, 8),
            "WGL_TRANSPARENT_ARB reported as {:?} turned off a config with 8 alpha bits",
            reported
        );
        assert!(
            !transparency_available(Platform::Windows, reported, 0),
            "a config with no alpha channel cannot be seen through"
        );
    }
}

#[test]
fn elsewhere_the_config_decides_whether_it_can_be_seen_through() {
    for platform in [Platform::Linux, Platform::MacOs] {
        for alpha in [0, 8] {
            assert!(transparency_available(platform, Some(true), alpha));
            assert!(transparency_available(platform, None, alpha));
            assert!(!transparency_available(platform, Some(false), alpha));
        }
    }
}

#[test]
fn a_reloaded_font_that_cannot_be_built_leaves_the_one_in_place_alone() {
    let mut harness = Harness::new(0, true, "");
    let before = harness.app.metrics;

    harness.reconfigure(TerminalConfig {
        font_size: Some(100_000.0),
        ..TerminalConfig::default()
    });

    assert_eq!(harness.app.metrics, before);
    assert_eq!(harness.app.cache.metrics(), before);
    assert_eq!(
        harness.app.options.font.size, DEFAULT_FONT_SIZE,
        "an unusable font size must not be left recorded as the one in force"
    );
}

#[test]
fn the_zoom_chords_rebuild_the_font_over_the_resolved_size() {
    let mut harness = Harness::new(0, true, "");
    let plain = harness.app.metrics;
    harness.app.modifiers = ModifiersState::CONTROL;

    harness.press(&character("="));
    assert_eq!(harness.app.zoom, 1.1);
    assert!(
        harness.app.metrics.height > plain.height,
        "zooming in did not grow the cell"
    );
    assert_eq!(
        harness.app.options.font.size, DEFAULT_FONT_SIZE,
        "the zoom must never be written back into the resolved size"
    );

    harness.press(&character("-"));
    assert!((harness.app.zoom - 1.0).abs() < 1e-9);
    assert_eq!(harness.app.metrics, plain);
}

#[test]
fn the_shifted_plus_zooms_in_as_the_unshifted_one_does() {
    let mut harness = Harness::new(0, true, "");
    harness.app.modifiers = ModifiersState::CONTROL | ModifiersState::SHIFT;
    harness.press(&character("+"));
    assert_eq!(harness.app.zoom, 1.1);

    let mut harness = Harness::new(0, true, "");
    harness.app.modifiers = ModifiersState::CONTROL;
    harness.press(&character("+"));
    assert_eq!(harness.app.zoom, 1.1);
}

#[test]
fn the_reset_chord_returns_to_the_resolved_size() {
    let mut harness = Harness::new(0, true, "");
    let plain = harness.app.metrics;
    harness.app.modifiers = ModifiersState::CONTROL;
    harness.press(&character("="));
    harness.press(&character("="));
    assert!(harness.app.zoom > 1.2);

    harness.press(&character("0"));
    assert_eq!(harness.app.zoom, 1.0);
    assert_eq!(harness.app.metrics, plain);
}

#[test]
fn a_zoom_chord_reaches_the_pane_when_it_is_unbound() {
    let mut harness = Harness::new(1, true, "");
    harness.reconfigure(TerminalConfig {
        zoom_in_keys: Some(Vec::new()),
        ..TerminalConfig::default()
    });
    harness.app.modifiers = ModifiersState::CONTROL;
    harness.press(&character("="));
    assert_eq!(harness.app.zoom, 1.0);
    assert!(
        matches!(harness.sent()[0], ClientToServerMsg::Key { .. }),
        "an unbound chord must be the escape hatch it is advertised as"
    );
}

#[test]
fn any_configuration_reload_resets_the_zoom() {
    let mut harness = Harness::new(0, true, "");
    let plain = harness.app.metrics;
    harness.app.modifiers = ModifiersState::CONTROL;
    harness.press(&character("="));
    assert!(harness.app.metrics.height > plain.height);

    harness.reconfigure(TerminalConfig::default());

    assert_eq!(harness.app.zoom, 1.0);
    assert_eq!(
        harness.app.metrics, plain,
        "a reload that changed nothing still has to take the zoom back"
    );
}

#[test]
fn a_pushed_change_that_leaves_the_font_size_alone_keeps_the_zoom() {
    let mut harness = Harness::new(0, true, "");
    harness.app.modifiers = ModifiersState::CONTROL;
    harness.press(&character("="));
    let zoomed = harness.app.metrics;

    harness.push(TerminalConfig {
        blur: Some(true),
        ..TerminalConfig::default()
    });

    assert_eq!(harness.app.zoom, 1.1);
    assert_eq!(harness.app.metrics, zoomed);
    assert!(
        harness.app.options.blur,
        "the pushed change was not applied"
    );
}

#[test]
fn a_pushed_font_size_resets_the_zoom() {
    let mut harness = Harness::new(0, true, "");
    harness.app.modifiers = ModifiersState::CONTROL;
    harness.press(&character("="));

    harness.push(TerminalConfig {
        font_size: Some(pixels_to_points(DEFAULT_FONT_SIZE * 2.0)),
        ..TerminalConfig::default()
    });

    assert_eq!(harness.app.zoom, 1.0);
    assert_eq!(harness.app.metrics.height, 40);
    assert_eq!(harness.app.options.font.size, DEFAULT_FONT_SIZE * 2.0);
}

#[test]
fn a_reload_that_changes_the_size_resolves_it_without_the_zoom() {
    let mut harness = Harness::new(0, true, "");
    harness.app.modifiers = ModifiersState::CONTROL;
    harness.press(&character("="));

    harness.reconfigure(TerminalConfig {
        font_size: Some(pixels_to_points(DEFAULT_FONT_SIZE * 2.0)),
        ..TerminalConfig::default()
    });

    assert_eq!(harness.app.zoom, 1.0);
    assert_eq!(harness.app.metrics.height, 40);
    assert_eq!(harness.app.options.font.size, DEFAULT_FONT_SIZE * 2.0);
}

#[test]
fn a_zoom_the_font_cannot_be_built_at_is_refused_and_forgotten() {
    let mut harness = Harness::new(0, true, "");
    harness.app.modifiers = ModifiersState::CONTROL;
    for _ in 0..80 {
        harness.press(&character("="));
    }
    let held = harness.app.zoom;
    assert!(
        held * DEFAULT_FONT_SIZE as f64 <= 512.0,
        "the zoom ran past the size the font stack refuses"
    );
    assert_eq!(
        harness.app.cache.metrics(),
        harness.app.metrics,
        "a refused zoom left the cache and the metrics disagreeing"
    );
    harness.press(&character("-"));
    assert!(harness.app.zoom < held);
}

#[test]
fn the_zoom_rides_on_top_of_the_display_scale() {
    let mut harness = Harness::new(0, true, "");
    harness.app.rescale(2.0);
    let scaled = harness.app.metrics;
    harness.app.modifiers = ModifiersState::CONTROL;
    harness.press(&character("="));
    assert_eq!(harness.app.scale, 2.0);
    assert_eq!(harness.app.zoom, 1.1);
    assert!(harness.app.metrics.height > scaled.height);

    harness.press(&character("0"));
    assert_eq!(harness.app.metrics, scaled);
    assert_eq!(harness.app.scale, 2.0);
}

fn with_a_link_under_the_pointer(harness: &mut Harness, uri: &str) {
    crate::testing::painter::Painter::apply(&mut harness.app.state, |painter| {
        painter.links(&[(1, uri)]);
        painter.linked(0, 0, "link", 1);
    });
    harness.app.on_cursor_moved(PhysicalPosition::new(4.0, 4.0));
}

fn left_click(harness: &mut Harness) {
    harness
        .app
        .on_mouse_input(MouseButton::Left, ElementState::Pressed);
    harness
        .app
        .on_mouse_input(MouseButton::Left, ElementState::Released);
}

fn opened_urls() -> Vec<String> {
    links::launch::opened()
        .into_iter()
        .map(|(_, uri)| uri)
        .collect()
}

#[test]
fn a_click_on_a_link_is_still_forwarded_to_the_pane() {
    links::launch::forget_opened();
    let mut harness = Harness::new(3, true, "");
    with_a_link_under_the_pointer(&mut harness, "https://example.com");
    assert_eq!(
        harness
            .app
            .mouse
            .hovered_link
            .as_ref()
            .map(|run| run.uri.as_str()),
        Some("https://example.com"),
        "the pointer should be over the link"
    );
    left_click(&mut harness);

    let events: Vec<_> = harness
        .actions()
        .into_iter()
        .map(|action| match action {
            Action::MouseEvent { event } => event,
            other => panic!("expected a mouse event, got {:?}", other),
        })
        .collect();
    assert_eq!(
        events.len(),
        3,
        "opening a link must not cost the pane its focus or its selection"
    );
    assert_eq!(events[1].event_type, MouseEventType::Press);
    assert!(events[1].left);
    assert_eq!(events[2].event_type, MouseEventType::Release);
    assert_eq!(opened_urls(), vec!["https://example.com".to_owned()]);
}

#[test]
fn a_press_over_a_link_arms_the_open_and_a_release_on_it_fires() {
    links::launch::forget_opened();
    let mut harness = Harness::new(3, true, "");
    with_a_link_under_the_pointer(&mut harness, "https://example.com");
    harness
        .app
        .on_mouse_input(MouseButton::Left, ElementState::Pressed);
    assert_eq!(
        harness
            .app
            .mouse
            .armed_link
            .as_ref()
            .map(|armed| armed.uri.as_str()),
        Some("https://example.com")
    );
    assert!(opened_urls().is_empty(), "a press alone opens nothing");
    harness
        .app
        .on_mouse_input(MouseButton::Left, ElementState::Released);
    assert!(
        harness.app.mouse.armed_link.is_none(),
        "the release must spend the armed open"
    );
    assert_eq!(opened_urls(), vec!["https://example.com".to_owned()]);
}

#[test]
fn a_press_on_a_link_dragged_off_it_opens_nothing() {
    links::launch::forget_opened();
    let mut harness = Harness::new(4, true, "");
    with_a_link_under_the_pointer(&mut harness, "https://example.com");
    harness
        .app
        .on_mouse_input(MouseButton::Left, ElementState::Pressed);
    assert!(harness.app.mouse.armed_link.is_some());
    harness
        .app
        .on_cursor_moved(PhysicalPosition::new(455.0, 4.0));
    harness
        .app
        .on_mouse_input(MouseButton::Left, ElementState::Released);
    assert!(
        harness.app.mouse.armed_link.is_none(),
        "a drag is a selection, and it must leave nothing armed"
    );
    assert!(opened_urls().is_empty());
}

#[test]
fn a_press_over_a_scheme_the_allow_list_refuses_arms_nothing() {
    links::launch::forget_opened();
    let mut harness = Harness::new(3, true, "");
    with_a_link_under_the_pointer(&mut harness, "javascript:alert(1)");
    harness
        .app
        .on_mouse_input(MouseButton::Left, ElementState::Pressed);
    assert!(harness.app.mouse.armed_link.is_none());
    harness
        .app
        .on_mouse_input(MouseButton::Left, ElementState::Released);
    assert!(opened_urls().is_empty());
    for action in harness.actions() {
        assert!(matches!(action, Action::MouseEvent { .. }), "{:?}", action);
    }
}

#[test]
fn a_press_away_from_any_link_arms_nothing() {
    links::launch::forget_opened();
    let mut harness = Harness::new(3, true, "");
    with_a_link_under_the_pointer(&mut harness, "https://example.com");
    harness
        .app
        .on_cursor_moved(PhysicalPosition::new(455.0, 4.0));
    assert!(harness.app.mouse.hovered_link.is_none());
    harness
        .app
        .on_mouse_input(MouseButton::Left, ElementState::Pressed);
    assert!(harness.app.mouse.armed_link.is_none());
}

#[test]
fn opening_links_switched_off_arms_nothing() {
    links::launch::forget_opened();
    let mut harness = Harness::new(3, true, "");
    harness.reconfigure(TerminalConfig {
        open_links: Some(false),
        ..TerminalConfig::default()
    });
    with_a_link_under_the_pointer(&mut harness, "https://example.com");
    harness
        .app
        .on_mouse_input(MouseButton::Left, ElementState::Pressed);
    assert!(harness.app.mouse.armed_link.is_none());
}

#[test]
fn the_pointer_becomes_a_hand_over_a_link_and_a_beam_beside_it() {
    let mut harness = Harness::new(2, true, "");
    with_a_link_under_the_pointer(&mut harness, "https://example.com");
    assert_eq!(harness.app.pointer_shape(), CursorIcon::Pointer);
    harness
        .app
        .on_cursor_moved(PhysicalPosition::new(455.0, 4.0));
    assert_eq!(harness.app.pointer_shape(), CursorIcon::Text);
}

#[test]
fn a_link_this_client_will_not_open_never_shows_the_hand() {
    let mut harness = Harness::new(1, true, "");
    with_a_link_under_the_pointer(&mut harness, "javascript:alert(1)");
    assert!(
        harness.app.mouse.hovered_link.is_some(),
        "it is still a link"
    );
    assert_eq!(
        harness.app.pointer_shape(),
        CursorIcon::Text,
        "the hand would promise an open the allow-list refuses"
    );
}

#[test]
fn switching_opening_off_takes_the_hand_away() {
    let mut harness = Harness::new(1, true, "");
    with_a_link_under_the_pointer(&mut harness, "https://example.com");
    assert_eq!(harness.app.pointer_shape(), CursorIcon::Pointer);
    harness.reconfigure(TerminalConfig {
        open_links: Some(false),
        ..TerminalConfig::default()
    });
    assert_eq!(harness.app.pointer_shape(), CursorIcon::Text);
}

#[test]
fn the_hover_run_follows_the_pointer_across_a_link_boundary() {
    let mut harness = Harness::new(2, true, "");
    with_a_link_under_the_pointer(&mut harness, "https://example.com");
    assert_eq!(
        harness.app.mouse.hovered_link.as_ref().map(|run| run.id),
        Some(1)
    );
    harness
        .app
        .on_cursor_moved(PhysicalPosition::new(45.0, 4.0));
    assert!(
        harness.app.mouse.hovered_link.is_none(),
        "leaving the link must drop the hover"
    );
}

fn shift_click(harness: &mut Harness) {
    harness.app.modifiers = ModifiersState::SHIFT;
    left_click(harness);
    harness.app.modifiers = ModifiersState::empty();
}

#[test]
fn a_shift_click_on_a_link_opens_it_in_a_pane_that_is_not_watching_the_mouse() {
    links::launch::forget_opened();
    let mut harness = Harness::new(1, true, "");
    with_a_link_under_the_pointer(&mut harness, "https://example.com");
    shift_click(&mut harness);
    assert_eq!(opened_urls(), vec!["https://example.com".to_owned()]);
    assert_eq!(harness.app.mouse.selection, None, "a click selects nothing");
    let actions = harness.actions();
    assert_eq!(
        actions.len(),
        1,
        "only the pointer motion reaches the session: {:?}",
        actions
    );
}

#[test]
fn a_shift_click_on_a_link_opens_nothing_once_shift_opening_is_off() {
    links::launch::forget_opened();
    let mut harness = Harness::new(1, true, "");
    harness.reconfigure(TerminalConfig {
        open_links_with_shift: Some(false),
        ..TerminalConfig::default()
    });
    with_a_link_under_the_pointer(&mut harness, "https://example.com");
    harness.app.modifiers = ModifiersState::SHIFT;
    assert_eq!(
        harness.app.pointer_shape(),
        CursorIcon::Text,
        "no hand while shift cannot open the link"
    );
    shift_click(&mut harness);
    assert!(opened_urls().is_empty());
}

#[test]
fn a_shift_click_on_a_link_opens_it_and_reaches_the_session_when_selecting_is_off() {
    links::launch::forget_opened();
    let mut harness = Harness::new(3, true, "");
    harness.reconfigure(TerminalConfig {
        shift_drag_selects: Some(false),
        ..TerminalConfig::default()
    });
    with_a_link_under_the_pointer(&mut harness, "https://example.com");
    shift_click(&mut harness);
    assert_eq!(opened_urls(), vec!["https://example.com".to_owned()]);
    let events: Vec<_> = harness
        .actions()
        .into_iter()
        .map(|action| match action {
            Action::MouseEvent { event } => event,
            other => panic!("expected a mouse event, got {:?}", other),
        })
        .collect();
    assert_eq!(events[1].event_type, MouseEventType::Press);
    assert!(events[1].shift);
    assert_eq!(events[2].event_type, MouseEventType::Release);
}

fn over_cell(harness: &Harness, row: usize, col: usize) -> PhysicalPosition<f64> {
    let geometry = harness.app.geometry.get();
    let (width, height) = (geometry.cell_width as f64, geometry.cell_height as f64);
    PhysicalPosition::new(
        col as f64 * width + width / 2.0,
        row as f64 * height + height / 2.0,
    )
}

fn with_an_interface_on_screen(harness: &mut Harness) {
    crate::testing::painter::Painter::apply(&mut harness.app.state, |painter| {
        painter.text(0, 0, "Tab #1  Tab #2");
        painter.text(1, 0, "│ hello │ world");
        painter.text(2, 0, "status bar");
    });
}

fn shift_drag(harness: &mut Harness, from: (usize, usize), to: (usize, usize)) {
    let start = over_cell(harness, from.0, from.1);
    harness.app.on_cursor_moved(start);
    harness.app.modifiers = ModifiersState::SHIFT;
    harness
        .app
        .on_mouse_input(MouseButton::Left, ElementState::Pressed);
    let end = over_cell(harness, to.0, to.1);
    harness.app.on_cursor_moved(end);
    harness
        .app
        .on_mouse_input(MouseButton::Left, ElementState::Released);
    harness.app.modifiers = ModifiersState::empty();
}

fn copy_key(harness: &mut Harness) {
    harness.app.modifiers = ModifiersState::CONTROL | ModifiersState::SHIFT;
    harness.press(&character("C"));
    harness.app.modifiers = ModifiersState::empty();
}

#[test]
fn a_shift_drag_selects_across_the_whole_window_and_the_copy_key_copies_it() {
    let mut harness = Harness::new(1, true, "");
    with_an_interface_on_screen(&mut harness);
    shift_drag(&mut harness, (0, 4), (2, 5));
    assert_eq!(
        harness.app.mouse.selection,
        Some(Selection::at(0, 4).extended_to(2, 5))
    );
    assert_eq!(
        harness.clipboard_text(),
        None,
        "nothing is copied before the copy key"
    );

    copy_key(&mut harness);
    let expected = "#1  Tab #2\n│ hello │ world\nstatus";
    assert_eq!(harness.clipboard_text().as_deref(), Some(expected));
    assert_eq!(
        harness.clipboard.lock().unwrap().get_primary().as_deref(),
        Some(expected)
    );
    assert_eq!(
        harness.app.mouse.selection, None,
        "copying clears the highlight"
    );
    assert_eq!(
        harness.actions().len(),
        1,
        "only the first pointer motion reaches the session"
    );
}

#[test]
fn the_copy_key_reaches_the_session_when_nothing_is_selected() {
    let mut harness = Harness::new(1, true, "");
    copy_key(&mut harness);
    assert_eq!(harness.clipboard_text(), None);
    assert_eq!(harness.sent().len(), 1);
}

#[test]
fn a_plain_click_clears_the_selection_and_reaches_the_session() {
    let mut harness = Harness::new(3, true, "");
    with_an_interface_on_screen(&mut harness);
    shift_drag(&mut harness, (0, 0), (1, 3));
    assert!(harness.app.mouse.selection.is_some());
    left_click(&mut harness);
    assert_eq!(harness.app.mouse.selection, None);
    assert_eq!(harness.actions().len(), 3);
}

#[test]
fn typing_clears_the_selection() {
    let mut harness = Harness::new(2, true, "");
    with_an_interface_on_screen(&mut harness);
    shift_drag(&mut harness, (0, 0), (1, 3));
    harness.press(&character("a"));
    assert_eq!(harness.app.mouse.selection, None);
    copy_key(&mut harness);
    assert_eq!(harness.clipboard_text(), None);
}

#[test]
fn a_frame_drawn_under_the_selection_leaves_it_in_place() {
    let mut harness = Harness::new(1, true, "");
    with_an_interface_on_screen(&mut harness);
    shift_drag(&mut harness, (2, 0), (2, 5));
    crate::testing::painter::Painter::apply(&mut harness.app.state, |painter| {
        painter.text(2, 0, "change");
    });
    copy_key(&mut harness);
    assert_eq!(harness.clipboard_text().as_deref(), Some("change"));
}

#[test]
fn a_shift_drag_reaches_the_session_when_selecting_is_off() {
    let mut harness = Harness::new(4, true, "");
    harness.reconfigure(TerminalConfig {
        shift_drag_selects: Some(false),
        ..TerminalConfig::default()
    });
    with_an_interface_on_screen(&mut harness);
    shift_drag(&mut harness, (0, 0), (1, 3));
    assert_eq!(harness.app.mouse.selection, None);
    let events: Vec<_> = harness
        .actions()
        .into_iter()
        .map(|action| match action {
            Action::MouseEvent { event } => event,
            other => panic!("expected a mouse event, got {:?}", other),
        })
        .collect();
    assert_eq!(events.len(), 4);
    assert_eq!(events[1].event_type, MouseEventType::Press);
    assert!(events[1].shift);
    assert_eq!(events[3].event_type, MouseEventType::Release);
}

#[test]
fn switching_selecting_off_drops_the_selection() {
    let mut harness = Harness::new(1, true, "");
    with_an_interface_on_screen(&mut harness);
    shift_drag(&mut harness, (0, 0), (1, 3));
    harness.reconfigure(TerminalConfig {
        shift_drag_selects: Some(false),
        ..TerminalConfig::default()
    });
    assert_eq!(harness.app.mouse.selection, None);
}

fn with_a_selection(harness: &mut Harness) {
    with_an_interface_on_screen(harness);
    shift_drag(harness, (0, 0), (1, 3));
    assert!(harness.app.mouse.selection.is_some());
}

#[test]
fn scrolling_clears_the_selection() {
    let mut harness = Harness::new(0, true, "");
    with_a_selection(&mut harness);
    harness
        .app
        .on_wheel(MouseScrollDelta::LineDelta(0.0, 1.0), TouchPhase::Moved);
    assert_eq!(harness.app.mouse.selection, None);
}

#[test]
fn resizing_clears_the_selection() {
    let mut harness = Harness::new(0, true, "");
    with_a_selection(&mut harness);
    harness.app.resized(640, 480);
    assert_eq!(harness.app.mouse.selection, None);
}

#[test]
fn zooming_clears_the_selection() {
    let mut harness = Harness::new(0, true, "");
    with_a_selection(&mut harness);
    harness.app.modifiers = ModifiersState::CONTROL;
    harness.press(&character("="));
    assert_eq!(harness.app.mouse.selection, None);
}

#[test]
fn a_scale_change_clears_the_selection() {
    let mut harness = Harness::new(0, true, "");
    with_a_selection(&mut harness);
    harness.app.rescale(2.0);
    assert_eq!(harness.app.mouse.selection, None);
}

#[test]
fn switching_sessions_clears_the_selection() {
    let mut harness = Harness::new(0, true, "");
    with_a_selection(&mut harness);
    harness.app.start_over(geometry());
    assert_eq!(harness.app.mouse.selection, None);
    assert!(!harness.app.mouse.selecting);
}

#[test]
fn pasting_clears_the_selection() {
    let mut harness = Harness::new(0, true, "pasted");
    with_a_selection(&mut harness);
    harness.app.modifiers = ModifiersState::CONTROL | ModifiersState::SHIFT;
    harness.press(&character("V"));
    assert_eq!(harness.app.mouse.selection, None);
}

#[test]
fn dropping_a_file_clears_the_selection() {
    let mut harness = Harness::new(0, true, "");
    with_a_selection(&mut harness);
    harness
        .app
        .on_window_event(WindowEvent::DroppedFile("/shots/shot.png".into()));
    assert_eq!(harness.app.mouse.selection, None);
}

#[test]
fn text_from_the_input_method_clears_the_selection() {
    let mut harness = Harness::new(0, true, "");
    with_a_selection(&mut harness);
    harness.app.on_ime(Ime::Preedit(String::new(), None));
    assert!(
        harness.app.mouse.selection.is_some(),
        "a composition in progress types nothing yet"
    );
    harness.app.on_ime(Ime::Commit("x".to_owned()));
    assert_eq!(harness.app.mouse.selection, None);
}

#[test]
fn losing_focus_mid_drag_keeps_what_was_marked_and_stops_extending_it() {
    let mut harness = Harness::new(0, true, "");
    with_an_interface_on_screen(&mut harness);
    let start = over_cell(&harness, 0, 0);
    harness.app.on_cursor_moved(start);
    harness.app.modifiers = ModifiersState::SHIFT;
    harness
        .app
        .on_mouse_input(MouseButton::Left, ElementState::Pressed);
    let middle = over_cell(&harness, 1, 3);
    harness.app.on_cursor_moved(middle);
    harness.app.on_focus_lost();
    harness.app.modifiers = ModifiersState::empty();
    let marked = Some(Selection::at(0, 0).extended_to(1, 3));
    assert_eq!(harness.app.mouse.selection, marked);
    assert!(!harness.app.mouse.selecting);

    let later = over_cell(&harness, 2, 5);
    harness.app.on_cursor_moved(later);
    assert_eq!(
        harness.app.mouse.selection, marked,
        "the drag ended with the focus"
    );
    copy_key(&mut harness);
    assert_eq!(
        harness.clipboard_text().as_deref(),
        Some("Tab #1  Tab #2\n│ he")
    );
}

#[test]
fn losing_focus_before_the_drag_moved_leaves_nothing_marked() {
    let mut harness = Harness::new(0, true, "");
    with_an_interface_on_screen(&mut harness);
    let start = over_cell(&harness, 0, 0);
    harness.app.on_cursor_moved(start);
    harness.app.modifiers = ModifiersState::SHIFT;
    harness
        .app
        .on_mouse_input(MouseButton::Left, ElementState::Pressed);
    harness.app.on_focus_lost();
    assert_eq!(harness.app.mouse.selection, None);
    assert!(!harness.app.mouse.selecting);
}

fn with_a_link_in_a_pane_watching_the_mouse(harness: &mut Harness) {
    use zellij_utils::structured_render::{
        GeometryRecord, PaneRect, PANE_FOCUSED, PANE_SELECTABLE, PANE_WANTS_MOUSE,
    };
    crate::testing::painter::Painter::apply(&mut harness.app.state, |painter| {
        painter.geometry(&GeometryRecord {
            panes: vec![PaneRect {
                x: 0,
                y: 0,
                cols: 120,
                rows: 40,
                top: 0,
                bottom: 0,
                left: 0,
                right: 0,
                flags: PANE_SELECTABLE | PANE_FOCUSED | PANE_WANTS_MOUSE,
            }],
        });
        painter.links(&[(1, "https://example.com")]);
        painter.linked(0, 0, "link", 1);
    });
    harness.app.on_cursor_moved(PhysicalPosition::new(4.0, 4.0));
    assert!(harness.app.pane_wants_mouse());
}

#[test]
fn a_shift_click_on_a_link_opens_it_in_a_pane_that_is_watching_the_mouse() {
    links::launch::forget_opened();
    let mut harness = Harness::new(1, true, "");
    with_a_link_in_a_pane_watching_the_mouse(&mut harness);
    shift_click(&mut harness);
    assert_eq!(opened_urls(), vec!["https://example.com".to_owned()]);
    assert_eq!(harness.app.mouse.selection, None);
    assert_eq!(
        harness.actions().len(),
        1,
        "only the pointer motion reaches the session"
    );
}

#[test]
fn a_plain_click_on_a_link_in_a_pane_watching_the_mouse_goes_to_the_pane_only() {
    links::launch::forget_opened();
    let mut harness = Harness::new(3, true, "");
    with_a_link_in_a_pane_watching_the_mouse(&mut harness);
    left_click(&mut harness);
    assert!(opened_urls().is_empty());
    assert_eq!(harness.actions().len(), 3);
}

fn with_a_link_under_a_popup(harness: &mut Harness) {
    use zellij_utils::structured_render::{
        GeometryRecord, PaneRect, PANE_FOCUSED, PANE_POPUP, PANE_SELECTABLE,
    };
    crate::testing::painter::Painter::apply(&mut harness.app.state, |painter| {
        painter.geometry(&GeometryRecord {
            panes: vec![
                PaneRect {
                    x: 0,
                    y: 0,
                    cols: 120,
                    rows: 40,
                    top: 0,
                    bottom: 0,
                    left: 0,
                    right: 0,
                    flags: PANE_SELECTABLE | PANE_FOCUSED,
                },
                PaneRect {
                    x: 0,
                    y: 0,
                    cols: 20,
                    rows: 5,
                    top: 0,
                    bottom: 0,
                    left: 0,
                    right: 0,
                    flags: PANE_POPUP,
                },
            ],
        });
        painter.links(&[(1, "https://example.com")]);
        painter.linked(0, 0, "link", 1);
    });
    harness.app.on_cursor_moved(PhysicalPosition::new(4.0, 4.0));
}

#[test]
fn over_a_popup_the_pointer_is_an_arrow_and_a_plain_click_goes_to_the_popup() {
    links::launch::forget_opened();
    let mut harness = Harness::new(3, true, "");
    with_a_link_under_a_popup(&mut harness);
    assert!(harness.app.pane_wants_mouse());
    assert_eq!(harness.app.pointer_shape(), CursorIcon::Default);
    left_click(&mut harness);
    assert!(opened_urls().is_empty());
    assert_eq!(harness.actions().len(), 3);
}

#[test]
fn a_shift_click_on_a_link_inside_a_popup_still_opens_it() {
    links::launch::forget_opened();
    let mut harness = Harness::new(1, true, "");
    with_a_link_under_a_popup(&mut harness);
    shift_click(&mut harness);
    assert_eq!(opened_urls(), vec!["https://example.com".to_owned()]);
}

#[test]
fn a_shift_drag_that_starts_on_a_link_selects_and_opens_nothing() {
    links::launch::forget_opened();
    let mut harness = Harness::new(1, true, "");
    with_a_link_under_the_pointer(&mut harness, "https://example.com");
    harness.app.modifiers = ModifiersState::SHIFT;
    harness
        .app
        .on_mouse_input(MouseButton::Left, ElementState::Pressed);
    assert!(
        harness.app.mouse.armed_link.is_some(),
        "until it moves, the press may still be a click"
    );
    let away = over_cell(&harness, 0, 9);
    harness.app.on_cursor_moved(away);
    assert!(harness.app.mouse.armed_link.is_none());
    harness
        .app
        .on_mouse_input(MouseButton::Left, ElementState::Released);
    harness.app.modifiers = ModifiersState::empty();
    assert!(opened_urls().is_empty());
    assert_eq!(
        harness.app.mouse.selection,
        Some(Selection::at(0, 0).extended_to(0, 9))
    );
    copy_key(&mut harness);
    assert_eq!(harness.clipboard_text().as_deref(), Some("link"));
    assert_eq!(harness.actions().len(), 1);
}

#[test]
fn a_shift_drag_off_a_link_and_back_onto_it_opens_nothing() {
    links::launch::forget_opened();
    let mut harness = Harness::new(0, true, "");
    with_a_link_under_the_pointer(&mut harness, "https://example.com");
    harness.app.modifiers = ModifiersState::SHIFT;
    harness
        .app
        .on_mouse_input(MouseButton::Left, ElementState::Pressed);
    let away = over_cell(&harness, 0, 9);
    harness.app.on_cursor_moved(away);
    harness.app.on_cursor_moved(PhysicalPosition::new(4.0, 4.0));
    harness
        .app
        .on_mouse_input(MouseButton::Left, ElementState::Released);
    harness.app.modifiers = ModifiersState::empty();
    assert!(opened_urls().is_empty());
    assert_eq!(harness.app.mouse.selection, None);
}

#[test]
fn a_middle_click_pastes_the_primary_selection_instead_of_being_forwarded() {
    let mut harness = Harness::new(1, true, "");
    harness.set_primary("selected");
    harness
        .app
        .on_mouse_input(MouseButton::Middle, ElementState::Pressed);
    harness
        .app
        .on_mouse_input(MouseButton::Middle, ElementState::Released);
    assert_eq!(
        harness.actions(),
        vec![Action::Paste {
            chars: "selected".to_owned(),
            pane_id: None
        }],
        "the release of a swallowed press must be swallowed with it"
    );
}

#[test]
fn a_middle_click_over_a_popup_reaches_the_popup_instead_of_pasting() {
    let mut harness = Harness::new(3, true, "");
    harness.set_primary("selected");
    with_a_link_under_a_popup(&mut harness);
    harness
        .app
        .on_mouse_input(MouseButton::Middle, ElementState::Pressed);
    harness
        .app
        .on_mouse_input(MouseButton::Middle, ElementState::Released);
    let actions = harness.actions();
    assert!(
        actions
            .iter()
            .all(|action| matches!(action, Action::MouseEvent { .. })),
        "nothing is pasted over a popup: {:?}",
        actions
    );
}

#[test]
fn a_middle_click_with_nothing_selected_reaches_the_session_as_before() {
    let mut harness = Harness::new(2, true, "");
    harness
        .app
        .on_mouse_input(MouseButton::Middle, ElementState::Pressed);
    harness
        .app
        .on_mouse_input(MouseButton::Middle, ElementState::Released);
    let events: Vec<_> = harness
        .actions()
        .into_iter()
        .map(|action| match action {
            Action::MouseEvent { event } => event,
            other => panic!("expected a mouse event, got {:?}", other),
        })
        .collect();
    assert_eq!(events[0].event_type, MouseEventType::Press);
    assert!(events[0].middle);
    assert_eq!(events[1].event_type, MouseEventType::Release);
}

#[test]
fn a_middle_click_reaches_the_session_when_the_option_is_off() {
    let mut harness = Harness::new(2, true, "");
    harness.set_primary("selected");
    harness.reconfigure(TerminalConfig {
        middle_click_paste: Some(false),
        ..TerminalConfig::default()
    });
    harness
        .app
        .on_mouse_input(MouseButton::Middle, ElementState::Pressed);
    harness
        .app
        .on_mouse_input(MouseButton::Middle, ElementState::Released);
    for action in harness.actions() {
        assert!(
            matches!(action, Action::MouseEvent { .. }),
            "a middle click was still swallowed: {:?}",
            action
        );
    }
}

#[test]
fn a_swallowed_middle_click_leaves_no_button_held() {
    let mut harness = Harness::new(1, true, "");
    harness.set_primary("selected");
    harness
        .app
        .on_mouse_input(MouseButton::Middle, ElementState::Pressed);
    harness.app.on_focus_lost();
    assert_eq!(
        harness.actions().len(),
        1,
        "losing focus released a button the session was never told was pressed"
    );
}

#[test]
fn the_left_button_is_untouched_by_the_middle_click_path() {
    let mut harness = Harness::new(1, true, "");
    harness.set_primary("selected");
    harness
        .app
        .on_mouse_input(MouseButton::Left, ElementState::Pressed);
    match &harness.actions()[0] {
        Action::MouseEvent { event } => assert!(event.left),
        other => panic!("expected a mouse event, got {:?}", other),
    }
}

#[test]
fn the_bell_mode_decides_what_a_bell_does() {
    use zellij_utils::input::terminal::BellMode;
    for (mode, attends, rings) in [
        (BellMode::Visual, true, false),
        (BellMode::Audible, false, true),
        (BellMode::Both, true, true),
        (BellMode::None, false, false),
    ] {
        let mut harness = Harness::new(1, true, "");
        harness.reconfigure(TerminalConfig {
            bell: Some(mode),
            ..TerminalConfig::default()
        });
        assert_eq!(harness.app.options.bell, mode);
        assert_eq!(harness.app.options.bell.attends(), attends, "{:?}", mode);
        assert_eq!(harness.app.options.bell.rings(), rings, "{:?}", mode);

        let rang_before = RINGS_COUNTED.load(Ordering::Relaxed);
        harness.app.ring = count_a_ring;
        let size = harness.app.state.size();
        let frame = frame_with_sideband(size.rows, size.cols, 0, b"\x07");
        harness.app.on_frame(&frame);
        assert_eq!(
            harness.app.state.take_bells(),
            0,
            "{:?} left the bell undrained",
            mode
        );
        assert_eq!(
            RINGS_COUNTED.load(Ordering::Relaxed) - rang_before,
            usize::from(rings),
            "{:?} rang the host's bell the wrong number of times",
            mode
        );
    }
}

#[test]
fn losing_focus_releases_a_held_button() {
    let mut harness = Harness::new(2, true, "");
    harness
        .app
        .on_mouse_input(MouseButton::Left, ElementState::Pressed);
    harness.app.on_focus_lost();
    match &harness.actions()[1] {
        Action::MouseEvent { event } => {
            assert_eq!(event.event_type, MouseEventType::Release);
            assert!(event.left);
        },
        other => panic!("expected a mouse event, got {:?}", other),
    }
}

mod composing {
    use super::*;
    use crate::input::composition::Gate;
    use winit::event::Ime;
    use zellij_utils::data::{BareKey, KeyWithModifier};

    fn preedit(text: &str) -> Ime {
        let end = text.len();
        Ime::Preedit(text.to_owned(), Some((end, end)))
    }

    fn dead(accent: char) -> Key {
        Key::Dead(Some(accent))
    }

    fn a_whole_composition(harness: &mut Harness) {
        for event in [
            Ime::Enabled,
            preedit("ni"),
            preedit("ni hao"),
            Ime::Commit("你好".to_owned()),
            Ime::Disabled,
        ] {
            harness.app.on_ime(event);
        }
    }

    #[test]
    fn a_composition_reaches_the_pane_as_its_committed_text_and_nothing_else() {
        let mut harness = Harness::new(1, true, "");
        a_whole_composition(&mut harness);
        assert_eq!(
            harness.actions(),
            vec![Action::WriteChars {
                chars: "你好".to_owned()
            }],
            "a pane is told the committed text once, and no preedit ever"
        );
    }

    #[test]
    fn a_commit_is_typed_rather_than_pasted() {
        let mut harness = Harness::new(1, true, "");
        a_whole_composition(&mut harness);
        match &harness.sent()[0] {
            ClientToServerMsg::Action {
                action: Action::WriteChars { chars },
                ..
            } => {
                assert_eq!(chars, "你好");
                assert!(
                    !chars.contains('\u{1b}'),
                    "typed text must carry no bracketed-paste markers"
                );
            },
            other => panic!("expected typed text, got {:?}", other),
        }
    }

    #[test]
    fn a_single_character_commit_is_a_key_press_so_keybindings_see_it() {
        let mut harness = Harness::new(1, true, "");
        harness.app.on_ime(Ime::Preedit(String::new(), None));
        harness.app.on_ime(Ime::Commit("t".to_owned()));
        assert_eq!(
            harness.sent(),
            vec![ClientToServerMsg::Key {
                key: KeyWithModifier::new(BareKey::Char('t')),
                raw_bytes: b"t".to_vec(),
                is_kitty_keyboard_protocol: true,
            }],
            "an input method that commits ordinary letters must not bypass the session's modes"
        );
    }

    #[test]
    fn a_multi_character_key_is_typed_rather_than_pasted_too() {
        let mut harness = Harness::new(1, true, "");
        harness.press(&character("ab"));
        assert_eq!(
            harness.actions(),
            vec![Action::WriteChars {
                chars: "ab".to_owned()
            }],
            "a compose result the layout finished is typed text, like a commit"
        );
    }

    #[test]
    fn a_preedit_never_reaches_the_pane() {
        let mut harness = Harness::new(0, true, "");
        harness.app.on_ime(Ime::Enabled);
        harness.app.on_ime(preedit("ni"));
        harness.app.on_ime(preedit("ni hao"));
        assert!(
            harness.sent().is_empty(),
            "a half-composed syllable must not reach the shell"
        );
    }

    #[test]
    fn a_chord_pressed_mid_composition_neither_zooms_nor_reaches_the_pane() {
        let mut harness = Harness::new(0, true, "");
        harness.app.on_ime(preedit("ni"));
        harness.app.modifiers = ModifiersState::CONTROL;
        harness.press(&character("="));
        assert_eq!(harness.app.zoom, 1.0, "a composing key zoomed the window");
        assert!(harness.sent().is_empty());
    }

    #[test]
    fn a_dead_key_and_the_character_it_composed_reach_the_pane_once() {
        let mut harness = Harness::new(1, true, "");
        harness.press(&dead('`'));
        harness.press(&character("à"));
        let sent = harness.sent();
        assert_eq!(sent.len(), 1, "got {:?}", sent);
        match &sent[0] {
            ClientToServerMsg::Key { key, raw_bytes, .. } => {
                assert_eq!(*key, KeyWithModifier::new(BareKey::Char('à')));
                assert_eq!(raw_bytes, "à".as_bytes());
            },
            other => panic!("expected a key, got {:?}", other),
        }
    }

    #[test]
    fn a_chord_is_suspended_for_the_key_that_finishes_a_dead_key() {
        let mut harness = Harness::new(1, true, "");
        harness.press(&dead('`'));
        harness.app.modifiers = ModifiersState::CONTROL;
        harness.press(&character("="));
        assert_eq!(
            harness.app.zoom, 1.0,
            "the second half of a composition is not a chord"
        );
        assert_eq!(harness.sent().len(), 1);
    }

    #[test]
    fn a_dead_key_on_its_own_sends_nothing_and_shows_the_accent() {
        let mut harness = Harness::new(0, true, "");
        harness.press(&dead('`'));
        assert_eq!(
            harness
                .app
                .ime
                .composition
                .shown()
                .map(|shown| shown.text().to_owned()),
            Some("`".to_owned())
        );
        assert!(harness.sent().is_empty());
    }

    fn press_with_text(harness: &mut Harness, logical: Key, text: &str) {
        let modifiers = harness.app.modifiers;
        harness
            .app
            .press(keyboard::Press::new(logical, modifiers).with_text(text));
    }

    fn typed_key(character: char) -> ClientToServerMsg {
        ClientToServerMsg::Key {
            key: KeyWithModifier::new(BareKey::Char(character)),
            raw_bytes: character.to_string().into_bytes(),
            is_kitty_keyboard_protocol: true,
        }
    }

    fn typed_text(chars: &str) -> ClientToServerMsg {
        ClientToServerMsg::Action {
            action: Action::WriteChars {
                chars: chars.to_owned(),
            },
            terminal_id: None,
            client_id: None,
            is_cli_client: false,
        }
    }

    #[test]
    fn space_after_a_dead_key_sends_the_accent_and_clears_it() {
        let mut harness = Harness::new(1, true, "");
        harness.press(&dead('^'));
        press_with_text(&mut harness, Key::Named(NamedKey::Space), "^");
        assert_eq!(harness.app.ime.composition.shown(), None);
        assert_eq!(harness.app.ime.composition.gate(), Gate::Open);
        assert_eq!(harness.sent(), vec![typed_key('^')]);
    }

    #[test]
    fn a_dead_key_pressed_twice_sends_what_the_system_produced() {
        let mut harness = Harness::new(1, true, "");
        harness.press(&dead('^'));
        press_with_text(&mut harness, dead('^'), "^^");
        assert_eq!(harness.app.ime.composition.shown(), None);
        assert_eq!(harness.sent(), vec![typed_text("^^")]);
    }

    #[test]
    fn an_accent_that_cannot_combine_reaches_the_pane_with_the_letter() {
        let mut harness = Harness::new(1, true, "");
        harness.press(&dead('^'));
        press_with_text(&mut harness, character("x"), "^x");
        assert_eq!(harness.sent(), vec![typed_text("^x")]);
    }

    #[test]
    fn enter_after_a_dead_key_sends_the_accent_then_enter() {
        let mut harness = Harness::new(2, true, "");
        harness.press(&dead('^'));
        press_with_text(&mut harness, Key::Named(NamedKey::Enter), "^\r");
        let sent = harness.sent();
        assert_eq!(sent[0], typed_key('^'));
        match &sent[1] {
            ClientToServerMsg::Key { key, raw_bytes, .. } => {
                assert_eq!(*key, KeyWithModifier::new(BareKey::Enter));
                assert_eq!(raw_bytes, b"\r");
            },
            other => panic!("expected enter, got {:?}", other),
        }
    }

    #[test]
    fn a_cancelled_accent_with_nothing_produced_sends_nothing() {
        let mut harness = Harness::new(0, true, "");
        harness.press(&dead('^'));
        harness.press(&Key::Unidentified(winit::keyboard::NativeKey::Unidentified));
        assert_eq!(harness.app.ime.composition.shown(), None);
        assert_eq!(harness.app.ime.composition.gate(), Gate::Open);
        assert!(harness.sent().is_empty());
    }

    #[test]
    fn a_shortcut_after_a_dead_key_still_reaches_the_pane_as_a_shortcut() {
        let mut harness = Harness::new(1, true, "");
        harness.press(&dead('^'));
        harness.app.modifiers = ModifiersState::CONTROL;
        press_with_text(&mut harness, character("c"), "^c");
        match &harness.sent()[0] {
            ClientToServerMsg::Key { raw_bytes, .. } => {
                assert_eq!(raw_bytes, b"\x1b[99;5u")
            },
            other => panic!("expected ctrl-c, got {:?}", other),
        }
    }

    #[test]
    fn a_character_picker_entry_reaches_the_pane() {
        let mut harness = Harness::new(1, true, "");
        press_with_text(
            &mut harness,
            Key::Unidentified(winit::keyboard::NativeKey::Unidentified),
            "′",
        );
        assert_eq!(harness.sent(), vec![typed_key('′')]);
    }

    #[test]
    fn typing_resumes_normally_after_an_accent_is_resolved() {
        let mut harness = Harness::new(2, true, "");
        harness.press(&dead('^'));
        press_with_text(&mut harness, Key::Named(NamedKey::Space), "^");
        harness.press(&character("a"));
        assert_eq!(harness.sent(), vec![typed_key('^'), typed_key('a')]);
    }

    fn shown_accent(harness: &Harness) -> Option<String> {
        harness
            .app
            .ime
            .composition
            .shown()
            .map(|shown| shown.text().to_owned())
    }

    #[test]
    fn shift_held_for_the_next_key_keeps_the_accent() {
        let mut harness = Harness::new(1, true, "");
        harness.press(&dead('^'));
        harness.app.modifiers = ModifiersState::SHIFT;
        for _ in 0..3 {
            harness.press(&Key::Named(NamedKey::Shift));
        }
        assert_eq!(
            shown_accent(&harness),
            Some("^".to_owned()),
            "the system still holds the accent while shift is down"
        );
        press_with_text(&mut harness, character("/"), "^/");
        assert_eq!(harness.app.ime.composition.shown(), None);
        assert_eq!(harness.sent(), vec![typed_text("^/")]);
    }

    #[test]
    fn altgr_held_for_the_next_key_keeps_the_accent() {
        let mut harness = Harness::new(1, true, "");
        harness.press(&dead('^'));
        for named in [
            NamedKey::AltGraph,
            NamedKey::AltGraph,
            NamedKey::Control,
            NamedKey::AltGraph,
        ] {
            harness.press(&Key::Named(named));
        }
        assert_eq!(
            shown_accent(&harness),
            Some("^".to_owned()),
            "windows repeats altgr with the control press it fakes for it"
        );
        press_with_text(&mut harness, character("\\"), "^\\");
        assert_eq!(harness.sent(), vec![typed_text("^\\")]);
    }

    #[test]
    fn a_shifted_letter_still_combines_with_the_accent() {
        let mut harness = Harness::new(1, true, "");
        harness.press(&dead('^'));
        harness.app.modifiers = ModifiersState::SHIFT;
        harness.press(&Key::Named(NamedKey::Shift));
        press_with_text(&mut harness, character("Ê"), "Ê");
        assert_eq!(harness.sent(), vec![typed_key('Ê')]);
    }

    fn is_left(msg: &ClientToServerMsg) -> bool {
        matches!(
            msg,
            ClientToServerMsg::Key { key, .. } if *key == KeyWithModifier::new(BareKey::Left)
        )
    }

    #[test]
    fn an_accent_the_system_kept_through_an_arrow_still_reaches_the_pane() {
        let mut harness = Harness::new(2, true, "");
        harness.press(&dead('^'));
        harness.press(&Key::Named(NamedKey::ArrowLeft));
        press_with_text(&mut harness, character("p"), "^p");
        let sent = harness.sent();
        assert!(is_left(&sent[0]), "{:?}", sent);
        assert_eq!(sent[1], typed_text("^p"));
    }

    #[test]
    fn a_letter_combined_after_an_arrow_arrives_composed() {
        let mut harness = Harness::new(2, true, "");
        harness.press(&dead('^'));
        harness.press(&Key::Named(NamedKey::ArrowLeft));
        press_with_text(&mut harness, character("ê"), "ê");
        let sent = harness.sent();
        assert!(is_left(&sent[0]), "{:?}", sent);
        assert_eq!(sent[1], typed_key('ê'));
    }

    #[test]
    fn losing_focus_mid_composition_leaves_no_residue() {
        let mut harness = Harness::new(0, true, "");
        harness.app.on_ime(preedit("ni"));
        harness.press(&dead('`'));
        harness.app.stop_composing();
        harness.app.on_focus_lost();
        assert_eq!(harness.app.ime.composition.shown(), None);
        assert_eq!(harness.app.ime.composed_row, None);
        assert!(harness.sent().is_empty());
    }

    #[test]
    fn a_disabled_ime_mid_composition_sends_nothing() {
        let mut harness = Harness::new(0, true, "");
        harness.app.on_ime(preedit("ni"));
        harness.app.on_ime(Ime::Disabled);
        assert_eq!(harness.app.ime.composition.shown(), None);
        assert!(harness.sent().is_empty());
    }

    #[test]
    fn an_empty_commit_is_not_sent() {
        let mut harness = Harness::new(0, true, "");
        harness.app.on_ime(preedit("ni"));
        harness.app.on_ime(Ime::Commit(String::new()));
        assert!(harness.sent().is_empty());
    }
}

mod damage {
    use super::*;
    use crate::emulation::damage::Damage;
    use crate::testing::painter;
    use winit::event::Ime;

    fn rebuilt(harness: &mut Harness) -> (bool, Vec<usize>) {
        let cursor = harness.app.cursor_options();
        let (state, cache, retained, blink, paints, hovered) = (
            &harness.app.state,
            &mut harness.app.cache,
            &mut harness.app.retained,
            harness.app.timing.blink,
            &harness.app.options.paints,
            harness.app.mouse.hovered_link.as_ref(),
        );
        let preedit = harness.app.ime.composition.shown();
        retained.refresh(
            SceneInputs {
                state: state,
                phase: blink,
                paints: paints,
                cursor: cursor,
                hovered_link: hovered,
                preedit: preedit.as_ref(),
            },
            cache,
        );
        (
            retained.rebuilt_everything(),
            retained.rebuilt_rows().to_vec(),
        )
    }

    fn painted(harness: &mut Harness, paint: impl FnOnce(&mut painter::Painter)) {
        let size = harness.app.state.size();
        let frame = painter::Painter::frame(size.rows, size.cols, paint);
        harness.app.on_frame(&frame);
    }

    #[test]
    fn a_frame_damages_the_rows_it_writes_and_nothing_else() {
        let mut harness = Harness::new(4, true, "");
        let _ = rebuilt(&mut harness);
        painted(&mut harness, |painter| painter.text(3, 0, "typed"));
        assert_eq!(rebuilt(&mut harness), (false, vec![3]));
    }

    #[test]
    fn a_redraw_with_nothing_marked_rebuilds_no_row_at_all() {
        let mut harness = Harness::new(4, true, "");
        let _ = rebuilt(&mut harness);
        assert_eq!(rebuilt(&mut harness), (false, Vec::new()));
    }

    #[test]
    fn a_blink_flip_damages_only_the_rows_that_carry_a_blinking_cell() {
        let mut harness = Harness::new(4, true, "");
        painted(&mut harness, |painter| {
            painter.text(0, 0, "steady");
            painter.styled(2, 0, "blinks", |cell| {
                cell.attrs |= zellij_utils::structured_render::ATTR_SLOW_BLINK
            });
        });
        let _ = rebuilt(&mut harness);
        harness.app.advance_blink();
        assert_eq!(rebuilt(&mut harness), (false, vec![2]));
    }

    #[test]
    fn hovering_a_link_damages_the_rows_the_underline_appears_and_disappears_on() {
        let mut harness = Harness::new(4, true, "");
        with_a_link_under_the_pointer(&mut harness, "https://example.com");
        let _ = rebuilt(&mut harness);
        harness
            .app
            .on_cursor_moved(PhysicalPosition::new(45.0, 4.0));
        assert_eq!(rebuilt(&mut harness), (false, vec![0]));
    }

    #[test]
    fn a_preedit_damages_the_row_the_cursor_sits_on_and_no_other() {
        let mut harness = Harness::new(0, true, "");
        painted(&mut harness, |painter| {
            painter.text(0, 0, "prompt");
            painter.cursor(0, 6, crate::emulation::screen_buffer::CursorShape::Block);
        });
        let _ = rebuilt(&mut harness);
        harness
            .app
            .on_ime(Ime::Preedit("ni".to_owned(), Some((2, 2))));
        assert_eq!(rebuilt(&mut harness), (false, vec![0]));
    }

    #[test]
    fn a_composition_that_ends_damages_the_row_it_was_drawn_on() {
        let mut harness = Harness::new(1, true, "");
        painted(&mut harness, |painter| {
            painter.text(2, 0, "prompt");
            painter.cursor(2, 6, crate::emulation::screen_buffer::CursorShape::Block);
        });
        harness
            .app
            .on_ime(Ime::Preedit("ni".to_owned(), Some((2, 2))));
        let _ = rebuilt(&mut harness);
        harness.app.on_ime(Ime::Commit("你".to_owned()));
        assert_eq!(rebuilt(&mut harness), (false, vec![2]));
    }

    #[test]
    fn a_theme_change_damages_every_row() {
        let mut harness = Harness::new(4, true, "");
        let _ = rebuilt(&mut harness);
        harness.app.retained.mark(&Damage::Everything);
        assert!(rebuilt(&mut harness).0);
    }
}

mod pacing {
    use super::*;

    fn painted(harness: &mut Harness, row: usize, text: &str) {
        let size = harness.app.state.size();
        let frame = crate::testing::painter::Painter::frame(size.rows, size.cols, |painter| {
            painter.text(row, 0, text)
        });
        harness.app.on_frame(&frame);
    }

    fn rebuilt_rows(harness: &mut Harness) -> Vec<usize> {
        let cursor = harness.app.cursor_options();
        let (state, cache, retained, blink, paints, hovered) = (
            &harness.app.state,
            &mut harness.app.cache,
            &mut harness.app.retained,
            harness.app.timing.blink,
            &harness.app.options.paints,
            harness.app.mouse.hovered_link.as_ref(),
        );
        let preedit = harness.app.ime.composition.shown();
        retained.refresh(
            SceneInputs {
                state: state,
                phase: blink,
                paints: paints,
                cursor: cursor,
                hovered_link: hovered,
                preedit: preedit.as_ref(),
            },
            cache,
        );
        retained.rebuilt_rows().to_vec()
    }

    #[test]
    fn frames_applied_between_two_draws_are_coalesced_into_a_single_draw() {
        let mut harness = Harness::new(3, true, "");
        let _ = rebuilt_rows(&mut harness);
        let drawn = Instant::now();
        harness.app.timing.pacer.drawn(drawn);

        painted(&mut harness, 1, "one");
        painted(&mut harness, 2, "two");
        painted(&mut harness, 3, "three");

        let interval = harness.app.timing.pacer.interval();
        assert_eq!(
            harness
                .app
                .timing
                .pacer
                .decide(drawn + Duration::from_micros(500)),
            Decision::At(drawn + interval),
            "three frames inside one interval must not ask for three draws"
        );
        assert_eq!(
            rebuilt_rows(&mut harness),
            vec![1, 2, 3],
            "the one draw owed must carry every row the coalesced frames wrote"
        );
        assert_eq!(
            harness.app.timing.pacer.decide(drawn + interval),
            Decision::Now
        );
    }

    #[test]
    fn every_coalesced_frame_is_still_acknowledged_on_apply() {
        let mut harness = Harness::new(3, true, "");
        harness.app.timing.pacer.drawn(Instant::now());
        painted(&mut harness, 1, "one");
        painted(&mut harness, 2, "two");
        painted(&mut harness, 3, "three");
        assert_eq!(
            harness.sent().len(),
            3,
            "pacing the display must not pace delivery"
        );
    }

    #[test]
    fn a_keystroke_on_a_quiet_screen_draws_at_the_next_opportunity() {
        let mut harness = Harness::new(2, true, "");
        let idle_since = Instant::now() - Duration::from_secs(1);
        harness.app.timing.pacer.drawn(idle_since);
        painted(&mut harness, 0, "a");
        assert_eq!(
            harness.app.timing.pacer.decide(Instant::now()),
            Decision::Now
        );
    }

    #[test]
    fn a_window_starts_out_pacing_at_the_fallback_rate() {
        let harness = Harness::new(0, true, "");
        assert_eq!(
            harness.app.timing.pacer.interval(),
            crate::render::pacing::interval_of(Some(60_000))
        );
    }
}

mod smooth_scrolling {
    use super::*;
    use zellij_utils::structured_render::{
        FrameBuilder, GeometryRecord, PaneRect, ScrollEntry, ScrollRecord, WireCell, PANE_FRAMED,
    };

    fn whole_pane() -> GeometryRecord {
        GeometryRecord {
            panes: vec![PaneRect {
                x: 0,
                y: 0,
                cols: 120,
                rows: 40,
                top: 1,
                bottom: 1,
                left: 1,
                right: 1,
                flags: PANE_FRAMED,
            }],
        }
    }

    fn frame(seq: u64, first: bool, base: usize, hint: Option<i16>) -> Vec<u8> {
        let mut builder = FrameBuilder::new(120, 40, seq);
        builder.set_full_repaint(first);
        builder.set_clear(first);
        for row in 1..39 {
            let text = format!("line {}", base + row);
            let cells: Vec<WireCell> = text
                .chars()
                .map(|character| WireCell {
                    ch: character as u32,
                    ..WireCell::BLANK
                })
                .collect();
            builder.push_row(row as u16, 1, &cells);
        }
        if first {
            builder.push_geometry(&whole_pane());
        }
        if let Some(lines) = hint {
            builder.push_scroll(&ScrollRecord {
                entries: vec![ScrollEntry { pane: 0, lines }],
            });
        }
        builder.finish()
    }

    fn scrolled(expected: usize) -> Harness {
        let mut harness = Harness::new(expected, true, "");
        harness.app.on_frame(&frame(1, true, 100, None));
        harness.app.on_frame(&frame(2, false, 97, Some(3)));
        harness
    }

    #[test]
    fn a_scroll_hint_starts_an_animation_that_draws_every_refresh_until_it_ends() {
        let mut harness = scrolled(2);
        assert!(harness.app.scroll.is_active());
        let start = Instant::now();
        harness.app.timing.pacer.drawn(start);
        harness.app.advance_scroll_animations(start);
        assert_eq!(
            harness.app.timing.pacer.decide(start),
            Decision::At(start + harness.app.timing.pacer.interval()),
            "a running slide must ask for the next refresh"
        );

        let after = start + Duration::from_millis(500);
        harness.app.advance_scroll_animations(after);
        assert!(!harness.app.scroll.is_active());
        harness.app.timing.pacer.drawn(after);
        harness.app.advance_scroll_animations(after);
        assert_eq!(
            harness.app.timing.pacer.decide(after),
            Decision::Idle,
            "nothing is drawn once every slide has ended"
        );
    }

    #[test]
    fn a_frame_without_a_hint_starts_nothing() {
        let mut harness = Harness::new(2, true, "");
        harness.app.on_frame(&frame(1, true, 100, None));
        harness.app.on_frame(&frame(2, false, 97, None));
        assert!(!harness.app.scroll.is_active());
    }

    #[test]
    fn turning_smooth_scrolling_off_ends_a_running_slide_at_its_final_position() {
        let mut harness = scrolled(2);
        assert!(harness.app.scroll.is_active());
        harness.reconfigure(TerminalConfig {
            smooth_scrolling: Some(false),
            ..TerminalConfig::default()
        });
        assert!(!harness.app.scroll.is_active());
        assert!(!harness.app.options.smooth_scrolling);

        harness.app.on_frame(&frame(3, false, 95, Some(2)));
        assert!(!harness.app.scroll.is_active());
    }

    #[test]
    fn the_configured_duration_is_used_after_a_reload() {
        let mut harness = Harness::new(2, true, "");
        harness.reconfigure(TerminalConfig {
            scroll_animation_duration: Some(400),
            ..TerminalConfig::default()
        });
        harness.app.on_frame(&frame(1, true, 100, None));
        let before = Instant::now();
        harness.app.on_frame(&frame(2, false, 97, Some(3)));
        assert!(harness.app.scroll.is_active());
        assert!(!harness
            .app
            .scroll
            .retire(before + Duration::from_millis(300)));
        assert!(harness
            .app
            .scroll
            .retire(Instant::now() + Duration::from_millis(400)));
    }

    #[test]
    fn a_configuration_reload_or_theme_change_cancels_the_slide() {
        let mut harness = scrolled(3);
        harness.reconfigure(TerminalConfig::default());
        assert!(!harness.app.scroll.is_active());

        harness.app.on_frame(&frame(3, false, 95, Some(2)));
        assert!(harness.app.scroll.is_active());
        harness.app.theme_mode = Some(HostTerminalThemeMode::Dark);
        harness.app.reapply(false);
        assert!(!harness.app.scroll.is_active());
    }
}

mod scroll_momentum {
    use super::*;
    use crate::input::mouse;
    use crate::input::mouse::PointerState;
    use winit::event::ElementState;
    use winit::event::Ime;
    use winit::event::TouchPhase;
    use zellij_utils::input::mouse::MouseEvent;
    use zellij_utils::structured_render::{
        FrameBuilder, GeometryRecord, PaneRect, ScrollEntry, ScrollRecord, PANE_FRAMED,
        PANE_SELECTABLE,
    };

    fn pane(x: u16) -> PaneRect {
        PaneRect {
            x,
            y: 0,
            cols: 60,
            rows: 40,
            top: 1,
            bottom: 1,
            left: 1,
            right: 1,
            flags: PANE_FRAMED | PANE_SELECTABLE,
        }
    }

    fn frame(seq: u64, first: bool, hints: &[(u16, i16)]) -> Vec<u8> {
        let mut builder = FrameBuilder::new(120, 40, seq);
        builder.set_full_repaint(first);
        builder.set_clear(first);
        if first {
            builder.push_geometry(&GeometryRecord {
                panes: vec![pane(0), pane(60)],
            });
        }
        builder.push_scroll(&ScrollRecord {
            entries: hints
                .iter()
                .map(|&(pane, lines)| ScrollEntry { pane, lines })
                .collect(),
        });
        builder.finish()
    }

    fn ms(millis: u64) -> Duration {
        Duration::from_millis(millis)
    }

    fn pixels(y: f64) -> MouseScrollDelta {
        MouseScrollDelta::PixelDelta(at(0.0, y))
    }

    const OVER_LEFT: (f64, f64) = (100.0, 200.0);
    const OVER_RIGHT: (f64, f64) = (800.0, 200.0);

    struct Fling {
        harness: Harness,
        seq: u64,
        start: Instant,
    }

    impl Fling {
        fn offline(wayland: bool) -> Self {
            Self::new(0, wayland, false)
        }

        fn wired(expected: usize, wayland: bool) -> Self {
            Self::new(expected, wayland, true)
        }

        fn new(expected: usize, wayland: bool, wired: bool) -> Self {
            let mut harness = Harness::new(expected, true, "");
            harness.app.wayland = wayland;
            if !wired {
                harness.app.sender = None;
            }
            let mut fling = Self {
                harness,
                seq: 0,
                start: Instant::now(),
            };
            fling.frame(true, &[]);
            fling
                .harness
                .app
                .on_cursor_moved(at(OVER_LEFT.0, OVER_LEFT.1));
            fling
        }

        fn frame(&mut self, first: bool, hints: &[(u16, i16)]) {
            self.seq += 1;
            self.harness.app.on_frame(&frame(self.seq, first, hints));
        }

        fn wheel(&mut self, delta: MouseScrollDelta, phase: TouchPhase, after: u64) {
            self.harness
                .app
                .on_wheel_at(delta, phase, self.start + ms(after));
        }

        fn swipe(&mut self, step: f64, hinted: bool) {
            self.wheel(pixels(step), TouchPhase::Started, 0);
            for index in 1..=5 {
                self.wheel(pixels(step), TouchPhase::Moved, 10 * index);
            }
            if hinted {
                self.frame(false, &[(0, 2)]);
            }
            self.wheel(pixels(0.0), TouchPhase::Ended, 55);
        }

        fn tick(&mut self, after: u64) -> Vec<MouseEvent> {
            self.harness.app.tick_momentum(self.start + ms(after))
        }

        fn coasting(&self) -> bool {
            self.harness.app.momentum.is_coasting()
        }

        fn coasting_fling(wayland: bool) -> Self {
            let mut fling = Self::offline(wayland);
            fling.swipe(40.0, true);
            fling
        }

        fn assert_stopped(&mut self, why: &str) {
            assert!(!self.coasting(), "{}", why);
            assert!(self.tick(400).is_empty(), "{}", why);
            assert!(self.tick(2000).is_empty(), "{}", why);
        }
    }

    #[test]
    fn a_fast_swipe_through_scrollback_keeps_scrolling_after_the_lift() {
        let mut fling = Fling::coasting_fling(true);
        assert!(fling.coasting());
        let events = fling.tick(100);
        assert_eq!(events.len(), 1);
        assert!(events[0].wheel_up && events[0].wheel_lines > 0);
        assert_eq!(
            events[0].position,
            zellij_utils::position::Position::new(10, 10),
            "the glide scrolls where the swipe ended"
        );
    }

    #[test]
    fn a_downward_swipe_glides_downward() {
        let mut fling = Fling::offline(true);
        fling.swipe(-40.0, true);
        let events = fling.tick(100);
        assert!(events[0].wheel_down && events[0].wheel_lines > 0);
    }

    #[test]
    fn the_glide_sends_whole_lines_slows_down_and_ends() {
        let mut fling = Fling::coasting_fling(true);
        let mut lines = Vec::new();
        let mut after = 55;
        while fling.coasting() {
            after += 16;
            let events = fling.tick(after);
            for event in &events {
                assert!(event.wheel_up && event.wheel_lines > 0);
                lines.push((after, event.wheel_lines as u64));
            }
            if !events.is_empty() {
                fling.frame(false, &[(0, 1)]);
            }
        }
        let total: u64 = lines.iter().map(|(_, count)| count).sum();
        assert_eq!(total, 97, "(4000 - 100) px / 2 / 20 px per line");
        let early: u64 = lines
            .iter()
            .filter(|(at, _)| *at < 555)
            .map(|(_, count)| count)
            .sum();
        assert!(early > total - early, "the glide slows down");
        assert!(fling.tick(after + 16).is_empty());
    }

    #[test]
    fn a_slow_swipe_or_a_bare_lift_starts_nothing() {
        let mut fling = Fling::offline(true);
        fling.swipe(3.0, true);
        assert!(!fling.coasting());

        let mut fling = Fling::offline(true);
        fling.frame(false, &[(0, 2)]);
        fling.wheel(pixels(0.0), TouchPhase::Ended, 0);
        assert!(!fling.coasting());
    }

    #[test]
    fn a_swipe_the_server_answered_without_a_hint_starts_nothing() {
        let mut fling = Fling::offline(true);
        fling.swipe(40.0, false);
        assert!(
            !fling.coasting(),
            "a full-screen program or a plugin pane produces no scroll hint"
        );

        let mut fling = Fling::offline(true);
        fling.wheel(pixels(40.0), TouchPhase::Started, 0);
        fling.wheel(pixels(40.0), TouchPhase::Moved, 10);
        fling.frame(false, &[(1, 2)]);
        fling.wheel(pixels(0.0), TouchPhase::Ended, 15);
        assert!(!fling.coasting(), "a hint for another pane is no evidence");
    }

    #[test]
    fn the_scrollback_running_out_stops_the_glide_once_the_frame_says_so() {
        let mut fling = Fling::coasting_fling(true);
        assert!(!fling.tick(100).is_empty());
        assert!(!fling.tick(130).is_empty());
        assert!(
            fling.coasting(),
            "no frame has answered the scroll yet, so nothing is decided"
        );
        fling.frame(false, &[(0, 3)]);
        assert!(fling.coasting());
        assert!(!fling.tick(160).is_empty());
        fling.frame(false, &[]);
        fling.assert_stopped("a frame without a hint after a sent scroll");
    }

    #[test]
    fn every_interruption_stops_the_glide_with_nothing_further_sent() {
        let interruptions: Vec<(&str, Box<dyn Fn(&mut Fling)>)> = vec![
            (
                "fingers back down",
                Box::new(|f: &mut Fling| f.wheel(pixels(0.0), TouchPhase::Started, 80)),
            ),
            (
                "fingers moving",
                Box::new(|f: &mut Fling| f.wheel(pixels(1.0), TouchPhase::Moved, 80)),
            ),
            (
                "a mouse wheel",
                Box::new(|f: &mut Fling| {
                    f.wheel(MouseScrollDelta::LineDelta(0.0, 1.0), TouchPhase::Moved, 80)
                }),
            ),
            (
                "a mouse button",
                Box::new(|f: &mut Fling| {
                    f.harness
                        .app
                        .on_mouse_input(MouseButton::Left, ElementState::Pressed)
                }),
            ),
            (
                "a key sent to the session",
                Box::new(|f: &mut Fling| f.harness.press(&character("a"))),
            ),
            (
                "a window key",
                Box::new(|f: &mut Fling| {
                    f.harness.app.modifiers = ModifiersState::CONTROL | ModifiersState::SHIFT;
                    f.harness.press(&character("V"))
                }),
            ),
            (
                "a font zoom",
                Box::new(|f: &mut Fling| {
                    f.harness.app.modifiers = ModifiersState::CONTROL;
                    f.harness.press(&character("="))
                }),
            ),
            (
                "text from the input method",
                Box::new(|f: &mut Fling| {
                    f.harness.app.on_ime(Ime::Preedit(String::new(), None));
                    f.harness.app.on_ime(Ime::Commit("x".to_owned()));
                }),
            ),
            (
                "focus loss",
                Box::new(|f: &mut Fling| f.harness.app.on_focus_lost()),
            ),
            (
                "the pointer leaving",
                Box::new(|f: &mut Fling| f.harness.app.on_cursor_left()),
            ),
            (
                "a resize",
                Box::new(|f: &mut Fling| f.harness.app.resized(640, 480)),
            ),
            (
                "a scale change",
                Box::new(|f: &mut Fling| f.harness.app.rescale(2.0)),
            ),
            (
                "a configuration reload",
                Box::new(|f: &mut Fling| f.harness.reconfigure(TerminalConfig::default())),
            ),
            (
                "a session switch",
                Box::new(|f: &mut Fling| {
                    f.harness.app.follow(None, None);
                }),
            ),
            (
                "the pointer moving to another pane",
                Box::new(|f: &mut Fling| {
                    f.harness
                        .app
                        .on_cursor_moved(at(OVER_RIGHT.0, OVER_RIGHT.1))
                }),
            ),
        ];
        for (why, interrupt) in interruptions {
            let mut fling = Fling::coasting_fling(true);
            assert!(!fling.tick(70).is_empty(), "{}", why);
            fling.frame(false, &[(0, 1)]);
            assert!(fling.coasting(), "{}", why);
            interrupt(&mut fling);
            fling.assert_stopped(why);
        }
    }

    #[test]
    fn moving_within_the_same_pane_keeps_the_glide() {
        let mut fling = Fling::coasting_fling(true);
        fling.harness.app.on_cursor_moved(at(150.0, 300.0));
        assert!(fling.coasting());
    }

    #[test]
    fn turning_momentum_off_stops_it_and_keeps_it_off() {
        let mut fling = Fling::coasting_fling(true);
        fling.harness.reconfigure(TerminalConfig {
            scroll_momentum: Some(false),
            ..TerminalConfig::default()
        });
        fling.assert_stopped("momentum switched off");
        fling.swipe(40.0, true);
        assert!(!fling.coasting());
    }

    #[test]
    fn momentum_works_with_smooth_scrolling_off() {
        let mut fling = Fling::offline(true);
        fling.harness.reconfigure(TerminalConfig {
            smooth_scrolling: Some(false),
            ..TerminalConfig::default()
        });
        fling.swipe(40.0, true);
        assert!(fling.coasting());
        assert!(!fling.tick(100).is_empty());
    }

    #[test]
    fn a_running_glide_asks_for_a_wake_up_and_an_idle_one_asks_for_nothing() {
        let mut fling = Fling::offline(true);
        let now = Instant::now();
        assert_eq!(fling.harness.app.advance_momentum(now), None);
        fling.swipe(40.0, true);
        let interval = fling.harness.app.timing.pacer.interval();
        let now = fling.start + ms(80);
        fling.harness.app.timing.pacer.drawn(now);
        assert_eq!(
            fling.harness.app.advance_momentum(now),
            Some(now + interval)
        );
        assert_eq!(
            fling.harness.app.timing.pacer.decide(now),
            Decision::Idle,
            "a glide redraws only when a frame comes back"
        );
    }

    fn scenario(mirror: &mut PointerState) -> Vec<ClientToServerMsg> {
        let geometry = geometry();
        let none = ModifiersState::empty();
        let mut expected = vec![ClientToServerMsg::RenderFrameAck { seq: 1 }];
        expected.extend(
            mirror
                .moved(at(OVER_LEFT.0, OVER_LEFT.1), geometry, none)
                .map(mouse::message),
        );
        for _ in 0..6 {
            expected.extend(
                mirror
                    .wheel(pixels(40.0), geometry, none)
                    .into_iter()
                    .map(mouse::message),
            );
        }
        expected.push(ClientToServerMsg::RenderFrameAck { seq: 2 });
        expected.extend(
            mirror
                .wheel(pixels(0.0), geometry, none)
                .into_iter()
                .map(mouse::message),
        );
        expected
    }

    fn run(
        wayland: bool,
        momentum: bool,
        tick: bool,
    ) -> (Vec<ClientToServerMsg>, Vec<ClientToServerMsg>) {
        let expected = scenario(&mut PointerState::new());
        let mut fling = Fling::wired(expected.len(), wayland);
        if !momentum {
            fling.harness.app.options.scroll_momentum = false;
        }
        fling.swipe(40.0, true);
        if tick {
            for after in [70, 90, 200, 900, 3000] {
                fling.harness.app.advance_momentum(fling.start + ms(after));
            }
        }
        assert_eq!(
            fling.harness.app.momentum.is_coasting(),
            wayland && momentum && !tick
        );
        (fling.harness.sent(), expected)
    }

    #[test]
    fn the_touch_phase_changes_nothing_that_each_wheel_event_sends() {
        let (sent, expected) = run(true, true, false);
        assert_eq!(sent, expected);
    }

    #[test]
    fn away_from_wayland_the_same_swipe_sends_exactly_what_it_did_before() {
        let (sent, expected) = run(false, true, true);
        assert_eq!(sent, expected);
        let mut fling = Fling::offline(false);
        fling.swipe(40.0, true);
        assert!(!fling.coasting());
        assert_eq!(
            fling.harness.app.advance_momentum(fling.start + ms(80)),
            None
        );
    }

    #[test]
    fn with_momentum_off_the_same_swipe_sends_exactly_what_it_did_before() {
        let (sent, expected) = run(true, false, true);
        assert_eq!(sent, expected);
    }
}
