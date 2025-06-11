// TODO: gate this file behind web_server_compatibility
use crate::consts::{WEBSERVER_SOCKET_PATH, ZELLIJ_SOCK_DIR};
use crate::errors::prelude::*;
use crate::ipc::{
    create_webserver_sender, send_webserver_instruction, ClientToServerMsg,
    InstructionForWebServer, IpcSenderWithContext,
};
use std::fs;
use std::os::unix::fs::FileTypeExt;

pub fn shutdown_all_webserver_instances() -> Result<()> {
    let entries = fs::read_dir(&*WEBSERVER_SOCKET_PATH)?;

    for entry in entries {
        let entry = entry?;
        let path = entry.path();

        if let Some(file_name) = path.file_name() {
            if let Some(_file_name_str) = file_name.to_str() {
                let metadata = entry.metadata()?;
                let file_type = metadata.file_type();

                if file_type.is_socket() {
                    match create_webserver_sender(path.to_str().unwrap_or("")) {
                        Ok(mut sender) => {
                            let _ = send_webserver_instruction(
                                &mut sender,
                                InstructionForWebServer::ShutdownWebServer,
                            );
                        },
                        Err(_) => {
                            // no-op
                        },
                    }
                }
            }
        }
    }
    Ok(())
}
