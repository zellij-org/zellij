//! Composite pipelines for the build system.
//!
//! Defines multiple "pipelines" that run specific individual steps in sequence.
use crate::flags;
use crate::{build, clippy, format, test};
use anyhow::Context;
use xshell::{cmd, Shell};

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
    test::test(sh, flags::Test { args: vec![] })?;
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

/// Run zellij debug build.
pub fn run(sh: &Shell, flags: flags::Run) -> anyhow::Result<()> {
    build::build(
        sh,
        flags::Build {
            release: false,
            no_plugins: false,
            plugins_only: true,
        },
    )?;

    crate::cargo()
        .and_then(|cargo| {
            cmd!(sh, "{cargo} run")
                .args(flags.args)
                .run()
                .context("command failure")
        })
        .context("failed to run debug build")
}

/// Bundle all distributable content to `target/dist`.
///
/// This includes the optimized zellij executable from the [`install`] pipeline, the man page, the
/// `.desktop` file and the application logo.
pub fn dist(sh: &Shell, _flags: flags::Dist) -> anyhow::Result<()> {
    sh.change_dir(crate::project_root());
    if sh.path_exists("target/dist") {
        sh.remove_path("target/dist")
            .context("Failed to clean up dist directory")?;
    }
    sh.create_dir("target/dist")
        .context("Failed to create dist directory")?;

    install(
        sh,
        flags::Install {
            destination: crate::project_root().join("./target/dist/zellij"),
        },
    )
    .context("Failed to build zellij for distributing")?;

    sh.create_dir("target/dist/man")
        .context("Failed to create directory for man pages in dist folder")?;
    sh.copy_file("assets/man/zellij.1", "target/dist/man/zellij.1")
        .context("Failed to copy generated manpage to dist folder")?;
    sh.copy_file("assets/zellij.desktop", "target/dist/zellij.desktop")
        .context("Failed to copy zellij desktop file to dist folder")?;
    sh.copy_file("assets/logo.png", "target/dist/logo.png")
        .context("Failed to copy zellij logo to dist folder")?;
    Ok(())
}
