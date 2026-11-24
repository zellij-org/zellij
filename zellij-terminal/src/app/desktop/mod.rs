pub mod bundle;
#[cfg(target_os = "linux")]
pub mod dbus;
pub mod icon;
#[cfg(target_os = "macos")]
pub mod macos_delegate;
#[cfg(target_os = "macos")]
pub mod macos_open;
pub mod notifier;
#[cfg(windows)]
pub mod platform_windows;
pub mod services;
pub mod tray;
