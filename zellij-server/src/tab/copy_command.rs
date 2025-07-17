use std::io::prelude::*;
use std::process::{Command, Stdio};

use anyhow::{Context, Result};

pub struct CopyCommand {
    command: String,
    args: Vec<String>,
}

impl CopyCommand {
    pub fn new(command: String) -> Self {
        let mut command_with_args = command.split(' ').map(String::from);

        Self {
            command: command_with_args.next().expect("missing command"),
            args: command_with_args.collect(),
        }
    }
    pub fn set(&self, value: String) -> Result<()> {
        let mut process = Command::new(self.command.clone())
            .args(self.args.clone())
            .stdin(Stdio::piped())
            .spawn()
            .with_context(|| format!("couldn't spawn {}", self.command))?;
        process
            .stdin
            .take()
            .context("could not get stdin")?
            .write_all(value.as_bytes())
            .with_context(|| format!("couldn't write to {} stdin", self.command))?;

        // reap process with a 1 second timeout
        std::thread::spawn(move || {
            let timeout = std::time::Duration::from_secs(1);
            let start = std::time::Instant::now();

            loop {
                match process.try_wait() {
                    Ok(Some(_)) => {
                        return; // Process finished normally
                    },
                    Ok(None) => {
                        if start.elapsed() > timeout {
                            let _ = process.kill();
                            log::error!("Copy operation times out after 1 second");
                            return;
                        }
                        std::thread::sleep(std::time::Duration::from_millis(50));
                    },
                    Err(e) => {
                        log::error!("Clipboard failure: {}", e);
                        return;
                    },
                }
            }
        });

        Ok(())
    }
}
