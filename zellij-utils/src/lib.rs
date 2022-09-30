pub mod cli;
pub mod consts;
pub mod data;
pub mod envs;
pub mod input;
pub mod kdl;
pub mod pane_size;
pub mod position;
pub mod setup;
pub mod shared;

// The following modules can't be used when targeting wasm
#[cfg(not(target_family = "wasm"))]
pub mod channels; // Requires async_std
#[cfg(not(target_family = "wasm"))]
pub mod errors; // Requires async_std (via channels)
#[cfg(not(target_family = "wasm"))]
pub mod ipc; // Requires interprocess
#[cfg(not(target_family = "wasm"))]
pub mod logging; // Requires log4rs

#[cfg(not(target_family = "wasm"))]
pub use ::{
    anyhow, async_std, clap, interprocess, lazy_static, libc, nix, regex, serde, signal_hook,
    tempfile, termwiz, vte,
};
