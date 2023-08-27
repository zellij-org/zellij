use prost_build;
use std::fs;

fn main() {
    let mut prost_build = prost_build::Config::new();
    let out_dir = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/prost");
    let out_dir = std::path::Path::new(out_dir);

    std::fs::create_dir_all(out_dir).unwrap();
    prost_build.out_dir(out_dir);
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
