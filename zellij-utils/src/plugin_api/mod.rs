pub mod action;
pub mod command;
pub mod event;
pub mod file;
pub mod input_mode;
pub mod key;
pub mod message;
pub mod plugin_command;
pub mod plugin_ids;
pub mod plugin_permission;
pub mod resize;
pub mod style;
pub mod generated_api {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/assets/prost/generated_plugin_api.rs"
    ));
}
