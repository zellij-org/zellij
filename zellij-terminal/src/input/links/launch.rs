use std::process::{Command, Stdio};

#[cfg_attr(windows, path = "launch/windows.rs")]
#[cfg_attr(not(windows), path = "launch/other.rs")]
mod native;

pub(crate) fn launch(opener: &str, uri: &str) -> Result<(), String> {
    if let Some(opened) = native::open(opener, uri) {
        return opened;
    }
    let child = Command::new(opener)
        .arg(uri)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("could not run {}: {}", opener, e))?;
    reap(child);
    Ok(())
}

fn reap(mut child: std::process::Child) {
    std::thread::Builder::new()
        .name("zellij-terminal-open".to_owned())
        .spawn(move || {
            let _ = child.wait();
        })
        .ok();
}
