#[cfg(test)]
mod tests {
    use crate::os_input_output::{ClientOsApi, StdinPoller};
    use crate::stdin_handler::might_have_more_data;
    use zellij_utils::{
        data::Palette,
        errors::ErrorContext,
        ipc::{ClientToServerMsg, ServerToClientMsg},
        pane_size::Size,
        shared::default_palette,
    };

    #[test]
    fn test_might_have_more_data_complete_sequences() {
        // Complete sequences should not require more data
        assert_eq!(might_have_more_data(b"hello"), false);
        assert_eq!(might_have_more_data(b"\x1b[31m"), false); // Complete color sequence
        assert_eq!(might_have_more_data(b"\x1b[H"), false); // Complete cursor home
        assert_eq!(might_have_more_data(b"\x1b[2J"), false); // Complete clear screen
        assert_eq!(might_have_more_data(b"\x1b[<0;10;5M"), false); // Complete mouse click
        assert_eq!(might_have_more_data(b"a"), false); // Single character
        assert_eq!(might_have_more_data(b""), false); // Empty buffer
    }

    #[test]
    fn test_might_have_more_data_alt_keys() {
        // Alt+[ should be treated as complete (special case)
        assert_eq!(might_have_more_data(b"\x1b["), false);

        // Other Alt+key combinations should be treated as complete
        assert_eq!(might_have_more_data(b"\x1ba"), false);
        assert_eq!(might_have_more_data(b"\x1bO"), false);
        assert_eq!(might_have_more_data(b"\x1b]"), false);
        assert_eq!(might_have_more_data(b"\x1b0"), false);
        assert_eq!(might_have_more_data(b"\x1bz"), false);
    }

    #[test]
    fn test_might_have_more_data_incomplete_sequences() {
        // Incomplete sequences should require more data
        assert_eq!(might_have_more_data(b"\x1b"), false); // Just ESC is send immediately
        assert_eq!(might_have_more_data(b"\x1b[3"), true); // Incomplete color
        assert_eq!(might_have_more_data(b"\x1b[31"), true); // Incomplete color
        assert_eq!(might_have_more_data(b"\x1b[<0"), true); // Incomplete mouse
        assert_eq!(might_have_more_data(b"\x1b[<0;10"), true); // Incomplete mouse
        assert_eq!(might_have_more_data(b"\x1b[2"), true); // Incomplete clear screen
    }

    #[test]
    fn test_might_have_more_data_sequences_at_end() {
        // Incomplete sequences at the end of longer buffers
        assert_eq!(might_have_more_data(b"hello\x1b"), true);
        assert_eq!(might_have_more_data(b"hello\x1b["), true);
        assert_eq!(might_have_more_data(b"hello\x1b[3"), true);

        // Complete sequences at the end should be fine
        assert_eq!(might_have_more_data(b"hello\x1b[31m"), false);
        assert_eq!(might_have_more_data(b"hello\x1b[H"), false);
    }

    #[test]
    fn test_might_have_more_data_mixed_content() {
        // Mixed content with complete sequences
        assert_eq!(might_have_more_data(b"\x1b[31mRED\x1b[0m"), false);
        assert_eq!(might_have_more_data(b"text\x1b[Hmore"), false);

        // Mixed content with incomplete at end
        assert_eq!(might_have_more_data(b"\x1b[31mRED\x1b["), true);
        assert_eq!(might_have_more_data(b"text\x1b[31"), true);
    }

    // Mock for testing stdin loop logic without actual I/O
    #[derive(Debug, Clone)]
    struct MockOsInput {
        data_sequence: Vec<Vec<u8>>,
        current_index: std::sync::Arc<std::sync::Mutex<usize>>,
    }

    impl MockOsInput {
        fn new(data_sequence: Vec<Vec<u8>>) -> Self {
            Self {
                data_sequence,
                current_index: std::sync::Arc::new(std::sync::Mutex::new(0)),
            }
        }
    }

    impl ClientOsApi for MockOsInput {
        fn read_from_stdin(&mut self) -> Result<Vec<u8>, &'static str> {
            let mut index = self.current_index.lock().unwrap();
            if *index >= self.data_sequence.len() {
                // Simulate blocking/waiting for more input
                std::thread::sleep(std::time::Duration::from_millis(100));
                return Ok(vec![]);
            }

            let data = self.data_sequence[*index].clone();
            *index += 1;
            Ok(data)
        }

        fn get_stdout_writer(&self) -> Box<dyn std::io::Write> {
            Box::new(std::io::sink())
        }

        fn stdin_poller(&self) -> StdinPoller {
            StdinPoller::default()
        }

        // Implement other required methods with defaults
        fn get_terminal_size_using_fd(&self, _fd: std::os::unix::io::RawFd) -> Size {
            Size { rows: 24, cols: 80 }
        }

        fn set_raw_mode(&mut self, _fd: std::os::unix::io::RawFd) {}

        fn unset_raw_mode(&self, _fd: std::os::unix::io::RawFd) -> Result<(), nix::Error> {
            Ok(())
        }

        fn get_stdin_reader(&self) -> Box<dyn std::io::BufRead> {
            Box::new(std::io::empty())
        }

        fn update_session_name(&mut self, _new_session_name: String) {}

        fn box_clone(&self) -> Box<dyn ClientOsApi> {
            Box::new(self.clone())
        }

        fn send_to_server(&self, _msg: ClientToServerMsg) {}

        fn recv_from_server(&self) -> Option<(ServerToClientMsg, ErrorContext)> {
            None
        }

        fn handle_signals(&self, _sigwinch_cb: Box<dyn Fn()>, _quit_cb: Box<dyn Fn()>) {}

        fn connect_to_server(&self, _path: &std::path::Path) {}

        fn load_palette(&self) -> Palette {
            default_palette()
        }

        fn enable_mouse(&self) -> anyhow::Result<()> {
            Ok(())
        }

        fn disable_mouse(&self) -> anyhow::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn test_fragmented_mouse_sequence_integration() {
        // Test that fragmented mouse sequence gets properly buffered and processed
        let fragments = vec![
            b"\x1b[<0;10".to_vec(), // First fragment
            b";5M".to_vec(),        // Completing fragment
        ];

        let mut mock_input = MockOsInput::new(fragments);

        // Test the read behavior
        let first_read = mock_input.read_from_stdin().unwrap();
        assert_eq!(first_read, b"\x1b[<0;10");
        assert_eq!(might_have_more_data(&first_read), true);

        let second_read = mock_input.read_from_stdin().unwrap();
        assert_eq!(second_read, b";5M");

        // Combined should be complete
        let mut combined = first_read;
        combined.extend_from_slice(&second_read);
        assert_eq!(combined, b"\x1b[<0;10;5M");
        assert_eq!(might_have_more_data(&combined), false);
    }

    #[test]
    fn test_alt_bracket_immediate_processing() {
        // Test that Alt+[ gets processed immediately without buffering
        let alt_bracket = vec![b"\x1b[".to_vec()];
        let mut mock_input = MockOsInput::new(alt_bracket);

        let read_data = mock_input.read_from_stdin().unwrap();
        assert_eq!(read_data, b"\x1b[");
        assert_eq!(might_have_more_data(&read_data), false); // Should not buffer
    }

    #[test]
    fn test_complete_ansi_sequence_immediate_processing() {
        // Test that complete ANSI sequences don't get buffered
        let complete_sequence = vec![b"\x1b[31mHello\x1b[0m".to_vec()];
        let mut mock_input = MockOsInput::new(complete_sequence);

        let read_data = mock_input.read_from_stdin().unwrap();
        assert_eq!(read_data, b"\x1b[31mHello\x1b[0m");
        assert_eq!(might_have_more_data(&read_data), false);
    }

    #[test]
    fn test_multiple_fragments_accumulation() {
        // Test sequence that fragments into 3 parts (avoiding the Alt+[ special case)
        let fragments = vec![
            b"\x1b[3".to_vec(), // ESC + [ + partial color code
            b"1".to_vec(),      // more partial color code
            b"m".to_vec(),      // Complete color code
        ];

        let mut mock_input = MockOsInput::new(fragments);
        let mut accumulated = Vec::new();

        // First fragment - should trigger buffering
        let fragment1 = mock_input.read_from_stdin().unwrap();
        accumulated.extend_from_slice(&fragment1);
        assert_eq!(might_have_more_data(&accumulated), true);

        // Second fragment - still incomplete (ESC[31 is incomplete)
        let fragment2 = mock_input.read_from_stdin().unwrap();
        accumulated.extend_from_slice(&fragment2);
        assert_eq!(might_have_more_data(&accumulated), true);

        // Third fragment - now complete
        let fragment3 = mock_input.read_from_stdin().unwrap();
        accumulated.extend_from_slice(&fragment3);
        assert_eq!(accumulated, b"\x1b[31m");
        assert_eq!(might_have_more_data(&accumulated), false);
    }

    // Simplified test for the buffering logic
    #[test]
    fn test_fragmentation_buffering_logic() {
        // Test data: fragmented mouse sequence
        let fragments = vec![
            b"\x1b[<0;10".to_vec(), // Incomplete fragment
            b";5M".to_vec(),        // Completing fragment
        ];

        let mut accumulated = Vec::new();

        // First fragment - should be buffered
        accumulated.extend_from_slice(&fragments[0]);
        assert_eq!(might_have_more_data(&accumulated), true);

        // Second fragment - should complete the sequence
        accumulated.extend_from_slice(&fragments[1]);
        assert_eq!(might_have_more_data(&accumulated), false);
        assert_eq!(accumulated, b"\x1b[<0;10;5M");
    }

    #[test]
    fn test_alt_key_special_cases() {
        // Test that Alt+[ is handled specially
        let alt_bracket = b"\x1b[";
        assert_eq!(might_have_more_data(alt_bracket), false);

        // But longer sequences starting with ESC[ should be buffered if incomplete
        let incomplete_csi = b"\x1b[31";
        assert_eq!(might_have_more_data(incomplete_csi), true);

        // Complete CSI sequences should not be buffered
        let complete_csi = b"\x1b[31m";
        assert_eq!(might_have_more_data(complete_csi), false);
    }

    #[test]
    fn test_csi_sequence_detection() {
        // Test various CSI sequence patterns

        // Parameters without terminator - should buffer
        assert_eq!(might_have_more_data(b"\x1b[1;2;3"), true);
        assert_eq!(might_have_more_data(b"\x1b[0"), true);
        assert_eq!(might_have_more_data(b"\x1b[255"), true);

        // Parameters with terminator - should not buffer
        assert_eq!(might_have_more_data(b"\x1b[1;2;3m"), false); // SGR
        assert_eq!(might_have_more_data(b"\x1b[0m"), false); // Reset
        assert_eq!(might_have_more_data(b"\x1b[255H"), false); // Cursor position
        assert_eq!(might_have_more_data(b"\x1b[2J"), false); // Clear screen
        assert_eq!(might_have_more_data(b"\x1b[6n"), false); // Query cursor
    }

    #[test]
    fn test_mouse_sequence_patterns() {
        // Complete mouse sequences
        assert_eq!(might_have_more_data(b"\x1b[<0;10;5M"), false); // Click
        assert_eq!(might_have_more_data(b"\x1b[<0;10;5m"), false); // Release
        assert_eq!(might_have_more_data(b"\x1b[<64;10;5M"), false); // Scroll

        // Incomplete mouse sequences
        assert_eq!(might_have_more_data(b"\x1b[<"), true);
        assert_eq!(might_have_more_data(b"\x1b[<0"), true);
        assert_eq!(might_have_more_data(b"\x1b[<0;"), true);
        assert_eq!(might_have_more_data(b"\x1b[<0;10"), true);
        assert_eq!(might_have_more_data(b"\x1b[<0;10;"), true);
        assert_eq!(might_have_more_data(b"\x1b[<0;10;5"), true);
    }

    #[test]
    fn test_edge_cases() {
        // Empty buffer
        assert_eq!(might_have_more_data(b""), false);

        // ESC followed by non-bracket
        assert_eq!(might_have_more_data(b"\x1bc"), false);

        // Multiple ESC sequences
        assert_eq!(might_have_more_data(b"\x1b[H\x1b["), true); // Complete + incomplete
        assert_eq!(might_have_more_data(b"\x1b[H\x1b[2J"), false); // Both complete

        // Very long buffer with incomplete at end
        let mut long_buf = b"hello world ".repeat(10);
        long_buf.extend_from_slice(b"\x1b[31");
        assert_eq!(might_have_more_data(&long_buf), true);
    }

    #[test]
    fn test_boundary_conditions() {
        // Test the 20-byte search limit
        let mut buf = vec![0u8; 25]; // 25 bytes
        buf[22] = 0x1b; // ESC at position 22
        buf[23] = b'['; // [ at position 23
        buf[24] = b'3'; // incomplete sequence at position 24

        // Should detect the incomplete sequence even near the boundary
        assert_eq!(might_have_more_data(&buf), true);

        // Test with ESC[ exactly at the boundary
        let mut buf2 = vec![0u8; 22];
        buf2.extend_from_slice(b"\x1b[3");
        assert_eq!(might_have_more_data(&buf2), true);
    }

    #[test]
    fn test_non_ascii_sequences() {
        // Test that non-ASCII bytes don't interfere
        assert_eq!(might_have_more_data(b"\xff\xfe\x1b[31"), true);
        assert_eq!(might_have_more_data(b"\xff\xfe\x1b[31m"), false);

        // UTF-8 sequences (this function doesn't handle UTF-8 fragmentation)
        assert_eq!(might_have_more_data(b"\xc3\xa9"), false); // é in UTF-8
    }
}
