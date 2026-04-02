use super::config::{WS_CONTROL_ENDPOINT, WS_TERMINAL_ENDPOINT};
use super::http_client::HttpClientWithCookies;
use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::TcpStream;
use tokio_tungstenite::WebSocketStream;

// -- MaybeTls stream enum -------------------------------------------------

/// A TCP stream that may or may not be wrapped in TLS.
pub enum MaybeTls {
    Plain(TcpStream),
    Tls(tokio_rustls::client::TlsStream<TcpStream>),
}

impl AsyncRead for MaybeTls {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        match self.get_mut() {
            MaybeTls::Plain(s) => Pin::new(s).poll_read(cx, buf),
            MaybeTls::Tls(s) => Pin::new(s).poll_read(cx, buf),
        }
    }
}

impl AsyncWrite for MaybeTls {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        match self.get_mut() {
            MaybeTls::Plain(s) => Pin::new(s).poll_write(cx, buf),
            MaybeTls::Tls(s) => Pin::new(s).poll_write(cx, buf),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        match self.get_mut() {
            MaybeTls::Plain(s) => Pin::new(s).poll_flush(cx),
            MaybeTls::Tls(s) => Pin::new(s).poll_flush(cx),
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        match self.get_mut() {
            MaybeTls::Plain(s) => Pin::new(s).poll_shutdown(cx),
            MaybeTls::Tls(s) => Pin::new(s).poll_shutdown(cx),
        }
    }
}

// -- NoVerifier (for --insecure mode) --------------------------------------

#[derive(Debug)]
struct NoVerifier;

impl rustls::client::danger::ServerCertVerifier for NoVerifier {
    fn verify_server_cert(
        &self,
        _end_entity: &rustls_pki_types::CertificateDer<'_>,
        _intermediates: &[rustls_pki_types::CertificateDer<'_>],
        _server_name: &rustls_pki_types::ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls_pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &rustls_pki_types::CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &rustls_pki_types::CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        rustls::crypto::aws_lc_rs::default_provider()
            .signature_verification_algorithms
            .supported_schemes()
    }
}

// -- TLS config builder ----------------------------------------------------

fn build_tls_config(
    ca_cert: Option<&Path>,
    insecure: bool,
) -> Result<Arc<rustls::ClientConfig>, Box<dyn std::error::Error>> {
    let provider = Arc::new(rustls::crypto::aws_lc_rs::default_provider());

    if insecure {
        let config = rustls::ClientConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()?
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(NoVerifier))
            .with_no_client_auth();
        Ok(Arc::new(config))
    } else if let Some(ca_path) = ca_cert {
        let ca_pem = std::fs::read(ca_path)?;
        let mut cursor = std::io::Cursor::new(ca_pem);
        let certs: Vec<rustls_pki_types::CertificateDer<'static>> =
            rustls_pemfile::certs(&mut cursor)
                .filter_map(|r| r.ok())
                .collect();
        let mut root_store = rustls::RootCertStore::empty();
        for cert in certs {
            root_store.add(cert)?;
        }
        let config = rustls::ClientConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()?
            .with_root_certificates(root_store)
            .with_no_client_auth();
        Ok(Arc::new(config))
    } else {
        let native_certs = rustls_native_certs::load_native_certs();
        for err in &native_certs.errors {
            log::warn!("Error loading native certificate: {}", err);
        }
        let mut root_store = rustls::RootCertStore::empty();
        root_store.add_parsable_certificates(native_certs.certs);
        let config = rustls::ClientConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()?
            .with_root_certificates(root_store)
            .with_no_client_auth();
        Ok(Arc::new(config))
    }
}

// -- WebSocket connection helpers ------------------------------------------

async fn connect_ws(
    request: tokio_tungstenite::tungstenite::http::Request<()>,
    host: &str,
    port: u16,
    tls_config: Option<Arc<rustls::ClientConfig>>,
) -> Result<WebSocketStream<MaybeTls>, Box<dyn std::error::Error>> {
    let tcp_stream = TcpStream::connect((host, port)).await?;

    let stream = if let Some(config) = tls_config {
        let connector = tokio_rustls::TlsConnector::from(config);
        let server_name = rustls_pki_types::ServerName::try_from(host.to_string())?;
        let tls_stream = connector.connect(server_name, tcp_stream).await?;
        MaybeTls::Tls(tls_stream)
    } else {
        MaybeTls::Plain(tcp_stream)
    };

    let (ws_stream, _response) =
        tokio_tungstenite::client_async_with_config(request, stream, None).await?;
    Ok(ws_stream)
}

// -- Public API ------------------------------------------------------------

pub struct WebSocketConnections {
    pub terminal_ws: WebSocketStream<MaybeTls>,
    pub control_ws: WebSocketStream<MaybeTls>,
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
    let parsed_url = url::Url::parse(server_base_url)?;
    let host = parsed_url
        .host_str()
        .ok_or("no host in server URL")?
        .to_string();
    let port = parsed_url
        .port_or_known_default()
        .ok_or("no port in server URL")?;
    let is_tls = parsed_url.scheme() == "https";

    let ws_protocol = if is_tls { "wss" } else { "ws" };
    let base_host = format!("{}:{}", host, port);

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

    // Build WebSocket requests with cookies
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

    // Build TLS config (only for wss://)
    let tls_config = if is_tls {
        Some(build_tls_config(ca_cert, insecure)?)
    } else {
        None
    };

    // Connect to both WebSockets
    let terminal_ws = connect_ws(terminal_request, &host, port, tls_config.clone()).await?;
    let control_ws = connect_ws(control_request, &host, port, tls_config).await?;

    Ok(WebSocketConnections {
        terminal_ws,
        control_ws,
        web_client_id: web_client_id.to_owned(),
    })
}
