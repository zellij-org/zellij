//! On Windows a connection is two named pipes: the request pipe, and a reply pipe for the answers
//! (a blocking pipe handle cannot read and write at the same time). The server accepts each on
//! its own listener, so it has to put the two halves of a connection back together.
//!
//! Pairing them by arrival order breaks as soon as one client connects its request pipe and
//! leaves before connecting its reply pipe: every later client then gets the previous one's
//! answers (#5740). So the halves are paired by the process that connected them instead, and a
//! process only connects one pair to the same pipe at a time.

use std::collections::{HashMap, VecDeque};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

use interprocess::local_socket::{Listener, Stream};

/// How long half a connection waits for its other half before it is dropped.
const MAX_WAIT: Duration = Duration::from_secs(30);

const ACCEPT_ERROR_BACKOFF: Duration = Duration::from_millis(100);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Half {
    Request,
    Reply,
}

struct Waiting<T> {
    process: u32,
    stream: T,
    since: Instant,
}

/// The halves of connections that are still waiting for their other half.
pub struct Pairing<T> {
    requests: VecDeque<Waiting<T>>,
    replies: VecDeque<Waiting<T>>,
}

impl<T> Default for Pairing<T> {
    fn default() -> Self {
        Pairing {
            requests: VecDeque::new(),
            replies: VecDeque::new(),
        }
    }
}

impl<T> Pairing<T> {
    /// Adds one half of a connection from `process`. Returns the whole connection, request half
    /// first, once its other half is there. Halves whose client is `gone`, or that waited too
    /// long, are dropped first, so they can never take another connection's half.
    pub fn add(
        &mut self,
        half: Half,
        process: u32,
        stream: T,
        now: Instant,
        gone: impl Fn(&T) -> bool,
    ) -> Option<(T, T)> {
        self.forget(now, &gone);
        let (mine, others) = match half {
            Half::Request => (&mut self.requests, &mut self.replies),
            Half::Reply => (&mut self.replies, &mut self.requests),
        };
        match others.iter().position(|other| other.process == process) {
            Some(index) => {
                let other = others.remove(index)?.stream;
                Some(match half {
                    Half::Request => (stream, other),
                    Half::Reply => (other, stream),
                })
            },
            None => {
                mine.push_back(Waiting {
                    process,
                    stream,
                    since: now,
                });
                None
            },
        }
    }

    fn forget(&mut self, now: Instant, gone: &impl Fn(&T) -> bool) {
        for waiting in [&mut self.requests, &mut self.replies] {
            waiting.retain(|half| {
                now.saturating_duration_since(half.since) < MAX_WAIT && !gone(&half.stream)
            });
        }
    }

    #[cfg(test)]
    fn waiting(&self) -> (usize, usize) {
        (self.requests.len(), self.replies.len())
    }
}

/// Accepts the request and reply halves of connections on their two listeners and hands out each
/// connection once both of its halves are there, request half first.
pub fn accept_pairs(requests: Listener, replies: Listener) -> mpsc::Receiver<(Stream, Stream)> {
    let (paired, connections) = mpsc::channel();
    let pairing = Arc::new(Mutex::new(Pairing::default()));
    for (half, listener) in [(Half::Request, requests), (Half::Reply, replies)] {
        let pairing = pairing.clone();
        let paired = paired.clone();
        let accepting = std::thread::Builder::new()
            .name(match half {
                Half::Request => "pipe_requests".to_owned(),
                Half::Reply => "pipe_replies".to_owned(),
            })
            .spawn(move || {
                use interprocess::local_socket::traits::Listener as _;
                loop {
                    let stream = match listener.accept() {
                        Ok(stream) => stream,
                        Err(e) => {
                            log::error!("failed to accept a connection: {}", e);
                            std::thread::sleep(ACCEPT_ERROR_BACKOFF);
                            continue;
                        },
                    };
                    let process = client_process(&stream).unwrap_or(0);
                    let connection = pairing.lock().unwrap_or_else(|e| e.into_inner()).add(
                        half,
                        process,
                        stream,
                        Instant::now(),
                        |stream| !connected(stream),
                    );
                    if let Some(connection) = connection {
                        if paired.send(connection).is_err() {
                            return;
                        }
                    }
                }
            });
        if let Err(e) = accepting {
            log::error!("failed to start accepting connections: {}", e);
        }
    }
    connections
}

/// Connects both pipes of a connection. Other threads of this process wait meanwhile, since the
/// server tells connections apart by process.
pub fn connect_pair(path: &Path) -> io::Result<(Stream, Stream)> {
    let lock = connecting(path);
    let _one_at_a_time = lock.lock().unwrap_or_else(|e| e.into_inner());
    let request = crate::consts::ipc_connect(path)?;
    let reply = crate::consts::ipc_connect_reply(path)?;
    Ok((request, reply))
}

fn connecting(path: &Path) -> Arc<Mutex<()>> {
    static LOCKS: Mutex<Option<HashMap<PathBuf, Arc<Mutex<()>>>>> = Mutex::new(None);
    LOCKS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get_or_insert_with(HashMap::new)
        .entry(path.to_path_buf())
        .or_default()
        .clone()
}

fn client_process(stream: &Stream) -> Option<u32> {
    match stream {
        Stream::NamedPipe(pipe) => pipe.inner().client_process_id().ok(),
    }
}

/// Whether the client still holds its end of the pipe.
fn connected(stream: &Stream) -> bool {
    use std::os::windows::io::{AsHandle, AsRawHandle};
    use windows_sys::Win32::System::Pipes::PeekNamedPipe;
    let handle = match stream {
        Stream::NamedPipe(pipe) => pipe.inner().as_handle().as_raw_handle(),
    };
    let peeked = unsafe {
        PeekNamedPipe(
            handle,
            std::ptr::null_mut(),
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    peeked != 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, PartialEq)]
    struct Pipe {
        name: &'static str,
        gone: bool,
    }

    fn pipe(name: &'static str) -> Pipe {
        Pipe { name, gone: false }
    }

    fn names(connection: Option<(Pipe, Pipe)>) -> Option<(&'static str, &'static str)> {
        connection.map(|(request, reply)| (request.name, reply.name))
    }

    fn is_gone(pipe: &Pipe) -> bool {
        pipe.gone
    }

    #[test]
    fn the_halves_of_a_connection_meet_in_either_order() {
        let now = Instant::now();
        let mut pairing = Pairing::default();
        assert_eq!(
            names(pairing.add(Half::Request, 7, pipe("a"), now, is_gone)),
            None
        );
        assert_eq!(
            names(pairing.add(Half::Reply, 7, pipe("a-reply"), now, is_gone)),
            Some(("a", "a-reply"))
        );
        assert_eq!(
            names(pairing.add(Half::Reply, 8, pipe("b-reply"), now, is_gone)),
            None
        );
        assert_eq!(
            names(pairing.add(Half::Request, 8, pipe("b"), now, is_gone)),
            Some(("b", "b-reply"))
        );
        assert_eq!(pairing.waiting(), (0, 0));
    }

    #[test]
    fn interleaved_clients_each_get_their_own_reply_pipe() {
        let now = Instant::now();
        let mut pairing = Pairing::default();
        pairing.add(Half::Request, 1, pipe("a"), now, is_gone);
        pairing.add(Half::Request, 2, pipe("b"), now, is_gone);
        assert_eq!(
            names(pairing.add(Half::Reply, 2, pipe("b-reply"), now, is_gone)),
            Some(("b", "b-reply"))
        );
        assert_eq!(
            names(pairing.add(Half::Reply, 1, pipe("a-reply"), now, is_gone)),
            Some(("a", "a-reply"))
        );
    }

    #[test]
    fn a_client_that_left_after_its_request_pipe_takes_no_one_elses_reply() {
        let now = Instant::now();
        let mut pairing = Pairing::default();
        pairing.add(
            Half::Request,
            5,
            Pipe {
                name: "orphan",
                gone: true,
            },
            now,
            is_gone,
        );
        pairing.add(Half::Request, 6, pipe("next"), now, is_gone);
        assert_eq!(
            names(pairing.add(Half::Reply, 6, pipe("next-reply"), now, is_gone)),
            Some(("next", "next-reply")),
            "#5740: pairing by arrival order gave the orphan this reply pipe"
        );
        assert_eq!(pairing.waiting(), (0, 0), "the orphan was dropped");
    }

    #[test]
    fn a_process_that_left_one_request_pipe_behind_still_pairs_its_next_one() {
        let now = Instant::now();
        let mut pairing = Pairing::default();
        pairing.add(
            Half::Request,
            9,
            Pipe {
                name: "abandoned",
                gone: true,
            },
            now,
            is_gone,
        );
        pairing.add(Half::Request, 9, pipe("retry"), now, is_gone);
        assert_eq!(
            names(pairing.add(Half::Reply, 9, pipe("retry-reply"), now, is_gone)),
            Some(("retry", "retry-reply"))
        );
    }

    #[test]
    fn real_pipes_pair_up_after_a_client_left_with_only_its_request_pipe() {
        use std::io::{Read, Write};
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("session");
        let requests = crate::consts::ipc_bind(&path).unwrap();
        let replies = crate::consts::ipc_bind_reply(&path).unwrap();
        let connections = accept_pairs(requests, replies);

        let orphan = crate::consts::ipc_connect(&path).unwrap();
        std::thread::sleep(Duration::from_millis(300));
        drop(orphan);

        let (mut request, mut reply) = connect_pair(&path).unwrap();
        request.write_all(b"ping").unwrap();
        let (mut server_request, mut server_reply) = connections
            .recv_timeout(Duration::from_secs(5))
            .expect("the client's two pipes to be paired");
        let mut asked = [0u8; 4];
        server_request.read_exact(&mut asked).unwrap();
        assert_eq!(&asked, b"ping");
        server_reply.write_all(b"pong").unwrap();
        let mut answered = [0u8; 4];
        reply.read_exact(&mut answered).unwrap();
        assert_eq!(
            &answered, b"pong",
            "the answer reaches the client that asked (#5740)"
        );
    }

    #[test]
    fn a_half_left_waiting_too_long_is_dropped() {
        let then = Instant::now();
        let mut pairing = Pairing::default();
        pairing.add(Half::Request, 3, pipe("stuck"), then, is_gone);
        let later = then + MAX_WAIT + Duration::from_secs(1);
        assert_eq!(
            names(pairing.add(Half::Reply, 3, pipe("late-reply"), later, is_gone)),
            None
        );
        assert_eq!(pairing.waiting(), (0, 1));
    }
}
