use directories_next::ProjectDirs;
use std::{fs, path::Path};
use structopt::clap::Shell;

include!("src/cli.rs");

const BIN_NAME: &str = "mosaic";

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
