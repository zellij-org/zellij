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

    if let Some(ref data_dir) = flags.data_dir {
        let data_dir = sh.current_dir().join(data_dir);

        crate::cargo()
            .and_then(|cargo| {
                cmd!(sh, "{cargo} run")
                    .args(["--package", "zellij"])
                    .arg("--no-default-features")
                    .args(["--features", "disable_automatic_asset_installation"])
                    .args(["--", "--data-dir", &format!("{}", data_dir.display())])
                    .args(&flags.args)
                    .run()
                    .map_err(anyhow::Error::new)
            })
            .with_context(err_context)
    } else {
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
            cmd!(sh, "{cargo} run --")
                .args(&flags.args)
                .run()
                .map_err(anyhow::Error::new)
        })
        .with_context(err_context)
    }
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

/// Make a zellij release and publish all crates.
pub fn publish(sh: &Shell, flags: flags::Publish) -> anyhow::Result<()> {
    let err_context = "failed to publish zellij";

    sh.change_dir(crate::project_root());
    let dry_run = if flags.dry_run {
        Some("--dry-run")
    } else {
        None
    };
    let cargo = crate::cargo().context(err_context)?;
    let project_dir = crate::project_root();
    let manifest = sh
        .read_file(project_dir.join("Cargo.toml"))
        .context(err_context)?
        .parse::<toml::Value>()
        .context(err_context)?;
    // Version of the core crate
    let version = manifest
        .get("package")
        .and_then(|package| package["version"].as_str())
        .context(err_context)?;

    let mut skip_build = false;
    if cmd!(sh, "git tag -l")
        .read()
        .context(err_context)?
        .contains(version)
    {
        println!();
        println!("Git tag 'v{version}' is already present.");
        println!("If this is a mistake, delete it with: git tag -d 'v{version}'");
        println!("Skip build phase and continue to publish? [y/n]");

        let stdin = std::io::stdin();
        loop {
            let mut buffer = String::new();
            stdin.read_line(&mut buffer).context(err_context)?;
            match buffer.trim_end() {
                "y" | "Y" => {
                    skip_build = true;
                    break;
                },
                "n" | "N" => {
                    skip_build = false;
                    break;
                },
                _ => {
                    println!(" --> Unknown input '{buffer}', ignoring...");
                    println!();
                    println!("Skip build phase and continue to publish? [y/n]");
                },
            }
        }
    }

    if !skip_build {
        // Clean project
        cmd!(sh, "{cargo} clean").run().context(err_context)?;

        // Build plugins
        build::build(
            sh,
            flags::Build {
                release: true,
                no_plugins: false,
                plugins_only: true,
            },
        )
        .context(err_context)?;

        // Update default config
        sh.copy_file(
            project_dir
                .join("zellij-utils")
                .join("assets")
                .join("config")
                .join("default.kdl"),
            project_dir.join("example").join("default.kdl"),
        )
        .context(err_context)?;

        // Commit changes
        cmd!(sh, "git commit -aem")
            .arg(format!("chore(release): v{}", version))
            .run()
            .context(err_context)?;

        // Tag release
        cmd!(sh, "git tag --annotate --message")
            .arg(format!("Version {}", version))
            .arg(format!("v{}", version))
            .run()
            .context(err_context)?;
    }

    let closure = || -> anyhow::Result<()> {
        // Push commit and tag
        if flags.dry_run {
            println!("Skipping push due to dry-run");
        } else {
            cmd!(sh, "git push --atomic origin main v{version}")
                .run()
                .context(err_context)?;
        }

        // Publish all the crates
        for member in crate::WORKSPACE_MEMBERS.iter() {
            if member.contains("plugin") {
                continue;
            }

            let _pd = sh.push_dir(project_dir.join(member));
            loop {
                if let Err(err) = cmd!(sh, "{cargo} publish {dry_run...}")
                    .run()
                    .context(err_context)
                {
                    println!();
                    println!("Publishing crate '{member}' failed with error:");
                    println!("{:?}", err);
                    println!();
                    println!("Retry? [y/n]");

                    let stdin = std::io::stdin();
                    let mut buffer = String::new();
                    let retry: bool;

                    loop {
                        stdin.read_line(&mut buffer).context(err_context)?;

                        match buffer.trim_end() {
                            "y" | "Y" => {
                                retry = true;
                                break;
                            },
                            "n" | "N" => {
                                retry = false;
                                break;
                            },
                            _ => {
                                println!(" --> Unknown input '{buffer}', ignoring...");
                                println!();
                                println!("Retry? [y/n]");
                            },
                        }
                    }

                    if retry {
                        continue;
                    } else {
                        println!("Aborting publish for crate '{member}'");
                        return Err::<(), _>(err);
                    }
                } else {
                    println!("Waiting for crates.io to catch up...");
                    std::thread::sleep(std::time::Duration::from_secs(15));
                    break;
                }
            }
        }
        Ok(())
    };

    // We run this in a closure so that a failure in any of the commands doesn't abort the whole
    // program. When dry-running we need to undo the release commit first!
    let result = closure();

    if flags.dry_run {
        cmd!(sh, "git reset --hard HEAD~1")
            .run()
            .context(err_context)?;
    }

    result
}
