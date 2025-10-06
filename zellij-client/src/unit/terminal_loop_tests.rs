use crate::os_input_output::{AsyncSignals, AsyncStdin, ClientOsApi, SignalEvent};
use crate::remote_attach::WebSocketConnections;
use crate::run_remote_client_terminal_loop;
use crate::web_client::control_message::{
    WebClientToWebServerControlMessage, WebClientToWebServerControlMessagePayload,
    WebServerToWebClientControlMessage,
};
use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use serial_test::serial;
use std::io::{self, Write};
use std::os::unix::io::RawFd;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::mpsc;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;
use zellij_utils::data::Palette;
use zellij_utils::errors::ErrorContext;
use zellij_utils::ipc::{ClientToServerMsg, ServerToClientMsg};
use zellij_utils::pane_size::Size;

/// Mock stdin that allows tests to inject input data
struct MockAsyncStdin {
    rx: Arc<tokio::sync::Mutex<mpsc::UnboundedReceiver<Vec<u8>>>>,
}

#[async_trait]
impl AsyncStdin for MockAsyncStdin {
    async fn read(&mut self) -> io::Result<Vec<u8>> {
        match self.rx.lock().await.recv().await {
            Some(data) => Ok(data),
            None => Ok(Vec::new()), // EOF
        }
    }
}

/// Mock signal listener that allows tests to inject signals
struct MockAsyncSignals {
    rx: Arc<tokio::sync::Mutex<mpsc::UnboundedReceiver<SignalEvent>>>,
}

#[async_trait]
impl AsyncSignals for MockAsyncSignals {
    async fn recv(&mut self) -> Option<SignalEvent> {
        self.rx.lock().await.recv().await
    }
}

/// Mock ClientOsApi for testing
#[derive(Clone)]
struct TestClientOsApi {
    stdout_buffer: Arc<Mutex<Vec<u8>>>,
    stdin_rx: Arc<tokio::sync::Mutex<mpsc::UnboundedReceiver<Vec<u8>>>>,
    signal_rx: Arc<tokio::sync::Mutex<mpsc::UnboundedReceiver<SignalEvent>>>,
    terminal_size: Size,
}

impl TestClientOsApi {
    fn new(
        stdin_rx: mpsc::UnboundedReceiver<Vec<u8>>,
        signal_rx: mpsc::UnboundedReceiver<SignalEvent>,
    ) -> Self {
        Self {
            stdout_buffer: Arc::new(Mutex::new(Vec::new())),
            stdin_rx: Arc::new(tokio::sync::Mutex::new(stdin_rx)),
            signal_rx: Arc::new(tokio::sync::Mutex::new(signal_rx)),
            terminal_size: Size { rows: 24, cols: 80 },
        }
    }
}

impl std::fmt::Debug for TestClientOsApi {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TestClientOsApi").finish()
    }
}

impl ClientOsApi for TestClientOsApi {
    fn get_terminal_size_using_fd(&self, _fd: RawFd) -> Size {
        self.terminal_size
    }

    fn set_raw_mode(&mut self, _fd: RawFd) {}

    fn unset_raw_mode(&self, _fd: RawFd) -> Result<(), nix::Error> {
        Ok(())
    }

    fn get_stdout_writer(&self) -> Box<dyn Write> {
        Box::new(TestWriter {
            buffer: self.stdout_buffer.clone(),
        })
    }

    fn get_stdin_reader(&self) -> Box<dyn io::BufRead> {
        Box::new(io::Cursor::new(Vec::new()))
    }

    fn update_session_name(&mut self, _new_session_name: String) {}

    fn read_from_stdin(&mut self) -> Result<Vec<u8>, &'static str> {
        Ok(Vec::new())
    }

    fn box_clone(&self) -> Box<dyn ClientOsApi> {
        Box::new(self.clone())
    }

    fn send_to_server(&self, _msg: ClientToServerMsg) {}

    fn recv_from_server(&self) -> Option<(ServerToClientMsg, ErrorContext)> {
        None
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

    fn get_async_stdin_reader(&self) -> Box<dyn AsyncStdin> {
        Box::new(MockAsyncStdin {
            rx: self.stdin_rx.clone(),
        })
    }

    fn get_async_signal_listener(&self) -> io::Result<Box<dyn AsyncSignals>> {
        Ok(Box::new(MockAsyncSignals {
            rx: self.signal_rx.clone(),
        }))
    }
}

struct TestWriter {
    buffer: Arc<Mutex<Vec<u8>>>,
}

impl Write for TestWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.buffer.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

mod mock_ws_server {
    use super::*;
    use axum::{
        extract::{
            ws::{WebSocket, WebSocketUpgrade},
            State,
        },
        routing::get,
        Router,
    };
    use tokio::net::TcpListener;
    use tokio::task::JoinHandle;

    #[derive(Clone)]
    struct WsState {
        client_tx: Arc<Mutex<mpsc::UnboundedSender<Message>>>,
        server_rx: Arc<Mutex<mpsc::UnboundedReceiver<Message>>>,
    }

    pub struct MockWsServer {
        pub terminal_to_client_tx: mpsc::UnboundedSender<Message>,
        pub control_to_client_tx: mpsc::UnboundedSender<Message>,
        pub client_to_terminal_rx: Arc<Mutex<mpsc::UnboundedReceiver<Message>>>,
        pub client_to_control_rx: Arc<Mutex<mpsc::UnboundedReceiver<Message>>>,
    }

    impl MockWsServer {
        pub async fn start() -> (u16, Self, JoinHandle<()>) {
            let (terminal_to_client_tx, terminal_to_client_rx) = mpsc::unbounded_channel();
            let (client_to_terminal_tx, client_to_terminal_rx) = mpsc::unbounded_channel();
            let (control_to_client_tx, control_to_client_rx) = mpsc::unbounded_channel();
            let (client_to_control_tx, client_to_control_rx) = mpsc::unbounded_channel();

            let terminal_state = WsState {
                client_tx: Arc::new(Mutex::new(client_to_terminal_tx)),
                server_rx: Arc::new(Mutex::new(terminal_to_client_rx)),
            };

            let control_state = WsState {
                client_tx: Arc::new(Mutex::new(client_to_control_tx)),
                server_rx: Arc::new(Mutex::new(control_to_client_rx)),
            };

            let app = Router::new()
                .route(
                    "/ws/terminal",
                    get(terminal_handler).with_state(terminal_state.clone()),
                )
                .route(
                    "/ws/control",
                    get(control_handler).with_state(control_state.clone()),
                );

            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let port = listener.local_addr().unwrap().port();

            let server_handle = tokio::spawn(async move {
                axum::serve(listener, app).await.unwrap();
            });

            // Wait for server to start
            tokio::time::sleep(Duration::from_millis(100)).await;

            let server = MockWsServer {
                terminal_to_client_tx,
                control_to_client_tx,
                client_to_terminal_rx: Arc::new(Mutex::new(client_to_terminal_rx)),
                client_to_control_rx: Arc::new(Mutex::new(client_to_control_rx)),
            };

            (port, server, server_handle)
        }
    }

    async fn terminal_handler(
        ws: WebSocketUpgrade,
        State(state): State<WsState>,
    ) -> impl axum::response::IntoResponse {
        ws.on_upgrade(move |socket| handle_websocket(socket, state))
    }

    async fn control_handler(
        ws: WebSocketUpgrade,
        State(state): State<WsState>,
    ) -> impl axum::response::IntoResponse {
        ws.on_upgrade(move |socket| handle_websocket(socket, state))
    }

    async fn handle_websocket(socket: WebSocket, state: WsState) {
        let (mut ws_tx, mut ws_rx) = socket.split();
        let client_tx = state.client_tx.clone();

        let recv_task = tokio::spawn(async move {
            while let Some(Ok(axum_msg)) = ws_rx.next().await {
                // Convert axum::extract::ws::Message to tokio_tungstenite::tungstenite::Message
                let tungstenite_msg = match axum_msg {
                    axum::extract::ws::Message::Text(text) => Message::Text(text.to_string()),
                    axum::extract::ws::Message::Binary(data) => Message::Binary(data.to_vec()),
                    axum::extract::ws::Message::Ping(data) => Message::Ping(data.to_vec()),
                    axum::extract::ws::Message::Pong(data) => Message::Pong(data.to_vec()),
                    axum::extract::ws::Message::Close(frame) => {
                        if let Some(f) = frame {
                            Message::Close(Some(tokio_tungstenite::tungstenite::protocol::CloseFrame {
                                code: tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode::from(f.code),
                                reason: std::borrow::Cow::Owned(f.reason.to_string()),
                            }))
                        } else {
                            Message::Close(None)
                        }
                    },
                };
                let _ = client_tx.lock().unwrap().send(tungstenite_msg);
            }
        });

        loop {
            let msg = state.server_rx.lock().unwrap().try_recv();
            match msg {
                Ok(tungstenite_msg) => {
                    // Convert tokio_tungstenite::tungstenite::Message to axum::extract::ws::Message
                    let axum_msg = match tungstenite_msg {
                        Message::Text(text) => axum::extract::ws::Message::Text(text.into()),
                        Message::Binary(data) => axum::extract::ws::Message::Binary(data.into()),
                        Message::Ping(data) => axum::extract::ws::Message::Ping(data.into()),
                        Message::Pong(data) => axum::extract::ws::Message::Pong(data.into()),
                        Message::Close(frame) => {
                            if let Some(f) = frame {
                                axum::extract::ws::Message::Close(Some(
                                    axum::extract::ws::CloseFrame {
                                        code: f.code.into(),
                                        reason: f.reason.to_string().into(),
                                    },
                                ))
                            } else {
                                axum::extract::ws::Message::Close(None)
                            }
                        },
                        Message::Frame(_) => continue, // Skip raw frames
                    };
                    if ws_tx.send(axum_msg).await.is_err() {
                        break;
                    }
                },
                Err(_) => {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                },
            }
        }

        recv_task.abort();
    }
}

#[tokio::test]
#[serial]
async fn test_stdin_forwarded_to_terminal_websocket() {
    // Setup mock WebSocket server
    let (port, server, _server_handle) = mock_ws_server::MockWsServer::start().await;

    // Create WebSocket connections
    let terminal_url = format!("ws://127.0.0.1:{}/ws/terminal", port);
    let control_url = format!("ws://127.0.0.1:{}/ws/control", port);

    let (terminal_ws, _) = connect_async(&terminal_url).await.unwrap();
    let (control_ws, _) = connect_async(&control_url).await.unwrap();

    let connections = WebSocketConnections {
        terminal_ws,
        control_ws,
        web_client_id: "test-stdin".to_string(),
    };

    // Create mock OS API with controllable stdin
    let (stdin_tx, stdin_rx) = mpsc::unbounded_channel();
    let (_signal_tx, signal_rx) = mpsc::unbounded_channel();

    let os_input = Box::new(TestClientOsApi::new(stdin_rx, signal_rx));

    // Spawn the async loop
    let loop_handle =
        tokio::spawn(async move { run_remote_client_terminal_loop(os_input, connections).await });

    // Send stdin data
    let test_data = b"hello from stdin\n".to_vec();
    stdin_tx.send(test_data.clone()).unwrap();

    // Verify terminal WebSocket received the data
    tokio::time::sleep(Duration::from_millis(200)).await;
    let received = tokio::time::timeout(
        Duration::from_secs(1),
        server.client_to_terminal_rx.lock().unwrap().recv(),
    )
    .await
    .expect("Timeout")
    .expect("No message");

    match received {
        Message::Binary(data) => assert_eq!(data, test_data),
        _ => panic!("Expected Binary message, got: {:?}", received),
    }

    // Cleanup: send EOF via stdin
    drop(stdin_tx);
    let _ = tokio::time::timeout(Duration::from_secs(2), loop_handle)
        .await
        .expect("Loop didn't exit")
        .unwrap();
}

#[tokio::test]
#[serial]
async fn test_terminal_output_written_to_stdout() {
    let (port, server, _server_handle) = mock_ws_server::MockWsServer::start().await;

    let terminal_url = format!("ws://127.0.0.1:{}/ws/terminal", port);
    let control_url = format!("ws://127.0.0.1:{}/ws/control", port);

    let (terminal_ws, _) = connect_async(&terminal_url).await.unwrap();
    let (control_ws, _) = connect_async(&control_url).await.unwrap();

    let connections = WebSocketConnections {
        terminal_ws,
        control_ws,
        web_client_id: "test-stdout".to_string(),
    };

    let (_stdin_tx, stdin_rx) = mpsc::unbounded_channel();
    let (_signal_tx, signal_rx) = mpsc::unbounded_channel();

    let os_input = TestClientOsApi::new(stdin_rx, signal_rx);
    let stdout_buffer = os_input.stdout_buffer.clone();
    let os_input = Box::new(os_input);

    let loop_handle =
        tokio::spawn(async move { run_remote_client_terminal_loop(os_input, connections).await });

    // Send terminal output from server
    let test_output = "Hello from terminal";
    server
        .terminal_to_client_tx
        .send(Message::Text(test_output.to_string()))
        .unwrap();

    tokio::time::sleep(Duration::from_millis(200)).await;

    // Verify stdout received the output
    let stdout = stdout_buffer.lock().unwrap().clone();
    let stdout_str = String::from_utf8_lossy(&stdout);
    assert!(
        stdout_str.contains(test_output),
        "Expected stdout to contain '{}', got: '{}'",
        test_output,
        stdout_str
    );

    // Cleanup
    server
        .terminal_to_client_tx
        .send(Message::Close(None))
        .unwrap();
    let _ = tokio::time::timeout(Duration::from_secs(2), loop_handle)
        .await
        .expect("Loop didn't exit")
        .unwrap();
}

#[tokio::test]
#[serial]
async fn test_resize_signal_sends_control_message() {
    let (port, server, _server_handle) = mock_ws_server::MockWsServer::start().await;

    let terminal_url = format!("ws://127.0.0.1:{}/ws/terminal", port);
    let control_url = format!("ws://127.0.0.1:{}/ws/control", port);

    let (terminal_ws, _) = connect_async(&terminal_url).await.unwrap();
    let (control_ws, _) = connect_async(&control_url).await.unwrap();

    let connections = WebSocketConnections {
        terminal_ws,
        control_ws,
        web_client_id: "test-resize".to_string(),
    };

    let (_stdin_tx, stdin_rx) = mpsc::unbounded_channel();
    let (signal_tx, signal_rx) = mpsc::unbounded_channel();

    let os_input = Box::new(TestClientOsApi::new(stdin_rx, signal_rx));

    let loop_handle =
        tokio::spawn(async move { run_remote_client_terminal_loop(os_input, connections).await });

    // Wait for initial resize message to be sent on startup
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Consume the initial resize message
    let _ = tokio::time::timeout(
        Duration::from_millis(500),
        server.client_to_control_rx.lock().unwrap().recv(),
    )
    .await;

    // Send resize signal
    signal_tx.send(SignalEvent::Resize).unwrap();

    tokio::time::sleep(Duration::from_millis(200)).await;

    // Verify control WebSocket received resize message
    let received = tokio::time::timeout(
        Duration::from_secs(1),
        server.client_to_control_rx.lock().unwrap().recv(),
    )
    .await
    .expect("Timeout")
    .expect("No message");

    match received {
        Message::Text(text) => {
            let parsed: WebClientToWebServerControlMessage =
                serde_json::from_str(&text).expect("Failed to parse");
            assert!(
                matches!(
                    parsed.payload,
                    WebClientToWebServerControlMessagePayload::TerminalResize(_)
                ),
                "Expected TerminalResize, got: {:?}",
                parsed.payload
            );
        },
        _ => panic!("Expected Text message, got: {:?}", received),
    }

    // Cleanup
    signal_tx.send(SignalEvent::Quit).unwrap();
    let _ = tokio::time::timeout(Duration::from_secs(2), loop_handle)
        .await
        .expect("Loop didn't exit")
        .unwrap();
}

#[tokio::test]
#[serial]
async fn test_quit_signal_exits_loop() {
    let (port, _server, _server_handle) = mock_ws_server::MockWsServer::start().await;

    let terminal_url = format!("ws://127.0.0.1:{}/ws/terminal", port);
    let control_url = format!("ws://127.0.0.1:{}/ws/control", port);

    let (terminal_ws, _) = connect_async(&terminal_url).await.unwrap();
    let (control_ws, _) = connect_async(&control_url).await.unwrap();

    let connections = WebSocketConnections {
        terminal_ws,
        control_ws,
        web_client_id: "test-quit".to_string(),
    };

    let (_stdin_tx, stdin_rx) = mpsc::unbounded_channel();
    let (signal_tx, signal_rx) = mpsc::unbounded_channel();

    let os_input = Box::new(TestClientOsApi::new(stdin_rx, signal_rx));

    let loop_handle =
        tokio::spawn(async move { run_remote_client_terminal_loop(os_input, connections).await });

    // Send quit signal
    signal_tx.send(SignalEvent::Quit).unwrap();

    // Verify loop exits cleanly
    let result = tokio::time::timeout(Duration::from_secs(2), loop_handle)
        .await
        .expect("Loop didn't exit within timeout")
        .expect("Loop panicked");

    assert!(result.is_ok(), "Expected Ok result, got: {:?}", result);
}

#[tokio::test]
#[serial]
async fn test_websocket_close_exits_loop() {
    let (port, server, _server_handle) = mock_ws_server::MockWsServer::start().await;

    let terminal_url = format!("ws://127.0.0.1:{}/ws/terminal", port);
    let control_url = format!("ws://127.0.0.1:{}/ws/control", port);

    let (terminal_ws, _) = connect_async(&terminal_url).await.unwrap();
    let (control_ws, _) = connect_async(&control_url).await.unwrap();

    let connections = WebSocketConnections {
        terminal_ws,
        control_ws,
        web_client_id: "test-close".to_string(),
    };

    let (_stdin_tx, stdin_rx) = mpsc::unbounded_channel();
    let (_signal_tx, signal_rx) = mpsc::unbounded_channel();

    let os_input = Box::new(TestClientOsApi::new(stdin_rx, signal_rx));

    let loop_handle =
        tokio::spawn(async move { run_remote_client_terminal_loop(os_input, connections).await });

    // Send close message
    server
        .terminal_to_client_tx
        .send(Message::Close(None))
        .unwrap();

    // Verify loop exits cleanly
    let result = tokio::time::timeout(Duration::from_secs(2), loop_handle)
        .await
        .expect("Loop didn't exit within timeout")
        .expect("Loop panicked");

    assert!(result.is_ok(), "Expected Ok result, got: {:?}", result);
}

#[tokio::test]
#[serial]
async fn test_control_message_handling() {
    let (port, server, _server_handle) = mock_ws_server::MockWsServer::start().await;

    let terminal_url = format!("ws://127.0.0.1:{}/ws/terminal", port);
    let control_url = format!("ws://127.0.0.1:{}/ws/control", port);

    let (terminal_ws, _) = connect_async(&terminal_url).await.unwrap();
    let (control_ws, _) = connect_async(&control_url).await.unwrap();

    let connections = WebSocketConnections {
        terminal_ws,
        control_ws,
        web_client_id: "test-control".to_string(),
    };

    let (_stdin_tx, stdin_rx) = mpsc::unbounded_channel();
    let (_signal_tx, signal_rx) = mpsc::unbounded_channel();

    let os_input = TestClientOsApi::new(stdin_rx, signal_rx);
    let terminal_size = os_input.terminal_size;
    let os_input = Box::new(os_input);

    let loop_handle =
        tokio::spawn(async move { run_remote_client_terminal_loop(os_input, connections).await });

    // Wait for initial resize message to be sent on startup
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Consume the initial resize message
    let _ = tokio::time::timeout(
        Duration::from_millis(500),
        server.client_to_control_rx.lock().unwrap().recv(),
    )
    .await;

    // Send QueryTerminalSize control message
    let query_msg = WebServerToWebClientControlMessage::QueryTerminalSize;
    server
        .control_to_client_tx
        .send(Message::Text(serde_json::to_string(&query_msg).unwrap()))
        .unwrap();

    tokio::time::sleep(Duration::from_millis(200)).await;

    // Verify we receive a resize response
    let received = tokio::time::timeout(
        Duration::from_secs(1),
        server.client_to_control_rx.lock().unwrap().recv(),
    )
    .await
    .expect("Timeout")
    .expect("No message");

    match received {
        Message::Text(text) => {
            let parsed: WebClientToWebServerControlMessage =
                serde_json::from_str(&text).expect("Failed to parse");
            let WebClientToWebServerControlMessagePayload::TerminalResize(size) = parsed.payload;
            assert_eq!(size, terminal_size);
        },
        _ => panic!("Expected Text message, got: {:?}", received),
    }

    // Test Log message (should not crash)
    let log_msg = WebServerToWebClientControlMessage::Log {
        lines: vec!["Test log".to_string()],
    };
    server
        .control_to_client_tx
        .send(Message::Text(serde_json::to_string(&log_msg).unwrap()))
        .unwrap();

    tokio::time::sleep(Duration::from_millis(100)).await;

    // Cleanup
    server
        .terminal_to_client_tx
        .send(Message::Close(None))
        .unwrap();
    let _ = tokio::time::timeout(Duration::from_secs(2), loop_handle)
        .await
        .expect("Loop didn't exit")
        .unwrap();
}
