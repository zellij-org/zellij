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
    let err_context = || format!("failed to run pipeline 'make' with args {flags:?}");

    if flags.clean {
        crate::cargo()
            .and_then(|cargo| cmd!(sh, "{cargo} clean").run().map_err(anyhow::Error::new))
            .with_context(err_context)?;
    }

    format::format(sh, flags::Format { check: false })
        .and_then(|_| {
            build::build(
                sh,
                flags::Build {
                    release: flags.release,
                    no_plugins: false,
                    plugins_only: false,
                },
            )
        })
        .and_then(|_| test::test(sh, flags::Test { args: vec![] }))
        .and_then(|_| clippy::clippy(sh, flags::Clippy {}))
        .with_context(err_context)
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
    let err_context = || format!("failed to run pipeline 'install' with args {flags:?}");

    // Build and optimize plugins
    build::build(
        sh,
        flags::Build {
            release: true,
            no_plugins: false,
            plugins_only: true,
        },
    )
    .and_then(|_| {
        // Build the main executable
        build::build(
            sh,
            flags::Build {
                release: true,
                no_plugins: true,
                plugins_only: false,
            },
        )
    })
    .and_then(|_| {
        // Generate man page
        build::manpage(sh)
    })
    .with_context(err_context)?;

    // Copy binary to destination
    let destination = if flags.destination.is_absolute() {
        flags.destination.clone()
    } else {
        std::env::current_dir()
            .context("Can't determine current working directory")?
            .join(&flags.destination)
    };
    sh.change_dir(crate::project_root());
    sh.copy_file("target/release/zellij", &destination)
        .with_context(err_context)
}

/// Run zellij debug build.
pub fn run(sh: &Shell, flags: flags::Run) -> anyhow::Result<()> {
    let err_context = || format!("failed to run pipeline 'run' with args {flags:?}");

    build::build(
        sh,
        flags::Build {
            release: false,
            no_plugins: false,
            plugins_only: true,
        },
    )
    .and_then(|_| crate::cargo())
    .and_then(|cargo| {
        cmd!(sh, "{cargo} run")
            .args(&flags.args)
            .run()
            .map_err(anyhow::Error::new)
    })
    .with_context(err_context)
}

/// Bundle all distributable content to `target/dist`.
///
/// This includes the optimized zellij executable from the [`install`] pipeline, the man page, the
/// `.desktop` file and the application logo.
pub fn dist(sh: &Shell, _flags: flags::Dist) -> anyhow::Result<()> {
    let err_context = || format!("failed to run pipeline 'dist'");

    sh.change_dir(crate::project_root());
    if sh.path_exists("target/dist") {
        sh.remove_path("target/dist").with_context(err_context)?;
    }
    sh.create_dir("target/dist")
        .map_err(anyhow::Error::new)
        .and_then(|_| {
            install(
                sh,
                flags::Install {
                    destination: crate::project_root().join("./target/dist/zellij"),
                },
            )
        })
        .with_context(err_context)?;

    sh.create_dir("target/dist/man")
        .and_then(|_| sh.copy_file("assets/man/zellij.1", "target/dist/man/zellij.1"))
        .and_then(|_| sh.copy_file("assets/zellij.desktop", "target/dist/zellij.desktop"))
        .and_then(|_| sh.copy_file("assets/logo.png", "target/dist/logo.png"))
        .with_context(err_context)
}
