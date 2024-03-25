pub mod action;
pub mod command;
pub mod event;
pub mod file;
pub mod input_mode;
pub mod key;
pub mod message;
pub mod pipe_message;
pub mod plugin_command;
pub mod plugin_ids;
pub mod plugin_permission;
pub mod resize;
pub mod style;
// NOTE: This code is currently out of order.
// Refer to [the PR introducing this change][1] to learn more about the reasons.
// TL;DR: When running `cargo release --dry-run` the build-script in zellij-utils is not executed
//        for unknown reasons, causing compilation to fail. To make a new release possible in the
//        meantime, we decided to temporarily include the protobuf plugin API definitions
//        statically.
//
// [1]: https://github.com/zellij-org/zellij/pull/2711#issuecomment-1695015818
//pub mod generated_api {
//    include!(concat!(env!("OUT_DIR"), "/generated_plugin_api.rs"));
//}
pub mod generated_api {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/assets/prost/generated_plugin_api.rs"
    ));
}
