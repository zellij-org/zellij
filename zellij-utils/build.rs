use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap_or_default());
    let repo_root = manifest_dir.join("..");

    println!(
        "cargo:rerun-if-changed={}",
        repo_root.join(".git/HEAD").display()
    );
    println!("cargo:rerun-if-env-changed=ZELLIJ_VERSION");

    let version = env::var("ZELLIJ_VERSION")
        .ok()
        .filter(|v| !v.is_empty())
        .or_else(|| {
            Command::new("git")
                .args([
                    "-C",
                    &repo_root.to_string_lossy(),
                    "describe",
                    "--tags",
                    "--dirty",
                    "--always",
                ])
                .output()
                .ok()
                .filter(|output| output.status.success())
                .and_then(|output| String::from_utf8(output.stdout).ok())
                .map(|version| version.trim().to_string())
                .filter(|version| !version.is_empty())
        })
        .unwrap_or_else(|| env::var("CARGO_PKG_VERSION").unwrap_or_else(|_| "unknown".to_owned()));

    println!("cargo:rustc-env=ZELLIJ_VERSION={version}");
}
