use std::time::Duration;

// API endpoints
pub const LOGIN_ENDPOINT: &str = "/command/login";
pub const SESSION_ENDPOINT: &str = "/session";
pub const WS_TERMINAL_ENDPOINT: &str = "/ws/terminal";
pub const WS_CONTROL_ENDPOINT: &str = "/ws/control";

// Connection settings
pub const CONNECTION_TIMEOUT_SECS: u64 = 30;

pub fn connection_timeout() -> Duration {
    Duration::from_secs(CONNECTION_TIMEOUT_SECS)
}
