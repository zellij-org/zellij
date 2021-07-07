use std::{
    collections::VecDeque,
    io::{Read, Seek, Write},
};

use log::{error, info};
use serde::{Deserialize, Serialize};
use wasmer_wasi::{WasiFile, WasiFsError};
use zellij_utils::logging::debug_log_to_file;

#[derive(Debug, Serialize, Deserialize)]
pub struct DecoratingPipe {
    buffer: VecDeque<u8>,
    plugin_name: String,
}

impl DecoratingPipe {
    pub fn new(plugin_name: &str) -> DecoratingPipe {
        info!("Creating decorating pipe!");
        debug_log_to_file("Creating decorating pipe!".to_string()).expect("xd");
        DecoratingPipe {
            buffer: VecDeque::new(),
            plugin_name: String::from(plugin_name),
        }
    }
}

impl Read for DecoratingPipe {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let amt = std::cmp::min(buf.len(), self.buffer.len());
        for (i, byte) in self.buffer.drain(..amt).enumerate() {
            buf[i] = byte;
        }
        Ok(amt)
    }
}

// TODO: do this better. We're not sure about byte boundaries and endl stuff but, we do expect
// to get the valid thing eventually
impl Write for DecoratingPipe {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.buffer.extend(buf);

        debug_log_to_file(format!(
            "Write called for {}, currentChunk: {}",
            self.plugin_name,
            std::str::from_utf8(buf).unwrap()
        ))
        .expect("xd2");

        Ok(buf.len())
    }

    // When we flush, check if current buffer is valid utf8 string, split by '\n' and truncate buffer in the process.
    // We assume that, eventually, flush will be called on valid string boundary (i.e. std::str::from_utf8(..).is_ok() returns true at some point).
    // Above assumption might not be true, in which case we'll have to think about it. Also, at some point we might actually require some synchronization
    // between write and flush (i.e. concurrent writes and flushes?). Make it simple for now.
    fn flush(&mut self) -> std::io::Result<()> {
        debug_log_to_file(format!(
            "Flush called for {}, buffer: {:?}",
            self.plugin_name, self.buffer
        ))
        .expect("xd3");

        self.buffer.make_contiguous();

        if let Ok(converted_string) = std::str::from_utf8(self.buffer.as_slices().0) {
            if converted_string.contains('\n') {
                let mut consumed_bytes = 0;
                let mut split_msg = converted_string.split('\n').peekable();
                debug_log_to_file(format!(
                    "Back: {}, len: {}, convertedString: {}",
                    split_msg.clone().collect::<String>(),
                    split_msg.clone().count(),
                    converted_string
                ))
                .expect("xD");
                while let Some(msg) = split_msg.next() {
                    if split_msg.peek().is_none() {
                        // Log last chunk iff the last char is endline. Otherwise do not do it.
                        if converted_string.chars().last().unwrap() == '\n' && !msg.is_empty() {
                            info!("special case: {}: {}", self.plugin_name, msg);
                            consumed_bytes += msg.len() + 1;
                        }
                    } else {
                        info!("normal case: {}: {}", self.plugin_name, msg);
                        consumed_bytes += msg.len() + 1;
                    }
                }
                drop(self.buffer.drain(..consumed_bytes));
                debug_log_to_file(format!(
                    "Consumed: {} bytes, buffer: {:?}",
                    consumed_bytes, self.buffer
                ))
                .expect("xd4");
            }
        } else {
            error!("Buffer conversion didn't work. This is unexpected");
        }

        Ok(())
    }
}

impl Seek for DecoratingPipe {
    fn seek(&mut self, _pos: std::io::SeekFrom) -> std::io::Result<u64> {
        Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            "can not seek in a pipe",
        ))
    }
}

#[typetag::serde]
impl WasiFile for DecoratingPipe {
    fn last_accessed(&self) -> u64 {
        0
    }
    fn last_modified(&self) -> u64 {
        0
    }
    fn created_time(&self) -> u64 {
        0
    }
    fn size(&self) -> u64 {
        self.buffer.len() as u64
    }
    fn set_len(&mut self, len: u64) -> Result<(), WasiFsError> {
        self.buffer.resize(len as usize, 0);
        Ok(())
    }
    fn unlink(&mut self) -> Result<(), WasiFsError> {
        Ok(())
    }
    fn bytes_available(&self) -> Result<usize, WasiFsError> {
        Ok(self.buffer.len())
    }
}
