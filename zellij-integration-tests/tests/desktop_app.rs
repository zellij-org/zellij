#![cfg(target_os = "linux")]

use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

use zellij_utils::app_control::{
    describe_app_at, send_app_request_to, AppReply, AppRequest, AppState, OpenWindowRequest,
    PanelState, PopupView,
};
use zellij_utils::consts::{
    app_dir_in, session_socket_path_in, APP_CONTROL_SOCKET_NAME, APP_PANEL_SESSION,
    CLIENT_SERVER_CONTRACT_DIR,
};
use zellij_utils::data::{BareKey, KeyWithModifier, PaneId};
use zellij_utils::input::actions::Action;
use zellij_utils::ipc::{ClientToServerMsg, ServerToClientMsg};
use zellij_utils::session_link::SessionLink;
use zellij_window::test_support::{FakeSessionBus, Message, Value, NOTIFICATIONS};

const REQUIRE_ENV: &str = "ZELLIJ_REQUIRE_DESKTOP_TESTS";
const BINARY_ENV: &str = "ZELLIJ_TEST_BINARY";
const LIMIT: Duration = Duration::from_secs(30);
const CONFIG: &str = "show_startup_tips false\nshow_release_notes false\n";
const XKB_X11: &str = "libxkbcommon-x11.so.0";
const LIBRARY_DIRS: [&str; 6] = [
    "/usr/lib/x86_64-linux-gnu",
    "/usr/lib/aarch64-linux-gnu",
    "/usr/lib64",
    "/usr/lib",
    "/lib/x86_64-linux-gnu",
    "/lib64",
];

fn unavailable(reason: &str) -> Option<Desktop> {
    if std::env::var_os(REQUIRE_ENV).is_some() {
        panic!("the desktop app tests cannot run: {}", reason);
    }
    eprintln!("skipping the desktop app test: {}", reason);
    None
}

fn on_path(program: &str) -> bool {
    std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).any(|dir| dir.join(program).is_file()))
        .unwrap_or(false)
}

fn zellij_binary() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os(BINARY_ENV) {
        return Some(PathBuf::from(path)).filter(|path| path.is_file());
    }
    let test = std::env::current_exe().ok()?;
    let profile_dir = test.parent()?.parent()?;
    Some(profile_dir.join("zellij")).filter(|path| path.is_file())
}

fn start_xvfb() -> Option<(Child, String)> {
    let mut xvfb = Command::new("Xvfb")
        .args([
            "-displayfd",
            "1",
            "-screen",
            "0",
            "1280x800x24",
            "-nolisten",
            "tcp",
            "-noreset",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let Some(stdout) = xvfb.stdout.take() else {
        let _ = xvfb.kill();
        let _ = xvfb.wait();
        return None;
    };
    let (sender, received) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut reader = std::io::BufReader::new(stdout);
        let mut line = String::new();
        let _ = std::io::BufRead::read_line(&mut reader, &mut line);
        let _ = sender.send(line);
        let _ = std::io::copy(&mut reader, &mut std::io::sink());
    });
    let number = received
        .recv_timeout(Duration::from_secs(10))
        .ok()
        .and_then(|line| line.trim().parse::<u32>().ok());
    match number {
        Some(number) => Some((xvfb, format!(":{}", number))),
        None => {
            let _ = xvfb.kill();
            let _ = xvfb.wait();
            None
        },
    }
}

struct Desktop {
    root: tempfile::TempDir,
    binary: PathBuf,
    display: String,
    xvfb: Child,
    apps: Vec<Child>,
    bus: Option<FakeSessionBus>,
}

impl Desktop {
    fn start() -> Option<Desktop> {
        let Some(binary) = zellij_binary() else {
            return unavailable(&format!(
                "no zellij executable next to the test (build it with `cargo xtask integration-test` or set {})",
                BINARY_ENV
            ));
        };
        if !on_path("Xvfb") {
            return unavailable("Xvfb is not installed");
        }
        if !LIBRARY_DIRS
            .iter()
            .any(|dir| Path::new(dir).join(XKB_X11).exists())
        {
            return unavailable("libxkbcommon-x11 is not installed");
        }
        let Some((xvfb, display)) = start_xvfb() else {
            return unavailable("Xvfb could not start a display");
        };
        let root = tempfile::Builder::new()
            .prefix("zd")
            .tempdir_in("/tmp")
            .expect("a temporary folder");
        for folder in ["sock", "home", "cache", "data", "config/zellij", "runtime"] {
            std::fs::create_dir_all(root.path().join(folder)).unwrap();
        }
        std::fs::write(root.path().join("config/zellij/config.kdl"), CONFIG).unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(
            root.path().join("runtime"),
            std::fs::Permissions::from_mode(0o700),
        )
        .unwrap();
        Some(Desktop {
            root,
            binary,
            display,
            xvfb,
            apps: vec![],
            bus: None,
        })
    }

    fn start_with_bus() -> Option<Desktop> {
        let mut desktop = Desktop::start()?;
        desktop.bus = Some(FakeSessionBus::listen(desktop.root.path().join("bus")));
        Some(desktop)
    }

    fn bus(&self) -> &FakeSessionBus {
        self.bus
            .as_ref()
            .expect("the desktop runs with a session bus")
    }

    fn socket_dir(&self) -> PathBuf {
        self.root.path().join("sock")
    }

    fn contract_dir(&self) -> PathBuf {
        self.socket_dir().join(&*CLIENT_SERVER_CONTRACT_DIR)
    }

    fn control_socket(&self) -> PathBuf {
        app_dir_in(&self.contract_dir()).join(APP_CONTROL_SOCKET_NAME)
    }

    fn session_index(&self) -> Option<zellij_utils::session_index::SessionIndex> {
        let cache_dir = self
            .root
            .path()
            .join("cache")
            .join(zellij_utils::consts::ZELLIJ_CACHE_DIR.file_name()?)
            .join(&*CLIENT_SERVER_CONTRACT_DIR);
        let index_path = cache_dir.join(zellij_utils::session_index::SESSION_INDEX_FILE_NAME);
        if !index_path.exists() {
            return None;
        }
        zellij_utils::session_index::SessionIndex::open(
            &index_path,
            self.contract_dir(),
            cache_dir.join("session_info"),
        )
        .ok()
    }

    fn socket_of(&self, session: &str) -> PathBuf {
        let id = self
            .session_index()
            .and_then(|index| index.session_id(session).ok().flatten());
        match id {
            Some(id) => self.contract_dir().join(id),
            None => session_socket_path_in(&self.contract_dir(), session),
        }
    }

    fn command(&self, args: &[&str]) -> Command {
        let root = self.root.path();
        let mut command = Command::new(&self.binary);
        command
            .args(args)
            .env_clear()
            .env("PATH", std::env::var_os("PATH").unwrap_or_default())
            .env("HOME", root.join("home"))
            .env("XDG_CACHE_HOME", root.join("cache"))
            .env("XDG_DATA_HOME", root.join("data"))
            .env("XDG_CONFIG_HOME", root.join("config"))
            .env("ZELLIJ_CONFIG_DIR", root.join("config/zellij"))
            .env("XDG_RUNTIME_DIR", root.join("runtime"))
            .env("DISPLAY", &self.display)
            .env("SHELL", "/bin/sh")
            .stdin(Stdio::null());
        if let Some(bus) = &self.bus {
            command.env("DBUS_SESSION_BUS_ADDRESS", bus.address());
        }
        command
    }

    fn window_command(&self, extra: &[&str]) -> Command {
        let socket_dir = self.socket_dir();
        let mut args = vec!["window", "--socket-dir", socket_dir.to_str().unwrap()];
        args.extend_from_slice(extra);
        self.command(&args)
    }

    fn log_path(&self, index: usize) -> PathBuf {
        self.root.path().join(format!("app-{}.log", index))
    }

    fn launch(&mut self, extra: &[&str]) -> usize {
        let index = self.apps.len();
        let log = std::fs::File::create(self.log_path(index)).unwrap();
        let child = self
            .window_command(extra)
            .stdout(log.try_clone().unwrap())
            .stderr(log)
            .spawn()
            .expect("the zellij executable starts");
        self.apps.push(child);
        index
    }

    fn launch_and_wait(&mut self, extra: &[&str]) -> ExitStatus {
        let index = self.launch(extra);
        self.wait_for_exit(index)
    }

    fn wait_for_exit(&mut self, index: usize) -> ExitStatus {
        let deadline = Instant::now() + LIMIT;
        loop {
            if let Some(status) = self.apps[index].try_wait().unwrap() {
                return status;
            }
            if Instant::now() >= deadline {
                panic!("the app did not exit\n{}", self.logs());
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    fn logs(&self) -> String {
        (0..self.apps.len())
            .map(|index| {
                format!(
                    "--- app {} ---\n{}",
                    index,
                    std::fs::read_to_string(self.log_path(index)).unwrap_or_default()
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn state(&self) -> Option<AppState> {
        describe_app_at(&self.control_socket()).ok()
    }

    fn wait_for_state(&self, what: &str, check: impl Fn(&AppState) -> bool) -> AppState {
        let deadline = Instant::now() + LIMIT;
        let mut last = None;
        loop {
            if let Some(state) = self.state() {
                if check(&state) {
                    return state;
                }
                last = Some(state);
            }
            if Instant::now() >= deadline {
                panic!(
                    "timed out waiting for {}\nlast state: {:?}\n{}",
                    what,
                    last,
                    self.logs()
                );
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    fn wait_for(&self, what: &str, check: impl Fn() -> bool) {
        let deadline = Instant::now() + LIMIT;
        while !check() {
            if Instant::now() >= deadline {
                panic!(
                    "timed out waiting for {}\nlast state: {:?}\n{}",
                    what,
                    self.state(),
                    self.logs()
                );
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    fn start_app(&mut self) -> String {
        self.launch(&[]);
        let state = self.wait_for_state("the first window on a new session", |state| {
            state.windows.len() == 1 && state.windows[0].session.is_some()
        });
        let session = state.windows[0].session.clone().unwrap();
        self.wait_for_state("the app to see its session", |state| {
            state.live_sessions.contains(&session)
        });
        session
    }

    fn start_background_session(&self, name: &str) {
        let status = self
            .command(&["attach", "--create-background", name])
            .env("ZELLIJ_SOCKET_DIR", self.socket_dir())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .expect("zellij attach runs");
        assert!(status.success(), "the background session {} started", name);
        self.wait_for_state("the app to see the background session", |state| {
            state.live_sessions.iter().any(|live| live == name)
        });
    }

    fn listed_sessions(&self) -> Vec<String> {
        let output = self
            .command(&["list-sessions", "--short"])
            .env("ZELLIJ_SOCKET_DIR", self.socket_dir())
            .output()
            .expect("zellij list-sessions runs");
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(|line| line.trim().to_owned())
            .filter(|line| !line.is_empty())
            .collect()
    }

    fn answers(&self, session: &str) -> bool {
        let Ok(mut link) = SessionLink::open_path(&self.socket_of(session)) else {
            return false;
        };
        if link.send(ClientToServerMsg::ConnStatus).is_err() {
            return false;
        }
        matches!(
            link.recv(Duration::from_millis(500)),
            Ok(ServerToClientMsg::Connected)
        )
    }

    fn act_on_panel(&self, actions: Vec<Action>) {
        let mut link = SessionLink::open_path(&self.socket_of(APP_PANEL_SESSION))
            .expect("the panel session answers");
        for action in actions {
            link.send(ClientToServerMsg::Action {
                action,
                terminal_id: None,
                client_id: None,
                is_cli_client: true,
            })
            .unwrap();
            loop {
                match link.recv(Duration::from_secs(5)) {
                    Ok(ServerToClientMsg::UnblockInputThread) => break,
                    Ok(ServerToClientMsg::Exit { .. }) | Err(_) => return,
                    Ok(_) => {},
                }
            }
        }
    }

    fn panel_plugin(&self) -> Option<u32> {
        self.panel_pane_lines()?
            .iter()
            .filter(|line| line.contains("session-manager"))
            .flat_map(|line| line.split_whitespace())
            .find_map(|word| word.strip_prefix("plugin_")?.parse().ok())
    }

    fn panel_shows(&self, view: &str) -> bool {
        let title = format!("Zellij · {}", view);
        self.panel_pane_lines()
            .map(|lines| lines.iter().any(|line| line.contains(&title)))
            .unwrap_or(false)
    }

    fn wait_for_panel_to_show(&self, view: &str) {
        let deadline = Instant::now() + LIMIT;
        while !self.panel_shows(view) {
            if Instant::now() >= deadline {
                panic!(
                    "the panel never showed {}\npanes: {:?}\n{}",
                    view,
                    self.panel_pane_lines(),
                    self.logs()
                );
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    fn needs_you_in(&self, session: &str) -> Option<usize> {
        let mut link = SessionLink::open_path(&self.socket_of(session)).ok()?;
        link.send(ClientToServerMsg::RequestSessionAttention {
            include_command_text: false,
        })
        .ok()?;
        match link.recv(Duration::from_secs(2)).ok()? {
            ServerToClientMsg::SessionAttention { attention } => Some(attention.counts.needs_you),
            _ => None,
        }
    }

    fn make_session_need_you(&self, session: &str) {
        let deadline = Instant::now() + LIMIT;
        loop {
            let mut link =
                SessionLink::open_path(&self.socket_of(session)).expect("the session answers");
            link.send(ClientToServerMsg::Action {
                action: Action::WriteCharsToPaneId {
                    chars: "printf '\\033]7501;state=blocked:kind=question\\033\\\\'\n".to_owned(),
                    pane_id: PaneId::Terminal(0),
                },
                terminal_id: None,
                client_id: None,
                is_cli_client: true,
            })
            .unwrap();
            let _ = link.recv(Duration::from_secs(5));
            let asked = Instant::now();
            while asked.elapsed() < Duration::from_secs(3) {
                if self.needs_you_in(session) == Some(1) {
                    return;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            if Instant::now() >= deadline {
                panic!(
                    "timed out waiting for {} to need you: {:?}\n{}",
                    session,
                    self.pane_lines_of(session),
                    self.logs()
                );
            }
        }
    }

    fn toggle_inbox_with_the_shortcut(&mut self) {
        assert!(self.launch_and_wait(&["--inbox"]).success());
    }

    fn panel_pane_lines(&self) -> Option<Vec<String>> {
        let mut link = SessionLink::open_path(&self.socket_of(APP_PANEL_SESSION)).ok()?;
        link.send(ClientToServerMsg::Action {
            action: Action::ListPanes {
                show_tab: false,
                show_command: true,
                show_state: false,
                show_geometry: false,
                show_all: false,
                output_json: false,
            },
            terminal_id: None,
            client_id: None,
            is_cli_client: true,
        })
        .ok()?;
        loop {
            match link.recv(Duration::from_secs(2)).ok()? {
                ServerToClientMsg::Log { lines } => return Some(lines),
                ServerToClientMsg::UnblockInputThread | ServerToClientMsg::Exit { .. } => {
                    return None
                },
                _ => {},
            }
        }
    }

    fn wait_for_panel_plugin(&self) -> u32 {
        let deadline = Instant::now() + LIMIT;
        loop {
            if let Some(plugin) = self.panel_plugin() {
                return plugin;
            }
            if Instant::now() >= deadline {
                panic!("the panel never showed its plugin\n{}", self.logs());
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    fn type_into_panel(&self, text: &str) {
        let plugin = self.wait_for_panel_plugin();
        let keys = text
            .chars()
            .map(|character| press(plugin, BareKey::Char(character)))
            .collect();
        self.act_on_panel(keys);
    }

    fn press_enter_in_panel(&self) {
        let plugin = self.wait_for_panel_plugin();
        self.act_on_panel(vec![press(plugin, BareKey::Enter)]);
    }

    fn running_sessions(&self) -> Vec<String> {
        let names_by_id = self
            .session_index()
            .and_then(|index| index.names_by_id().ok())
            .unwrap_or_default();
        std::fs::read_dir(self.contract_dir())
            .map(|entries| {
                entries
                    .filter_map(|entry| entry.ok())
                    .filter(|entry| {
                        use std::os::unix::fs::FileTypeExt;
                        entry
                            .file_type()
                            .map(|kind| kind.is_socket())
                            .unwrap_or(false)
                    })
                    .map(|entry| entry.file_name().to_string_lossy().into_owned())
                    .map(|id| names_by_id.get(&id).cloned().unwrap_or(id))
                    .collect()
            })
            .unwrap_or_default()
    }

    fn stop_everything(&mut self) {
        for app in self.apps.iter_mut() {
            if app.try_wait().ok().flatten().is_none() {
                let _ = Command::new("kill").arg(app.id().to_string()).status();
            }
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        for app in self.apps.iter_mut() {
            while app.try_wait().ok().flatten().is_none() && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(20));
            }
            let _ = app.kill();
            let _ = app.wait();
        }
        let mut sessions = self.running_sessions();
        sessions.push(APP_PANEL_SESSION.to_owned());
        for session in sessions {
            if let Ok(mut link) = SessionLink::open_path(&self.socket_of(&session)) {
                let _ = link.send(ClientToServerMsg::KillSession);
                let _ = link.recv(Duration::from_secs(2));
            }
        }
    }
}

impl Drop for Desktop {
    fn drop(&mut self) {
        self.stop_everything();
        let _ = self.xvfb.kill();
        let _ = self.xvfb.wait();
    }
}

fn press(plugin: u32, bare_key: BareKey) -> Action {
    Action::WriteKeyToPaneId {
        key_with_modifier: Some(KeyWithModifier::new(bare_key)),
        pane_id: PaneId::Plugin(plugin),
    }
}

fn panel(view: PopupView) -> Option<PanelState> {
    Some(PanelState {
        view: Some(view),
        shown: true,
    })
}

#[test]
fn the_app_opens_one_window_on_a_new_session_and_keeps_its_panel_session_hidden() {
    let Some(mut desktop) = Desktop::start() else {
        return;
    };
    let session = desktop.start_app();
    assert!(desktop.listed_sessions().contains(&session));
    assert_eq!(desktop.launch_and_wait(&["--sessions"]).code(), Some(0));
    desktop.wait_for_state("the sessions panel", |state| {
        state.panel == panel(PopupView::Sessions)
    });
    assert!(desktop.answers(APP_PANEL_SESSION));
    let state = desktop.state().unwrap();
    assert_eq!(state.live_sessions, vec![session.clone()]);
    assert_eq!(state.windows.len(), 1);
    assert_eq!(desktop.listed_sessions(), vec![session.clone()]);
    assert_eq!(
        desktop.running_sessions(),
        vec![session],
        "an older zellij reading the socket folder finds only the real session"
    );
}

#[test]
fn a_panel_shortcut_toggles_its_own_view_and_switches_another() {
    let Some(mut desktop) = Desktop::start() else {
        return;
    };
    desktop.start_app();
    assert!(desktop.launch_and_wait(&["--inbox"]).success());
    desktop.wait_for_state("the inbox panel", |state| {
        state.panel == panel(PopupView::Inbox)
    });
    assert!(desktop.launch_and_wait(&["--sessions"]).success());
    desktop.wait_for_state("the panel switched to the sessions", |state| {
        state.panel == panel(PopupView::Sessions)
    });
    assert!(desktop.launch_and_wait(&["--sessions"]).success());
    let state = desktop.wait_for_state("the panel closed", |state| state.panel.is_none());
    assert_eq!(state.windows.len(), 1);
    assert!(
        desktop.answers(APP_PANEL_SESSION),
        "closing the panel keeps its session"
    );
}

#[test]
fn a_panel_shortcut_without_a_running_app_starts_the_app_with_only_the_panel() {
    let Some(mut desktop) = Desktop::start() else {
        return;
    };
    desktop.launch(&["--inbox"]);
    let state = desktop.wait_for_state("the inbox panel", |state| {
        state.panel == panel(PopupView::Inbox)
    });
    assert!(state.windows.is_empty(), "{:?}", state);
    assert!(desktop.answers(APP_PANEL_SESSION));
    assert!(desktop.running_sessions().is_empty());
}

#[test]
fn a_second_launch_opens_its_window_in_the_running_app() {
    let Some(mut desktop) = Desktop::start() else {
        return;
    };
    let first = desktop.start_app();
    assert!(desktop.launch_and_wait(&[]).success());
    let state = desktop.wait_for_state("a second window in the same app", |state| {
        state.windows.len() == 2 && state.windows.iter().all(|w| w.session.is_some())
    });
    assert_ne!(state.windows[1].session.as_deref(), Some(first.as_str()));
    assert!(desktop.apps[0].try_wait().unwrap().is_none());
}

#[test]
fn choosing_a_session_in_the_panel_opens_a_window_for_it_or_brings_its_window_forward() {
    let Some(mut desktop) = Desktop::start() else {
        return;
    };
    let first = desktop.start_app();
    desktop.start_background_session("elsewhere");
    assert!(desktop.launch_and_wait(&["--sessions"]).success());
    desktop.wait_for_state("the sessions panel", |state| {
        state.panel == panel(PopupView::Sessions)
    });
    desktop.type_into_panel("elsewhere");
    desktop.press_enter_in_panel();
    let state = desktop.wait_for_state("a window on the chosen session", |state| {
        state.panel.is_none()
            && state
                .windows
                .iter()
                .any(|window| window.session.as_deref() == Some("elsewhere"))
    });
    assert_eq!(state.windows.len(), 2);
    assert!(desktop.launch_and_wait(&["--sessions"]).success());
    desktop.wait_for_state("the sessions panel again", |state| {
        state.panel == panel(PopupView::Sessions)
    });
    desktop.type_into_panel(&first);
    desktop.press_enter_in_panel();
    let state = desktop.wait_for_state("the panel to go to the session", |state| {
        state.panel.is_none()
    });
    assert_eq!(
        state.windows.len(),
        2,
        "the window already on the session is brought forward instead of opening another"
    );
}

#[test]
fn confirming_quit_in_the_panel_stops_every_session_and_the_app() {
    let Some(mut desktop) = Desktop::start() else {
        return;
    };
    let first = desktop.start_app();
    desktop.start_background_session("elsewhere");
    assert_eq!(
        send_app_request_to(
            &desktop.control_socket(),
            &AppRequest::ShowPopup(PopupView::Quit)
        ),
        Ok(AppReply::Done)
    );
    desktop.wait_for_state("the quit question", |state| {
        state.panel == panel(PopupView::Quit)
    });
    desktop.press_enter_in_panel();
    desktop.wait_for_exit(0);
    for session in [first.as_str(), "elsewhere", APP_PANEL_SESSION] {
        desktop.wait_for(&format!("{} to stop", session), || {
            !desktop.answers(session)
        });
    }
}

#[test]
fn the_inbox_shortcut_and_the_tray_open_the_sessions_until_something_needs_you() {
    let Some(mut desktop) = Desktop::start() else {
        return;
    };
    desktop.start_app();
    desktop.start_background_session("elsewhere");
    desktop.toggle_inbox_with_the_shortcut();
    desktop.wait_for_state("the inbox panel", |state| {
        state.panel == panel(PopupView::Inbox)
    });
    desktop.wait_for_panel_to_show("Sessions");
    desktop.toggle_inbox_with_the_shortcut();
    desktop.wait_for_state("the panel closed", |state| state.panel.is_none());
    assert_eq!(
        send_app_request_to(
            &desktop.control_socket(),
            &AppRequest::ShowPopup(PopupView::Inbox)
        ),
        Ok(AppReply::Done)
    );
    desktop.wait_for_state("the inbox panel from the tray", |state| {
        state.panel == panel(PopupView::Inbox)
    });
    desktop.wait_for_panel_to_show("Sessions");
    assert!(!desktop.panel_shows("Inbox"));
    desktop.toggle_inbox_with_the_shortcut();
    desktop.wait_for_state("the panel closed", |state| state.panel.is_none());
    desktop.make_session_need_you("elsewhere");
    desktop.toggle_inbox_with_the_shortcut();
    desktop.wait_for_state("the inbox panel", |state| {
        state.panel == panel(PopupView::Inbox)
    });
    desktop.wait_for_panel_to_show("Inbox");
    desktop.toggle_inbox_with_the_shortcut();
    desktop.wait_for_state("the panel closed", |state| state.panel.is_none());
    assert_eq!(
        send_app_request_to(
            &desktop.control_socket(),
            &AppRequest::ShowPopup(PopupView::Inbox)
        ),
        Ok(AppReply::Done)
    );
    desktop.wait_for_state("the inbox panel from the tray", |state| {
        state.panel == panel(PopupView::Inbox)
    });
    desktop.wait_for_panel_to_show("Inbox");
}

const NOTIFICATIONS_PATH: &str = "/org/freedesktop/Notifications";
const MENU_PATH: &str = "/MenuBar";
const MENU_INTERFACE: &str = "com.canonical.dbusmenu";
const WATCHER: &str = "org.kde.StatusNotifierWatcher";
const MENU_PAUSE_HOUR: i32 = 51;
const MENU_RESUME: i32 = 53;
const MENU_CLEAR_ALL: i32 = 9;
const MENU_CLOSE_ALL_WINDOWS: i32 = 111;
const MENU_HIDE_PERMANENTLY: i32 = 62;
const QUIET: Duration = Duration::from_millis(3000);

impl Desktop {
    fn act_on(&self, session: &str, actions: Vec<Action>) {
        let mut link =
            SessionLink::open_path(&self.socket_of(session)).expect("the session answers");
        for action in actions {
            link.send(ClientToServerMsg::Action {
                action,
                terminal_id: None,
                client_id: None,
                is_cli_client: true,
            })
            .unwrap();
            loop {
                match link.recv(Duration::from_secs(5)) {
                    Ok(ServerToClientMsg::UnblockInputThread) => break,
                    Ok(ServerToClientMsg::Exit { .. }) | Err(_) => return,
                    Ok(_) => {},
                }
            }
        }
    }

    fn notifications(&self) -> Vec<(u32, String)> {
        let mut next = 0;
        let mut shown: Vec<(u32, String)> = vec![];
        for call in self.bus().calls_to(NOTIFICATIONS, "Notify") {
            let replaces = call.body.get(1).and_then(Value::as_u32).unwrap_or(0);
            let id = if replaces == 0 {
                next += 1;
                next
            } else {
                replaces
            };
            let text = call
                .body
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(" ");
            shown.push((id, text));
        }
        shown
    }

    fn notification_about(&self, session: &str) -> Option<u32> {
        self.notifications()
            .into_iter()
            .find(|(_, text)| text.contains(session))
            .map(|(id, _)| id)
    }

    fn wait_for_notification_about(&self, session: &str) -> u32 {
        self.wait_for(&format!("a notification about {}", session), || {
            self.notification_about(session).is_some()
        });
        self.notification_about(session).unwrap()
    }

    fn tray_service(&self) -> String {
        self.wait_for("the tray icon to register", || {
            !self
                .bus()
                .calls_to(WATCHER, "RegisterStatusNotifierItem")
                .is_empty()
        });
        self.bus().calls_to(WATCHER, "RegisterStatusNotifierItem")[0]
            .body
            .first()
            .and_then(Value::as_str)
            .unwrap()
            .to_owned()
    }

    fn click_menu(&self, id: i32) {
        let service = self.tray_service();
        self.bus()
            .call_client(
                &service,
                MENU_PATH,
                MENU_INTERFACE,
                "Event",
                vec![
                    Value::I32(id),
                    Value::str("clicked"),
                    Value::variant(Value::I32(0)),
                    Value::U32(0),
                ],
            )
            .expect("the tray menu takes the click");
    }

    fn pane_lines_of(&self, session: &str) -> Option<Vec<String>> {
        let mut link = SessionLink::open_path(&self.socket_of(session)).ok()?;
        link.send(ClientToServerMsg::Action {
            action: Action::ListPanes {
                show_tab: false,
                show_command: true,
                show_state: false,
                show_geometry: false,
                show_all: false,
                output_json: false,
            },
            terminal_id: None,
            client_id: None,
            is_cli_client: true,
        })
        .ok()?;
        loop {
            match link.recv(Duration::from_secs(2)).ok()? {
                ServerToClientMsg::Log { lines } => return Some(lines),
                ServerToClientMsg::UnblockInputThread | ServerToClientMsg::Exit { .. } => {
                    return None
                },
                _ => {},
            }
        }
    }
}

#[test]
fn a_session_needing_you_shows_a_notification_and_seeing_it_withdraws_it() {
    let Some(mut desktop) = Desktop::start_with_bus() else {
        return;
    };
    desktop.start_app();
    desktop.start_background_session("elsewhere");
    desktop.make_session_need_you("elsewhere");
    let id = desktop.wait_for_notification_about("elsewhere");
    desktop.act_on(
        "elsewhere",
        vec![Action::MarkPaneSeen {
            session: Some("elsewhere".to_owned()),
            pane_id: Some(PaneId::Terminal(0)),
        }],
    );
    desktop.wait_for("the notification to be withdrawn", || {
        desktop
            .bus()
            .calls_to(NOTIFICATIONS, "CloseNotification")
            .iter()
            .any(|call| call.body.first().and_then(Value::as_u32) == Some(id))
    });
}

#[test]
fn clicking_a_notification_opens_a_window_on_its_session() {
    let Some(mut desktop) = Desktop::start_with_bus() else {
        return;
    };
    desktop.start_app();
    desktop.start_background_session("elsewhere");
    desktop.make_session_need_you("elsewhere");
    let id = desktop.wait_for_notification_about("elsewhere");
    let has_window = |state: &AppState| {
        state
            .windows
            .iter()
            .any(|window| window.session.as_deref() == Some("elsewhere"))
    };
    let deadline = Instant::now() + LIMIT;
    while !desktop.state().map_or(false, |state| has_window(&state)) {
        assert!(
            Instant::now() < deadline,
            "timed out waiting for a window on the notified session\nlast state: {:?}\n{}",
            desktop.state(),
            desktop.logs()
        );
        desktop.bus().signal(Message::signal(
            NOTIFICATIONS_PATH,
            NOTIFICATIONS,
            "ActionInvoked",
            vec![Value::U32(id), Value::str("default")],
        ));
        let retry_at = Instant::now() + Duration::from_secs(1);
        while Instant::now() < retry_at
            && !desktop.state().map_or(false, |state| has_window(&state))
        {
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}

#[test]
fn the_tray_icon_registers_and_its_menu_works() {
    let Some(mut desktop) = Desktop::start_with_bus() else {
        return;
    };
    let first = desktop.start_app();
    desktop.start_background_session("paused");
    desktop.start_background_session("resumed");
    desktop.tray_service();
    desktop.click_menu(MENU_PAUSE_HOUR);
    desktop.make_session_need_you("paused");
    std::thread::sleep(QUIET);
    assert_eq!(
        desktop.notification_about("paused"),
        None,
        "a paused app notified: {:?}",
        desktop.notifications()
    );
    desktop.click_menu(MENU_RESUME);
    desktop.make_session_need_you("resumed");
    desktop.wait_for_notification_about("resumed");
    desktop.click_menu(MENU_CLEAR_ALL);
    for session in ["paused", "resumed"] {
        desktop.wait_for(&format!("{} to be cleared", session), || {
            desktop.needs_you_in(session) == Some(0)
        });
    }
    desktop.click_menu(MENU_CLOSE_ALL_WINDOWS);
    desktop.wait_for_state("every window closed", |state| state.windows.is_empty());
    for session in [first.as_str(), "paused", "resumed"] {
        assert!(desktop.answers(session), "{} stopped", session);
    }
    desktop.click_menu(MENU_HIDE_PERMANENTLY);
    desktop.wait_for_state("the icon to be hidden", |state| !state.tray_icon);
    let config = desktop.root.path().join("config/zellij/config.kdl");
    desktop.wait_for("the hidden icon to be saved in the config file", || {
        std::fs::read_to_string(&config)
            .map(|text| text.contains("attention") && text.contains("tray_icon false"))
            .unwrap_or(false)
    });
}

#[test]
fn toggle_tray_icon_from_the_cli_hides_and_shows_the_real_icon() {
    let Some(mut desktop) = Desktop::start() else {
        return;
    };
    desktop.start_app();
    let before = desktop.state().unwrap().tray_icon;
    let toggle = |desktop: &Desktop| {
        let status = desktop
            .command(&["action", "toggle-tray-icon"])
            .env("ZELLIJ_SOCKET_DIR", desktop.socket_dir())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .expect("zellij action runs");
        assert!(status.success());
    };
    toggle(&desktop);
    desktop.wait_for_state("the icon to flip", |state| state.tray_icon != before);
    toggle(&desktop);
    desktop.wait_for_state("the icon to flip back", |state| state.tray_icon == before);
}

#[test]
fn a_launch_after_the_app_crashed_takes_over() {
    let Some(mut desktop) = Desktop::start() else {
        return;
    };
    desktop.start_app();
    let _ = Command::new("kill")
        .args(["-9", &desktop.apps[0].id().to_string()])
        .status();
    desktop.wait_for_exit(0);
    desktop.launch(&[]);
    desktop.wait_for_state("a new app with a window", |state| {
        state.windows.len() == 1 && state.windows[0].session.is_some()
    });
    assert!(desktop.apps[1].try_wait().unwrap().is_none());
}

#[test]
fn two_socket_folders_keep_two_apps_apart() {
    let Some(mut first) = Desktop::start() else {
        return;
    };
    let Some(mut second) = Desktop::start() else {
        return;
    };
    let first_session = first.start_app();
    let second_session = second.start_app();
    let first_state = first.state().unwrap();
    let second_state = second.state().unwrap();
    assert_eq!(first_state.live_sessions, vec![first_session.clone()]);
    assert_eq!(second_state.live_sessions, vec![second_session.clone()]);
    assert_ne!(first_session, second_session);
}

#[test]
fn a_standalone_window_is_listed_brought_forward_and_closed_by_the_app() {
    let Some(mut desktop) = Desktop::start_with_bus() else {
        return;
    };
    desktop.start_app();
    desktop.start_background_session("solo");
    let standalone = desktop.launch(&["--standalone", "solo"]);
    let on_solo = |state: &AppState| -> usize {
        state
            .windows
            .iter()
            .filter(|window| window.session.as_deref() == Some("solo"))
            .count()
    };
    desktop.wait_for_state("the app to list the standalone window", |state| {
        state.windows.len() == 2 && on_solo(state) == 1
    });
    assert_eq!(
        send_app_request_to(
            &desktop.control_socket(),
            &AppRequest::OpenWindow(OpenWindowRequest {
                session: Some("solo".to_owned()),
                create: false,
                ..Default::default()
            })
        ),
        Ok(AppReply::Done)
    );
    std::thread::sleep(QUIET);
    let state = desktop.state().unwrap();
    assert_eq!(state.windows.len(), 2, "{:?}\n{}", state, desktop.logs());
    assert_eq!(on_solo(&state), 1, "{:?}", state);
    assert!(
        desktop.apps[standalone].try_wait().unwrap().is_none(),
        "the standalone window exited\n{}",
        desktop.logs()
    );
    desktop.click_menu(MENU_CLOSE_ALL_WINDOWS);
    desktop.wait_for_state("every window closed, the standalone one too", |state| {
        state.windows.is_empty()
    });
    desktop.wait_for_exit(standalone);
    assert!(desktop.answers("solo"), "closing the windows stopped solo");
}

#[test]
fn the_desktop_entry_is_installed_and_removed_from_the_cli() {
    let Some(desktop) = Desktop::start() else {
        return;
    };
    let applications = desktop.root.path().join("data/applications");
    let output = desktop
        .command(&["setup", "--install-desktop-entry"])
        .env(
            zellij_utils::desktop_integration::DESKTOP_OVERRIDE_ENV,
            "on",
        )
        .output()
        .expect("zellij runs");
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let entries: Vec<PathBuf> = std::fs::read_dir(&applications)
        .expect("the applications folder exists")
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.extension().map(|e| e == "desktop").unwrap_or(false))
        .collect();
    assert_eq!(entries.len(), 1, "{:?}", entries);
    let entry = std::fs::read_to_string(&entries[0]).unwrap();
    for action in [
        "[Desktop Action inbox]",
        "[Desktop Action sessions]",
        "[Desktop Action new-window]",
    ] {
        assert!(entry.contains(action), "{} is missing:\n{}", action, entry);
    }
    let status = desktop
        .command(&["setup", "--uninstall-desktop-entry"])
        .env(
            zellij_utils::desktop_integration::DESKTOP_OVERRIDE_ENV,
            "on",
        )
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .expect("zellij runs");
    assert!(status.success());
    assert!(!entries[0].exists());
}

#[test]
fn a_new_session_window_starts_in_the_chosen_folder() {
    let Some(mut desktop) = Desktop::start() else {
        return;
    };
    desktop.start_app();
    let folder = desktop.root.path().join("chosen-folder");
    std::fs::create_dir_all(&folder).unwrap();
    assert_eq!(
        send_app_request_to(
            &desktop.control_socket(),
            &AppRequest::OpenWindow(zellij_utils::app_control::OpenWindowRequest {
                session: Some("in-folder".to_owned()),
                create: true,
                cwd: Some(folder.clone()),
                ..Default::default()
            })
        ),
        Ok(AppReply::Done)
    );
    desktop.wait_for_state("a window on the new session", |state| {
        state
            .windows
            .iter()
            .any(|window| window.session.as_deref() == Some("in-folder"))
    });
    let folder_text = folder.display().to_string();
    desktop.wait_for("the new session's pane to start in the folder", || {
        desktop
            .pane_lines_of("in-folder")
            .map(|lines| lines.iter().any(|line| line.contains(&folder_text)))
            .unwrap_or(false)
    });
}

#[test]
fn a_window_on_an_unnamed_new_session_starts_in_the_chosen_folder() {
    let Some(mut desktop) = Desktop::start() else {
        return;
    };
    let first = desktop.start_app();
    let folder = desktop.root.path().join("unnamed-folder");
    std::fs::create_dir_all(&folder).unwrap();
    assert_eq!(
        send_app_request_to(
            &desktop.control_socket(),
            &AppRequest::OpenWindow(OpenWindowRequest {
                session: None,
                create: true,
                cwd: Some(folder.clone()),
                ..Default::default()
            })
        ),
        Ok(AppReply::Done)
    );
    let is_new = |session: &Option<String>| -> bool {
        session
            .as_deref()
            .map(|name| name != first)
            .unwrap_or(false)
    };
    let state = desktop.wait_for_state("a window on a second session", |state| {
        state.windows.iter().any(|window| is_new(&window.session))
    });
    let session = state
        .windows
        .iter()
        .find(|window| is_new(&window.session))
        .and_then(|window| window.session.clone())
        .unwrap();
    let folder_text = folder.display().to_string();
    desktop.wait_for("the new session's pane to start in the folder", || {
        desktop
            .pane_lines_of(&session)
            .map(|lines| lines.iter().any(|line| line.contains(&folder_text)))
            .unwrap_or(false)
    });
}

#[test]
fn the_cli_setup_actions_refuse_what_the_setup_screen_would_not_offer() {
    let Some(desktop) = Desktop::start() else {
        return;
    };
    let output = desktop
        .command(&["setup", "--install-desktop-entry"])
        .env(
            zellij_utils::desktop_integration::DESKTOP_OVERRIDE_ENV,
            "off",
        )
        .output()
        .expect("zellij runs");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Install the desktop entry: Turned off by ZELLIJ_DESKTOP_INTEGRATION"),
        "{}",
        stderr
    );
    assert!(
        !desktop.root.path().join("data/applications").exists()
            || std::fs::read_dir(desktop.root.path().join("data/applications"))
                .unwrap()
                .next()
                .is_none()
    );
    let output = desktop
        .command(&["setup", "--keybinding-preset", "unlock-first"])
        .output()
        .expect("zellij runs");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let config = std::fs::read_to_string(desktop.root.path().join("config/zellij/config.kdl"))
        .expect("the config file was written");
    assert!(
        config.contains("keybinds preset=\"unlock-first\""),
        "{}",
        config
    );
    let output = desktop
        .command(&["setup", "--keybinding-preset", "no-such-preset"])
        .output()
        .expect("zellij runs");
    assert!(!output.status.success());
}
