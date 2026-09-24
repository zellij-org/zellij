#![cfg(unix)]

use futures_util::{SinkExt, StreamExt};
use isahc::prelude::*;
use std::time::Duration;
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::http::Request;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};
use zellij_integration_tests::{
    claim_first_terminal_and_wait_for_prompt, TestRunner, TestSession, TERMINAL_SIZE,
};
use zellij_utils::structured_render::{self, GraphicsMedium, GraphicsRecord};
use zellij_utils::web_authentication_tokens::{create_token, revoke_token};

type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;

const FRAME_TIMEOUT: Duration = Duration::from_secs(20);
const RGB_2X2_A_T: &[u8] = b"\x1b_Ga=T,q=2,f=24,s=2,v=2,m=0;////////////////\x1b\\";

fn start_shared_session() -> TestSession {
    TestRunner::new(TERMINAL_SIZE)
        .with_config("web_sharing \"on\"")
        .start()
}

struct WebServer {
    port: u16,
    token_name: String,
    session_token: String,
    handle: tokio::task::JoinHandle<()>,
}

fn start_web_server(runtime: &tokio::runtime::Runtime, token_name: &str) -> WebServer {
    let (auth_token, _) =
        create_token(Some(token_name.to_owned()), false).expect("failed to create an auth token");

    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("failed to bind a port");
    let address = listener.local_addr().unwrap();
    let port = address.port();

    let handle = runtime.spawn(zellij_client::web_client::serve_web_client(
        zellij_utils::input::config::Config::default(),
        zellij_utils::input::options::Options::default(),
        Some(zellij_integration_tests::test_env::write_config(
            token_name, "",
        )),
        listener,
        None,
        None,
        None,
        address.ip(),
        port,
    ));

    wait_for_web_server(port);
    let session_token = log_in(port, &auth_token);

    WebServer {
        port,
        token_name: token_name.to_owned(),
        session_token,
        handle,
    }
}

fn wait_for_web_server(port: u16) {
    let url = format!("http://127.0.0.1:{}/info/version", port);
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while std::time::Instant::now() < deadline {
        if isahc::get(&url).is_ok() {
            return;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    panic!("the web server did not come up on port {}", port);
}

fn log_in(port: u16, auth_token: &str) -> String {
    let payload = serde_json::json!({ "auth_token": auth_token, "remember_me": true });
    let response = isahc::Request::post(format!("http://127.0.0.1:{}/command/login", port))
        .header("Content-Type", "application/json")
        .body(payload.to_string())
        .unwrap()
        .send()
        .expect("the login request failed");
    assert!(response.status().is_success(), "login was refused");
    let cookie = response
        .headers()
        .get("set-cookie")
        .expect("login returned no session cookie")
        .to_str()
        .unwrap()
        .to_owned();
    let _ = response;
    cookie
        .split(';')
        .next()
        .and_then(|pair| pair.split('=').nth(1))
        .expect("the session cookie has no value")
        .to_owned()
}

fn create_web_client_id(server: &WebServer, session_name: &str) -> String {
    let mut response = isahc::Request::post(format!(
        "http://127.0.0.1:{}/session?session={}",
        server.port, session_name
    ))
    .header("Cookie", format!("session_token={}", server.session_token))
    .header("Content-Type", "application/json")
    .body("{}")
    .unwrap()
    .send()
    .expect("the session request failed");
    assert!(
        response.status().is_success(),
        "the session request was refused"
    );
    let body: serde_json::Value = serde_json::from_str(&response.text().unwrap()).unwrap();
    body["web_client_id"].as_str().unwrap().to_owned()
}

async fn open_socket(url: &str, session_token: &str) -> Socket {
    let request = Request::builder()
        .uri(url)
        .header("Cookie", format!("session_token={}", session_token))
        .header("Connection", "Upgrade")
        .header("Upgrade", "websocket")
        .header("Sec-WebSocket-Version", "13")
        .header("Sec-WebSocket-Key", "dGhlIHNhbXBsZSBub25jZQ==")
        .header("Host", "127.0.0.1")
        .body(())
        .unwrap();
    let (socket, _) = connect_async(request)
        .await
        .expect("failed to open a websocket");
    socket
}

struct StructuredClient {
    web_client_id: String,
    terminal_ws: Socket,
    control_ws: Socket,
}

impl StructuredClient {
    async fn next_frame(&mut self) -> Vec<u8> {
        let deadline = tokio::time::Instant::now() + FRAME_TIMEOUT;
        loop {
            let message = tokio::time::timeout_at(deadline, self.terminal_ws.next())
                .await
                .expect("no render frame arrived before the deadline")
                .expect("the terminal websocket closed")
                .expect("the terminal websocket failed");
            match message {
                Message::Binary(frame) => return frame.to_vec(),
                Message::Text(_) | Message::Ping(_) | Message::Pong(_) => continue,
                other => panic!("unexpected terminal message {:?}", other),
            }
        }
    }

    async fn acknowledge(&mut self, seq: u64) {
        self.send_control(serde_json::json!({ "type": "RenderFrameAck", "seq": seq }))
            .await;
    }

    async fn send_control(&mut self, payload: serde_json::Value) {
        let message = serde_json::json!({
            "web_client_id": self.web_client_id,
            "payload": payload,
        });
        self.control_ws
            .send(Message::Text(message.to_string().into()))
            .await
            .expect("failed to send a control message");
    }

    async fn control_message_named(&mut self, name: &str) -> (serde_json::Value, Vec<String>) {
        let deadline = tokio::time::Instant::now() + FRAME_TIMEOUT;
        let mut seen: Vec<String> = vec![];
        loop {
            let message = tokio::time::timeout_at(deadline, self.control_ws.next())
                .await
                .unwrap_or_else(|_| {
                    panic!("no {} arrived before the deadline, saw {:?}", name, seen)
                })
                .expect("the control websocket closed")
                .expect("the control websocket failed");
            let Message::Text(text) = message else {
                continue;
            };
            let parsed: serde_json::Value =
                serde_json::from_str(&text).expect("a control message must be JSON");
            if parsed["type"] == name {
                return (parsed, seen);
            }
            seen.push(parsed["type"].as_str().unwrap_or("?").to_owned());
        }
    }
}

fn attach_structured_client(
    runtime: &tokio::runtime::Runtime,
    server: &WebServer,
    session_name: &str,
) -> StructuredClient {
    let web_client_id = create_web_client_id(server, session_name);
    runtime.block_on(async {
        let control_ws = open_socket(
            &format!(
                "ws://127.0.0.1:{}/ws/control?web_client_id={}",
                server.port, web_client_id
            ),
            &server.session_token,
        )
        .await;
        let terminal_ws = open_socket(
            &format!(
                "ws://127.0.0.1:{}/ws/terminal/{}?web_client_id={}&rows={}&cols={}&cell_width=8&cell_height=16&structured=true",
                server.port,
                session_name,
                web_client_id,
                TERMINAL_SIZE.rows,
                TERMINAL_SIZE.cols
            ),
            &server.session_token,
        )
        .await;
        StructuredClient {
            web_client_id,
            terminal_ws,
            control_ws,
        }
    })
}

fn shut_down(runtime: tokio::runtime::Runtime, server: WebServer, mut zellij: TestSession) {
    server.handle.abort();
    let _ = revoke_token(&server.token_name);
    runtime.shutdown_timeout(Duration::from_millis(200));
    zellij.quit();
}

#[test]
fn a_structured_client_through_the_web_server_is_paced_by_its_acknowledgements() {
    let zellij = start_shared_session();
    let terminal = claim_first_terminal_and_wait_for_prompt(&zellij);

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    let server = start_web_server(&runtime, "integration_structured_pacing");
    let mut client = attach_structured_client(&runtime, &server, zellij.session_name());

    let mut delivered = vec![];
    for round in 0..3 {
        let frame = runtime.block_on(client.next_frame());
        let view = structured_render::decode(&frame).expect("a frame must decode");
        let seq = view.header().seq;
        assert_eq!(
            view.header().cols,
            TERMINAL_SIZE.cols as u16,
            "the frame is built for the client's own viewport"
        );
        delivered.push(seq);
        runtime.block_on(client.acknowledge(seq));
        terminal.output(format!("frame round {}\r\n", round).as_bytes());
    }

    assert_eq!(
        delivered.len(),
        3,
        "three frames were delivered in sequence"
    );
    assert!(
        delivered.windows(2).all(|pair| pair[1] > pair[0]),
        "each acknowledged frame is followed by a later one, got {:?}",
        delivered
    );

    shut_down(runtime, server, zellij);
}

#[test]
fn a_kitty_image_reaches_a_remote_structured_client_inline() {
    let zellij = start_shared_session();
    let terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    terminal.disable_echo();

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    let server = start_web_server(&runtime, "integration_structured_kitty");
    let mut client = attach_structured_client(&runtime, &server, zellij.session_name());

    let first = runtime.block_on(client.next_frame());
    let seq = structured_render::decode(&first)
        .expect("a frame must decode")
        .header()
        .seq;
    runtime.block_on(client.acknowledge(seq));

    terminal.output(RGB_2X2_A_T);

    let mut residencies = vec![];
    for _ in 0..8 {
        let frame = runtime.block_on(client.next_frame());
        let view = structured_render::decode(&frame).expect("a frame must decode");
        for record in view.graphics() {
            if let GraphicsRecord::Residency { medium, data, .. } = record {
                residencies.push((medium, data.len()));
            }
        }
        runtime.block_on(client.acknowledge(view.header().seq));
        if !residencies.is_empty() {
            break;
        }
    }

    assert!(
        !residencies.is_empty(),
        "the image placed in the pane must reach the client as a residency record"
    );
    assert!(
        residencies
            .iter()
            .all(|(medium, _)| *medium == GraphicsMedium::Inline),
        "a remote client is never handed a shared media file, got {:?}",
        residencies
    );
    assert!(
        residencies.iter().all(|(_, byte_len)| *byte_len > 0),
        "an inline residency carries the pixels themselves, got {:?}",
        residencies
    );

    shut_down(runtime, server, zellij);
}

#[test]
fn a_structured_client_types_resizes_and_detaches_over_the_control_socket() {
    let zellij = start_shared_session();
    let terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    terminal.disable_echo();

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    let server = start_web_server(&runtime, "integration_structured_input");
    let mut client = attach_structured_client(&runtime, &server, zellij.session_name());

    let first = runtime.block_on(client.next_frame());
    let seq = structured_render::decode(&first)
        .expect("a frame must decode")
        .header()
        .seq;
    runtime.block_on(client.acknowledge(seq));

    runtime.block_on(client.send_control(serde_json::json!({
        "type": "Key",
        "key": { "bare_key": { "Char": "x" }, "key_modifiers": [] },
        "raw_bytes": [b'x'],
        "is_kitty_keyboard_protocol": false,
    })));
    terminal.wait_for_stdin("the typed key to reach the pane", |stdin| {
        stdin.contains(&b'x')
    });

    runtime.block_on(client.send_control(serde_json::json!({
        "type": "Text",
        "chars": "你好",
    })));
    let stdin = terminal.wait_for_stdin("the composed text to reach the pane", |stdin| {
        stdin
            .windows("你好".len())
            .any(|window| window == "你好".as_bytes())
    });
    assert!(
        !stdin
            .windows(6)
            .any(|window| window == b"\x1b[200~" || window == b"\x1b[201~"),
        "a commit is typed text and must not arrive wrapped in bracketed-paste markers, \
         got {:?}",
        String::from_utf8_lossy(&stdin)
    );

    let before = terminal.size().expect("the pane has a size");
    runtime.block_on(client.send_control(serde_json::json!({
        "type": "TerminalResize",
        "rows": TERMINAL_SIZE.rows - 4,
        "cols": TERMINAL_SIZE.cols - 6,
    })));
    let after = terminal.wait_for_size("the pane to follow the client's new size", {
        let before_rows = before.1;
        move |_cols, rows| rows != before_rows
    });
    assert_ne!(
        after, before,
        "a resize sent on the control socket must reach the pane"
    );

    runtime.block_on(client.send_control(serde_json::json!({ "type": "Detach" })));
    let (exit, seen_control_types) = runtime.block_on(client.control_message_named("Exit"));
    assert_eq!(
        exit["reason"], "Normal",
        "a window is told why it left, in the same words a unix-socket client is told \
         (DetachSession sends ExitReason::Normal, zellij-server/src/lib.rs:1526), got {:?}",
        exit
    );

    assert!(
        !seen_control_types.contains(&"MobileState".to_owned()),
        "a window is never told the phone layout, not even in the window between its attach \
         and its declaration reaching the screen thread, got {:?}",
        seen_control_types
    );

    let mut rejoined = attach_structured_client(&runtime, &server, zellij.session_name());
    let frame = runtime.block_on(rejoined.next_frame());
    let view = structured_render::decode(&frame).expect("a frame must decode");
    assert_eq!(
        view.header().cols,
        TERMINAL_SIZE.cols as u16,
        "the session outlived the detach and serves a second structured client"
    );

    shut_down(runtime, server, zellij);
}
