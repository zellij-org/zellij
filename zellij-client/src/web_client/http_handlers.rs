use crate::web_client::authentication::{IsReadOnly, SessionTokenHash};
use crate::web_client::control_message::SetConfigPayload;
use crate::web_client::types::{
    record_pending_welcome_session, AppState, ClientLimitExceeded, CreateClientIdResponse,
    LoginRequest, LoginResponse, SessionListResponse, SessionQuery,
};
use crate::web_client::utils::get_mime_type;
use axum::{
    extract::{ConnectInfo, Path as AxumPath, Query, State},
    http::{header, StatusCode},
    response::IntoResponse,
    Json,
};
use axum_extra::extract::cookie::{Cookie, SameSite};
use include_dir;
use std::net::SocketAddr;
use uuid::Uuid;
use zellij_utils::{
    consts::VERSION, sessions::generate_unique_session_name,
    web_authentication_tokens::create_session_token,
};

const ASSETS_DIR: include_dir::Dir<'_> = include_dir::include_dir!("$CARGO_MANIFEST_DIR/assets");

pub async fn serve_html() -> impl IntoResponse {
    match ASSETS_DIR.get_file("index.html") {
        Some(file) => (
            StatusCode::OK,
            [(header::CONTENT_TYPE, "text/html")],
            file.contents(),
        ),
        None => (
            StatusCode::INTERNAL_SERVER_ERROR,
            [(header::CONTENT_TYPE, "text/plain")],
            "index.html missing".as_bytes(),
        ),
    }
}

pub async fn login_handler(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Json(login_request): Json<LoginRequest>,
) -> impl IntoResponse {
    // this endpoint is unauthenticated and every attempt costs a database
    // lookup, so cap how fast any one address can hammer it
    let rate_limit_verdict = state.login_rate_limiter.lock().unwrap().check(peer.ip());
    if let Err(retry_after) = rate_limit_verdict {
        let retry_after_secs = retry_after.as_secs_f64().ceil().max(1.0) as u64;
        return (
            StatusCode::TOO_MANY_REQUESTS,
            [(header::RETRY_AFTER, retry_after_secs.to_string())],
            Json(LoginResponse {
                success: false,
                message: "Too many login attempts, try again later".to_string(),
            }),
        )
            .into_response();
    }

    let remember_me = login_request.remember_me.unwrap_or(false);
    let auth_token = login_request.auth_token;
    // opening the token database is blocking work
    let session_token =
        tokio::task::spawn_blocking(move || create_session_token(&auth_token, remember_me)).await;
    let session_token = match session_token {
        Ok(session_token) => session_token,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    match session_token {
        Ok(session_token) => {
            let is_https = state.is_https;
            let cookie = if remember_me {
                // Persistent cookie for remember_me
                Cookie::build(("session_token", session_token))
                    .http_only(true)
                    .secure(is_https)
                    .same_site(SameSite::Strict)
                    .path("/")
                    .max_age(time::Duration::weeks(4))
                    .build()
            } else {
                // Session cookie - NO max_age means it expires when browser closes/refreshes
                Cookie::build(("session_token", session_token))
                    .http_only(true)
                    .secure(is_https)
                    .same_site(SameSite::Strict)
                    .path("/")
                    .build()
            };

            let mut response = Json(LoginResponse {
                success: true,
                message: "Login successful".to_string(),
            })
            .into_response();

            if let Ok(cookie_header) = axum::http::HeaderValue::from_str(&cookie.to_string()) {
                response.headers_mut().insert("set-cookie", cookie_header);
            }

            response
        },
        Err(_) => (
            StatusCode::UNAUTHORIZED,
            Json(LoginResponse {
                success: false,
                message: "Invalid authentication token".to_string(),
            }),
        )
            .into_response(),
    }
}

pub async fn create_new_client(
    State(state): State<AppState>,
    Query(params): Query<SessionQuery>,
    request: axum::extract::Request,
) -> Result<Json<CreateClientIdResponse>, (StatusCode, impl IntoResponse)> {
    // Extract is_read_only from request extensions (set by auth middleware)
    let is_read_only = request
        .extensions()
        .get::<IsReadOnly>()
        .copied()
        .unwrap_or(IsReadOnly(true))
        .0;
    let session_token_hash = request
        .extensions()
        .get::<SessionTokenHash>()
        .cloned()
        .ok_or((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json("Missing session info".to_string()),
        ))?;

    // This request carries nothing of interest, but it is not exempt from the
    // body size limit just because no extractor reads the body (that is the
    // only thing `DefaultBodyLimit` applies to): read it, bounded, so that an
    // oversized one is refused instead of being left for the connection to
    // sort out.
    // (the only way reading fails here is the limit or the client going away)
    if axum::body::to_bytes(
        request.into_body(),
        crate::web_client::MAX_REQUEST_BODY_BYTES,
    )
    .await
    .is_err()
    {
        return Err((
            StatusCode::PAYLOAD_TOO_LARGE,
            Json("Request body too large".to_string()),
        ));
    }

    let web_client_id = String::from(Uuid::new_v4());
    let os_input = state
        .client_os_api_factory
        .create_client_os_api()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(e.to_string())))?;

    // an id is only handed out if it fits under the client limits
    let added = state.connection_table.lock().unwrap().try_add_new_client(
        web_client_id.to_owned(),
        os_input,
        is_read_only,
        session_token_hash.0,
    );
    match added {
        Ok(()) => {},
        Err(ClientLimitExceeded::TooManyClients) => {
            return Err((
                StatusCode::SERVICE_UNAVAILABLE,
                Json("Too many clients are connected".to_string()),
            ))
        },
        Err(ClientLimitExceeded::TooManyUnattachedClients) => {
            return Err((
                StatusCode::TOO_MANY_REQUESTS,
                Json("Too many clients are waiting to connect".to_string()),
            ))
        },
    }

    let session_name = match params.session.filter(|name| !name.is_empty()) {
        Some(session_name) => session_name,
        None => {
            let generated = match generate_unique_session_name() {
                Some(generated) => generated,
                None => {
                    state
                        .connection_table
                        .lock()
                        .unwrap()
                        .remove_client(&web_client_id);
                    return Err((
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json("Failed to generate unique session name".to_string()),
                    ));
                },
            };
            if params.welcome.unwrap_or(true) {
                record_pending_welcome_session(&state.pending_welcome_sessions, &generated);
            }
            generated
        },
    };

    let config = SetConfigPayload::from(&*state.config.lock().unwrap());

    Ok(Json(CreateClientIdResponse {
        web_client_id,
        is_read_only,
        session_name,
        config,
    }))
}

pub async fn list_sessions_handler(State(state): State<AppState>) -> Json<SessionListResponse> {
    let mut sessions = state.session_manager.list_sessions();
    sessions.sort_by(|a, b| a.name.cmp(&b.name));
    Json(SessionListResponse { sessions })
}

pub async fn get_static_asset(AxumPath(path): AxumPath<String>) -> impl IntoResponse {
    let path = path.trim_start_matches('/');

    match ASSETS_DIR.get_file(path) {
        None => (
            [(header::CONTENT_TYPE, "text/html")],
            "Not Found".as_bytes(),
        ),
        Some(file) => {
            let ext = file.path().extension().and_then(|ext| ext.to_str());
            let mime_type = get_mime_type(ext);
            ([(header::CONTENT_TYPE, mime_type)], file.contents())
        },
    }
}

pub async fn version_handler() -> &'static str {
    VERSION
}
