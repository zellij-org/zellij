//! Handle running `cargo fmt` on the sources.
use crate::flags;
use anyhow::Context;
use std::path::{Path, PathBuf};
use xshell::{cmd, Shell};

pub fn format(sh: &Shell, _flags: flags::Format) -> anyhow::Result<()> {
    let cargo = crate::cargo()?;
    check_rustfmt()?;

    for subcrate in crate::WORKSPACE_MEMBERS.iter() {
        let _pd = sh.push_dir(Path::new(subcrate));
        // Tell the user where we are now
        println!();
        println!(">> Formatting '{subcrate}'");

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
