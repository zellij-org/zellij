use crate::{build, flags, WorkspaceMember};
use anyhow::{anyhow, Context};
use std::path::Path;
use xshell::{cmd, Shell};

pub fn test(sh: &Shell, flags: flags::Test) -> anyhow::Result<()> {
    let err_context = "failed to run task 'test'";

    let _pdo = sh.push_dir(crate::project_root());
    let cargo = crate::cargo().context(err_context)?;
    let host_triple = host_target_triple(sh).context(err_context)?;

    build::build(
        sh,
        flags::Build {
            release: false,
            no_plugins: false,
            plugins_only: true,
            no_web: false,
        },
    )
    .context(err_context)?;

    for WorkspaceMember { crate_name, .. } in crate::workspace_members().iter() {
        // the workspace root only contains e2e tests, skip it
        if crate_name == &"." {
            continue;
        }

        let _pd = sh.push_dir(Path::new(crate_name));
        // Tell the user where we are now
        println!();
        let msg = format!(">> Testing '{}'", crate_name);
        crate::status(&msg);
        println!("{}", msg);

        // Override wasm32-wasip1 target for plugins only
        let cmd = if crate_name.contains("plugins") {
            cmd!(sh, "{cargo} test --target {host_triple} --")
        } else {
            cmd!(sh, "{cargo} test --all-features --")
        };

        cmd.args(&flags.args)
            .run()
            .with_context(|| format!("Failed to run tests for '{}'", crate_name))?;
    }
    Ok(())
}

// Determine the target triple of the host. We explicitly run all tests against the host
// architecture so we can test the plugins, too (they default to wasm32-wasip1 otherwise).
pub fn host_target_triple(sh: &Shell) -> anyhow::Result<String> {
    let rustc_ver = cmd!(sh, "rustc -vV")
        .read()
        .context("Failed to determine host triple")?;
    let maybe_triple = rustc_ver
        .lines()
        .filter_map(|line| {
            if !line.starts_with("host") {
                return None;
            }
            if let Some((_, triple)) = line.split_once(": ") {
                Some(triple.to_string())
            } else {
                None
            }
        })
        .collect::<Vec<String>>();
    match maybe_triple.len() {
        0 => Err(anyhow!("rustc didn't output the 'host' triple")),
        1 => Ok(maybe_triple.into_iter().next().unwrap()),
        _ => Err(anyhow!(
            "rustc provided multiple host triples: {:?}",
            maybe_triple
        )),
    }
}
