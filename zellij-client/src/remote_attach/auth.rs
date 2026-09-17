use super::config::{LOGIN_ENDPOINT, SESSION_ENDPOINT};
use super::http_client::HttpClientWithCookies;
use super::RemoteTransport;
use crate::RemoteClientError;
use isahc::{AsyncReadResponseExt, Request};
use serde::{Deserialize, Serialize};

#[derive(Serialize)]
struct LoginRequest {
    auth_token: String,
    remember_me: bool,
}

#[derive(Deserialize)]
pub struct SessionResponse {
    pub web_client_id: String,
}

fn session_url(server_base_url: &str, session_name: &str) -> String {
    if session_name.is_empty() {
        format!("{}{}", server_base_url, SESSION_ENDPOINT)
    } else {
        format!(
            "{}{}?session={}",
            server_base_url,
            SESSION_ENDPOINT,
            urlencoding::encode(session_name)
        )
    }
}

fn redirect_error(status: u16) -> RemoteClientError {
    RemoteClientError::ConnectionFailed(format!(
        "Server returned redirect status {status}; use the final remote URL directly"
    ))
}

fn session_token_status_error(status: u16) -> Option<RemoteClientError> {
    match status {
        401 | 300..=399 => Some(RemoteClientError::SessionTokenExpired),
        status if !(200..=299).contains(&status) => Some(RemoteClientError::ConnectionFailed(
            format!("Server returned status {status}"),
        )),
        _ => None,
    }
}

pub async fn authenticate(
    server_base_url: &str,
    auth_token: &str,
    remember_me: bool,
    session_name: &str,
    ca_cert: Option<&std::path::Path>,
    insecure: bool,
    transport: Option<&RemoteTransport>,
) -> Result<(String, HttpClientWithCookies, Option<String>), RemoteClientError> {
    let http_client = HttpClientWithCookies::new_with_transport(ca_cert, insecure, transport)
        .map_err(|e| RemoteClientError::Other(Box::new(e)))?;

    // Step 1: Login with auth token
    let login_url = format!("{}{}", server_base_url, LOGIN_ENDPOINT);

    let login_request = LoginRequest {
        auth_token: auth_token.to_string(),
        remember_me,
    };

    let response = http_client
        .send_with_cookies(
            Request::post(login_url)
                .header("Content-Type", "application/json")
                .header("User-Agent", "http-terminal-client/1.0")
                .header("Accept", "application/json")
                .body(
                    serde_json::to_vec(&login_request)
                        .map_err(|e| RemoteClientError::Other(Box::new(e)))?,
                )
                .map_err(|e| RemoteClientError::Other(Box::new(e)))?,
        )
        .await
        .map_err(|e| RemoteClientError::ConnectionFailed(e.to_string()))?;

    // Handle HTTP status codes
    match response.status().as_u16() {
        401 => return Err(RemoteClientError::InvalidAuthToken),
        status if (300..400).contains(&status) => return Err(redirect_error(status)),
        status if !response.status().is_success() => {
            return Err(RemoteClientError::ConnectionFailed(format!(
                "Server returned status {}",
                status
            )));
        },
        _ => {},
    }

    // Step 2: Get session/client ID
    let session_url = session_url(server_base_url, session_name);

    let mut session_response = http_client
        .send_with_cookies(
            Request::post(session_url)
                .header("Content-Type", "application/json")
                .header("User-Agent", "http-terminal-client/1.0")
                .header("Accept", "application/json")
                .body("{}".as_bytes().to_vec())
                .map_err(|e| RemoteClientError::Other(Box::new(e)))?,
        )
        .await
        .map_err(|e| RemoteClientError::ConnectionFailed(e.to_string()))?;

    // Handle session response
    match session_response.status().as_u16() {
        401 => return Err(RemoteClientError::Unauthorized),
        status if (300..400).contains(&status) => return Err(redirect_error(status)),
        status if !session_response.status().is_success() => {
            return Err(RemoteClientError::ConnectionFailed(format!(
                "Server returned status {}",
                status
            )));
        },
        _ => {},
    }

    let response_body = session_response
        .text()
        .await
        .map_err(|e| RemoteClientError::Other(Box::new(e)))?;
    let session_data: SessionResponse =
        serde_json::from_str(&response_body).map_err(|e| RemoteClientError::Other(Box::new(e)))?;

    // Extract session_token if remember_me was true
    let session_token = if remember_me {
        http_client.get_cookie("session_token")
    } else {
        None
    };

    Ok((session_data.web_client_id, http_client, session_token))
}

pub async fn validate_session_token(
    server_base_url: &str,
    session_token: &str,
    session_name: &str,
    ca_cert: Option<&std::path::Path>,
    insecure: bool,
    transport: Option<&RemoteTransport>,
) -> Result<(String, HttpClientWithCookies), RemoteClientError> {
    let http_client = HttpClientWithCookies::new_with_transport(ca_cert, insecure, transport)
        .map_err(|e| RemoteClientError::Other(Box::new(e)))?;

    // Pre-populate the session_token cookie
    http_client.set_cookie("session_token".to_string(), session_token.to_string());

    // Skip /login, go directly to /session endpoint
    let session_url = session_url(server_base_url, session_name);

    let mut session_response = http_client
        .send_with_cookies(
            Request::post(session_url)
                .header("Content-Type", "application/json")
                .header("User-Agent", "http-terminal-client/1.0")
                .header("Accept", "application/json")
                .body("{}".as_bytes().to_vec())
                .map_err(|e| RemoteClientError::Other(Box::new(e)))?,
        )
        .await
        .map_err(|e| RemoteClientError::ConnectionFailed(e.to_string()))?;

    let status = session_response.status().as_u16();
    if let Some(error) = session_token_status_error(status) {
        return Err(error);
    }

    let response_body = session_response
        .text()
        .await
        .map_err(|e| RemoteClientError::Other(Box::new(e)))?;
    let session_data: SessionResponse =
        serde_json::from_str(&response_body).map_err(|e| RemoteClientError::Other(Box::new(e)))?;
    Ok((session_data.web_client_id, http_client))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redirects_expire_saved_session_tokens() {
        assert!(matches!(
            session_token_status_error(302),
            Some(RemoteClientError::SessionTokenExpired)
        ));
    }
}
