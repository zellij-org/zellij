//! Subcommands for building.
//!
//! Currently has the following functions:
//!
//! - [`build`]: Builds general cargo projects (i.e. zellij components) with `cargo build`
//! - [`wasm_opt_plugin`]: Calls `wasm-opt` on all plugins
//! - [`manpage`]: Builds the manpage with `mandown`
use crate::flags;
use anyhow::Context;
use std::path::{Path, PathBuf};
use xshell::{cmd, Shell};

/// Build members of the zellij workspace.
///
/// Build behavior is controlled by the [`flags`](flags::Build). Calls some variation of `cargo
/// build` under the hood.
pub fn build(sh: &Shell, flags: flags::Build) -> anyhow::Result<()> {
    let cargo = crate::cargo()?;
    if flags.no_plugins && flags.plugins_only {
        eprintln!("Cannot use both '--no-plugins' and '--plugins-only'");
        std::process::exit(1);
    }

    for subcrate in crate::WORKSPACE_MEMBERS.iter() {
        if subcrate.contains("plugins") {
            if flags.no_plugins {
                continue;
            }
        } else {
            if flags.plugins_only {
                continue;
            }
        }

        let _pd = sh.push_dir(Path::new(subcrate));
        // Tell the user where we are now
        println!();
        println!(">> Building '{subcrate}'");

        let mut base_cmd = cmd!(sh, "{cargo} build");
        if flags.release {
            base_cmd = base_cmd.arg("--release");
        }
        base_cmd
            .run()
            .with_context(|| format!("Failed to build '{subcrate}'"))?;
    }
    Ok(())
}

/// Call `wasm-opt` on all plugins.
///
/// Plugins are discovered automatically by scanning the contents of `target/wasm32-wasi/release`
/// for filenames ending with `.wasm`. For this to work the plugins must be built beforehand.
// TODO: Should this panic if there is no plugin found? What should we do when only some plugins
// have been built before?
pub fn wasm_opt_plugins(sh: &Shell) -> anyhow::Result<()> {
    let wasm_opt = wasm_opt(sh)?;

    let asset_dir = crate::project_root().join("assets").join("plugins");
    sh.create_dir(&asset_dir)
        .context("Couldn't create asset dir for plugins")?;
    let _pd = sh.push_dir(asset_dir);

    let mut target_dir = PathBuf::from(
        std::env::var_os("CARGO_TARGET_DIR")
            .unwrap_or(crate::project_root().join("target").into_os_string()),
    );
    target_dir.push("wasm32-wasi");
    target_dir.push("release");

    for entry in std::fs::read_dir(&target_dir)? {
        let entry = entry
            .with_context(|| format!("Failed to read contents of '{}'", target_dir.display()))?;
        if !entry.file_type()?.is_file() {
            continue;
        }
        let name = match entry.file_name().into_string() {
            Ok(name) => name,
            Err(e) => {
                return Err(anyhow::anyhow!(
                    "Couldn't read filename '{e:?}' containing invalid unicode"
                ))
            },
        };
        if name.ends_with(".wasm") {
            // This is a plugin we want to optimize
            println!();
            println!(">> Optimizing plugin {name}");

            let input = entry.path();
            cmd!(sh, "{wasm_opt} -O {input} -o {name}")
                .run()
                .with_context(|| format!("Error while optimizing WASM for plugin '{name}'"))?;
        }
    }
    Ok(())
}

/// Get the path to a `wasm-opt` executable.
///
/// If the executable isn't found, an error is returned instead.
// TODO: Offer the user to install latest wasm-opt on path?
fn wasm_opt(_sh: &Shell) -> anyhow::Result<PathBuf> {
    match which::which("wasm-opt") {
        Ok(path) => Ok(path),
        Err(e) => {
            println!("!! 'wasm-opt' wasn't found but is needed for this build step.");
            println!("!! Please install it from here: https://github.com/WebAssembly/binaryen");
            Err(e).context("Couldn't find 'wasm-opt' executable")
        },
    }
}

/// Build the manpage with `mandown`.
//      mkdir -p ${root_dir}/assets/man
//      mandown ${root_dir}/docs/MANPAGE.md 1 > ${root_dir}/assets/man/zellij.1
pub fn manpage(sh: &Shell) -> anyhow::Result<()> {
    let mandown = mandown(sh)?;

    let project_root = crate::project_root();
    let asset_dir = &project_root.join("assets").join("man");
    sh.create_dir(&asset_dir)
        .context("Couldn't create asset dir for plugins")?;
    let _pd = sh.push_dir(asset_dir);

    let text = cmd!(sh, "{mandown} {project_root}/docs/MANPAGE.md 1")
        .read()
        .context("Generating man pages failed")?;
    sh.write_file("zellij.1", text).context("Writing man pages failed")?;

    Ok(())
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
