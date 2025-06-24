use crate::consts::WEBSERVER_SOCKET_PATH;
use crate::errors::prelude::*;
use crate::input::config::Config;
use interprocess::local_socket::LocalSocketStream;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{self, BufWriter, Write};
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

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum InstructionForWebServer {
    ShutdownWebServer,
    ConfigWrittenToDisk(Config),
}

pub fn create_webserver_sender(path: &str) -> Result<BufWriter<LocalSocketStream>> {
    let stream = LocalSocketStream::connect(path)?;
    Ok(BufWriter::new(stream))
}

pub fn send_webserver_instruction(
    sender: &mut BufWriter<LocalSocketStream>,
    instruction: InstructionForWebServer,
) -> Result<()> {
    rmp_serde::encode::write(sender, &instruction)
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
    sender.flush()?;
    Ok(())
}
