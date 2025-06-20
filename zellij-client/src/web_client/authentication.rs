use crate::web_client::utils::parse_cookies;
use axum::body::Body;
use axum::http::header::SET_COOKIE;
use axum::{extract::Request, http::StatusCode, middleware::Next, response::Response};
use axum_extra::extract::cookie::{Cookie, SameSite};
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
        Ok(false) | Err(_) => {
            // revoke session_token as if it exists it's no longer valid
            let clear_cookie = Cookie::build(("session_token", ""))
                .http_only(true)
                .secure(true)
                .same_site(SameSite::Strict)
                .path("/")
                .max_age(time::Duration::seconds(0))
                .build();

            let mut response = Response::builder()
                .status(StatusCode::UNAUTHORIZED)
                .body(Body::empty())
                .unwrap();

            response
                .headers_mut()
                .insert(SET_COOKIE, clear_cookie.to_string().parse().unwrap());

            Ok(response)
        },
    }
}
