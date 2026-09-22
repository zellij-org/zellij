use std::io::{self, BufRead, Read};

// Host messages are newline-delimited JSON. Stream one message without keeping
// its JSON representation alongside the decoded value in the plugin's memory.
pub(crate) fn read_bytes(reader: &mut impl BufRead) -> serde_json::Result<Vec<u8>> {
    let mut line = JsonLine {
        reader,
        ended: false,
    };
    let result = serde_json::from_reader(&mut line);
    // A malformed message must not leave the next call in the middle of a line.
    io::copy(&mut line, &mut io::sink()).map_err(serde_json::Error::io)?;
    result
}

struct JsonLine<'a, R> {
    reader: &'a mut R,
    ended: bool,
}

impl<R: BufRead> Read for JsonLine<'_, R> {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        if self.ended || output.is_empty() {
            return Ok(0);
        }
        let available = self.reader.fill_buf()?;
        let count = available.len().min(output.len());
        let count = match available[..count].iter().position(|b| *b == b'\n') {
            Some(position) => {
                self.ended = true;
                position + 1
            },
            None => count,
        };
        output[..count].copy_from_slice(&available[..count]);
        self.reader.consume(count);
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufReader, Cursor};

    #[test]
    fn reads_one_message_at_a_time_with_small_buffers() {
        let mut reader = BufReader::with_capacity(2, Cursor::new(b"[0,255,10]\r\n[42]\n[]"));
        assert_eq!(read_bytes(&mut reader).unwrap(), vec![0, 255, 10]);
        assert_eq!(read_bytes(&mut reader).unwrap(), vec![42]);
        assert_eq!(read_bytes(&mut reader).unwrap(), Vec::<u8>::new());
        assert!(read_bytes(&mut reader).is_err());
    }

    #[test]
    fn malformed_messages_do_not_consume_the_next_message() {
        for invalid in ["[256]", "[1,", "[1] garbage", "", "null"] {
            let input = format!("{}\n[42]\n", invalid);
            let mut reader = Cursor::new(input);
            assert!(read_bytes(&mut reader).is_err(), "{}", invalid);
            assert_eq!(read_bytes(&mut reader).unwrap(), vec![42]);
        }
    }

    #[test]
    fn leaves_following_object_messages_for_the_existing_reader() {
        let mut reader = Cursor::new(b"[1,2]\n{\"name\":\"next\"}\n");
        assert_eq!(read_bytes(&mut reader).unwrap(), vec![1, 2]);
        let mut next = String::new();
        reader.read_line(&mut next).unwrap();
        assert_eq!(next, "{\"name\":\"next\"}\n");
    }

    #[test]
    fn propagates_io_errors() {
        struct Broken;
        impl Read for Broken {
            fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
                Err(io::Error::other("broken pipe"))
            }
        }
        let error = read_bytes(&mut BufReader::new(Broken)).unwrap_err();
        assert!(error.is_io());
        assert!(error.to_string().contains("broken pipe"));
    }

    #[test]
    fn reads_large_byte_arrays_without_changing_contents() {
        let expected: Vec<u8> = (0..1_500_000).map(|n| (n % 256) as u8).collect();
        let mut input = serde_json::to_vec(&expected).unwrap();
        input.push(b'\n');
        let mut reader = BufReader::with_capacity(1024, Cursor::new(input));
        assert_eq!(read_bytes(&mut reader).unwrap(), expected);
    }
}
