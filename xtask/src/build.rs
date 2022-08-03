use xshell::{cmd, Shell};
use crate::flags;
use std::path::Path;

pub fn build(sh: &Shell, flags: flags::Build) -> anyhow::Result<()> {
    let cargo = crate::cargo();

    for subcrate in crate::WORKSPACE_MEMBERS.iter() {
        let _pd = sh.push_dir(Path::new(subcrate));
        // Tell the user where we are now
        println!("");
        println!(">> Building '{}'", subcrate);

        let mut base_cmd = cmd!(sh, "{cargo} build");
        if flags.release {
            base_cmd = base_cmd.arg("--release");
        }
        base_cmd.run()?;
    }
    Ok(())
}
