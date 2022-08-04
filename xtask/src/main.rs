//! See <https://github.com/matklad/cargo-xtask/>.
//!
//! This binary defines various auxiliary build commands, which are not
//! expressible with just `cargo`. Notably, it provides tests via `cargo test -p xtask`
//! for code generation and `cargo xtask install` for installation of
//! rust-analyzer server and client.
//!
//! This binary is integrated into the `cargo` command line by using an alias in
//! `.cargo/config`.

mod build;
mod dist;
mod flags;
mod test;

use std::{
    env,
    path::{Path, PathBuf},
};
use xshell::Shell;

lazy_static::lazy_static! {
    pub static ref WORKSPACE_MEMBERS: Vec<&'static str> = vec![
        "zellij-tile",
        "zellij-tile-utils",
        "default-plugins/compact-bar",
        "default-plugins/status-bar",
        "default-plugins/strider",
        "default-plugins/tab-bar",
        "zellij-utils",
        "zellij-client",
        "zellij-server",
        ".",
    ];
}

fn main() -> anyhow::Result<()> {
    let shell = &Shell::new()?;
    shell.change_dir(project_root());

    let flags = flags::Xtask::from_env()?;
    match flags.subcommand {
        flags::XtaskCmd::Help(_) => {
            println!("{}", flags::Xtask::HELP);
            Ok(())
        },
        flags::XtaskCmd::Build(flags) => build::build(&shell, flags),
        flags::XtaskCmd::Test(flags) => test::test(&shell, flags),
        _ => unimplemented!(),
    }
}

fn project_root() -> PathBuf {
    Path::new(
        &env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| env!("CARGO_MANIFEST_DIR").to_owned()),
    )
    .ancestors()
    .nth(1)
    .unwrap()
    .to_path_buf()
}

pub fn cargo() -> anyhow::Result<PathBuf> {
    std::env::var_os("CARGO")
        .map_or_else(|| which::which("cargo"), |exe| Ok(PathBuf::from(exe)))
        .context("Couldn't find 'cargo' executable")
}
