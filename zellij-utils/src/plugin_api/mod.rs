pub mod key;
pub mod event;
pub mod action;
pub mod style;
pub mod file;
pub mod command;
pub mod message;
pub mod input_mode;
pub mod resize;
pub mod plugin_ids;
pub mod plugin_command;
pub mod generated_api {
    include!(concat!(env!("OUT_DIR"), "/generated_plugin_api.rs"));
}
