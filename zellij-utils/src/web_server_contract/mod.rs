include!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/assets/prost_web_server/generated_web_server_api.rs"
));

mod protobuf_conversion;
