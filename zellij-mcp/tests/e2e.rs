//! End-to-end test: a real MCP client (rmcp's own) talks to a real
//! `zellij-mcp` server, which drives a real `zellij api-server`, which drives
//! a real Zellij session.
//!
//! This crate is a thin translation layer over `zellij-api` — the underlying
//! session/tab/pane/input logic is already exhaustively tested there. What is
//! specific to *this* crate, and therefore what this test focuses on, is the
//! MCP wiring itself: does the tool list come through correctly, does a tool
//! call actually reach a real session, and does the bearer-token auth work.
//!
//! Needs both binaries built:
//!
//! ```sh
//! cargo xtask build          # for target/dev-opt/zellij (zellij-api)
//! cargo build -p zellij-mcp  # for target/debug/zellij-mcp
//! ZELLIJ_API_E2E=target/dev-opt/zellij ZELLIJ_MCP_E2E=target/debug/zellij-mcp \
//!   cargo test -p zellij-mcp --test e2e -- --nocapture
//! ```

use std::process::{Child, Command, Stdio};
use std::time::Duration;

use rmcp::model::{CallToolRequestParams, ClientCapabilities, ClientInfo, Implementation};
use rmcp::transport::streamable_http_client::StreamableHttpClientTransportConfig;
use rmcp::transport::StreamableHttpClientTransport;
use rmcp::ServiceExt;

const API_TOKEN: &str = "e2e-api-secret";
const MCP_TOKEN: &str = "e2e-mcp-secret";
const API_PORT: u16 = 8791;
const MCP_PORT: u16 = 8792;

struct ChildGuard(Child);

impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

struct SessionGuard {
    binary: String,
    session: String,
}

impl Drop for SessionGuard {
    fn drop(&mut self) {
        let _ = Command::new(&self.binary)
            .args(["delete-session", &self.session, "--force"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

fn env_binary(var: &str) -> Option<String> {
    std::env::var(var).ok().filter(|p| {
        let exists = std::path::Path::new(p).exists();
        if !exists {
            eprintln!("{var} points at '{p}', which does not exist");
        }
        exists
    })
}

/// Starts both servers and returns a connected, initialized MCP client.
/// Returns `None` (callers should skip) if the required binaries are not set.
async fn start_stack() -> Option<(
    ChildGuard,
    ChildGuard,
    rmcp::service::RunningService<rmcp::RoleClient, ClientInfo>,
)> {
    let zellij_binary = env_binary("ZELLIJ_API_E2E")?;
    let mcp_binary = env_binary("ZELLIJ_MCP_E2E")?;

    let api_server = ChildGuard(
        Command::new(&zellij_binary)
            .arg("api-server")
            .arg("--port")
            .arg(API_PORT.to_string())
            .arg("--token")
            .arg(API_TOKEN)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("could not start zellij api-server"),
    );

    let mcp_server = ChildGuard(
        Command::new(&mcp_binary)
            .arg("--port")
            .arg(MCP_PORT.to_string())
            .arg("--token")
            .arg(MCP_TOKEN)
            .arg("--api-url")
            .arg(format!("ws://127.0.0.1:{API_PORT}/api"))
            .arg("--api-token")
            .arg(API_TOKEN)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("could not start zellij-mcp"),
    );

    // Give both servers a moment to bind their ports.
    tokio::time::sleep(Duration::from_millis(800)).await;

    let config =
        StreamableHttpClientTransportConfig::with_uri(format!("http://127.0.0.1:{MCP_PORT}/mcp"))
            .auth_header(MCP_TOKEN);
    let transport = StreamableHttpClientTransport::from_config(config);
    let client_info = ClientInfo::new(
        ClientCapabilities::default(),
        Implementation::new("zellij-mcp-e2e", "0.0.1"),
    );
    let client = client_info
        .serve(transport)
        .await
        .expect("could not connect to the mcp server");

    Some((api_server, mcp_server, client))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn lists_every_tool() {
    let Some((_api, _mcp, client)) = start_stack().await else {
        eprintln!("skipping: set ZELLIJ_API_E2E and ZELLIJ_MCP_E2E to run this test");
        return;
    };

    let tools = client.list_all_tools().await.expect("could not list tools");
    let names: Vec<&str> = tools.iter().map(|t| t.name.as_ref()).collect();

    for expected in [
        "list_sessions",
        "create_session",
        "kill_session",
        "rename_session",
        "attach_session",
        "list_tabs",
        "create_tab",
        "focus_tab",
        "close_tab",
        "rename_tab",
        "list_panes",
        "create_pane",
        "focus_pane",
        "close_pane",
        "rename_pane",
        "resize_pane",
        "send_text",
        "send_keys",
        "send_mouse",
        "read_screen",
        "screen_history",
    ] {
        assert!(
            names.contains(&expected),
            "missing tool '{expected}', got {names:?}"
        );
    }

    client.cancel().await.ok();
}

/// Every tool that operates on an *existing* session must name that
/// parameter `session` — the same name every other tool in the surface
/// uses. `create_session` is the one legitimate exception: its `name` names
/// the session being brought into existence, not one being looked up.
///
/// Regression test for a real inconsistency: `kill_session` and
/// `rename_session` originally took `name` instead of `session`, so an
/// agent that had just learned the convention from any of the other 19
/// tools would get a confusing "missing field `session`" on exactly these
/// two — undiscoverable without reading the schema for each tool
/// individually.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn every_tool_names_its_session_parameter_consistently() {
    let Some((_api, _mcp, client)) = start_stack().await else {
        eprintln!("skipping: set ZELLIJ_API_E2E and ZELLIJ_MCP_E2E to run this test");
        return;
    };

    let tools = client.list_all_tools().await.expect("could not list tools");

    for tool in &tools {
        if tool.name == "create_session" || tool.name == "list_sessions" {
            continue;
        }
        let properties = tool
            .input_schema
            .get("properties")
            .and_then(|p| p.as_object());
        let has_session_field = properties.is_some_and(|p| p.contains_key("session"));
        assert!(
            has_session_field,
            "tool '{}' takes a session but doesn't name the parameter 'session': {:?}",
            tool.name, tool.input_schema
        );
    }

    client.cancel().await.ok();
}

/// MCP distinguishes two failure modes: a *tool-level* error
/// (`CallToolResult.is_error = true`, content the calling model can see and
/// react to) for "the request was valid but the operation failed", versus a
/// *protocol-level* error (a JSON-RPC `error` object) for "the server cannot
/// even route this". Every Zellij command failure — a missing session, a bad
/// key name — is the former: the tool ran, the underlying operation just
/// didn't succeed. Getting this backwards (returning every failure as a
/// protocol error) would mean the calling model often never sees why its
/// request failed, only that something broke.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn business_failures_are_tool_errors_not_protocol_errors() {
    let Some((_api, _mcp, client)) = start_stack().await else {
        eprintln!("skipping: set ZELLIJ_API_E2E and ZELLIJ_MCP_E2E to run this test");
        return;
    };

    // A well-formed request naming a session that does not exist: the tool
    // runs, the operation fails. Must come back as a *tool* error.
    let result = client
        .call_tool(
            CallToolRequestParams::new("list_tabs").with_arguments(
                serde_json::json!({"session": "definitely-not-a-real-session"})
                    .as_object()
                    .cloned()
                    .unwrap(),
            ),
        )
        .await
        .expect("call_tool itself should not fail — the failure belongs in the result");
    assert_eq!(
        result.is_error,
        Some(true),
        "a missing session should be a tool-level error: {result:?}"
    );
    assert!(
        tool_result_text(&result).contains("not running"),
        "the error content should explain what went wrong: {result:?}"
    );

    // A request naming a tool that does not exist at all: the server cannot
    // route it. This one genuinely is a protocol error.
    let routing_error = client
        .call_tool(CallToolRequestParams::new("this_tool_does_not_exist"))
        .await;
    assert!(
        routing_error.is_err(),
        "an unknown tool name should be a protocol-level error, not a tool result"
    );

    client.cancel().await.ok();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn drives_a_real_session_end_to_end() {
    let Some((_api, _mcp, client)) = start_stack().await else {
        eprintln!("skipping: set ZELLIJ_API_E2E and ZELLIJ_MCP_E2E to run this test");
        return;
    };

    let zellij_binary = env_binary("ZELLIJ_API_E2E").unwrap();
    let session = format!("zellij-mcp-e2e-{}", std::process::id());
    let _guard = SessionGuard {
        binary: zellij_binary,
        session: session.clone(),
    };

    // --- create_session -----------------------------------------------------
    let created = client
        .call_tool(
            CallToolRequestParams::new("create_session").with_arguments(
                serde_json::json!({"name": session, "rows": 24, "cols": 80})
                    .as_object()
                    .cloned()
                    .unwrap(),
            ),
        )
        .await
        .expect("create_session failed");
    assert_ne!(
        created.is_error,
        Some(true),
        "create_session reported an error: {created:?}"
    );

    // --- attach_session: the composite "get oriented" tool ------------------
    let attached = client
        .call_tool(
            CallToolRequestParams::new("attach_session").with_arguments(
                serde_json::json!({"session": session})
                    .as_object()
                    .cloned()
                    .unwrap(),
            ),
        )
        .await
        .expect("attach_session failed");
    assert_ne!(
        attached.is_error,
        Some(true),
        "attach_session reported an error: {attached:?}"
    );
    let attached_text = tool_result_text(&attached);
    let attached_json: serde_json::Value =
        serde_json::from_str(&attached_text).expect("attach_session did not return JSON");
    assert!(
        attached_json["tabs"]
            .as_array()
            .is_some_and(|t| !t.is_empty()),
        "attach_session should report at least one tab: {attached_json}"
    );
    assert!(
        attached_json["screens"]
            .as_array()
            .is_some_and(|s| !s.is_empty()),
        "attach_session should include at least one screen: {attached_json}"
    );

    // --- send_text + read_screen: drive it and see what happened ------------
    let marker = "zellij_mcp_e2e_marker";
    let typed = client
        .call_tool(
            CallToolRequestParams::new("send_text").with_arguments(
                serde_json::json!({"session": session, "text": format!("echo {marker}\n")})
                    .as_object()
                    .cloned()
                    .unwrap(),
            ),
        )
        .await
        .expect("send_text failed");
    assert_ne!(
        typed.is_error,
        Some(true),
        "send_text reported an error: {typed:?}"
    );

    // Poll for the shell to echo and run the command — a fixed sleep here was
    // flaky under load (render lands asynchronously after the write
    // completes), so retry instead of gambling on one delay being long enough.
    let mut screen_text = String::new();
    for _ in 0..10 {
        tokio::time::sleep(Duration::from_millis(300)).await;
        let screen = client
            .call_tool(
                CallToolRequestParams::new("read_screen").with_arguments(
                    serde_json::json!({"session": session, "pane_id": "terminal_0"})
                        .as_object()
                        .cloned()
                        .unwrap(),
                ),
            )
            .await
            .expect("read_screen failed");
        assert_ne!(
            screen.is_error,
            Some(true),
            "read_screen reported an error: {screen:?}"
        );
        screen_text = tool_result_text(&screen);
        if screen_text.contains(marker) {
            break;
        }
    }
    assert!(
        screen_text.contains(marker),
        "read_screen should show what was typed; got:\n{screen_text}"
    );

    // --- kill_session ---------------------------------------------------------
    let killed = client
        .call_tool(
            CallToolRequestParams::new("kill_session").with_arguments(
                serde_json::json!({"session": session})
                    .as_object()
                    .cloned()
                    .unwrap(),
            ),
        )
        .await
        .expect("kill_session failed");
    assert_ne!(
        killed.is_error,
        Some(true),
        "kill_session reported an error: {killed:?}"
    );

    client.cancel().await.ok();
}

/// `send_text`/`send_keys` default to the focused pane when `pane_id` is
/// omitted; `read_screen`/`screen_history` originally didn't offer that same
/// default and required `pane_id` even though there's exactly one sensible
/// thing "the pane" means with nothing else specified. Regression test for
/// making them symmetric with the input tools.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn read_screen_and_screen_history_default_to_the_focused_pane() {
    let Some((_api, _mcp, client)) = start_stack().await else {
        eprintln!("skipping: set ZELLIJ_API_E2E and ZELLIJ_MCP_E2E to run this test");
        return;
    };

    let zellij_binary = env_binary("ZELLIJ_API_E2E").unwrap();
    let session = format!("zellij-mcp-e2e-focused-pane-{}", std::process::id());
    let _guard = SessionGuard {
        binary: zellij_binary,
        session: session.clone(),
    };

    let created = client
        .call_tool(
            CallToolRequestParams::new("create_session").with_arguments(
                serde_json::json!({"name": session, "rows": 24, "cols": 80})
                    .as_object()
                    .cloned()
                    .unwrap(),
            ),
        )
        .await
        .expect("create_session failed");
    assert_ne!(
        created.is_error,
        Some(true),
        "create_session failed: {created:?}"
    );

    let read_without_pane_id = client
        .call_tool(
            CallToolRequestParams::new("read_screen").with_arguments(
                serde_json::json!({"session": session})
                    .as_object()
                    .cloned()
                    .unwrap(),
            ),
        )
        .await
        .expect("read_screen failed");
    assert_ne!(
        read_without_pane_id.is_error,
        Some(true),
        "read_screen with no pane_id should default to the focused pane, not error: \
         {read_without_pane_id:?}"
    );

    let history_without_pane_id = client
        .call_tool(
            CallToolRequestParams::new("screen_history").with_arguments(
                serde_json::json!({"session": session})
                    .as_object()
                    .cloned()
                    .unwrap(),
            ),
        )
        .await
        .expect("screen_history failed");
    assert_ne!(
        history_without_pane_id.is_error,
        Some(true),
        "screen_history with no pane_id should default to the focused pane, not error: \
         {history_without_pane_id:?}"
    );

    client
        .call_tool(
            CallToolRequestParams::new("kill_session").with_arguments(
                serde_json::json!({"session": session})
                    .as_object()
                    .cloned()
                    .unwrap(),
            ),
        )
        .await
        .ok();
    client.cancel().await.ok();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn rejects_the_wrong_token() {
    let Some(zellij_binary) = env_binary("ZELLIJ_API_E2E") else {
        eprintln!("skipping: set ZELLIJ_API_E2E and ZELLIJ_MCP_E2E to run this test");
        return;
    };
    let Some(mcp_binary) = env_binary("ZELLIJ_MCP_E2E") else {
        eprintln!("skipping: set ZELLIJ_API_E2E and ZELLIJ_MCP_E2E to run this test");
        return;
    };

    let port = MCP_PORT + 1;
    let api_port = API_PORT + 1;
    let _api = ChildGuard(
        Command::new(&zellij_binary)
            .arg("api-server")
            .arg("--port")
            .arg(api_port.to_string())
            .arg("--token")
            .arg(API_TOKEN)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("could not start zellij api-server"),
    );
    let _mcp = ChildGuard(
        Command::new(&mcp_binary)
            .arg("--port")
            .arg(port.to_string())
            .arg("--token")
            .arg(MCP_TOKEN)
            .arg("--api-url")
            .arg(format!("ws://127.0.0.1:{api_port}/api"))
            .arg("--api-token")
            .arg(API_TOKEN)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("could not start zellij-mcp"),
    );
    tokio::time::sleep(Duration::from_millis(800)).await;

    let config =
        StreamableHttpClientTransportConfig::with_uri(format!("http://127.0.0.1:{port}/mcp"))
            .auth_header("not-the-right-token");
    let transport = StreamableHttpClientTransport::from_config(config);
    let client_info = ClientInfo::new(
        ClientCapabilities::default(),
        Implementation::new("zellij-mcp-e2e", "0.0.1"),
    );
    let result = client_info.serve(transport).await;
    assert!(
        result.is_err(),
        "connecting with the wrong bearer token must be refused"
    );
}

fn tool_result_text(result: &rmcp::model::CallToolResult) -> String {
    result
        .content
        .iter()
        .filter_map(|block| match block {
            rmcp::model::ContentBlock::Text(text) => Some(text.text.clone()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}
