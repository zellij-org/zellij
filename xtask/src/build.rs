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
        let err_context = || format!("failed to build '{subcrate}'");

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
        let msg = format!(">> Building '{subcrate}'");
        crate::status(&msg);
        println!("{}", msg);

        let mut base_cmd = cmd!(sh, "{cargo} build");
        if flags.release {
            base_cmd = base_cmd.arg("--release");
        }
        base_cmd.run().with_context(err_context)?;

        if subcrate.contains("plugins") {
            let (_, plugin_name) = subcrate
                .rsplit_once('/')
                .context("Cannot determine plugin name from '{subcrate}'")?;

            if flags.release {
                // Perform wasm-opt on plugin
                wasm_opt_plugin(sh, plugin_name).with_context(err_context)?;
            } else {
                // Must copy plugins to zellij-utils, too!
                let source = PathBuf::from(
                    std::env::var_os("CARGO_TARGET_DIR")
                        .unwrap_or(crate::project_root().join("target").into_os_string()),
                )
                .join("wasm32-wasi")
                .join("debug")
                .join(plugin_name)
                .with_extension("wasm");
                let target = crate::project_root()
                    .join("zellij-utils")
                    .join("assets")
                    .join("plugins");
                sh.copy_file(source, target).with_context(err_context)?;
            }
        }
    }
    Ok(())
}

/// Call `wasm-opt` on all plugins.
///
/// Plugins are discovered automatically by scanning the contents of `target/wasm32-wasi/release`
/// for filenames ending with `.wasm`. For this to work the plugins must be built beforehand.
// TODO: Should this panic if there is no plugin found? What should we do when only some plugins
// have been built before?
pub fn wasm_opt_plugin(sh: &Shell, plugin_name: &str) -> anyhow::Result<()> {
    let err_context = || format!("failed to run 'wasm-opt' on plugin '{plugin_name}'");

    let wasm_opt = wasm_opt(sh).with_context(err_context)?;

    let asset_dir = crate::project_root()
        .join("zellij-utils")
        .join("assets")
        .join("plugins");
    sh.create_dir(&asset_dir).with_context(err_context)?;
    let _pd = sh.push_dir(asset_dir);

    let plugin = PathBuf::from(
        std::env::var_os("CARGO_TARGET_DIR")
            .unwrap_or(crate::project_root().join("target").into_os_string()),
    )
    .join("wasm32-wasi")
    .join("release")
    .join(plugin_name)
    .with_extension("wasm");

    if !plugin.is_file() {
        return Err(anyhow::anyhow!("No plugin found at '{}'", plugin.display()))
            .with_context(err_context);
    }
    let name = match plugin.file_name().with_context(err_context)?.to_str() {
        Some(name) => name,
        None => {
            return Err(anyhow::anyhow!(
                "couldn't read filename containing invalid unicode"
            ))
            .with_context(err_context)
        },
    };

    // This is a plugin we want to optimize
    println!();
    let msg = format!(">> Optimizing plugin '{name}'");
    crate::status(&msg);
    println!("{}", msg);

    let input = plugin.as_path();
    cmd!(sh, "{wasm_opt} -O {input} -o {name}")
        .run()
        .with_context(err_context)?;

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
            Err(e).context("couldn't find 'wasm-opt' executable")
        },
    }
}

/// Build the manpage with `mandown`.
//      mkdir -p ${root_dir}/assets/man
//      mandown ${root_dir}/docs/MANPAGE.md 1 > ${root_dir}/assets/man/zellij.1
pub fn manpage(sh: &Shell) -> anyhow::Result<()> {
    let err_context = "failed to generate manpage";

    let mandown = mandown(sh).context(err_context)?;

    let project_root = crate::project_root();
    let asset_dir = &project_root.join("assets").join("man");
    sh.create_dir(&asset_dir).context(err_context)?;
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
