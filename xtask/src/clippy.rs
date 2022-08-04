//! Handle running `cargo clippy` on the sources.
use crate::flags;
use anyhow::Context;
use std::path::{Path, PathBuf};
use xshell::{cmd, Shell};

pub fn clippy(sh: &Shell, _flags: flags::Clippy) -> anyhow::Result<()> {
    let cargo = crate::cargo()?;
    check_clippy()?;

    for subcrate in crate::WORKSPACE_MEMBERS.iter() {
        let _pd = sh.push_dir(Path::new(subcrate));
        // Tell the user where we are now
        println!();
        println!(">> Running clippy on '{subcrate}'");

        cmd!(
            sh,
            "{cargo} clippy --all-targets --all-features -- --deny warnings"
        )
        .run()
        .with_context(|| format!("Failed to run clippy on '{subcrate}'"))?;
    }
    Ok(())
}

fn check_clippy() -> anyhow::Result<PathBuf> {
    which::which("cargo-clippy").context(
        "Couldn't find 'clippy' executable. Please install it with `rustup component add clippy`",
    )
}
