use super::super::*;
use crate::RemoteClientError;
use serial_test::serial;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use zellij_utils::remote_session_tokens;

// Mock server infrastructure
#[cfg(feature = "web_server_capability")]
mod mock_server {
    use super::*;
    use axum::{
        extract::State,
        http::StatusCode,
        response::Response,
        routing::{get, post},
        Json, Router,
    };
    use axum_extra::extract::cookie::{Cookie, CookieJar};
    use serde::Deserialize;
    use serde_json::json;
    use tokio::net::TcpListener;
    use uuid::Uuid;

    #[derive(Clone)]
    pub struct MockRemoteServerState {
        pub valid_auth_tokens: Arc<Mutex<HashMap<String, ()>>>,
        pub session_tokens: Arc<Mutex<HashMap<String, String>>>, // token -> web_client_id
        pub endpoints_called: Arc<Mutex<Vec<String>>>,
    }

    impl MockRemoteServerState {
        pub fn new() -> Self {
            Self {
                valid_auth_tokens: Arc::new(Mutex::new(HashMap::new())),
                session_tokens: Arc::new(Mutex::new(HashMap::new())),
                endpoints_called: Arc::new(Mutex::new(Vec::new())),
            }
        }

        pub fn add_valid_token(&self, token: &str) {
            self.valid_auth_tokens
                .lock()
                .unwrap()
                .insert(token.to_string(), ());
        }

        fn record_endpoint(&self, endpoint: &str) {
            self.endpoints_called
                .lock()
                .unwrap()
                .push(endpoint.to_string());
        }

        pub fn get_endpoints_called(&self) -> Vec<String> {
            self.endpoints_called.lock().unwrap().clone()
        }
    }

    #[derive(Deserialize)]
    struct LoginRequest {
        auth_token: String,
    }

    async fn handle_login(
        State(state): State<MockRemoteServerState>,
        jar: CookieJar,
        Json(payload): Json<LoginRequest>,
    ) -> Result<(CookieJar, Json<serde_json::Value>), StatusCode> {
        state.record_endpoint("/command/login");

        let valid_tokens = state.valid_auth_tokens.lock().unwrap();
        if !valid_tokens.contains_key(&payload.auth_token) {
            return Err(StatusCode::UNAUTHORIZED);
        }
        drop(valid_tokens);

        // Always create a session token (cookie is always set)
        let session_token = Uuid::new_v4().to_string();
        let web_client_id = Uuid::new_v4().to_string();

        state
            .session_tokens
            .lock()
            .unwrap()
            .insert(session_token.clone(), web_client_id);

        let cookie = Cookie::build(("session_token", session_token))
            .path("/")
            .http_only(true)
            .build();
        let jar = jar.add(cookie);

        Ok((
            jar,
            Json(json!({
                "success": true,
                "message": "Login successful"
            })),
        ))
    }

    async fn handle_session(
        State(state): State<MockRemoteServerState>,
        jar: CookieJar,
    ) -> Result<Json<serde_json::Value>, StatusCode> {
        state.record_endpoint("/session");

        let session_token = jar
            .get("session_token")
            .map(|c| c.value().to_string())
            .ok_or(StatusCode::UNAUTHORIZED)?;

        let session_tokens = state.session_tokens.lock().unwrap();
        let web_client_id = session_tokens
            .get(&session_token)
            .ok_or(StatusCode::UNAUTHORIZED)?
            .clone();
        drop(session_tokens);

        Ok(Json(json!({
            "web_client_id": web_client_id
        })))
    }

    async fn handle_ws_terminal(
        ws: axum::extract::ws::WebSocketUpgrade,
        State(state): State<MockRemoteServerState>,
        jar: CookieJar,
    ) -> Result<Response, StatusCode> {
        state.record_endpoint("/ws/terminal");

        // Validate session token
        let session_token = jar
            .get("session_token")
            .map(|c| c.value().to_string())
            .ok_or(StatusCode::UNAUTHORIZED)?;

        let session_tokens = state.session_tokens.lock().unwrap();
        if !session_tokens.contains_key(&session_token) {
            return Err(StatusCode::UNAUTHORIZED);
        }
        drop(session_tokens);

        Ok(ws.on_upgrade(|socket| async move {
            // Basic echo WebSocket handler
            use axum::extract::ws::Message;
            use futures_util::{SinkExt, StreamExt};
            let (mut sender, mut receiver) = socket.split();

            while let Some(Ok(msg)) = receiver.next().await {
                if let Message::Text(text) = msg {
                    let _ = sender.send(Message::Text(text)).await;
                }
            }
        }))
    }

    async fn handle_ws_control(
        ws: axum::extract::ws::WebSocketUpgrade,
        State(state): State<MockRemoteServerState>,
        jar: CookieJar,
    ) -> Result<Response, StatusCode> {
        state.record_endpoint("/ws/control");

        // Validate session token
        let session_token = jar
            .get("session_token")
            .map(|c| c.value().to_string())
            .ok_or(StatusCode::UNAUTHORIZED)?;

        let session_tokens = state.session_tokens.lock().unwrap();
        if !session_tokens.contains_key(&session_token) {
            return Err(StatusCode::UNAUTHORIZED);
        }
        drop(session_tokens);

        Ok(ws.on_upgrade(|socket| async move {
            // Basic echo WebSocket handler
            use axum::extract::ws::Message;
            use futures_util::{SinkExt, StreamExt};
            let (mut sender, mut receiver) = socket.split();

            while let Some(Ok(msg)) = receiver.next().await {
                if let Message::Text(text) = msg {
                    let _ = sender.send(Message::Text(text)).await;
                }
            }
        }))
    }

    pub async fn start_mock_server(
        state: MockRemoteServerState,
    ) -> (u16, tokio::task::JoinHandle<()>) {
        let app = Router::new()
            .route("/command/login", post(handle_login))
            .route("/session", post(handle_session))
            .route("/ws/terminal", get(handle_ws_terminal))
            .route("/ws/terminal/{session_name}", get(handle_ws_terminal))
            .route("/ws/control", get(handle_ws_control))
            .with_state(state);

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();

        let server_handle = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        // Wait for server to be ready
        tokio::time::sleep(Duration::from_millis(100)).await;

        (port, server_handle)
    }
}

// Database test helpers
fn setup_test_db(server_url: &str) {
    let _ = remote_session_tokens::delete_session_token(server_url);
}

fn cleanup_test_db(server_url: &str) {
    let _ = remote_session_tokens::delete_session_token(server_url);
}

// Mock ClientOsApi for testing
#[derive(Debug, Clone)]
struct MockClientOsApi;

impl crate::os_input_output::ClientOsApi for MockClientOsApi {
    fn get_terminal_size_using_fd(&self, _fd: i32) -> zellij_utils::pane_size::Size {
        zellij_utils::pane_size::Size { rows: 24, cols: 80 }
    }

    fn set_raw_mode(&mut self, _fd: i32) {}

    fn unset_raw_mode(&self, _fd: i32) -> Result<(), nix::Error> {
        Ok(())
    }

    fn box_clone(&self) -> Box<dyn crate::os_input_output::ClientOsApi> {
        Box::new(MockClientOsApi)
    }

    fn read_from_stdin(&mut self) -> Result<Vec<u8>, &'static str> {
        Ok(Vec::new())
    }

    fn get_stdin_reader(&self) -> Box<dyn std::io::BufRead> {
        Box::new(std::io::BufReader::new(std::io::empty()))
    }

    fn get_stdout_writer(&self) -> Box<dyn std::io::Write> {
        Box::new(std::io::sink())
    }

    fn update_session_name(&mut self, _new_session_name: String) {}

    fn send_to_server(&self, _msg: zellij_utils::ipc::ClientToServerMsg) {}

    fn recv_from_server(
        &self,
    ) -> Option<(
        zellij_utils::ipc::ServerToClientMsg,
        zellij_utils::errors::ErrorContext,
    )> {
        None
    }

    fn handle_signals(&self, _sigwinch_cb: Box<dyn Fn()>, _quit_cb: Box<dyn Fn()>) {}

    fn connect_to_server(&self, _path: &std::path::Path) {}

    fn load_palette(&self) -> zellij_utils::data::Palette {
        zellij_utils::shared::default_palette()
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

// Tests
#[cfg(feature = "web_server_capability")]
mod tests {
    use super::mock_server::*;
    use super::*;

    // Helper function to call attach_to_remote_session from async context
    async fn call_attach_to_remote_session(
        remote_session_url: String,
        token: Option<String>,
        remember: bool,
        forget: bool,
    ) -> Result<WebSocketConnections, RemoteClientError> {
        tokio::task::spawn_blocking(move || {
            let runtime = tokio::runtime::Runtime::new().unwrap();
            let os_input: Box<dyn crate::os_input_output::ClientOsApi> = Box::new(MockClientOsApi);
            attach_to_remote_session(
                &runtime,
                os_input,
                &remote_session_url,
                token,
                remember,
                forget,
            )
        })
        .await
        .unwrap()
    }

    #[tokio::test]
    #[serial]
    async fn test_successful_authentication_with_valid_token() {
        let server_state = MockRemoteServerState::new();
        let auth_token = "test-auth-token-123";
        server_state.add_valid_token(auth_token);

        let (port, server_handle) = start_mock_server(server_state.clone()).await;
        let server_url = format!("http://127.0.0.1:{}/session-name", port);

        setup_test_db(&format!("http://127.0.0.1:{}", port));

        let result =
            call_attach_to_remote_session(server_url, Some(auth_token.to_string()), false, false)
                .await;

        assert!(
            result.is_ok(),
            "Should successfully authenticate: {:?}",
            result.err()
        );

        let endpoints = server_state.get_endpoints_called();
        assert!(
            endpoints.contains(&"/command/login".to_string()),
            "Should call login endpoint"
        );
        assert!(
            endpoints.contains(&"/session".to_string()),
            "Should call session endpoint"
        );
        assert!(
            endpoints.contains(&"/ws/terminal".to_string()),
            "Should establish terminal WebSocket"
        );
        assert!(
            endpoints.contains(&"/ws/control".to_string()),
            "Should establish control WebSocket"
        );

        server_handle.abort();
        cleanup_test_db(&format!("http://127.0.0.1:{}", port));
    }

    #[tokio::test]
    #[serial]
    async fn test_failed_authentication_with_invalid_token() {
        let server_state = MockRemoteServerState::new();
        // Don't add the token to valid tokens - server will reject it

        let (port, server_handle) = start_mock_server(server_state.clone()).await;
        let server_url = format!("http://127.0.0.1:{}/session-name", port);

        setup_test_db(&format!("http://127.0.0.1:{}", port));

        let result = call_attach_to_remote_session(
            server_url,
            Some("invalid-token".to_string()),
            false,
            false,
        )
        .await;

        assert!(result.is_err(), "Should fail with invalid token");
        assert!(
            matches!(result.unwrap_err(), RemoteClientError::InvalidAuthToken),
            "Should return InvalidAuthToken error"
        );

        server_handle.abort();
        cleanup_test_db(&format!("http://127.0.0.1:{}", port));
    }

    #[tokio::test]
    #[serial]
    async fn test_save_session_token_with_remember_true() {
        let server_state = MockRemoteServerState::new();
        let auth_token = "test-token-remember";
        server_state.add_valid_token(auth_token);

        let (port, server_handle) = start_mock_server(server_state.clone()).await;
        let server_url = format!("http://127.0.0.1:{}/session-name", port);
        let base_url = format!("http://127.0.0.1:{}", port);

        setup_test_db(&base_url);

        let result = call_attach_to_remote_session(
            server_url,
            Some(auth_token.to_string()),
            true, // remember = true
            false,
        )
        .await;

        assert!(result.is_ok(), "Connection should succeed");

        // Verify token was saved
        let saved_token = remote_session_tokens::get_session_token(&base_url);
        assert!(saved_token.is_ok());
        assert!(
            saved_token.unwrap().is_some(),
            "Session token should be saved"
        );

        server_handle.abort();
        cleanup_test_db(&base_url);
    }

    #[tokio::test]
    #[serial]
    async fn test_dont_save_token_with_remember_false() {
        let server_state = MockRemoteServerState::new();
        let auth_token = "test-token-no-remember";
        server_state.add_valid_token(auth_token);

        let (port, server_handle) = start_mock_server(server_state.clone()).await;
        let server_url = format!("http://127.0.0.1:{}/session-name", port);
        let base_url = format!("http://127.0.0.1:{}", port);

        setup_test_db(&base_url);

        let result = call_attach_to_remote_session(
            server_url,
            Some(auth_token.to_string()),
            false, // remember = false
            false,
        )
        .await;

        assert!(result.is_ok(), "Connection should succeed");

        // Verify token was NOT saved
        let saved_token = remote_session_tokens::get_session_token(&base_url);
        assert!(saved_token.is_ok());
        assert!(
            saved_token.unwrap().is_none(),
            "Session token should NOT be saved"
        );

        server_handle.abort();
        cleanup_test_db(&base_url);
    }

    #[tokio::test]
    #[serial]
    async fn test_load_and_use_saved_session_token() {
        let server_state = MockRemoteServerState::new();

        // Pre-create a session token
        let session_token = uuid::Uuid::new_v4().to_string();
        let web_client_id = uuid::Uuid::new_v4().to_string();
        server_state
            .session_tokens
            .lock()
            .unwrap()
            .insert(session_token.clone(), web_client_id);

        let (port, server_handle) = start_mock_server(server_state.clone()).await;
        let server_url = format!("http://127.0.0.1:{}/session-name", port);
        let base_url = format!("http://127.0.0.1:{}", port);

        setup_test_db(&base_url);

        // Save the session token
        remote_session_tokens::save_session_token(&base_url, &session_token).unwrap();

        let result = call_attach_to_remote_session(
            server_url, None, // No auth token provided
            false, false,
        )
        .await;

        assert!(result.is_ok(), "Should successfully use saved token");

        // Verify we did NOT call login endpoint (used saved token directly)
        let endpoints = server_state.get_endpoints_called();
        assert!(
            !endpoints.contains(&"/command/login".to_string()),
            "Should NOT call login endpoint"
        );
        assert!(
            endpoints.contains(&"/session".to_string()),
            "Should call session endpoint"
        );

        server_handle.abort();
        cleanup_test_db(&base_url);
    }

    #[tokio::test]
    #[serial]
    async fn test_token_flag_deletes_saved_token() {
        let server_state = MockRemoteServerState::new();
        let auth_token = "new-auth-token";
        server_state.add_valid_token(auth_token);

        let (port, server_handle) = start_mock_server(server_state.clone()).await;
        let server_url = format!("http://127.0.0.1:{}/session-name", port);
        let base_url = format!("http://127.0.0.1:{}", port);

        setup_test_db(&base_url);

        // Pre-save an old token
        remote_session_tokens::save_session_token(&base_url, "old-token").unwrap();

        let result = call_attach_to_remote_session(
            server_url,
            Some(auth_token.to_string()), // Providing new token
            false,
            false,
        )
        .await;

        assert!(result.is_ok(), "Should succeed with new token");

        // The old token should have been deleted before using new one
        // (New token won't be saved because remember=false)
        // Verify by checking that session endpoint was called (not using saved token)
        let endpoints = server_state.get_endpoints_called();
        assert!(
            endpoints.contains(&"/command/login".to_string()),
            "Should use new auth token, not saved token"
        );

        server_handle.abort();
        cleanup_test_db(&base_url);
    }

    #[tokio::test]
    #[serial]
    async fn test_successful_websocket_establishment() {
        let server_state = MockRemoteServerState::new();
        let auth_token = "test-ws-token";
        server_state.add_valid_token(auth_token);

        let (port, server_handle) = start_mock_server(server_state.clone()).await;
        let server_url = format!("http://127.0.0.1:{}/test-session", port);
        let base_url = format!("http://127.0.0.1:{}", port);

        setup_test_db(&base_url);

        let result =
            call_attach_to_remote_session(server_url, Some(auth_token.to_string()), false, false)
                .await;

        assert!(
            result.is_ok(),
            "WebSocket connections should be established"
        );

        let connections = result.unwrap();
        assert!(
            !connections.web_client_id.is_empty(),
            "Should have web_client_id"
        );

        // Verify both WebSocket endpoints were called
        let endpoints = server_state.get_endpoints_called();
        assert!(
            endpoints.contains(&"/ws/terminal".to_string()),
            "Terminal WebSocket should be established"
        );
        assert!(
            endpoints.contains(&"/ws/control".to_string()),
            "Control WebSocket should be established"
        );

        server_handle.abort();
        cleanup_test_db(&base_url);
    }

    #[tokio::test]
    async fn test_url_parsing_for_session_name() {
        // Test various URL formats
        let test_cases = vec![
            ("https://example.com/my-session", "my-session"),
            ("https://example.com/", ""),
            ("https://example.com/path/to/session", "path/to/session"),
            ("http://localhost:8080/test", "test"),
        ];

        for (url, expected_name) in test_cases {
            let result = extract_session_name(url);
            assert!(result.is_ok(), "Failed to parse URL: {}", url);
            assert_eq!(
                result.unwrap(),
                expected_name,
                "Wrong session name for URL: {}",
                url
            );
        }
    }

    #[tokio::test]
    async fn test_server_url_extraction() {
        // Test various URL formats
        let test_cases = vec![
            (
                "https://example.com:8080/session?foo=bar",
                "https://example.com:8080",
            ),
            ("http://localhost/test", "http://localhost"),
            (
                "https://example.com/path/to/session#anchor",
                "https://example.com",
            ),
        ];

        for (url, expected_base) in test_cases {
            let result = extract_server_url(url);
            assert!(result.is_ok(), "Failed to extract server URL: {}", url);
            assert_eq!(
                result.unwrap(),
                expected_base,
                "Wrong base URL for: {}",
                url
            );
        }
    }

    #[tokio::test]
    async fn test_invalid_url_format() {
        let result = call_attach_to_remote_session(
            "not-a-valid-url".to_string(),
            Some("token".to_string()),
            false,
            false,
        )
        .await;

        assert!(result.is_err(), "Should fail with malformed URL");
        assert!(matches!(
            result.unwrap_err(),
            RemoteClientError::UrlParseError(_)
        ));
    }
}

// Tests that don't require the web_server_capability feature
#[cfg(not(feature = "web_server_capability"))]
mod tests {
    use super::*;

    #[test]
    fn test_url_parsing_without_server() {
        // Basic URL parsing tests that don't require a server
        let result = extract_session_name("https://example.com/my-session");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "my-session");

        let result = extract_server_url("https://example.com:8080/session?foo=bar");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "https://example.com:8080");
    }
}
