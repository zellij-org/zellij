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
    for project in members {
        let path = manifest_dir.join(project.as_str().unwrap());
        set_current_dir(&path);

        if var("PROFILE").unwrap() == "release" {
            panic!("no");
            Command::new("cargo").arg("build").arg("--release").status();
        } else {
            Command::new("cargo").arg("build").status();
        }

        eprintln!("{:?}", &path);
    }
    set_current_dir(&starting_dir);

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

    // Install Default Plugins and Layouts
    let assets = vec![
        "plugins/status-bar.wasm",
        "plugins/strider.wasm",
        "layouts/default.yaml",
        "layouts/strider.yaml",
    ];
    let project_dirs = ProjectDirs::from("org", "Mosaic Contributors", "Mosaic").unwrap();
    let data_dir = project_dirs.data_dir();
    fs::create_dir_all(data_dir.join("plugins")).unwrap();
    fs::create_dir_all(data_dir.join("layouts")).unwrap();
    for asset in assets {
        println!("cargo:rerun-if-changed=assets/{}", asset);
        fs::copy(Path::new("assets/").join(asset), data_dir.join(asset))
            .expect("Failed to copy asset files");
    }
}
