#![cfg(unix)]

pub mod client_screen;
pub mod fake_client_os_api;
pub mod fake_pty;
pub mod fake_server_os_api;
pub mod keys;
pub mod runner;
pub mod test_env;

pub use client_screen::{ClientScreen, CursorPosition, GridSnapshot};
pub use fake_pty::FakePtyHandle;
pub use runner::{normalized, TestClient, TestRunner, TestSession};
pub use zellij_utils::data::LayoutInfo;
pub use zellij_utils::pane_size::Size;

use std::time::Duration;

pub fn default_timeout() -> Duration {
    std::env::var("ZELLIJ_TEST_TIMEOUT_MS")
        .ok()
        .and_then(|v| v.parse().ok())
        .map(Duration::from_millis)
        .unwrap_or(Duration::from_secs(10))
}
