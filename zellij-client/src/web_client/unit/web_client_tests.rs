use super::serve_web_client;
use super::*;
use futures_util::{SinkExt, StreamExt};
use isahc::prelude::*;
use serde_json;
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::time::timeout;
use tokio_tungstenite::tungstenite::http::Request;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use zellij_utils::input::layout::Layout;
use zellij_utils::{consts::VERSION, input::config::Config, input::options::Options};

use crate::os_input_output::ClientOsApi;
use crate::web_client::control_message::{
    WebClientToWebServerControlMessage, WebClientToWebServerControlMessagePayload,
    WebServerToWebClientControlMessage,
};
use crate::web_client::ClientOsApiFactory;
use zellij_utils::{
    data::{LayoutInfo, Palette},
    errors::ErrorContext,
    ipc::{ClientAttributes, ClientToServerMsg, ServerToClientMsg},
    pane_size::Size,
    web_authentication_tokens::{create_token, delete_db, revoke_token},
};

use serial_test::serial;

mod web_client_tests {
    use super::*;

    #[tokio::test]
    #[serial]
    async fn test_version_endpoint() {
        let _ = delete_db();

        let session_manager = Arc::new(MockSessionManager::new());
        let client_os_api_factory = Arc::new(MockClientOsApiFactory::new());

        let config = Config::default();
        let options = Options::default();

        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let port = addr.port();

        let server_handle = tokio::spawn(async move {
            serve_web_client(
                config,
                options,
                None,
                listener,
                None,
                Some(session_manager),
                Some(client_os_api_factory),
            )
            .await;
        });

        tokio::time::sleep(Duration::from_millis(100)).await;

        let url = format!("http://127.0.0.1:{}/info/version", port);

        let mut response = timeout(
            Duration::from_secs(5),
            tokio::task::spawn_blocking(move || isahc::get(&url)),
        )
        .await
        .expect("Request timed out")
        .expect("Spawn blocking failed")
        .expect("Request failed");

        assert!(response.status().is_success());

        let version_text = response.text().expect("Failed to read response body");
        assert_eq!(version_text, VERSION);

        server_handle.abort();
    }

    #[tokio::test]
    #[serial]
    async fn test_login_endpoint() {
        let _ = delete_db();

        let test_token_name = "test_token_login";
        let (auth_token, _) =
            create_token(Some(test_token_name.to_string())).expect("Failed to create test token");

        let session_manager = Arc::new(MockSessionManager::new());
        let client_os_api_factory = Arc::new(MockClientOsApiFactory::new());

        let config = Config::default();
        let options = Options::default();

        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let port = addr.port();

        let server_handle = tokio::spawn(async move {
            serve_web_client(
                config,
                options,
                None,
                listener,
                None,
                Some(session_manager),
                Some(client_os_api_factory),
            )
            .await;
        });

        tokio::time::sleep(Duration::from_millis(100)).await;

        let login_url = format!("http://127.0.0.1:{}/command/login", port);
        let login_payload = serde_json::json!({
            "auth_token": auth_token,
            "remember_me": true
        });

        let mut response = timeout(
            Duration::from_secs(5),
            tokio::task::spawn_blocking(move || {
                isahc::Request::post(&login_url)
                    .header("Content-Type", "application/json")
                    .body(login_payload.to_string())
                    .unwrap()
                    .send()
            }),
        )
        .await
        .expect("Login request timed out")
        .expect("Spawn blocking failed")
        .expect("Login request failed");

        assert!(response.status().is_success());

        let response_text = response.text().expect("Failed to read response body");
        let response_json: serde_json::Value =
            serde_json::from_str(&response_text).expect("Failed to parse JSON");

        assert_eq!(response_json["success"], true);
        assert_eq!(response_json["message"], "Login successful");

        println!("✓ Login endpoint test passed");

        server_handle.abort();
        revoke_token(test_token_name).expect("Failed to revoke test token");
    }

    #[tokio::test]
    #[serial]
    async fn test_invalid_auth_token_login() {
        let _ = delete_db();

        let session_manager = Arc::new(MockSessionManager::new());
        let client_os_api_factory = Arc::new(MockClientOsApiFactory::new());

        let config = Config::default();
        let options = Options::default();

        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let port = addr.port();

        let server_handle = tokio::spawn(async move {
            serve_web_client(
                config,
                options,
                None,
                listener,
                None,
                Some(session_manager),
                Some(client_os_api_factory),
            )
            .await;
        });

        tokio::time::sleep(Duration::from_millis(100)).await;

        let login_url = format!("http://127.0.0.1:{}/command/login", port);
        let login_payload = serde_json::json!({
            "auth_token": "invalid_token_123",
            "remember_me": false
        });

        let response = timeout(
            Duration::from_secs(5),
            tokio::task::spawn_blocking(move || {
                isahc::Request::post(&login_url)
                    .header("Content-Type", "application/json")
                    .body(login_payload.to_string())
                    .unwrap()
                    .send()
            }),
        )
        .await
        .expect("Login request timed out")
        .expect("Spawn blocking failed")
        .expect("Login request failed");

        assert_eq!(response.status(), 401);
        println!("✓ Invalid auth token correctly rejected");

        server_handle.abort();
    }

    #[tokio::test]
    #[serial]
    async fn test_full_session_flow() {
        let _ = delete_db();

        let test_token_name = "test_token_session_flow";
        let (auth_token, _) =
            create_token(Some(test_token_name.to_string())).expect("Failed to create test token");

        let session_manager = Arc::new(MockSessionManager::new());
        let client_os_api_factory = Arc::new(MockClientOsApiFactory::new());
        let factory_for_verification = client_os_api_factory.clone();

        let config = Config::default();
        let options = Options::default();

        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let port = addr.port();

        let server_handle = tokio::spawn(async move {
            serve_web_client(
                config,
                options,
                None,
                listener,
                None,
                Some(session_manager),
                Some(client_os_api_factory),
            )
            .await;
        });

        tokio::time::sleep(Duration::from_millis(100)).await;

        let login_url = format!("http://127.0.0.1:{}/command/login", port);
        let login_payload = serde_json::json!({
            "auth_token": auth_token,
            "remember_me": true
        });

        let login_response = timeout(
            Duration::from_secs(5),
            tokio::task::spawn_blocking(move || {
                isahc::Request::post(&login_url)
                    .header("Content-Type", "application/json")
                    .body(login_payload.to_string())
                    .unwrap()
                    .send()
            }),
        )
        .await
        .unwrap()
        .unwrap()
        .unwrap();

        assert!(login_response.status().is_success());

        let set_cookie_header = login_response.headers().get("set-cookie");
        assert!(
            set_cookie_header.is_some(),
            "Should have received session cookie"
        );
        let cookie_value = set_cookie_header.unwrap().to_str().unwrap();
        let session_token = cookie_value
            .split(';')
            .next()
            .and_then(|part| part.split('=').nth(1))
            .unwrap();

        println!("✓ Successfully logged in and received session token");

        let session_url = format!("http://127.0.0.1:{}/session", port);
        let mut client_response = timeout(
            Duration::from_secs(5),
            tokio::task::spawn_blocking({
                let session_token = session_token.to_string();
                move || {
                    isahc::Request::post(&session_url)
                        .header("Cookie", format!("session_token={}", session_token))
                        .header("Content-Type", "application/json")
                        .body("{}")
                        .unwrap()
                        .send()
                }
            }),
        )
        .await
        .unwrap()
        .unwrap()
        .unwrap();

        assert!(client_response.status().is_success());

        let client_data: serde_json::Value =
            serde_json::from_str(&client_response.text().unwrap()).unwrap();
        let web_client_id = client_data["web_client_id"].as_str().unwrap().to_string();

        println!("✓ Successfully created client session");

        let control_ws_url = format!("ws://127.0.0.1:{}/ws/control", port);
        let (control_ws, _) = timeout(
            Duration::from_secs(5),
            connect_async_with_cookie(&control_ws_url, session_token),
        )
        .await
        .expect("Control WebSocket connection timed out")
        .expect("Failed to connect to control WebSocket");

        let (mut control_sink, mut control_stream) = control_ws.split();

        let control_message = timeout(Duration::from_secs(2), control_stream.next())
            .await
            .expect("Timeout waiting for control message")
            .expect("Control stream ended")
            .expect("Error receiving control message");

        if let Message::Text(text) = control_message {
            let parsed: WebServerToWebClientControlMessage =
                serde_json::from_str(&text).expect("Failed to parse control message");

            match parsed {
                WebServerToWebClientControlMessage::SetConfig(_) => {
                    println!("✓ Received expected SetConfig message");
                },
                _ => panic!("Expected SetConfig message, got: {:?}", parsed),
            }
        } else {
            panic!("Expected text message, got: {:?}", control_message);
        }

        let resize_msg = WebClientToWebServerControlMessage {
            web_client_id: web_client_id.clone(),
            payload: WebClientToWebServerControlMessagePayload::TerminalResize(Size {
                rows: 30,
                cols: 100,
            }),
        };

        control_sink
            .send(Message::Text(serde_json::to_string(&resize_msg).unwrap()))
            .await
            .expect("Failed to send resize message");

        println!("✓ Sent terminal resize message");

        let terminal_ws_url = format!(
            "ws://127.0.0.1:{}/ws/terminal?web_client_id={}",
            port, web_client_id
        );
        let (terminal_ws, _) = timeout(
            Duration::from_secs(5),
            connect_async_with_cookie(&terminal_ws_url, session_token),
        )
        .await
        .expect("Terminal WebSocket connection timed out")
        .expect("Failed to connect to terminal WebSocket");

        let (mut terminal_sink, _terminal_stream) = terminal_ws.split();

        terminal_sink
            .send(Message::Text("echo hello\n".to_string()))
            .await
            .expect("Failed to send terminal input");

        println!("✓ Sent terminal input");

        tokio::time::sleep(Duration::from_millis(500)).await;

        let mock_apis = factory_for_verification.mock_apis.lock().unwrap();
        let mut found_resize = false;
        let mut found_terminal_input = false;

        for (_, mock_api) in mock_apis.iter() {
            let messages = mock_api.get_sent_messages();
            for msg in messages {
                match msg {
                    ClientToServerMsg::TerminalResize(_) => {
                        found_resize = true;
                    },
                    ClientToServerMsg::Key(_, _, _) | ClientToServerMsg::Action(_, _, _) => {
                        found_terminal_input = true;
                    },
                    _ => {},
                }
            }
        }

        assert!(
            found_resize,
            "Terminal resize message was not received by mock OS API"
        );
        println!("✓ Verified terminal resize message was processed by mock OS API");

        assert!(
            found_terminal_input,
            "Terminal input message was not received by mock OS API"
        );
        println!("✓ Verified terminal input message was processed by mock OS API");

        let _ = control_sink.close().await;
        let _ = terminal_sink.close().await;
        server_handle.abort();

        revoke_token(test_token_name).expect("Failed to revoke test token");
        println!("✓ Full session flow test completed successfully");
    }

    #[tokio::test]
    #[serial]
    async fn test_unauthorized_access_without_session() {
        let _ = delete_db();

        let session_manager = Arc::new(MockSessionManager::new());
        let client_os_api_factory = Arc::new(MockClientOsApiFactory::new());

        let config = Config::default();
        let options = Options::default();

        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let port = addr.port();

        let server_handle = tokio::spawn(async move {
            serve_web_client(
                config,
                options,
                None,
                listener,
                None,
                Some(session_manager),
                Some(client_os_api_factory),
            )
            .await;
        });

        tokio::time::sleep(Duration::from_millis(100)).await;

        let session_url = format!("http://127.0.0.1:{}/session", port);
        let response = timeout(
            Duration::from_secs(5),
            tokio::task::spawn_blocking(move || isahc::post(&session_url, "{}")),
        )
        .await
        .expect("Session request timed out")
        .expect("Spawn blocking failed")
        .expect("Session request failed");

        assert_eq!(response.status(), 401);
        println!("✓ Unauthorized access correctly rejected");

        server_handle.abort();
    }

    #[tokio::test]
    #[serial]
    async fn test_invalid_session_token() {
        let _ = delete_db();

        let session_manager = Arc::new(MockSessionManager::new());
        let client_os_api_factory = Arc::new(MockClientOsApiFactory::new());

        let config = Config::default();
        let options = Options::default();

        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let port = addr.port();

        let server_handle = tokio::spawn(async move {
            serve_web_client(
                config,
                options,
                None,
                listener,
                None,
                Some(session_manager),
                Some(client_os_api_factory),
            )
            .await;
        });

        tokio::time::sleep(Duration::from_millis(100)).await;

        let session_url = format!("http://127.0.0.1:{}/session", port);
        let response = timeout(
            Duration::from_secs(5),
            tokio::task::spawn_blocking(move || {
                isahc::Request::post(&session_url)
                    .header("Cookie", "session_token=invalid_session_token_123")
                    .header("Content-Type", "application/json")
                    .body("{}")
                    .unwrap()
                    .send()
            }),
        )
        .await
        .expect("Session request timed out")
        .expect("Spawn blocking failed")
        .expect("Session request failed");

        assert_eq!(response.status(), 401);
        println!("✓ Invalid session token correctly rejected");

        server_handle.abort();
    }

    async fn connect_async_with_cookie(
        url: &str,
        session_token: &str,
    ) -> Result<
        (
            tokio_tungstenite::WebSocketStream<
                tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
            >,
            tokio_tungstenite::tungstenite::handshake::client::Response,
        ),
        tokio_tungstenite::tungstenite::Error,
    > {
        // Manually construct WebSocket request with required headers since we need to add a custom cookie.
        // When building the request manually, we must include all the standard WebSocket handshake headers
        // that would normally be added automatically by the WebSocket client library.
        let request = Request::builder()
            .uri(url)
            .header("Cookie", format!("session_token={}", session_token))
            .header("Connection", "Upgrade")
            .header("Upgrade", "websocket")
            .header("Sec-WebSocket-Version", "13")
            .header("Sec-WebSocket-Key", "dGhlIHNhbXBsZSBub25jZQ==") // Standard test key
            .header("Host", "127.0.0.1")
            .body(())
            .unwrap();
        connect_async(request).await
    }
}

#[derive(Debug, Clone)]
pub struct MockSessionManager {
    pub mock_sessions: HashMap<String, bool>,
    pub mock_layouts: HashMap<String, Layout>,
}

impl MockSessionManager {
    pub fn new() -> Self {
        Self {
            mock_sessions: HashMap::new(),
            mock_layouts: HashMap::new(),
        }
    }
}

#[cfg(test)]
impl SessionManager for MockSessionManager {
    fn session_exists(&self, session_name: &str) -> Result<bool, Box<dyn std::error::Error>> {
        Ok(self
            .mock_sessions
            .get(session_name)
            .copied()
            .unwrap_or(false))
    }

    fn get_resurrection_layout(&self, session_name: &str) -> Option<Layout> {
        self.mock_layouts.get(session_name).cloned()
    }

    fn spawn_session_if_needed(
        &self,
        session_name: &str,
        _path: String,
        client_attributes: ClientAttributes,
        config: &Config,
        config_options: &Options,
        is_web_client: bool,
        _os_input: Box<dyn ClientOsApi>,
        _requested_layout: Option<LayoutInfo>,
    ) -> (ClientToServerMsg, PathBuf) {
        let mock_ipc_path = PathBuf::from(format!("/tmp/mock_zellij_{}", session_name));

        let first_message = ClientToServerMsg::AttachClient(
            client_attributes,
            config.clone(),
            config_options.clone(),
            None,
            None,
            is_web_client,
        );

        (first_message, mock_ipc_path)
    }
}

#[derive(Debug, Clone)]
struct MockClientOsApiFactory {
    mock_apis: Arc<Mutex<HashMap<String, Arc<MockClientOsApi>>>>,
}

impl MockClientOsApiFactory {
    fn new() -> Self {
        Self {
            mock_apis: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

impl ClientOsApiFactory for MockClientOsApiFactory {
    fn create_client_os_api(&self) -> Result<Box<dyn ClientOsApi>, Box<dyn std::error::Error>> {
        let mock_api = Arc::new(MockClientOsApi::new());

        let client_id = uuid::Uuid::new_v4().to_string();
        self.mock_apis
            .lock()
            .unwrap()
            .insert(client_id, mock_api.clone());

        Ok(Box::new((*mock_api).clone()))
    }
}

#[derive(Debug, Clone)]
struct MockClientOsApi {
    terminal_size: Size,
    messages_to_server: Arc<Mutex<Vec<ClientToServerMsg>>>,
    messages_from_server: Arc<Mutex<VecDeque<(ServerToClientMsg, ErrorContext)>>>,
}

impl MockClientOsApi {
    fn new() -> Self {
        Self {
            terminal_size: Size { rows: 24, cols: 80 },
            messages_to_server: Arc::new(Mutex::new(Vec::new())),
            messages_from_server: Arc::new(Mutex::new(VecDeque::new())),
        }
    }

    fn get_sent_messages(&self) -> Vec<ClientToServerMsg> {
        self.messages_to_server.lock().unwrap().clone()
    }
}

impl ClientOsApi for MockClientOsApi {
    fn get_terminal_size_using_fd(&self, _fd: std::os::unix::io::RawFd) -> Size {
        self.terminal_size
    }
    fn set_raw_mode(&mut self, _fd: std::os::unix::io::RawFd) {}
    fn unset_raw_mode(&self, _fd: std::os::unix::io::RawFd) -> Result<(), nix::Error> {
        Ok(())
    }
    fn get_stdout_writer(&self) -> Box<dyn std::io::Write> {
        Box::new(std::io::sink())
    }
    fn get_stdin_reader(&self) -> Box<dyn std::io::BufRead> {
        Box::new(std::io::Cursor::new(Vec::new()))
    }
    fn update_session_name(&mut self, _new_session_name: String) {}
    fn read_from_stdin(&mut self) -> Result<Vec<u8>, &'static str> {
        Ok(Vec::new())
    }
    fn box_clone(&self) -> Box<dyn ClientOsApi> {
        Box::new(self.clone())
    }
    fn send_to_server(&self, msg: ClientToServerMsg) {
        self.messages_to_server.lock().unwrap().push(msg);
    }
    fn recv_from_server(&self) -> Option<(ServerToClientMsg, ErrorContext)> {
        self.messages_from_server.lock().unwrap().pop_front()
    }
    fn handle_signals(&self, _sigwinch_cb: Box<dyn Fn()>, _quit_cb: Box<dyn Fn()>) {}
    fn connect_to_server(&self, _path: &std::path::Path) {}
    fn load_palette(&self) -> Palette {
        Palette::default()
    }
    fn enable_mouse(&self) -> anyhow::Result<()> {
        Ok(())
    }
    fn disable_mouse(&self) -> anyhow::Result<()> {
        Ok(())
    }
    fn stdin_poller(&self) -> crate::os_input_output::StdinPoller {
        crate::os_input_output::StdinPoller::default()
    }
}
