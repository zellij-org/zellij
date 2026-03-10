use super::config::{WS_CONTROL_ENDPOINT, WS_TERMINAL_ENDPOINT};
use super::http_client::HttpClientWithCookies;
use std::path::Path;
use tokio::net::TcpStream;
use tokio_tungstenite::{connect_async_tls_with_config, Connector, MaybeTlsStream, WebSocketStream};

pub struct WebSocketConnections {
    pub terminal_ws: WebSocketStream<MaybeTlsStream<TcpStream>>,
    pub control_ws: WebSocketStream<MaybeTlsStream<TcpStream>>,
    pub web_client_id: String,
}

impl std::fmt::Debug for WebSocketConnections {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WebSocketConnections")
            .field("web_client_id", &self.web_client_id)
            .finish()
    }
}

pub async fn establish_websocket_connections(
    web_client_id: &str,
    http_client: &HttpClientWithCookies,
    server_base_url: &str,
    session_name: &str,
    ca_cert: Option<&Path>,
    insecure: bool,
) -> Result<WebSocketConnections, Box<dyn std::error::Error>> {
    let ws_protocol = if server_base_url.starts_with("https") {
        "wss"
    } else {
        "ws"
    };
    let base_host = server_base_url
        .replace("https://", "")
        .replace("http://", "");

    let terminal_url = if session_name.is_empty() {
        format!(
            "{}://{}{WS_TERMINAL_ENDPOINT}?web_client_id={}",
            ws_protocol,
            base_host,
            urlencoding::encode(web_client_id)
        )
    } else {
        format!(
            "{}://{}{WS_TERMINAL_ENDPOINT}/{}?web_client_id={}",
            ws_protocol,
            base_host,
            urlencoding::encode(session_name),
            urlencoding::encode(web_client_id)
        )
    };

    let control_url = format!("{}://{}{WS_CONTROL_ENDPOINT}", ws_protocol, base_host);

    log::info!("Connecting to terminal WebSocket: {}", terminal_url);
    log::info!("Connecting to control WebSocket: {}", control_url);

    // Create WebSocket request with cookies
    let mut terminal_request = tokio_tungstenite::tungstenite::http::Request::builder()
        .uri(&terminal_url)
        .header("Host", &base_host)
        .header("Upgrade", "websocket")
        .header("Connection", "Upgrade")
        .header(
            "Sec-WebSocket-Key",
            tokio_tungstenite::tungstenite::handshake::client::generate_key(),
        )
        .header("Sec-WebSocket-Version", "13");

    let mut control_request = tokio_tungstenite::tungstenite::http::Request::builder()
        .uri(&control_url)
        .header("Host", &base_host)
        .header("Upgrade", "websocket")
        .header("Connection", "Upgrade")
        .header(
            "Sec-WebSocket-Key",
            tokio_tungstenite::tungstenite::handshake::client::generate_key(),
        )
        .header("Sec-WebSocket-Version", "13");

    // Add cookies if available
    if let Some(cookie_header) = http_client.get_cookie_header() {
        terminal_request = terminal_request.header("Cookie", &cookie_header);
        control_request = control_request.header("Cookie", &cookie_header);
    }

    let terminal_request = terminal_request.body(())?;
    let control_request = control_request.body(())?;

    // Build TLS connector based on ca_cert/insecure settings
    let connector = if insecure {
        let tls_connector = native_tls::TlsConnector::builder()
            .danger_accept_invalid_certs(true)
            .danger_accept_invalid_hostnames(true)
            .build()?;
        Some(Connector::NativeTls(tls_connector))
    } else if let Some(ca_path) = ca_cert {
        let ca_pem = std::fs::read(ca_path)?;
        let ca_cert = native_tls::Certificate::from_pem(&ca_pem)?;
        let tls_connector = native_tls::TlsConnector::builder()
            .add_root_certificate(ca_cert)
            .build()?;
        Some(Connector::NativeTls(tls_connector))
    } else {
        None
    };

    // Connect to both WebSockets concurrently
    let (terminal_ws, _) = connect_async_tls_with_config(terminal_request, None, false, connector.clone()).await?;
    let (control_ws, _) = connect_async_tls_with_config(control_request, None, false, connector).await?;

    Ok(WebSocketConnections {
        terminal_ws,
        control_ws,
        web_client_id: web_client_id.to_owned(),
    })
}
