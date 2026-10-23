use super::*;
use crate::pane_mirror::view::MirrorEventRoute;
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use zellij_utils::data::{BareKey, PaneId};
use zellij_utils::pane_mirror::{
    MirrorAttach, MirrorCell, MirrorConnector, MirrorEventSink, MirrorFrameEncoder, MirrorScreen,
    MirrorTransport, MirrorTransportEvent, SourceMessage, ViewerMessage,
};
use zellij_utils::structured_render::WireCell;

#[derive(Default)]
struct Recorded {
    sent: Vec<ViewerMessage>,
}

struct FakeTransport {
    recorded: Arc<Mutex<Recorded>>,
}

impl MirrorTransport for FakeTransport {
    fn send(&self, message: ViewerMessage) -> bool {
        self.recorded.lock().unwrap().sent.push(message);
        true
    }
    fn close(&mut self) {}
}

struct FakeConnector {
    recorded: Arc<Mutex<Recorded>>,
}

impl MirrorConnector for FakeConnector {
    fn connect(
        &self,
        _address: &PaneAddress,
        _attach: MirrorAttach,
        _sink: MirrorEventSink,
    ) -> Box<dyn MirrorTransport> {
        Box::new(FakeTransport {
            recorded: self.recorded.clone(),
        })
    }
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

fn frame(encoder: &mut MirrorFrameEncoder, lines: &[&str]) -> MirrorTransportEvent {
    MirrorTransportEvent::Message(SourceMessage::Frame {
        frame: encoder.encode(&screen_of(lines, 10, 3)).unwrap().bytes,
        alternate_screen: false,
    })
}

struct Fixture {
    live_views: LiveViews,
    recorded: Arc<Mutex<Recorded>>,
    encoder: MirrorFrameEncoder,
    bound: Vec<KeyWithModifier>,
}

const CLIENT: ClientId = 1;
const PLUGIN: u32 = 7;
const KEY: LiveViewKey = (PLUGIN, CLIENT);

fn slot(
    recorded: &Arc<Mutex<Recorded>>,
    view_id: u32,
    target: Option<PaneAddress>,
    view_only: bool,
) -> LiveViewSlot {
    let connector = Arc::new(FakeConnector {
        recorded: recorded.clone(),
    });
    let route: MirrorEventRoute = Arc::new(|_, _, _| {});
    let mut view = MirrorView::new(view_id, connector, route, view_only, true);
    view.set_target(target, Size { cols: 10, rows: 3 });
    LiveViewSlot::new(view, LiveViewRect::new(1, 1, 10, 3), None)
}

fn fixture_with(view_only: bool) -> Fixture {
    let recorded = Arc::new(Mutex::new(Recorded::default()));
    let mut live_views = LiveViews::default();
    let target = PaneAddress::local("api", PaneId::Terminal(2));
    live_views.insert(KEY, slot(&recorded, 0, Some(target), view_only));
    let mut fixture = Fixture {
        live_views,
        recorded,
        encoder: MirrorFrameEncoder::new(),
        bound: vec![],
    };
    let first = frame(&mut fixture.encoder, &["hello"]);
    fixture.event(first);
    fixture
}

fn fixture() -> Fixture {
    fixture_with(false)
}

impl Fixture {
    fn slot(&mut self) -> &mut LiveViewSlot {
        self.live_views.get_mut(&KEY).unwrap()
    }
    fn event(&mut self, event: MirrorTransportEvent) {
        let slot = self.slot();
        if let Some(update) = slot.view.handle_event(1, event) {
            slot.redraw(update.redraw);
        }
    }
    fn inputs(&self) -> Vec<MirrorInput> {
        self.recorded
            .lock()
            .unwrap()
            .sent
            .iter()
            .filter_map(|message| match message {
                ViewerMessage::Input(input) => Some(input.clone()),
                _ => None,
            })
            .collect()
    }
    fn focus_reports(&self) -> Vec<bool> {
        self.recorded
            .lock()
            .unwrap()
            .sent
            .iter()
            .filter_map(|message| match message {
                ViewerMessage::Resize { focused, .. } => Some(*focused),
                _ => None,
            })
            .collect()
    }
}

fn key(character: char) -> Option<KeyWithModifier> {
    Some(KeyWithModifier::new(BareKey::Char(character)))
}

fn content() -> LiveViewRect {
    LiveViewRect::new(20, 4, 30, 10)
}

#[test]
fn capture_starts_only_for_a_focused_view_with_a_target_that_may_type() {
    let mut fixture = fixture();
    assert_eq!(
        fixture.live_views.start_capture(KEY, false),
        Err(CaptureRefusal::NotFocused)
    );
    assert!(!fixture.live_views.is_captured(PLUGIN, CLIENT));
    assert_eq!(
        fixture
            .slot()
            .report_if_changed()
            .unwrap()
            .input_released_by,
        Some(LiveViewRelease::NotAllowed)
    );
    assert_eq!(
        fixture.live_views.start_capture((PLUGIN + 1, CLIENT), true),
        Err(CaptureRefusal::NoView)
    );
    assert_eq!(fixture.live_views.start_capture(KEY, true), Ok(()));
    assert!(fixture.live_views.is_captured(PLUGIN, CLIENT));
    let report = fixture.slot().report_if_changed().unwrap();
    assert!(report.input_captured);
    assert_eq!(report.input_released_by, None);
}

#[test]
fn a_view_without_a_target_cannot_capture() {
    let recorded = Arc::new(Mutex::new(Recorded::default()));
    let mut live_views = LiveViews::default();
    live_views.insert(KEY, slot(&recorded, 0, None, false));
    assert_eq!(
        live_views.start_capture(KEY, true),
        Err(CaptureRefusal::NoTarget)
    );
}

#[test]
fn a_view_only_view_cannot_capture() {
    let mut fixture = fixture_with(true);
    assert_eq!(
        fixture.live_views.start_capture(KEY, true),
        Err(CaptureRefusal::ViewOnly)
    );
    assert!(!fixture.slot().report().resize_allowed);
}

#[test]
fn a_refusal_is_reported_even_when_nothing_else_changed() {
    let mut fixture = fixture();
    let _ = fixture.live_views.start_capture(KEY, false);
    assert!(fixture.slot().report_if_changed().is_some());
    assert!(fixture.slot().report_if_changed().is_none());
    let _ = fixture.live_views.start_capture(KEY, false);
    assert!(fixture.slot().report_if_changed().is_some());
}

#[test]
fn typed_keys_go_to_the_source_only_while_capture_is_on() {
    let mut fixture = fixture();
    assert_eq!(
        fixture.live_views.send_key(CLIENT, &key('a'), b"a", false),
        LiveViewKeyOutcome::NotCaptured
    );
    fixture.live_views.start_capture(KEY, true).unwrap();
    assert_eq!(
        fixture.live_views.send_key(CLIENT, &key('b'), b"b", false),
        LiveViewKeyOutcome::Sent
    );
    assert_eq!(
        fixture
            .live_views
            .send_key(CLIENT, &None, b"\x1b[200~pasted", false),
        LiveViewKeyOutcome::Sent
    );
    assert!(fixture.live_views.send_paste(CLIENT, b"more".to_vec()));
    let inputs = fixture.inputs();
    assert_eq!(inputs.len(), 3);
    assert!(
        matches!(&inputs[0], MirrorInput::Key { key, .. } if key.bare_key == BareKey::Char('b'))
    );
    assert_eq!(inputs[1], MirrorInput::Bytes(b"\x1b[200~pasted".to_vec()));
    assert_eq!(inputs[2], MirrorInput::Paste(b"more".to_vec()));
}

impl Fixture {
    fn route_key(&mut self, key: KeyWithModifier, raw_bytes: &[u8]) -> bool {
        let bound = self.bound.clone();
        let released = self
            .live_views
            .captures
            .lock()
            .unwrap()
            .key_pressed(CLIENT, &key, || bound);
        if released {
            self.live_views.release_client(CLIENT, LiveViewRelease::Key);
            self.live_views.announce_captures();
            return true;
        }
        self.live_views
            .send_key(CLIENT, &Some(key), raw_bytes, false);
        false
    }
    fn route_keybinding(&mut self, key: KeyWithModifier) -> bool {
        let bound = self.bound.clone();
        self.live_views
            .captures
            .lock()
            .unwrap()
            .key_pressed(CLIENT, &key, || bound)
    }
    fn configure(&mut self, release_key: &str) {
        self.live_views
            .set_release_key(parse_release_key(Some(release_key)));
    }
    fn capture(&mut self) {
        self.live_views.start_capture(KEY, true).unwrap();
        self.live_views.announce_captures();
    }
}

fn esc() -> KeyWithModifier {
    KeyWithModifier::new(BareKey::Esc)
}

fn f9() -> KeyWithModifier {
    KeyWithModifier::new(BareKey::F(9))
}

fn esc_twice() -> LiveViewReleaseGesture {
    parse_release_key(Some("Esc Esc")).unwrap().0
}

#[test]
fn a_single_release_key_ends_capture_and_never_reaches_the_source() {
    let mut fixture = fixture();
    fixture.configure("Ctrl Alt Home");
    fixture.capture();
    let release_key = KeyWithModifier::new(BareKey::Home)
        .with_ctrl_modifier()
        .with_alt_modifier();
    assert!(fixture.route_key(release_key.clone(), b"\x1b[1;7H"));
    assert!(!fixture.live_views.is_captured(PLUGIN, CLIENT));
    assert!(fixture.inputs().is_empty());
    assert_eq!(
        fixture.slot().report().input_released_by,
        Some(LiveViewRelease::Key)
    );
    assert!(!fixture
        .live_views
        .captures
        .lock()
        .unwrap()
        .is_capturing(CLIENT));
    assert!(!fixture.route_key(release_key, b"\x1b[1;7H"));
}

#[test]
fn the_default_release_key_is_f9_and_a_double_esc_reaches_the_source() {
    let mut fixture = fixture();
    fixture.capture();
    assert!(!fixture.route_key(esc(), b"\x1b"));
    assert!(!fixture.route_key(esc(), b"\x1b"));
    assert_eq!(
        fixture.inputs().len(),
        2,
        "both Esc presses reach the program"
    );
    assert!(fixture.live_views.is_captured(PLUGIN, CLIENT));
    assert!(fixture.route_key(f9(), b"\x1b[20~"));
    assert_eq!(fixture.inputs().len(), 2, "F9 stays in Zellij");
    assert!(!fixture.live_views.is_captured(PLUGIN, CLIENT));
}

#[test]
fn the_key_bound_to_the_inbox_releases_and_f9_then_reaches_the_source() {
    let mut fixture = fixture();
    let ctrl_y = KeyWithModifier::new(BareKey::Char('y')).with_ctrl_modifier();
    let alt_i = KeyWithModifier::new(BareKey::Char('i')).with_alt_modifier();
    fixture.bound = vec![ctrl_y.clone(), alt_i.clone()];
    fixture.capture();
    assert!(!fixture.route_key(f9(), b"\x1b[20~"));
    assert_eq!(fixture.inputs().len(), 1, "F9 reaches the program");
    assert!(fixture.route_key(alt_i, b"\x1bi"));
    assert!(!fixture.live_views.is_captured(PLUGIN, CLIENT));
    fixture.capture();
    assert!(
        fixture.route_key(ctrl_y, b"\x19"),
        "any of the bindings works"
    );
    assert_eq!(fixture.inputs().len(), 1);
}

#[test]
fn a_configured_release_key_beats_the_binding() {
    let mut fixture = fixture();
    fixture.bound = vec![f9()];
    fixture.configure("Esc Esc");
    fixture.capture();
    assert!(!fixture.route_key(f9(), b"\x1b[20~"));
    assert!(!fixture.route_key(esc(), b"\x1b"));
    assert!(fixture.route_key(esc(), b"\x1b"));
    assert!(!fixture.live_views.is_captured(PLUGIN, CLIENT));
    assert_eq!(
        fixture.inputs().len(),
        2,
        "F9 and the first Esc reach the program"
    );
    fixture.configure("not a key");
    assert_eq!(fixture.live_views.release_key, LiveViewReleaseKey::Bound);
    fixture.capture();
    assert!(fixture.route_key(f9(), b"\x1b[20~"));
}

#[test]
fn the_keys_bound_to_the_inbox_are_the_ones_that_toggle_the_session_card() {
    let f9_key = f9();
    let ctrl_y = KeyWithModifier::new(BareKey::Char('y')).with_ctrl_modifier();
    let ctrl_g = KeyWithModifier::new(BareKey::Char('g')).with_ctrl_modifier();
    let toggle = vec![Action::ToggleSessionCard];
    let lock = vec![Action::SwitchToMode {
        input_mode: zellij_utils::data::InputMode::Locked,
    }];
    let bindings = vec![(&f9_key, &toggle), (&ctrl_g, &lock), (&ctrl_y, &toggle)];
    let bound = keys_bound_to_the_inbox(bindings);
    assert_eq!(bound, vec![ctrl_y.clone(), f9_key.clone()]);
    assert_eq!(release_key_text_for(bound), "Ctrl y");
    assert_eq!(release_key_text_for(vec![]), "F9");
    assert_eq!(release_keys_or_default(vec![]), vec![f9_key]);
}

#[test]
fn the_screen_never_releases_on_keys_it_is_sent() {
    let mut fixture = fixture();
    fixture.capture();
    for _ in 0..3 {
        assert_eq!(
            fixture
                .live_views
                .send_key(CLIENT, &Some(esc()), b"\x1b", false),
            LiveViewKeyOutcome::Sent
        );
    }
    assert!(fixture.live_views.is_captured(PLUGIN, CLIENT));
    assert_eq!(fixture.inputs().len(), 3);
}

#[test]
fn a_key_that_runs_a_keybinding_between_two_escs_starts_the_double_press_over() {
    let mut fixture = fixture();
    fixture.configure("Esc Esc");
    fixture.capture();
    assert!(!fixture.route_key(esc(), b"\x1b"));
    let alt_n = KeyWithModifier::new(BareKey::Char('n')).with_alt_modifier();
    assert!(!fixture.route_keybinding(alt_n));
    assert!(
        !fixture.route_key(esc(), b"\x1b"),
        "Esc, a bound key, Esc does not release"
    );
    assert!(fixture.live_views.is_captured(PLUGIN, CLIENT));
    assert!(fixture.route_key(esc(), b"\x1b"));
    assert!(!fixture.live_views.is_captured(PLUGIN, CLIENT));
}

#[test]
fn capture_is_known_to_the_shared_count_as_soon_as_it_starts_and_ends() {
    let mut fixture = fixture();
    assert!(!fixture
        .live_views
        .captures
        .lock()
        .unwrap()
        .is_capturing(CLIENT));
    fixture.capture();
    assert!(fixture
        .live_views
        .captures
        .lock()
        .unwrap()
        .is_capturing(CLIENT));
    assert!(!fixture.route_key(esc(), b"\x1b"));
    fixture
        .live_views
        .release_client(CLIENT, LiveViewRelease::ClickOutside);
    fixture.live_views.announce_captures();
    assert!(!fixture
        .live_views
        .captures
        .lock()
        .unwrap()
        .is_capturing(CLIENT));
    fixture.capture();
    assert!(
        !fixture.route_key(esc(), b"\x1b"),
        "a press before the release does not count after capturing again"
    );
}

#[test]
fn the_configured_release_key_is_used_and_a_bad_or_missing_one_follows_the_binding() {
    assert_eq!(
        parse_release_key(Some("Ctrl Alt End")),
        Some((
            LiveViewReleaseGesture {
                key: KeyWithModifier::from_str("Ctrl Alt End").unwrap(),
                double: false,
            },
            "Ctrl Alt End".to_owned()
        ))
    );
    assert_eq!(
        parse_release_key(Some(" Ctrl q Ctrl q ")).map(|(gesture, _)| gesture),
        Some(LiveViewReleaseGesture {
            key: KeyWithModifier::from_str("Ctrl q").unwrap(),
            double: true,
        })
    );
    assert_eq!(parse_release_key(Some("not a key")), None);
    assert_eq!(parse_release_key(Some("  ")), None);
    assert_eq!(parse_release_key(None), None);
    assert_eq!(
        default_release_key(),
        LiveViewReleaseGesture {
            key: f9(),
            double: false,
        }
    );
}

fn press(
    gesture: &LiveViewReleaseGesture,
    key: BareKey,
    presses: &mut BTreeMap<ClientId, std::time::Instant>,
) -> bool {
    is_release_press(gesture, &KeyWithModifier::new(key), presses, CLIENT)
}

#[test]
fn a_single_key_gesture_releases_on_the_first_press() {
    let gesture = parse_release_key(Some("Ctrl Alt Home")).unwrap().0;
    let mut presses = BTreeMap::new();
    let ctrl_alt_home = KeyWithModifier::new(BareKey::Home)
        .with_ctrl_modifier()
        .with_alt_modifier();
    assert!(is_release_press(
        &gesture,
        &ctrl_alt_home,
        &mut presses,
        CLIENT
    ));
    assert!(!press(&gesture, BareKey::Home, &mut presses));
    assert!(presses.is_empty());
}

#[test]
fn a_double_gesture_needs_two_presses_within_the_window_with_nothing_between() {
    let gesture = esc_twice();
    let mut presses = BTreeMap::new();
    assert!(!press(&gesture, BareKey::Esc, &mut presses));
    assert!(press(&gesture, BareKey::Esc, &mut presses));
    assert!(presses.is_empty());
    assert!(
        !press(&gesture, BareKey::Esc, &mut presses),
        "a third press starts over"
    );
    let window = std::time::Duration::from_millis(LIVE_VIEW_DOUBLE_PRESS_MS);
    if let Some(long_ago) =
        std::time::Instant::now().checked_sub(window + std::time::Duration::from_millis(100))
    {
        presses.insert(CLIENT, long_ago);
        assert!(
            !press(&gesture, BareKey::Esc, &mut presses),
            "a press after the window counts as a first press"
        );
        assert!(press(&gesture, BareKey::Esc, &mut presses));
    }
    assert!(!press(&gesture, BareKey::Esc, &mut presses));
    assert!(!press(&gesture, BareKey::Char('i'), &mut presses));
    assert!(
        !press(&gesture, BareKey::Esc, &mut presses),
        "another key between the presses starts over"
    );
    assert!(press(&gesture, BareKey::Esc, &mut presses));
    let mut other_client = BTreeMap::new();
    assert!(!is_release_press(
        &gesture,
        &KeyWithModifier::new(BareKey::Esc),
        &mut other_client,
        CLIENT
    ));
    assert!(!is_release_press(
        &gesture,
        &KeyWithModifier::new(BareKey::Esc),
        &mut other_client,
        CLIENT + 1
    ));
}

#[test]
fn two_plain_escapes_merged_into_alt_esc_count_as_the_double_press() {
    let gesture = esc_twice();
    let mut presses = BTreeMap::new();
    let alt_esc = KeyWithModifier::new(BareKey::Esc).with_alt_modifier();
    assert!(is_release_press(&gesture, &alt_esc, &mut presses, CLIENT));
    assert!(presses.is_empty());
    assert!(!press(&gesture, BareKey::Esc, &mut presses));
    assert!(is_release_press(&gesture, &alt_esc, &mut presses, CLIENT));
    assert!(presses.is_empty());
    let single = parse_release_key(Some("Ctrl Alt Home")).unwrap().0;
    let alt_ctrl_alt_home = KeyWithModifier::new(BareKey::Home)
        .with_ctrl_modifier()
        .with_alt_modifier();
    assert!(is_release_press(
        &single,
        &alt_ctrl_alt_home,
        &mut presses,
        CLIENT
    ));
    let alt_q = parse_release_key(Some("Alt q Alt q")).unwrap().0;
    assert!(!is_release_press(
        &alt_q,
        &KeyWithModifier::new(BareKey::Char('q')).with_alt_modifier(),
        &mut presses,
        CLIENT
    ));
    let ctrl_q = parse_release_key(Some("Ctrl q Ctrl q")).unwrap().0;
    let alt_ctrl_q = KeyWithModifier::new(BareKey::Char('q'))
        .with_ctrl_modifier()
        .with_alt_modifier();
    assert!(is_release_press(&ctrl_q, &alt_ctrl_q, &mut presses, CLIENT));
}

#[test]
fn a_merged_alt_esc_while_capturing_ends_capture_without_reaching_the_source() {
    let mut fixture = fixture();
    fixture.configure("Esc Esc");
    fixture.capture();
    assert!(fixture.route_key(
        KeyWithModifier::new(BareKey::Esc).with_alt_modifier(),
        b"\x1b\x1b"
    ));
    assert!(!fixture.live_views.is_captured(PLUGIN, CLIENT));
    assert!(fixture.inputs().is_empty());
}

#[test]
fn every_release_reason_is_kept_until_capture_starts_again() {
    for reason in [
        LiveViewRelease::Key,
        LiveViewRelease::ClickOutside,
        LiveViewRelease::FocusLost,
        LiveViewRelease::Plugin,
        LiveViewRelease::Cleared,
    ] {
        let mut fixture = fixture();
        assert!(!fixture.live_views.release(&KEY, reason));
        fixture.live_views.start_capture(KEY, true).unwrap();
        assert!(fixture.live_views.release(&KEY, reason));
        assert_eq!(fixture.slot().report().input_released_by, Some(reason));
        assert!(!fixture.slot().report().input_captured);
        fixture.live_views.start_capture(KEY, true).unwrap();
        assert_eq!(fixture.slot().report().input_released_by, None);
    }
}

#[test]
fn capturing_another_view_of_the_same_client_releases_the_first() {
    let mut fixture = fixture();
    let other = (PLUGIN + 1, CLIENT);
    let target = PaneAddress::local("api", PaneId::Terminal(3));
    let other_slot = slot(&fixture.recorded, 1, Some(target), false);
    fixture.live_views.insert(other, other_slot);
    fixture.live_views.start_capture(KEY, true).unwrap();
    fixture.live_views.start_capture(other, true).unwrap();
    assert!(!fixture.live_views.is_captured(PLUGIN, CLIENT));
    assert!(fixture.live_views.is_captured(PLUGIN + 1, CLIENT));
    assert_eq!(
        fixture.slot().report().input_released_by,
        Some(LiveViewRelease::FocusLost)
    );
    assert_eq!(fixture.live_views.captured_by(CLIENT), Some(other));
}

#[test]
fn views_of_different_clients_capture_independently() {
    let mut fixture = fixture();
    let other = (PLUGIN, CLIENT + 1);
    let target = PaneAddress::local("api", PaneId::Terminal(3));
    let other_slot = slot(&fixture.recorded, 1, Some(target), false);
    fixture.live_views.insert(other, other_slot);
    fixture.live_views.start_capture(KEY, true).unwrap();
    fixture.live_views.start_capture(other, true).unwrap();
    assert!(fixture.live_views.is_captured(PLUGIN, CLIENT));
    assert!(fixture.live_views.is_captured(PLUGIN, CLIENT + 1));
    fixture
        .live_views
        .release_client(CLIENT, LiveViewRelease::Plugin);
    assert!(fixture.live_views.is_captured(PLUGIN, CLIENT + 1));
}

#[test]
fn a_shown_view_wants_the_pin_and_capture_is_reported_as_focus() {
    let mut fixture = fixture();
    assert!(fixture.slot().view.wants_pin());
    fixture.live_views.start_capture(KEY, true).unwrap();
    assert!(fixture.slot().view.wants_pin());
    assert_eq!(fixture.focus_reports().last(), Some(&true));
    fixture.live_views.release(&KEY, LiveViewRelease::Plugin);
    assert!(fixture.slot().view.wants_pin());
    assert_eq!(fixture.focus_reports().last(), Some(&false));
}

#[test]
fn captures_are_announced_once_and_withdrawn() {
    let mut fixture = fixture();
    assert!(fixture.live_views.capture_changes().is_empty());
    fixture.live_views.start_capture(KEY, true).unwrap();
    let release_key = fixture.live_views.release_key.clone();
    assert_eq!(
        fixture.live_views.capture_changes(),
        vec![(CLIENT, Some(release_key.clone()))]
    );
    assert!(fixture.live_views.capture_changes().is_empty());
    fixture.configure("Ctrl Alt End");
    assert_eq!(fixture.live_views.capture_changes().len(), 1);
    fixture.live_views.release(&KEY, LiveViewRelease::Plugin);
    assert_eq!(fixture.live_views.capture_changes(), vec![(CLIENT, None)]);
    assert!(fixture.live_views.capture_changes().is_empty());
}

#[test]
fn the_first_draw_is_full_and_later_frames_redraw_only_their_rows() {
    let mut fixture = fixture();
    let chunks = fixture.live_views.draw(&KEY, &content(), false, None);
    assert_eq!(chunks.len(), 3);
    assert!(fixture
        .live_views
        .draw(&KEY, &content(), false, None)
        .is_empty());
    let changed = frame(&mut fixture.encoder, &["hello", "", "third"]);
    fixture.event(changed);
    let chunks = fixture.live_views.draw(&KEY, &content(), false, None);
    assert_eq!(chunks.len(), 1);
    assert_eq!(chunks[0].y, content().y + 1 + 2);
    assert_eq!(chunks[0].x, content().x + 1);
}

#[test]
fn a_plugin_render_or_a_moved_rectangle_redraws_everything() {
    let mut fixture = fixture();
    fixture.live_views.draw(&KEY, &content(), false, None);
    assert_eq!(
        fixture.live_views.draw(&KEY, &content(), true, None).len(),
        3
    );
    fixture.slot().rect = LiveViewRect::new(2, 2, 10, 3);
    assert_eq!(
        fixture.live_views.draw(&KEY, &content(), false, None).len(),
        3
    );
    let shrunk = LiveViewRect::new(20, 4, 8, 4);
    let chunks = fixture.live_views.draw(&KEY, &shrunk, false, None);
    assert_eq!(chunks.len(), 2);
    assert!(chunks.iter().all(|chunk| chunk.width() == 6));
    let gone = LiveViewRect::new(20, 4, 1, 1);
    assert!(fixture.live_views.draw(&KEY, &gone, true, None).is_empty());
}

#[test]
fn links_are_registered_once_with_the_link_handler() {
    let mut fixture = fixture();
    let mut screen = screen_of(&["ab"], 10, 3);
    screen.cells[0].link = Some("https://example.com".to_owned());
    screen.cells[1].link = Some("https://example.com".to_owned());
    let linked = MirrorTransportEvent::Message(SourceMessage::Frame {
        frame: fixture.encoder.encode(&screen).unwrap().bytes,
        alternate_screen: false,
    });
    fixture.event(linked);
    let link_handler = Rc::new(RefCell::new(LinkHandler::new()));
    let chunks = fixture
        .live_views
        .draw(&KEY, &content(), true, Some(&link_handler));
    let first = chunks[0].terminal_characters[0].styles.link_anchor;
    let second = chunks[0].terminal_characters[1].styles.link_anchor;
    assert!(matches!(first, Some(LinkAnchor::Start(_))));
    assert_eq!(first, second);
    assert_eq!(
        link_handler.borrow().uri(first),
        Some("https://example.com")
    );
    fixture
        .live_views
        .draw(&KEY, &content(), true, Some(&link_handler));
    assert_eq!(link_handler.borrow().links().len(), 1);
}

#[test]
fn a_view_that_is_not_live_draws_its_placeholder() {
    let recorded = Arc::new(Mutex::new(Recorded::default()));
    let mut live_views = LiveViews::default();
    let mut empty = slot(&recorded, 0, None, false);
    empty.rect = LiveViewRect::new(1, 1, 20, 3);
    live_views.insert(KEY, empty);
    let chunks = live_views.draw(&KEY, &content(), false, None);
    let mut text = String::new();
    for chunk in &chunks {
        for character in &chunk.terminal_characters {
            character.push_text_to(&mut text);
        }
    }
    assert!(text.contains("No pane selected"));
}

#[test]
fn the_cursor_follows_the_source_inside_the_rectangle_only_while_captured() {
    let mut fixture = fixture();
    assert_eq!(fixture.live_views.cursor(CLIENT, PLUGIN, &content()), None);
    fixture.live_views.start_capture(KEY, true).unwrap();
    let mut screen = screen_of(&["hello"], 10, 3);
    screen.cursor = zellij_utils::structured_render::CursorState {
        x: 5,
        y: 0,
        visible: true,
        shape: zellij_utils::structured_render::CURSOR_SHAPE_BEAM,
    };
    let moved = MirrorTransportEvent::Message(SourceMessage::Frame {
        frame: fixture.encoder.encode(&screen).unwrap().bytes,
        alternate_screen: false,
    });
    fixture.event(moved);
    let (x, y, visible, shape) = fixture
        .live_views
        .cursor(CLIENT, PLUGIN, &content())
        .unwrap();
    assert_eq!(
        (x, y, visible),
        (content().x + 1 + 5, content().y + 1, true)
    );
    assert_eq!(shape, "\u{1b}[6 q");
    let narrow = LiveViewRect::new(20, 4, 4, 10);
    let (_, _, visible, _) = fixture.live_views.cursor(CLIENT, PLUGIN, &narrow).unwrap();
    assert!(!visible);
}

fn mouse_fixture() -> Fixture {
    let mut fixture = fixture();
    let mut screen = screen_of(&["hello"], 10, 3);
    screen.wants_mouse = true;
    let wants_mouse = MirrorTransportEvent::Message(SourceMessage::Frame {
        frame: fixture.encoder.encode(&screen).unwrap().bytes,
        alternate_screen: false,
    });
    fixture.event(wants_mouse);
    fixture
}

fn mouse_inputs(fixture: &Fixture) -> Vec<MouseEvent> {
    fixture
        .inputs()
        .into_iter()
        .filter_map(|input| match input {
            MirrorInput::Mouse(event) => Some(event),
            _ => None,
        })
        .collect()
}

fn is_left_release_at(event: &MouseEvent, line: i32, column: u16) -> bool {
    event.event_type == zellij_utils::input::mouse::MouseEventType::Release
        && event.left
        && event.position == Position::new(line, column)
}

#[test]
fn a_drag_cut_short_by_the_end_of_capture_releases_the_button_in_the_source() {
    let mut fixture = mouse_fixture();
    fixture.live_views.start_capture(KEY, true).unwrap();
    fixture
        .live_views
        .start_drag(KEY, DragButton::Left, Position::new(1, 2));
    fixture.live_views.drag_moved(CLIENT, Position::new(2, 4));
    assert!(fixture.live_views.release(&KEY, LiveViewRelease::Key));
    assert_eq!(fixture.live_views.drag_of(CLIENT), None);
    let sent = mouse_inputs(&fixture);
    assert_eq!(sent.len(), 1);
    assert!(is_left_release_at(&sent[0], 2, 4), "{:?}", sent[0]);
}

#[test]
fn a_drag_that_ends_normally_sends_nothing_more() {
    let mut fixture = mouse_fixture();
    fixture.live_views.start_capture(KEY, true).unwrap();
    fixture
        .live_views
        .start_drag(KEY, DragButton::Left, Position::new(1, 2));
    fixture.live_views.end_drag(CLIENT);
    assert!(!fixture.live_views.cut_drag(CLIENT));
    fixture.live_views.release(&KEY, LiveViewRelease::Key);
    assert!(mouse_inputs(&fixture).is_empty());
}

#[test]
fn capturing_another_view_or_removing_the_view_cuts_its_drag() {
    let mut fixture = mouse_fixture();
    let other = (PLUGIN + 1, CLIENT);
    let target = PaneAddress::local("api", PaneId::Terminal(3));
    let other_slot = slot(&fixture.recorded, 1, Some(target), false);
    fixture.live_views.insert(other, other_slot);
    fixture.live_views.start_capture(KEY, true).unwrap();
    fixture
        .live_views
        .start_drag(KEY, DragButton::Left, Position::new(0, 1));
    assert!(!fixture.live_views.cut_drag_of_view(&other));
    assert!(fixture.live_views.drag_of(CLIENT).is_some());
    fixture.live_views.start_capture(other, true).unwrap();
    assert_eq!(fixture.live_views.drag_of(CLIENT), None);
    assert!(is_left_release_at(&mouse_inputs(&fixture)[0], 0, 1));
    fixture.live_views.start_capture(KEY, true).unwrap();
    fixture
        .live_views
        .start_drag(KEY, DragButton::Left, Position::new(2, 3));
    assert!(fixture.live_views.remove(&KEY).is_some());
    assert_eq!(fixture.live_views.drag_of(CLIENT), None);
    let sent = mouse_inputs(&fixture);
    assert_eq!(sent.len(), 2);
    assert!(is_left_release_at(&sent[1], 2, 3));
}

#[test]
fn the_release_matches_the_button_held_and_another_clients_drag_is_untouched() {
    let mut fixture = mouse_fixture();
    fixture.live_views.start_capture(KEY, true).unwrap();
    let other_client = (PLUGIN, CLIENT + 1);
    fixture
        .live_views
        .start_drag(other_client, DragButton::Left, Position::new(0, 0));
    fixture
        .live_views
        .start_drag(KEY, DragButton::Right, Position::new(1, 1));
    assert!(fixture.live_views.cut_drag(CLIENT));
    let sent = mouse_inputs(&fixture);
    assert_eq!(sent.len(), 1);
    assert_eq!(
        sent[0].event_type,
        zellij_utils::input::mouse::MouseEventType::Release
    );
    assert!(sent[0].right && !sent[0].left);
    assert!(fixture.live_views.drag_of(CLIENT + 1).is_some());
}
