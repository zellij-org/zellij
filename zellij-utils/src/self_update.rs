use crate::consts::VERSION;

pub fn run() -> Result<String, Box<dyn std::error::Error>> {
    let status = self_update::backends::github::Update::configure()
        .repo_owner("zellij-org")
        .repo_name("zellij")
        .bin_name("zellij")
        .current_version(VERSION)
        .show_download_progress(true)
        .build()?
        .update()?;

    Ok(status.version().to_string())
}
