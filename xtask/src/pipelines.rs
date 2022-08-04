//! Composite pipelines for the build system.
//!
//! Defines multiple "pipelines" that run specific individual steps in sequence.
use crate::flags;
use crate::{build, clippy, format, test};
use anyhow::Context;
use xshell::Shell;

/// Perform a default build.
///
/// Runs the following steps in sequence:
///
/// - format
/// - build
/// - test
/// - clippy
pub fn make(sh: &Shell, flags: flags::Make) -> anyhow::Result<()> {
    format::format(sh, flags::Format {})?;
    build::build(
        sh,
        flags::Build {
            release: flags.release,
            no_plugins: false,
            plugins_only: false,
        },
    )?;
    test::test(sh, flags::Test {})?;
    clippy::clippy(sh, flags::Clippy {})?;
    Ok(())
}

/// Generate a runnable executable.
///
/// Runs the following steps in sequence:
///
/// - [`build`](build::build) (release, plugins only)
/// - [`wasm_opt_plugins`](build::wasm_opt_plugins)
/// - [`build`](build::build) (release, without plugins)
/// - [`manpage`](build::manpage)
/// - Copy the executable to [target file](flags::Install::destination)
pub fn install(sh: &Shell, flags: flags::Install) -> anyhow::Result<()> {
    // Build and optimize plugins
    build::build(
        sh,
        flags::Build {
            release: true,
            no_plugins: false,
            plugins_only: true,
        },
    )?;
    build::wasm_opt_plugins(sh)?;

    // Build the main executable
    build::build(
        sh,
        flags::Build {
            release: true,
            no_plugins: true,
            plugins_only: false,
        },
    )?;

    // Generate man page
    build::manpage(sh)?;

    // Copy binary to destination
    let destination = if flags.destination.is_absolute() {
        flags.destination
    } else {
        std::env::current_dir()
            .context("Can't determine current working directory")?
            .join(flags.destination)
    };
    sh.change_dir(crate::project_root());
    sh.copy_file("target/release/zellij", &destination)
        .with_context(|| format!("Failed to copy executable to '{}", destination.display()))?;
    Ok(())
}
