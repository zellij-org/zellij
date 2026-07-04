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

    let profile = if flags.no_opt { "dev" } else { "dev-opt" };

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
