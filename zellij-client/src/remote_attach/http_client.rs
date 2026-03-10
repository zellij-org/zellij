use super::config::connection_timeout;
use isahc::prelude::*;
use isahc::{config::RedirectPolicy, AsyncBody, HttpClient, Request, Response};
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

pub fn create_http_client(
    ca_cert: Option<&Path>,
    insecure: bool,
) -> Result<HttpClient, isahc::Error> {
    let mut builder = HttpClient::builder()
        .redirect_policy(RedirectPolicy::Follow)
        .timeout(connection_timeout());

    if insecure {
        eprintln!("WARNING: TLS certificate validation is disabled. This connection is NOT secure.");
        builder = builder.ssl_options(isahc::config::SslOption::DANGER_ACCEPT_INVALID_CERTS);
    } else if let Some(ca_path) = ca_cert {
        builder = builder.ssl_ca_certificate(isahc::config::CaCertificate::file(ca_path));
    }

    builder.build()
}

pub struct HttpClientWithCookies {
    client: HttpClient,
    cookies: Arc<Mutex<HashMap<String, String>>>,
}

impl HttpClientWithCookies {
    pub fn new(
        ca_cert: Option<&Path>,
        insecure: bool,
    ) -> Result<Self, isahc::Error> {
        Ok(Self {
            client: create_http_client(ca_cert, insecure)?,
            cookies: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    pub async fn send_with_cookies<T: Into<Request<Vec<u8>>>>(
        &self,
        request: T,
    ) -> Result<Response<AsyncBody>, isahc::Error> {
        let mut req = request.into();

        // Add cookies to request
        if let Ok(cookies) = self.cookies.lock() {
            if !cookies.is_empty() {
                let cookie_header = cookies
                    .iter()
                    .map(|(k, v)| format!("{}={}", k, v))
                    .collect::<Vec<_>>()
                    .join("; ");
                req.headers_mut()
                    .insert("cookie", cookie_header.parse().unwrap());
            }
        }

        let response = self.client.send_async(req).await?;

        // Extract and store cookies from response
        if let Some(set_cookie_headers) = response.headers().get_all("set-cookie").iter().next() {
            if let Ok(cookie_str) = set_cookie_headers.to_str() {
                self.parse_and_store_cookies(cookie_str);
            }
        }

        Ok(response)
    }

    fn parse_and_store_cookies(&self, cookie_header: &str) {
        if let Ok(mut cookies) = self.cookies.lock() {
            // Simple cookie parsing - just extract name=value pairs
            for cookie_part in cookie_header.split(';') {
                let cookie_part = cookie_part.trim();
                if let Some((name, value)) = cookie_part.split_once('=') {
                    // Skip cookie attributes like Path, Domain, HttpOnly, etc.
                    if ![
                        "path", "domain", "httponly", "secure", "samesite", "expires", "max-age",
                    ]
                    .contains(&name.to_lowercase().as_str())
                    {
                        cookies.insert(name.trim().to_string(), value.trim().to_string());
                    }
                }
            }
        }
    }

    pub fn get_cookie_header(&self) -> Option<String> {
        if let Ok(cookies) = self.cookies.lock() {
            if !cookies.is_empty() {
                let cookie_header = cookies
                    .iter()
                    .map(|(k, v)| format!("{}={}", k, v))
                    .collect::<Vec<_>>()
                    .join("; ");
                return Some(cookie_header);
            }
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
            cookies.insert(name, value);
        }
    }
}
