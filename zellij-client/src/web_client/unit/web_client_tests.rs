use super::*;
use super::serve_web_client;
use futures_util::{SinkExt, StreamExt};
use isahc::prelude::*;
use serde_json;
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::time::timeout;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use zellij_utils::{consts::VERSION, input::config::Config, input::options::Options};
use zellij_utils::input::layout::Layout;

use crate::os_input_output::ClientOsApi;
use crate::web_client::control_message::{
    WebClientToWebServerControlMessage, WebClientToWebServerControlMessagePayload,
    WebServerToWebClientControlMessage,
};
use crate::web_client::ClientOsApiFactory;
use zellij_utils::{
    data::{Palette, LayoutInfo},
    errors::ErrorContext,
    ipc::{ClientToServerMsg, ServerToClientMsg, ClientAttributes},
    pane_size::Size,
    web_authentication_tokens::{create_token, delete_db, revoke_token},
};

use serial_test::serial;

mod web_client_tests {
    use super::*;

    #[tokio::test]
    #[serial]
    async fn test_version_endpoint() {
        // clean up token state
        let _ = delete_db();

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
    #[serial]
    async fn test_shutdown_endpoint() {
        // clean up token state
        let _ = delete_db();

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
    #[serial]
    async fn test_full_connection_flow_with_auth() {
        // clean up token state
        let _ = delete_db();

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
    #[serial]
    async fn test_unauthorized_access() {
        // clean up token state
        let _ = delete_db();

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
    #[serial]
    async fn test_invalid_token() {
        // clean up token state
        let _ = delete_db();

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
    #[serial]
    async fn test_websocket_with_mocked_session() {
        // clean up token state
        let _ = delete_db();

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

    #[tokio::test]
    #[serial]
    async fn test_multiple_client_connections() {
        // clean up token state
        let _ = delete_db();

        let test_token_name = "test_token_multiple_clients";
        let (token, _) =
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
                listener,
                None,
                Some(session_manager),
                Some(client_os_api_factory),
            )
            .await;
        });

        tokio::time::sleep(Duration::from_millis(100)).await;

        // Create two separate clients
        let mut client_ids = Vec::new();
        for i in 0..2 {
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
            .unwrap()
            .unwrap()
            .unwrap();

            assert!(client_response.status().is_success());
            let client_data: serde_json::Value =
                serde_json::from_str(&client_response.text().unwrap()).unwrap();
            let web_client_id = client_data["web_client_id"].as_str().unwrap().to_string();
            client_ids.push(web_client_id);
            println!("✓ Created client {}: {}", i + 1, client_ids[i]);
        }

        // Connect both clients to terminal WebSockets
        let mut terminal_sinks = Vec::new();
        for (i, client_id) in client_ids.iter().enumerate() {
            let terminal_ws_url = format!(
                "ws://127.0.0.1:{}/ws/terminal?web_client_id={}&token={}",
                port,
                client_id,
                urlencoding::encode(&token)
            );
            let (terminal_ws, _) = connect_async(&terminal_ws_url).await.unwrap();
            let (terminal_sink, _) = terminal_ws.split();
            terminal_sinks.push(terminal_sink);
            println!("✓ Connected client {} to terminal WebSocket", i + 1);
        }

        // Send specific different inputs from each client
        let client_inputs = vec!["client_1_unique_command\n", "client_2_different_input\n"];
        for (i, sink) in terminal_sinks.iter_mut().enumerate() {
            sink.send(Message::Text(client_inputs[i].to_string()))
                .await
                .unwrap();
            println!("✓ Sent '{}' from client {}", client_inputs[i].trim(), i + 1);
        }

        tokio::time::sleep(Duration::from_millis(500)).await;

        // Verify each client's specific message was received by their respective mock OS API
        let mock_apis = factory_for_verification.mock_apis.lock().unwrap();
        assert_eq!(mock_apis.len(), 2, "Should have created 2 mock OS APIs");

        let expected_inputs = ["client_1_unique_command\n", "client_2_different_input\n"];
        let mut found_inputs = vec![false, false];

        for (_, mock_api) in mock_apis.iter() {
            let messages = mock_api.get_sent_messages();
            for msg in messages {
                if let ClientToServerMsg::Key(_, raw_bytes, _) = msg {
                    let input_str = String::from_utf8_lossy(&raw_bytes);
                    for (i, expected) in expected_inputs.iter().enumerate() {
                        if input_str.contains(*expected) {
                            found_inputs[i] = true;
                            println!(
                                "✓ Found expected input '{}' in mock API messages",
                                expected.trim()
                            );
                        }
                    }
                }
            }
        }

        assert!(
            found_inputs[0],
            "Should have found client 1's specific input: '{}'",
            expected_inputs[0].trim()
        );
        assert!(
            found_inputs[1],
            "Should have found client 2's specific input: '{}'",
            expected_inputs[1].trim()
        );
        println!(
            "✓ Verified both clients sent their specific inputs to their respective mock OS APIs"
        );

        for sink in terminal_sinks.iter_mut() {
            let _ = sink.close().await;
        }
        server_handle.abort();
        revoke_token(test_token_name).expect("Failed to revoke test token");
    }

    #[tokio::test]
    #[serial]
    async fn test_control_and_terminal_message_coordination() {
        // clean up token state
        let _ = delete_db();

        let test_token_name = "test_token_control_terminal";
        let (token, _) =
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
        .unwrap()
        .unwrap()
        .unwrap();

        let client_data: serde_json::Value =
            serde_json::from_str(&client_response.text().unwrap()).unwrap();
        let web_client_id = client_data["web_client_id"].as_str().unwrap().to_string();

        // Connect control WebSocket first
        let control_ws_url = format!(
            "ws://127.0.0.1:{}/ws/control?token={}",
            port,
            urlencoding::encode(&token)
        );
        let (control_ws, _) = connect_async(&control_ws_url).await.unwrap();
        let (mut control_sink, mut control_stream) = control_ws.split();

        // Receive initial SetConfig message
        let _ = timeout(Duration::from_secs(2), control_stream.next())
            .await
            .unwrap();

        // Connect terminal WebSocket
        let terminal_ws_url = format!(
            "ws://127.0.0.1:{}/ws/terminal?web_client_id={}&token={}",
            port,
            web_client_id,
            urlencoding::encode(&token)
        );
        let (terminal_ws, _) = connect_async(&terminal_ws_url).await.unwrap();
        let (mut terminal_sink, _) = terminal_ws.split();

        // Send specific resize through control channel
        let expected_size = Size {
            rows: 50,
            cols: 120,
        };
        let resize_msg = WebClientToWebServerControlMessage {
            web_client_id: web_client_id.clone(),
            payload: WebClientToWebServerControlMessagePayload::TerminalResize(expected_size),
        };
        control_sink
            .send(Message::Text(serde_json::to_string(&resize_msg).unwrap()))
            .await
            .unwrap();

        // Send specific input through terminal channel
        let expected_input = "ls -la specific test\n";
        terminal_sink
            .send(Message::Text(expected_input.to_string()))
            .await
            .unwrap();

        tokio::time::sleep(Duration::from_millis(500)).await;

        // Verify both specific messages were received with correct content
        let mock_apis = factory_for_verification.mock_apis.lock().unwrap();
        let messages: Vec<_> = mock_apis
            .iter()
            .flat_map(|(_, api)| api.get_sent_messages())
            .collect();

        // Check for specific resize message
        let found_resize = messages.iter().find_map(|msg| {
            if let ClientToServerMsg::TerminalResize(size) = msg {
                Some(size)
            } else {
                None
            }
        });
        assert!(
            found_resize.is_some(),
            "Should have received terminal resize message"
        );
        let actual_size = found_resize.unwrap();
        assert_eq!(
            actual_size.rows, expected_size.rows,
            "Resize message should have correct rows"
        );
        assert_eq!(
            actual_size.cols, expected_size.cols,
            "Resize message should have correct cols"
        );
        println!(
            "✓ Verified resize message with correct dimensions: {}x{}",
            actual_size.cols, actual_size.rows
        );

        // Check for specific input message
        let found_input = messages.iter().any(|msg| {
            if let ClientToServerMsg::Key(_, raw_bytes, _) = msg {
                let input_str = String::from_utf8_lossy(&raw_bytes);
                input_str.contains(expected_input)
            } else {
                false
            }
        });
        assert!(
            found_input,
            "Should have received terminal input message with correct content: '{}'",
            expected_input.trim()
        );
        println!(
            "✓ Verified terminal input message with correct content: '{}'",
            expected_input.trim()
        );

        let _ = control_sink.close().await;
        let _ = terminal_sink.close().await;
        server_handle.abort();
        revoke_token(test_token_name).expect("Failed to revoke test token");
    }

    #[tokio::test]
    #[serial]
    async fn test_websocket_disconnection_cleanup() {
        // clean up token state
        let _ = delete_db();

        let test_token_name = "test_token_disconnection";
        let (token, _) =
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
                listener,
                None,
                Some(session_manager),
                Some(client_os_api_factory),
            )
            .await;
        });

        tokio::time::sleep(Duration::from_millis(100)).await;

        // Create client and connect
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
        .unwrap()
        .unwrap()
        .unwrap();

        let client_data: serde_json::Value =
            serde_json::from_str(&client_response.text().unwrap()).unwrap();
        let web_client_id = client_data["web_client_id"].as_str().unwrap().to_string();

        // Connect and immediately disconnect terminal WebSocket
        let terminal_ws_url = format!(
            "ws://127.0.0.1:{}/ws/terminal?web_client_id={}&token={}",
            port,
            web_client_id,
            urlencoding::encode(&token)
        );
        let (terminal_ws, _) = connect_async(&terminal_ws_url).await.unwrap();
        let (mut terminal_sink, _) = terminal_ws.split();

        // Send some specific input
        let test_input = "test cleanup input\n";
        terminal_sink
            .send(Message::Text(test_input.to_string()))
            .await
            .unwrap();

        // Close the connection
        let _ = terminal_sink.close().await;
        println!("✓ Closed terminal WebSocket connection");

        tokio::time::sleep(Duration::from_millis(500)).await;

        // Verify that ClientExited message was sent
        let mock_apis = factory_for_verification.mock_apis.lock().unwrap();
        let messages: Vec<_> = mock_apis
            .iter()
            .flat_map(|(_, api)| api.get_sent_messages())
            .collect();

        let client_exited_count = messages
            .iter()
            .filter(|msg| matches!(msg, ClientToServerMsg::ClientExited))
            .count();
        assert!(
            client_exited_count > 0,
            "Should have received at least one ClientExited message after disconnection"
        );
        println!(
            "✓ Verified {} ClientExited message(s) were sent after disconnection",
            client_exited_count
        );

        // Also verify the input we sent was processed before disconnection
        let found_input = messages.iter().any(|msg| {
            if let ClientToServerMsg::Key(_, raw_bytes, _) = msg {
                let input_str = String::from_utf8_lossy(&raw_bytes);
                input_str.contains(test_input)
            } else {
                false
            }
        });
        assert!(
            found_input,
            "Should have processed input '{}' before disconnection",
            test_input.trim()
        );
        println!("✓ Verified input was processed before disconnection cleanup");

        server_handle.abort();
        revoke_token(test_token_name).expect("Failed to revoke test token");
    }

    #[tokio::test]
    #[serial]
    async fn test_session_with_specific_name() {
        // clean up token state
        let _ = delete_db();

        let test_token_name = "test_token_specific_session";
        let (token, _) =
            create_token(Some(test_token_name.to_string())).expect("Failed to create test token");

        let mut session_manager = MockSessionManager::new();
        session_manager.add_session("my_custom_session".to_string());
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
        .unwrap()
        .unwrap()
        .unwrap();

        let client_data: serde_json::Value =
            serde_json::from_str(&client_response.text().unwrap()).unwrap();
        let web_client_id = client_data["web_client_id"].as_str().unwrap().to_string();

        // Connect to the specific session
        let terminal_ws_url = format!(
            "ws://127.0.0.1:{}/ws/terminal/my_custom_session?web_client_id={}&token={}",
            port,
            web_client_id,
            urlencoding::encode(&token)
        );
        let (terminal_ws, _) = connect_async(&terminal_ws_url).await.unwrap();
        let (mut terminal_sink, _) = terminal_ws.split();

        terminal_sink
            .send(Message::Text("session specific command\n".to_string()))
            .await
            .unwrap();
        println!("✓ Successfully connected to specific session 'my_custom_session'");

        tokio::time::sleep(Duration::from_millis(500)).await;

        let mock_apis = factory_for_verification.mock_apis.lock().unwrap();
        let has_messages = mock_apis
            .iter()
            .any(|(_, api)| !api.get_sent_messages().is_empty());
        assert!(
            has_messages,
            "Should have received messages when connecting to specific session"
        );
        println!("✓ Verified input was processed for specific session");

        let _ = terminal_sink.close().await;
        server_handle.abort();
        revoke_token(test_token_name).expect("Failed to revoke test token");
    }

    #[tokio::test]
    #[serial]
    async fn test_cookie_authentication() {
        // clean up token state
        let _ = delete_db();

        let test_token_name = "test_token_cookie_auth";
        let (token, _) =
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
                listener,
                None,
                Some(session_manager),
                Some(client_os_api_factory),
            )
            .await;
        });

        tokio::time::sleep(Duration::from_millis(100)).await;

        // First, authenticate with header and request to remember
        let session_url = format!("http://127.0.0.1:{}/session", port);
        let first_response = timeout(
            Duration::from_secs(5),
            tokio::task::spawn_blocking({
                let token = token.clone();
                let session_url = session_url.clone();
                move || {
                    isahc::Request::post(&session_url)
                        .header("Authorization", format!("Bearer {}", token))
                        .header("X-Remember-Me", "true")
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

        assert!(first_response.status().is_success());

        // Extract cookie from response
        let set_cookie_header = first_response.headers().get("set-cookie");
        assert!(
            set_cookie_header.is_some(),
            "Should have received set-cookie header"
        );
        let cookie_value = set_cookie_header.unwrap().to_str().unwrap();
        assert!(
            cookie_value.contains("auth_token="),
            "Cookie should contain auth_token"
        );
        println!("✓ Received authentication cookie: {}", cookie_value);

        // Extract just the token value from the cookie
        let cookie_token = cookie_value
            .split(';')
            .next()
            .and_then(|part| part.split('=').nth(1))
            .unwrap();

        // Now make a second request using only the cookie (no Authorization header)
        let second_response = timeout(
            Duration::from_secs(5),
            tokio::task::spawn_blocking({
                let cookie_token = cookie_token.to_string();
                move || {
                    isahc::Request::post(&session_url)
                        .header("Cookie", format!("auth_token={}", cookie_token))
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

        assert!(
            second_response.status().is_success(),
            "Cookie authentication should work"
        );
        println!("✓ Successfully authenticated using cookie");

        server_handle.abort();
        revoke_token(test_token_name).expect("Failed to revoke test token");
    }

    #[tokio::test]
    #[serial]
    async fn test_unknown_client_id_handling() {
        // clean up token state
        let _ = delete_db();

        let test_token_name = "test_token_unknown_client";
        let (token, _) =
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
                listener,
                None,
                Some(session_manager),
                Some(client_os_api_factory),
            )
            .await;
        });

        tokio::time::sleep(Duration::from_millis(100)).await;

        // First, create a valid client to establish baseline
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
        .unwrap()
        .unwrap()
        .unwrap();

        let client_data: serde_json::Value =
            serde_json::from_str(&client_response.text().unwrap()).unwrap();
        let valid_web_client_id = client_data["web_client_id"].as_str().unwrap().to_string();

        // Try to connect to control WebSocket and send message with fake client ID
        let control_ws_url = format!(
            "ws://127.0.0.1:{}/ws/control?token={}",
            port,
            urlencoding::encode(&token)
        );
        let (control_ws, _) = connect_async(&control_ws_url).await.unwrap();
        let (mut control_sink, _) = control_ws.split();

        // Receive initial SetConfig message
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Send control message with unknown client ID
        let fake_resize_msg = WebClientToWebServerControlMessage {
            web_client_id: "fake-client-id-12345".to_string(),
            payload: WebClientToWebServerControlMessagePayload::TerminalResize(Size {
                rows: 99,
                cols: 199,
            }),
        };

        control_sink
            .send(Message::Text(
                serde_json::to_string(&fake_resize_msg).unwrap(),
            ))
            .await
            .unwrap();
        println!("✓ Sent control message with unknown client ID");

        // Try to connect to terminal WebSocket with fake client ID
        let terminal_ws_url = format!(
            "ws://127.0.0.1:{}/ws/terminal?web_client_id={}&token={}",
            port,
            "fake-client-id-67890",
            urlencoding::encode(&token)
        );

        let connection_result =
            timeout(Duration::from_secs(2), connect_async(&terminal_ws_url)).await;

        match connection_result {
            Ok(Ok((terminal_ws, _))) => {
                let (mut terminal_sink, _) = terminal_ws.split();
                // Try to send a message with fake client ID
                let fake_input = "fake client input that should not reach server\n";
                let send_result = terminal_sink
                    .send(Message::Text(fake_input.to_string()))
                    .await;
                println!(
                    "✓ Terminal connection with fake ID: send result = {:?}",
                    send_result.is_ok()
                );

                // Give some time for message processing
                tokio::time::sleep(Duration::from_millis(300)).await;
                let _ = terminal_sink.close().await;
            },
            Ok(Err(_)) => {
                println!("✓ Terminal connection with fake client ID was rejected");
            },
            Err(_) => {
                println!("✓ Terminal connection with fake client ID timed out");
            },
        }

        // Now send a valid message from the real client to ensure server is still working
        let valid_terminal_ws_url = format!(
            "ws://127.0.0.1:{}/ws/terminal?web_client_id={}&token={}",
            port,
            valid_web_client_id,
            urlencoding::encode(&token)
        );
        let (valid_terminal_ws, _) = connect_async(&valid_terminal_ws_url).await.unwrap();
        let (mut valid_terminal_sink, _) = valid_terminal_ws.split();

        let valid_input = "valid client input that should reach server\n";
        valid_terminal_sink
            .send(Message::Text(valid_input.to_string()))
            .await
            .unwrap();
        println!("✓ Sent valid input from real client");

        tokio::time::sleep(Duration::from_millis(500)).await;

        // Verify that fake messages did NOT reach the mock OS API
        let mock_apis = factory_for_verification.mock_apis.lock().unwrap();
        let all_messages: Vec<_> = mock_apis
            .iter()
            .flat_map(|(_, api)| api.get_sent_messages())
            .collect();

        // Check that the fake resize message (99x199) was NOT processed
        let found_fake_resize = all_messages.iter().any(|msg| {
            if let ClientToServerMsg::TerminalResize(size) = msg {
                size.rows == 99 && size.cols == 199
            } else {
                false
            }
        });
        assert!(
            !found_fake_resize,
            "Fake resize message should NOT have reached the mock OS API"
        );
        println!(
            "✓ Verified fake resize message (99x199) was rejected and did not reach mock OS API"
        );

        // Check that the fake terminal input was NOT processed
        let found_fake_input = all_messages.iter().any(|msg| {
            if let ClientToServerMsg::Key(_, raw_bytes, _) = msg {
                let input_str = String::from_utf8_lossy(&raw_bytes);
                input_str.contains("fake client input that should not reach server")
            } else {
                false
            }
        });
        assert!(
            !found_fake_input,
            "Fake terminal input should NOT have reached the mock OS API"
        );
        println!("✓ Verified fake terminal input was rejected and did not reach mock OS API");

        // Check that the valid input WAS processed (to ensure server is still working)
        let found_valid_input = all_messages.iter().any(|msg| {
            if let ClientToServerMsg::Key(_, raw_bytes, _) = msg {
                let input_str = String::from_utf8_lossy(&raw_bytes);
                input_str.contains("valid client input that should reach server")
            } else {
                false
            }
        });
        assert!(
            found_valid_input,
            "Valid terminal input should have reached the mock OS API"
        );
        println!("✓ Verified valid terminal input reached mock OS API (server still working)");

        let _ = control_sink.close().await;
        let _ = valid_terminal_sink.close().await;
        server_handle.abort();
        revoke_token(test_token_name).expect("Failed to revoke test token");
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

    pub fn add_session(&mut self, name: String) {
        self.mock_sessions.insert(name, true);
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
