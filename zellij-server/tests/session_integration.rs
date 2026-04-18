//! Integration test: real client-server communication via IPC sockets.
//!
//! This test binary gets its own process, so we can set `ZELLIJ_SOCKET_DIR`
//! before any lazy_static is initialized, giving us full test isolation.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use zellij_server::os_input_output::{AsyncReader, ServerOsApi};
use zellij_server::panes::PaneId;
use zellij_utils::channels;
use zellij_utils::consts::{ipc_connect, ZELLIJ_SOCK_DIR};
use zellij_utils::data::Palette;
use zellij_utils::errors::prelude::*;
use zellij_utils::input::cli_assets::CliAssets;
use zellij_utils::input::command::RunCommand;
use zellij_utils::ipc::{
    ClientToServerMsg, IpcReceiverWithContext, IpcSenderWithContext, ServerToClientMsg,
};
use zellij_utils::pane_size::Size;
use zellij_utils::sessions::register_session;

use interprocess::local_socket::Stream as LocalSocketStream;

type ClientId = u16;

/// Shared test directory — initialized once per process.
static TEST_DIR: std::sync::OnceLock<tempfile::TempDir> = std::sync::OnceLock::new();

fn init_test_env() {
    TEST_DIR.get_or_init(|| {
        let tmpdir = tempfile::tempdir().expect("failed to create tmpdir");
        std::env::set_var("ZELLIJ_SOCKET_DIR", tmpdir.path());
        std::env::set_var("ZELLIJ_NO_DAEMONIZE", "1");
        let sock_dir = tmpdir.path().join(format!(
            "contract_version_{}",
            zellij_utils::consts::CLIENT_SERVER_CONTRACT_VERSION,
        ));
        std::fs::create_dir_all(&sock_dir).unwrap();
        tmpdir
    });
}

/// Channel-driven mock PTY reader. The test pushes output bytes via
/// `PaneOutputSender`, and the server's terminal_bytes thread reads them
/// through this reader as if a real PTY produced them.
struct MockAsyncReader {
    receiver: tokio::sync::mpsc::Receiver<Vec<u8>>,
    buf: Vec<u8>,
}

impl MockAsyncReader {
    fn new(receiver: tokio::sync::mpsc::Receiver<Vec<u8>>) -> Self {
        Self {
            receiver,
            buf: Vec::new(),
        }
    }
}

#[async_trait::async_trait]
impl AsyncReader for MockAsyncReader {
    async fn read(&mut self, buf: &mut [u8]) -> std::result::Result<usize, std::io::Error> {
        // If we have leftover data from a previous chunk, serve it first.
        if !self.buf.is_empty() {
            let n = std::cmp::min(buf.len(), self.buf.len());
            buf[..n].copy_from_slice(&self.buf[..n]);
            self.buf.drain(..n);
            return Ok(n);
        }
        // Wait for the next chunk from the test.
        match self.receiver.recv().await {
            Some(data) => {
                let n = std::cmp::min(buf.len(), data.len());
                buf[..n].copy_from_slice(&data[..n]);
                if data.len() > n {
                    self.buf.extend_from_slice(&data[n..]);
                }
                Ok(n)
            },
            None => {
                // Channel closed — sleep forever (pane stays idle).
                loop {
                    tokio::time::sleep(Duration::from_secs(3600)).await;
                }
            },
        }
    }
}

/// Handle for pushing output to a specific pane's mock PTY.
type PaneOutputSender = tokio::sync::mpsc::Sender<Vec<u8>>;

/// Minimal ServerOsApi for integration tests.
///
/// Real IPC client handling. Mock PTY with channel-driven output.
#[derive(Clone, Default)]
struct HarnessOsInput {
    client_senders: Arc<Mutex<HashMap<ClientId, channels::Sender<ServerToClientMsg>>>>,
    pane_senders: Arc<Mutex<HashMap<u32, PaneOutputSender>>>,
}

impl HarnessOsInput {
    /// Get a sender for pushing output to a pane by terminal_id.
    fn pane_output_sender(&self, terminal_id: u32) -> Option<PaneOutputSender> {
        self.pane_senders.lock().unwrap().get(&terminal_id).cloned()
    }

    /// Return all terminal IDs that have been allocated by this harness.
    fn terminal_ids(&self) -> Vec<u32> {
        self.pane_senders.lock().unwrap().keys().copied().collect()
    }
}

impl ServerOsApi for HarnessOsInput {
    fn set_terminal_size_using_terminal_id(
        &self,
        _id: u32,
        _cols: u16,
        _rows: u16,
        _w: Option<u16>,
        _h: Option<u16>,
    ) -> Result<()> {
        Ok(())
    }

    fn spawn_terminal(
        &self,
        _action: zellij_utils::input::command::TerminalAction,
        _quit_cb: Box<dyn Fn(PaneId, Option<i32>, RunCommand) + Send>,
        _default_editor: Option<PathBuf>,
    ) -> Result<(u32, Box<dyn AsyncReader>, Option<u32>)> {
        static NEXT_ID: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(1);
        let id = NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let (tx, rx) = tokio::sync::mpsc::channel(256);
        self.pane_senders.lock().unwrap().insert(id, tx);
        Ok((id, Box::new(MockAsyncReader::new(rx)), None))
    }

    fn reserve_terminal_id(&self) -> Result<u32> {
        static NEXT_ID: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(1000);
        Ok(NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::SeqCst))
    }

    fn write_to_tty_stdin(&self, _id: u32, _buf: &[u8]) -> Result<usize> {
        Ok(0)
    }
    fn tcdrain(&self, _id: u32) -> Result<()> {
        Ok(())
    }
    fn kill(&self, _pid: u32) -> Result<()> {
        Ok(())
    }
    fn force_kill(&self, _pid: u32) -> Result<()> {
        Ok(())
    }
    fn send_sigint(&self, _pid: u32) -> Result<()> {
        Ok(())
    }

    fn box_clone(&self) -> Box<dyn ServerOsApi> {
        Box::new((*self).clone())
    }

    fn send_to_client(&self, client_id: ClientId, msg: ServerToClientMsg) -> Result<()> {
        if let Some(sender) = self.client_senders.lock().unwrap().get(&client_id) {
            let _ = sender.send(msg);
        }
        Ok(())
    }

    fn new_client(
        &mut self,
        client_id: ClientId,
        stream: LocalSocketStream,
    ) -> Result<IpcReceiverWithContext<ClientToServerMsg>> {
        let receiver = IpcReceiverWithContext::new(stream);
        let ipc_sender: IpcSenderWithContext<ServerToClientMsg> = receiver.get_sender();
        let (tx, rx) = channels::bounded::<ServerToClientMsg>(500);
        thread::spawn(move || {
            let mut ipc_sender = ipc_sender;
            for msg in rx.iter() {
                let _ = ipc_sender.send_server_msg(msg);
            }
        });
        self.client_senders.lock().unwrap().insert(client_id, tx);
        Ok(receiver)
    }

    fn new_client_with_reply(
        &mut self,
        client_id: ClientId,
        stream: LocalSocketStream,
        reply_stream: LocalSocketStream,
    ) -> Result<IpcReceiverWithContext<ClientToServerMsg>> {
        let receiver = IpcReceiverWithContext::new(stream);
        let ipc_sender: IpcSenderWithContext<ServerToClientMsg> =
            IpcSenderWithContext::new(reply_stream);
        let (tx, rx) = channels::bounded::<ServerToClientMsg>(500);
        thread::spawn(move || {
            let mut ipc_sender = ipc_sender;
            for msg in rx.iter() {
                let _ = ipc_sender.send_server_msg(msg);
            }
        });
        self.client_senders.lock().unwrap().insert(client_id, tx);
        Ok(receiver)
    }

    fn remove_client(&mut self, client_id: ClientId) -> Result<()> {
        self.client_senders.lock().unwrap().remove(&client_id);
        Ok(())
    }
    fn load_palette(&self) -> Palette {
        Palette::default()
    }
    fn get_cwd(&self, _pid: u32) -> Option<PathBuf> {
        None
    }
    fn write_to_file(&mut self, _buf: String, _file: Option<String>) -> Result<()> {
        Ok(())
    }
    fn re_run_command_in_terminal(
        &self,
        _id: u32,
        _cmd: RunCommand,
        _quit_cb: Box<dyn Fn(PaneId, Option<i32>, RunCommand) + Send>,
    ) -> Result<(Box<dyn AsyncReader>, Option<u32>)> {
        let (tx, rx) = tokio::sync::mpsc::channel(256);
        // We don't track re-run terminals in pane_senders for now.
        drop(tx);
        Ok((Box::new(MockAsyncReader::new(rx)), None))
    }
    fn clear_terminal_id(&self, _id: u32) -> Result<()> {
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

/// Try to connect to the server (main pipe + reply pipe on Windows).
fn try_connect(
    socket_path: &std::path::Path,
) -> Option<(
    IpcSenderWithContext<ClientToServerMsg>,
    IpcReceiverWithContext<ServerToClientMsg>,
)> {
    let stream = ipc_connect(socket_path).ok()?;
    #[cfg(windows)]
    {
        let reply_stream = zellij_utils::consts::ipc_connect_reply(socket_path).ok()?;
        Some((
            IpcSenderWithContext::new(stream),
            IpcReceiverWithContext::new(reply_stream),
        ))
    }
    #[cfg(not(windows))]
    {
        let sender = IpcSenderWithContext::new(stream);
        let receiver = sender.get_receiver();
        Some((sender, receiver))
    }
}

fn minimal_cli_assets() -> CliAssets {
    CliAssets {
        config_file_path: None,
        config_dir: None,
        should_ignore_config: true,
        configuration_options: None,
        layout: None,
        terminal_window_size: Size { rows: 24, cols: 80 },
        data_dir: None,
        is_debug: false,
        max_panes: None,
        force_run_layout_commands: false,
        cwd: None,
    }
}

/// A running test session with mock PTY output.
struct TestSession {
    pub sender: IpcSenderWithContext<ClientToServerMsg>,
    pub receiver: IpcReceiverWithContext<ServerToClientMsg>,
    os_input: HarnessOsInput,
}

impl TestSession {
    /// Spawn a server, connect, initialize a session, wait for Render.
    fn new(session_name: &str) -> Self {
        init_test_env();
        let session_id = register_session(session_name).expect("failed to register session");
        let socket_path = ZELLIJ_SOCK_DIR.join(&session_id);

        let os_input = HarnessOsInput::default();
        let os_input_clone = os_input.clone();
        let server_socket_path = socket_path.clone();
        thread::spawn(move || {
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                zellij_server::start_server(Box::new(os_input_clone), server_socket_path);
            }));
        });

        // Poll until connected.
        let (mut sender, receiver) = {
            let mut conn = None;
            for _ in 0..200 {
                if let Some(c) = try_connect(&socket_path) {
                    conn = Some(c);
                    break;
                }
                thread::sleep(Duration::from_millis(10));
            }
            conn.expect("timed out waiting for server to bind")
        };

        sender
            .send_client_msg(ClientToServerMsg::FirstClientConnected {
                cli_assets: minimal_cli_assets(),
                is_web_client: false,
            })
            .expect("failed to send FirstClientConnected");

        let mut session = TestSession {
            sender,
            receiver,
            os_input,
        };
        session.wait_for_render();
        session
    }

    /// Push output bytes to a pane's mock PTY.
    fn push_pane_output(&self, terminal_id: u32, data: &[u8]) {
        if let Some(tx) = self.os_input.pane_output_sender(terminal_id) {
            tx.blocking_send(data.to_vec())
                .expect("failed to push pane output");
        } else {
            panic!("no pane with terminal_id {}", terminal_id);
        }
    }

    /// Send an action to the server.
    fn send_action(&mut self, action: zellij_utils::input::actions::Action) {
        self.sender
            .send_client_msg(ClientToServerMsg::Action {
                action,
                terminal_id: None,
                client_id: None,
                is_cli_client: false,
            })
            .expect("failed to send action");
    }

    /// Drain messages until we get a Render, return its content.
    fn wait_for_render(&mut self) -> String {
        for _ in 0..100 {
            match self.receiver.recv_server_msg() {
                Some((ServerToClientMsg::Render { content }, _)) => return content,
                Some(_) => continue,
                None => thread::sleep(Duration::from_millis(50)),
            }
        }
        panic!("timed out waiting for Render");
    }

    /// Drain Render messages until one contains `needle`, return its content.
    /// Avoids races where the first Render arrives before async PTY bytes have
    /// been parsed into the grid.
    fn wait_for_render_containing(&mut self, needle: &str) -> String {
        for _ in 0..200 {
            match self.receiver.recv_server_msg() {
                Some((ServerToClientMsg::Render { content }, _)) => {
                    if content.contains(needle) {
                        return content;
                    }
                },
                Some(_) => continue,
                None => thread::sleep(Duration::from_millis(50)),
            }
        }
        panic!("timed out waiting for Render containing {:?}", needle);
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn session_initializes_and_renders() {
    let session = TestSession::new("init-test");
    // If we got here, the server initialized, created a pane, and rendered.
    drop(session);
}

#[test]
fn client_can_rename_session_via_action() {
    let session_name = "rename-action-test";
    let mut session = TestSession::new(session_name);

    session.send_action(zellij_utils::input::actions::Action::RenameSession {
        name: "renamed-session".to_string(),
    });

    let mut got_renamed = false;
    for _ in 0..50 {
        match session.receiver.recv_server_msg() {
            Some((ServerToClientMsg::RenamedSession { name }, _)) => {
                assert_eq!(name, "renamed-session");
                got_renamed = true;
                break;
            },
            Some(_) => continue,
            None => thread::sleep(Duration::from_millis(50)),
        }
    }
    assert!(
        got_renamed,
        "client should receive RenamedSession notification"
    );

    let registry = zellij_utils::sessions::read_registry();
    assert!(registry.find_running_by_name("renamed-session").is_some());
    assert!(registry.find_running_by_name(session_name).is_none());
}

#[test]
fn pane_displays_mock_pty_output() {
    let mut session = TestSession::new("pty-output-test");

    // Get the terminal ID of the first pane (allocated by spawn_terminal).
    let terminal_ids = session.os_input.terminal_ids();
    assert!(!terminal_ids.is_empty(), "no terminals were spawned");
    let first_terminal = *terminal_ids.iter().min().unwrap();
    session.push_pane_output(first_terminal, b"hello from mock pty\r\n");

    // The server only sends Render on certain events (resize, input, etc.),
    // not immediately when pane content changes. Force a re-render by
    // sending a terminal resize.
    session
        .sender
        .send_client_msg(ClientToServerMsg::TerminalResize {
            new_size: Size { rows: 24, cols: 80 },
        })
        .expect("failed to send resize");

    let _render = session.wait_for_render_containing("hello from mock pty");
}
