use super::*;
use crate::os_input_output::{AsyncReader, ServerOsApi, TerminalRunChangedBeforeWrite};
use crate::panes::PaneId;
use crate::thread_bus::Bus;
use interprocess::local_socket::Stream as LocalSocketStream;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use zellij_utils::channels::SenderWithContext;
use zellij_utils::data::Palette;

use zellij_utils::input::command::RunCommand;
use zellij_utils::ipc::{ClientToServerMsg, IpcReceiverWithContext, ServerToClientMsg};

use crate::ClientId;
use std::time::{Duration, Instant};

/// Per-terminal write behavior for the mock.
#[derive(Clone)]
enum WriteBehavior {
    /// Accept all bytes (like a healthy terminal).
    AcceptAll,
    /// Accept at most `n` bytes per write call (simulates short writes).
    AcceptAtMost(usize),
    /// Accept zero bytes (simulates EAGAIN / full buffer).
    AcceptNone,
    /// Return an error (simulates EBADF, EIO, etc).
    Error,
}

#[derive(Clone)]
struct MockServerOsApi {
    /// Controls how each terminal behaves on write.
    write_behavior: Arc<Mutex<HashMap<u32, WriteBehavior>>>,
    /// Log of (terminal_id, bytes) for each successful write.
    write_log: Arc<Mutex<Vec<(u32, Vec<u8>)>>>,
    /// Number of write_to_tty_stdin calls, whatever their outcome.
    write_attempts: Arc<std::sync::atomic::AtomicUsize>,
    /// Installed run for a terminal. Absent means the test does not
    /// check generations. `None` means an endpoint is installed and not
    /// yet bound to a run.
    endpoint_generation: Arc<Mutex<HashMap<u32, Option<u64>>>>,
}

impl MockServerOsApi {
    fn new() -> Self {
        MockServerOsApi {
            write_behavior: Arc::new(Mutex::new(HashMap::new())),
            write_log: Arc::new(Mutex::new(Vec::new())),
            write_attempts: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            endpoint_generation: Arc::new(Mutex::new(HashMap::new())),
        }
    }
    fn set_endpoint_generation(&self, terminal_id: u32, generation: Option<u64>) {
        self.endpoint_generation
            .lock()
            .unwrap()
            .insert(terminal_id, generation);
    }
    fn set_behavior(&self, terminal_id: u32, behavior: WriteBehavior) {
        self.write_behavior
            .lock()
            .unwrap()
            .insert(terminal_id, behavior);
    }
    fn get_written_bytes(&self, terminal_id: u32) -> Vec<u8> {
        self.write_log
            .lock()
            .unwrap()
            .iter()
            .filter(|(tid, _)| *tid == terminal_id)
            .flat_map(|(_, bytes)| bytes.clone())
            .collect()
    }
}

impl ServerOsApi for MockServerOsApi {
    fn set_terminal_size_using_terminal_id(
        &self,
        _id: u32,
        _cols: u16,
        _rows: u16,
        _width_in_pixels: Option<u16>,
        _height_in_pixels: Option<u16>,
    ) -> Result<()> {
        Ok(())
    }
    fn spawn_terminal(
        &self,
        _terminal_action: zellij_utils::input::command::TerminalAction,
        _quit_cb: Box<dyn Fn(PaneId, Option<i32>, RunCommand) + Send>,
        _default_editor: Option<PathBuf>,
        _pane_env: &crate::os_input_output::PaneEnv,
    ) -> Result<(u32, Box<dyn AsyncReader>, Option<u32>)> {
        unimplemented!()
    }
    fn write_to_tty_stdin(&self, terminal_id: u32, buf: &[u8]) -> Result<usize> {
        self.write_attempts
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let behavior = self
            .write_behavior
            .lock()
            .unwrap()
            .get(&terminal_id)
            .cloned()
            .unwrap_or(WriteBehavior::AcceptAll);
        match behavior {
            WriteBehavior::AcceptAll => {
                self.write_log
                    .lock()
                    .unwrap()
                    .push((terminal_id, buf.to_vec()));
                Ok(buf.len())
            },
            WriteBehavior::AcceptAtMost(max) => {
                let n = buf.len().min(max);
                if n > 0 {
                    self.write_log
                        .lock()
                        .unwrap()
                        .push((terminal_id, buf[..n].to_vec()));
                }
                Ok(n)
            },
            WriteBehavior::AcceptNone => Ok(0),
            WriteBehavior::Error => Err(anyhow::anyhow!(
                "simulated write error for terminal {}",
                terminal_id
            )),
        }
    }
    fn write_to_tty_stdin_for_run(
        &self,
        terminal_id: u32,
        generation: Option<u64>,
        buf: &[u8],
    ) -> Result<usize> {
        if let Some(expected) = generation {
            let endpoints = self.endpoint_generation.lock().unwrap();
            if let Some(actual) = endpoints.get(&terminal_id) {
                if *actual != Some(expected) {
                    return Err(anyhow::Error::new(TerminalRunChangedBeforeWrite));
                }
            }
        }
        self.write_to_tty_stdin(terminal_id, buf)
    }
    fn tcdrain(&self, _id: u32) -> Result<()> {
        Ok(())
    }
    fn kill(&self, _pid: u32) -> Result<()> {
        Ok(())
    }
    fn force_kill(&self, _pid: u32) -> Result<()> {
        Ok(())
    }
    fn send_sigint(&self, _pid: u32) -> Result<()> {
        Ok(())
    }
    fn box_clone(&self) -> Box<dyn ServerOsApi> {
        Box::new((*self).clone())
    }
    fn send_to_client(&self, _client_id: ClientId, _msg: ServerToClientMsg) -> Result<()> {
        Ok(())
    }
    fn register_client(
        &mut self,
        _client_id: ClientId,
        _receiver: &IpcReceiverWithContext<ClientToServerMsg>,
    ) -> Result<()> {
        unimplemented!()
    }
    fn register_client_with_reply(
        &mut self,
        _client_id: ClientId,
        _reply_stream: LocalSocketStream,
    ) -> Result<()> {
        unimplemented!()
    }
    fn remove_client(&mut self, _client_id: ClientId) -> Result<()> {
        Ok(())
    }
    fn load_palette(&self) -> Palette {
        Palette::default()
    }
    fn get_cwd(&self, _pid: u32) -> Option<PathBuf> {
        None
    }
    fn write_to_file(&mut self, _buf: String, _file: Option<String>) -> Result<()> {
        Ok(())
    }
    fn re_run_command_in_terminal(
        &self,
        _terminal_id: u32,
        _run_command: RunCommand,
        _quit_cb: Box<dyn Fn(PaneId, Option<i32>, RunCommand) + Send>,
        _pane_env: &crate::os_input_output::PaneEnv,
    ) -> Result<(Box<dyn AsyncReader>, Option<u32>)> {
        unimplemented!()
    }
    fn clear_terminal_id(&self, _terminal_id: u32) -> Result<()> {
        Ok(())
    }
}

/// Helper: create a Bus and sender for PtyWriteInstruction with the given mock.
fn make_test_bus(
    mock: MockServerOsApi,
) -> (
    Bus<PtyWriteInstruction>,
    SenderWithContext<PtyWriteInstruction>,
) {
    let (sender, receiver) = channels::unbounded();
    let sender_with_ctx = SenderWithContext::new(sender);
    let bus = Bus::new(
        vec![receiver],
        None,
        None,
        None,
        None,
        None,
        None,
        Some(Box::new(mock)),
    );
    (bus, sender_with_ctx)
}

#[test]
fn pending_queue_tracks_remaining_bytes_incrementally() {
    let mut queue = PendingQueue::default();
    queue.push_back(PendingWrite {
        bytes: b"hello".to_vec(),
        offset: 0,
        completion: None,
        run_generation: None,
    });
    queue.push_back(PendingWrite {
        bytes: b" world".to_vec(),
        offset: 2,
        completion: None,
        run_generation: None,
    });

    assert_eq!(queue.remaining_bytes, 9);
    queue.advance(2);
    assert_eq!(queue.remaining_bytes, 7);
    queue.advance(3);
    assert_eq!(queue.remaining_bytes, 4);
    queue.advance(4);

    assert!(queue.is_empty());
    assert_eq!(queue.remaining_bytes, 0);
}

/// Helper: send instructions and then Exit, run pty_writer_main, return when done.
fn run_pty_writer(
    bus: Bus<PtyWriteInstruction>,
    sender: SenderWithContext<PtyWriteInstruction>,
    instructions: Vec<PtyWriteInstruction>,
) {
    // Send all instructions followed by Exit
    for instruction in instructions {
        sender.send(instruction).unwrap();
    }
    sender.send(PtyWriteInstruction::Exit).unwrap();

    pty_writer_main(bus).unwrap();
}

#[test]
fn full_write_completes_immediately() {
    let mock = MockServerOsApi::new();
    mock.set_behavior(1, WriteBehavior::AcceptAll);
    let mock_clone = mock.clone();

    let (bus, sender) = make_test_bus(mock);
    run_pty_writer(
        bus,
        sender,
        vec![PtyWriteInstruction::Write(b"hello world".to_vec(), 1, None)],
    );

    let written = mock_clone.get_written_bytes(1);
    assert_eq!(written, b"hello world");
}

#[test]
fn partial_write_is_buffered_and_drained() {
    let mock = MockServerOsApi::new();
    // Accept only 3 bytes at a time
    mock.set_behavior(1, WriteBehavior::AcceptAtMost(3));
    let mock_clone = mock.clone();

    let (bus, sender) = make_test_bus(mock);
    run_pty_writer(
        bus,
        sender,
        vec![PtyWriteInstruction::Write(b"abcdefghij".to_vec(), 1, None)],
    );

    let written = mock_clone.get_written_bytes(1);
    assert_eq!(
        written, b"abcdefghij",
        "all bytes should eventually be written"
    );
}

#[test]
fn eagain_on_one_terminal_does_not_block_another() {
    let mock = MockServerOsApi::new();
    // Terminal 1 is stuck (EAGAIN)
    mock.set_behavior(1, WriteBehavior::AcceptNone);
    // Terminal 2 accepts everything
    mock.set_behavior(2, WriteBehavior::AcceptAll);
    let mock_clone = mock.clone();

    let (bus, sender) = make_test_bus(mock);
    run_pty_writer(
        bus,
        sender,
        vec![
            PtyWriteInstruction::Write(b"stuck data".to_vec(), 1, None),
            PtyWriteInstruction::Write(b"good data".to_vec(), 2, None),
        ],
    );

    // Terminal 2 should have received its bytes despite terminal 1 being stuck
    let written_2 = mock_clone.get_written_bytes(2);
    assert_eq!(written_2, b"good data");

    // Terminal 1 data is lost (stuck until Exit, then dropped)
    // — this is expected behavior; no assertion on terminal 1
}

#[test]
fn writes_to_same_terminal_preserve_order() {
    let mock = MockServerOsApi::new();
    // Accept only 4 bytes at a time — forces buffering
    mock.set_behavior(1, WriteBehavior::AcceptAtMost(4));
    let mock_clone = mock.clone();

    let (bus, sender) = make_test_bus(mock);
    run_pty_writer(
        bus,
        sender,
        vec![
            PtyWriteInstruction::Write(b"AAAA".to_vec(), 1, None),
            PtyWriteInstruction::Write(b"BBBB".to_vec(), 1, None),
            PtyWriteInstruction::Write(b"CCCC".to_vec(), 1, None),
        ],
    );

    let written = mock_clone.get_written_bytes(1);
    assert_eq!(
        written, b"AAAABBBBCCCC",
        "writes must be delivered in order"
    );
}

#[test]
fn memory_cap_drops_buffer_when_exceeded() {
    let mock = MockServerOsApi::new();
    // Terminal is stuck — nothing drains
    mock.set_behavior(1, WriteBehavior::AcceptNone);
    let mock_clone = mock.clone();

    let (bus, sender) = make_test_bus(mock);

    // Send enough data to exceed MAX_PENDING_BYTES
    let chunk_size = 1024 * 1024; // 1 MB
    let mut instructions = Vec::new();
    for _ in 0..(MAX_PENDING_BYTES / chunk_size + 1) {
        instructions.push(PtyWriteInstruction::Write(vec![0x42; chunk_size], 1, None));
    }
    // After the cap is exceeded, send a write to terminal 2 to verify
    // the writer thread is still functional
    instructions.push(PtyWriteInstruction::Write(b"still alive".to_vec(), 2, None));
    mock_clone.set_behavior(2, WriteBehavior::AcceptAll);

    run_pty_writer(bus, sender, instructions);

    let written_2 = mock_clone.get_written_bytes(2);
    assert_eq!(
        written_2, b"still alive",
        "writer thread should continue functioning after dropping a buffer"
    );
}

/// A terminal that accepts everything the kernel offers must not monopolize
/// the writer: with a bounded per-terminal turn, a sparse terminal's queued
/// write lands while the burst terminal still has pending bytes.
#[test]
fn fast_terminal_burst_yields_to_sparse_terminal() {
    let mock = MockServerOsApi::new();
    mock.set_behavior(1, WriteBehavior::AcceptAll);
    mock.set_behavior(2, WriteBehavior::AcceptAll);
    let mock_clone = mock.clone();

    let (bus, sender) = make_test_bus(mock);

    let burst_size = 8 * 1024 * 1024; // under MAX_PENDING_BYTES
    sender
        .send(PtyWriteInstruction::Write(vec![0xAA; burst_size], 1, None))
        .unwrap();
    sender
        .send(PtyWriteInstruction::Write(b"sparse".to_vec(), 2, None))
        .unwrap();

    let writer = std::thread::spawn(move || pty_writer_main(bus));

    // Wait until terminal 2's write lands and terminal 1 fully drains.
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        {
            let log = mock_clone.write_log.lock().unwrap();
            let t2_seen = log.iter().any(|(tid, _)| *tid == 2);
            let t1_bytes: usize = log
                .iter()
                .filter(|(tid, _)| *tid == 1)
                .map(|(_, b)| b.len())
                .sum();
            if t2_seen && t1_bytes == burst_size {
                break;
            }
            assert!(Instant::now() < deadline, "writes did not complete in time");
        }
        std::thread::sleep(Duration::from_millis(5));
    }

    let log = mock_clone.write_log.lock().unwrap();
    let t2_pos = log.iter().position(|(tid, _)| *tid == 2).unwrap();
    let last_t1_pos = log.iter().rposition(|(tid, _)| *tid == 1).unwrap();
    assert!(
        t2_pos < last_t1_pos,
        "sparse terminal's write must land before the burst finishes draining"
    );
    drop(log);

    // Byte-perfect, ordered delivery to each terminal.
    assert_eq!(mock_clone.get_written_bytes(2), b"sparse");
    assert_eq!(mock_clone.get_written_bytes(1).len(), burst_size);
    assert!(mock_clone.get_written_bytes(1).iter().all(|b| *b == 0xAA));

    sender.send(PtyWriteInstruction::Exit).unwrap();
    writer.join().unwrap().unwrap();
}

/// When every pending terminal refuses bytes, the writer must keep polling
/// instead of spinning, and later writes must still drain once the consumer
/// accepts again.
#[test]
fn stalled_terminals_resume_draining_without_new_instructions() {
    let mock = MockServerOsApi::new();
    mock.set_behavior(1, WriteBehavior::AcceptNone);
    let mock_clone = mock.clone();

    let (bus, sender) = make_test_bus(mock);

    sender
        .send(PtyWriteInstruction::Write(b"blocked".to_vec(), 1, None))
        .unwrap();

    let writer = std::thread::spawn(move || pty_writer_main(bus));
    std::thread::sleep(Duration::from_millis(300));
    assert!(mock_clone.get_written_bytes(1).is_empty());

    // While every terminal refuses bytes the writer retries on the
    // PENDING_DRAIN_TIMEOUT cadence: roughly 30 attempts in 300 ms. A
    // zero-timeout spin makes orders of magnitude more; a writer that stops
    // retrying (or waits far longer) makes almost none.
    let attempts = mock_clone
        .write_attempts
        .load(std::sync::atomic::Ordering::SeqCst);
    assert!(
        (5..=120).contains(&attempts),
        "stalled writer made {attempts} attempts in 300 ms; expected polling near 10 ms"
    );

    // The terminal starts accepting; the pending write must drain without
    // any new instruction arriving.
    mock_clone.set_behavior(1, WriteBehavior::AcceptAll);

    let deadline = Instant::now() + Duration::from_secs(10);
    while mock_clone.get_written_bytes(1) != b"blocked" {
        assert!(Instant::now() < deadline, "stalled write never drained");
        std::thread::sleep(Duration::from_millis(5));
    }

    sender.send(PtyWriteInstruction::Exit).unwrap();
    writer.join().unwrap().unwrap();
}

/// While the last pass made progress the writer must poll without waiting;
/// sleeping the stall timeout between productive passes would throttle a
/// slow-but-moving consumer to a few passes per second. A consumer taking one
/// byte per call needs 400 passes for this payload, which would take over
/// four seconds with a 10 ms wait per pass.
#[test]
fn progressing_terminal_drains_without_waiting_between_passes() {
    let mock = MockServerOsApi::new();
    mock.set_behavior(1, WriteBehavior::AcceptAtMost(1));
    let mock_clone = mock.clone();

    let (bus, sender) = make_test_bus(mock);
    let payload = vec![0x5A; MAX_DRAIN_CALLS_PER_TURN * 400];
    sender
        .send(PtyWriteInstruction::Write(payload.clone(), 1, None))
        .unwrap();

    let started = Instant::now();
    let writer = std::thread::spawn(move || pty_writer_main(bus));
    while mock_clone.get_written_bytes(1).len() < payload.len() {
        assert!(
            started.elapsed() < Duration::from_secs(20),
            "payload never drained"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "productive passes must not wait out the stall timeout, took {:?}",
        started.elapsed()
    );

    sender.send(PtyWriteInstruction::Exit).unwrap();
    writer.join().unwrap().unwrap();
}

/// Retiring an empty write costs a unit of the turn's call budget, so a
/// terminal with a long run of empty writes queued cannot hold the writer
/// past its turn.
#[test]
fn empty_writes_spend_the_turn_budget() {
    let mock = MockServerOsApi::new();
    let mock_clone = mock.clone();
    let mut queue = PendingQueue::default();
    let empties = MAX_DRAIN_CALLS_PER_TURN * 3;
    for _ in 0..empties {
        queue.push_back(PendingWrite {
            bytes: Vec::new(),
            offset: 0,
            completion: None,
            run_generation: None,
        });
    }
    queue.push_back(PendingWrite {
        bytes: b"tail".to_vec(),
        offset: 0,
        completion: None,
        run_generation: None,
    });

    assert!(drain_terminal_turn(&mut queue, 1, None, &mock));
    assert_eq!(
        queue.writes.len(),
        empties + 1 - MAX_DRAIN_CALLS_PER_TURN,
        "one turn may retire at most the call budget's worth of empty writes"
    );
    assert!(mock_clone.get_written_bytes(1).is_empty());

    // Later turns finish the run and then deliver the real write.
    while !queue.is_empty() {
        drain_terminal_turn(&mut queue, 1, None, &mock);
    }
    assert_eq!(mock_clone.get_written_bytes(1), b"tail");
}

#[test]
fn write_error_clears_terminal_queue() {
    let mock = MockServerOsApi::new();
    mock.set_behavior(1, WriteBehavior::Error);
    mock.set_behavior(2, WriteBehavior::AcceptAll);
    let mock_clone = mock.clone();

    let (bus, sender) = make_test_bus(mock);
    run_pty_writer(
        bus,
        sender,
        vec![
            PtyWriteInstruction::Write(b"will fail".to_vec(), 1, None),
            PtyWriteInstruction::Write(b"also queued".to_vec(), 1, None),
            PtyWriteInstruction::Write(b"should work".to_vec(), 2, None),
        ],
    );

    // Terminal 1 should have nothing written (error cleared queue)
    let written_1 = mock_clone.get_written_bytes(1);
    assert!(
        written_1.is_empty(),
        "errored terminal should have no written bytes"
    );

    // Terminal 2 should be unaffected
    let written_2 = mock_clone.get_written_bytes(2);
    assert_eq!(written_2, b"should work");
}

#[test]
fn a_queued_write_does_not_reach_a_replacement_before_adoption() {
    // Writer-local adoption is still generation 1. The native endpoint
    // has already been replaced with generation 2. Both a write that was
    // already queued and one admitted after the replacement are refused,
    // but stay queued until adoption settles them.
    let os = MockServerOsApi::new();
    os.set_endpoint_generation(1, Some(2));
    let writer_adopted = Some(1u64);
    for bytes in [b"already-queued".as_slice(), b"arrived-late".as_slice()] {
        let mut queue = super::PendingQueue::default();
        queue.push_back(super::PendingWrite {
            bytes: bytes.to_vec(),
            offset: 0,
            completion: None,
            run_generation: writer_adopted,
        });
        super::drain_terminal_turn(&mut queue, 1, writer_adopted, &os);
        assert_eq!(queue.writes.len(), 1, "{bytes:?} did not stay queued");
        let mut pending = HashMap::from([(1, queue)]);
        let mut adopted = HashMap::from([(1, 1u64)]);
        super::adopt_run(&mut adopted, &mut pending, 1, 2);
        assert!(pending[&1].is_empty(), "{bytes:?} survived adoption");
    }
    assert!(
        os.get_written_bytes(1).is_empty(),
        "neither write may reach generation 2's endpoint"
    );

    os.set_endpoint_generation(1, Some(2));
    let mut live = super::PendingQueue::default();
    live.push_back(super::PendingWrite {
        bytes: b"for-the-new-run".to_vec(),
        offset: 0,
        completion: None,
        run_generation: Some(2),
    });
    assert!(super::drain_terminal_turn(&mut live, 1, Some(2), &os));
    assert_eq!(os.get_written_bytes(1), b"for-the-new-run");
}
