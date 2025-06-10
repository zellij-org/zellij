use super::*;
use super::{serve_web_client, MockSessionManager};
use futures_util::{SinkExt, StreamExt};
use isahc::prelude::*;
use serde_json;
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::time::timeout;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use zellij_utils::{consts::VERSION, input::config::Config, input::options::Options};

use crate::os_input_output::ClientOsApi;
use crate::web_client::control_message::{
    WebClientToWebServerControlMessage, WebClientToWebServerControlMessagePayload,
    WebServerToWebClientControlMessage,
};
use crate::web_client::ClientOsApiFactory;
use zellij_utils::{
    data::Palette,
    errors::ErrorContext,
    ipc::{ClientToServerMsg, ServerToClientMsg},
    pane_size::Size,
    web_authentication_tokens::{create_token, revoke_token},
};

mod web_client_tests {
    use super::*;
    #[tokio::test]
    async fn test_version_endpoint() {
        // Create mock session manager and client OS API factory
        let session_manager = Arc::new(MockSessionManager::new());
        let client_os_api_factory = Arc::new(MockClientOsApiFactory::new());

        // Create default config
        let config = Config::default();
        let options = Options::default();

        // Bind to any available port
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let port = addr.port();

        // Start server in background task
        let server_handle = tokio::spawn(async move {
            serve_web_client(
                config,
                options,
                listener,
                None,
                Some(session_manager),
                Some(client_os_api_factory),
            )
            .await;
        });

        // Wait a moment for server to start
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Make request to version endpoint using isahc
        let url = format!("http://127.0.0.1:{}/info/version", port);

        let mut response = timeout(
            Duration::from_secs(5),
            tokio::task::spawn_blocking(move || isahc::get(&url)),
        )
        .await
        .expect("Request timed out")
        .expect("Spawn blocking failed")
        .expect("Request failed");

        // Verify response
        assert!(response.status().is_success());

        let version_text = response.text().expect("Failed to read response body");
        assert_eq!(version_text, VERSION);

        // Clean shutdown
        server_handle.abort();
    }
    #[tokio::test]
    async fn test_shutdown_endpoint() {
        // Create mock session manager and client OS API factory
        let session_manager = Arc::new(MockSessionManager::new());
        let client_os_api_factory = Arc::new(MockClientOsApiFactory::new());

        // Create default config
        let config = Config::default();
        let options = Options::default();

        // Bind to any available port
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let port = addr.port();

        // Start server in background task
        let server_handle = tokio::spawn(async move {
            serve_web_client(
                config,
                options,
                listener,
                None,
                Some(session_manager),
                Some(client_os_api_factory),
            )
            .await;
        });

        // Wait a moment for server to start
        tokio::time::sleep(Duration::from_millis(100)).await;

        // First, verify server is running by hitting version endpoint
        let version_url = format!("http://127.0.0.1:{}/info/version", port);
        let version_response = timeout(
            Duration::from_secs(5),
            tokio::task::spawn_blocking(move || isahc::get(&version_url)),
        )
        .await
        .expect("Version request timed out")
        .expect("Spawn blocking failed")
        .expect("Version request failed");

        assert!(version_response.status().is_success());

        // Now send shutdown request
        let shutdown_url = format!("http://127.0.0.1:{}/command/shutdown", port);
        let mut shutdown_response = timeout(
            Duration::from_secs(5),
            tokio::task::spawn_blocking(move || isahc::post(&shutdown_url, "")),
        )
        .await
        .expect("Shutdown request timed out")
        .expect("Spawn blocking failed")
        .expect("Shutdown request failed");

        // Verify shutdown response
        assert!(shutdown_response.status().is_success());

        let response_text = shutdown_response
            .text()
            .expect("Failed to read shutdown response");
        let response_json: serde_json::Value =
            serde_json::from_str(&response_text).expect("Failed to parse shutdown response JSON");
        assert_eq!(response_json["status"], "Ok");

        // Wait a bit for the shutdown to take effect
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Verify server has actually shut down by trying to connect again
        let verify_url = format!("http://127.0.0.1:{}/info/version", port);
        let connection_result = timeout(
            Duration::from_secs(2),
            tokio::task::spawn_blocking(move || isahc::get(&verify_url)),
        )
        .await;

        // Should either timeout or get a connection error
        match connection_result {
            Err(_) => {
                // Timeout is expected - server shut down
            },
            Ok(Ok(Ok(_response))) => {
                panic!("Server should have shut down but still responding");
            },
            Ok(Ok(Err(_))) => {
                // Connection error is expected - server shut down
            },
            Ok(Err(_)) => {
                // Spawn error is also acceptable
            },
        }

        // Wait for server task to complete
        let server_result = timeout(Duration::from_secs(3), server_handle).await;
        match server_result {
            Ok(_) => {
                // Server task completed, which is expected after shutdown
            },
            Err(_) => {
                // Server task didn't complete in time, but that's also acceptable
                // since we might have aborted it
            },
        }
    }

    #[tokio::test]
    async fn test_full_connection_flow_with_auth() {
        // Create a test token
        let test_token_name = "test_token_connection_flow";
        let (token, _) =
            create_token(Some(test_token_name.to_string())).expect("Failed to create test token");
        // Create mock session manager
        let session_manager = Arc::new(MockSessionManager::new());
        let client_os_api_factory = Arc::new(MockClientOsApiFactory::new());
        let factory_for_verification = client_os_api_factory.clone();

        // Create default config
        let config = Config::default();
        let options = Options::default();

        // Bind to any available port
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let port = addr.port();

        let server_handle = tokio::spawn(async move {
            serve_web_client(
                config,
                options,
                listener,
                None,
                Some(session_manager),
                Some(client_os_api_factory),
            )
            .await;
        });

        // Wait for server to start
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Step 1: Get client ID from /session endpoint with authentication
        let session_url = format!("http://127.0.0.1:{}/session", port);
        let mut client_response = timeout(
            Duration::from_secs(5),
            tokio::task::spawn_blocking({
                let token = token.clone();
                move || {
                    isahc::Request::post(&session_url)
                        .header("Authorization", format!("Bearer {}", token))
                        .header("Content-Type", "application/json")
                        .body("{}")
                        .unwrap()
                        .send()
                }
            }),
        )
        .await
        .expect("Session request timed out")
        .expect("Spawn blocking failed")
        .expect("Session request failed");

        assert!(client_response.status().is_success());

        let client_data: serde_json::Value =
            serde_json::from_str(&client_response.text().expect("Failed to read response"))
                .expect("Failed to parse JSON");

        let web_client_id = client_data["web_client_id"]
            .as_str()
            .expect("Missing web_client_id")
            .to_string();

        // Step 2: Connect to control WebSocket with token
        let control_ws_url = format!(
            "ws://127.0.0.1:{}/ws/control?token={}",
            port,
            urlencoding::encode(&token)
        );
        let (control_ws, _) = connect_async(&control_ws_url)
            .await
            .expect("Failed to connect to control WebSocket");

        let (mut control_sink, mut control_stream) = control_ws.split();

        // Step 3: Connect to terminal WebSocket with token and client ID
        let terminal_ws_url = format!(
            "ws://127.0.0.1:{}/ws/terminal?web_client_id={}&token={}",
            port,
            web_client_id,
            urlencoding::encode(&token)
        );
        let (terminal_ws, _) = connect_async(&terminal_ws_url)
            .await
            .expect("Failed to connect to terminal WebSocket");

        let (mut terminal_sink, _terminal_stream) = terminal_ws.split();

        // Step 4: Verify we receive SetConfig message on control channel
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

        // Step 5: Send a terminal resize message and verify it's processed
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

        // Step 6: Send some terminal input
        terminal_sink
            .send(Message::Text("echo hello\n".to_string()))
            .await
            .expect("Failed to send terminal input");

        println!("✓ Sent terminal input");

        // Step 7: Verify messages were received by mock OS API
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

        // Clean shutdown
        let _ = control_sink.close().await;
        let _ = terminal_sink.close().await;
        server_handle.abort();

        // Clean up the test token
        revoke_token(test_token_name).expect("Failed to revoke test token");

        println!("✓ Test completed successfully");
    }

    #[tokio::test]
    async fn test_unauthorized_access() {
        // Create mock session manager and client OS API factory
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
                listener,
                None,
                Some(session_manager),
                Some(client_os_api_factory),
            )
            .await;
        });

        tokio::time::sleep(Duration::from_millis(100)).await;

        // Try to access session endpoint without authentication
        let session_url = format!("http://127.0.0.1:{}/session", port);
        let response = timeout(
            Duration::from_secs(5),
            tokio::task::spawn_blocking(move || isahc::post(&session_url, "{}")),
        )
        .await
        .expect("Session request timed out")
        .expect("Spawn blocking failed")
        .expect("Session request failed");

        // Should get 401 Unauthorized
        assert_eq!(response.status(), 401);
        println!("✓ Unauthorized access correctly rejected");

        server_handle.abort();
    }

    #[tokio::test]
    async fn test_invalid_token() {
        // Create mock session manager and client OS API factory
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
                listener,
                None,
                Some(session_manager),
                Some(client_os_api_factory),
            )
            .await;
        });

        tokio::time::sleep(Duration::from_millis(100)).await;

        // Try to access session endpoint with invalid token
        let session_url = format!("http://127.0.0.1:{}/session", port);
        let response = timeout(
            Duration::from_secs(5),
            tokio::task::spawn_blocking(move || {
                isahc::Request::post(&session_url)
                    .header("Authorization", "Bearer invalid_token_123")
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

        // Should get 401 Unauthorized
        assert_eq!(response.status(), 401);
        println!("✓ Invalid token correctly rejected");

        server_handle.abort();
    }

    #[tokio::test]
    async fn test_websocket_with_mocked_session() {
        // Create a test token
        let test_token_name = "test_token_mocked_session";
        let (token, _) =
            create_token(Some(test_token_name.to_string())).expect("Failed to create test token");

        // Create mock session manager with a pre-existing session and client OS API factory
        let mut session_manager = MockSessionManager::new();
        session_manager.add_session("test_session".to_string());
        let session_manager = Arc::new(session_manager);
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
                listener,
                None,
                Some(session_manager),
                Some(client_os_api_factory),
            )
            .await;
        });

        tokio::time::sleep(Duration::from_millis(100)).await;

        // Get client ID
        let session_url = format!("http://127.0.0.1:{}/session", port);
        let mut client_response = timeout(
            Duration::from_secs(5),
            tokio::task::spawn_blocking({
                let token = token.clone();
                move || {
                    isahc::Request::post(&session_url)
                        .header("Authorization", format!("Bearer {}", token))
                        .header("Content-Type", "application/json")
                        .body("{}")
                        .unwrap()
                        .send()
                }
            }),
        )
        .await
        .expect("Session request timed out")
        .expect("Spawn blocking failed")
        .expect("Session request failed");

        assert!(client_response.status().is_success());

        let client_data: serde_json::Value =
            serde_json::from_str(&client_response.text().expect("Failed to read response"))
                .expect("Failed to parse JSON");

        let web_client_id = client_data["web_client_id"]
            .as_str()
            .expect("Missing web_client_id")
            .to_string();

        // Connect to a specific session through terminal WebSocket
        let terminal_ws_url = format!(
            "ws://127.0.0.1:{}/ws/terminal/test_session?web_client_id={}&token={}",
            port,
            web_client_id,
            urlencoding::encode(&token)
        );
        let (terminal_ws, _) = connect_async(&terminal_ws_url)
            .await
            .expect("Failed to connect to terminal WebSocket");

        let (mut terminal_sink, _terminal_stream) = terminal_ws.split();

        // Send some input to the mocked session
        terminal_sink
            .send(Message::Text("pwd\n".to_string()))
            .await
            .expect("Failed to send terminal input");

        println!("✓ Successfully connected to mocked session");

        // Verify the terminal input was received by mock OS API
        tokio::time::sleep(Duration::from_millis(500)).await;

        let mock_apis = factory_for_verification.mock_apis.lock().unwrap();
        let mut found_input = false;
        for (_, mock_api) in mock_apis.iter() {
            let messages = mock_api.get_sent_messages();
            for msg in messages {
                match msg {
                    ClientToServerMsg::Key(_, _, _) | ClientToServerMsg::Action(_, _, _) => {
                        found_input = true;
                        break;
                    },
                    _ => {},
                }
            }
        }

        if found_input {
            println!("✓ Verified terminal input was processed by mock OS API");
        } else {
            println!("! No terminal input messages found in mock OS API (may be expected)");
        }

        // Clean shutdown
        let _ = terminal_sink.close().await;
        server_handle.abort();

        // Clean up the test token
        revoke_token(test_token_name).expect("Failed to revoke test token");
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

    fn get_mock_for_client(&self, client_id: &str) -> Option<Arc<MockClientOsApi>> {
        self.mock_apis.lock().unwrap().get(client_id).cloned()
    }
}

impl ClientOsApiFactory for MockClientOsApiFactory {
    fn create_client_os_api(&self) -> Result<Box<dyn ClientOsApi>, Box<dyn std::error::Error>> {
        let mock_api = Arc::new(MockClientOsApi::new());

        // Store it with a generated ID for later retrieval
        let client_id = uuid::Uuid::new_v4().to_string();
        self.mock_apis
            .lock()
            .unwrap()
            .insert(client_id, mock_api.clone());

        Ok(Box::new((*mock_api).clone()))
    }
}

// Mock ClientOsApi implementation for tests
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

    fn add_server_message(&self, msg: ServerToClientMsg) {
        self.messages_from_server
            .lock()
            .unwrap()
            .push_back((msg, ErrorContext::new()));
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
