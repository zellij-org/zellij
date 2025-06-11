use crate::web_client::utils::parse_cookies;
use axum::{extract::Request, http::StatusCode, middleware::Next, response::Response};
use zellij_utils::web_authentication_tokens::validate_session_token;

pub async fn auth_middleware(request: Request, next: Next) -> Result<Response, StatusCode> {
    let cookies = parse_cookies(&request);

    let session_token = match cookies.get("session_token") {
        Some(token) => token.clone(),
        None => return Err(StatusCode::UNAUTHORIZED),
    };

    match validate_session_token(&session_token) {
        Ok(true) => {
            let response = next.run(request).await;
            Ok(response)
        },
        Ok(false) | Err(_) => Err(StatusCode::UNAUTHORIZED),
    }
}
