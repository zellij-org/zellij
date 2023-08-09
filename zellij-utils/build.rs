use prost_build;
use std::fs;

fn main() {
    let mut prost_build = prost_build::Config::new();
    prost_build.include_file("generated_plugin_api.rs");
    let mut proto_files = vec![];
    for entry in fs::read_dir("src/plugin_api").unwrap() {
        let entry_path = entry.unwrap().path();
        if entry_path.is_file() {
            if let Some(extension) = entry_path.extension() {
                if extension == "proto" {
                    proto_files.push(entry_path.display().to_string())
                }
            }
        }
    }
    prost_build
        .compile_protos(&proto_files, &["src/plugin_api"])
        .unwrap();
}
