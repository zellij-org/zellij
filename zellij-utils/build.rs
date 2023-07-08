use prost_build;

fn main() {
    prost_build::compile_protos(&["src/plugin_api/key.proto", "src/plugin_api/event.proto"],
                                &["src/"]).unwrap();
}

