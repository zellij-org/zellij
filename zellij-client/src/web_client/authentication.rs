use crate::web_client::utils::parse_cookies;
use axum::body::Body;
use axum::http::header::SET_COOKIE;
use axum::{extract::Request, http::StatusCode, middleware::Next, response::Response};
use axum_extra::extract::cookie::{Cookie, SameSite};
use zellij_utils::web_authentication_tokens::{hash_token, session_token_status};

#[derive(Clone)]
pub struct SessionTokenHash(pub String);

#[derive(Clone, Copy)]
pub struct IsReadOnly(pub bool);

pub async fn auth_middleware(request: Request, next: Next) -> Result<Response, StatusCode> {
    let cookies = parse_cookies(&request);

    let session_token = match cookies.get("session_token") {
        Some(token) => token.clone(),
        None => return Err(StatusCode::UNAUTHORIZED),
    };

    // the token lookups open the sqlite database, which must not block the
    // async workers that serve every other connection
    let lookup = {
        let session_token = session_token.clone();
        tokio::task::spawn_blocking(move || session_token_status(&session_token)).await
    };
    let lookup = match lookup {
        Ok(lookup) => lookup,
        Err(_) => return Err(StatusCode::INTERNAL_SERVER_ERROR),
    };

    match lookup {
        Ok(Some(is_read_only)) => {
            // Compute session token hash for client ownership verification
            let session_token_hash = hash_token(&session_token);

            // Store in request extensions for downstream handlers
            let mut request = request;
            request.extensions_mut().insert(IsReadOnly(is_read_only));
            request
                .extensions_mut()
                .insert(SessionTokenHash(session_token_hash));

            let response = next.run(request).await;
            Ok(response)
        },
        Ok(None) | Err(_) => {
            // revoke session_token as if it exists it's no longer valid
            let mut response = Response::builder()
                .status(StatusCode::UNAUTHORIZED)
                .body(Body::empty())
                .unwrap();

            // Clear both secure and non-secure versions
            // in case the user was on http before and is now on https
            // or vice versa
            let clear_cookies = [
                Cookie::build(("session_token", ""))
                    .http_only(true)
                    .secure(false)
                    .same_site(SameSite::Strict)
                    .path("/")
                    .max_age(time::Duration::seconds(0))
                    .build(),
                Cookie::build(("session_token", ""))
                    .http_only(true)
                    .secure(true)
                    .same_site(SameSite::Strict)
                    .path("/")
                    .max_age(time::Duration::seconds(0))
                    .build(),
            ];

            for cookie in clear_cookies {
                response
                    .headers_mut()
                    .append(SET_COOKIE, cookie.to_string().parse().unwrap());
            }

            Ok(response)
        },
    }
}
