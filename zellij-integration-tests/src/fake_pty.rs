use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use crate::deadline::{ProgressDeadline, FAKE_PTY_PROGRESS};
use async_trait::async_trait;
use zellij_server::os_input_output::AsyncReader;
use zellij_server::panes::PaneId;
use zellij_utils::input::command::{RunCommand, TerminalAction};

pub(crate) type QuitCb = Box<dyn Fn(PaneId, Option<i32>, RunCommand) + Send>;

pub(crate) const FAKE_PID_BASE: u32 = 100_000;

pub(crate) struct FakePtyState {
    pub terminal_action: Option<TerminalAction>,
    pub output_tx: Option<tokio::sync::mpsc::UnboundedSender<Vec<u8>>>,
    pub stdin: Vec<u8>,
    pub size: Option<(u16, u16)>,
    pub size_history: Vec<(u16, u16)>,
    pub quit_cb: Option<QuitCb>,
    pub exited: bool,
    pub echo: bool,
    pub stdin_tap: Option<crossbeam::channel::Sender<Vec<u8>>>,
}

impl FakePtyState {
    fn run_command(&self) -> RunCommand {
        match &self.terminal_action {
            Some(TerminalAction::RunCommand(run_command)) => run_command.clone(),
            _ => RunCommand::default(),
        }
    }

    fn describe(&self) -> String {
        format!(
            "size={:?} exited={} stdin_bytes={}",
            self.size,
            self.exited,
            self.stdin.len()
        )
    }
}

#[derive(Default)]
pub(crate) struct FakePtyRegistry {
    next_terminal_id: u32,
    generation: u64,
    pub fake_pty_states: HashMap<u32, FakePtyState>,
    pub spawn_queue: VecDeque<u32>,
}

impl FakePtyRegistry {
    fn describe_all(&self) -> String {
        let mut fake_pty_state_lines: Vec<String> = self
            .fake_pty_states
            .iter()
            .map(|(terminal_id, fake_pty_state)| {
                format!("terminal {}: {}", terminal_id, fake_pty_state.describe())
            })
            .collect();
        fake_pty_state_lines.sort();
        format!(
            "{}\nspawn queue: {:?}",
            fake_pty_state_lines.join("\n"),
            self.spawn_queue
        )
    }
}

#[derive(Default)]
struct RegistryWithChangeSignal {
    fake_pty_registry: Mutex<FakePtyRegistry>,
    change_signal: Condvar,
}

#[derive(Clone, Default)]
pub struct SharedPtys {
    inner: Arc<RegistryWithChangeSignal>,
}

impl SharedPtys {
    fn lock_registry(&self) -> MutexGuard<'_, FakePtyRegistry> {
        self.inner.fake_pty_registry.lock().unwrap()
    }

    pub(crate) fn read<T>(&self, reader: impl FnOnce(&FakePtyRegistry) -> T) -> T {
        reader(&self.lock_registry())
    }

    pub(crate) fn mutate<T>(&self, mutator: impl FnOnce(&mut FakePtyRegistry) -> T) -> T {
        let result = {
            let mut fake_pty_registry = self.lock_registry();
            let result = mutator(&mut fake_pty_registry);
            fake_pty_registry.generation += 1;
            result
        };
        self.inner.change_signal.notify_all();
        result
    }

    pub(crate) fn next_terminal_id(&self) -> u32 {
        self.mutate(|fake_pty_registry| {
            let terminal_id = fake_pty_registry.next_terminal_id;
            fake_pty_registry.next_terminal_id += 1;
            terminal_id
        })
    }

    pub(crate) fn register(
        &self,
        terminal_id: u32,
        terminal_action: Option<TerminalAction>,
        quit_cb: Option<QuitCb>,
    ) -> Box<dyn AsyncReader> {
        let (output_tx, output_rx) = tokio::sync::mpsc::unbounded_channel();
        self.mutate(|fake_pty_registry| {
            fake_pty_registry.fake_pty_states.insert(
                terminal_id,
                FakePtyState {
                    terminal_action,
                    output_tx: Some(output_tx),
                    stdin: Vec::new(),
                    size: None,
                    size_history: Vec::new(),
                    quit_cb,
                    exited: false,
                    echo: true,
                    stdin_tap: None,
                },
            );
            fake_pty_registry.spawn_queue.push_back(terminal_id);
        });
        Box::new(FakeAsyncReader {
            output_rx,
            pending: VecDeque::new(),
        })
    }

    pub(crate) fn rerun(&self, terminal_id: u32, quit_cb: QuitCb) -> Box<dyn AsyncReader> {
        let (output_tx, output_rx) = tokio::sync::mpsc::unbounded_channel();
        self.mutate(|fake_pty_registry| {
            let fake_pty_state = fake_pty_registry
                .fake_pty_states
                .get_mut(&terminal_id)
                .expect("re-run for unknown terminal id");
            fake_pty_state.output_tx = Some(output_tx);
            fake_pty_state.quit_cb = Some(quit_cb);
            fake_pty_state.exited = false;
        });
        Box::new(FakeAsyncReader {
            output_rx,
            pending: VecDeque::new(),
        })
    }

    pub(crate) fn exit_terminal(&self, terminal_id: u32, exit_status: Option<i32>) {
        let newly_exited = self.mutate(|fake_pty_registry| {
            let fake_pty_state = fake_pty_registry.fake_pty_states.get_mut(&terminal_id)?;
            if fake_pty_state.exited {
                return None;
            }
            fake_pty_state.exited = true;
            fake_pty_state.output_tx.take();
            Some((fake_pty_state.quit_cb.take(), fake_pty_state.run_command()))
        });
        if let Some((Some(quit_cb), run_command)) = newly_exited {
            quit_cb(PaneId::Terminal(terminal_id), exit_status, run_command);
        }
    }

    pub(crate) fn remove(&self, terminal_id: u32) {
        self.mutate(|fake_pty_registry| {
            fake_pty_registry.fake_pty_states.remove(&terminal_id);
        });
    }

    pub(crate) fn set_size(&self, terminal_id: u32, cols: u16, rows: u16) {
        self.mutate(|fake_pty_registry| {
            if let Some(fake_pty_state) = fake_pty_registry.fake_pty_states.get_mut(&terminal_id) {
                // like a real pty, setting the same size again is a no-op that will not make the
                // program inside the terminal react (the kernel only sends a SIGWINCH if the
                // winsize actually changed)
                if fake_pty_state.size != Some((cols, rows)) {
                    fake_pty_state.size_history.push((cols, rows));
                }
                fake_pty_state.size = Some((cols, rows));
            }
        });
    }

    pub(crate) fn set_stdin_tap(
        &self,
        terminal_id: u32,
        sender: crossbeam::channel::Sender<Vec<u8>>,
    ) {
        self.mutate(|fake_pty_registry| {
            if let Some(fake_pty_state) = fake_pty_registry.fake_pty_states.get_mut(&terminal_id) {
                fake_pty_state.stdin_tap = Some(sender);
            }
        });
    }

    pub(crate) fn write_output(&self, terminal_id: u32, bytes: &[u8]) {
        self.read(|fake_pty_registry| {
            if let Some(fake_pty_state) = fake_pty_registry.fake_pty_states.get(&terminal_id) {
                if let Some(output_tx) = fake_pty_state.output_tx.as_ref() {
                    let _ = output_tx.send(bytes.to_vec());
                }
            }
        });
    }

    pub(crate) fn append_stdin(&self, terminal_id: u32, bytes: &[u8]) -> bool {
        self.mutate(|fake_pty_registry| {
            match fake_pty_registry.fake_pty_states.get_mut(&terminal_id) {
                Some(fake_pty_state) => {
                    fake_pty_state.stdin.extend_from_slice(bytes);
                    if fake_pty_state.echo {
                        if let Some(output_tx) = fake_pty_state.output_tx.as_ref() {
                            let _ = output_tx.send(bytes.to_vec());
                        }
                    }
                    if let Some(stdin_tap) = fake_pty_state.stdin_tap.as_ref() {
                        let _ = stdin_tap.send(bytes.to_vec());
                    }
                    true
                },
                None => false,
            }
        })
    }

    pub(crate) fn wait_for<T>(
        &self,
        what: &str,
        condition: impl FnMut(&mut FakePtyRegistry) -> Option<T>,
    ) -> T {
        self.wait_for_within(
            ProgressDeadline::starting_now(FAKE_PTY_PROGRESS),
            what,
            condition,
        )
    }

    fn wait_for_within<T>(
        &self,
        mut deadline: ProgressDeadline,
        what: &str,
        mut condition: impl FnMut(&mut FakePtyRegistry) -> Option<T>,
    ) -> T {
        let mut fake_pty_registry = self.lock_registry();
        loop {
            if let Some(result) = condition(&mut fake_pty_registry) {
                return result;
            }
            let now = Instant::now();
            if let Some(tripped) = deadline.tripped(now) {
                panic!(
                    "timed out waiting for: {}\n{}\n{}\n=== zellij log tail ({}) ===\n{}",
                    what,
                    tripped,
                    fake_pty_registry.describe_all(),
                    crate::test_env::log_file_path().display(),
                    crate::test_env::log_tail(40),
                );
            }
            let generation = fake_pty_registry.generation;
            let (guard, _) = self
                .inner
                .change_signal
                .wait_timeout(fake_pty_registry, deadline.remaining(now))
                .unwrap();
            fake_pty_registry = guard;
            if fake_pty_registry.generation != generation {
                deadline.note_progress(Instant::now());
            }
        }
    }

    pub(crate) fn wait_for_change(
        &self,
        terminal_id: u32,
        last_seen: (u16, u16),
    ) -> Option<(u16, u16)> {
        let mut fake_pty_registry = self.lock_registry();
        loop {
            match fake_pty_registry.fake_pty_states.get(&terminal_id) {
                None => return None,
                Some(fake_pty_state) => {
                    if let Some((cols, rows)) = fake_pty_state.size {
                        if (cols, rows) != last_seen {
                            return Some((cols, rows));
                        }
                    }
                },
            }
            let (guard, _) = self
                .inner
                .change_signal
                .wait_timeout(fake_pty_registry, Duration::from_millis(200))
                .unwrap();
            fake_pty_registry = guard;
        }
    }
}

struct FakeAsyncReader {
    output_rx: tokio::sync::mpsc::UnboundedReceiver<Vec<u8>>,
    pending: VecDeque<u8>,
}

#[async_trait]
impl AsyncReader for FakeAsyncReader {
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize, std::io::Error> {
        if self.pending.is_empty() {
            match self.output_rx.recv().await {
                Some(bytes) => self.pending.extend(bytes),
                None => return Ok(0),
            }
        }
        let mut written = 0;
        while written < buf.len() {
            match self.pending.pop_front() {
                Some(byte) => {
                    buf[written] = byte;
                    written += 1;
                },
                None => break,
            }
        }
        Ok(written)
    }
}

#[derive(Clone)]
pub struct FakePtyHandle {
    pub(crate) terminal_id: u32,
    pub(crate) shared_ptys: SharedPtys,
}

impl FakePtyHandle {
    pub fn terminal_id(&self) -> u32 {
        self.terminal_id
    }

    pub fn terminal_action(&self) -> Option<TerminalAction> {
        self.shared_ptys.read(|fake_pty_registry| {
            fake_pty_registry
                .fake_pty_states
                .get(&self.terminal_id)
                .and_then(|fake_pty_state| fake_pty_state.terminal_action.clone())
        })
    }

    pub fn disable_echo(&self) {
        self.shared_ptys.mutate(|fake_pty_registry| {
            if let Some(fake_pty_state) =
                fake_pty_registry.fake_pty_states.get_mut(&self.terminal_id)
            {
                fake_pty_state.echo = false;
            }
        });
    }

    pub fn output(&self, bytes: &[u8]) {
        self.shared_ptys.read(|fake_pty_registry| {
            let fake_pty_state = fake_pty_registry
                .fake_pty_states
                .get(&self.terminal_id)
                .unwrap_or_else(|| panic!("unknown terminal id {}", self.terminal_id));
            let output_tx = fake_pty_state
                .output_tx
                .as_ref()
                .unwrap_or_else(|| panic!("terminal {} already exited", self.terminal_id));
            let _ = output_tx.send(bytes.to_vec());
        })
    }

    pub fn try_output(&self, bytes: &[u8]) {
        self.shared_ptys.write_output(self.terminal_id, bytes);
    }

    pub fn tap_stdin(&self) -> crossbeam::channel::Receiver<Vec<u8>> {
        let (sender, receiver) = crossbeam::channel::unbounded();
        self.shared_ptys.set_stdin_tap(self.terminal_id, sender);
        receiver
    }

    pub fn exit(&self, exit_status: Option<i32>) {
        self.shared_ptys
            .exit_terminal(self.terminal_id, exit_status);
    }

    pub fn is_exited(&self) -> bool {
        self.shared_ptys.read(|fake_pty_registry| {
            fake_pty_registry
                .fake_pty_states
                .get(&self.terminal_id)
                .map(|fake_pty_state| fake_pty_state.exited)
                .unwrap_or(true)
        })
    }

    pub fn stdin_bytes(&self) -> Vec<u8> {
        self.shared_ptys.read(|fake_pty_registry| {
            fake_pty_registry
                .fake_pty_states
                .get(&self.terminal_id)
                .map(|fake_pty_state| fake_pty_state.stdin.clone())
                .unwrap_or_default()
        })
    }

    pub fn wait_for_stdin(&self, what: &str, predicate: impl Fn(&[u8]) -> bool) -> Vec<u8> {
        let terminal_id = self.terminal_id;
        self.shared_ptys.wait_for(what, move |fake_pty_registry| {
            fake_pty_registry
                .fake_pty_states
                .get(&terminal_id)
                .and_then(|fake_pty_state| {
                    if predicate(&fake_pty_state.stdin) {
                        Some(fake_pty_state.stdin.clone())
                    } else {
                        None
                    }
                })
        })
    }

    pub fn size(&self) -> Option<(u16, u16)> {
        self.shared_ptys.read(|fake_pty_registry| {
            fake_pty_registry
                .fake_pty_states
                .get(&self.terminal_id)
                .and_then(|fake_pty_state| fake_pty_state.size)
        })
    }

    pub fn size_history(&self) -> Vec<(u16, u16)> {
        self.shared_ptys.read(|fake_pty_registry| {
            fake_pty_registry
                .fake_pty_states
                .get(&self.terminal_id)
                .map(|fake_pty_state| fake_pty_state.size_history.clone())
                .unwrap_or_default()
        })
    }

    pub fn wait_for_size(&self, what: &str, predicate: impl Fn(u16, u16) -> bool) -> (u16, u16) {
        let terminal_id = self.terminal_id;
        self.shared_ptys.wait_for(what, move |fake_pty_registry| {
            fake_pty_registry
                .fake_pty_states
                .get(&terminal_id)
                .and_then(|fake_pty_state| fake_pty_state.size)
                .filter(|(cols, rows)| predicate(*cols, *rows))
        })
    }

    pub fn wait_for_size_change(&self, last_seen: (u16, u16)) -> Option<(u16, u16)> {
        self.shared_ptys
            .wait_for_change(self.terminal_id, last_seen)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc as StdArc;

    const NO_PROGRESS_MS: u64 = 200;
    const HARD_CAP_MS: u64 = 600;

    fn budgeted() -> ProgressDeadline {
        ProgressDeadline::with_budgets(
            FAKE_PTY_PROGRESS,
            Duration::from_millis(NO_PROGRESS_MS),
            Duration::from_millis(HARD_CAP_MS),
        )
    }

    fn never(_fake_pty_registry: &mut FakePtyRegistry) -> Option<()> {
        None
    }

    fn message_of(payload: Box<dyn std::any::Any + Send>) -> String {
        match payload.downcast::<String>() {
            Ok(message) => *message,
            Err(payload) => match payload.downcast::<&'static str>() {
                Ok(message) => (*message).to_owned(),
                Err(_) => String::new(),
            },
        }
    }

    fn waiting_panic(shared_ptys: &SharedPtys) -> (String, Duration) {
        let previous_hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {}));
        let started_at = Instant::now();
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            shared_ptys.wait_for_within(budgeted(), "a condition that never holds", never)
        }));
        let waited = started_at.elapsed();
        std::panic::set_hook(previous_hook);
        match outcome {
            Ok(_) => panic!("the wait returned rather than expiring"),
            Err(payload) => (message_of(payload), waited),
        }
    }

    #[test]
    fn a_silent_registry_trips_the_no_progress_tier() {
        let shared_ptys = SharedPtys::default();
        let (message, waited) = waiting_panic(&shared_ptys);
        assert!(
            message
                .contains("no-progress deadline tripped: nothing changed in the fake pty registry"),
            "{}",
            message
        );
        assert!(
            waited < Duration::from_millis(HARD_CAP_MS),
            "silence waited {:?}, which is the hard cap rather than the no-progress tier",
            waited
        );
    }

    #[test]
    fn a_registry_that_keeps_changing_outlives_the_no_progress_tier_and_stops_at_the_hard_cap() {
        let shared_ptys = SharedPtys::default();
        let stop = StdArc::new(AtomicBool::new(false));
        let mutating = {
            let shared_ptys = shared_ptys.clone();
            let stop = stop.clone();
            std::thread::spawn(move || {
                while !stop.load(Ordering::Relaxed) {
                    shared_ptys.mutate(|fake_pty_registry| {
                        fake_pty_registry.spawn_queue.push_back(0);
                        fake_pty_registry.spawn_queue.pop_front();
                    });
                    std::thread::sleep(Duration::from_millis(20));
                }
            })
        };
        let (message, waited) = waiting_panic(&shared_ptys);
        stop.store(true, Ordering::Relaxed);
        mutating.join().unwrap();
        assert!(
            message.contains("hard cap tripped: the fake pty registry kept changing"),
            "{}",
            message
        );
        assert!(
            waited >= Duration::from_millis(NO_PROGRESS_MS),
            "a registry that kept changing expired after {:?}, inside the no-progress budget",
            waited
        );
    }

    #[test]
    fn a_condition_that_already_holds_is_answered_without_waiting() {
        let shared_ptys = SharedPtys::default();
        let started_at = Instant::now();
        let answer = shared_ptys.wait_for_within(budgeted(), "a condition that holds", |_| Some(7));
        assert_eq!(answer, 7);
        assert!(started_at.elapsed() < Duration::from_millis(NO_PROGRESS_MS));
    }
}
