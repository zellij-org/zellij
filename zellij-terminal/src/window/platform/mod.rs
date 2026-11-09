#[cfg_attr(not(target_os = "macos"), path = "stub/macos.rs")]
mod macos;
#[cfg_attr(not(freedesktop), path = "stub/wayland.rs")]
mod wayland;
#[cfg_attr(not(windows), path = "stub/windows.rs")]
mod windows;

pub(super) use macos::{
    activate_app, set_accessory, set_option_as_alt, set_see_through, with_option_as_alt,
};
pub(super) use wayland::{is_wayland, named, with_activation_token, Activation};
pub(super) use windows::{
    bring_opened_window_forward, created_hidden, show_cloaked, skip_taskbar,
    square_without_system_border, tray_work_area, uncloak, Shell, WindowMemory,
};
