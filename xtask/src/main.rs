//! See <https://github.com/matklad/cargo-xtask/>.
//!
//! This binary defines various auxiliary build commands, which are not expressible with just
//! `cargo`. Notably, it provides tests via `cargo test -p xtask` for code generation and `cargo
//! xtask install` for installation of rust-analyzer server and client.
//!
//! This binary is integrated into the `cargo` command line by using an alias in `.cargo/config`.
// Current default "flow":
// - format-flow: `cargo fmt`
// - format-toml-conditioned-flow: ??
// - build: `cargo build`
// - test: `cargo test`
// - clippy: `cargo clippy --all-targets --all-features -- --deny warnings $@`
//
// # Install flow:
// - build-plugins-release: `cargo build --release ...`
// - wasm-opt-plugins: `wasm-opt ...`
// - build-release: `cargo build --release`
// - install-mandown: `cargo install mandown`
// - manpage: |
//      mkdir -p ${root_dir}/assets/man
//      mandown ${root_dir}/docs/MANPAGE.md 1 > ${root_dir}/assets/man/zellij.1
// - install: `cp target/release/zellij "$1"`
//
// # Release flow:
// - workspace: cargo make --profile development -- release
//
// # Publish flow:
// - update-default-config:
// - build-plugins-release: `cargo build --release ...`
// - wasm-opt-plugins: `wasm-opt ...`
// - release-commit:
//      - commit-all: `git commit -aem "chore(release): v${CRATE_VERSION}"`
//      - tag-release: `git tag --annotate --message "Version ${CRATE_VERSION}"
//      "v${CRATE_VERSION}"`
//      - `git push --atomic origin main "v${CRATE_VERSION}"`
// - publish-zellij: `cargo publish [tile, client, server, utils, tile-utils, zellij]`

mod build;
mod ci;
mod clippy;
mod dist;
mod flags;
mod format;
mod pipelines;
mod test;

use anyhow::Context;
use std::{
    env,
    path::{Path, PathBuf},
    time::Instant,
};
use xshell::Shell;

lazy_static::lazy_static! {
    pub static ref WORKSPACE_MEMBERS: Vec<&'static str> = vec![
        "default-plugins/compact-bar",
        "default-plugins/status-bar",
        "default-plugins/strider",
        "default-plugins/tab-bar",
        "zellij-utils",
        "zellij-tile-utils",
        "zellij-tile",
        "zellij-client",
        "zellij-server",
        ".",
    ];
}

fn main() -> anyhow::Result<()> {
    let shell = &Shell::new()?;
    shell.change_dir(project_root());

    let flags = flags::Xtask::from_env()?;
    let now = Instant::now();

    match flags.subcommand {
        flags::XtaskCmd::Deprecated(_flags) => deprecation_notice(),
        flags::XtaskCmd::Dist(flags) => pipelines::dist(shell, flags),
        flags::XtaskCmd::Build(flags) => build::build(shell, flags),
        flags::XtaskCmd::Clippy(flags) => clippy::clippy(shell, flags),
        flags::XtaskCmd::Format(flags) => format::format(shell, flags),
        flags::XtaskCmd::Test(flags) => test::test(shell, flags),
        flags::XtaskCmd::Manpage(_flags) => build::manpage(shell),
        // Pipelines
        // These are composite commands, made up of multiple "stages" defined above.
        flags::XtaskCmd::Make(flags) => pipelines::make(shell, flags),
        flags::XtaskCmd::Install(flags) => pipelines::install(shell, flags),
        flags::XtaskCmd::Run(flags) => pipelines::run(shell, flags),
        flags::XtaskCmd::Ci(flags) => ci::main(shell, flags),
        flags::XtaskCmd::Publish(flags) => pipelines::publish(shell, flags),
    }?;

    let elapsed = now.elapsed().as_secs();
    status(&format!("xtask (done after {} s)", elapsed));
    println!("\n\n>> Command took {} s", elapsed);
    Ok(())
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

// Set terminal title to 'msg'
pub fn status(msg: &str) {
    print!("\u{1b}]0;{}\u{07}", msg);
}

fn deprecation_notice() -> anyhow::Result<()> {
    Err(anyhow::anyhow!(
        " !!! cargo make has been deprecated by zellij !!!

Our build system is now `cargo xtask`. Don't worry, you won't have to install
anything!

- To get an overview of the new build tasks, run `cargo xtask --help`
- Quick compatibility table:

| cargo make task                 | cargo xtask equivalent        |
| ------------------------------- | ----------------------------- |
| make                            | xtask                         |
| make format                     | xtask format                  |
| make build                      | xtask build                   |
| make test                       | xtask test                    |
| make run                        | xtask run                     |
| make run -l strider             | xtask run -- -l strider       |
| make clippy                     | xtask clippy                  |
| make clippy -W clippy::pedantic | N/A                           |
| make install /path/to/binary    | xtask install /path/to/binary |
| make publish                    | xtask publish                 |
| make manpage                    | xtask manpage                 |
"
    ))
}
