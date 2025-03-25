//! Subcommands for building.
//!
//! Currently has the following functions:
//!
//! - [`build`]: Builds general cargo projects (i.e. zellij components) with `cargo build`
//! - [`manpage`]: Builds the manpage with `mandown`
use crate::{flags, metadata, WorkspaceMember};
use anyhow::Context;
use std::path::{Path, PathBuf};
use xshell::{cmd, Shell};

/// Build members of the zellij workspace.
///
/// Build behavior is controlled by the [`flags`](flags::Build). Calls some variation of `cargo
/// build` under the hood.
pub fn build(sh: &Shell, flags: flags::Build) -> anyhow::Result<()> {
    let _pd = sh.push_dir(crate::project_root());

    let cargo = crate::cargo()?;
    if flags.no_plugins && flags.plugins_only {
        eprintln!("Cannot use both '--no-plugins' and '--plugins-only'");
        std::process::exit(1);
    }

    for WorkspaceMember { crate_name, .. } in crate::workspace_members()
        .iter()
        .filter(|member| member.build)
    {
        let err_context = || format!("failed to build '{crate_name}'");

        if crate_name.contains("plugins") {
            if flags.no_plugins {
                continue;
            }
        } else if flags.plugins_only {
            continue;
        }

        // zellij-utils requires protobuf definition files to be present. Usually these are
        // auto-generated with `build.rs`-files, but this is currently broken for us.
        // See [this PR][1] for details.
        //
        // [1]: https://github.com/zellij-org/zellij/pull/2711#issuecomment-1695015818
        {
            let zellij_utils_basedir = crate::project_root().join("zellij-utils");
            let _pd = sh.push_dir(zellij_utils_basedir);

            let prost_asset_dir = sh.current_dir().join("assets").join("prost");
            let protobuf_source_dir = sh.current_dir().join("src").join("plugin_api");
            std::fs::create_dir_all(&prost_asset_dir).unwrap();

            let mut prost = prost_build::Config::new();
            let last_generated = prost_asset_dir
                .join("generated_plugin_api.rs")
                .metadata()
                .and_then(|m| m.modified());
            let mut needs_regeneration = false;
            prost.out_dir(prost_asset_dir);
            prost.include_file("generated_plugin_api.rs");
            let mut proto_files = vec![];
            for entry in std::fs::read_dir(&protobuf_source_dir).unwrap() {
                let entry_path = entry.unwrap().path();
                if entry_path.is_file() {
                    if !entry_path
                        .extension()
                        .map(|e| e == "proto")
                        .unwrap_or(false)
                    {
                        continue;
                    }
                    proto_files.push(entry_path.display().to_string());
                    let modified = entry_path.metadata().and_then(|m| m.modified());
                    needs_regeneration |= match (&last_generated, modified) {
                        (Ok(last_generated), Ok(modified)) => modified >= *last_generated,
                        // Couldn't read some metadata, assume needs update
                        _ => true,
                    }
                }
            }
            if needs_regeneration {
                prost
                    .compile_protos(&proto_files, &[protobuf_source_dir])
                    .unwrap();
            }
        }

        let _pd = sh.push_dir(Path::new(crate_name));
        // Tell the user where we are now
        println!();
        let msg = format!(">> Building '{crate_name}'");
        crate::status(&msg);
        println!("{}", msg);

        let mut base_cmd = cmd!(sh, "{cargo} build");
        if flags.release {
            base_cmd = base_cmd.arg("--release");
        }
        if flags.no_web {
            // Check if this crate has web features that need modification
            match metadata::get_no_web_features(sh, crate_name)
                .context("Failed to check web features")?
            {
                Some(features) => {
                    base_cmd = base_cmd.arg("--no-default-features");
                    if !features.is_empty() {
                        base_cmd = base_cmd.arg("--features");
                        base_cmd = base_cmd.arg(features);
                    }
                },
                None => {
                    // Crate doesn't have web features, build normally
                },
            }
        }
        base_cmd.run().with_context(err_context)?;

        if crate_name.contains("plugins") {
            let (_, plugin_name) = crate_name
                .rsplit_once('/')
                .context("Cannot determine plugin name from '{subcrate}'")?;

            if flags.release {
                // Move plugin into assets folder
                move_plugin_to_assets(sh, plugin_name)?;
            }
        }
    }
    Ok(())
}

fn move_plugin_to_assets(sh: &Shell, plugin_name: &str) -> anyhow::Result<()> {
    let err_context = || format!("failed to move plugin '{plugin_name}' to assets folder");

    // Get asset path
    let asset_name = crate::asset_dir()
        .join("plugins")
        .join(plugin_name)
        .with_extension("wasm");

    // Get plugin path
    let plugin = PathBuf::from(
        std::env::var_os("CARGO_TARGET_DIR")
            .unwrap_or(crate::project_root().join("target").into_os_string()),
    )
    .join("wasm32-wasip1")
    .join("release")
    .join(plugin_name)
    .with_extension("wasm");

    if !plugin.is_file() {
        return Err(anyhow::anyhow!("No plugin found at '{}'", plugin.display()))
            .with_context(err_context);
    }

    // This is a plugin we want to move
    let from = plugin.as_path();
    let to = asset_name.as_path();
    sh.copy_file(from, to).with_context(err_context)
}

/// Build the manpage with `mandown`.
//      mkdir -p ${root_dir}/assets/man
//      mandown ${root_dir}/docs/MANPAGE.md 1 > ${root_dir}/assets/man/zellij.1
pub fn manpage(sh: &Shell) -> anyhow::Result<()> {
    let err_context = "failed to generate manpage";

    let mandown = mandown(sh).context(err_context)?;

    let project_root = crate::project_root();
    let asset_dir = &project_root.join("assets").join("man");
    sh.create_dir(asset_dir).context(err_context)?;
    let _pd = sh.push_dir(asset_dir);

    cmd!(sh, "{mandown} {project_root}/docs/MANPAGE.md 1")
        .read()
        .and_then(|text| sh.write_file("zellij.1", text))
        .context(err_context)
}

/// Get the path to a `mandown` executable.
///
/// If the executable isn't found, an error is returned instead.
fn mandown(_sh: &Shell) -> anyhow::Result<PathBuf> {
    match which::which("mandown") {
        Ok(path) => Ok(path),
        Err(e) => {
            eprintln!("!! 'mandown' wasn't found but is needed for this build step.");
            eprintln!("!! Please install it with: `cargo install mandown`");
            Err(e).context("Couldn't find 'mandown' executable")
        },
    }
}
