use std::path::PathBuf;
use std::process::Command;

const PLUGIN: &str = "example-distribution-plugin";
const TARGET: &str = "wasm32-wasip1";

fn main() {
    println!("cargo:rerun-if-changed=plugin/src");
    println!("cargo:rerun-if-changed=plugin/Cargo.toml");
    println!("cargo:rerun-if-env-changed=EXAMPLE_DISTRIBUTION_PLUGIN_WASM");

    if let Ok(prebuilt) = std::env::var("EXAMPLE_DISTRIBUTION_PLUGIN_WASM") {
        println!("cargo:rustc-env=EXAMPLE_DISTRIBUTION_PLUGIN_WASM={}", prebuilt);
        return;
    }

    let out_dir = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR is set by cargo"));
    let target_dir = out_dir.join("plugin-target");
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_owned());

    let mut command = Command::new(cargo);
    command
        .arg("build")
        .arg("--release")
        .args(["--target", TARGET])
        .args(["--package", PLUGIN])
        .arg("--target-dir")
        .arg(&target_dir)
        .env_remove("CARGO_MAKEFLAGS")
        .env_remove("CARGO_ENCODED_RUSTFLAGS")
        .env_remove("RUSTFLAGS")
        .env_remove("CARGO_TARGET_DIR")
        .env_remove("CARGO_BUILD_TARGET");

    let status = command
        .status()
        .unwrap_or_else(|e| panic!("failed to run cargo to build '{}': {}", PLUGIN, e));

    if !status.success() {
        panic!(
            "failed to build '{}' for {}. If the target is missing, install it with:\n    \
             rustup target add {}",
            PLUGIN, TARGET, TARGET
        );
    }

    let wasm = target_dir
        .join(TARGET)
        .join("release")
        .join(format!("{}.wasm", PLUGIN.replace('-', "_")));
    let wasm = if wasm.exists() {
        wasm
    } else {
        target_dir
            .join(TARGET)
            .join("release")
            .join(format!("{}.wasm", PLUGIN))
    };

    assert!(
        wasm.exists(),
        "'{}' built successfully but no wasm artifact was found at '{}'",
        PLUGIN,
        wasm.display()
    );

    println!(
        "cargo:rustc-env=EXAMPLE_DISTRIBUTION_PLUGIN_WASM={}",
        wasm.display()
    );
}
