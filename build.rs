use directories_next::ProjectDirs;
use std::{ffi::OsStr, fs};
use structopt::clap::Shell;
use walkdir::WalkDir;

include!("src/cli.rs");

const BIN_NAME: &str = "zellij";

fn main() {
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

    // Clear Default Plugins and Layouts

    // Rerun on layout change
    for entry in WalkDir::new("assets/layouts") {
        let entry = entry.unwrap();
        println!("cargo:rerun-if-changed={}", entry.path().to_string_lossy());
    }

    // Rerun on plugin change
    for entry in WalkDir::new("target") {
        let entry = entry.unwrap();
        if entry.path().extension() == Some(OsStr::new("wasm")) {
            println!("cargo:rerun-if-changed={}", entry.path().to_string_lossy());
        }
    }

    let project_dirs = ProjectDirs::from("org", "Zellij Contributors", "Zellij").unwrap();
    let data_dir = project_dirs.data_dir();
    drop(fs::remove_file(data_dir.join("plugins/status-bar.wasm")));
    drop(fs::remove_file(data_dir.join("plugins/tab-bar.wasm")));
    drop(fs::remove_file(data_dir.join("plugins/strider.wasm")));
    drop(fs::remove_file(data_dir.join("layouts/default.yaml")));
    drop(fs::remove_file(data_dir.join("layouts/strider.yaml")));
}
