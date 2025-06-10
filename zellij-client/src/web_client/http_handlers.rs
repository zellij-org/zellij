use crate::web_client::types::{AppState, CreateClientIdResponse, SendShutdownSignalResponse};
use crate::web_client::utils::{get_mime_type, parse_cookies};
use axum::{
    extract::{Path as AxumPath, Request, State},
    http::{header, StatusCode},
    response::{Html, IntoResponse},
    Json,
};
use include_dir;
use uuid::Uuid;
use zellij_utils::consts::VERSION;

const WEB_CLIENT_PAGE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/",
    "assets/index.html"
));

const ASSETS_DIR: include_dir::Dir<'_> = include_dir::include_dir!("$CARGO_MANIFEST_DIR/assets");

pub async fn serve_html(request: Request) -> Html<String> {
    let cookies = parse_cookies(&request);
    let is_authenticated = cookies.get("auth_token").is_some();
    let auth_value = if is_authenticated { "true" } else { "false" };
    let html = Html(WEB_CLIENT_PAGE.replace("IS_AUTHENTICATED", &format!("{}", auth_value)));
    html
}

pub async fn create_new_client(
    State(state): State<AppState>,
) -> Result<Json<CreateClientIdResponse>, (StatusCode, impl IntoResponse)> {
    let web_client_id = String::from(Uuid::new_v4());
    let os_input = state
        .client_os_api_factory
        .create_client_os_api()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(e.to_string())))?;

    state
        .connection_table
        .lock()
        .unwrap()
        .add_new_client(web_client_id.to_owned(), os_input);

    Ok(Json(CreateClientIdResponse { web_client_id }))
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

pub async fn send_shutdown_signal(State(state): State<AppState>) -> Json<SendShutdownSignalResponse> {
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        state.server_handle.shutdown();
    });
    Json(SendShutdownSignalResponse {
        status: "Ok".to_owned(),
    })
}
