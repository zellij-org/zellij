use crate::{build, flags, metadata, WorkspaceMember};
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
            no_web: flags.no_web,
        },
    )
    .context(err_context)?;

    for WorkspaceMember { crate_name, .. } in crate::workspace_members().iter() {
        let _pd = sh.push_dir(Path::new(crate_name));
        println!();
        let msg = format!(">> Testing '{}'", crate_name);
        crate::status(&msg);
        println!("{}", msg);

        let cmd = if crate_name.contains("plugins") {
            cmd!(sh, "{cargo} test --target {host_triple} --")
        } else if flags.no_web {
            // Check if this crate has web features that need modification
            match metadata::get_no_web_features(sh, crate_name)
                .context("Failed to check web features")?
            {
                Some(features) => {
                    if features.is_empty() {
                        // Crate has web_server_capability but no other applicable features
                        cmd!(sh, "{cargo} test --no-default-features --")
                    } else {
                        cmd!(sh, "{cargo} test --no-default-features --features")
                            .arg(features)
                            .arg("--")
                    }
                },
                None => {
                    // Crate doesn't have web features, use normal test
                    cmd!(sh, "{cargo} test --all-features --")
                },
            }
        } else {
            cmd!(sh, "{cargo} test --all-features --")
        };

        cmd.args(&flags.args)
            .run()
            .with_context(|| format!("Failed to run tests for '{}'", crate_name))?;
    }
    Ok(())
}

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
