//! Tasks related to zellij CI
use crate::{
    build,
    flags::{self, CiCmd, Cross, E2e},
};
use anyhow::Context;
use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};
use xshell::{cmd, Shell};

pub fn main(sh: &Shell, flags: flags::Ci) -> anyhow::Result<()> {
    let err_context = "failed to run CI task";

    match flags.subcommand {
        CiCmd::E2e(E2e {
            build: false,
            test: false,
            ..
        }) => Err(anyhow::anyhow!(
            "either '--build' or '--test' must be provided!"
        )),
        CiCmd::E2e(E2e {
            build: true,
            test: true,
            ..
        }) => Err(anyhow::anyhow!(
            "flags '--build' and '--test' are mutually exclusive!"
        )),
        CiCmd::E2e(E2e {
            build: true,
            test: false,
            ..
        }) => e2e_build(sh),
        CiCmd::E2e(E2e {
            build: false,
            test: true,
            args,
        }) => e2e_test(sh, args),
        CiCmd::Cross(Cross { triple }) => cross_compile(sh, &triple),
    }
    .context(err_context)
}

fn e2e_build(sh: &Shell) -> anyhow::Result<()> {
    let err_context = "failed to build E2E binary";

    build::build(
        sh,
        flags::Build {
            release: true,
            no_plugins: false,
            plugins_only: true,
        },
    )
    .context(err_context)?;

    // Copy plugins to e2e data-dir
    let plugin_dir = crate::asset_dir().join("plugins");
    let project_root = crate::project_root();
    let data_dir = project_root.join("target").join("e2e-data");
    let plugins: Vec<_> = std::fs::read_dir(plugin_dir)
        .context(err_context)?
        .filter_map(|dir_entry| {
            if let Ok(entry) = dir_entry {
                entry
                    .file_name()
                    .to_string_lossy()
                    .ends_with(".wasm")
                    .then_some(entry.path())
            } else {
                None
            }
        })
        .collect();

    sh.remove_path(&data_dir)
        .and_then(|_| sh.create_dir(&data_dir))
        .and_then(|_| sh.create_dir(data_dir.join("plugins")))
        .context(err_context)?;

    for plugin in plugins {
        sh.copy_file(plugin, data_dir.join("plugins"))
            .context(err_context)?;
    }

    let _pd = sh.push_dir(project_root);
    crate::cargo()
        .and_then(|cargo| {
            cmd!(
                sh,
                "{cargo} build --release --target x86_64-unknown-linux-musl"
            )
            .run()
            .map_err(anyhow::Error::new)
        })
        .context(err_context)
}

fn e2e_test(sh: &Shell, args: Vec<OsString>) -> anyhow::Result<()> {
    let err_context = "failed to run E2E tests";

    e2e_build(sh).context(err_context)?;

    let _pd = sh.push_dir(crate::project_root());

    // set --no-default-features so the test binary gets built with the plugins from assets/plugins that just got built
    crate::cargo()
        .and_then(|cargo| {
            // e2e tests
            cmd!(
                sh,
                "{cargo} test --no-default-features -- --ignored --nocapture --test-threads 1"
            )
            .args(args.clone())
            .run()
            .map_err(anyhow::Error::new)?;

            // plugin system tests are run here because they're medium-slow
            let _pd = sh.push_dir(Path::new("zellij-server"));
            println!();
            let msg = ">> Testing Plugin System".to_string();
            crate::status(&msg);
            println!("{}", msg);

            cmd!(sh, "{cargo} test -- --ignored --nocapture --test-threads 1")
                .args(args.clone())
                .run()
                .with_context(|| "Failed to run tests for the Plugin System".to_string())?;
            Ok(())
        })
        .context(err_context)
}

fn cross_compile(sh: &Shell, target: &OsString) -> anyhow::Result<()> {
    let err_context = || format!("failed to cross-compile for {target:?}");

    crate::cargo()
        .and_then(|cargo| {
            cmd!(sh, "{cargo} install mandown").run()?;
            Ok(cargo)
        })
        .and_then(|cargo| {
            cmd!(sh, "{cargo} install cross")
                .run()
                .map_err(anyhow::Error::new)
        })
        .with_context(err_context)?;

    build::build(
        sh,
        flags::Build {
            release: true,
            no_plugins: false,
            plugins_only: true,
        },
    )
    .and_then(|_| build::manpage(sh))
    .with_context(err_context)?;

    cross()
        .and_then(|cross| {
            cmd!(sh, "{cross} build --verbose --release --target {target}")
                .run()
                .map_err(anyhow::Error::new)
        })
        .with_context(err_context)
}

fn cross() -> anyhow::Result<PathBuf> {
    match which::which("cross") {
        Ok(path) => Ok(path),
        Err(e) => {
            eprintln!("!! 'cross' wasn't found but is needed for this build step.");
            eprintln!("!! Please install it with: `cargo install cross`");
            Err(e).context("couldn't find 'cross' executable")
        },
    }
}
