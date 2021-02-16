use directories_next::ProjectDirs;
use std::{env::*, fs, path::Path, process::Command};
use structopt::clap::Shell;
use toml::Value;
use walkdir::WalkDir;

include!("src/cli.rs");

const BIN_NAME: &str = "zellij";

fn main() {
    // Build Sub-Projects (Temporary?)
    for entry in WalkDir::new(".") {
        let entry = entry.unwrap();
        let ext = entry.path().extension();
        if ext.is_some() && ext.unwrap() == "rs" {
            println!("cargo:rerun-if-changed={}", entry.path().to_string_lossy());
        }
    }

    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let manifest: Value =
        toml::from_str(&fs::read_to_string(manifest_dir.join("Cargo.toml")).unwrap()).unwrap();
    let members = manifest
        .get("workspace")
        .unwrap()
        .get("members")
        .unwrap()
        .as_array()
        .unwrap();

    let release = if var("PROFILE").unwrap() == "release" {
        "--release"
    } else {
        ""
    };
    let starting_dir = current_dir().unwrap();
    let alt_target = manifest_dir.join("target/tiles");
    for project in members {
        let path = manifest_dir.join(project.as_str().unwrap());
        // Should be able to change directory to continue build process
        set_current_dir(&path).unwrap();

        // FIXME: This is soul-crushingly terrible, but I can't keep the values alive long enough
        if var("PROFILE").unwrap() == "release" {
            Command::new("cargo".to_string())
                .arg("build")
                .arg("--target-dir")
                .arg(&alt_target.as_os_str())
                .arg("--release")
                .status()
                .unwrap();
        } else {
            Command::new("cargo")
                .arg("build")
                .arg("--target-dir")
                .arg(&alt_target.as_os_str())
                .status()
                .unwrap();
        }
    }
    // Should be able to change directory to continue build process
    set_current_dir(&starting_dir).unwrap();

    if var("PROFILE").unwrap() == "release" {
        // FIXME: Deduplicate this with the initial walk all .rs pattern
        for entry in fs::read_dir(alt_target.join("wasm32-wasi/release/")).unwrap() {
            let entry = entry.unwrap().path();
            let ext = entry.extension();
            if ext.is_some() && ext.unwrap() == "wasm" {
                dbg!(&entry);
                Command::new("wasm-opt")
                    .arg("-O")
                    .arg(entry.as_os_str())
                    .arg("-o")
                    .arg(format!(
                        "assets/plugins/{}",
                        entry.file_name().unwrap().to_string_lossy()
                    ))
                    .status()
                    .unwrap_or_else(|_| {
                        Command::new("cp")
                            .arg(entry.as_os_str())
                            .arg(format!(
                                "assets/plugins/{}",
                                entry.file_name().unwrap().to_string_lossy()
                            ))
                            .status()
                            .unwrap()
                    });
            }
        }
    } else {
        // FIXME: Deduplicate this with the initial walk all .rs pattern
        for entry in fs::read_dir(alt_target.join("wasm32-wasi/debug/")).unwrap() {
            let entry = entry.unwrap().path();
            let ext = entry.extension();
            if ext.is_some() && ext.unwrap() == "wasm" {
                dbg!(&entry);
                Command::new("wasm-opt")
                    .arg("-O")
                    .arg(entry.as_os_str())
                    .arg("-o")
                    .arg(format!(
                        "assets/plugins/{}",
                        entry.file_name().unwrap().to_string_lossy()
                    ))
                    .status()
                    .unwrap_or_else(|_| {
                        Command::new("cp")
                            .arg(entry.as_os_str())
                            .arg(format!(
                                "assets/plugins/{}",
                                entry.file_name().unwrap().to_string_lossy()
                            ))
                            .status()
                            .unwrap()
                    });
            }
        }
    }

    // Generate Shell Completions
    let mut clap_app = CliArgs::clap();
    println!("cargo:rerun-if-changed=src/cli.rs");
    let mut out_dir = std::env::var_os("CARGO_MANIFEST_DIR").unwrap();
    out_dir.push("/assets/completions");

    println!(
        "Completion files will to added to this location: {:?}",
        out_dir
    );
    fs::create_dir_all(&out_dir).unwrap();
    clap_app.gen_completions(BIN_NAME, Shell::Bash, &out_dir);
    clap_app.gen_completions(BIN_NAME, Shell::Zsh, &out_dir);
    clap_app.gen_completions(BIN_NAME, Shell::Fish, &out_dir);
}
