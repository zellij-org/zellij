//! Handle running `cargo clippy` on the sources.
use crate::{build, flags, WorkspaceMember};
use anyhow::Context;
use std::path::{Path, PathBuf};
use xshell::{cmd, Shell};

pub fn clippy(sh: &Shell, _flags: flags::Clippy) -> anyhow::Result<()> {
    let _pd = sh.push_dir(crate::project_root());

    build::build(
        sh,
        flags::Build {
            release: false,
            no_plugins: false,
            plugins_only: true,
            no_web: false,
        },
    )
    .context("failed to run task 'clippy'")?;

    let cargo = check_clippy()
        .and_then(|_| crate::cargo())
        .context("failed to run task 'clippy'")?;

    for WorkspaceMember { crate_name, .. } in crate::workspace_members().iter() {
        let _pd = sh.push_dir(Path::new(crate_name));
        // Tell the user where we are now
        println!();
        let msg = format!(">> Running clippy on '{crate_name}'");
        crate::status(&msg);
        println!("{}", msg);

        cmd!(sh, "{cargo} clippy --all-targets --all-features")
            .run()
            .with_context(|| format!("failed to run task 'clippy' on '{crate_name}'"))?;
    }
    Ok(())
}

fn check_clippy() -> anyhow::Result<PathBuf> {
    which::which("cargo-clippy").context(
        "Couldn't find 'clippy' executable. Please install it with `rustup component add clippy`",
    )
}
