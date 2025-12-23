use axum::extract::ws::Message;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc::UnboundedSender;
use tokio_util::sync::CancellationToken;

use crate::os_input_output::ClientOsApi;
use crate::web_client::session_management::spawn_new_session;
use std::path::PathBuf;
use zellij_utils::{
    input::{config::Config, options::Options},
    ipc::ClientToServerMsg,
};

pub trait ClientOsApiFactory: Send + Sync + std::fmt::Debug {
    fn create_client_os_api(&self) -> Result<Box<dyn ClientOsApi>, Box<dyn std::error::Error>>;
}

#[derive(Debug, Clone)]
pub struct RealClientOsApiFactory;

impl ClientOsApiFactory for RealClientOsApiFactory {
    fn create_client_os_api(&self) -> Result<Box<dyn ClientOsApi>, Box<dyn std::error::Error>> {
        crate::os_input_output::get_client_os_input()
            .map(|os_input| Box::new(os_input) as Box<dyn ClientOsApi>)
            .map_err(|e| format!("Failed to create client OS API: {:?}", e).into())
    }
}

pub trait SessionManager: Send + Sync + std::fmt::Debug {
    fn session_exists(&self, session_name: &str) -> Result<bool, Box<dyn std::error::Error>>;
    fn get_resurrection_layout(
        &self,
        session_name: &str,
    ) -> Option<zellij_utils::input::layout::Layout>;
    fn spawn_session_if_needed(
        &self,
        session_name: &str,
        os_input: Box<dyn ClientOsApi>,
        session_exists: bool,
        zellij_ipc_pipe: &PathBuf,
        first_message: ClientToServerMsg,
    );
}

#[derive(Debug, Clone)]
pub struct RealSessionManager;

impl SessionManager for RealSessionManager {
    fn session_exists(&self, session_name: &str) -> Result<bool, Box<dyn std::error::Error>> {
        zellij_utils::sessions::session_exists(session_name)
            .map_err(|e| format!("Session check failed: {:?}", e).into())
    }

    fn get_resurrection_layout(
        &self,
        session_name: &str,
    ) -> Option<zellij_utils::input::layout::Layout> {
        zellij_utils::sessions::resurrection_layout(session_name)
            .ok()
            .flatten()
    }

    fn spawn_session_if_needed(
        &self,
        session_name: &str,
        os_input: Box<dyn ClientOsApi>,
        session_exists: bool,
        zellij_ipc_pipe: &PathBuf,
        first_message: ClientToServerMsg,
    ) {
        if !session_exists {
            spawn_new_session(session_name, os_input.clone(), zellij_ipc_pipe);
        }
        os_input.connect_to_server(&zellij_ipc_pipe);
        os_input.send_to_server(first_message);
    }
}

#[derive(Debug, Default, Clone)]
pub struct ConnectionTable {
    pub client_id_to_channels: HashMap<String, ClientChannels>,
    pub client_read_only_status: HashMap<String, bool>,
}

#[derive(Debug, Clone)]
pub struct ClientChannels {
    pub os_api: Box<dyn ClientOsApi>,
    pub control_channel_tx: Option<UnboundedSender<Message>>,
    pub terminal_channel_tx: Option<UnboundedSender<String>>,
    terminal_channel_cancellation_token: Option<CancellationToken>,
}

impl ClientChannels {
    pub fn new(os_api: Box<dyn ClientOsApi>) -> Self {
        ClientChannels {
            os_api,
            control_channel_tx: None,
            terminal_channel_tx: None,
            terminal_channel_cancellation_token: None,
        }
    }

    pub fn add_control_tx(&mut self, control_channel_tx: UnboundedSender<Message>) {
        self.control_channel_tx = Some(control_channel_tx);
    }

    pub fn add_terminal_tx(&mut self, terminal_channel_tx: UnboundedSender<String>) {
        self.terminal_channel_tx = Some(terminal_channel_tx);
    }

    pub fn add_terminal_channel_cancellation_token(
        &mut self,
        terminal_channel_cancellation_token: CancellationToken,
    ) {
        self.terminal_channel_cancellation_token = Some(terminal_channel_cancellation_token);
    }
    pub fn cleanup(&mut self) {
        if let Some(terminal_channel_cancellation_token) =
            self.terminal_channel_cancellation_token.take()
        {
            terminal_channel_cancellation_token.cancel();
        }
    }
}

#[derive(Debug)]
pub struct ClientConnectionBus {
    pub connection_table: Arc<Mutex<ConnectionTable>>,
    pub stdout_channel_tx: Option<UnboundedSender<String>>,
    pub control_channel_tx: Option<UnboundedSender<Message>>,
    pub web_client_id: String,
}

impl ClientConnectionBus {
    pub fn new(web_client_id: &str, connection_table: &Arc<Mutex<ConnectionTable>>) -> Self {
        let connection_table = connection_table.clone();
        let web_client_id = web_client_id.to_owned();
        let (stdout_channel_tx, control_channel_tx) = {
            let connection_table = connection_table.lock().unwrap();
            (
                connection_table.get_client_terminal_tx(&web_client_id),
                connection_table.get_client_control_tx(&web_client_id),
            )
        };
        ClientConnectionBus {
            connection_table,
            stdout_channel_tx,
            control_channel_tx,
            web_client_id,
        }
    }
}

#[derive(Clone)]
pub struct AppState {
    pub connection_table: Arc<Mutex<ConnectionTable>>,
    pub config: Arc<Mutex<Config>>,
    pub config_options: Options,
    pub config_file_path: PathBuf,
    pub session_manager: Arc<dyn SessionManager>,
    pub client_os_api_factory: Arc<dyn ClientOsApiFactory>,
    pub is_https: bool,
}

#[derive(Serialize)]
pub struct CreateClientIdResponse {
    pub web_client_id: String,
    pub is_read_only: bool,
}

#[derive(Deserialize)]
pub struct TerminalParams {
    pub web_client_id: String,
}

#[derive(Deserialize)]
pub struct LoginRequest {
    pub auth_token: String,
    pub remember_me: Option<bool>,
}

#[derive(Serialize)]
pub struct LoginResponse {
    pub success: bool,
    pub message: String,
}

pub const BRACKETED_PASTE_START: [u8; 6] = [27, 91, 50, 48, 48, 126]; // \u{1b}[200~
pub const BRACKETED_PASTE_END: [u8; 6] = [27, 91, 50, 48, 49, 126]; // \u{1b}[201~
