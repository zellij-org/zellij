use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use zellij_client::os_input_output::{ClientOsApi, SignalEvent};
use zellij_utils::data::Palette;
use zellij_utils::errors::ErrorContext;
use zellij_utils::ipc::{
    ClientToServerMsg, IpcReceiverWithContext, IpcSenderWithContext, ServerToClientMsg,
};
use zellij_utils::pane_size::Size;
use zellij_utils::shared::default_palette;

use crate::client_screen::ClientScreen;

type ServerSpawner = Box<dyn FnOnce(PathBuf) + Send>;

pub struct FakeClientOsApi {
    client_screen: ClientScreen,
    size: Arc<Mutex<Size>>,
    stdin_rx: crossbeam::channel::Receiver<Vec<u8>>,
    signal_rx: crossbeam::channel::Receiver<SignalEvent>,
    send_instructions_to_server: Arc<Mutex<Option<IpcSenderWithContext<ClientToServerMsg>>>>,
    receive_instructions_from_server: Arc<Mutex<Option<IpcReceiverWithContext<ServerToClientMsg>>>>,
    session_name: Arc<Mutex<Option<String>>>,
    server_spawner: Arc<Mutex<Option<ServerSpawner>>>,
}

impl Clone for FakeClientOsApi {
    fn clone(&self) -> Self {
        FakeClientOsApi {
            client_screen: self.client_screen.clone(),
            size: self.size.clone(),
            stdin_rx: self.stdin_rx.clone(),
            signal_rx: self.signal_rx.clone(),
            send_instructions_to_server: self.send_instructions_to_server.clone(),
            receive_instructions_from_server: self.receive_instructions_from_server.clone(),
            session_name: self.session_name.clone(),
            server_spawner: self.server_spawner.clone(),
        }
    }
}

impl std::fmt::Debug for FakeClientOsApi {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FakeClientOsApi").finish()
    }
}

pub struct FakeClientHandle {
    pub client_screen: ClientScreen,
    pub size: Arc<Mutex<Size>>,
    pub stdin_tx: crossbeam::channel::Sender<Vec<u8>>,
    pub signal_tx: crossbeam::channel::Sender<SignalEvent>,
}

impl FakeClientOsApi {
    pub fn new(
        initial_size: Size,
        server_spawner: Option<ServerSpawner>,
    ) -> (Self, FakeClientHandle) {
        let size = Arc::new(Mutex::new(initial_size));
        let client_screen = ClientScreen::new(size.clone());
        let (stdin_tx, stdin_rx) = crossbeam::channel::unbounded();
        let (signal_tx, signal_rx) = crossbeam::channel::unbounded();
        let fake_client_os_api = FakeClientOsApi {
            client_screen: client_screen.clone(),
            size: size.clone(),
            stdin_rx,
            signal_rx,
            send_instructions_to_server: Arc::new(Mutex::new(None)),
            receive_instructions_from_server: Arc::new(Mutex::new(None)),
            session_name: Arc::new(Mutex::new(None)),
            server_spawner: Arc::new(Mutex::new(server_spawner)),
        };
        let fake_client_handle = FakeClientHandle {
            client_screen,
            size,
            stdin_tx,
            signal_tx,
        };
        (fake_client_os_api, fake_client_handle)
    }
}

impl ClientOsApi for FakeClientOsApi {
    fn get_terminal_size(&self) -> Size {
        *self.size.lock().unwrap()
    }
    fn set_raw_mode(&mut self) {}
    fn unset_raw_mode(&self) -> Result<(), std::io::Error> {
        Ok(())
    }
    fn get_stdout_writer(&self) -> Box<dyn std::io::Write> {
        self.client_screen.writer()
    }
    fn get_stdin_reader(&self) -> Box<dyn std::io::BufRead> {
        Box::new(std::io::BufReader::new(std::io::empty()))
    }
    fn update_session_name(&mut self, new_session_name: String) {
        *self.session_name.lock().unwrap() = Some(new_session_name);
    }
    fn read_from_stdin(&mut self) -> Result<Vec<u8>, &'static str> {
        self.stdin_rx.recv().map_err(|_| "fake client stdin closed")
    }
    fn box_clone(&self) -> Box<dyn ClientOsApi> {
        Box::new(self.clone())
    }
    fn send_to_server(&self, msg: ClientToServerMsg) {
        match self.send_instructions_to_server.lock().unwrap().as_mut() {
            Some(ipc_sender) => {
                let _ = ipc_sender.send_client_msg(msg);
            },
            None => {
                log::warn!("Server not ready, dropping message.");
            },
        }
    }
    fn recv_from_server(&self) -> Option<(ServerToClientMsg, ErrorContext)> {
        self.receive_instructions_from_server
            .lock()
            .unwrap()
            .as_mut()
            .unwrap()
            .recv_server_msg()
    }
    fn handle_signals(
        &self,
        sigwinch_cb: Box<dyn Fn()>,
        quit_cb: Box<dyn Fn()>,
        _resize_receiver: Option<std::sync::mpsc::Receiver<()>>,
    ) {
        loop {
            match self.signal_rx.recv() {
                Ok(SignalEvent::Resize) => sigwinch_cb(),
                Ok(SignalEvent::Quit) => {
                    quit_cb();
                    break;
                },
                Err(_) => break,
            }
        }
    }
    fn connect_to_server(&self, path: &Path) {
        let socket;
        loop {
            match zellij_utils::consts::ipc_connect(path) {
                Ok(sock) => {
                    socket = sock;
                    break;
                },
                Err(_) => {
                    std::thread::sleep(std::time::Duration::from_millis(10));
                },
            }
        }
        let ipc_sender = IpcSenderWithContext::new(socket);
        let ipc_receiver = ipc_sender.get_receiver();
        *self.send_instructions_to_server.lock().unwrap() = Some(ipc_sender);
        *self.receive_instructions_from_server.lock().unwrap() = Some(ipc_receiver);
    }
    fn spawn_server(&self, socket_path: &Path, _debug: bool) -> Result<(), std::io::Error> {
        if let Some(server_spawner) = self.server_spawner.lock().unwrap().take() {
            server_spawner(socket_path.to_path_buf());
        }
        Ok(())
    }
    fn load_palette(&self) -> Palette {
        default_palette()
    }
    fn enable_mouse(&self) -> anyhow::Result<()> {
        Ok(())
    }
    fn disable_mouse(&self) -> anyhow::Result<()> {
        Ok(())
    }
    fn env_variable(&self, _name: &str) -> Option<String> {
        None
    }
    fn should_install_panic_hook(&self) -> bool {
        false
    }
}
