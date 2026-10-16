use crate::flags;
use anyhow::Context;
use xshell::{cmd, Shell};

pub fn integration_test(sh: &Shell, flags: flags::IntegrationTest) -> anyhow::Result<()> {
    let err_context = "failed to run task 'integration-test'";

    let _pd = sh.push_dir(crate::project_root());
    let cargo = crate::cargo().context(err_context)?;

    let msg = ">> Running whole-app integration tests";
    crate::status(msg);
    println!("{}", msg);

    crate::build::ensure_plugin_assets(sh).context(err_context)?;

    let profile = if flags.no_opt { "dev" } else { "dev-opt" };

    if desktop_tests_wanted(&flags.args) {
        let msg = ">> Building the zellij executable for the desktop app tests";
        crate::status(msg);
        println!("{}", msg);
        cmd!(
            sh,
            "{cargo} build --profile {profile} -p zellij --bin zellij"
        )
        .run()
        .context(err_context)?;
    }

    let nextest_available = cmd!(sh, "{cargo} nextest --version")
        .quiet()
        .ignore_stderr()
        .read()
        .is_ok();
    if nextest_available {
        let serial_args: &[&str] = if flags.serial {
            &["--test-threads=1"]
        } else {
            &[]
        };
        cmd!(
            sh,
            "{cargo} nextest run --cargo-profile {profile} -p zellij-integration-tests"
        )
        .args(serial_args)
        .args(&flags.args)
        .run()
        .context(err_context)?;
    } else {
        println!(">> cargo-nextest not found, falling back to `cargo test -- --test-threads=1`");
        cmd!(
            sh,
            "{cargo} test --profile {profile} -p zellij-integration-tests -- --test-threads=1"
        )
        .args(&flags.args)
        .run()
        .context(err_context)?;
    }
    Ok(())
}

const DESKTOP_TESTS: &str = "desktop_app";
const REQUIRE_DESKTOP_TESTS: &str = "ZELLIJ_REQUIRE_DESKTOP_TESTS";

fn desktop_tests_wanted(args: &[std::ffi::OsString]) -> bool {
    let can_run = std::env::var_os(REQUIRE_DESKTOP_TESTS).is_some() || xvfb_installed();
    let selected = args.is_empty()
        || args
            .iter()
            .any(|arg| arg.to_string_lossy().contains(DESKTOP_TESTS));
    let excluded = args.iter().any(|arg| {
        let arg = arg.to_string_lossy().replace(' ', "");
        arg.contains(&format!("notbinary({})", DESKTOP_TESTS))
            || arg.contains(&format!("!binary({})", DESKTOP_TESTS))
    });
    can_run && (selected || !args.iter().any(|arg| arg == "--test")) && !excluded
}

fn xvfb_installed() -> bool {
    std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).any(|dir| dir.join("Xvfb").is_file()))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<std::ffi::OsString> {
        list.iter().map(std::ffi::OsString::from).collect()
    }

    #[test]
    fn the_desktop_executable_is_built_only_when_its_tests_can_run() {
        std::env::set_var(REQUIRE_DESKTOP_TESTS, "1");
        assert!(desktop_tests_wanted(&args(&[])));
        assert!(desktop_tests_wanted(&args(&["--test", "desktop_app"])));
        assert!(desktop_tests_wanted(&args(&["some_filter"])));
        assert!(!desktop_tests_wanted(&args(&["--test", "app_panel"])));
        assert!(!desktop_tests_wanted(&args(&[
            "-E",
            "not binary(desktop_app)"
        ])));
        assert!(desktop_tests_wanted(&args(&[
            "-E",
            "binary(app_notifications) | binary(desktop_app)"
        ])));
        std::env::remove_var(REQUIRE_DESKTOP_TESTS);
    }
}
