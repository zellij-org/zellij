#![cfg(unix)]

use std::collections::BTreeMap;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use zellij_client::{first_message, ClientInfo};
use zellij_integration_tests::{
    claim_first_terminal_and_wait_for_prompt, keys, Size, TestRunner, TestSession, PROMPT,
    TERMINAL_SIZE,
};
use zellij_utils::cli::CliArgs;
use zellij_utils::consts::{ipc_connect, ZELLIJ_SOCK_DIR};
use zellij_utils::data::{BareKey, KeyWithModifier};
use zellij_utils::input::actions::Action;
use zellij_utils::input::mouse::MouseEvent;
use zellij_utils::input::options::Options;
use zellij_utils::ipc::{ClientToServerMsg, IpcSenderWithContext, ServerToClientMsg};
use zellij_utils::position::Position;
use zellij_utils::structured_render::{
    self, GeometryRecord, PaneRect, PointerKind, WireCell, WireColor,
};

const DEADLINE: Duration = Duration::from_secs(20);
const COMMON_MENU_MARKER: &str = "Session manager";

struct WindowClient {
    sender: IpcSenderWithContext<ClientToServerMsg>,
    messages: mpsc::Receiver<ServerToClientMsg>,
    size: Size,
    cells: Vec<WireCell>,
    geometry: GeometryRecord,
}

impl WindowClient {
    fn attach(zellij: &TestSession, size: Size) -> Self {
        let path = ZELLIJ_SOCK_DIR.join(zellij.session_name());
        let stream = ipc_connect(&path)
            .unwrap_or_else(|e| panic!("failed to connect to {:?}: {:?}", path, e));
        let mut sender = IpcSenderWithContext::new(stream);
        let mut receiver = sender.get_receiver::<ServerToClientMsg>();
        let handshake = first_message(
            &ClientInfo::Attach(zellij.session_name().to_owned(), Options::default()),
            &CliArgs::default(),
            &Options::default(),
            size,
            BTreeMap::new(),
            None,
            None,
        );
        sender
            .send_client_msg(handshake.message)
            .expect("failed to send the attach message");
        sender
            .send_client_msg(ClientToServerMsg::StructuredRenderSupport { supported: true })
            .expect("failed to declare structured rendering");

        let (messages_tx, messages) = mpsc::channel();
        std::thread::Builder::new()
            .name("window_layout_reader".to_owned())
            .spawn(move || {
                while let Some((msg, _)) = receiver.recv_server_msg() {
                    if messages_tx.send(msg).is_err() {
                        break;
                    }
                }
            })
            .unwrap();

        WindowClient {
            sender,
            messages,
            size,
            cells: vec![WireCell::BLANK; size.rows * size.cols],
            geometry: GeometryRecord::default(),
        }
    }

    fn send(&mut self, msg: ClientToServerMsg) {
        self.sender
            .send_client_msg(msg)
            .expect("failed to send a message to the session");
    }

    fn act(&mut self, action: Action) {
        self.send(ClientToServerMsg::Action {
            action,
            terminal_id: None,
            client_id: None,
            is_cli_client: false,
        });
    }

    fn mouse(&mut self, event: MouseEvent) {
        self.act(Action::MouseEvent { event });
    }

    fn key(&mut self, key: BareKey, raw_bytes: &[u8]) {
        self.send(ClientToServerMsg::Key {
            key: KeyWithModifier::new(key),
            raw_bytes: raw_bytes.to_vec(),
            is_kitty_keyboard_protocol: false,
        });
    }

    fn apply(&mut self, frame: &[u8]) {
        let view = structured_render::decode(frame).expect("a frame must decode");
        view.apply(&mut self.cells, self.size.cols, self.size.rows);
        if let Some(geometry) = view.geometry() {
            self.geometry = geometry;
        }
        let seq = view.header().seq;
        self.send(ClientToServerMsg::RenderFrameAck { seq });
    }

    fn text(&self) -> String {
        self.cells
            .chunks(self.size.cols)
            .map(|row| row.iter().map(|cell| cell.character()).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn cell(&self, x: usize, y: usize) -> WireCell {
        self.cells[y * self.size.cols + x]
    }

    fn until(&mut self, what: &str, done: impl Fn(&WindowClient) -> bool) {
        if done(self) {
            return;
        }
        let deadline = Instant::now() + DEADLINE;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            let message = self.messages.recv_timeout(remaining).unwrap_or_else(|e| {
                panic!(
                    "waiting for {}: {:?}\n{}\n{:?}",
                    what,
                    e,
                    self.text(),
                    self.geometry
                )
            });
            match message {
                ServerToClientMsg::RenderFrame { frame } => {
                    self.apply(&frame);
                    if done(self) {
                        return;
                    }
                },
                ServerToClientMsg::Exit { exit_reason } => {
                    panic!(
                        "the session let the window go while waiting for {}: {:?}",
                        what, exit_reason
                    )
                },
                _ => {},
            }
        }
    }

    fn popup(&self) -> Option<PaneRect> {
        self.geometry
            .panes
            .iter()
            .rev()
            .find(|pane| pane.popup())
            .copied()
    }

    fn floating(&self) -> Option<PaneRect> {
        self.geometry
            .panes
            .iter()
            .rev()
            .find(|pane| pane.floating())
            .copied()
    }

    fn leave(mut self) {
        self.act(Action::Detach);
        let deadline = Instant::now() + DEADLINE;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            match self.messages.recv_timeout(remaining) {
                Ok(ServerToClientMsg::RenderFrame { frame }) => self.apply(&frame),
                Ok(ServerToClientMsg::Exit { .. }) | Err(_) => return,
                Ok(_) => {},
            }
        }
    }
}

fn at(x: u16, y: u16) -> Position {
    Position::new(y as i32, x)
}

fn left_click(window: &mut WindowClient, x: u16, y: u16) {
    window.mouse(MouseEvent::new_left_press_event(at(x, y)));
    window.mouse(MouseEvent::new_left_release_event(at(x, y)));
}

fn left_drag(window: &mut WindowClient, from: (u16, u16), to: (u16, u16), ctrl: bool) {
    let mut press = MouseEvent::new_left_press_event(at(from.0, from.1));
    press.ctrl = ctrl;
    window.mouse(press);
    window.mouse(MouseEvent::new_left_motion_event(at(to.0, to.1)));
    window.mouse(MouseEvent::new_left_release_event(at(to.0, to.1)));
}

fn start_zellij() -> TestSession {
    TestRunner::new(TERMINAL_SIZE)
        .with_config("mouse_mode true")
        .start()
}

fn attach_window_and_wait_for_the_prompt(zellij: &TestSession) -> WindowClient {
    let mut window = WindowClient::attach(zellij, TERMINAL_SIZE);
    window.until("the window to draw the session", |window| {
        window.text().contains("$ ") && !window.geometry.panes.is_empty()
    });
    window
}

fn open_a_floating_pane(zellij: &TestSession, window: &mut WindowClient) -> PaneRect {
    zellij.send_stdin(&keys::ctrl('p'));
    zellij.send_stdin(&keys::key('w'));
    let floating_terminal = zellij.expect_pty_spawn();
    floating_terminal.output(PROMPT);
    window.until("a floating pane in the window's layout", |window| {
        window.floating().is_some()
    });
    window.floating().unwrap()
}

#[test]
fn a_popup_the_window_opened_is_part_of_the_layout_it_is_told_about() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    let mut window = attach_window_and_wait_for_the_prompt(&zellij);
    assert!(window.popup().is_none());

    window.mouse(MouseEvent::new_right_press_event(at(30, 10)));
    window.mouse(MouseEvent::new_right_release_event(at(30, 10)));
    window.until("the context menu to open as a popup", |window| {
        window.text().contains(COMMON_MENU_MARKER) && window.popup().is_some()
    });

    let menu = window.popup().unwrap();
    assert!(menu.focused(), "a menu takes the focus: {:?}", menu);
    assert!(
        menu.plugin_ui(),
        "a menu handles its own clicks: {:?}",
        menu
    );
    let menu_row = window
        .text()
        .lines()
        .position(|line| line.contains(COMMON_MENU_MARKER))
        .unwrap() as u16;
    assert!(
        menu.contains(menu.x + 1, menu_row),
        "the popup rectangle covers what the menu drew: {:?} row {}",
        menu,
        menu_row
    );

    let inside = (menu.x + 1, menu_row);
    let outside = if menu.x > 5 {
        (menu.x - 3, menu.y + 1)
    } else {
        (menu.x + menu.cols + 3, menu.y + 1)
    };
    for (x, y) in [inside, outside] {
        assert_eq!(window.geometry.pointer_at(x, y, false), PointerKind::Arrow);
        assert!(
            window.geometry.claims_the_click(x, y),
            "while the menu is open the session takes the click at {:?}",
            (x, y)
        );
    }

    window.key(BareKey::Esc, &[0x1b]);
    window.until("the menu to leave the layout", |window| {
        window.popup().is_none() && !window.text().contains(COMMON_MENU_MARKER)
    });
    assert_eq!(
        window.geometry.pointer_at(outside.0, outside.1, false),
        PointerKind::Text
    );
    assert!(!window.geometry.claims_the_click(outside.0, outside.1));

    window.leave();
    zellij.quit();
}

#[test]
fn a_popup_opened_by_another_client_is_not_in_the_window_layout() {
    let mut zellij = start_zellij();
    let terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    let mut window = attach_window_and_wait_for_the_prompt(&zellij);

    zellij.send_stdin(&format!("\u{1b}[<2;{};{}M", 30, 10).into_bytes());
    zellij.send_stdin(&format!("\u{1b}[<2;{};{}m", 30, 10).into_bytes());
    zellij.wait_until("the terminal client's menu to open", |grid| {
        grid.contains(COMMON_MENU_MARKER)
    });
    terminal.output(b"while the menu is open");
    window.until("the window to draw past the opened menu", |window| {
        window.text().contains("while the menu is open")
    });
    assert!(window.popup().is_none(), "{:?}", window.geometry);
    assert!(!window.text().contains(COMMON_MENU_MARKER));
    assert_eq!(window.geometry.pointer_at(30, 10, false), PointerKind::Text);

    zellij.send_stdin(&keys::ESC);
    zellij.wait_until("the terminal client's menu to close", |grid| {
        !grid.contains(COMMON_MENU_MARKER)
    });

    window.leave();
    zellij.quit();
}

#[test]
fn the_pointer_over_a_floating_frame_matches_what_a_drag_there_does() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    let mut window = attach_window_and_wait_for_the_prompt(&zellij);
    let floating = open_a_floating_pane(&zellij, &mut window);

    assert!(floating.framed() && floating.selectable(), "{:?}", floating);
    assert!(
        window
            .geometry
            .panes
            .iter()
            .filter(|pane| pane.selectable() && !pane.floating())
            .all(|pane| pane.under_floating()),
        "{:?}",
        window.geometry
    );
    let content = (floating.content_x() + 2, floating.content_y() + 1);
    assert_eq!(
        window.geometry.pointer_at(content.0, content.1, false),
        PointerKind::Text
    );
    let left_edge = (floating.x, floating.y + 2);
    assert_eq!(
        window.geometry.pointer_at(left_edge.0, left_edge.1, false),
        PointerKind::Move
    );
    assert_eq!(
        window.geometry.pointer_at(left_edge.0, left_edge.1, true),
        PointerKind::ColResize
    );

    let to = (left_edge.0 + 5, left_edge.1 + 1);
    left_drag(&mut window, left_edge, to, false);
    window.until("the floating pane to follow the drag", |window| {
        window
            .floating()
            .map(|moved| (moved.x, moved.y) == (floating.x + 5, floating.y + 1))
            .unwrap_or(false)
    });
    let moved = window.floating().unwrap();
    assert_eq!((moved.cols, moved.rows), (floating.cols, floating.rows));

    window.leave();
    zellij.quit();
}

#[test]
fn a_ctrl_drag_on_a_floating_frame_resizes_it_as_the_pointer_says() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    let mut window = attach_window_and_wait_for_the_prompt(&zellij);
    let floating = open_a_floating_pane(&zellij, &mut window);

    let right_edge = (floating.x + floating.cols - 1, floating.y + 2);
    assert_eq!(
        window.geometry.pointer_at(right_edge.0, right_edge.1, true),
        PointerKind::ColResize
    );
    left_drag(
        &mut window,
        right_edge,
        (right_edge.0 + 4, right_edge.1),
        true,
    );
    window.until("the floating pane to widen", |window| {
        window
            .floating()
            .map(|resized| resized.cols > floating.cols && resized.x == floating.x)
            .unwrap_or(false)
    });

    window.leave();
    zellij.quit();
}

#[test]
fn the_hand_over_a_floating_pane_pin_marks_where_a_click_pins_it() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    let mut window = attach_window_and_wait_for_the_prompt(&zellij);
    let floating = open_a_floating_pane(&zellij, &mut window);
    window.until("the pin button to be drawn", |window| {
        window.text().contains("PIN [ ]")
    });

    let pin: Vec<u16> = (floating.x..floating.x + floating.cols)
        .filter(|x| window.geometry.pointer_at(*x, floating.y, false) == PointerKind::Hand)
        .collect();
    assert_eq!(pin.len(), 3, "{:?} {:?}", pin, floating);
    let row: Vec<char> = window
        .text()
        .lines()
        .nth(floating.y as usize)
        .unwrap()
        .chars()
        .collect();
    let drawn: String = pin.iter().map(|x| row[*x as usize]).collect();
    assert_eq!(drawn, "[ ]", "the hand covers the drawn checkbox");

    left_click(&mut window, pin[1], floating.y);
    window.until("the pane to be pinned", |window| {
        window.text().contains("PIN [+]")
    });

    window.leave();
    zellij.quit();
}

#[test]
fn the_bars_and_plugin_panes_show_an_arrow_and_terminals_a_text_pointer() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    let mut window = attach_window_and_wait_for_the_prompt(&zellij);
    window.until("the tab and status bars in the layout", |window| {
        window
            .geometry
            .panes
            .iter()
            .filter(|pane| pane.plugin_ui())
            .count()
            >= 2
    });

    let last_row = TERMINAL_SIZE.rows as u16 - 1;
    assert_eq!(window.geometry.pointer_at(10, 0, false), PointerKind::Arrow);
    assert_eq!(
        window.geometry.pointer_at(10, last_row, false),
        PointerKind::Arrow
    );
    let terminal = *window
        .geometry
        .panes
        .iter()
        .find(|pane| pane.selectable() && !pane.plugin_ui())
        .expect("the terminal pane is described");
    assert_eq!(
        window
            .geometry
            .pointer_at(terminal.content_x() + 1, terminal.content_y() + 1, false),
        PointerKind::Text
    );

    window.leave();
    zellij.quit();
}

#[test]
fn another_users_cursor_reaches_the_window_as_a_coloured_cell() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    let mut window = attach_window_and_wait_for_the_prompt(&zellij);

    zellij.send_stdin(&keys::ctrl('p'));
    zellij.send_stdin(&keys::key('r'));
    let right_terminal = zellij.expect_pty_spawn();
    right_terminal.output(PROMPT);
    let grid = zellij.wait_until("the terminal client to focus the new pane", |grid| {
        grid.status_bar_appears()
            && grid
                .cursor
                .map(|cursor| cursor.x > TERMINAL_SIZE.cols / 2)
                .unwrap_or(false)
    });
    let cursor = grid.cursor.unwrap();

    window.until("the other client's cursor in the window", |window| {
        WireColor::unpack(window.cell(cursor.x, cursor.y).bg) != WireColor::Default
    });
    let left_of_cursor = window.cell(cursor.x - 1, cursor.y);
    assert_eq!(
        WireColor::unpack(left_of_cursor.bg),
        WireColor::Default,
        "only the cursor cell is coloured"
    );

    right_terminal.output(b"typed");
    let moved = cursor.x + "typed".len();
    window.until("the other client's cursor to follow its typing", |window| {
        WireColor::unpack(window.cell(moved, cursor.y).bg) != WireColor::Default
            && WireColor::unpack(window.cell(cursor.x, cursor.y).bg) == WireColor::Default
    });

    window.leave();
    zellij.quit();
}
