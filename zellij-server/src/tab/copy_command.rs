use std::io::prelude::*;
use std::process::{Command, Stdio};

use zellij_utils::anyhow::{Context, Result};

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
        let process = Command::new(self.command.clone())
            .args(self.args.clone())
            .stdin(Stdio::piped())
            .spawn()
            .with_context(|| format!("couldn't spawn {}", self.command))?;
        process
            .stdin
            .context("could not get stdin")?
            .write_all(value.as_bytes())
            .with_context(|| format!("couldn't write to {} stdin", self.command))?;

        Ok(())
    }
}
