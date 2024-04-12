use super::*;

use nix::{pty::openpty, unistd::close};

struct TestTerminal {
    openpty: OpenptyResult,
}

impl TestTerminal {
    pub fn new() -> TestTerminal {
        let openpty = openpty(None, None).expect("Could not create openpty");
        TestTerminal { openpty }
    }

    #[allow(dead_code)]
    pub fn master(&self) -> RawFd {
        self.openpty.master
    }

    pub fn slave(&self) -> RawFd {
        self.openpty.slave
    }
}

impl Drop for TestTerminal {
    fn drop(&mut self) {
        close(self.openpty.master).expect("Failed to close the master");
        close(self.openpty.slave).expect("Failed to close the slave");
    }
}

#[test]
fn get_cwd() {
    let test_terminal = TestTerminal::new();
    let test_termios =
        termios::tcgetattr(test_terminal.slave()).expect("Could not configure the termios");

    let server = ServerOsInputOutput {
        orig_termios: Arc::new(Mutex::new(Some(test_termios))),
        client_senders: Arc::default(),
        terminal_id_to_raw_fd: Arc::default(),
        cached_resizes: Arc::default(),
    };

    let pid = nix::unistd::getpid();
    assert!(
        server.get_cwd(pid).is_some(),
        "Get current working directory from PID {}",
        pid
    );
}
