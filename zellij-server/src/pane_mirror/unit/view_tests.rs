use super::*;
use std::sync::Mutex;
use std::time::Duration;
use zellij_utils::data::{HostId, PaneId};
use zellij_utils::pane_mirror::{
    MirrorCell, MirrorFrameEncoder, MirrorScreen, MIRROR_CONNECT_TIMEOUT,
};
use zellij_utils::structured_render::WireCell;

#[derive(Default)]
struct Recorded {
    connects: Vec<(PaneAddress, MirrorAttach)>,
    sent: Vec<ViewerMessage>,
    closed: usize,
}

struct FakeTransport {
    recorded: Arc<Mutex<Recorded>>,
}

impl MirrorTransport for FakeTransport {
    fn send(&self, message: ViewerMessage) -> bool {
        self.recorded.lock().unwrap().sent.push(message);
        true
    }
    fn close(&mut self) {
        self.recorded.lock().unwrap().closed += 1;
    }
}

struct FakeConnector {
    recorded: Arc<Mutex<Recorded>>,
}

impl MirrorConnector for FakeConnector {
    fn connect(
        &self,
        address: &PaneAddress,
        attach: MirrorAttach,
        _sink: MirrorEventSink,
    ) -> Box<dyn MirrorTransport> {
        self.recorded
            .lock()
            .unwrap()
            .connects
            .push((address.clone(), attach));
        Box::new(FakeTransport {
            recorded: self.recorded.clone(),
        })
    }
}

fn view(view_only: bool) -> (MirrorView, Arc<Mutex<Recorded>>) {
    let recorded = Arc::new(Mutex::new(Recorded::default()));
    let connector = Arc::new(FakeConnector {
        recorded: recorded.clone(),
    });
    let route: MirrorEventRoute = Arc::new(|_, _, _| {});
    (
        MirrorView::new(0xF000_0000, connector, route, view_only, true),
        recorded,
    )
}

fn frame_of(lines: &[&str], cols: u16, rows: u16) -> Vec<u8> {
    MirrorFrameEncoder::new()
        .encode(&screen_of(lines, cols, rows))
        .unwrap()
        .bytes
}

fn screen_of(lines: &[&str], cols: u16, rows: u16) -> MirrorScreen {
    let mut screen = MirrorScreen::blank(cols, rows);
    for (y, line) in lines.iter().enumerate() {
        for (x, character) in line.chars().enumerate() {
            screen.cells[y * cols as usize + x] = MirrorCell::plain(WireCell {
                ch: character as u32,
                ..WireCell::BLANK
            });
        }
    }
    screen
}

fn size(cols: usize, rows: usize) -> Size {
    Size { cols, rows }
}

fn placeholder_text(view: &MirrorView) -> String {
    view.placeholder_lines().join("\n")
}

fn row_text(screen: &MirrorScreen, y: u16) -> String {
    screen
        .row(y)
        .iter()
        .map(|cell| char::from_u32(cell.cell.ch).unwrap_or(' '))
        .collect()
}

#[test]
fn a_new_mirror_shows_that_no_pane_is_selected() {
    let (view, _) = view(false);
    assert_eq!(view.state(), MirrorState::NoTarget);
    assert!(view.live_screen().is_none());
    assert!(placeholder_text(&view).contains("No pane selected"));
}

#[test]
fn a_target_connects_and_the_first_frame_makes_the_mirror_live() {
    let (mut view, recorded) = view(false);
    let target = PaneAddress::local("api", PaneId::Terminal(2));
    let update = view.set_target(Some(target.clone()), size(20, 4));
    assert!(update.state_changed);
    assert_eq!(view.state(), MirrorState::Connecting);
    {
        let recorded = recorded.lock().unwrap();
        assert_eq!(recorded.connects.len(), 1);
        assert_eq!(recorded.connects[0].0, target);
        assert_eq!(recorded.connects[0].1.target_pane_id, PaneId::Terminal(2));
        assert_eq!(recorded.connects[0].1.viewer_size, size(20, 4));
    }
    assert!(placeholder_text(&view).contains("Connecting"));
    assert!(view.live_screen().is_none());
    let update = view
        .handle_event(
            1,
            MirrorTransportEvent::Message(SourceMessage::Frame {
                frame: frame_of(&["hello"], 10, 2),
                alternate_screen: false,
            }),
        )
        .unwrap();
    assert!(update.state_changed);
    assert_eq!(view.state(), MirrorState::Live);
    assert_eq!(update.redraw, MirrorRedraw::Full);
    assert!(row_text(view.live_screen().unwrap(), 0).starts_with("hello"));
    assert!(recorded
        .lock()
        .unwrap()
        .sent
        .iter()
        .any(|m| matches!(m, ViewerMessage::FrameAck { .. })));
}

#[test]
fn events_from_an_old_target_are_ignored() {
    let (mut view, recorded) = view(false);
    view.set_target(
        Some(PaneAddress::local("old", PaneId::Terminal(1))),
        size(10, 2),
    );
    view.set_target(
        Some(PaneAddress::local("new", PaneId::Terminal(1))),
        size(10, 2),
    );
    assert_eq!(recorded.lock().unwrap().closed, 1);
    let stale = view.handle_event(
        1,
        MirrorTransportEvent::Message(SourceMessage::Frame {
            frame: frame_of(&["old"], 10, 2),
            alternate_screen: false,
        }),
    );
    assert_eq!(stale, None);
    assert_eq!(view.state(), MirrorState::Connecting);
}

#[test]
fn a_remote_host_is_refused_without_connecting() {
    let (mut view, recorded) = view(false);
    view.set_target(
        Some(PaneAddress {
            host: Some(HostId("laptop".to_owned())),
            session: "api".to_owned(),
            pane_id: PaneId::Terminal(1),
        }),
        size(30, 3),
    );
    assert_eq!(
        view.state(),
        MirrorState::Closed(MirrorClosedReason::UnsupportedHost)
    );
    assert!(recorded.lock().unwrap().connects.is_empty());
    assert!(placeholder_text(&view).contains("Unsupported host: laptop"));
}

#[test]
fn closed_reasons_show_their_placeholder() {
    let cases = [
        (MirrorClosedReason::PaneClosed, "The pane was closed"),
        (MirrorClosedReason::SessionEnding, "The session ended"),
        (MirrorClosedReason::Rejected, "refused"),
        (MirrorClosedReason::VersionMismatch, "Preview not available"),
        (MirrorClosedReason::ConnectionLost, "Preview not available"),
    ];
    for (reason, text) in cases {
        let (mut view, _) = view(false);
        view.set_target(
            Some(PaneAddress::local("api", PaneId::Terminal(1))),
            size(60, 3),
        );
        let update = view
            .handle_event(
                1,
                MirrorTransportEvent::Message(SourceMessage::closed(reason)),
            )
            .unwrap();
        assert!(update.state_changed);
        assert_eq!(view.state(), MirrorState::Closed(reason));
        let rendered = placeholder_text(&view);
        assert!(rendered.contains(text), "{:?}: {:?}", reason, rendered);
    }
}

#[test]
fn a_newer_protocol_version_is_reported_as_a_mismatch() {
    let (mut view, _) = view(false);
    view.set_target(
        Some(PaneAddress::local("api", PaneId::Terminal(1))),
        size(10, 2),
    );
    view.handle_event(
        1,
        MirrorTransportEvent::Message(SourceMessage::Attached {
            version: PANE_MIRROR_VERSION + 1,
            capabilities: vec![],
            pane_size: size(10, 2),
        }),
    );
    assert_eq!(
        view.state(),
        MirrorState::Closed(MirrorClosedReason::VersionMismatch)
    );
}

#[test]
fn a_view_only_mirror_does_not_send_input() {
    let (mut view, recorded) = view(true);
    view.set_target(
        Some(PaneAddress::local("api", PaneId::Terminal(1))),
        size(10, 2),
    );
    view.handle_event(
        1,
        MirrorTransportEvent::Message(SourceMessage::Frame {
            frame: frame_of(&["x"], 10, 2),
            alternate_screen: false,
        }),
    );
    assert!(!view.send_key(&None, b"typed".to_vec(), false));
    assert!(recorded.lock().unwrap().connects[0].1.view_only);
    assert!(!recorded
        .lock()
        .unwrap()
        .sent
        .iter()
        .any(|m| matches!(m, ViewerMessage::Input(_))));
}

fn live_view(view_only: bool) -> (MirrorView, Arc<Mutex<Recorded>>) {
    let (mut view, recorded) = view(view_only);
    view.set_target(
        Some(PaneAddress::local("api", PaneId::Terminal(1))),
        size(10, 2),
    );
    view.handle_event(
        1,
        MirrorTransportEvent::Message(SourceMessage::Frame {
            frame: frame_of(&["x"], 10, 2),
            alternate_screen: false,
        }),
    );
    (view, recorded)
}

fn resizes_sent(recorded: &Arc<Mutex<Recorded>>) -> Vec<(Size, bool, bool)> {
    recorded
        .lock()
        .unwrap()
        .sent
        .iter()
        .filter_map(|message| match message {
            ViewerMessage::Resize {
                size,
                focused,
                resize_allowed,
            } => Some((*size, *focused, *resize_allowed)),
            _ => None,
        })
        .collect()
}

#[test]
fn a_mirror_that_never_hears_back_gives_up_connecting() {
    let (mut view, _) = view(false);
    view.set_target(
        Some(PaneAddress::local("api", PaneId::Terminal(1))),
        size(10, 2),
    );
    let deadline = view.next_deadline().expect("a connect deadline");
    assert_eq!(view.tick(deadline - Duration::from_millis(1)), None);
    assert_eq!(view.state(), MirrorState::Connecting);
    let update = view
        .tick(Instant::now() + MIRROR_CONNECT_TIMEOUT)
        .expect("the time limit closes the mirror");
    assert!(update.state_changed);
    assert_eq!(
        view.state(),
        MirrorState::Closed(MirrorClosedReason::ConnectionFailed)
    );
    assert_eq!(
        view.placeholder_lines(),
        vec![
            "Preview not available".to_owned(),
            "The session may be running an older or unsupported version of Zellij".to_owned(),
            "api / terminal_1".to_owned(),
        ]
    );
}

#[test]
fn a_live_mirror_has_no_connect_deadline() {
    let (mut view, _) = live_view(false);
    view.set_resize_allowed(false);
    assert_eq!(view.next_deadline(), None);
    assert_eq!(view.tick(Instant::now() + MIRROR_CONNECT_TIMEOUT * 2), None);
    assert_eq!(view.state(), MirrorState::Live);
}

#[test]
fn an_unreadable_frame_closes_the_mirror_with_a_placeholder() {
    let (mut view, _) = view(false);
    view.set_target(
        Some(PaneAddress::local("api", PaneId::Terminal(1))),
        size(30, 3),
    );
    let update = view
        .handle_event(
            1,
            MirrorTransportEvent::Message(SourceMessage::Frame {
                frame: vec![0xde, 0xad],
                alternate_screen: false,
            }),
        )
        .expect("an update");
    assert!(update.state_changed);
    assert_eq!(
        view.state(),
        MirrorState::Closed(MirrorClosedReason::ProtocolError)
    );
    assert!(placeholder_text(&view).contains("could not be read"));
}

#[test]
fn an_unreadable_message_closes_the_mirror() {
    let (mut view, _) = live_view(false);
    view.handle_event(1, MirrorTransportEvent::Unreadable("garbage".to_owned()))
        .expect("an update");
    assert_eq!(
        view.state(),
        MirrorState::Closed(MirrorClosedReason::ProtocolError)
    );
}

#[test]
fn attach_carries_the_focus_and_resize_setting() {
    let (mut view, recorded) = view(false);
    view.set_focused(true);
    view.set_target(
        Some(PaneAddress::local("api", PaneId::Terminal(1))),
        size(10, 2),
    );
    let attach = recorded.lock().unwrap().connects[0].1.clone();
    assert!(attach.focused);
    assert!(attach.resize_allowed);
    assert!(!attach.view_only);
}

#[test]
fn focus_changes_and_renewals_send_resize() {
    let (mut view, recorded) = live_view(false);
    assert!(resizes_sent(&recorded).is_empty());
    assert!(view.set_focused(true));
    assert!(!view.set_focused(true));
    assert_eq!(resizes_sent(&recorded), vec![(size(10, 2), true, true)]);
    let renewal_at = view.next_deadline().expect("a renewal deadline");
    view.tick(renewal_at);
    assert_eq!(resizes_sent(&recorded).len(), 2);
    assert!(view.set_focused(false));
    assert_eq!(
        resizes_sent(&recorded).last().map(|sent| sent.1),
        Some(false)
    );
    let renewal_at = view
        .next_deadline()
        .expect("an unfocused view still renews");
    view.tick(renewal_at.max(Instant::now() + PIN_SETTLE));
    assert_eq!(resizes_sent(&recorded).len(), 4);
    assert_eq!(
        resizes_sent(&recorded).last(),
        Some(&(size(10, 2), false, true))
    );
    assert!(view.set_resize_allowed(false));
    assert_eq!(view.next_deadline(), None);
}

#[test]
fn turning_resizing_off_is_sent_and_kept_across_targets() {
    let (mut view, recorded) = live_view(false);
    view.set_focused(true);
    assert!(view.set_resize_allowed(false));
    assert_eq!(
        resizes_sent(&recorded).last(),
        Some(&(size(10, 2), true, false))
    );
    view.set_target(
        Some(PaneAddress::local("api", PaneId::Terminal(2))),
        size(10, 2),
    );
    assert!(!view.resize_allowed());
    assert!(!recorded.lock().unwrap().connects[1].1.resize_allowed);
    assert!(!view.wants_pin());
}

#[test]
fn a_view_only_mirror_never_asks_for_a_pin() {
    let (mut view, recorded) = live_view(true);
    view.set_focused(true);
    assert!(!view.wants_pin());
    assert!(resizes_sent(&recorded)
        .iter()
        .all(|(_, _, resize_allowed)| !resize_allowed));
    assert!(!recorded.lock().unwrap().connects[0].1.resize_allowed);
}

#[test]
fn the_pin_state_reported_by_the_source_is_kept() {
    let (mut view, _) = live_view(false);
    let update = view
        .handle_event(
            1,
            MirrorTransportEvent::Message(SourceMessage::PinState {
                pinned: true,
                pane_size: size(10, 2),
            }),
        )
        .unwrap();
    assert!(update.state_changed);
    assert_eq!(update.redraw, MirrorRedraw::Nothing);
    assert!(view.is_pinned());
    assert!(view.is_pinned());
    assert_eq!(view.source_size(), Some(size(10, 2)));
    view.set_target(None, size(10, 2));
    assert!(!view.is_pinned());
}

#[test]
fn the_alternate_screen_flag_of_the_source_is_reported() {
    let (mut view, _) = live_view(false);
    assert!(!view.alternate_screen_active());
    view.handle_event(
        1,
        MirrorTransportEvent::Message(SourceMessage::Frame {
            frame: frame_of(&["y"], 10, 2),
            alternate_screen: true,
        }),
    );
    assert!(view.alternate_screen_active());
}

#[test]
fn only_a_size_change_counts_as_a_resize() {
    let (mut view, recorded) = live_view(false);
    assert!(!view.resized(size(10, 2)));
    assert!(view.resized(size(12, 3)));
    assert_eq!(
        resizes_sent(&recorded),
        vec![(size(12, 3), false, false)],
        "a resize before the target settles does not ask to resize the source"
    );
    assert!(view.set_focused(true));
    assert!(!view.resized(size(12, 3)));
    assert!(view.resized(size(14, 4)));
    assert_eq!(
        resizes_sent(&recorded).last(),
        Some(&(size(14, 4), true, true)),
        "a resize of a typing view asks to resize the source"
    );
}

#[test]
fn a_view_only_mirror_reports_that_it_may_not_resize_and_ignores_turning_it_on() {
    let (mut view, recorded) = live_view(true);
    let sent_before = resizes_sent(&recorded).len();
    assert!(!view.resize_allowed());
    assert!(view.is_view_only());
    assert!(!view.set_resize_allowed(true));
    assert!(!view.set_resize_allowed(false));
    assert!(!view.resize_allowed());
    assert_eq!(resizes_sent(&recorded).len(), sent_before);
}

#[test]
fn a_new_source_size_counts_as_a_state_change() {
    let (mut view, _) = live_view(false);
    let pin_state = |cols, rows| {
        MirrorTransportEvent::Message(SourceMessage::PinState {
            pinned: false,
            pane_size: size(cols, rows),
        })
    };
    assert!(
        view.handle_event(1, pin_state(80, 24))
            .unwrap()
            .state_changed
    );
    assert_eq!(view.source_size(), Some(size(80, 24)));
    assert!(
        !view
            .handle_event(1, pin_state(80, 24))
            .unwrap()
            .state_changed
    );
    assert!(
        view.handle_event(1, pin_state(100, 30))
            .unwrap()
            .state_changed
    );
    assert_eq!(view.source_size(), Some(size(100, 30)));
}

#[test]
fn a_changed_row_redraws_only_that_row() {
    let (mut view, _) = view(false);
    view.set_target(
        Some(PaneAddress::local("api", PaneId::Terminal(1))),
        size(10, 2),
    );
    let mut encoder = MirrorFrameEncoder::new();
    let frame = |encoder: &mut MirrorFrameEncoder, lines: &[&str]| {
        MirrorTransportEvent::Message(SourceMessage::Frame {
            frame: encoder.encode(&screen_of(lines, 10, 2)).unwrap().bytes,
            alternate_screen: false,
        })
    };
    let first = view.handle_event(1, frame(&mut encoder, &["x"])).unwrap();
    assert_eq!(first.redraw, MirrorRedraw::Full);
    let update = view
        .handle_event(1, frame(&mut encoder, &["x", "changed"]))
        .unwrap();
    assert!(!update.state_changed);
    match update.redraw {
        MirrorRedraw::Rows(rows) => assert!(rows.contains(&1)),
        other => panic!("expected a row redraw, got {:?}", other),
    }
    assert!(row_text(view.live_screen().unwrap(), 1).starts_with("changed"));
}

#[test]
fn a_shown_view_asks_for_the_resize_only_once_its_target_has_settled() {
    let (mut view, recorded) = view(false);
    let before = Instant::now();
    view.set_target(
        Some(PaneAddress::local("api", PaneId::Terminal(1))),
        size(10, 2),
    );
    let after = Instant::now();
    assert!(!recorded.lock().unwrap().connects[0].1.resize_allowed);
    let settle_at = view.next_deadline().expect("a settle deadline");
    assert!(settle_at >= before + PIN_SETTLE && settle_at <= after + PIN_SETTLE);
    view.tick(settle_at - Duration::from_millis(1));
    assert!(resizes_sent(&recorded).is_empty());
    view.tick(settle_at);
    assert_eq!(resizes_sent(&recorded), vec![(size(10, 2), false, true)]);
    view.set_target(
        Some(PaneAddress::local("api", PaneId::Terminal(2))),
        size(10, 2),
    );
    assert!(
        !recorded.lock().unwrap().connects[1].1.resize_allowed,
        "a new target settles again"
    );
    view.set_focused(true);
    assert_eq!(
        resizes_sent(&recorded).last(),
        Some(&(size(10, 2), true, true)),
        "typing asks for the resize at once"
    );
    view.set_target(
        Some(PaneAddress::local("api", PaneId::Terminal(3))),
        size(10, 2),
    );
    assert!(recorded.lock().unwrap().connects[2].1.resize_allowed);
}

#[test]
fn switching_targets_keeps_the_old_picture_for_a_moment_instead_of_connecting() {
    let (mut view, _) = live_view(false);
    let before = Instant::now();
    view.set_target(
        Some(PaneAddress::local("api", PaneId::Terminal(2))),
        size(10, 2),
    );
    let after = Instant::now();
    assert_eq!(view.state(), MirrorState::Connecting);
    assert!(view.live_screen().is_none());
    assert!(row_text(view.shown_screen().expect("the old picture"), 0).starts_with('x'));
    assert_eq!(
        view.tick(before + SWITCH_HOLD - Duration::from_millis(1)),
        None
    );
    let update = view
        .tick(after + SWITCH_HOLD)
        .expect("the hold ends with a redraw");
    assert_eq!(
        update,
        MirrorUpdate {
            state_changed: false,
            redraw: MirrorRedraw::Full,
        }
    );
    assert!(view.shown_screen().is_none());
    assert!(placeholder_text(&view).contains("Connecting"));
}

#[test]
fn the_new_targets_first_frame_or_a_close_replaces_the_held_picture() {
    let (mut view, _) = live_view(false);
    view.set_target(
        Some(PaneAddress::local("api", PaneId::Terminal(2))),
        size(10, 2),
    );
    view.handle_event(
        2,
        MirrorTransportEvent::Message(SourceMessage::Frame {
            frame: frame_of(&["new"], 10, 2),
            alternate_screen: false,
        }),
    );
    assert!(row_text(view.shown_screen().unwrap(), 0).starts_with("new"));
    view.set_target(
        Some(PaneAddress::local("api", PaneId::Terminal(3))),
        size(10, 2),
    );
    view.set_target(
        Some(PaneAddress::local("api", PaneId::Terminal(4))),
        size(10, 2),
    );
    assert!(
        row_text(view.shown_screen().unwrap(), 0).starts_with("new"),
        "switching again while connecting keeps the same picture"
    );
    view.handle_event(
        4,
        MirrorTransportEvent::Message(SourceMessage::closed(MirrorClosedReason::PaneNotFound)),
    );
    assert!(view.shown_screen().is_none());
    assert_eq!(view.next_deadline(), None);
}
