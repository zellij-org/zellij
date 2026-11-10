//! On Windows a connection is two named pipes: the request pipe, and a reply pipe for the answers
//! (a blocking pipe handle cannot read and write at the same time). The server accepts each on
//! its own listener, so it has to put the two halves of a connection back together.
//!
//! Pairing them by arrival order breaks as soon as one client connects its request pipe and
//! leaves before connecting its reply pipe: every later client then gets the previous one's
//! answers. Halves are paired by the process that connected them, and a process only connects
//! one pair to the same pipe at a time.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

#[cfg(windows)]
use std::collections::HashMap;
#[cfg(windows)]
use std::io;
#[cfg(windows)]
use std::path::{Path, PathBuf};
#[cfg(windows)]
use std::sync::{mpsc, Arc, Mutex};
#[cfg(windows)]
use std::thread::JoinHandle;

/// How long half a connection waits for its other half before it is dropped.
const MAX_WAIT: Duration = Duration::from_secs(30);

/// How often the independent expiry task drops stale halves. This bounds how
/// long a half can be retained past [`MAX_WAIT`] when no new half arrives.
#[cfg(windows)]
const EXPIRY_INTERVAL: Duration = Duration::from_secs(1);

#[cfg(windows)]
const ACCEPT_ERROR_BACKOFF: Duration = Duration::from_millis(100);

/// Default bound for callers that do not have a narrower admission limit.
///
/// This matches the server's pre-handshake connection ceiling: enough for a
/// burst of attaches and status probes, while keeping unmatched halves and the
/// completed-pair backlog finite.
#[cfg(windows)]
pub const DEFAULT_PAIR_CAPACITY: usize = 64;

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

/// The half of the pair was refused because the waiting queue is full.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CapacityExceeded;

/// The halves of connections that are still waiting for their other half.
///
/// Matching scans the waiting side for the process. The caller bounds both
/// sides, so a burst cannot make that scan grow without limit. Expired or
/// disconnected halves are dropped before they can be paired.
pub struct Pairing<T> {
    requests: VecDeque<Waiting<T>>,
    replies: VecDeque<Waiting<T>>,
    capacity: usize,
}

impl<T> Pairing<T> {
    /// Bounds each waiting side.
    pub fn new(capacity: usize) -> Self {
        Pairing {
            requests: VecDeque::new(),
            replies: VecDeque::new(),
            capacity,
        }
    }

    /// Adds one half of a connection from `process`. Returns the whole connection, request half
    /// first, once its other half is there. Halves whose client is `gone`, or that waited too
    /// long, are dropped before they can take another connection's half.
    ///
    /// A half is refused with [`CapacityExceeded`] when its side already holds
    /// the configured number of halves. The stream is dropped in that case.
    pub fn add(
        &mut self,
        half: Half,
        process: u32,
        stream: T,
        now: Instant,
        gone: impl Fn(&T) -> bool,
    ) -> Result<Option<(T, T)>, CapacityExceeded> {
        self.forget(now, &gone);
        let (mine, others) = match half {
            Half::Request => (&mut self.requests, &mut self.replies),
            Half::Reply => (&mut self.replies, &mut self.requests),
        };
        match others.iter().position(|other| other.process == process) {
            Some(index) => {
                let other = others.remove(index).expect("matched half").stream;
                Ok(Some(match half {
                    Half::Request => (stream, other),
                    Half::Reply => (other, stream),
                }))
            },
            None => {
                if mine.len() >= self.capacity {
                    return Err(CapacityExceeded);
                }
                mine.push_back(Waiting {
                    process,
                    stream,
                    since: now,
                });
                Ok(None)
            },
        }
    }

    /// Drops every half that has waited too long or whose client is gone.
    /// The expiry task calls this without another half arriving.
    fn forget(&mut self, now: Instant, gone: impl Fn(&T) -> bool) {
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

/// The pair-accepting threads plus the independent expiry task.
#[cfg(windows)]
pub struct PairedConnections {
    connections: mpsc::Receiver<(
        interprocess::local_socket::Stream,
        interprocess::local_socket::Stream,
    )>,
    stop_expiry: mpsc::Sender<()>,
    expiry: Option<JoinHandle<()>>,
}

#[cfg(windows)]
impl Iterator for PairedConnections {
    type Item = (
        interprocess::local_socket::Stream,
        interprocess::local_socket::Stream,
    );

    fn next(&mut self) -> Option<Self::Item> {
        self.connections.recv().ok()
    }
}

#[cfg(windows)]
impl Drop for PairedConnections {
    fn drop(&mut self) {
        let _ = self.stop_expiry.send(());
        if let Some(expiry) = self.expiry.take() {
            let _ = expiry.join();
        }
    }
}

/// Accepts the request and reply halves of connections on their two listeners and hands out each
/// connection once both of its halves are there, request half first. `capacity` bounds both the
/// unmatched halves per side and the completed connections waiting for the server to take them.
#[cfg(windows)]
pub fn accept_pairs(
    requests: interprocess::local_socket::Listener,
    replies: interprocess::local_socket::Listener,
    capacity: usize,
) -> PairedConnections {
    let (paired, connections) = mpsc::sync_channel(capacity);
    let pairing = Arc::new(Mutex::new(Pairing::new(capacity)));
    let (stop_expiry, stop) = mpsc::channel();
    let expiry = spawn_expiry(Arc::clone(&pairing), stop);
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
                    let connection = match pairing.lock().unwrap_or_else(|e| e.into_inner()).add(
                        half,
                        process,
                        stream,
                        Instant::now(),
                        |stream| !connected(stream),
                    ) {
                        Ok(connection) => connection,
                        Err(CapacityExceeded) => {
                            log::warn!(
                                "refusing a {:?} pipe half: too many unmatched halves",
                                half
                            );
                            continue;
                        },
                    };
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
    PairedConnections {
        connections,
        stop_expiry,
        expiry,
    }
}

/// Drops stale halves even while no new half arrives. Without this task a
/// one-sided connection would retain its handle until the next client
/// connected.
#[cfg(windows)]
fn spawn_expiry(
    pairing: Arc<Mutex<Pairing<interprocess::local_socket::Stream>>>,
    stop: mpsc::Receiver<()>,
) -> Option<JoinHandle<()>> {
    let thread = std::thread::Builder::new()
        .name("pipe_expiry".to_owned())
        .spawn(move || loop {
            match stop.recv_timeout(EXPIRY_INTERVAL) {
                Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    pairing
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .forget(Instant::now(), |stream| !connected(stream));
                },
            }
        });
    match thread {
        Ok(thread) => Some(thread),
        Err(error) => {
            log::error!("failed to start the pipe expiry task: {}", error);
            None
        },
    }
}

/// Connects both pipes of a connection. Other threads of this process wait meanwhile, since the
/// server tells connections apart by process.
#[cfg(windows)]
pub fn connect_pair(
    path: &Path,
) -> io::Result<(
    interprocess::local_socket::Stream,
    interprocess::local_socket::Stream,
)> {
    let lock = connecting(path);
    let _one_at_a_time = lock.lock().unwrap_or_else(|e| e.into_inner());
    let request = crate::consts::ipc_connect(path)?;
    let reply = crate::consts::ipc_connect_reply(path)?;
    Ok((request, reply))
}

/// Like [`connect_pair`], but the lock wait and both connects share one deadline.
/// A failed reply connect drops the request pipe.
#[cfg(windows)]
pub fn connect_pair_with_timeout(
    path: &Path,
    timeout: Duration,
) -> io::Result<(
    interprocess::local_socket::Stream,
    interprocess::local_socket::Stream,
)> {
    if timeout.is_zero() {
        return Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "ipc connect timed out",
        ));
    }
    let deadline = Instant::now() + timeout;
    let lock = connecting(path);
    let _one_at_a_time = wait_for_turn(&lock, timeout)?;
    let remaining = deadline.saturating_duration_since(Instant::now());
    if remaining.is_zero() {
        return Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "ipc connect timed out",
        ));
    }
    let request = crate::consts::ipc_connect_with_timeout(path, remaining)?;
    let remaining = deadline.saturating_duration_since(Instant::now());
    if remaining.is_zero() {
        return Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "ipc connect timed out",
        ));
    }
    let reply = crate::consts::ipc_connect_reply_with_timeout(path, remaining)?;
    Ok((request, reply))
}

#[cfg(windows)]
fn wait_for_turn(lock: &Mutex<()>, timeout: Duration) -> io::Result<std::sync::MutexGuard<'_, ()>> {
    let deadline = Instant::now() + timeout;
    loop {
        match lock.try_lock() {
            Ok(guard) => return Ok(guard),
            Err(std::sync::TryLockError::Poisoned(poisoned)) => return Ok(poisoned.into_inner()),
            Err(std::sync::TryLockError::WouldBlock) => {
                let remaining = deadline.saturating_duration_since(Instant::now());
                if remaining.is_zero() {
                    return Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        "ipc connect timed out",
                    ));
                }
                std::thread::sleep(Duration::from_millis(1).min(remaining));
            },
        }
    }
}

#[cfg(windows)]
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

#[cfg(windows)]
fn client_process(stream: &interprocess::local_socket::Stream) -> Option<u32> {
    match stream {
        interprocess::local_socket::Stream::NamedPipe(pipe) => {
            pipe.inner().client_process_id().ok()
        },
    }
}

/// Whether the client still holds its end of the pipe.
#[cfg(windows)]
fn connected(stream: &interprocess::local_socket::Stream) -> bool {
    use std::os::windows::io::{AsHandle, AsRawHandle};
    use windows_sys::Win32::System::Pipes::PeekNamedPipe;
    let handle = match stream {
        interprocess::local_socket::Stream::NamedPipe(pipe) => {
            pipe.inner().as_handle().as_raw_handle()
        },
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

    fn pairing() -> Pairing<Pipe> {
        Pairing::new(16)
    }

    #[test]
    fn the_halves_of_a_connection_meet_in_either_order() {
        let now = Instant::now();
        let mut pairing = pairing();
        assert_eq!(
            names(
                pairing
                    .add(Half::Request, 7, pipe("a"), now, is_gone)
                    .unwrap()
            ),
            None
        );
        assert_eq!(
            names(
                pairing
                    .add(Half::Reply, 7, pipe("a-reply"), now, is_gone)
                    .unwrap()
            ),
            Some(("a", "a-reply"))
        );
        assert_eq!(
            names(
                pairing
                    .add(Half::Reply, 8, pipe("b-reply"), now, is_gone)
                    .unwrap()
            ),
            None
        );
        assert_eq!(
            names(
                pairing
                    .add(Half::Request, 8, pipe("b"), now, is_gone)
                    .unwrap()
            ),
            Some(("b", "b-reply"))
        );
        assert_eq!(pairing.waiting(), (0, 0));
    }

    #[test]
    fn interleaved_clients_each_get_their_own_reply_pipe() {
        let now = Instant::now();
        let mut pairing = pairing();
        pairing
            .add(Half::Request, 1, pipe("a"), now, is_gone)
            .unwrap();
        pairing
            .add(Half::Request, 2, pipe("b"), now, is_gone)
            .unwrap();
        assert_eq!(
            names(
                pairing
                    .add(Half::Reply, 2, pipe("b-reply"), now, is_gone)
                    .unwrap()
            ),
            Some(("b", "b-reply"))
        );
        assert_eq!(
            names(
                pairing
                    .add(Half::Reply, 1, pipe("a-reply"), now, is_gone)
                    .unwrap()
            ),
            Some(("a", "a-reply"))
        );
    }

    #[test]
    fn a_client_that_left_after_its_request_pipe_takes_no_one_elses_reply() {
        let now = Instant::now();
        let mut pairing = pairing();
        pairing
            .add(
                Half::Request,
                5,
                Pipe {
                    name: "orphan",
                    gone: true,
                },
                now,
                is_gone,
            )
            .unwrap();
        pairing
            .add(Half::Request, 6, pipe("next"), now, is_gone)
            .unwrap();
        assert_eq!(
            names(
                pairing
                    .add(Half::Reply, 6, pipe("next-reply"), now, is_gone)
                    .unwrap()
            ),
            Some(("next", "next-reply")),
            "pairing by arrival order gave the orphan this reply pipe"
        );
        assert_eq!(
            pairing.waiting(),
            (0, 0),
            "the gone orphan is dropped before another half can match"
        );
    }

    #[test]
    fn a_process_that_left_one_request_pipe_behind_still_pairs_its_next_one() {
        let now = Instant::now();
        let mut pairing = pairing();
        pairing
            .add(
                Half::Request,
                9,
                Pipe {
                    name: "abandoned",
                    gone: true,
                },
                now,
                is_gone,
            )
            .unwrap();
        pairing
            .add(Half::Request, 9, pipe("retry"), now, is_gone)
            .unwrap();
        assert_eq!(
            names(
                pairing
                    .add(Half::Reply, 9, pipe("retry-reply"), now, is_gone)
                    .unwrap()
            ),
            Some(("retry", "retry-reply"))
        );
    }

    #[test]
    fn a_half_left_waiting_too_long_is_dropped() {
        let then = Instant::now();
        let mut pairing = pairing();
        pairing
            .add(Half::Request, 3, pipe("stuck"), then, is_gone)
            .unwrap();
        let later = then + MAX_WAIT + Duration::from_secs(1);
        assert_eq!(
            names(
                pairing
                    .add(Half::Reply, 3, pipe("late-reply"), later, is_gone)
                    .unwrap()
            ),
            None
        );
        assert_eq!(pairing.waiting(), (0, 1));
    }

    #[test]
    fn expiry_can_run_without_another_half_arriving() {
        let then = Instant::now();
        let mut pairing = pairing();
        pairing
            .add(Half::Request, 3, pipe("stuck"), then, is_gone)
            .unwrap();
        pairing
            .add(Half::Reply, 4, pipe("also-stuck"), then, is_gone)
            .unwrap();

        let later = then + MAX_WAIT + Duration::from_secs(1);
        pairing.forget(later, is_gone);

        assert_eq!(
            pairing.waiting(),
            (0, 0),
            "the expiry pass did not release both halves"
        );
    }

    #[test]
    fn unmatched_halves_are_capped_per_side() {
        let now = Instant::now();
        let capacity = 4;
        let mut pairing = Pairing::new(capacity);
        for process in 0..capacity as u32 {
            assert_eq!(
                pairing
                    .add(Half::Request, process + 1, pipe("waiting"), now, is_gone)
                    .unwrap(),
                None
            );
        }
        assert_eq!(
            pairing.add(
                Half::Request,
                capacity as u32 + 1,
                pipe("overflow"),
                now,
                is_gone,
            ),
            Err(CapacityExceeded)
        );
        assert_eq!(pairing.waiting(), (capacity, 0));

        assert_eq!(
            pairing
                .add(Half::Reply, 1, pipe("reply"), now, is_gone)
                .unwrap(),
            Some((pipe("waiting"), pipe("reply"))),
            "the reply side has its own capacity and can still pair"
        );
        assert_eq!(pairing.waiting(), (capacity - 1, 0));
    }

    #[test]
    fn pairing_a_half_frees_capacity_for_the_next_connection() {
        let now = Instant::now();
        let capacity = 1;
        let mut pairing = Pairing::new(capacity);
        pairing
            .add(Half::Request, 1, pipe("one"), now, is_gone)
            .unwrap();
        assert_eq!(
            pairing.add(Half::Request, 2, pipe("refused"), now, is_gone),
            Err(CapacityExceeded)
        );

        assert_eq!(
            names(
                pairing
                    .add(Half::Reply, 1, pipe("one-reply"), now, is_gone)
                    .unwrap()
            ),
            Some(("one", "one-reply"))
        );
        assert_eq!(
            pairing
                .add(Half::Request, 2, pipe("two"), now, is_gone)
                .unwrap(),
            None,
            "a completed pair did not release capacity for the next request"
        );
    }
}
