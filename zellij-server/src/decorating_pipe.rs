use std::{
    collections::VecDeque,
    io::{Read, Seek, Write},
};

use log::info;
use serde::{Deserialize, Serialize};
use wasmer_wasi::{WasiFile, WasiFsError};

#[derive(Debug, Serialize, Deserialize)]
pub struct DecoratingPipe {
    buffer: VecDeque<u8>,
    plugin_name: String,
}

impl DecoratingPipe {
    pub fn new(plugin_name: &str) -> DecoratingPipe {
        info!("Creating decorating pipe!");
        dbg!("Creating the decorating pipe :)");
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
        let current_chunk = std::str::from_utf8(buf).unwrap();
        for c in current_chunk.chars() {
            if c == '\n' {
                info!(
                    "{}: {}",
                    self.plugin_name,
                    std::str::from_utf8(&self.buffer.make_contiguous().split_last().unwrap().1)
                        .unwrap()
                );
                self.buffer.clear();
            }
        }
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
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
