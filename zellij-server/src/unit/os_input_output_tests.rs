use super::*;

use std::fs::{File, OpenOptions};
use std::os::unix::io::AsRawFd;

pub struct Vt {
    f: File,
}

impl Vt {
    pub fn new() -> Result<Vt, std::io::Error> {
        OpenOptions::new()
            .read(true)
            .write(true)
            .open("/dev/tty")
            .map(|f| Vt { f })
    }

    pub fn get_raw_fd(&self) -> RawFd {
        self.f.as_raw_fd()
    }
}

#[test]
fn get_cwd() {
    let vt = match Vt::new() {
        Ok(vt) => vt,
        Err(e) => panic!("Failed to open /dev/tty: {}", e),
    };

    let server = ServerOsInputOutput {
        orig_termios: Arc::new(Mutex::new(termios::tcgetattr(vt.get_raw_fd()).unwrap())),
        client_senders: Arc::default(),
    };

    let pid = nix::unistd::getpid();
    assert!(
        server.get_cwd(pid).is_some(),
        "Get current working directory from PID {}",
        pid
    )
}
