//! Handle running `cargo fmt` on the sources.
use crate::flags;
use anyhow::Context;
use std::path::{Path, PathBuf};
use xshell::{cmd, Shell};

pub fn format(sh: &Shell, _flags: flags::Format) -> anyhow::Result<()> {
    let cargo = check_rustfmt()
        .and_then(|_| crate::cargo())
        .context("failed to run task 'format'")?;

    for subcrate in crate::WORKSPACE_MEMBERS.iter() {
        let _pd = sh.push_dir(Path::new(subcrate));
        // Tell the user where we are now
        println!();
        let msg = format!(">> Formatting '{subcrate}'");
        println!("{}", msg);
        crate::status(&msg);

        cmd!(sh, "{cargo} fmt")
            .run()
            .with_context(|| format!("Failed to format '{subcrate}'"))?;
    }
    Ok(())
}

fn check_rustfmt() -> anyhow::Result<PathBuf> {
    which::which("rustfmt").context(
        "Couldn't find 'rustfmt' executable. Please install it with `cargo install rustfmt`",
    )
}
