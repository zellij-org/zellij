#[cfg(test)]
mod tests;

mod cli;
mod client;
mod common;
mod server;

use client::{boundaries, layout, panes, tab};
use common::{
    command_is_executing, errors, ipc, os_input_output, pty_bus, screen, start, utils, wasm_vm,
    ApiCommand, IpcSenderWithContext,
};
use directories_next::ProjectDirs;

use structopt::StructOpt;

use crate::cli::CliArgs;
use crate::command_is_executing::CommandIsExecuting;
use crate::os_input_output::get_os_input;
use crate::utils::{
    consts::{ZELLIJ_TMP_DIR, ZELLIJ_TMP_LOG_DIR},
    logging::*,
};

pub fn main() {
    // First run installation of default plugins & layouts
    let project_dirs = ProjectDirs::from("org", "Zellij Contributors", "Zellij").unwrap();
    let data_dir = project_dirs.data_dir();
    let mut assets = asset_map! {
        "assets/layouts/default.yaml" => "layouts/default.yaml",
        "assets/layouts/strider.yaml" => "layouts/strider.yaml",
    };
    assets.extend(asset_map! {
        "assets/plugins/status-bar.wasm" => "plugins/status-bar.wasm",
        "assets/plugins/tab-bar.wasm" => "plugins/tab-bar.wasm",
        "assets/plugins/strider.wasm" => "plugins/strider.wasm",
    });

    for (path, bytes) in assets {
        let path = data_dir.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        if !path.exists() {
            std::fs::write(path, bytes).expect("Failed to install default assets!");
        }
    }

    let opts = CliArgs::from_args();
    if let Some(split_dir) = opts.split {
        match split_dir {
            'h' => {
                let mut send_server_instructions = IpcSenderWithContext::new();
                send_server_instructions
                    .send(ApiCommand::SplitHorizontally)
                    .unwrap();
            }
            'v' => {
                let mut send_server_instructions = IpcSenderWithContext::new();
                send_server_instructions
                    .send(ApiCommand::SplitVertically)
                    .unwrap();
            }
            _ => {}
        };
    } else if opts.move_focus {
        let mut send_server_instructions = IpcSenderWithContext::new();
        send_server_instructions
            .send(ApiCommand::MoveFocus)
            .unwrap();
    } else if let Some(file_to_open) = opts.open_file {
        let mut send_server_instructions = IpcSenderWithContext::new();
        send_server_instructions
            .send(ApiCommand::OpenFile(file_to_open))
            .unwrap();
    } else {
        let os_input = get_os_input();
        atomic_create_dir(ZELLIJ_TMP_DIR).unwrap();
        atomic_create_dir(ZELLIJ_TMP_LOG_DIR).unwrap();
        start(Box::new(os_input), opts);
    }
}
