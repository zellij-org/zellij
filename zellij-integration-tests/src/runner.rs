use std::path::PathBuf;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use zellij_client::os_input_output::SignalEvent;
use zellij_client::ClientInfo;
use zellij_utils::cli::{CliAction, CliArgs};
use zellij_utils::data::{CommandOrPlugin, ConnectToSession, LayoutInfo};
use zellij_utils::input::actions::{Action, RunCommandAction};
use zellij_utils::input::options::Options;
use zellij_utils::pane_size::Size;
use zellij_utils::setup::Setup;

use crate::client_screen::GridSnapshot;
use crate::fake_client_os_api::{FakeClientHandle, FakeClientOsApi};
use crate::fake_pty::FakePtyHandle;
use crate::fake_server_os_api::FakeServerOsApi;
use crate::{keys, test_env};

const GUEST_MODAL_TITLE: &str = "Nested Zellij session detected";

pub struct TestRunner {
    size: Size,
    extra_config_kdl: String,
    layout: Option<LayoutInfo>,
    initial_panes: Option<Vec<CommandOrPlugin>>,
    env: std::collections::HashMap<String, String>,
    stdout_tap: Option<crossbeam::channel::Sender<Vec<u8>>>,
    skip_concurrency_slot: bool,
}

impl TestRunner {
    pub fn new(size: Size) -> Self {
        TestRunner {
            size,
            extra_config_kdl: String::new(),
            layout: None,
            initial_panes: None,
            env: std::collections::HashMap::new(),
            stdout_tap: None,
            skip_concurrency_slot: false,
        }
    }

    pub fn with_config(mut self, extra_config_kdl: &str) -> Self {
        self.extra_config_kdl.push('\n');
        self.extra_config_kdl.push_str(extra_config_kdl);
        self
    }

    pub fn with_layout(mut self, layout: LayoutInfo) -> Self {
        self.layout = Some(layout);
        self
    }

    pub fn with_initial_command(mut self, command: &[&str]) -> Self {
        let mut command: Vec<String> = command.iter().map(|part| part.to_string()).collect();
        let run_command_action = RunCommandAction {
            command: PathBuf::from(command.remove(0)),
            args: command,
            hold_on_close: true,
            ..Default::default()
        };
        self.initial_panes = Some(vec![CommandOrPlugin::Command(run_command_action)]);
        self
    }

    pub fn with_stdout_tap(mut self, sender: crossbeam::channel::Sender<Vec<u8>>) -> Self {
        self.stdout_tap = Some(sender);
        self
    }

    pub fn skip_concurrency_slot(mut self) -> Self {
        self.skip_concurrency_slot = true;
        self
    }

    pub fn start(self) -> TestSession {
        let (session_context, fake_client_os_api, fake_client_handle, client_info) = self.boot();
        let client_thread = spawn_client_thread(
            fake_client_os_api,
            session_context.cli_args.clone(),
            session_context.config.clone(),
            session_context.config_options.clone(),
            client_info,
        );
        session_context.into_session(TestClient {
            fake_client_handle,
            thread: Some(client_thread),
        })
    }

    pub fn start_in_background(self) -> BackgroundTestSession {
        let (session_context, fake_client_os_api, _fake_client_handle, client_info) = self.boot();
        let cli_args = session_context.cli_args.clone();
        let config = session_context.config.clone();
        let config_options = session_context.config_options.clone();
        let detaching_client_thread = std::thread::Builder::new()
            .name("in_process_zellij_detached_client".to_string())
            .spawn(move || {
                let start_detached_and_exit = true;
                zellij_client::start_client(
                    Box::new(fake_client_os_api),
                    cli_args,
                    config,
                    config_options,
                    client_info,
                    None,
                    None,
                    false,
                    start_detached_and_exit,
                );
            })
            .unwrap();
        join_detaching_client_thread_with_timeout(detaching_client_thread);
        BackgroundTestSession { session_context }
    }

    fn boot(
        self,
    ) -> (
        SessionContext,
        FakeClientOsApi,
        FakeClientHandle,
        ClientInfo,
    ) {
        test_env::init();
        let session_name = test_env::unique_session_name();
        let config_path = test_env::write_config(&session_name, &self.extra_config_kdl);
        let data_dir = test_env::init().join("data");

        let cli_args = CliArgs {
            config: Some(config_path),
            data_dir: Some(data_dir),
            ..Default::default()
        };
        let (config, default_layout_info, config_options, _, _) =
            Setup::from_cli_args(&cli_args).expect("failed to load harness config");

        let concurrency_slot = if self.skip_concurrency_slot {
            None
        } else {
            Some(test_env::acquire_concurrency_slot())
        };

        let fake_server_os_api = FakeServerOsApi::default();
        let server_thread: Arc<Mutex<Option<JoinHandle<()>>>> = Arc::new(Mutex::new(None));
        let server_spawner = in_process_server_spawner(fake_server_os_api.clone(), &server_thread);

        let (fake_client_os_api, fake_client_handle) =
            FakeClientOsApi::new_with_env(self.size, Some(server_spawner), self.env);
        if let Some(stdout_tap) = self.stdout_tap {
            fake_client_handle.client_screen.set_stdout_tap(stdout_tap);
        }
        let layout_info = self.layout.or(default_layout_info);
        let client_info =
            ClientInfo::New(session_name.clone(), layout_info, None, self.initial_panes);

        (
            SessionContext {
                session_name,
                size: self.size,
                cli_args,
                config,
                config_options,
                fake_server_os_api,
                server_thread,
                concurrency_slot,
            },
            fake_client_os_api,
            fake_client_handle,
            client_info,
        )
    }
}

struct SessionContext {
    session_name: String,
    size: Size,
    cli_args: CliArgs,
    config: zellij_utils::input::config::Config,
    config_options: Options,
    fake_server_os_api: FakeServerOsApi,
    server_thread: Arc<Mutex<Option<JoinHandle<()>>>>,
    concurrency_slot: Option<test_env::ConcurrencySlot>,
}

impl SessionContext {
    fn into_session(self, main_client: TestClient) -> TestSession {
        TestSession {
            session_name: self.session_name,
            size: self.size,
            cli_args: self.cli_args,
            config: self.config,
            config_options: self.config_options,
            fake_server_os_api: self.fake_server_os_api,
            server_thread: self.server_thread,
            main_client,
            _concurrency_slot: self.concurrency_slot,
        }
    }
}

pub struct BackgroundTestSession {
    session_context: SessionContext,
}

impl BackgroundTestSession {
    pub fn session_name(&self) -> &str {
        &self.session_context.session_name
    }

    pub fn expect_pty_spawn(&self) -> FakePtyHandle {
        expect_pty_spawn(&self.session_context.fake_server_os_api)
    }

    pub fn attach(self, size: Size) -> TestSession {
        let (fake_client_os_api, fake_client_handle) = FakeClientOsApi::new(size, None);
        let client_thread = spawn_client_thread(
            fake_client_os_api,
            self.session_context.cli_args.clone(),
            self.session_context.config.clone(),
            self.session_context.config_options.clone(),
            ClientInfo::Attach(
                self.session_context.session_name.clone(),
                self.session_context.config_options.clone(),
            ),
        );
        self.session_context.into_session(TestClient {
            fake_client_handle,
            thread: Some(client_thread),
        })
    }
}

fn expect_pty_spawn(fake_server_os_api: &FakeServerOsApi) -> FakePtyHandle {
    let terminal_id = fake_server_os_api
        .shared_ptys
        .wait_for("a pty spawn", |fake_pty_registry| {
            fake_pty_registry.spawn_queue.pop_front()
        });
    FakePtyHandle {
        terminal_id,
        shared_ptys: fake_server_os_api.shared_ptys.clone(),
    }
}

fn join_detaching_client_thread_with_timeout(join_handle: JoinHandle<()>) {
    let (done_tx, done_rx) = mpsc::channel();
    std::thread::Builder::new()
        .name("detaching_client_join_watchdog".to_string())
        .spawn(move || {
            let result = join_handle.join();
            let _ = done_tx.send(result);
        })
        .unwrap();
    match done_rx.recv_timeout(CLIENT_JOIN_TIMEOUT) {
        Ok(Ok(())) => {},
        Ok(Err(_)) => panic!("detaching client thread panicked"),
        Err(_) => panic!("timed out starting a session in the background"),
    }
}

fn in_process_server_spawner(
    fake_server_os_api: FakeServerOsApi,
    server_thread: &Arc<Mutex<Option<JoinHandle<()>>>>,
) -> Box<dyn FnOnce(std::path::PathBuf) + Send> {
    let server_thread = server_thread.clone();
    Box::new(move |socket_path: std::path::PathBuf| {
        let install_panic_hook = false;
        let join_handle = std::thread::Builder::new()
            .name("in_process_zellij_server".to_string())
            .spawn(move || {
                zellij_server::start_server_impl(
                    Box::new(fake_server_os_api),
                    socket_path,
                    install_panic_hook,
                );
            })
            .unwrap();
        *server_thread.lock().unwrap() = Some(join_handle);
    })
}

fn new_pane_cli_action(
    command: &[&str],
    floating: bool,
    close_on_exit: bool,
    start_suspended: bool,
    block_until_exit: bool,
) -> CliAction {
    CliAction::NewPane {
        direction: None,
        command: command.iter().map(|part| part.to_string()).collect(),
        plugin: None,
        cwd: None,
        floating,
        in_place: false,
        close_replaced_pane: false,
        pane_id: None,
        name: None,
        close_on_exit,
        start_suspended,
        configuration: None,
        skip_plugin_cache: false,
        x: None,
        y: None,
        width: None,
        height: None,
        pinned: None,
        stacked: false,
        blocking: false,
        block_until_exit_success: false,
        block_until_exit_failure: false,
        block_until_exit,
        unblock_condition: None,
        near_current_pane: false,
        no_focus: false,
        borderless: None,
        tab_id: None,
    }
}

fn referenced_contents_files(layout: &str) -> impl Iterator<Item = &str> {
    layout
        .match_indices("contents_file=\"")
        .filter_map(|(start, marker)| {
            let rest = &layout[start + marker.len()..];
            rest.find('"').map(|end| &rest[..end])
        })
}

fn spawn_client_thread(
    fake_client_os_api: FakeClientOsApi,
    cli_args: CliArgs,
    config: zellij_utils::input::config::Config,
    config_options: Options,
    client_info: ClientInfo,
) -> JoinHandle<Option<ConnectToSession>> {
    std::thread::Builder::new()
        .name("in_process_zellij_client".to_string())
        .spawn(move || {
            let tab_position_to_focus = None;
            let pane_id_to_focus = None;
            let is_a_reconnect = false;
            let start_detached_and_exit = false;
            zellij_client::start_client(
                Box::new(fake_client_os_api),
                cli_args,
                config,
                config_options,
                client_info,
                tab_position_to_focus,
                pane_id_to_focus,
                is_a_reconnect,
                start_detached_and_exit,
            )
        })
        .unwrap()
}

pub struct TestClient {
    fake_client_handle: FakeClientHandle,
    thread: Option<JoinHandle<Option<ConnectToSession>>>,
}

#[derive(Clone)]
pub struct GuestResizer {
    size: Arc<Mutex<Size>>,
    signal_tx: crossbeam::channel::Sender<SignalEvent>,
}

impl GuestResizer {
    pub fn resize(&self, new_size: Size) -> bool {
        *self.size.lock().unwrap() = new_size;
        self.signal_tx.send(SignalEvent::Resize).is_ok()
    }
}

impl TestClient {
    pub fn send_stdin(&self, bytes: &[u8]) {
        self.fake_client_handle
            .stdin_tx
            .send(bytes.to_vec())
            .expect("client stdin closed");
    }

    pub fn resize(&self, new_size: Size) {
        *self.fake_client_handle.size.lock().unwrap() = new_size;
        let _ = self.fake_client_handle.signal_tx.send(SignalEvent::Resize);
    }

    pub fn wait_until(
        &self,
        what: &str,
        predicate: impl Fn(&GridSnapshot) -> bool,
    ) -> GridSnapshot {
        self.fake_client_handle
            .client_screen
            .wait_until(what, predicate)
    }

    pub fn wait_until_raw_output(&self, what: &str, predicate: impl Fn(&[u8]) -> bool) -> Vec<u8> {
        self.fake_client_handle
            .client_screen
            .wait_until_raw_output(what, predicate)
    }

    pub fn snapshot(&self) -> GridSnapshot {
        self.fake_client_handle.client_screen.snapshot()
    }

    pub fn raw_bytes(&self) -> Vec<u8> {
        self.fake_client_handle.client_screen.raw_bytes()
    }

    pub fn received_server_messages(&self) -> Vec<String> {
        self.fake_client_handle.received_server_messages()
    }

    pub fn quit(mut self) {
        self.send_stdin(&keys::ctrl('q'));
        self.join();
    }

    pub fn detach(mut self) {
        self.send_stdin(&keys::ctrl('o'));
        self.send_stdin(&keys::key('d'));
        self.join();
    }

    fn join(&mut self) -> Option<ConnectToSession> {
        self.thread.take().and_then(join_client_thread_with_timeout)
    }
}

fn join_client_thread_with_timeout(
    join_handle: JoinHandle<Option<ConnectToSession>>,
) -> Option<ConnectToSession> {
    let (done_tx, done_rx) = mpsc::channel();
    std::thread::Builder::new()
        .name("nested_client_join_watchdog".to_string())
        .spawn(move || {
            let result = join_handle.join();
            let _ = done_tx.send(result);
        })
        .unwrap();
    match done_rx.recv_timeout(CLIENT_JOIN_TIMEOUT) {
        Ok(Ok(reconnect)) => reconnect,
        Ok(Err(_)) => panic!("client thread panicked"),
        Err(mpsc::RecvTimeoutError::Timeout) => None,
        Err(mpsc::RecvTimeoutError::Disconnected) => None,
    }
}

fn join_server_thread_with_timeout(join_handle: JoinHandle<()>) {
    let (done_tx, done_rx) = mpsc::channel();
    std::thread::Builder::new()
        .name("nested_server_join_watchdog".to_string())
        .spawn(move || {
            let result = join_handle.join();
            let _ = done_tx.send(result);
        })
        .unwrap();
    match done_rx.recv_timeout(CLIENT_JOIN_TIMEOUT) {
        Ok(Ok(())) => {},
        Ok(Err(_)) => panic!("server thread panicked"),
        Err(_) => {},
    }
}

const CLIENT_JOIN_TIMEOUT: Duration = Duration::from_secs(10);

pub struct TestSession {
    session_name: String,
    size: Size,
    cli_args: CliArgs,
    config: zellij_utils::input::config::Config,
    config_options: Options,
    fake_server_os_api: FakeServerOsApi,
    server_thread: Arc<Mutex<Option<JoinHandle<()>>>>,
    main_client: TestClient,
    _concurrency_slot: Option<test_env::ConcurrencySlot>,
}

pub struct CliClientHandle {
    thread: JoinHandle<i32>,
}

impl CliClientHandle {
    pub fn wait_for_exit(self) -> i32 {
        self.thread.join().expect("cli client thread panicked")
    }
}

impl TestSession {
    pub fn session_name(&self) -> &str {
        &self.session_name
    }

    pub fn expect_pty_spawn(&self) -> FakePtyHandle {
        expect_pty_spawn(&self.fake_server_os_api)
    }

    pub fn main_client(&self) -> &TestClient {
        &self.main_client
    }

    pub fn send_stdin(&self, bytes: &[u8]) {
        self.main_client.send_stdin(bytes);
    }

    pub fn stdin_sender(&self) -> crossbeam::channel::Sender<Vec<u8>> {
        self.main_client.fake_client_handle.stdin_tx.clone()
    }

    pub fn resize(&self, new_size: Size) {
        self.main_client.resize(new_size);
    }

    pub fn resize_sender(&self) -> GuestResizer {
        GuestResizer {
            size: self.main_client.fake_client_handle.size.clone(),
            signal_tx: self.main_client.fake_client_handle.signal_tx.clone(),
        }
    }

    pub fn wait_until(
        &self,
        what: &str,
        predicate: impl Fn(&GridSnapshot) -> bool,
    ) -> GridSnapshot {
        self.main_client.wait_until(what, predicate)
    }

    pub fn wait_until_raw_output(&self, what: &str, predicate: impl Fn(&[u8]) -> bool) -> Vec<u8> {
        self.main_client.wait_until_raw_output(what, predicate)
    }

    pub fn snapshot(&self) -> GridSnapshot {
        self.main_client.snapshot()
    }

    pub fn raw_bytes(&self) -> Vec<u8> {
        self.main_client.raw_bytes()
    }

    pub fn received_server_messages(&self) -> Vec<String> {
        self.main_client.received_server_messages()
    }

    pub fn wait_for_app_load(&self) -> GridSnapshot {
        self.main_client.wait_until("app to load", |grid_snapshot| {
            (grid_snapshot.status_bar_appears() || grid_snapshot.contains("Descend:"))
                && grid_snapshot.tab_bar_appears()
                && (grid_snapshot.cursor.is_some() || grid_snapshot.contains(GUEST_MODAL_TITLE))
        })
    }

    pub fn written_files(&self) -> std::collections::HashMap<String, String> {
        self.fake_server_os_api.written_files()
    }

    pub fn attach_client(&self, size: Size) -> TestClient {
        let (fake_client_os_api, fake_client_handle) = FakeClientOsApi::new(size, None);
        let thread = spawn_client_thread(
            fake_client_os_api,
            self.cli_args.clone(),
            self.config.clone(),
            self.config_options.clone(),
            ClientInfo::Attach(self.session_name.clone(), self.config_options.clone()),
        );
        TestClient {
            fake_client_handle,
            thread: Some(thread),
        }
    }

    pub fn attach_watcher(&self, size: Size) -> TestClient {
        let (fake_client_os_api, fake_client_handle) = FakeClientOsApi::new(size, None);
        let thread = spawn_client_thread(
            fake_client_os_api,
            self.cli_args.clone(),
            self.config.clone(),
            self.config_options.clone(),
            ClientInfo::Watch(self.session_name.clone(), self.config_options.clone()),
        );
        TestClient {
            fake_client_handle,
            thread: Some(thread),
        }
    }

    pub fn run_cli_action(&self, cli_action: CliAction) -> i32 {
        self.spawn_cli_client(cli_action)
            .join()
            .expect("cli client thread panicked")
    }

    fn spawn_cli_client(&self, cli_action: CliAction) -> JoinHandle<i32> {
        let actions = Action::actions_from_cli(
            cli_action,
            Box::new(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"))),
            Some(self.config.clone()),
        )
        .expect("failed to build cli actions");
        let (fake_client_os_api, _fake_client_handle) = FakeClientOsApi::new(self.size, None);
        let session_name = self.session_name.clone();
        std::thread::Builder::new()
            .name("in_process_cli_client".to_string())
            .spawn(move || {
                zellij_client::cli_client::start_cli_client(
                    Box::new(fake_client_os_api),
                    &session_name,
                    actions,
                )
            })
            .unwrap()
    }

    pub fn save_session(&self) {
        self.run_cli_action(CliAction::SaveSession);
    }

    pub fn wait_for_serialized_session(&self) {
        let layout_path = zellij_utils::consts::session_layout_cache_file_name(&self.session_name);
        let session_dir = layout_path.parent().map(|parent| parent.to_path_buf());
        let deadline = std::time::Instant::now() + crate::default_timeout();
        loop {
            if let Ok(layout) = std::fs::read_to_string(&layout_path) {
                let referenced_contents_files_exist = session_dir.as_ref().map_or(true, |dir| {
                    referenced_contents_files(&layout).all(|name| dir.join(name).exists())
                });
                if referenced_contents_files_exist {
                    return;
                }
            }
            if std::time::Instant::now() >= deadline {
                panic!(
                    "timed out waiting for session serialization at {}",
                    layout_path.display()
                );
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }

    pub fn resurrect(&mut self, size: Size) {
        let layout_path = zellij_utils::consts::session_layout_cache_file_name(&self.session_name);
        assert!(
            layout_path.exists(),
            "no serialized layout to resurrect from at {}",
            layout_path.display()
        );
        let mut cli_args = self.cli_args.clone();
        if cli_args.config_dir.is_none() {
            cli_args.config_dir = cli_args
                .config
                .as_ref()
                .and_then(|config| config.parent().map(|parent| parent.to_path_buf()));
        }
        let fake_server_os_api = FakeServerOsApi::default();
        let server_thread: Arc<Mutex<Option<JoinHandle<()>>>> = Arc::new(Mutex::new(None));
        let server_spawner = in_process_server_spawner(fake_server_os_api.clone(), &server_thread);
        let (fake_client_os_api, fake_client_handle) =
            FakeClientOsApi::new(size, Some(server_spawner));
        let client_thread = spawn_client_thread(
            fake_client_os_api,
            cli_args,
            self.config.clone(),
            self.config_options.clone(),
            ClientInfo::Resurrect(self.session_name.clone(), layout_path, false, None),
        );
        self.fake_server_os_api = fake_server_os_api;
        self.server_thread = server_thread;
        self.main_client = TestClient {
            fake_client_handle,
            thread: Some(client_thread),
        };
    }

    pub fn override_layout(&self, layout_name: &str) {
        self.run_cli_action(CliAction::OverrideLayout {
            layout: Some(PathBuf::from(layout_name)),
            layout_string: None,
            layout_dir: None,
            retain_existing_terminal_panes: false,
            retain_existing_plugin_panes: false,
            apply_only_to_active_tab: false,
        });
    }

    pub fn run_suspended_command(&self, command: &[&str]) {
        let start_suspended = true;
        self.run_cli_action(new_pane_cli_action(
            command,
            false,
            false,
            start_suspended,
            false,
        ));
    }

    pub fn run_blocking_command(&self, command: &[&str]) -> CliClientHandle {
        let floating = false;
        let close_on_exit = true;
        let block_until_exit = true;
        CliClientHandle {
            thread: self.spawn_cli_client(new_pane_cli_action(
                command,
                floating,
                close_on_exit,
                false,
                block_until_exit,
            )),
        }
    }

    pub fn run_blocking_floating_command(&self, command: &[&str]) -> CliClientHandle {
        let floating = true;
        let close_on_exit = true;
        let block_until_exit = true;
        CliClientHandle {
            thread: self.spawn_cli_client(new_pane_cli_action(
                command,
                floating,
                close_on_exit,
                false,
                block_until_exit,
            )),
        }
    }

    pub fn detach_main_client(&mut self) {
        let connected_clients_before_detach = self.fake_server_os_api.connected_client_count();
        self.send_stdin(&keys::ctrl('o'));
        self.send_stdin(&keys::key('d'));
        self.main_client.join();
        self.wait_for_server_to_release_a_client(connected_clients_before_detach);
    }

    fn wait_for_server_to_release_a_client(&self, connected_clients_before_detach: usize) {
        let deadline = std::time::Instant::now() + crate::default_timeout();
        loop {
            if self.fake_server_os_api.connected_client_count() < connected_clients_before_detach {
                return;
            }
            if std::time::Instant::now() >= deadline {
                panic!("timed out waiting for server to release the detached client");
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }

    pub fn quit(&mut self) {
        if self.main_client.thread.is_some() {
            self.send_stdin(&keys::ctrl('q'));
            self.main_client.join();
        }
        if let Some(server_thread) = self.server_thread.lock().unwrap().take() {
            join_server_thread_with_timeout(server_thread);
        }
    }

    fn send_quit_without_joining(&self) {
        let _ = self
            .main_client
            .fake_client_handle
            .stdin_tx
            .send(keys::ctrl('q').to_vec());
    }
}

impl Drop for TestSession {
    fn drop(&mut self) {
        if self.main_client.thread.is_some() {
            self.send_quit_without_joining();
        }
    }
}

pub fn normalized(grid_snapshot: &GridSnapshot) -> String {
    let text = replace_unique_session_name(&grid_snapshot.text);
    let text = strip_swap_layout_indication(&text);
    let text = strip_tip_indication(&text);
    strip_trailing_whitespace(&text)
}

pub fn assert_same_rendered_grid(actual: &GridSnapshot, expected: &GridSnapshot, what: &str) {
    let actual = normalized(actual);
    let expected = normalized(expected);
    assert!(
        actual == expected,
        "{what}\n--- expected grid ---\n{expected}\n--- actual grid ---\n{actual}"
    );
}

fn strip_tip_indication(text: &str) -> String {
    regex::Regex::new(r" Tip: [^\n]*")
        .unwrap()
        .replace_all(text, "")
        .to_string()
}

fn replace_unique_session_name(text: &str) -> String {
    regex::Regex::new(r"Zellij \(test-\d+\)")
        .unwrap()
        .replace_all(text, "Zellij (test)")
        .to_string()
}

fn strip_swap_layout_indication(text: &str) -> String {
    let swap_layout_name = "BASE|VERTICAL|HORIZONTAL|STACKED|STAGGERED|ENLARGED|SPREAD";
    let powerline_separated_or_plain = |prefix: &str| {
        regex::Regex::new(&format!(
            "{} [\u{e0b0}]? ?(?:{}) ?[\u{e0b0}]?[ ]*\n",
            prefix, swap_layout_name
        ))
        .unwrap()
    };
    let normal_mode = powerline_separated_or_plain("Alt <\\[\\]>");
    let tmux_mode_1 = powerline_separated_or_plain("Alt \\[\\|SPACE\\|Alt \\]");
    let tmux_mode_2 = powerline_separated_or_plain("Alt \\[\\|Alt \\]\\|SPACE");
    let tab_bar = regex::Regex::new(&format!(
        " {{2,}}[\u{e0b0}]? ?(?:{}) ?[\u{e0b0}]?[ ]*\n",
        swap_layout_name
    ))
    .unwrap();
    let text = normal_mode.replace_all(text, "\n");
    let text = tmux_mode_1.replace_all(&text, "\n");
    let text = tmux_mode_2.replace_all(&text, "\n");
    tab_bar.replace_all(&text, "\n").to_string()
}

fn strip_trailing_whitespace(text: &str) -> String {
    regex::Regex::new(r"\s*\n")
        .unwrap()
        .replace_all(text, "\n")
        .to_string()
}
