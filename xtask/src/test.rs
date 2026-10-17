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
            args: vec![],
        },
    )
    .context(err_context)?;

    // Test all plugins in a single invocation so Cargo unifies features and compiles
    // shared crates (e.g. zellij-utils) only once, mirroring how plugins are built.
    {
        let plugin_names: Vec<&str> = crate::workspace_members()
            .iter()
            .filter(|m| m.crate_name.contains("plugins"))
            .map(|m| m.crate_name.rsplit_once('/').unwrap().1)
            .collect();

        if !plugin_names.is_empty() {
            println!();
            let msg = ">> Testing plugins";
            crate::status(msg);
            println!("{}", msg);

            let mut cmd = cmd!(sh, "{cargo} test --target {host_triple}");
            for name in &plugin_names {
                cmd = cmd.args(["-p", name]);
            }
            cmd.arg("--")
                .args(&flags.args)
                .run()
                .context("Failed to run plugin tests")?;
        }
    }

    let mut excluded: Vec<&str> = Vec::new();
    if flags.no_web {
        excluded.push(metadata::WEB_FEATURE);
    }
    if flags.no_window {
        excluded.push(metadata::WINDOW_FEATURE);
    }

    for WorkspaceMember { crate_name, .. } in crate::workspace_members()
        .iter()
        .filter(|m| !m.crate_name.contains("plugins"))
        .filter(|m| !(flags.no_window && m.crate_name == "zellij-window"))
    {
        let _pd = sh.push_dir(Path::new(crate_name));
        println!();
        let msg = format!(">> Testing '{}'", crate_name);
        crate::status(&msg);
        println!("{}", msg);

        let cmd = if excluded.is_empty() {
            cmd!(sh, "{cargo} test --all-features --")
        } else {
            match metadata::get_features_without(sh, crate_name, &excluded)
                .context("Failed to check features")?
            {
                Some(features) => {
                    if features.is_empty() {
                        cmd!(sh, "{cargo} test --no-default-features --")
                    } else {
                        cmd!(sh, "{cargo} test --no-default-features --features")
                            .arg(features)
                            .arg("--")
                    }
                },
                None => {
                    cmd!(sh, "{cargo} test --all-features --")
                },
            }
        };

        cmd.args(&flags.args)
            .run()
            .with_context(|| format!("Failed to run tests for '{}'", crate_name))?;
    }

    if !flags.no_window {
        check_feature_is_optional(sh, &cargo, &excluded, metadata::WINDOW_FEATURE)
            .context(err_context)?;
    }
    if !flags.no_web {
        check_feature_is_optional(sh, &cargo, &excluded, metadata::WEB_FEATURE)
            .context(err_context)?;
    }

    Ok(())
}

fn check_feature_is_optional(
    sh: &Shell,
    cargo: &std::path::PathBuf,
    already_excluded: &[&str],
    feature: &str,
) -> anyhow::Result<()> {
    let mut excluded = already_excluded.to_vec();
    excluded.push(feature);

    let Some(features) =
        metadata::get_features_without(sh, ".", &excluded).context("Failed to check features")?
    else {
        return Ok(());
    };

    let _pd = sh.push_dir(crate::project_root());
    println!();
    let msg = format!(">> Checking the build without '{}'", feature);
    crate::status(&msg);
    println!("{}", msg);

    let cmd = if features.is_empty() {
        cmd!(sh, "{cargo} check --all-targets --no-default-features")
    } else {
        cmd!(
            sh,
            "{cargo} check --all-targets --no-default-features --features"
        )
        .arg(features)
    };

    cmd.run()
        .with_context(|| format!("The build without '{}' does not compile", feature))
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
