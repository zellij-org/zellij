use crate::stdin_ansi_parser::SyncOutput;
use crate::write_render_output;
use std::io::{self, Write};

#[derive(Default)]
struct TestWriter {
    output: Vec<u8>,
    flush_error: bool,
}

impl Write for TestWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.output.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        if self.flush_error {
            Err(io::Error::other(
                "insufficient system resources to flush stdout",
            ))
        } else {
            Ok(())
        }
    }
}

#[test]
fn render_output_wraps_content_in_synchronised_output_sequences() {
    let mut writer = TestWriter::default();

    write_render_output(&mut writer, b"rendered content", Some(SyncOutput::CSI)).unwrap();

    let mut expected = SyncOutput::CSI.start_seq().to_vec();
    expected.extend_from_slice(b"rendered content");
    expected.extend_from_slice(SyncOutput::CSI.end_seq());
    assert_eq!(writer.output, expected);
}

#[test]
fn render_output_returns_stdout_flush_errors() {
    let mut writer = TestWriter {
        flush_error: true,
        ..Default::default()
    };

    let error = write_render_output(&mut writer, b"rendered content", None).unwrap_err();

    assert_eq!(error.kind(), io::ErrorKind::Other);
}
