#[cfg(not(target_family = "wasm"))]
#[allow(
    clippy::all,
    dead_code,
    mismatched_lifetime_syntaxes,
    unused_assignments,
    unused_imports
)]
pub mod termwiz;
