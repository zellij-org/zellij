mod cli;
mod common;
#[cfg(test)]
mod tests;
// TODO mod server;
mod client;

use crate::cli::CliArgs;
use crate::command_is_executing::CommandIsExecuting;
use crate::os_input_output::get_os_input;
use crate::utils::{
    consts::{ZELLIJ_IPC_PIPE, ZELLIJ_TMP_DIR, ZELLIJ_TMP_LOG_DIR},
    logging::*,
};
use client::{boundaries, layout, panes, tab};
use common::{
    command_is_executing, errors, install, os_input_output, screen, start, utils, wasm_vm,
    ApiCommand,
};
use std::io::Write;
use std::os::unix::net::UnixStream;
use structopt::StructOpt;

pub fn main() {
    let opts = CliArgs::from_args();
    if let Some(split_dir) = opts.split {
        match split_dir {
            'h' => {
                let mut stream = UnixStream::connect(ZELLIJ_IPC_PIPE).unwrap();
                let api_command = bincode::serialize(&ApiCommand::SplitHorizontally).unwrap();
                stream.write_all(&api_command).unwrap();
            }
            'v' => {
                let mut stream = UnixStream::connect(ZELLIJ_IPC_PIPE).unwrap();
                let api_command = bincode::serialize(&ApiCommand::SplitVertically).unwrap();
                stream.write_all(&api_command).unwrap();
            }
            _ => {}
        };
    } else if opts.move_focus {
        let mut stream = UnixStream::connect(ZELLIJ_IPC_PIPE).unwrap();
        let api_command = bincode::serialize(&ApiCommand::MoveFocus).unwrap();
        stream.write_all(&api_command).unwrap();
    } else if let Some(file_to_open) = opts.open_file {
        let mut stream = UnixStream::connect(ZELLIJ_IPC_PIPE).unwrap();
        let api_command = bincode::serialize(&ApiCommand::OpenFile(file_to_open)).unwrap();
        stream.write_all(&api_command).unwrap();
    } else if let Some(crate::cli::ConfigCli::GenerateCompletion { shell }) = opts.option {
        let shell = match shell.as_ref() {
            "bash" => structopt::clap::Shell::Bash,
            "fish" => structopt::clap::Shell::Fish,
            "zsh" => structopt::clap::Shell::Zsh,
            "powerShell" => structopt::clap::Shell::PowerShell,
            "elvish" => structopt::clap::Shell::Elvish,
            other => {
                eprintln!("Unsupported shell: {}", other);
                std::process::exit(1);
            }
        };
        let mut out = std::io::stdout();
        CliArgs::clap().gen_completions_to("zellij", shell, &mut out);
    } else if let Some(crate::cli::ConfigCli::Setup { .. }) = opts.option {
        install::dump_default_config().expect("Failed to print to stdout");
        std::process::exit(1);
    } else {
        let os_input = get_os_input();
        atomic_create_dir(ZELLIJ_TMP_DIR).unwrap();
        atomic_create_dir(ZELLIJ_TMP_LOG_DIR).unwrap();
        start(Box::new(os_input), opts);
    }
}
