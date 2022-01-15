use std::io::prelude::*;
use std::process::{Command, Stdio};

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
    pub fn set(&self, value: String) -> bool {
        let process = match Command::new(self.command.clone())
            .args(self.args.clone())
            .stdin(Stdio::piped())
            .spawn()
        {
            Err(why) => {
                eprintln!("couldn't spawn {}: {}", self.command, why);
                return false;
            }
            Ok(process) => process,
        };

        match process.stdin.unwrap().write_all(value.as_bytes()) {
            Err(why) => {
                eprintln!("couldn't write to {} stdin: {}", self.command, why);
                false
            }
            Ok(_) => true,
        }
    }
}
