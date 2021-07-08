use std::{
    collections::VecDeque,
    io::{Read, Seek, Write},
};

use log::{error, info};
use serde::{Deserialize, Serialize};
use wasmer_wasi::{WasiFile, WasiFsError};

// 16kB log buffer
const ZELLIJ_MAX_PIPE_BUFFER_SIZE: usize = 16_384;
#[derive(Debug, Serialize, Deserialize)]
pub struct DecoratingPipe {
    buffer: VecDeque<u8>,
    plugin_name: String,
}

impl DecoratingPipe {
    pub fn new(plugin_name: &str) -> DecoratingPipe {
        info!("Creating decorating pipe!");
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

impl Write for DecoratingPipe {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.buffer.extend(buf);

        if self.buffer.len() > ZELLIJ_MAX_PIPE_BUFFER_SIZE {
            let error_msg =
                "Exceeded log buffer size. Make sure that your plugin calls flush on stderr on \
                valid UTF-8 symbol boundary. Aditionally, make sure that your log message contains \
                endline \\n symbol.";
            error!("{}: {}", self.plugin_name, error_msg);
            self.buffer.clear();
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                error_msg,
            ));
        }

        Ok(buf.len())
    }

    // When we flush, check if current buffer is valid utf8 string, split by '\n' and truncate buffer in the process.
    // We assume that eventually, flush will be called on valid string boundary (i.e. std::str::from_utf8(..).is_ok() returns true at some point).
    // Above assumption might not be true, in which case we'll have to think about it. Make it simple for now.
    fn flush(&mut self) -> std::io::Result<()> {
        self.buffer.make_contiguous();

        if let Ok(converted_buffer) = std::str::from_utf8(self.buffer.as_slices().0) {
            if converted_buffer.contains('\n') {
                let mut consumed_bytes = 0;
                let mut split_converted_buffer = converted_buffer.split('\n').peekable();

                while let Some(msg) = split_converted_buffer.next() {
                    if split_converted_buffer.peek().is_none() {
                        // Log last chunk iff the last char is endline. Otherwise do not do it.
                        if converted_buffer.chars().last().unwrap() == '\n' && !msg.is_empty() {
                            info!("{}: {}", self.plugin_name, msg);
                            consumed_bytes += msg.len() + 1;
                        }
                    } else {
                        info!("{}: {}", self.plugin_name, msg);
                        consumed_bytes += msg.len() + 1;
                    }
                }
                drop(self.buffer.drain(..consumed_bytes));
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

// Unit tests
#[cfg(test)]
mod decorating_pipe_test {

    use super::*;

    #[test]
    fn write_without_endl_does_not_consume_buffer_after_flush() {
        let mut pipe = DecoratingPipe::new("TestPipe");

        let test_buffer = "Testing write".as_bytes();

        pipe.write(test_buffer).expect("Err write");
        pipe.flush().expect("Err flush");

        assert_eq!(pipe.buffer.len(), test_buffer.len());
    }

    #[test]
    fn write_with_single_endl_at_the_end_consumes_whole_buffer_after_flush() {
        let mut pipe = DecoratingPipe::new("TestPipe");

        let test_buffer = "Testing write \n".as_bytes();

        pipe.write(test_buffer).expect("Err write");
        pipe.flush().expect("Err flush");

        assert_eq!(pipe.buffer.len(), 0);
    }

    #[test]
    fn write_with_endl_in_the_middle_consumes_buffer_up_to_endl_after_flush() {
        let mut pipe = DecoratingPipe::new("TestPipe");

        let test_buffer = "Testing write \n".as_bytes();
        let test_buffer2 = "And the rest".as_bytes();

        pipe.write(
            [
                test_buffer,
                test_buffer,
                test_buffer,
                test_buffer,
                test_buffer2,
            ]
            .concat()
            .as_slice(),
        )
        .expect("Err write");
        pipe.flush().expect("Err flush");

        assert_eq!(pipe.buffer.len(), test_buffer2.len());
    }

    #[test]
    fn write_with_many_endl_consumes_whole_buffer_after_flush() {
        let mut pipe = DecoratingPipe::new("TestPipe");

        let test_buffer = "Testing write \n".as_bytes();

        pipe.write(
            [
                test_buffer,
                test_buffer,
                test_buffer,
                test_buffer,
                test_buffer,
            ]
            .concat()
            .as_slice(),
        )
        .expect("Err write");
        pipe.flush().expect("Err flush");

        assert_eq!(pipe.buffer.len(), 0);
    }

    #[test]
    fn write_with_incorrect_byte_boundary_does_not_crash() {
        let mut pipe = DecoratingPipe::new("TestPipe");

        let test_buffer = "😱".as_bytes();

        // make sure it's not valid utf-8 string if we drop last symbol
        assert!(std::str::from_utf8(&test_buffer[..test_buffer.len() - 1]).is_err());

        pipe.write(&test_buffer[..test_buffer.len() - 1])
            .expect("Err write");
        pipe.flush().expect("Err flush");

        assert_eq!(pipe.buffer.len(), test_buffer.len() - 1);

        println!("len: {}, buf: {:?}", test_buffer.len(), test_buffer);
    }
}
