//! See <https://github.com/matklad/cargo-xtask/>.
//!
//! This binary defines various auxiliary build commands, which are not expressible with just
//! `cargo`. Notably, it provides tests via `cargo test -p xtask` for code generation and `cargo
//! xtask install` for installation of rust-analyzer server and client.
//!
//! This binary is integrated into the `cargo` command line by using an alias in `.cargo/config`.

mod build;
mod ci;
mod clippy;
mod dist;
mod flags;
mod format;
mod metadata;
mod pipelines;
mod test;

use anyhow::Context;
use std::{
    env,
    path::{Path, PathBuf},
    sync::OnceLock,
    time::Instant,
};
use xshell::Shell;

pub struct WorkspaceMember {
    crate_name: &'static str,
    build: bool,
}

fn workspace_members() -> &'static Vec<WorkspaceMember> {
    static WORKSPACE_MEMBERS: OnceLock<Vec<WorkspaceMember>> = OnceLock::new();
    WORKSPACE_MEMBERS.get_or_init(|| {
        vec![
            WorkspaceMember {
                crate_name: "default-plugins/compact-bar",
                build: true,
            },
            WorkspaceMember {
                crate_name: "default-plugins/status-bar",
                build: true,
            },
            WorkspaceMember {
                crate_name: "default-plugins/strider",
                build: true,
            },
            WorkspaceMember {
                crate_name: "default-plugins/tab-bar",
                build: true,
            },
            WorkspaceMember {
                crate_name: "default-plugins/fixture-plugin-for-tests",
                build: true,
            },
            WorkspaceMember {
                crate_name: "default-plugins/session-manager",
                build: true,
            },
            WorkspaceMember {
                crate_name: "default-plugins/configuration",
                build: true,
            },
            WorkspaceMember {
                crate_name: "default-plugins/plugin-manager",
                build: true,
            },
            WorkspaceMember {
                crate_name: "default-plugins/about",
                build: true,
            },
            WorkspaceMember {
                crate_name: "default-plugins/multiple-select",
                build: true,
            },
            WorkspaceMember {
                crate_name: "default-plugins/share",
                build: true,
            },
            WorkspaceMember {
                crate_name: "default-plugins/layout-manager",
                build: true,
            },
            WorkspaceMember {
                crate_name: "zellij-utils",
                build: false,
            },
            WorkspaceMember {
                crate_name: "zellij-tile-utils",
                build: false,
            },
            WorkspaceMember {
                crate_name: "zellij-tile",
                build: false,
            },
            WorkspaceMember {
                crate_name: "zellij-client",
                build: false,
            },
            WorkspaceMember {
                crate_name: "zellij-server",
                build: false,
            },
            WorkspaceMember {
                crate_name: ".",
                build: true,
            },
        ]
    })
}

fn main() -> anyhow::Result<()> {
    let shell = &Shell::new()?;

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

fn asset_dir() -> PathBuf {
    crate::project_root().join("zellij-utils").join("assets")
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


In order to disable xtask during the transitioning period: Delete/comment the
`[alias]` section in `.cargo/config.toml` and use `cargo make` as before.
If you're unhappy with `xtask` and decide to disable it, please tell us why so
we can discuss this before making it final for the next release. Thank you!
"
    ))
}
