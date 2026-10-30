#![cfg(unix)]

use crossbeam::channel::{unbounded, Receiver};
use std::sync::Arc;
use std::time::{Duration, Instant};
use zellij_integration_tests::inbox::{press_esc, press_release_key};
use zellij_integration_tests::{
    keys, BackgroundTestSession, CliCaller, FakePtyHandle, GridSnapshot, LayoutInfo, TestClient,
    TestRunner, TestSession, TERMINAL_SIZE,
};
use zellij_utils::cli::CliAction;
use zellij_utils::data::{MirrorClosedReason, PaneAddress, PaneId, PipePopupPlacement};
use zellij_utils::pane_mirror::{
    LocalSocketConnector, MirrorAttach, MirrorConnector, MirrorInput, MirrorTransport,
    MirrorTransportEvent, SourceMessage, ViewerMessage, PANE_MIRROR_VERSION,
};
use zellij_utils::pane_size::Size;

const FIXTURE_PLUGIN_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../zellij-utils/assets/plugins/fixture-plugin-for-tests.wasm"
);
const FIXTURE_CONFIGURATION: &str = "live_view_fixture=true";
const PERMISSION_PROMPT: &str = "Allow? (y/n)";
const NO_TARGET: &str = "No pane selected";
const VIEW_SIZE: (u16, u16) = (40, 10);

fn fixture_plugin_url() -> String {
    format!("file:{}", FIXTURE_PLUGIN_PATH)
}

fn fixture_plugin_node() -> String {
    format!(
        "plugin location=\"{}\" {{\n live_view_fixture \"true\"\n }}",
        fixture_plugin_url()
    )
}

struct Viewer {
    zellij: TestSession,
    first_terminal: FakePtyHandle,
    second_terminal: FakePtyHandle,
}

fn tiled_viewer_layout() -> String {
    format!(
        r#"
        layout {{
            pane size=1 borderless=true {{
                plugin location="tab-bar"
            }}
            pane split_direction="vertical" {{
                pane size="60%" focus=true {{
                    {}
                }}
                pane split_direction="horizontal" {{
                    pane
                    pane
                }}
            }}
            pane size=2 borderless=true {{
                plugin location="status-bar"
            }}
        }}
        "#,
        fixture_plugin_node()
    )
}

fn floating_viewer_layout() -> String {
    format!(
        r#"
        layout {{
            pane size=1 borderless=true {{
                plugin location="tab-bar"
            }}
            pane split_direction="vertical" {{
                pane
                pane
            }}
            pane size=2 borderless=true {{
                plugin location="status-bar"
            }}
            floating_panes {{
                pane x=4 y=2 width=80 height=20 {{
                    {}
                }}
            }}
        }}
        "#,
        fixture_plugin_node()
    )
}

fn start_viewer() -> Viewer {
    start_viewer_with(&tiled_viewer_layout(), "")
}

fn start_viewer_with_config(extra_config: &str) -> Viewer {
    start_viewer_with(&tiled_viewer_layout(), extra_config)
}

fn start_viewer_with(layout: &str, extra_config: &str) -> Viewer {
    let zellij = TestRunner::new(TERMINAL_SIZE)
        .with_config(&format!("mouse_mode true\n{}", extra_config))
        .with_layout(LayoutInfo::Stringified(layout.to_owned()))
        .start();
    let first_terminal = zellij.expect_pty_spawn();
    let second_terminal = zellij.expect_pty_spawn();
    first_terminal.disable_echo();
    second_terminal.disable_echo();
    grant_the_fixture_permissions(&zellij);
    Viewer {
        zellij,
        first_terminal,
        second_terminal,
    }
}

fn grant_the_fixture_permissions(zellij: &TestSession) {
    zellij.wait_until("the fixture plugin asks for permissions", |grid_snapshot| {
        grid_snapshot.contains(PERMISSION_PROMPT)
    });
    zellij.send_stdin(b"y");
    zellij.wait_until("the fixture plugin is running", |grid_snapshot| {
        !grid_snapshot.contains(PERMISSION_PROMPT)
            && grid_snapshot.contains("LV none")
            && grid_snapshot.status_bar_appears()
    });
}

fn start_source() -> (BackgroundTestSession, FakePtyHandle) {
    start_source_with_config("")
}

fn start_source_with_config(extra_config: &str) -> (BackgroundTestSession, FakePtyHandle) {
    let source = TestRunner::new(TERMINAL_SIZE)
        .with_config(extra_config)
        .with_layout(LayoutInfo::Stringified(
            "layout {\n    pane\n    pane\n}\n".to_owned(),
        ))
        .skip_concurrency_slot()
        .start_in_background();
    let terminal = source.expect_pty_spawn();
    terminal.disable_echo();
    (source, terminal)
}

fn pipe_to_the_fixture(
    zellij: &TestSession,
    name: &str,
    args: &[(&str, String)],
    popup: Option<PipePopupPlacement>,
    caller: CliCaller,
) {
    let joined = std::iter::once(("from", "test".to_owned()))
        .chain(args.iter().cloned())
        .map(|(key, value)| format!("{}={}", key, value))
        .collect::<Vec<_>>()
        .join(",");
    let args = Some(joined.parse().unwrap());
    let pipe = CliAction::Pipe {
        name: Some(name.to_owned()),
        payload: None,
        args,
        plugin: Some(fixture_plugin_url()),
        plugin_configuration: Some(FIXTURE_CONFIGURATION.parse().unwrap()),
        force_launch_plugin: popup.is_some(),
        skip_plugin_cache: false,
        floating_plugin: None,
        in_place_plugin: None,
        plugin_cwd: None,
        plugin_title: None,
        popup,
        popup_no_focus: false,
    };
    zellij.run_cli_pipe(pipe, caller).wait_for_exit();
}

fn send_to_the_fixture(zellij: &TestSession, name: &str, args: &[(&str, String)]) {
    pipe_to_the_fixture(
        zellij,
        name,
        args,
        None,
        CliCaller {
            pane_id: None,
            stdin: None,
        },
    );
}

fn target_args(session: &str, pane: u32) -> Vec<(&'static str, String)> {
    vec![("session", session.to_owned()), ("pane", pane.to_string())]
}

fn show_pane(zellij: &TestSession, session: &str, pane: u32) {
    show_pane_with(zellij, session, pane, &[]);
}

fn show_pane_with(
    zellij: &TestSession,
    session: &str,
    pane: u32,
    extra_args: &[(&'static str, String)],
) {
    let mut args = target_args(session, pane);
    args.extend(extra_args.iter().cloned());
    send_to_the_fixture(zellij, "live_view_set", &args);
}

fn capture_input(zellij: &TestSession) -> GridSnapshot {
    send_to_the_fixture(zellij, "live_view_input", &[("capture", "true".to_owned())]);
    status_shows(zellij, "capture starts", "captured=true")
}

fn release_input(zellij: &TestSession) -> GridSnapshot {
    send_to_the_fixture(
        zellij,
        "live_view_input",
        &[("capture", "false".to_owned())],
    );
    status_shows(zellij, "the plugin ends capture", "released=Some(Plugin)")
}

fn status_shows(zellij: &TestSession, what: &str, text: &str) -> GridSnapshot {
    zellij.wait_until(what, |grid_snapshot| grid_snapshot.contains(text))
}

fn occurrences(grid_snapshot: &GridSnapshot, text: &str) -> usize {
    grid_snapshot.text.matches(text).count()
}

#[derive(Debug, Clone, Copy)]
struct Area {
    x: usize,
    y: usize,
    cols: usize,
    rows: usize,
}

impl Area {
    fn contains(&self, column: usize, row: usize) -> bool {
        column >= self.x && column < self.x + self.cols && row >= self.y && row < self.y + self.rows
    }
}

fn border_of(grid_snapshot: &GridSnapshot) -> Option<Area> {
    let lines = grid_snapshot.lines();
    let (column, row) = grid_snapshot.position_of("+--")?;
    let top: Vec<char> = lines[row].chars().collect();
    let right = (column + 1..top.len()).find(|x| top[*x] == '+')?;
    let bottom = (row + 1..lines.len()).find(|y| {
        lines[*y]
            .chars()
            .nth(column)
            .map(|character| character == '+')
            .unwrap_or(false)
    })?;
    Some(Area {
        x: column + 1,
        y: row + 1,
        cols: right - column - 1,
        rows: bottom - row - 1,
    })
}

fn assert_inside_the_view(grid_snapshot: &GridSnapshot, text: &str) -> (usize, usize) {
    let area = border_of(grid_snapshot)
        .unwrap_or_else(|| panic!("the fixture draws no border:\n{}", grid_snapshot.text));
    let (column, row) = grid_snapshot
        .position_of(text)
        .unwrap_or_else(|| panic!("{:?} is not shown:\n{}", text, grid_snapshot.text));
    assert!(
        area.contains(column, row) && area.contains(column + text.chars().count() - 1, row),
        "{:?} at {:?} is outside the view {:?}:\n{}",
        text,
        (column, row),
        area,
        grid_snapshot.text
    );
    (column, row)
}

fn connect_directly(
    session: &str,
    pane: u32,
    view_only: bool,
    version: u32,
) -> (Box<dyn MirrorTransport>, Receiver<MirrorTransportEvent>) {
    connect_with(
        session,
        pane,
        MirrorAttach {
            version,
            view_only,
            ..MirrorAttach::new(PaneId::Terminal(pane), Size { cols: 40, rows: 10 })
        },
    )
}

fn connect_with(
    session: &str,
    pane: u32,
    attach: MirrorAttach,
) -> (Box<dyn MirrorTransport>, Receiver<MirrorTransportEvent>) {
    let (sender, receiver) = unbounded();
    let transport = LocalSocketConnector.connect(
        &PaneAddress::local(session, PaneId::Terminal(pane)),
        attach,
        Arc::new(move |event| {
            let _ = sender.send(event);
        }),
    );
    (transport, receiver)
}

fn wait_for_event(
    receiver: &Receiver<MirrorTransportEvent>,
    what: &str,
    predicate: impl Fn(&MirrorTransportEvent) -> bool,
) -> MirrorTransportEvent {
    let deadline = Instant::now() + zellij_integration_tests::default_timeout();
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        match receiver.recv_timeout(remaining) {
            Ok(event) if predicate(&event) => return event,
            Ok(_) => {},
            Err(_) => panic!("timed out waiting for {}", what),
        }
    }
}

fn is_frame(event: &MirrorTransportEvent) -> bool {
    matches!(
        event,
        MirrorTransportEvent::Message(SourceMessage::Frame { .. })
    )
}

struct WatchedSource {
    source: BackgroundTestSession,
    terminal: FakePtyHandle,
    view: TestClient,
    layout_size: (u16, u16),
}

fn start_watched_source(extra_config: &str) -> WatchedSource {
    let (source, terminal) = start_source_with_config(extra_config);
    terminal.output(b"$ watched source\r\n");
    let view = source.attach_client(TERMINAL_SIZE);
    view.wait_until("the source view shows the source", |grid_snapshot| {
        grid_snapshot.contains("watched source")
    });
    let layout_size = wait_for_the_layout_size(&terminal);
    WatchedSource {
        source,
        terminal,
        view,
        layout_size,
    }
}

fn wait_for_the_layout_size(terminal: &FakePtyHandle) -> (u16, u16) {
    terminal.wait_for_size(
        "the source pane has the size of the attached source view",
        |cols, rows| {
            cols as usize + 10 > TERMINAL_SIZE.cols && (rows as usize) < TERMINAL_SIZE.rows
        },
    )
}

fn wait_for_the_pin_at(terminal: &FakePtyHandle, size: (u16, u16), what: &str) {
    terminal.wait_for_size(what, |cols, rows| (cols, rows) == size);
}

fn wait_for_the_release(terminal: &FakePtyHandle, layout_size: (u16, u16), what: &str) {
    terminal.wait_for_size(what, |cols, rows| (cols, rows) == layout_size);
}

fn pinned_marker((cols, rows): (u16, u16)) -> String {
    format!("mirrored {}x{}", cols, rows)
}

fn longest_run_of(grid_snapshot: &GridSnapshot, character: char) -> usize {
    grid_snapshot
        .lines()
        .iter()
        .map(|line| {
            line.split(|c| c != character)
                .map(|run| run.chars().count())
                .max()
                .unwrap_or(0)
        })
        .max()
        .unwrap_or(0)
}

fn assert_not_pinned_after_a_barrier(watched: &WatchedSource, barrier: &str) {
    watched
        .terminal
        .output(format!("\r\n$ {}\r\n", barrier).as_bytes());
    let grid_snapshot = watched
        .view
        .wait_until("the source view shows the barrier", |grid_snapshot| {
            grid_snapshot.contains(barrier)
        });
    assert!(
        !grid_snapshot.contains("mirrored "),
        "the source pane is pinned:\n{}",
        grid_snapshot.text
    );
    assert_eq!(watched.terminal.size(), Some(watched.layout_size));
}

fn is_pin_state(event: &MirrorTransportEvent, pinned: bool, size: Option<Size>) -> bool {
    match event {
        MirrorTransportEvent::Message(SourceMessage::PinState {
            pinned: reported,
            pane_size,
        }) => *reported == pinned && size.map(|size| size == *pane_size).unwrap_or(true),
        _ => false,
    }
}

fn is_any_pin_state(event: &MirrorTransportEvent) -> bool {
    matches!(
        event,
        MirrorTransportEvent::Message(SourceMessage::PinState { .. })
    )
}

fn pinning_attach(pane: u32, size: Size) -> MirrorAttach {
    MirrorAttach {
        focused: true,
        resize_allowed: true,
        ..MirrorAttach::new(PaneId::Terminal(pane), size)
    }
}

fn as_pty_size(size: Size) -> (u16, u16) {
    (size.cols as u16, size.rows as u16)
}

struct PinnedView {
    watched: WatchedSource,
    viewer: Viewer,
}

impl PinnedView {
    fn terminal(&self) -> &FakePtyHandle {
        &self.watched.terminal
    }
    fn layout_size(&self) -> (u16, u16) {
        self.watched.layout_size
    }
    fn zellij(&self) -> &TestSession {
        &self.viewer.zellij
    }
}

fn show_the_watched_source(extra_args: &[(&'static str, String)]) -> PinnedView {
    let watched = start_watched_source("");
    let viewer = start_viewer();
    show_pane_with(
        &viewer.zellij,
        watched.source.session_name(),
        watched.terminal.terminal_id(),
        extra_args,
    );
    status_shows(&viewer.zellij, "the live view is live", "watched source");
    PinnedView { watched, viewer }
}

fn capture_and_pin() -> PinnedView {
    let pinned = show_the_watched_source(&[]);
    wait_for_the_pin_at(
        pinned.terminal(),
        VIEW_SIZE,
        "showing the source pins it at the size of the live view",
    );
    capture_input(pinned.zellij());
    pinned
        .zellij()
        .wait_until("the captured view holds the pin", |grid_snapshot| {
            grid_snapshot.contains("captured=true") && grid_snapshot.contains("pinned=true")
        });
    pinned
}

fn assert_keeps_the_size_after_a_barrier(watched: &WatchedSource, size: (u16, u16), barrier: &str) {
    watched
        .terminal
        .output(format!("\r\n$ {}\r\n", barrier).as_bytes());
    watched
        .view
        .wait_until("the source view shows the barrier", |grid_snapshot| {
            grid_snapshot.contains(barrier)
        });
    assert_eq!(watched.terminal.size(), Some(size));
}

fn give_the_size_back_by_typing_in_the_source(watched: &WatchedSource) {
    watched.view.send_stdin(b"k");
    watched
        .terminal
        .wait_for_stdin("the local key reached the pane", |stdin| {
            stdin.ends_with(b"k")
        });
    wait_for_the_release(
        &watched.terminal,
        watched.layout_size,
        "typing in the source session gives the pane its layout size back",
    );
}

fn client_id_of(grid_snapshot: &GridSnapshot) -> String {
    let line = grid_snapshot
        .lines()
        .into_iter()
        .find(|line| line.contains("client="))
        .unwrap_or_else(|| panic!("the fixture shows no client id:\n{}", grid_snapshot.text));
    line.split("client=")
        .nth(1)
        .and_then(|rest| rest.split_whitespace().next())
        .expect("a client id")
        .to_owned()
}

#[test]
fn live_output_with_colour_a_wide_character_and_a_link_appears_inside_the_rectangle() {
    let (source, terminal) = start_source();
    terminal.output(b"$ first line from the source\r\n");
    let mut viewer = start_viewer();
    show_pane(
        &viewer.zellij,
        source.session_name(),
        terminal.terminal_id(),
    );
    let grid_snapshot = status_shows(
        &viewer.zellij,
        "the live view shows the source pane",
        "first line from the source",
    );
    assert_inside_the_view(&grid_snapshot, "first line from the source");
    assert_eq!(occurrences(&grid_snapshot, "first line from the source"), 1);
    terminal.output(
        b"\x1b[1;31mcoloured\x1b[0m and \xe4\xb8\xad wide \x1b]8;;https://example.com/live\x1b\\linked\x1b]8;;\x1b\\\r\n",
    );
    let grid_snapshot = status_shows(
        &viewer.zellij,
        "the live view follows new output",
        "coloured and 中 wide linked",
    );
    let (column, row) = assert_inside_the_view(&grid_snapshot, "coloured and 中 wide linked");
    assert!(grid_snapshot.char_is_bold(column, row));
    assert!(grid_snapshot.cell_foreground(column, row).is_some());
    assert!(!grid_snapshot.char_is_bold(column + "coloured ".len(), row));
    assert_eq!(occurrences(&grid_snapshot, "coloured and"), 1);
    let raw = String::from_utf8_lossy(&viewer.zellij.raw_bytes()).into_owned();
    assert!(
        raw.contains("https://example.com/live"),
        "the link did not reach the viewer"
    );
    viewer.zellij.quit();
}

#[test]
fn keys_paste_and_a_click_reach_the_source_only_while_capture_is_on() {
    let (source, terminal) = start_source();
    terminal.output(b"\x1b[?2004h\x1b[?1h\x1b[?1000h\x1b[?1006h$ ready\r\n");
    let mut viewer = start_viewer();
    show_pane(
        &viewer.zellij,
        source.session_name(),
        terminal.terminal_id(),
    );
    status_shows(&viewer.zellij, "the live view is live", "$ ready");
    viewer.zellij.send_stdin(b"w");
    status_shows(
        &viewer.zellij,
        "without capture the key reaches the plugin",
        "input key=w",
    );
    capture_input(&viewer.zellij);
    wait_for_the_pin_at(&terminal, VIEW_SIZE, "capture pins the source");
    terminal.output(b"\x1b[H\x1b[2Jmouse target here");
    let grid_snapshot = status_shows(
        &viewer.zellij,
        "the live view shows the redrawn source",
        "mouse target here",
    );
    viewer.zellij.send_stdin(b"abc");
    terminal.wait_for_stdin("typed keys reached the source", |stdin| {
        stdin.ends_with(b"abc")
    });
    viewer.zellij.send_stdin(&keys::UP);
    terminal.wait_for_stdin(
        "an arrow key in application cursor mode reached the source",
        |stdin| stdin.ends_with(b"\x1bOA"),
    );
    let mut paste = keys::BRACKETED_PASTE_START.to_vec();
    paste.extend_from_slice(b"pasted text");
    paste.extend_from_slice(&keys::BRACKETED_PASTE_END);
    viewer.zellij.send_stdin(&paste);
    terminal.wait_for_stdin("the paste reached the source", |stdin| {
        String::from_utf8_lossy(stdin).contains("\x1b[200~pasted text\x1b[201~")
    });
    let (column, row) = assert_inside_the_view(&grid_snapshot, "mouse target here");
    viewer.zellij.send_stdin(&keys::sgr_click(column + 2, row));
    let stdin = terminal.wait_for_stdin("the click reached the source", |stdin| {
        String::from_utf8_lossy(stdin).contains("\x1b[<0;3;1M")
    });
    assert!(
        !stdin.contains(&b'w'),
        "a key typed before capture reached the source: {:?}",
        String::from_utf8_lossy(&stdin)
    );
    assert!(!viewer.zellij.snapshot().contains("input key=a"));
    viewer.zellij.quit();
}

#[test]
fn the_wheel_scrolls_an_alternate_screen_program_without_mouse_tracking() {
    let (source, terminal) = start_source();
    terminal.output(b"\x1b[?1049h\x1b[H\x1b[2Jalternate screen program");
    let mut viewer = start_viewer();
    show_pane(
        &viewer.zellij,
        source.session_name(),
        terminal.terminal_id(),
    );
    status_shows(
        &viewer.zellij,
        "the live view shows the alternate screen",
        "alternate screen program",
    );
    let grid_snapshot = capture_input(&viewer.zellij);
    let (column, row) = assert_inside_the_view(&grid_snapshot, "alternate screen program");
    viewer
        .zellij
        .send_stdin(format!("\u{1b}[<64;{};{}M", column + 3, row + 1).as_bytes());
    terminal.wait_for_stdin("the wheel arrived as arrow keys", |stdin| {
        stdin.ends_with(b"\x1b[A\x1b[A\x1b[A")
    });
    viewer.zellij.quit();
}

#[test]
fn the_release_key_never_reaches_the_source_and_capture_ends() {
    let (source, terminal) = start_source();
    terminal.output(b"$ release key source\r\n");
    let mut viewer = start_viewer();
    show_pane(
        &viewer.zellij,
        source.session_name(),
        terminal.terminal_id(),
    );
    status_shows(
        &viewer.zellij,
        "the live view is live",
        "release key source",
    );
    capture_input(&viewer.zellij);
    viewer.zellij.send_stdin(b"a");
    terminal.wait_for_stdin("the first key reached the source", |stdin| {
        stdin.ends_with(b"a")
    });
    press_release_key(&viewer.zellij);
    status_shows(
        &viewer.zellij,
        "the release key ends capture",
        "captured=false released=Some(Key)",
    );
    viewer.zellij.send_stdin(b"b");
    status_shows(
        &viewer.zellij,
        "after the release keys reach the plugin",
        "input key=b",
    );
    capture_input(&viewer.zellij);
    viewer.zellij.send_stdin(b"c");
    let stdin = terminal.wait_for_stdin("the next captured key reached the source", |stdin| {
        stdin.ends_with(b"c")
    });
    assert!(
        stdin.ends_with(b"ac"),
        "something reached the source between the captures: {:?}",
        String::from_utf8_lossy(&stdin)
    );
    viewer.zellij.quit();
}

#[test]
fn a_click_outside_the_rectangle_ends_capture_and_reaches_the_plugin() {
    let (source, terminal) = start_source();
    terminal.output(b"$ click outside source\r\n");
    let mut viewer = start_viewer();
    show_pane(
        &viewer.zellij,
        source.session_name(),
        terminal.terminal_id(),
    );
    status_shows(
        &viewer.zellij,
        "the live view is live",
        "click outside source",
    );
    let grid_snapshot = capture_input(&viewer.zellij);
    let (column, row) = grid_snapshot.position_of("LV ").unwrap();
    viewer.zellij.send_stdin(&keys::sgr_click(column + 1, row));
    let grid_snapshot = status_shows(
        &viewer.zellij,
        "a click outside ends capture",
        "released=Some(ClickOutside)",
    );
    assert!(grid_snapshot.contains("captured=false"));
    status_shows(
        &viewer.zellij,
        "the click reached the plugin",
        "input mouse=",
    );
    viewer.zellij.quit();
}

#[test]
fn focusing_another_pane_ends_capture() {
    let (source, terminal) = start_source();
    terminal.output(b"$ focus source\r\n");
    let mut viewer = start_viewer();
    show_pane(
        &viewer.zellij,
        source.session_name(),
        terminal.terminal_id(),
    );
    status_shows(&viewer.zellij, "the live view is live", "focus source");
    capture_input(&viewer.zellij);
    viewer.zellij.run_cli_action(CliAction::MoveFocus {
        direction: zellij_utils::data::Direction::Right,
    });
    status_shows(
        &viewer.zellij,
        "moving the focus ends capture",
        "released=Some(FocusLost)",
    );
    viewer.zellij.send_stdin(b"q");
    zellij_integration_tests::wait_for("keys go to the newly focused pane", || {
        viewer.first_terminal.stdin_bytes().ends_with(b"q")
            || viewer.second_terminal.stdin_bytes().ends_with(b"q")
    });
    assert!(!terminal.stdin_bytes().contains(&b'q'));
    viewer.zellij.quit();
}

#[test]
fn a_shown_view_pins_the_source_and_crops_the_source_view_and_capture_keeps_the_pin() {
    let watched = start_watched_source("");
    let long_line = "X".repeat(100);
    watched
        .terminal
        .output(format!("{}\r\n", long_line).as_bytes());
    watched
        .view
        .wait_until("the source view shows the long line", |grid_snapshot| {
            grid_snapshot.contains(&long_line)
        });
    let viewer = start_viewer();
    show_pane(
        &viewer.zellij,
        watched.source.session_name(),
        watched.terminal.terminal_id(),
    );
    let pinned = PinnedView { watched, viewer };
    wait_for_the_pin_at(pinned.terminal(), VIEW_SIZE, "showing pins the source");
    let marker = pinned_marker(VIEW_SIZE);
    let grid_snapshot = pinned.watched.view.wait_until(
        "the source view shows the pin marker and only the program's area",
        |grid_snapshot| grid_snapshot.contains(&marker) && !grid_snapshot.contains(&long_line),
    );
    assert!(longest_run_of(&grid_snapshot, 'X') <= VIEW_SIZE.0 as usize);
    status_shows(
        pinned.zellij(),
        "the plugin sees the pin and the source size",
        "pinned=true source=40x10",
    );
    pinned
        .zellij()
        .wait_until("the live view shows the whole program", |grid_snapshot| {
            longest_run_of(grid_snapshot, 'X') == VIEW_SIZE.0 as usize
        });
    capture_input(pinned.zellij());
    release_input(pinned.zellij());
    assert_keeps_the_size_after_a_barrier(&pinned.watched, VIEW_SIZE, "after capture");
    let mut viewer = pinned.viewer;
    viewer.zellij.quit();
}

#[test]
fn clearing_the_live_view_gives_the_pane_its_layout_size_back_at_once() {
    let pinned = capture_and_pin();
    send_to_the_fixture(pinned.zellij(), "live_view_clear", &[]);
    let grid_snapshot = status_shows(
        pinned.zellij(),
        "the plugin hears that capture ended",
        "released=Some(Cleared)",
    );
    assert!(!grid_snapshot.contains("watched source"));
    wait_for_the_release(
        pinned.terminal(),
        pinned.layout_size(),
        "clearing the view gives the pane its layout size back",
    );
    assert_not_pinned_after_a_barrier(&pinned.watched, "after clearing");
    let mut viewer = pinned.viewer;
    viewer.zellij.quit();
}

#[test]
fn switching_the_target_keeps_the_old_pane_at_its_size_until_it_is_used_in_its_session() {
    let pinned = capture_and_pin();
    let session = pinned.zellij().session_name().to_owned();
    pinned
        .viewer
        .first_terminal
        .output(b"$ text of the second target\r\n");
    show_pane(
        pinned.zellij(),
        &session,
        pinned.viewer.first_terminal.terminal_id(),
    );
    pinned
        .zellij()
        .wait_until("the second target replaced the first", |grid_snapshot| {
            occurrences(grid_snapshot, "text of the second target") >= 2
                && !grid_snapshot.contains("watched source")
                && grid_snapshot.contains("released=Some(Cleared)")
        });
    assert_keeps_the_size_after_a_barrier(&pinned.watched, VIEW_SIZE, "after the switch");
    give_the_size_back_by_typing_in_the_source(&pinned.watched);
    let mut viewer = pinned.viewer;
    viewer.zellij.quit();
}

#[test]
fn closing_the_plugin_pane_gives_the_pane_its_layout_size_back() {
    let pinned = capture_and_pin();
    pinned.zellij().send_stdin(&keys::ctrl('p'));
    pinned.zellij().send_stdin(&keys::key('x'));
    pinned
        .zellij()
        .wait_until("the plugin pane is gone", |grid_snapshot| {
            !grid_snapshot.contains("LV ")
        });
    wait_for_the_release(
        pinned.terminal(),
        pinned.layout_size(),
        "closing the plugin pane gives the pane its layout size back",
    );
    assert_not_pinned_after_a_barrier(&pinned.watched, "after closing");
    let mut viewer = pinned.viewer;
    viewer.zellij.quit();
}

#[test]
fn switching_the_target_and_then_clearing_the_view_gives_the_old_pane_its_size_back() {
    let pinned = capture_and_pin();
    let session = pinned.zellij().session_name().to_owned();
    pinned
        .viewer
        .first_terminal
        .output(b"$ text of the next target\r\n");
    show_pane(
        pinned.zellij(),
        &session,
        pinned.viewer.first_terminal.terminal_id(),
    );
    pinned
        .zellij()
        .wait_until("the next target replaced the first", |grid_snapshot| {
            occurrences(grid_snapshot, "text of the next target") >= 2
                && !grid_snapshot.contains("watched source")
        });
    assert_keeps_the_size_after_a_barrier(&pinned.watched, VIEW_SIZE, "after the switch");
    let sizes_before = pinned.terminal().size_history();
    show_pane(
        pinned.zellij(),
        pinned.watched.source.session_name(),
        pinned.watched.terminal.terminal_id(),
    );
    status_shows(
        pinned.zellij(),
        "going back shows the first target again",
        "watched source",
    );
    std::thread::sleep(Duration::from_millis(700));
    assert_eq!(
        pinned.terminal().size_history(),
        sizes_before,
        "going back to the parked target resized the pane"
    );
    send_to_the_fixture(pinned.zellij(), "live_view_clear", &[]);
    wait_for_the_release(
        pinned.terminal(),
        pinned.layout_size(),
        "clearing the view gives the parked pane its layout size back",
    );
    let mut viewer = pinned.viewer;
    viewer.zellij.quit();
}

fn acknowledge_frames(viewer: &dyn MirrorTransport, events: Vec<MirrorTransportEvent>) {
    for event in events {
        if let MirrorTransportEvent::Message(SourceMessage::Frame { frame, .. }) = event {
            let seq = zellij_utils::structured_render::decode(&frame)
                .expect("a readable frame")
                .header()
                .seq;
            assert!(viewer.send(ViewerMessage::FrameAck { seq }));
        }
    }
}

#[test]
fn a_parked_viewer_gets_no_frames_keeps_the_size_and_gives_it_back_when_it_detaches() {
    let watched = start_watched_source("");
    let session = watched.source.session_name().to_owned();
    let pane = watched.terminal.terminal_id();
    let viewer_size = Size { cols: 40, rows: 10 };
    let (mut viewer, events) = connect_with(&session, pane, pinning_attach(pane, viewer_size));
    wait_for_event(&events, "the pin", |event| {
        is_pin_state(event, true, Some(viewer_size))
    });
    wait_for_the_pin_at(
        &watched.terminal,
        as_pty_size(viewer_size),
        "the viewer pins the pane",
    );
    assert!(viewer.send(ViewerMessage::Park));
    assert_keeps_the_size_after_a_barrier(&watched, as_pty_size(viewer_size), "while parked");
    std::thread::sleep(Duration::from_millis(300));
    acknowledge_frames(viewer.as_ref(), events.try_iter().collect());
    watched.terminal.output(b"$ not sent while parked\r\n");
    watched
        .view
        .wait_until("the source shows the new output", |grid_snapshot| {
            grid_snapshot.contains("not sent while parked")
        });
    std::thread::sleep(Duration::from_millis(300));
    assert!(
        !events.try_iter().any(|event| is_frame(&event)),
        "a parked viewer got a frame"
    );
    assert!(viewer.send(ViewerMessage::Unpark {
        resize_allowed: true
    }));
    wait_for_event(&events, "a frame after unparking", is_frame);
    assert_eq!(watched.terminal.size(), Some(as_pty_size(viewer_size)));
    assert!(viewer.send(ViewerMessage::Park));
    viewer.close();
    wait_for_the_release(
        &watched.terminal,
        watched.layout_size,
        "a parked viewer that detaches gives the size back",
    );
}

#[test]
fn a_pane_kept_by_one_viewer_stays_at_its_size_when_another_viewer_closes() {
    let watched = start_watched_source("");
    let session = watched.source.session_name().to_owned();
    let pane = watched.terminal.terminal_id();
    let first_size = Size { cols: 40, rows: 10 };
    let second_size = Size { cols: 50, rows: 12 };
    let (mut first, first_events) = connect_with(&session, pane, pinning_attach(pane, first_size));
    wait_for_event(&first_events, "the first pin", |event| {
        is_pin_state(event, true, Some(first_size))
    });
    wait_for_the_pin_at(
        &watched.terminal,
        as_pty_size(first_size),
        "the first viewer pins the pane",
    );
    assert!(first.send(ViewerMessage::Park));
    let (mut second, second_events) =
        connect_with(&session, pane, pinning_attach(pane, second_size));
    wait_for_event(&second_events, "the second pin", |event| {
        is_pin_state(event, true, Some(second_size))
    });
    wait_for_the_pin_at(
        &watched.terminal,
        as_pty_size(second_size),
        "the second viewer pins the pane",
    );
    second.close();
    wait_for_the_pin_at(
        &watched.terminal,
        as_pty_size(first_size),
        "the size the first viewer keeps applies again",
    );
    assert_keeps_the_size_after_a_barrier(&watched, as_pty_size(first_size), "after the close");
    first.close();
    wait_for_the_release(
        &watched.terminal,
        watched.layout_size,
        "closing the last viewer gives the size back",
    );
}

fn attach_a_fixture_client(zellij: &TestSession) -> (TestClient, String) {
    let client = zellij.attach_client(TERMINAL_SIZE);
    let grid_snapshot = client.wait_until(
        "the new client sees the fixture or its permission prompt",
        |grid_snapshot| grid_snapshot.contains("LV ") || grid_snapshot.contains(PERMISSION_PROMPT),
    );
    if grid_snapshot.contains(PERMISSION_PROMPT) {
        client.send_stdin(b"y");
    }
    let grid_snapshot = client.wait_until("the new client sees the fixture", |grid_snapshot| {
        grid_snapshot.contains("client=") && !grid_snapshot.contains(PERMISSION_PROMPT)
    });
    let client_id = client_id_of(&grid_snapshot);
    (client, client_id)
}

#[test]
fn a_viewer_client_that_detaches_releases_the_pin() {
    let watched = start_watched_source("");
    let session = watched.source.session_name().to_owned();
    let pane = watched.terminal.terminal_id();
    let mut viewer = start_viewer();
    let first_id = client_id_of(&status_shows(
        &viewer.zellij,
        "the first client sees the fixture",
        "client=",
    ));
    let (second_client, second_id) = attach_a_fixture_client(&viewer.zellij);
    assert_ne!(first_id, second_id);
    let mut first_args = target_args(&session, pane);
    first_args.push(("only_client", first_id.clone()));
    send_to_the_fixture(&viewer.zellij, "live_view_set", &first_args);
    let mut second_args = target_args(&session, pane);
    second_args.push(("only_client", second_id.clone()));
    send_to_the_fixture(&viewer.zellij, "live_view_set", &second_args);
    status_shows(
        &viewer.zellij,
        "the first client shows the source",
        "watched source",
    );
    second_client.wait_until("the second client shows the source", |grid_snapshot| {
        grid_snapshot.contains("watched source")
    });
    send_to_the_fixture(
        &viewer.zellij,
        "live_view_input",
        &[("capture", "true".to_owned()), ("only_client", second_id)],
    );
    wait_for_the_pin_at(
        &watched.terminal,
        VIEW_SIZE,
        "the second client pins the source",
    );
    second_client.detach();
    watched.terminal.output(b"$ after the detach\r\n");
    status_shows(
        &viewer.zellij,
        "the first client's live view keeps working and takes the pin over",
        "after the detach",
    );
    status_shows(
        &viewer.zellij,
        "the first client's view holds the pin",
        "pinned=true",
    );
    assert_eq!(watched.terminal.size(), Some(VIEW_SIZE));
    let (third_client, third_id) = attach_a_fixture_client(&viewer.zellij);
    let third_grid = third_client.wait_until(
        "the client attached after the detach is drawn",
        |grid_snapshot| grid_snapshot.contains("LV none"),
    );
    assert!(
        !third_grid.contains("after the detach") && !third_grid.contains("watched source"),
        "the detached client's live view is still drawn for client {}:\n{}",
        third_id,
        third_grid.text
    );
    press_esc(&third_client);
    third_client.wait_until(
        "an esc of a client that never captured reaches its plugin",
        |grid_snapshot| grid_snapshot.contains("input key=ESC"),
    );
    third_client.detach();
    viewer.zellij.quit();
}

#[test]
fn a_pin_that_is_not_renewed_expires_and_the_pane_keeps_its_size_until_used_in_its_session() {
    std::env::set_var(zellij_utils::pane_mirror::MIRROR_PIN_LEASE_ENV_VAR, "400");
    let watched = start_watched_source("");
    let viewer_size = Size { cols: 40, rows: 10 };
    let pane = watched.terminal.terminal_id();
    let (_transport, events) = connect_with(
        watched.source.session_name(),
        pane,
        pinning_attach(pane, viewer_size),
    );
    wait_for_event(&events, "the pin", |event| {
        is_pin_state(event, true, Some(viewer_size))
    });
    wait_for_the_pin_at(
        &watched.terminal,
        as_pty_size(viewer_size),
        "the program has the viewer's size",
    );
    wait_for_event(&events, "the lease to expire", |event| {
        is_pin_state(event, false, None)
    });
    assert_keeps_the_size_after_a_barrier(&watched, as_pty_size(viewer_size), "after the lease");
    give_the_size_back_by_typing_in_the_source(&watched);
}

#[test]
fn a_captured_live_view_keeps_renewing_its_pin_with_a_short_lease() {
    std::env::set_var(zellij_utils::pane_mirror::MIRROR_PIN_LEASE_ENV_VAR, "400");
    let pinned = capture_and_pin();
    let started = Instant::now();
    while started.elapsed() < Duration::from_millis(1200) {
        assert_eq!(pinned.terminal().size(), Some(VIEW_SIZE));
        pinned.watched.terminal.output(b".");
        pinned
            .watched
            .view
            .wait_until("the source view keeps updating", |grid_snapshot| {
                grid_snapshot.contains(&pinned_marker(VIEW_SIZE))
            });
    }
    release_input(pinned.zellij());
    assert_keeps_the_size_after_a_barrier(&pinned.watched, VIEW_SIZE, "after capture");
    let mut viewer = pinned.viewer;
    viewer.zellij.quit();
}

#[test]
fn the_most_recent_viewer_wins_and_turning_its_resizing_off_hands_the_pin_back() {
    let watched = start_watched_source("");
    let session = watched.source.session_name().to_owned();
    let pane = watched.terminal.terminal_id();
    let first_size = Size { cols: 40, rows: 10 };
    let second_size = Size { cols: 50, rows: 12 };
    let (first, first_events) = connect_with(&session, pane, pinning_attach(pane, first_size));
    wait_for_event(&first_events, "the first pin", |event| {
        is_pin_state(event, true, Some(first_size))
    });
    let (second, second_events) = connect_with(&session, pane, pinning_attach(pane, second_size));
    wait_for_event(&second_events, "the second pin", |event| {
        is_pin_state(event, true, Some(second_size))
    });
    wait_for_event(&first_events, "the first viewer loses the pin", |event| {
        is_pin_state(event, false, Some(second_size))
    });
    assert!(second.send(ViewerMessage::Resize {
        size: second_size,
        focused: false,
        resize_allowed: false,
    }));
    wait_for_event(&first_events, "the pin is handed back", |event| {
        is_pin_state(event, true, Some(first_size))
    });
    assert!(first.send(ViewerMessage::Resize {
        size: first_size,
        focused: false,
        resize_allowed: false,
    }));
    wait_for_the_release(
        &watched.terminal,
        watched.layout_size,
        "nobody holds the pin",
    );
}

#[test]
fn a_newer_attach_and_then_a_capture_each_claim_the_pin_from_the_viewer_that_holds_it() {
    let watched = start_watched_source("");
    let session = watched.source.session_name().to_owned();
    let pane = watched.terminal.terminal_id();
    let first_size = Size { cols: 40, rows: 10 };
    let second_size = Size { cols: 50, rows: 12 };
    let third_size = Size { cols: 44, rows: 11 };
    let shown = |size: Size| MirrorAttach {
        focused: false,
        ..pinning_attach(pane, size)
    };
    let (first, first_events) = connect_with(&session, pane, shown(first_size));
    wait_for_event(&first_events, "the first pin", |event| {
        is_pin_state(event, true, Some(first_size))
    });
    wait_for_the_pin_at(
        &watched.terminal,
        as_pty_size(first_size),
        "the first viewer pins",
    );
    let (_second, second_events) = connect_with(&session, pane, shown(second_size));
    wait_for_event(&second_events, "the newer attach claims the pin", |event| {
        is_pin_state(event, true, Some(second_size))
    });
    wait_for_event(&first_events, "the first viewer loses the pin", |event| {
        is_pin_state(event, false, Some(second_size))
    });
    wait_for_the_pin_at(
        &watched.terminal,
        as_pty_size(second_size),
        "the newer attach resizes the pane",
    );
    assert!(first.send(ViewerMessage::Resize {
        size: first_size,
        focused: true,
        resize_allowed: true,
    }));
    wait_for_event(&first_events, "a capture claims the pin back", |event| {
        is_pin_state(event, true, Some(first_size))
    });
    wait_for_event(
        &second_events,
        "the newer viewer loses the pin to the capture",
        |event| is_pin_state(event, false, Some(first_size)),
    );
    wait_for_the_pin_at(
        &watched.terminal,
        as_pty_size(first_size),
        "the capture resizes the pane while the other viewer still asks to resize",
    );
    let (_third, third_events) = connect_with(&session, pane, shown(third_size));
    wait_for_event(
        &third_events,
        "an attach after the capture claims the pin",
        |event| is_pin_state(event, true, Some(third_size)),
    );
    wait_for_the_pin_at(
        &watched.terminal,
        as_pty_size(third_size),
        "the latest attach resizes the pane",
    );
}

#[test]
fn two_clients_have_their_own_live_views_and_captures() {
    let source = TestRunner::new(TERMINAL_SIZE)
        .with_layout(LayoutInfo::Stringified(
            "layout {\n    pane\n    pane\n}\n".to_owned(),
        ))
        .skip_concurrency_slot()
        .start_in_background();
    let first_terminal = source.expect_pty_spawn();
    let second_terminal = source.expect_pty_spawn();
    first_terminal.disable_echo();
    second_terminal.disable_echo();
    first_terminal.output(b"$ shown to the first client\r\n");
    second_terminal.output(b"$ shown to the second client\r\n");
    let source_view = source.attach_client(TERMINAL_SIZE);
    source_view.wait_until("the source view is up", |grid_snapshot| {
        grid_snapshot.contains("shown to the second client")
    });
    let first_layout_size = wait_for_the_layout_size(&first_terminal);
    let mut viewer = start_viewer();
    let first_id = client_id_of(&status_shows(
        &viewer.zellij,
        "the first client sees the fixture",
        "client=",
    ));
    let second_client = viewer.zellij.attach_client(TERMINAL_SIZE);
    let second_grid = second_client.wait_until(
        "the second client sees the fixture or its permission prompt",
        |grid_snapshot| grid_snapshot.contains("LV ") || grid_snapshot.contains(PERMISSION_PROMPT),
    );
    if second_grid.contains(PERMISSION_PROMPT) {
        second_client.send_stdin(b"y");
    }
    let second_grid =
        second_client.wait_until("the second client sees the fixture", |grid_snapshot| {
            grid_snapshot.contains("client=") && !grid_snapshot.contains(PERMISSION_PROMPT)
        });
    let second_id = client_id_of(&second_grid);
    assert_ne!(first_id, second_id);
    let mut first_args = target_args(source.session_name(), first_terminal.terminal_id());
    first_args.push(("only_client", first_id.clone()));
    send_to_the_fixture(&viewer.zellij, "live_view_set", &first_args);
    let mut second_args = target_args(source.session_name(), second_terminal.terminal_id());
    second_args.push(("only_client", second_id.clone()));
    second_args.push(("cols", "30".to_owned()));
    second_args.push(("rows", "8".to_owned()));
    send_to_the_fixture(&viewer.zellij, "live_view_set", &second_args);
    let first_grid = status_shows(
        &viewer.zellij,
        "the first client sees its own target",
        "shown to the first client",
    );
    assert!(!first_grid.contains("shown to the second client"));
    let second_grid = second_client
        .wait_until("the second client sees its own target", |grid_snapshot| {
            grid_snapshot.contains("shown to the second client")
        });
    assert!(!second_grid.contains("shown to the first client"));
    send_to_the_fixture(
        &viewer.zellij,
        "live_view_input",
        &[
            ("capture", "true".to_owned()),
            ("only_client", first_id.clone()),
        ],
    );
    wait_for_the_pin_at(
        &first_terminal,
        VIEW_SIZE,
        "the first client pins its target",
    );
    status_shows(&viewer.zellij, "the first client captures", "captured=true");
    assert!(!second_client.snapshot().contains("captured=true"));
    let mut same_target = target_args(source.session_name(), first_terminal.terminal_id());
    same_target.push(("only_client", second_id.clone()));
    same_target.push(("cols", "30".to_owned()));
    same_target.push(("rows", "8".to_owned()));
    send_to_the_fixture(&viewer.zellij, "live_view_set", &same_target);
    second_client.wait_until("the second client switched targets", |grid_snapshot| {
        grid_snapshot.contains("shown to the first client")
    });
    send_to_the_fixture(
        &viewer.zellij,
        "live_view_input",
        &[
            ("capture", "true".to_owned()),
            ("only_client", second_id.clone()),
        ],
    );
    wait_for_the_pin_at(
        &first_terminal,
        (30, 8),
        "the most recently capturing client holds the pin",
    );
    send_to_the_fixture(
        &viewer.zellij,
        "live_view_clear",
        &[("only_client", second_id.clone())],
    );
    wait_for_the_pin_at(
        &first_terminal,
        VIEW_SIZE,
        "the pin goes back to the client that still shows the pane",
    );
    status_shows(
        &viewer.zellij,
        "the first client still captures",
        "captured=true",
    );
    send_to_the_fixture(
        &viewer.zellij,
        "live_view_input",
        &[
            ("capture", "false".to_owned()),
            ("only_client", first_id.clone()),
        ],
    );
    status_shows(
        &viewer.zellij,
        "the first client stopped capturing",
        "released=Some(Plugin)",
    );
    wait_for_the_pin_at(
        &first_terminal,
        VIEW_SIZE,
        "a shown view holds its pin after capture ends",
    );
    send_to_the_fixture(
        &viewer.zellij,
        "live_view_clear",
        &[("only_client", first_id)],
    );
    let cursor_before = source_view.snapshot().cursor;
    source_view.send_action(zellij_utils::input::actions::Action::FocusNextPane);
    source_view.wait_until("the focus moved to the other pane", |grid_snapshot| {
        grid_snapshot.cursor.is_some() && grid_snapshot.cursor != cursor_before
    });
    source_view.send_action(zellij_utils::input::actions::Action::FocusNextPane);
    source_view.wait_until("the focus moved back", |grid_snapshot| {
        grid_snapshot.cursor == cursor_before
    });
    wait_for_the_release(
        &first_terminal,
        first_layout_size,
        "focusing the pane in its session gives its size back once nobody shows it",
    );
    viewer.zellij.quit();
}

#[test]
fn a_view_only_live_view_refuses_capture_never_pins_and_cannot_be_toggled() {
    let pinned = show_the_watched_source(&[("view_only", "true".to_owned())]);
    status_shows(
        pinned.zellij(),
        "the view-only live view says so",
        "resize=false view_only=true",
    );
    send_to_the_fixture(
        pinned.zellij(),
        "live_view_input",
        &[("capture", "true".to_owned())],
    );
    let grid_snapshot = status_shows(
        pinned.zellij(),
        "capture is refused",
        "released=Some(NotAllowed)",
    );
    assert!(grid_snapshot.contains("captured=false"));
    assert_not_pinned_after_a_barrier(&pinned.watched, "view-only barrier");
    let exit_status = pinned
        .zellij()
        .run_cli_action(CliAction::ToggleMirrorResize);
    assert_ne!(exit_status, 0, "toggling a view-only live view must fail");
    assert_not_pinned_after_a_barrier(&pinned.watched, "after the toggle");
    let mut viewer = pinned.viewer;
    viewer.zellij.quit();
}

#[test]
fn the_toggle_fails_when_the_focused_pane_has_no_live_view() {
    let mut viewer = start_viewer();
    assert_ne!(
        viewer.zellij.run_cli_action(CliAction::ToggleMirrorResize),
        0,
        "toggling without a live view must fail"
    );
    viewer.zellij.quit();
}

#[test]
fn a_view_only_viewer_has_its_input_dropped_by_the_source() {
    let (source, terminal) = start_source();
    terminal.output(b"$ view only source\r\n");
    let (view_only, view_only_events) = connect_directly(
        source.session_name(),
        terminal.terminal_id(),
        true,
        PANE_MIRROR_VERSION,
    );
    wait_for_event(&view_only_events, "the first frame", is_frame);
    assert!(view_only.send(ViewerMessage::Input(MirrorInput::Bytes(
        b"must not arrive".to_vec()
    ))));
    let (writer, writer_events) = connect_directly(
        source.session_name(),
        terminal.terminal_id(),
        false,
        PANE_MIRROR_VERSION,
    );
    wait_for_event(&writer_events, "the first frame of the writer", is_frame);
    assert!(writer.send(ViewerMessage::Input(MirrorInput::Bytes(
        b"barrier".to_vec()
    ))));
    let stdin = terminal.wait_for_stdin("the writable viewer's input arrived", |stdin| {
        stdin.ends_with(b"barrier")
    });
    assert!(!String::from_utf8_lossy(&stdin).contains("must not arrive"));
    drop(view_only);
    drop(writer);
}

#[test]
fn a_newer_protocol_version_is_refused() {
    let (source, terminal) = start_source();
    let (_transport, events) = connect_directly(
        source.session_name(),
        terminal.terminal_id(),
        false,
        PANE_MIRROR_VERSION + 1,
    );
    wait_for_event(&events, "a version mismatch", |event| {
        matches!(
            event,
            MirrorTransportEvent::Message(SourceMessage::Closed {
                reason: MirrorClosedReason::VersionMismatch,
                ..
            })
        )
    });
}

#[test]
fn a_viewer_with_resizing_off_in_its_config_pins_only_when_toggled_or_typed_into() {
    let watched = start_watched_source("");
    let mut viewer = start_viewer_with_config("mirror_resizes_source_pane false");
    show_pane(
        &viewer.zellij,
        watched.source.session_name(),
        watched.terminal.terminal_id(),
    );
    status_shows(&viewer.zellij, "the live view is live", "watched source");
    status_shows(
        &viewer.zellij,
        "the live view may not resize",
        "resize=false view_only=false pinned=false",
    );
    assert_not_pinned_after_a_barrier(&watched, "first barrier");
    assert_eq!(
        viewer.zellij.run_cli_action(CliAction::ToggleMirrorResize),
        0
    );
    wait_for_the_pin_at(&watched.terminal, VIEW_SIZE, "the toggle turns resizing on");
    status_shows(
        &viewer.zellij,
        "the plugin sees the pin",
        "resize=true view_only=false pinned=true",
    );
    assert_eq!(
        viewer.zellij.run_cli_action(CliAction::ToggleMirrorResize),
        0
    );
    wait_for_the_release(
        &watched.terminal,
        watched.layout_size,
        "the toggle turns resizing off again",
    );
    status_shows(
        &viewer.zellij,
        "the plugin sees resizing turned off",
        "resize=false view_only=false pinned=false",
    );
    assert_not_pinned_after_a_barrier(&watched, "second barrier");
    capture_input(&viewer.zellij);
    wait_for_the_pin_at(
        &watched.terminal,
        VIEW_SIZE,
        "typing into the view resizes the source even with resizing off",
    );
    status_shows(
        &viewer.zellij,
        "the plugin sees the pin while resizing is off",
        "resize=false view_only=false pinned=true",
    );
    viewer.zellij.quit();
}

#[test]
fn a_source_with_resizing_off_in_its_config_refuses_every_pin() {
    let watched = start_watched_source("mirror_resizes_source_pane false");
    let pane = watched.terminal.terminal_id();
    let (_transport, events) = connect_with(
        watched.source.session_name(),
        pane,
        pinning_attach(pane, Size { cols: 40, rows: 10 }),
    );
    let event = wait_for_event(&events, "a pin state", is_any_pin_state);
    assert!(is_pin_state(&event, false, None), "{:?}", event);
    let mut viewer = start_viewer();
    show_pane(&viewer.zellij, watched.source.session_name(), pane);
    status_shows(&viewer.zellij, "the live view is live", "watched source");
    capture_input(&viewer.zellij);
    status_shows(
        &viewer.zellij,
        "the live view may resize but is not pinned",
        "resize=true view_only=false pinned=false",
    );
    assert_not_pinned_after_a_barrier(&watched, "viewer barrier");
    viewer.zellij.quit();
}

#[test]
fn the_toggle_action_releases_and_restores_the_pin() {
    let pinned = show_the_watched_source(&[]);
    wait_for_the_pin_at(
        pinned.terminal(),
        VIEW_SIZE,
        "showing the source pins it at the size of the live view",
    );
    assert_eq!(
        pinned
            .zellij()
            .run_cli_action(CliAction::ToggleMirrorResize),
        0
    );
    wait_for_the_release(
        pinned.terminal(),
        pinned.layout_size(),
        "the action turns resizing off",
    );
    status_shows(
        pinned.zellij(),
        "the plugin sees resizing turned off",
        "resize=false view_only=false pinned=false",
    );
    pinned
        .zellij()
        .run_cli_action(CliAction::ToggleMirrorResize);
    wait_for_the_pin_at(
        pinned.terminal(),
        VIEW_SIZE,
        "the action turns resizing back on",
    );
    let mut viewer = pinned.viewer;
    viewer.zellij.quit();
}

#[test]
fn a_live_view_that_may_not_resize_pins_only_once_typed_into_and_keeps_the_pin_after() {
    let pinned = show_the_watched_source(&[("resize", "false".to_owned())]);
    status_shows(
        pinned.zellij(),
        "the live view may not resize",
        "resize=false view_only=false pinned=false",
    );
    assert_not_pinned_after_a_barrier(&pinned.watched, "resize false barrier");
    capture_input(pinned.zellij());
    wait_for_the_pin_at(
        pinned.terminal(),
        VIEW_SIZE,
        "typing into the view resizes the source",
    );
    status_shows(
        pinned.zellij(),
        "the plugin sees the pin while resizing is off",
        "resize=false view_only=false pinned=true",
    );
    release_input(pinned.zellij());
    assert_keeps_the_size_after_a_barrier(&pinned.watched, VIEW_SIZE, "after typing");
    let mut viewer = pinned.viewer;
    viewer.zellij.quit();
}

#[test]
fn a_live_view_without_a_target_says_so_and_cannot_capture() {
    let mut viewer = start_viewer();
    send_to_the_fixture(&viewer.zellij, "live_view_set", &[]);
    let grid_snapshot = status_shows(&viewer.zellij, "the empty placeholder", NO_TARGET);
    assert_inside_the_view(&grid_snapshot, NO_TARGET);
    send_to_the_fixture(
        &viewer.zellij,
        "live_view_input",
        &[("capture", "true".to_owned())],
    );
    status_shows(
        &viewer.zellij,
        "capture is refused without a target",
        "released=Some(NotAllowed)",
    );
    viewer.zellij.quit();
}

#[test]
fn a_live_view_waiting_for_a_silent_session_shows_that_it_is_connecting_then_gives_up() {
    let mut viewer = start_viewer();
    let silent_session = format!("{}-silent", viewer.zellij.session_name());
    let socket_path = zellij_utils::consts::session_socket_path(&silent_session);
    let listener = std::os::unix::net::UnixListener::bind(&socket_path).unwrap();
    let (held_sender, held_receiver) = unbounded();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            if held_sender.send(stream).is_err() {
                return;
            }
        }
    });
    show_pane(&viewer.zellij, &silent_session, 1);
    status_shows(
        &viewer.zellij,
        "the connecting placeholder",
        "Connecting...",
    );
    let _held = held_receiver.recv_timeout(Duration::from_secs(5));
    status_shows(
        &viewer.zellij,
        "the connecting time limit closes the live view",
        "Preview not available",
    );
    viewer.zellij.quit();
    let _ = std::fs::remove_file(&socket_path);
}

#[test]
fn a_live_view_of_a_missing_pane_says_it_was_not_found() {
    let (source, _terminal) = start_source();
    let mut viewer = start_viewer();
    show_pane(&viewer.zellij, source.session_name(), 999);
    status_shows(
        &viewer.zellij,
        "the not found placeholder",
        "The pane was not found",
    );
    viewer.zellij.quit();
}

#[test]
fn a_live_view_shows_when_its_pane_closes() {
    let mut viewer = start_viewer();
    let session = viewer.zellij.session_name().to_owned();
    viewer.second_terminal.output(b"$ about to close\r\n");
    show_pane(
        &viewer.zellij,
        &session,
        viewer.second_terminal.terminal_id(),
    );
    viewer
        .zellij
        .wait_until("the live view shows the pane", |grid_snapshot| {
            occurrences(grid_snapshot, "about to close") >= 2
        });
    viewer.second_terminal.exit(Some(0));
    status_shows(
        &viewer.zellij,
        "the pane closed placeholder",
        "The pane was closed",
    );
    viewer.zellij.quit();
}

#[test]
fn closing_the_tab_of_the_source_pane_closes_the_live_view() {
    let source = TestRunner::new(TERMINAL_SIZE)
        .with_layout(LayoutInfo::Stringified(
            "layout {\n    tab {\n        pane\n    }\n    tab {\n        pane\n    }\n}\n"
                .to_owned(),
        ))
        .skip_concurrency_slot()
        .start_in_background();
    let first_tab_terminal = source.expect_pty_spawn();
    let second_tab_terminal = source.expect_pty_spawn();
    first_tab_terminal.disable_echo();
    second_tab_terminal.disable_echo();
    second_tab_terminal.output(b"$ in the second tab\r\n");
    let mut viewer = start_viewer();
    show_pane(
        &viewer.zellij,
        source.session_name(),
        second_tab_terminal.terminal_id(),
    );
    status_shows(&viewer.zellij, "the live view is live", "in the second tab");
    let mut source = source.attach(TERMINAL_SIZE);
    source.send_stdin(&keys::ctrl('t'));
    source.send_stdin(&keys::key('2'));
    source.send_stdin(&keys::ENTER);
    source.wait_until("the second tab is shown", |grid_snapshot| {
        grid_snapshot.contains("in the second tab")
    });
    source.send_stdin(&keys::ctrl('t'));
    source.send_stdin(&keys::key('x'));
    status_shows(
        &viewer.zellij,
        "closing the tab closes the live view",
        "The pane was closed",
    );
    viewer.zellij.quit();
    source.quit();
}

#[test]
fn a_live_view_shows_when_its_session_ends() {
    let (source, terminal) = start_source();
    terminal.output(b"$ session about to end\r\n");
    let mut viewer = start_viewer();
    show_pane(
        &viewer.zellij,
        source.session_name(),
        terminal.terminal_id(),
    );
    status_shows(
        &viewer.zellij,
        "the live view shows the source pane",
        "session about to end",
    );
    zellij_utils::session_link::stop_session(source.session_name(), Duration::from_secs(2))
        .unwrap();
    status_shows(
        &viewer.zellij,
        "the session ended placeholder",
        "The session ended",
    );
    viewer.zellij.quit();
}

#[test]
fn a_live_view_of_a_plugin_pane_is_refused() {
    let mut viewer = start_viewer();
    let session = viewer.zellij.session_name().to_owned();
    show_pane_with(
        &viewer.zellij,
        &session,
        0,
        &[("plugin", "true".to_owned())],
    );
    status_shows(
        &viewer.zellij,
        "the refused placeholder",
        "The session refused to mirror this pane",
    );
    status_shows(&viewer.zellij, "the plugin hears it", "Closed(Rejected)");
    viewer.zellij.quit();
}

#[test]
fn a_live_view_of_another_host_is_unsupported() {
    let mut viewer = start_viewer();
    let mut args = target_args("anywhere", 1);
    args.push(("host", "laptop".to_owned()));
    send_to_the_fixture(&viewer.zellij, "live_view_set", &args);
    status_shows(
        &viewer.zellij,
        "the unsupported host placeholder",
        "Unsupported host: laptop",
    );
    viewer.zellij.quit();
}

#[test]
fn a_live_view_inside_a_floating_plugin_pane_shows_the_source_and_takes_keys() {
    let (source, terminal) = start_source();
    terminal.output(b"$ shown in a floating pane\r\n");
    let mut viewer = start_viewer_with(&floating_viewer_layout(), "");
    show_pane(
        &viewer.zellij,
        source.session_name(),
        terminal.terminal_id(),
    );
    let grid_snapshot = status_shows(
        &viewer.zellij,
        "the floating live view is live",
        "shown in a floating pane",
    );
    assert_inside_the_view(&grid_snapshot, "shown in a floating pane");
    capture_input(&viewer.zellij);
    viewer.zellij.send_stdin(b"floating keys");
    terminal.wait_for_stdin("keys reached the source", |stdin| {
        stdin.ends_with(b"floating keys")
    });
    viewer.zellij.quit();
}

#[test]
fn a_live_view_inside_a_popup_plugin_pane_shows_the_source_and_takes_keys() {
    let (source, terminal) = start_source();
    terminal.output(b"$ in a popup\r\n");
    let mut zellij = TestRunner::new(TERMINAL_SIZE)
        .with_config("mouse_mode true")
        .start();
    let local_terminal = zellij.expect_pty_spawn();
    local_terminal.disable_echo();
    local_terminal.output(b"$ ");
    zellij.wait_until("the session is up", |grid_snapshot| {
        grid_snapshot.status_bar_appears()
    });
    let mut args = target_args(source.session_name(), terminal.terminal_id());
    args.push(("x", "1".to_owned()));
    args.push(("y", "3".to_owned()));
    args.push(("cols", "18".to_owned()));
    args.push(("rows", "3".to_owned()));
    let opened = zellij.run_cli_pipe(
        CliAction::Pipe {
            name: Some("live_view_set".to_owned()),
            payload: None,
            args: Some(
                args.iter()
                    .map(|(key, value)| format!("{}={}", key, value))
                    .collect::<Vec<_>>()
                    .join(",")
                    .parse()
                    .unwrap(),
            ),
            plugin: Some(fixture_plugin_url()),
            plugin_configuration: Some(FIXTURE_CONFIGURATION.parse().unwrap()),
            force_launch_plugin: true,
            skip_plugin_cache: false,
            floating_plugin: None,
            in_place_plugin: None,
            plugin_cwd: None,
            plugin_title: None,
            popup: Some(PipePopupPlacement::Center),
            popup_no_focus: false,
        },
        CliCaller::from_pane(local_terminal.terminal_id()),
    );
    let grid_snapshot = zellij.wait_until(
        "the popup shows the fixture or its permission prompt",
        |grid_snapshot| grid_snapshot.contains("in a popup") || grid_snapshot.contains("(y/n)"),
    );
    if grid_snapshot.contains("(y/n)") {
        zellij.send_stdin(b"y");
    }
    opened.wait_for_exit();
    let grid_snapshot = status_shows(&zellij, "the popup live view is live", "in a popup");
    assert_inside_the_view(&grid_snapshot, "in a popup");
    zellij.send_stdin(b"C");
    status_shows(
        &zellij,
        "a key in the popup starts capture",
        "captured=true",
    );
    zellij.send_stdin(b"popup keys");
    terminal.wait_for_stdin("keys reached the source", |stdin| {
        stdin.ends_with(b"popup keys")
    });
    zellij.quit();
}

#[test]
fn shrinking_the_plugin_pane_clamps_the_view_and_a_new_rectangle_does_not_reconnect() {
    let watched = start_watched_source("");
    let long_line = "X".repeat(100);
    watched
        .terminal
        .output(format!("{}\r\n", long_line).as_bytes());
    let mut viewer = start_viewer();
    show_pane(
        &viewer.zellij,
        watched.source.session_name(),
        watched.terminal.terminal_id(),
    );
    status_shows(&viewer.zellij, "the live view is live", "connects=1");
    viewer
        .zellij
        .wait_until("the live view shows the long line", |grid_snapshot| {
            longest_run_of(grid_snapshot, 'X') == VIEW_SIZE.0 as usize
        });
    viewer.zellij.resize(Size { cols: 60, rows: 24 });
    let grid_snapshot = viewer.zellij.wait_until(
        "the live view is clamped to the narrower pane",
        |grid_snapshot| {
            let run = longest_run_of(grid_snapshot, 'X');
            run > 0 && run < VIEW_SIZE.0 as usize
        },
    );
    let clamped_cols = longest_run_of(&grid_snapshot, 'X');
    capture_input(&viewer.zellij);
    wait_for_the_pin_at(
        &watched.terminal,
        (clamped_cols as u16, VIEW_SIZE.1),
        "the source is pinned at the clamped size",
    );
    viewer.zellij.resize(TERMINAL_SIZE);
    show_pane_with(
        &viewer.zellij,
        watched.source.session_name(),
        watched.terminal.terminal_id(),
        &[
            ("x", "1".to_owned()),
            ("y", "4".to_owned()),
            ("cols", "20".to_owned()),
            ("rows", "5".to_owned()),
        ],
    );
    wait_for_the_pin_at(
        &watched.terminal,
        (20, 5),
        "the new rectangle resizes the pin",
    );
    let grid_snapshot =
        viewer
            .zellij
            .wait_until("the fixture draws the new rectangle", |grid_snapshot| {
                border_of(grid_snapshot).map(|area| (area.cols, area.rows)) == Some((20, 5))
            });
    assert!(
        grid_snapshot.contains("captured=true"),
        "{}",
        grid_snapshot.text
    );
    assert!(
        grid_snapshot.contains("connects=1"),
        "{}",
        grid_snapshot.text
    );
    viewer.zellij.quit();
}

#[test]
fn a_watcher_sees_no_live_view_and_does_not_crash() {
    let (source, terminal) = start_source();
    terminal.output(b"$ not for watchers\r\n");
    let mut viewer = start_viewer();
    let watcher = viewer.zellij.attach_watcher(TERMINAL_SIZE);
    show_pane(
        &viewer.zellij,
        source.session_name(),
        terminal.terminal_id(),
    );
    status_shows(&viewer.zellij, "the live view is live", "not for watchers");
    capture_input(&viewer.zellij);
    terminal.output(b"$ later output\r\n");
    let viewer_grid = viewer.zellij.wait_until(
        "the capturing client shows the source text",
        |grid_snapshot| grid_snapshot.contains("later output") && grid_snapshot.cursor.is_some(),
    );
    let view_area = border_of(&viewer_grid).expect("the fixture draws its border");
    let viewer_cursor = viewer_grid.cursor.expect("the live view cursor is shown");
    assert!(
        view_area.contains(viewer_cursor.x, viewer_cursor.y),
        "the capturing client's cursor {:?} is outside the view {:?}",
        viewer_cursor,
        view_area
    );
    let watcher_grid = watcher.wait_until(
        "the watcher renders the plugin after capture started",
        |grid_snapshot| grid_snapshot.contains("captured=true"),
    );
    assert!(
        !watcher_grid.contains("not for watchers") && !watcher_grid.contains("later output"),
        "the watcher sees the live view:\n{}",
        watcher_grid.text
    );
    if let Some(watcher_cursor) = watcher_grid.cursor {
        assert!(
            !view_area.contains(watcher_cursor.x, watcher_cursor.y),
            "the watcher got the live view cursor {:?}",
            watcher_cursor
        );
    }
    release_input(&viewer.zellij);
    viewer.zellij.send_stdin(b"w");
    status_shows(&viewer.zellij, "the viewer still works", "input key=w");
    viewer.zellij.quit();
}

#[test]
fn a_live_view_that_may_not_resize_leaves_the_source_client_count_and_tab_size_alone() {
    let watched = start_watched_source("");
    let clients_before = watched.source.connected_client_count();
    let mut viewer = start_viewer();
    show_pane_with(
        &viewer.zellij,
        watched.source.session_name(),
        watched.terminal.terminal_id(),
        &[("resize", "false".to_owned())],
    );
    status_shows(&viewer.zellij, "the live view is live", "watched source");
    watched.terminal.output(b"$ still the same size\r\n");
    status_shows(
        &viewer.zellij,
        "the live view follows the source",
        "still the same size",
    );
    assert_not_pinned_after_a_barrier(&watched, "size barrier");
    assert_eq!(watched.source.connected_client_count(), clients_before);
    viewer.zellij.quit();
}

#[test]
fn a_resizing_live_view_changes_only_the_program_size_and_leaves_the_source_client_count_and_tab_size_alone(
) {
    let watched = start_watched_source("");
    let other_pane = watched.source.expect_pty_spawn();
    let other_size = other_pane.wait_for_size(
        "the other source pane has the size of the attached source view",
        |cols, rows| {
            cols as usize + 10 > TERMINAL_SIZE.cols && (rows as usize) < TERMINAL_SIZE.rows
        },
    );
    let other_sizes_before = other_pane.size_history().len();
    let clients_before = watched.source.connected_client_count();
    let mut viewer = start_viewer();
    show_pane(
        &viewer.zellij,
        watched.source.session_name(),
        watched.terminal.terminal_id(),
    );
    status_shows(&viewer.zellij, "the live view is live", "watched source");
    wait_for_the_pin_at(
        &watched.terminal,
        VIEW_SIZE,
        "the resizing view gives the program the size of the view",
    );
    status_shows(&viewer.zellij, "the live view is pinned", "pinned=true");
    let marker = pinned_marker(VIEW_SIZE);
    other_pane.output(b"$ other pane barrier\r\n");
    watched.view.wait_until(
        "the source view shows the pin and the barrier",
        |grid_snapshot| {
            grid_snapshot.contains(&marker) && grid_snapshot.contains("other pane barrier")
        },
    );
    assert_eq!(
        other_pane.size(),
        Some(other_size),
        "the other pane of the source tab changed size"
    );
    assert_eq!(
        other_pane.size_history().len(),
        other_sizes_before,
        "the other pane of the source tab was resized: {:?}",
        other_pane.size_history()
    );
    assert_eq!(watched.source.connected_client_count(), clients_before);
    assert_eq!(watched.terminal.size(), Some(VIEW_SIZE));
    viewer.zellij.quit();
}

fn show_a_fresh_source(viewer: &Viewer, text: &str) -> (BackgroundTestSession, FakePtyHandle) {
    let (source, terminal) = start_source();
    terminal.output(format!("$ {}\r\n", text).as_bytes());
    show_pane(
        &viewer.zellij,
        source.session_name(),
        terminal.terminal_id(),
    );
    status_shows(&viewer.zellij, "the live view is live", text);
    (source, terminal)
}

fn type_and_wait_for_the_source(zellij: &TestSession, terminal: &FakePtyHandle, text: &str) {
    zellij.send_stdin(text.as_bytes());
    terminal.wait_for_stdin("the typed text reached the source", |stdin| {
        stdin.ends_with(text.as_bytes())
    });
}

#[test]
fn rename_and_search_prompts_opened_while_capturing_take_the_typed_keys() {
    let mut viewer = start_viewer_with_config(
        "keybinds {\n    shared_among \"renametab\" \"renamepane\" \"entersearch\" {\n        bind \"Alt y\" { Write 121; }\n    }\n}",
    );
    let (_source, terminal) = show_a_fresh_source(&viewer, "prompt source");
    capture_input(&viewer.zellij);
    type_and_wait_for_the_source(&viewer.zellij, &terminal, "first");
    viewer.zellij.send_stdin(&keys::ctrl('t'));
    viewer.zellij.send_stdin(&keys::key('r'));
    viewer.zellij.send_stdin(b"QZTAB");
    viewer.zellij.send_stdin(b"\x1by");
    viewer
        .zellij
        .wait_until("the tab is renamed", |grid_snapshot| {
            grid_snapshot.contains("QZTABy")
        });
    viewer.zellij.send_stdin(&keys::ENTER);
    type_and_wait_for_the_source(&viewer.zellij, &terminal, "second");
    viewer.zellij.send_stdin(&keys::ctrl('p'));
    viewer.zellij.send_stdin(&keys::key('c'));
    viewer.zellij.send_stdin(b"QZPANE");
    viewer.zellij.send_stdin(b"\x1by");
    viewer
        .zellij
        .wait_until("the plugin pane is renamed", |grid_snapshot| {
            grid_snapshot.contains("QZPANEy")
        });
    viewer.zellij.send_stdin(&keys::ENTER);
    type_and_wait_for_the_source(&viewer.zellij, &terminal, "third");
    viewer.zellij.send_stdin(&keys::ctrl('s'));
    viewer.zellij.send_stdin(&keys::key('s'));
    viewer.zellij.send_stdin(b"QZFIND");
    viewer.zellij.send_stdin(b"\x1by");
    viewer.zellij.send_stdin(&keys::ENTER);
    viewer.zellij.send_stdin(&keys::ctrl('s'));
    viewer
        .zellij
        .wait_until("the search prompt is closed", |grid_snapshot| {
            grid_snapshot.status_bar_appears()
        });
    let stdin = {
        type_and_wait_for_the_source(&viewer.zellij, &terminal, "fourth");
        terminal.stdin_bytes()
    };
    let stdin = String::from_utf8_lossy(&stdin).into_owned();
    assert!(
        !stdin.contains("QZ") && !stdin.contains('y'),
        "a key typed into a prompt reached the source: {:?}",
        stdin
    );
    assert!(stdin.ends_with("firstsecondthirdfourth"), "{:?}", stdin);
    status_shows(&viewer.zellij, "capture is still on", "captured=true");
    viewer.zellij.quit();
}

#[test]
fn a_binding_that_moves_the_focus_and_then_writes_does_not_reach_the_source() {
    let mut viewer = start_viewer_with_config(
        "keybinds {\n    normal {\n        bind \"Alt w\" { MoveFocus \"Right\"; Write 120; }\n    }\n}",
    );
    let (_source, terminal) = show_a_fresh_source(&viewer, "focus change source");
    capture_input(&viewer.zellij);
    type_and_wait_for_the_source(&viewer.zellij, &terminal, "before");
    viewer.zellij.send_stdin(b"\x1bw");
    zellij_integration_tests::wait_for("the written key reaches the newly focused pane", || {
        viewer.first_terminal.stdin_bytes().ends_with(b"x")
            || viewer.second_terminal.stdin_bytes().ends_with(b"x")
    });
    status_shows(
        &viewer.zellij,
        "the focus change ends capture",
        "released=Some(FocusLost)",
    );
    let stdin = terminal.stdin_bytes();
    assert!(
        stdin.ends_with(b"before"),
        "the written key reached the source: {:?}",
        String::from_utf8_lossy(&stdin)
    );
    viewer.zellij.quit();
}

#[test]
fn a_drag_that_starts_inside_the_rectangle_stays_with_the_source_and_is_clamped() {
    let mut viewer = start_viewer();
    let (source, terminal) = start_source();
    terminal.output(b"\x1b[?1002h\x1b[?1006h$ drag source\r\n");
    show_pane(
        &viewer.zellij,
        source.session_name(),
        terminal.terminal_id(),
    );
    status_shows(&viewer.zellij, "the live view is live", "drag source");
    let grid_snapshot = capture_input(&viewer.zellij);
    let area = border_of(&grid_snapshot).expect("the fixture draws its border");
    let press_column = area.x + 2;
    let press_row = area.y;
    let far_column = area.x + area.cols + 4;
    let far_row = area.y + area.rows + 2;
    viewer
        .zellij
        .send_stdin(format!("\u{1b}[<0;{};{}M", press_column + 1, press_row + 1).as_bytes());
    terminal.wait_for_stdin("the press reached the source", |stdin| {
        String::from_utf8_lossy(stdin).contains("\x1b[<0;3;1M")
    });
    viewer
        .zellij
        .send_stdin(format!("\u{1b}[<32;{};{}M", far_column + 1, far_row + 1).as_bytes());
    viewer
        .zellij
        .send_stdin(format!("\u{1b}[<0;{};{}m", far_column + 1, far_row + 1).as_bytes());
    let clamped = format!("{};{}", area.cols, area.rows);
    let stdin = terminal.wait_for_stdin("the clamped release reached the source", |stdin| {
        String::from_utf8_lossy(stdin).contains(&format!("\x1b[<0;{}m", clamped))
    });
    assert!(
        String::from_utf8_lossy(&stdin).contains(&format!("\x1b[<32;{}M", clamped)),
        "the motion outside the rectangle was not clamped: {:?}",
        String::from_utf8_lossy(&stdin)
    );
    let grid_snapshot = status_shows(&viewer.zellij, "the drag left capture on", "captured=true");
    assert!(!grid_snapshot.contains("ClickOutside"));
    viewer
        .zellij
        .send_stdin(&keys::sgr_click(far_column, far_row));
    status_shows(
        &viewer.zellij,
        "a press outside the rectangle ends capture",
        "released=Some(ClickOutside)",
    );
    viewer.zellij.quit();
}

#[test]
fn a_drag_cut_short_by_the_release_key_releases_the_button_in_the_source() {
    let mut viewer = start_viewer();
    let (source, terminal) = start_source();
    terminal.output(b"\x1b[?1002h\x1b[?1006h$ cut drag source\r\n");
    show_pane(
        &viewer.zellij,
        source.session_name(),
        terminal.terminal_id(),
    );
    status_shows(&viewer.zellij, "the live view is live", "cut drag source");
    let grid_snapshot = capture_input(&viewer.zellij);
    let area = border_of(&grid_snapshot).expect("the fixture draws its border");
    let press_column = area.x + 2;
    let press_row = area.y;
    viewer
        .zellij
        .send_stdin(format!("\u{1b}[<0;{};{}M", press_column + 1, press_row + 1).as_bytes());
    viewer
        .zellij
        .send_stdin(format!("\u{1b}[<32;{};{}M", press_column + 3, press_row + 2).as_bytes());
    terminal.wait_for_stdin("the drag reached the source", |stdin| {
        String::from_utf8_lossy(stdin).contains("\x1b[<32;5;2M")
    });
    press_release_key(&viewer.zellij);
    status_shows(
        &viewer.zellij,
        "the release key ends capture",
        "released=Some(Key)",
    );
    terminal.wait_for_stdin("the source gets the button released", |stdin| {
        String::from_utf8_lossy(stdin).contains("\x1b[<0;5;2m")
    });
    viewer.zellij.quit();
}

fn pinned_cover_layout(main_pane: &str) -> String {
    format!(
        r#"
        layout {{
            tab hide_floating_panes=true {{
                pane size=1 borderless=true {{
                    plugin location="tab-bar"
                }}
                pane split_direction="vertical" {{
                    pane size="60%" focus=true {{
                        {}
                    }}
                    pane
                }}
                pane size=2 borderless=true {{
                    plugin location="status-bar"
                }}
                floating_panes {{
                    pane x=30 y=6 width=60 height=12 pinned=true
                }}
            }}
        }}
        "#,
        main_pane
    )
}

#[test]
fn a_click_on_a_pinned_pane_over_the_rectangle_is_not_sent_to_the_source() {
    let zellij = TestRunner::new(TERMINAL_SIZE)
        .with_config("mouse_mode true")
        .with_layout(LayoutInfo::Stringified(pinned_cover_layout(
            &fixture_plugin_node(),
        )))
        .start();
    let first_terminal = zellij.expect_pty_spawn();
    let second_terminal = zellij.expect_pty_spawn();
    first_terminal.disable_echo();
    second_terminal.disable_echo();
    first_terminal.output(b"$ COVERONE\r\n");
    second_terminal.output(b"$ COVERTWO\r\n");
    grant_the_fixture_permissions(&zellij);
    let mut viewer = Viewer {
        zellij,
        first_terminal,
        second_terminal,
    };
    let (source, terminal) = start_source();
    terminal.output(b"\x1b[?1000h\x1b[?1006h$ covered source\r\n");
    show_pane(
        &viewer.zellij,
        source.session_name(),
        terminal.terminal_id(),
    );
    status_shows(&viewer.zellij, "the live view is live", "covered source");
    let grid_snapshot = capture_input(&viewer.zellij);
    let area = border_of(&grid_snapshot).expect("the fixture draws its border");
    let (cover_column, cover_row) = ["COVERONE", "COVERTWO"]
        .iter()
        .filter_map(|text| grid_snapshot.position_of(text))
        .find(|(column, row)| area.contains(*column, *row))
        .unwrap_or_else(|| {
            panic!(
                "no pinned pane covers the rectangle:\n{}",
                grid_snapshot.text
            )
        });
    type_and_wait_for_the_source(&viewer.zellij, &terminal, "barrier");
    viewer
        .zellij
        .send_stdin(&keys::sgr_click(cover_column + 2, cover_row));
    let grid_snapshot = status_shows(
        &viewer.zellij,
        "the click focuses the pinned pane",
        "released=Some(FocusLost)",
    );
    assert!(!grid_snapshot.contains("ClickOutside"));
    let stdin = terminal.stdin_bytes();
    assert!(
        stdin.ends_with(b"barrier"),
        "the click on the pinned pane reached the source: {:?}",
        String::from_utf8_lossy(&stdin)
    );
    viewer.zellij.quit();
}

#[test]
fn without_capture_paste_a_click_and_the_wheel_inside_the_rectangle_reach_the_plugin() {
    let mut viewer = start_viewer();
    let (_source, terminal) = show_a_fresh_source(&viewer, "uncaptured source");
    terminal.output(b"\x1b[?1000h\x1b[?1006h");
    let grid_snapshot = status_shows(&viewer.zellij, "the view is live", "uncaptured source");
    let (column, row) = assert_inside_the_view(&grid_snapshot, "uncaptured source");
    let mut paste = keys::BRACKETED_PASTE_START.to_vec();
    paste.extend_from_slice(b"pasted into the plugin");
    paste.extend_from_slice(&keys::BRACKETED_PASTE_END);
    viewer.zellij.send_stdin(&paste);
    status_shows(
        &viewer.zellij,
        "the paste reaches the plugin",
        "input paste=pasted into the plugin",
    );
    viewer.zellij.send_stdin(&keys::sgr_click(column + 2, row));
    status_shows(
        &viewer.zellij,
        "the click reaches the plugin",
        "input mouse=",
    );
    viewer
        .zellij
        .send_stdin(format!("\u{1b}[<64;{};{}M", column + 3, row + 1).as_bytes());
    status_shows(
        &viewer.zellij,
        "the wheel reaches the plugin",
        "input mouse=ScrollUp",
    );
    let stdin = terminal.stdin_bytes();
    assert!(
        stdin.is_empty(),
        "input reached the source without capture: {:?}",
        String::from_utf8_lossy(&stdin)
    );
    viewer.zellij.quit();
}

#[test]
fn a_popup_starts_capture_through_the_api_shows_the_cursor_over_a_pinned_pane_and_releases_on_a_click_outside(
) {
    let (source, terminal) = start_source();
    terminal.output(b"$ in a popup\r\n");
    let zellij = TestRunner::new(TERMINAL_SIZE)
        .with_config("mouse_mode true")
        .with_layout(LayoutInfo::Stringified(pinned_cover_layout("pane")))
        .start();
    let local_terminal = zellij.expect_pty_spawn();
    let other_terminal = zellij.expect_pty_spawn();
    let pinned_terminal = zellij.expect_pty_spawn();
    for pane in [&local_terminal, &other_terminal, &pinned_terminal] {
        pane.disable_echo();
    }
    pinned_terminal.output(b"$ pinned under the popup\r\n");
    zellij.wait_until("the session is up", |grid_snapshot| {
        grid_snapshot.status_bar_appears() && grid_snapshot.contains("pinned under the popup")
    });
    let mut args = target_args(source.session_name(), terminal.terminal_id());
    args.push(("x", "1".to_owned()));
    args.push(("y", "3".to_owned()));
    args.push(("cols", "18".to_owned()));
    args.push(("rows", "3".to_owned()));
    args.push(("capture", "true".to_owned()));
    let joined = args
        .iter()
        .map(|(key, value)| format!("{}={}", key, value))
        .collect::<Vec<_>>()
        .join(",");
    let opened = zellij.run_cli_pipe(
        CliAction::Pipe {
            name: Some("live_view_set".to_owned()),
            payload: None,
            args: Some(joined.parse().unwrap()),
            plugin: Some(fixture_plugin_url()),
            plugin_configuration: Some(FIXTURE_CONFIGURATION.parse().unwrap()),
            force_launch_plugin: true,
            skip_plugin_cache: false,
            floating_plugin: None,
            in_place_plugin: None,
            plugin_cwd: None,
            plugin_title: None,
            popup: Some(PipePopupPlacement::Center),
            popup_no_focus: false,
        },
        CliCaller::from_pane(local_terminal.terminal_id()),
    );
    let grid_snapshot = zellij.wait_until(
        "the popup shows the fixture or its permission prompt",
        |grid_snapshot| grid_snapshot.contains("in a popup") || grid_snapshot.contains("(y/n)"),
    );
    if grid_snapshot.contains("(y/n)") {
        zellij.send_stdin(b"y");
    }
    opened.wait_for_exit();
    let grid_snapshot = status_shows(&zellij, "the popup live view is live", "in a popup");
    let area = border_of(&grid_snapshot).expect("the fixture draws its border");
    status_shows(
        &zellij,
        "the popup plugin starts capture through the API",
        "captured=true",
    );
    let grid_snapshot = zellij.wait_until(
        "the source cursor is shown inside the popup's view",
        |grid_snapshot| {
            grid_snapshot
                .cursor
                .map(|cursor| area.contains(cursor.x, cursor.y))
                .unwrap_or(false)
        },
    );
    let cursor = grid_snapshot.cursor.unwrap();
    assert_eq!((cursor.x, cursor.y), (area.x, area.y + 1));
    zellij.send_stdin(b"popup keys");
    terminal.wait_for_stdin("keys reached the source", |stdin| {
        stdin.ends_with(b"popup keys")
    });
    let (column, row) = grid_snapshot
        .position_of("LV ")
        .expect("the popup shows the fixture status");
    zellij.send_stdin(&keys::sgr_click(column + 1, row));
    status_shows(
        &zellij,
        "a click outside the popup's view ends capture",
        "released=Some(ClickOutside)",
    );
    status_shows(
        &zellij,
        "the click reached the popup plugin",
        "input mouse=",
    );
    let mut zellij = zellij;
    zellij.quit();
}
