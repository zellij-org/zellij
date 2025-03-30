pub mod cli;
pub mod client_server_contract;
pub mod consts;
pub mod data;
pub mod envs;
pub mod errors;
pub mod home;
pub mod input;
pub mod kdl;
pub mod pane_size;
pub mod plugin_api;
pub mod position;
pub mod session_serialization;
pub mod setup;
pub mod shared;

// The following modules can't be used when targeting wasm
#[cfg(not(target_family = "wasm"))]
pub mod channels; // Requires tokio
#[cfg(not(target_family = "wasm"))]
pub mod common_path;
#[cfg(not(target_family = "wasm"))]
pub mod downloader; // Requires tokio
#[cfg(not(target_family = "wasm"))]
pub mod ipc; // Requires interprocess
#[cfg(not(target_family = "wasm"))]
pub mod logging; // Requires log4rs
#[cfg(all(not(target_family = "wasm"), feature = "web_server_capability"))]
pub mod remote_session_tokens;
#[cfg(not(target_family = "wasm"))]
pub mod sessions;
#[cfg(all(not(target_family = "wasm"), feature = "web_server_capability"))]
pub mod web_authentication_tokens;
#[cfg(all(not(target_family = "wasm"), feature = "web_server_capability"))]
pub mod web_server_commands;
#[cfg(all(not(target_family = "wasm"), feature = "web_server_capability"))]
pub mod web_server_contract;

// TODO(hartan): Remove this re-export for the next minor release.
pub use ::prost;

#[cfg(not(target_family = "wasm"))]
static ASYNC_RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
#[cfg(not(target_family = "wasm"))]
use std::sync::OnceLock;

#[cfg(not(target_family = "wasm"))]
pub(crate) fn async_runtime() -> tokio::runtime::Handle {
    match tokio::runtime::Handle::try_current() {
        Ok(handle) => handle.clone(),
        _ => {
            let runtime = ASYNC_RUNTIME.get_or_init(|| {
                tokio::runtime::Builder::new_multi_thread()
                    .worker_threads(4)
                    .thread_name("zellij utils async-runtime")
                    .enable_all()
                    .build()
                    .expect("Failed to create tokio runtime")
            });
            runtime.handle().clone()
        },
    }
}
