use super::config::connection_timeout;
use super::RemoteTransport;
use isahc::prelude::*;
use isahc::{
    config::{Dialer, RedirectPolicy},
    http::{header::LOCATION, Method, Uri},
    AsyncBody, HttpClient, Request, Response,
};
use std::collections::BTreeMap;
use std::net::Ipv4Addr;
use std::path::Path;
use std::sync::{Arc, Mutex};
use url::Url;

const MAX_SAME_ORIGIN_REDIRECTS: usize = 10;

pub fn create_http_client(
    ca_cert: Option<&Path>,
    insecure: bool,
) -> Result<HttpClient, isahc::Error> {
    create_http_client_with_transport(ca_cert, insecure, None)
}

fn create_http_client_with_transport(
    ca_cert: Option<&Path>,
    insecure: bool,
    transport: Option<&RemoteTransport>,
) -> Result<HttpClient, isahc::Error> {
    let mut builder = HttpClient::builder()
        // Redirects are followed manually below so credentials and cookies are
        // never sent to a different origin.
        .redirect_policy(RedirectPolicy::None)
        .timeout(connection_timeout());

    if insecure {
        eprintln!(
            "WARNING: TLS certificate validation is disabled. This connection is NOT secure."
        );
        builder = builder.ssl_options(
            isahc::config::SslOption::DANGER_ACCEPT_INVALID_CERTS
                | isahc::config::SslOption::DANGER_ACCEPT_INVALID_HOSTS,
        );
    } else if let Some(ca_path) = ca_cert {
        builder = builder.ssl_ca_certificate(isahc::config::CaCertificate::file(ca_path));
    }

    if let Some(transport) = transport {
        match transport {
            RemoteTransport::Tcp(local_port) => {
                builder = builder.dial(Dialer::ip_socket((Ipv4Addr::LOCALHOST, *local_port)));
            },
            #[cfg(unix)]
            RemoteTransport::Unix(local_socket_path) => {
                builder = builder.dial(Dialer::unix_socket(local_socket_path));
            },
        }
    }

    builder.build()
}

pub struct HttpClientWithCookies {
    client: HttpClient,
    cookies: Arc<Mutex<BTreeMap<String, String>>>,
}

impl HttpClientWithCookies {
    pub fn new(ca_cert: Option<&Path>, insecure: bool) -> Result<Self, isahc::Error> {
        Self::new_with_transport(ca_cert, insecure, None)
    }

    pub(crate) fn new_with_transport(
        ca_cert: Option<&Path>,
        insecure: bool,
        transport: Option<&RemoteTransport>,
    ) -> Result<Self, isahc::Error> {
        Ok(Self {
            client: create_http_client_with_transport(ca_cert, insecure, transport)?,
            cookies: Arc::new(Mutex::new(BTreeMap::new())),
        })
    }

    pub async fn send_with_cookies<T: Into<Request<Vec<u8>>>>(
        &self,
        request: T,
    ) -> Result<Response<AsyncBody>, isahc::Error> {
        let mut request = request.into();

        for redirect_count in 0..=MAX_SAME_ORIGIN_REDIRECTS {
            let (parts, body) = request.into_parts();
            let template = RequestTemplate::from_parts(&parts, &body);
            let response = self.send_once(Request::from_parts(parts, body)).await?;

            let Some(redirect_uri) = same_origin_redirect(&template.uri, &response) else {
                return Ok(response);
            };
            if redirect_count == MAX_SAME_ORIGIN_REDIRECTS {
                return Ok(response);
            }

            let mut redirect_request = template.into_request();
            *redirect_request.uri_mut() = redirect_uri;
            if matches!(response.status().as_u16(), 301..=303) {
                *redirect_request.method_mut() = Method::GET;
                *redirect_request.body_mut() = Vec::new();
                redirect_request.headers_mut().remove("content-length");
                redirect_request.headers_mut().remove("content-type");
            }
            // 307 and 308 preserve the original method and body.
            request = redirect_request;
        }

        unreachable!("same-origin redirect loop must return a response")
    }

    async fn send_once(
        &self,
        mut req: Request<Vec<u8>>,
    ) -> Result<Response<AsyncBody>, isahc::Error> {
        // Add cookies to request
        if let Ok(cookies) = self.cookies.lock() {
            if let Some(cookie_header) = cookie_header_value(&cookies) {
                req.headers_mut().insert("cookie", cookie_header);
            }
        }

        let response = self.client.send_async(req).await?;

        // Extract and store cookies from response
        for set_cookie_header in response.headers().get_all("set-cookie").iter() {
            if let Ok(cookie_str) = set_cookie_header.to_str() {
                self.parse_and_store_cookies(cookie_str);
            }
        }

        Ok(response)
    }

    fn parse_and_store_cookies(&self, cookie_header: &str) {
        if let Ok(mut cookies) = self.cookies.lock() {
            // The first pair is the cookie; the rest are Set-Cookie
            // attributes and must not become independent cookies.
            let Some(cookie_pair) = cookie_header.split(';').next() else {
                return;
            };
            let Some((name, value)) = cookie_pair.split_once('=') else {
                return;
            };
            let name = name.trim();
            let value = value.trim();
            if is_valid_cookie_pair(name, value) {
                cookies.insert(name.to_owned(), value.to_owned());
            }
        }
    }

    pub fn get_cookie_header(&self) -> Option<String> {
        if let Ok(cookies) = self.cookies.lock() {
            return cookie_header_value(&cookies)
                .and_then(|header| header.to_str().ok().map(str::to_owned));
        }
        None
    }

    /// Extract a specific cookie value
    pub fn get_cookie(&self, name: &str) -> Option<String> {
        if let Ok(cookies) = self.cookies.lock() {
            return cookies.get(name).cloned();
        }
        None
    }

    /// Pre-populate a cookie (for saved session tokens)
    pub fn set_cookie(&self, name: String, value: String) {
        if let Ok(mut cookies) = self.cookies.lock() {
            if is_valid_cookie_pair(&name, &value) {
                cookies.insert(name, value);
            } else {
                log::warn!("ignoring invalid pre-populated cookie");
            }
        }
    }
}

fn same_origin_redirect(request_uri: &Uri, response: &Response<AsyncBody>) -> Option<Uri> {
    if !response.status().is_redirection() {
        return None;
    }

    let location = response.headers().get(LOCATION)?.to_str().ok()?;
    let current = Url::parse(request_uri.to_string().as_str()).ok()?;
    let mut redirect = current.join(location).ok()?;
    if !same_origin(&current, &redirect) {
        return None;
    }
    redirect.set_fragment(None);
    redirect.as_str().parse().ok()
}

fn same_origin(current: &Url, redirect: &Url) -> bool {
    matches!(current.scheme(), "http" | "https")
        && current.scheme() == redirect.scheme()
        && current.host_str() == redirect.host_str()
        && current.port_or_known_default() == redirect.port_or_known_default()
}

struct RequestTemplate {
    method: Method,
    uri: Uri,
    version: isahc::http::Version,
    headers: isahc::http::HeaderMap,
    body: Vec<u8>,
}

impl RequestTemplate {
    fn from_parts(parts: &isahc::http::request::Parts, body: &[u8]) -> Self {
        Self {
            method: parts.method.clone(),
            uri: parts.uri.clone(),
            version: parts.version,
            headers: parts.headers.clone(),
            body: body.to_vec(),
        }
    }

    fn into_request(self) -> Request<Vec<u8>> {
        let mut request = Request::new(self.body);
        *request.method_mut() = self.method;
        *request.uri_mut() = self.uri;
        *request.version_mut() = self.version;
        *request.headers_mut() = self.headers;
        request
    }
}

fn cookie_header_value(cookies: &BTreeMap<String, String>) -> Option<isahc::http::HeaderValue> {
    if cookies.is_empty() {
        return None;
    }

    let cookie_header = cookies
        .iter()
        .map(|(name, value)| format!("{name}={value}"))
        .collect::<Vec<_>>()
        .join("; ");
    cookie_header.parse().ok()
}

fn is_valid_cookie_pair(name: &str, value: &str) -> bool {
    !name.is_empty()
        && name.bytes().all(|byte| {
            byte.is_ascii_graphic()
                && !matches!(
                    byte,
                    b'"' | b'('
                        | b')'
                        | b','
                        | b'/'
                        | b':'
                        | b';'
                        | b'<'
                        | b'='
                        | b'>'
                        | b'?'
                        | b'@'
                        | b'['
                        | b'\\'
                        | b']'
                        | b'{'
                        | b'}'
                )
        })
        && value.bytes().all(
            |byte| matches!(byte, 0x21 | 0x23..=0x2B | 0x2D..=0x3A | 0x3C..=0x5B | 0x5D..=0x7E),
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    use isahc::AsyncReadResponseExt;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::{TcpListener, TcpStream};

    async fn read_request(stream: &mut TcpStream) -> String {
        let mut request = Vec::new();
        let mut buffer = [0; 1024];
        while !request.windows(4).any(|window| window == b"\r\n\r\n") {
            let bytes_read = stream.read(&mut buffer).await.unwrap();
            assert_ne!(bytes_read, 0, "HTTP request ended before its headers");
            request.extend_from_slice(&buffer[..bytes_read]);
        }
        String::from_utf8(request).unwrap()
    }

    #[tokio::test]
    async fn follows_same_origin_redirects_with_cookies() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = tokio::spawn(async move {
            let (mut first, _) = listener.accept().await.unwrap();
            assert!(read_request(&mut first).await.starts_with("POST /start "));
            first
                .write_all(
                    b"HTTP/1.1 302 Found\r\nLocation: /finish\r\nSet-Cookie: first=one; Path=/\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                )
                .await
                .unwrap();

            let (mut second, _) = listener.accept().await.unwrap();
            let request = read_request(&mut second).await;
            assert!(request.starts_with("GET /finish "));
            assert!(request
                .lines()
                .any(|line| line.eq_ignore_ascii_case("cookie: first=one")));
            second
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok")
                .await
                .unwrap();
        });

        let client = HttpClientWithCookies::new(None, false).unwrap();
        let mut response = client
            .send_with_cookies(
                Request::post(format!("http://127.0.0.1:{port}/start"))
                    .body(Vec::new())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status().as_u16(), 200);
        assert_eq!(response.text().await.unwrap(), "ok");
        server.await.unwrap();
    }

    #[tokio::test]
    async fn does_not_follow_cross_origin_redirects() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let _ = read_request(&mut stream).await;
            let response = format!(
                "HTTP/1.1 302 Found\r\nLocation: http://localhost:{port}/finish\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        });

        let client = HttpClientWithCookies::new(None, false).unwrap();
        let response = client
            .send_with_cookies(
                Request::get(format!("http://127.0.0.1:{port}/start"))
                    .body(Vec::new())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status().as_u16(), 302);
        server.await.unwrap();
    }

    #[tokio::test]
    async fn sends_http_requests_through_a_tcp_transport() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            assert!(read_request(&mut stream).await.starts_with("GET /test "));
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok")
                .await
                .unwrap();
        });

        let transport = RemoteTransport::Tcp(port);
        let client =
            HttpClientWithCookies::new_with_transport(None, false, Some(&transport)).unwrap();
        let mut response = client
            .send_with_cookies(
                Request::get("http://remote.example:8082/test")
                    .body(Vec::new())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status().as_u16(), 200);
        assert_eq!(response.text().await.unwrap(), "ok");
        server.await.unwrap();
    }

    #[cfg(unix)]
    use std::io::Write;
    #[cfg(unix)]
    use std::os::unix::net::UnixListener;

    #[cfg(unix)]
    #[tokio::test]
    async fn sends_http_requests_through_a_unix_socket() {
        let socket_directory = tempfile::tempdir().unwrap();
        let socket_path = socket_directory.path().join("http.sock");
        let listener = UnixListener::bind(&socket_path).unwrap();
        let server_thread = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nSet-Cookie: first=one; Path=/\r\nSet-Cookie: second=two; Path=/\r\n\r\nok",
                )
                .unwrap();
        });

        let transport = RemoteTransport::Unix(socket_path.clone());
        let client =
            HttpClientWithCookies::new_with_transport(None, false, Some(&transport)).unwrap();
        let mut response = client
            .send_with_cookies(
                Request::get("http://localhost/test")
                    .body(Vec::new())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status().as_u16(), 200);
        assert_eq!(response.text().await.unwrap(), "ok");
        assert_eq!(client.get_cookie("first"), Some("one".to_owned()));
        assert_eq!(client.get_cookie("second"), Some("two".to_owned()));
        server_thread.join().unwrap();
    }

    #[test]
    fn ignores_invalid_prepopulated_cookie_values() {
        let client = HttpClientWithCookies::new(None, false).unwrap();
        client.set_cookie("session_token".to_owned(), "bad\nvalue".to_owned());
        client.set_cookie("session_token".to_owned(), "bad; injected=true".to_owned());

        assert_eq!(client.get_cookie("session_token"), None);
        assert_eq!(client.get_cookie_header(), None);
    }

    #[test]
    fn stores_only_the_cookie_pair_from_set_cookie() {
        let client = HttpClientWithCookies::new(None, false).unwrap();
        client.parse_and_store_cookies("session_token=good; Path=/; Partitioned");

        assert_eq!(client.get_cookie("session_token"), Some("good".to_owned()));
        assert_eq!(client.get_cookie("Partitioned"), None);
    }
}
