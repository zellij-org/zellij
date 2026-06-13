use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use zellij_client::os_input_output::SignalEvent;
use zellij_client::ClientInfo;
use zellij_utils::cli::CliArgs;
use zellij_utils::data::{ConnectToSession, LayoutInfo};
use zellij_utils::input::options::Options;
use zellij_utils::pane_size::Size;
use zellij_utils::setup::Setup;

use crate::client_screen::GridSnapshot;
use crate::fake_client_os_api::{FakeClientHandle, FakeClientOsApi};
use crate::fake_pty::FakePtyHandle;
use crate::fake_server_os_api::FakeServerOsApi;
use crate::{keys, test_env};

pub struct TestRunner {
    size: Size,
    extra_config_kdl: String,
    layout: Option<LayoutInfo>,
}

impl TestRunner {
    pub fn new(size: Size) -> Self {
        TestRunner {
            size,
            extra_config_kdl: String::new(),
            layout: None,
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

    pub fn start(self) -> TestSession {
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

        let concurrency_slot = test_env::acquire_concurrency_slot();

        let fake_server_os_api = FakeServerOsApi::default();
        let server_thread: Arc<Mutex<Option<JoinHandle<()>>>> = Arc::new(Mutex::new(None));
        let server_spawner = in_process_server_spawner(fake_server_os_api.clone(), &server_thread);

        let (fake_client_os_api, fake_client_handle) =
            FakeClientOsApi::new(self.size, Some(server_spawner));
        let layout_info = self.layout.or(default_layout_info);
        let client_thread = spawn_client_thread(
            fake_client_os_api,
            cli_args.clone(),
            config.clone(),
            config_options.clone(),
            ClientInfo::New(session_name.clone(), layout_info, None),
        );

        TestSession {
            session_name,
            cli_args,
            config,
            config_options,
            fake_server_os_api,
            server_thread,
            main_client: TestClient {
                fake_client_handle,
                thread: Some(client_thread),
            },
            _concurrency_slot: concurrency_slot,
        }
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

    pub fn snapshot(&self) -> GridSnapshot {
        self.fake_client_handle.client_screen.snapshot()
    }

    fn join(&mut self) -> Option<ConnectToSession> {
        self.thread
            .take()
            .and_then(|join_handle: JoinHandle<Option<ConnectToSession>>| {
                join_handle.join().expect("client thread panicked")
            })
    }
}

pub struct TestSession {
    session_name: String,
    cli_args: CliArgs,
    config: zellij_utils::input::config::Config,
    config_options: Options,
    fake_server_os_api: FakeServerOsApi,
    server_thread: Arc<Mutex<Option<JoinHandle<()>>>>,
    main_client: TestClient,
    _concurrency_slot: test_env::ConcurrencySlot,
}

impl TestSession {
    pub fn session_name(&self) -> &str {
        &self.session_name
    }

    pub fn expect_pty_spawn(&self) -> FakePtyHandle {
        let terminal_id = self
            .fake_server_os_api
            .shared_ptys
            .wait_for("a pty spawn", |fake_pty_registry| {
                fake_pty_registry.spawn_queue.pop_front()
            });
        FakePtyHandle {
            terminal_id,
            shared_ptys: self.fake_server_os_api.shared_ptys.clone(),
        }
    }

    pub fn send_stdin(&self, bytes: &[u8]) {
        self.main_client.send_stdin(bytes);
    }

    pub fn resize(&self, new_size: Size) {
        self.main_client.resize(new_size);
    }

    pub fn wait_until(
        &self,
        what: &str,
        predicate: impl Fn(&GridSnapshot) -> bool,
    ) -> GridSnapshot {
        self.main_client.wait_until(what, predicate)
    }

    pub fn snapshot(&self) -> GridSnapshot {
        self.main_client.snapshot()
    }

    pub fn wait_for_app_load(&self) -> GridSnapshot {
        self.main_client.wait_until("app to load", |grid_snapshot| {
            grid_snapshot.status_bar_appears()
                && grid_snapshot.tab_bar_appears()
                && grid_snapshot.cursor.is_some()
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

    pub fn quit(&mut self) {
        if self.main_client.thread.is_some() {
            self.send_stdin(&keys::QUIT);
            self.main_client.join();
        }
        if let Some(server_thread) = self.server_thread.lock().unwrap().take() {
            server_thread.join().expect("server thread panicked");
        }
    }

    fn send_quit_without_joining(&self) {
        let _ = self
            .main_client
            .fake_client_handle
            .stdin_tx
            .send(keys::QUIT.to_vec());
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
    strip_trailing_whitespace(&text)
}

fn replace_unique_session_name(text: &str) -> String {
    regex::Regex::new(r"Zellij \(test-\d+\)")
        .unwrap()
        .replace_all(text, "Zellij (test)")
        .to_string()
}

fn strip_swap_layout_indication(text: &str) -> String {
    let powerline_separated_or_plain_base = |prefix: &str| {
        regex::Regex::new(&format!("{} [\u{e0b0}]? ?BASE ?[\u{e0b0}]?[ ]*\n", prefix)).unwrap()
    };
    let normal_mode = powerline_separated_or_plain_base("Alt <\\[\\]>");
    let tmux_mode_1 = powerline_separated_or_plain_base("Alt \\[\\|SPACE\\|Alt \\]");
    let tmux_mode_2 = powerline_separated_or_plain_base("Alt \\[\\|Alt \\]\\|SPACE");
    let text = normal_mode.replace_all(text, "\n");
    let text = tmux_mode_1.replace_all(&text, "\n");
    tmux_mode_2.replace_all(&text, "\n").to_string()
}

fn strip_trailing_whitespace(text: &str) -> String {
    regex::Regex::new(r"\s*\n")
        .unwrap()
        .replace_all(text, "\n")
        .to_string()
}
