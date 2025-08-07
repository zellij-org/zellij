mod control_message;

mod authentication;
mod connection_manager;
mod http_handlers;
mod ipc_listener;
mod message_handlers;
mod server_listener;
mod session_management;
mod types;
mod utils;
mod websocket_handlers;

use std::{
    net::{IpAddr, Ipv4Addr},
    path::PathBuf,
    sync::{Arc, Mutex},
    thread,
};

use axum::{
    middleware,
    routing::{any, get, post},
    Router,
};

use axum_server::tls_rustls::RustlsConfig;
use axum_server::Handle;

use daemonize::{self, Outcome};
use nix::sys::stat::{umask, Mode};

use interprocess::unnamed_pipe::pipe;
use std::io::{prelude::*, BufRead, BufReader};
use tokio::runtime::Runtime;
use tower_http::cors::CorsLayer;
use zellij_utils::input::{
    config::{watch_config_file_changes, Config},
    options::Options,
};

use authentication::auth_middleware;
use http_handlers::{
    create_new_client, get_static_asset, login_handler, serve_html, version_handler,
};
use ipc_listener::listen_to_web_server_instructions;

use types::{
    AppState, ClientOsApiFactory, ConnectionTable, RealClientOsApiFactory, RealSessionManager,
    SessionManager,
};
use utils::should_use_https;
use uuid::Uuid;
use websocket_handlers::{ws_handler_control, ws_handler_terminal};

use zellij_utils::{
    consts::WEBSERVER_SOCKET_PATH,
    web_server_commands::{
        create_webserver_sender, send_webserver_instruction, InstructionForWebServer,
    },
};

pub fn start_web_client(
    config: Config,
    config_options: Options,
    config_file_path: Option<PathBuf>,
    run_daemonized: bool,
    custom_ip: Option<IpAddr>,
    custom_port: Option<u16>,
    custom_server_cert: Option<PathBuf>,
    custom_server_key: Option<PathBuf>,
) {
    std::panic::set_hook({
        Box::new(move |info| {
            let thread = thread::current();
            let thread = thread.name().unwrap_or("unnamed");
            let msg = match info.payload().downcast_ref::<&'static str>() {
                Some(s) => Some(*s),
                None => info.payload().downcast_ref::<String>().map(|s| &**s),
            }
            .unwrap_or("An unexpected error occurred!");
            log::error!(
                "Thread {} panicked: {}, location {:?}",
                thread,
                msg,
                info.location()
            );
            eprintln!("{}", msg);
            std::process::exit(2);
        })
    });
    let web_server_ip = custom_ip.unwrap_or_else(|| {
        config_options
            .web_server_ip
            .unwrap_or_else(|| IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)))
    });
    let web_server_port =
        custom_port.unwrap_or_else(|| config_options.web_server_port.unwrap_or_else(|| 8082));
    let web_server_cert = custom_server_cert.or_else(|| config.options.web_server_cert.clone());
    let web_server_key = custom_server_key.or_else(|| config.options.web_server_key.clone());
    let has_https_certificate = web_server_cert.is_some() && web_server_key.is_some();

    if let Err(e) = should_use_https(
        web_server_ip,
        has_https_certificate,
        config.options.enforce_https_for_localhost.unwrap_or(false),
    ) {
        eprintln!("{}", e);
        std::process::exit(2);
    };
    let (runtime, listener, tls_config) = if run_daemonized {
        daemonize_web_server(
            web_server_ip,
            web_server_port,
            web_server_cert,
            web_server_key,
        )
    } else {
        let runtime = Runtime::new().unwrap();
        let listener = runtime.block_on(async move {
            std::net::TcpListener::bind(format!("{}:{}", web_server_ip, web_server_port))
        });
        let tls_config = match (web_server_cert, web_server_key) {
            (Some(web_server_cert), Some(web_server_key)) => {
                let tls_config = runtime.block_on(async move {
                    RustlsConfig::from_pem_file(
                        PathBuf::from(web_server_cert),
                        PathBuf::from(web_server_key),
                    )
                    .await
                });
                let tls_config = match tls_config {
                    Ok(tls_config) => tls_config,
                    Err(e) => {
                        eprintln!("{}", e);
                        std::process::exit(2);
                    },
                };
                Some(tls_config)
            },
            (None, None) => None,
            _ => {
                eprintln!("Must specify both web_server_cert and web_server_key");
                std::process::exit(2);
            },
        };

        match listener {
            Ok(listener) => {
                println!(
                    "Web Server started on {} port {}",
                    web_server_ip, web_server_port
                );
                (runtime, listener, tls_config)
            },
            Err(e) => {
                eprintln!("{}", e);
                std::process::exit(2);
            },
        }
    };

    runtime.block_on(serve_web_client(
        config,
        config_options,
        config_file_path,
        listener,
        tls_config,
        None,
        None,
    ));
}

#[allow(dead_code)]
async fn listen_to_config_file_changes(config_file_path: PathBuf, instance_id: &str) {
    let socket_path = WEBSERVER_SOCKET_PATH.join(instance_id);

    watch_config_file_changes(config_file_path, move |new_config| {
        let socket_path = socket_path.clone();
        async move {
            if let Ok(mut sender) = create_webserver_sender(&socket_path.to_string_lossy()) {
                let _ = send_webserver_instruction(
                    &mut sender,
                    InstructionForWebServer::ConfigWrittenToDisk(new_config),
                );
                drop(sender);
            }
        }
    })
    .await;
}

pub async fn serve_web_client(
    config: Config,
    config_options: Options,
    config_file_path: Option<PathBuf>,
    listener: std::net::TcpListener,
    rustls_config: Option<RustlsConfig>,
    session_manager: Option<Arc<dyn SessionManager>>,
    client_os_api_factory: Option<Arc<dyn ClientOsApiFactory>>,
) {
    let Some(config_file_path) = config_file_path.or_else(|| Config::default_config_file_path())
    else {
        log::error!("Failed to find default config file path");
        return;
    };
    let connection_table = Arc::new(Mutex::new(ConnectionTable::default()));
    let server_handle = Handle::new();
    let session_manager = session_manager.unwrap_or_else(|| Arc::new(RealSessionManager));
    let client_os_api_factory =
        client_os_api_factory.unwrap_or_else(|| Arc::new(RealClientOsApiFactory));

    // we use a short version here to bypass macos socket path length limitations
    // since there likely aren't going to be more than a handful of web instances on the same
    // machine listening to the same ipc socket path, the collision risk here is extremely low
    let id: String = Uuid::new_v4()
        .simple()
        .to_string()
        .chars()
        .take(5)
        .collect();

    #[cfg(not(test))]
    tokio::spawn({
        let config_file_path = config_file_path.clone();
        let id_string = format!("{}", id);
        async move {
            listen_to_config_file_changes(config_file_path, &id_string).await;
        }
    });

    let is_https = rustls_config.is_some();
    let state = AppState {
        connection_table: connection_table.clone(),
        config: Arc::new(Mutex::new(config)),
        config_options,
        config_file_path,
        session_manager,
        client_os_api_factory,
        is_https,
    };

    tokio::spawn({
        let server_handle = server_handle.clone();
        let state = state.clone();
        async move {
            listen_to_web_server_instructions(server_handle, state, &format!("{}", id)).await;
        }
    });

    let app = Router::new()
        .route("/ws/control", any(ws_handler_control))
        .route("/ws/terminal", any(ws_handler_terminal))
        .route("/ws/terminal/{session}", any(ws_handler_terminal))
        .route("/session", post(create_new_client))
        .route_layer(middleware::from_fn(auth_middleware))
        .route("/", get(serve_html))
        .route("/{session}", get(serve_html))
        .route("/assets/{*path}", get(get_static_asset))
        .route("/command/login", post(login_handler))
        .route("/info/version", get(version_handler))
        .layer(CorsLayer::permissive()) // TODO: configure properly
        .with_state(state);

    match rustls_config {
        Some(rustls_config) => {
            let _ = axum_server::from_tcp_rustls(listener, rustls_config)
                .handle(server_handle)
                .serve(app.into_make_service())
                .await;
        },
        None => {
            let _ = axum_server::from_tcp(listener)
                .handle(server_handle)
                .serve(app.into_make_service())
                .await;
        },
    }
}

fn daemonize_web_server(
    web_server_ip: IpAddr,
    web_server_port: u16,
    web_server_cert: Option<PathBuf>,
    web_server_key: Option<PathBuf>,
) -> (Runtime, std::net::TcpListener, Option<RustlsConfig>) {
    let (mut exit_message_tx, exit_message_rx) = pipe().unwrap();
    let (mut exit_status_tx, mut exit_status_rx) = pipe().unwrap();
    let current_umask = umask(Mode::all());
    umask(current_umask);
    let web_server_key = web_server_key.clone();
    let web_server_cert = web_server_cert.clone();
    let daemonization_outcome = daemonize::Daemonize::new()
        .working_directory(std::env::current_dir().unwrap())
        .umask(current_umask.bits() as u32)
        .privileged_action(
            move || -> Result<(Runtime, std::net::TcpListener, Option<RustlsConfig>), String> {
                let runtime = Runtime::new().map_err(|e| e.to_string())?;
                let tls_config = match (web_server_cert, web_server_key) {
                    (Some(web_server_cert), Some(web_server_key)) => {
                        let tls_config = runtime.block_on(async move {
                            RustlsConfig::from_pem_file(
                                PathBuf::from(web_server_cert),
                                PathBuf::from(web_server_key),
                            )
                            .await
                        });
                        let tls_config = match tls_config {
                            Ok(tls_config) => tls_config,
                            Err(e) => {
                                return Err(e.to_string());
                            },
                        };
                        Some(tls_config)
                    },
                    (None, None) => None,
                    _ => {
                        return Err(
                            "Must specify both web_server_cert and web_server_key".to_owned()
                        )
                    },
                };

                let listener = runtime.block_on(async move {
                    std::net::TcpListener::bind(format!("{}:{}", web_server_ip, web_server_port))
                });
                listener
                    .map(|listener| (runtime, listener, tls_config))
                    .map_err(|e| e.to_string())
            },
        )
        .execute();
    match daemonization_outcome {
        Outcome::Parent(Ok(parent)) => {
            if parent.first_child_exit_code == 0 {
                let mut buf = [0; 1];
                match exit_status_rx.read_exact(&mut buf) {
                    Ok(_) => {
                        let exit_status = buf.iter().next().copied().unwrap_or(0) as i32;
                        let mut message = String::new();
                        let mut reader = BufReader::new(exit_message_rx);
                        let _ = reader.read_line(&mut message);
                        if exit_status == 0 {
                            println!("{}", message.trim());
                        } else {
                            eprintln!("{}", message.trim());
                        }
                        std::process::exit(exit_status);
                    },
                    Err(e) => {
                        eprintln!("{}", e);
                        std::process::exit(2);
                    },
                }
            } else {
                std::process::exit(parent.first_child_exit_code);
            }
        },
        Outcome::Child(Ok(child)) => match child.privileged_action_result {
            Ok(listener_and_runtime) => {
                let _ = writeln!(
                    exit_message_tx,
                    "Web Server started on {} port {}",
                    web_server_ip, web_server_port
                );
                let _ = exit_status_tx.write_all(&[0]);
                listener_and_runtime
            },
            Err(e) => {
                let _ = exit_status_tx.write_all(&[2]);
                let _ = writeln!(exit_message_tx, "{}", e);
                std::process::exit(2);
            },
        },
        _ => {
            eprintln!("Failed to start server");
            std::process::exit(2);
        },
    }
}

#[cfg(test)]
#[path = "./unit/web_client_tests.rs"]
mod web_client_tests;
