pub mod action;
pub mod command;
pub mod event;
pub mod plugin_permission;
pub mod file;
pub mod input_mode;
pub mod key;
pub mod message;
pub mod plugin_command;
pub mod plugin_ids;
pub mod resize;
pub mod style;
pub mod generated_api {
    include!(concat!(env!("OUT_DIR"), "/generated_plugin_api.rs"));
}
