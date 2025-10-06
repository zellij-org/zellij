mod auth;
mod config;
mod http_client;
mod websockets;

#[cfg(test)]
mod unit;

pub use websockets::WebSocketConnections;

use crate::os_input_output::ClientOsApi;
use crate::RemoteClientError;
use tokio::runtime::Runtime;
use zellij_utils::remote_session_tokens;

// In tests, only attempt once (no retries) to avoid interactive prompts
// In production, allow up to 3 attempts (initial + 2 retries)
#[cfg(test)]
const MAX_AUTH_ATTEMPTS: u32 = 1;

#[cfg(not(test))]
const MAX_AUTH_ATTEMPTS: u32 = 3;

/// Attach to a remote Zellij session via HTTP(S)
///
/// This function handles the complete authentication flow including:
/// - URL validation
/// - Session token management (--forget, --token flags)
/// - Trying saved session tokens
/// - Interactive authentication with retry logic
/// - Saving session tokens when --remember is used
///
/// Returns WebSocketConnections on success
pub fn attach_to_remote_session(
    runtime: &Runtime,
    _os_input: Box<dyn ClientOsApi>,
    remote_session_url: &str,
    token: Option<String>,
    remember: bool,
    forget: bool,
) -> Result<WebSocketConnections, RemoteClientError> {
    // Extract server URL for token management
    let server_url = extract_server_url(remote_session_url)?;

    // Handle --forget flag
    if forget {
        let _ = remote_session_tokens::delete_session_token(&server_url);
    }

    // If --token provided, delete saved session token
    if token.is_some() {
        let _ = remote_session_tokens::delete_session_token(&server_url);
    }

    if token.is_none() {
        if let Some(connections) =
            try_to_connect_with_saved_session_token(runtime, remote_session_url, &server_url)?
        {
            return Ok(connections);
        }
    }

    // Normal auth flow with retry logic
    authenticate_with_retry(runtime, remote_session_url, token, remember)
}

/// Try to connect using a saved session token
/// Returns Ok(Some(connections)) on success, Ok(None) if should retry with auth
fn try_to_connect_with_saved_session_token(
    runtime: &Runtime,
    remote_session_url: &str,
    server_url: &str,
) -> Result<Option<WebSocketConnections>, RemoteClientError> {
    if let Ok(Some(saved_session_token)) = remote_session_tokens::get_session_token(server_url) {
        // we have a saved session token, let's try to authenticate with it
        match runtime.block_on(async move {
            remote_attach_with_session_token(remote_session_url, &saved_session_token).await
        }) {
            Ok(connections) => {
                return Ok(Some(connections));
            },
            Err(RemoteClientError::SessionTokenExpired) => {
                // Session expired - delete and return to retry
                let _ = remote_session_tokens::delete_session_token(server_url);
                eprintln!("Session expired, please re-authenticate");
                return Ok(None);
            },
            Err(e) => {
                return Err(e);
            },
        }
    }
    Ok(None)
}

fn authenticate_with_retry(
    runtime: &Runtime,
    remote_session_url: &str,
    initial_token: Option<String>,
    remember: bool,
) -> Result<WebSocketConnections, RemoteClientError> {
    use dialoguer::{Confirm, Password};

    let mut attempt = 0;
    let mut current_token = initial_token;

    loop {
        attempt += 1;

        let auth_token = match &current_token {
            Some(t) => t.clone(),
            None => Password::new()
                .with_prompt("Enter authentication token")
                .interact()
                .map_err(|e| RemoteClientError::IoError(e))?,
        };

        match runtime
            .block_on(async move { remote_attach(remote_session_url, &auth_token, remember).await })
        {
            Ok((connections, session_token_opt)) => {
                // Save session token if we got one
                if let Some(session_token) = session_token_opt {
                    let server_url = extract_server_url(remote_session_url)?;
                    let _ = remote_session_tokens::save_session_token(&server_url, &session_token);
                }
                return Ok(connections);
            },
            Err(RemoteClientError::InvalidAuthToken) => {
                eprintln!("Invalid authentication token");

                if attempt >= MAX_AUTH_ATTEMPTS {
                    eprintln!(
                        "Maximum authentication attempts ({}) exceeded.",
                        MAX_AUTH_ATTEMPTS
                    );
                    return Err(RemoteClientError::InvalidAuthToken);
                }

                match Confirm::new()
                    .with_prompt("Try again?")
                    .default(true)
                    .interact()
                {
                    Ok(true) => {
                        current_token = None;
                        continue;
                    },
                    Ok(false) => {
                        return Err(RemoteClientError::InvalidAuthToken);
                    },
                    Err(e) => {
                        return Err(RemoteClientError::IoError(e));
                    },
                }
            },
            Err(e) => {
                return Err(e);
            },
        }
    }
}

async fn remote_attach(
    server_url: &str,
    auth_token: &str,
    remember_me: bool,
) -> Result<(websockets::WebSocketConnections, Option<String>), RemoteClientError> {
    let server_base_url = extract_server_url(server_url)?;
    let session_name = extract_session_name(server_url)?;
    let (web_client_id, http_client, session_token) =
        auth::authenticate(&server_base_url, auth_token, remember_me).await?;
    let connections = websockets::establish_websocket_connections(
        &web_client_id,
        &http_client,
        &server_base_url,
        &session_name,
    )
    .await
    .map_err(|e| RemoteClientError::ConnectionFailed(e.to_string()))?;
    Ok((connections, session_token))
}

async fn remote_attach_with_session_token(
    server_url: &str,
    session_token: &str,
) -> Result<websockets::WebSocketConnections, RemoteClientError> {
    let server_base_url = extract_server_url(server_url)?;
    let session_name = extract_session_name(server_url)?;
    let (web_client_id, http_client) =
        auth::validate_session_token(&server_base_url, session_token).await?;
    let connections = websockets::establish_websocket_connections(
        &web_client_id,
        &http_client,
        &server_base_url,
        &session_name,
    )
    .await
    .map_err(|e| RemoteClientError::ConnectionFailed(e.to_string()))?;
    Ok(connections)
}

pub fn extract_server_url(full_url: &str) -> Result<String, RemoteClientError> {
    let parsed = url::Url::parse(full_url)?;
    let mut base_url = parsed.clone();
    base_url.set_path("");
    base_url.set_query(None);
    base_url.set_fragment(None);
    Ok(base_url.to_string().trim_end_matches('/').to_string())
}

fn extract_session_name(server_url: &str) -> Result<String, RemoteClientError> {
    let parsed_url = url::Url::parse(server_url)?;
    let path = parsed_url.path();
    // Extract session name from path (everything after the first /)
    if path.len() > 1 && path.starts_with('/') {
        Ok(path[1..].trim_end_matches('/').to_string())
    } else {
        Ok(String::new())
    }
}
