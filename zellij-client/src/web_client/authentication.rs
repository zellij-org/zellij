use crate::web_client::utils::parse_cookies;
use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::header::SET_COOKIE;
use axum::{extract::Request, http::StatusCode, middleware::Next, response::Response};
use axum_extra::extract::cookie::{Cookie, SameSite};
use std::net::{IpAddr, SocketAddr};
use zellij_utils::web_authentication_tokens::{
    hash_token, is_session_token_read_only, validate_session_token,
};

#[derive(Clone)]
pub struct SessionTokenHash(pub String);

#[derive(Clone, Copy)]
pub struct IsReadOnly(pub bool);

#[derive(Clone, Copy)]
pub struct SkipAuthForLocalNetwork(pub bool);

fn is_local_network_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ipv4) => ipv4.is_loopback() || ipv4.is_private() || ipv4.is_link_local(),
        IpAddr::V6(ipv6) => {
            ipv6.is_loopback() || ipv6.is_unique_local() || ipv6.is_unicast_link_local()
        },
    }
}

pub async fn auth_middleware(request: Request, next: Next) -> Result<Response, StatusCode> {
    let skip_auth = request
        .extensions()
        .get::<SkipAuthForLocalNetwork>()
        .map(|s| s.0)
        .unwrap_or(false);

    if skip_auth {
        let peer_addr = request
            .extensions()
            .get::<ConnectInfo<SocketAddr>>()
            .map(|ci| ci.0);
        if let Some(addr) = peer_addr {
            if is_local_network_ip(addr.ip()) {
                let mut request = request;
                request.extensions_mut().insert(IsReadOnly(false));
                request
                    .extensions_mut()
                    .insert(SessionTokenHash(String::new()));
                let response = next.run(request).await;
                return Ok(response);
            }
        }
    }

    let cookies = parse_cookies(&request);

    let session_token = match cookies.get("session_token") {
        Some(token) => token.clone(),
        None => return Err(StatusCode::UNAUTHORIZED),
    };

    match validate_session_token(&session_token) {
        Ok(true) => {
            let is_read_only = is_session_token_read_only(&session_token).unwrap_or(true);

            let session_token_hash = hash_token(&session_token);

            let mut request = request;
            request.extensions_mut().insert(IsReadOnly(is_read_only));
            request
                .extensions_mut()
                .insert(SessionTokenHash(session_token_hash));

            let response = next.run(request).await;
            Ok(response)
        },
        Ok(false) | Err(_) => {
            let mut response = Response::builder()
                .status(StatusCode::UNAUTHORIZED)
                .body(Body::empty())
                .unwrap();

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
