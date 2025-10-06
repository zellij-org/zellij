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
pub mod channels; // Requires async_std
#[cfg(not(target_family = "wasm"))]
pub mod common_path;
#[cfg(not(target_family = "wasm"))]
pub mod downloader; // Requires async_std
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
