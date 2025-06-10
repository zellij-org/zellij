use crate::web_client::utils::{parse_cookies, token_is_valid};
use axum::{
    extract::{Query, Request},
    http::{HeaderMap, HeaderValue, StatusCode},
    middleware::Next,
    response::Response,
};
use axum_extra::extract::cookie::{Cookie, SameSite};
use std::collections::HashMap;

pub async fn auth_middleware(
    headers: HeaderMap,
    Query(params): Query<HashMap<String, String>>,
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let cookies = parse_cookies(&request);
    let header_token = headers
        .get("authorization")
        .and_then(|h| h.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "))
        .or_else(|| params.get("token").map(|s| s.as_str()));
    let cookie_token = cookies.get("auth_token").cloned();
    let (token, should_set_cookie) = match (header_token, &cookie_token) {
        (Some(header_tok), _) => {
            let remember_me = headers
                .get("x-remember-me")
                .and_then(|h| h.to_str().ok())
                .map(|s| s == "true")
                .unwrap_or(false);
            (header_tok.to_owned(), remember_me)
        },
        (None, Some(cookie_tok)) => {
            (cookie_tok.to_owned(), false)
        },
        (None, None) => return Err(StatusCode::UNAUTHORIZED),
    };
    if !token_is_valid(&token) {
        return Err(StatusCode::UNAUTHORIZED);
    };

    let mut response = next.run(request).await;

    if should_set_cookie {
        let cookie = Cookie::build(("auth_token", token))
            .http_only(true)
            .secure(true)
            .same_site(SameSite::Strict)
            .max_age(time::Duration::hours(24 * 30))
            .path("/")
            .build();
        if let Ok(cookie_header) = HeaderValue::from_str(&cookie.to_string()) {
            response.headers_mut().insert("set-cookie", cookie_header);
        }
    }

    Ok(response)
}
