use prost_build;

fn main() {
    // TODO: include all .proto files dynamically somehow
    let mut prost_build = prost_build::Config::new();
    prost_build.include_file("generated_plugin_api.rs");
    prost_build.compile_protos(
        &[
            "src/plugin_api/key.proto",
            "src/plugin_api/event.proto",
            "src/plugin_api/file.proto",
            "src/plugin_api/command.proto",
            "src/plugin_api/message.proto",
            "src/plugin_api/input_mode.proto",
            "src/plugin_api/resize.proto",
            "src/plugin_api/plugin_ids.proto",
            "src/plugin_api/plugin_command.proto",
        ],
        &["src/plugin_api"]
    ).unwrap();
}

