//! MCP server exposing the Zellij remote-control WebSocket API (`zellij-api`)
//! as tools an MCP client (an LLM agent) can call — list sessions, attach to
//! one, drive it, read its screen. See `tools.rs` for the tool surface and
//! `REMOTE_API.md` at the repository root for the underlying protocol this
//! wraps.
//!
//! This process is a *client* of that WebSocket API, not a reimplementation
//! of it — it talks to an already-running `zellij api-server` the same way
//! `zellij-api/examples/drive.rs` does, and adds nothing to the session/pane
//! control surface itself.

mod tools;
mod ws_client;

use std::net::{IpAddr, SocketAddr};

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::middleware::{self, Next};
use axum::response::IntoResponse;
use clap::Parser;
use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use rmcp::transport::streamable_http_server::{StreamableHttpServerConfig, StreamableHttpService};

use tools::ZellijTools;
use ws_client::ApiClient;

/// MCP server exposing Zellij remote-control tools.
#[derive(Parser, Debug)]
struct Args {
    /// Address to bind the MCP server to.
    #[clap(long, default_value = "127.0.0.1")]
    bind: String,

    /// Port to listen on.
    #[clap(short, long, default_value = "8788")]
    port: u16,

    /// Token MCP clients must present as `Authorization: Bearer <token>`.
    /// Without it the server is open to anyone who can reach the port.
    #[clap(long, env = "ZELLIJ_MCP_TOKEN")]
    token: Option<String>,

    /// WebSocket URL of the underlying zellij-api server this MCP server
    /// drives.
    #[clap(
        long,
        default_value = "ws://127.0.0.1:8787/api",
        env = "ZELLIJ_API_URL"
    )]
    api_url: String,

    /// Token for the underlying zellij-api server.
    #[clap(long, env = "ZELLIJ_API_TOKEN")]
    api_token: String,
}

#[derive(Clone)]
struct AuthState {
    token: Option<String>,
}

/// Constant-time bearer-token check — see `zellij-api`'s `tokens_match` for
/// why a plain `==` on the presented token would leak timing information
/// about how much of it was correct.
fn tokens_match(expected: &str, presented: &str) -> bool {
    let (expected, presented) = (expected.as_bytes(), presented.as_bytes());
    if expected.len() != presented.len() {
        return false;
    }
    expected
        .iter()
        .zip(presented)
        .fold(0u8, |differences, (a, b)| differences | (a ^ b))
        == 0
}

async fn require_token(
    State(state): State<AuthState>,
    headers: HeaderMap,
    request: axum::extract::Request,
    next: Next,
) -> axum::response::Response {
    let Some(expected) = &state.token else {
        return next.run(request).await;
    };
    let presented = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));
    match presented {
        Some(presented) if tokens_match(expected, presented) => next.run(request).await,
        _ => (StatusCode::UNAUTHORIZED, "invalid or missing bearer token").into_response(),
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    if args.token.is_none() {
        eprintln!(
            "WARNING: starting without a token. Anyone who can reach the MCP port can drive your \
             Zellij sessions through it."
        );
        eprintln!("         Pass --token <secret> (or set ZELLIJ_MCP_TOKEN) to require one.");
    }

    let api = ApiClient::new(&args.api_url, &args.api_token);

    let session_manager = std::sync::Arc::new(LocalSessionManager::default());
    let service = StreamableHttpService::new(
        move || Ok(ZellijTools::new(api.clone())),
        session_manager,
        StreamableHttpServerConfig::default(),
    );

    let auth_state = AuthState { token: args.token };
    let router = axum::Router::new()
        .nest_service("/mcp", service)
        .route_layer(middleware::from_fn_with_state(auth_state, require_token))
        .route("/health", axum::routing::get(|| async { "ok" }));

    let ip: IpAddr = args
        .bind
        .parse()
        .map_err(|e| anyhow::anyhow!("invalid --bind address '{}': {}", args.bind, e))?;
    let addr = SocketAddr::new(ip, args.port);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    log::info!("zellij mcp server listening on http://{}/mcp", addr);

    axum::serve(listener, router)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}
