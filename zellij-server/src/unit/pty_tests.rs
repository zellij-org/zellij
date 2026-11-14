use super::*;
use crate::os_input_output::{ChildPidFd, ForegroundSignal, ServerOsApi};
use crate::plugins::PluginInstruction;
use crate::thread_bus::Bus;
use interprocess::local_socket::Stream as LocalSocketStream;
use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use zellij_utils::channels::{self, SenderWithContext};
use zellij_utils::data::{Event, Palette};
#[cfg(target_os = "linux")]
use zellij_utils::data::{GetPaneCwdResponse, GetPanePidResponse, GetPaneRunningCommandResponse};
use zellij_utils::errors::ErrorContext;
use zellij_utils::input::command::RunCommand;
use zellij_utils::ipc::{ClientToServerMsg, IpcReceiverWithContext, ServerToClientMsg};

enum MockSpawnOutcome {
    Success { terminal_id: u32, child_pid: u32 },
    Error(String),
    CommandNotFound { terminal_id: u32, command: String },
}

#[derive(Clone)]
struct MockOsApi {
    cwds: Arc<Mutex<HashMap<u32, PathBuf>>>,
    cmds: Arc<Mutex<HashMap<u32, Vec<String>>>>,
    cmds_by_ppid: Arc<Mutex<HashMap<String, Vec<String>>>>,
    // (signal, pid) for every kill/force_kill/send_sigint request
    signals: Arc<Mutex<Vec<(&'static str, u32)>>>,
    // pid handed out by spawn_terminal, and the quit callback it was given
    spawn_pid: Arc<Mutex<Option<u32>>>,
    spawn_error: Arc<Mutex<Option<String>>>,
    spawn_plan: Arc<Mutex<VecDeque<MockSpawnOutcome>>>,
    cleared_terminal_ids: Arc<Mutex<Vec<u32>>>,
    quit_cb: Arc<Mutex<Option<Box<dyn Fn(PaneId, Option<i32>, RunCommand) + Send>>>>,
    /// `tcgetpgrp` result. `None` means the master has no foreground group.
    foreground_pgrp: Arc<Mutex<Option<i32>>>,
    /// Group-signal attempts `(signal, terminal_id)`. Not numeric shell kills.
    foreground_group_attempts: Arc<Mutex<Vec<(&'static str, u32)>>>,
    /// Pids handed to cwd, command, or foreground-command lookups.
    pid_lookups: Arc<Mutex<Vec<u32>>>,
    /// Generation `bind_terminal_generation` recorded for each terminal.
    bound_generations: Arc<Mutex<HashMap<u32, u64>>>,
    /// Writes whose generation matched the bound run.
    applied_writes: Arc<Mutex<Vec<(u32, Option<u64>, Vec<u8>)>>>,
    /// Next `re_run_command_in_terminal` result: child pid and optional pidfd.
    rerun_child: Arc<Mutex<Option<(u32, Option<ChildPidFd>)>>>,
}

impl MockOsApi {
    fn new() -> Self {
        MockOsApi {
            cwds: Arc::new(Mutex::new(HashMap::new())),
            cmds: Arc::new(Mutex::new(HashMap::new())),
            cmds_by_ppid: Arc::new(Mutex::new(HashMap::new())),
            signals: Arc::new(Mutex::new(Vec::new())),
            spawn_pid: Arc::new(Mutex::new(None)),
            spawn_error: Arc::new(Mutex::new(None)),
            spawn_plan: Arc::new(Mutex::new(VecDeque::new())),
            cleared_terminal_ids: Arc::new(Mutex::new(Vec::new())),
            quit_cb: Arc::new(Mutex::new(None)),
            foreground_pgrp: Arc::new(Mutex::new(None)),
            foreground_group_attempts: Arc::new(Mutex::new(Vec::new())),
            pid_lookups: Arc::new(Mutex::new(Vec::new())),
            bound_generations: Arc::new(Mutex::new(HashMap::new())),
            applied_writes: Arc::new(Mutex::new(Vec::new())),
            rerun_child: Arc::new(Mutex::new(None)),
        }
    }
    fn bound_generation(&self, terminal_id: u32) -> Option<u64> {
        self.bound_generations
            .lock()
            .unwrap()
            .get(&terminal_id)
            .copied()
    }
    fn applied_writes(&self) -> Vec<(u32, Option<u64>, Vec<u8>)> {
        self.applied_writes.lock().unwrap().clone()
    }
    fn clear_applied_writes(&self) {
        self.applied_writes.lock().unwrap().clear();
    }
    fn plan_rerun(&self, child_pid: u32, pidfd: Option<ChildPidFd>) {
        *self.rerun_child.lock().unwrap() = Some((child_pid, pidfd));
    }
    #[cfg(target_os = "linux")]
    fn pid_lookups(&self) -> Vec<u32> {
        self.pid_lookups.lock().unwrap().clone()
    }
    #[cfg(target_os = "linux")]
    fn clear_pid_lookups(&self) {
        self.pid_lookups.lock().unwrap().clear();
    }
    fn signals(&self) -> Vec<(&'static str, u32)> {
        self.signals.lock().unwrap().clone()
    }
    #[cfg(target_os = "linux")]
    fn foreground_group_attempts(&self) -> Vec<(&'static str, u32)> {
        self.foreground_group_attempts.lock().unwrap().clone()
    }
    fn set_cwd(&self, pid: u32, path: PathBuf) {
        self.cwds.lock().unwrap().insert(pid, path);
    }
    fn set_cmd(&self, pid: u32, cmd: Vec<String>) {
        self.cmds.lock().unwrap().insert(pid, cmd);
    }
    fn set_foreground_cmd(&self, ppid: u32, cmd: Vec<String>) {
        self.cmds_by_ppid
            .lock()
            .unwrap()
            .insert(ppid.to_string(), cmd);
    }
    fn clear_foreground_cmd(&self, ppid: u32) {
        self.cmds_by_ppid.lock().unwrap().remove(&ppid.to_string());
    }
    fn set_spawn_error(&self, message: &str) {
        *self.spawn_error.lock().unwrap() = Some(message.to_string());
    }
    fn plan_spawns(&self, outcomes: Vec<MockSpawnOutcome>) {
        *self.spawn_plan.lock().unwrap() = outcomes.into();
    }
    fn cleared_terminal_ids(&self) -> Vec<u32> {
        self.cleared_terminal_ids.lock().unwrap().clone()
    }
}

impl ServerOsApi for MockOsApi {
    fn set_terminal_size_using_terminal_id(
        &self,
        _: u32,
        _: u16,
        _: u16,
        _: Option<u16>,
        _: Option<u16>,
    ) -> anyhow::Result<()> {
        Ok(())
    }
    fn spawn_terminal(
        &self,
        _: TerminalAction,
        quit_cb: Box<dyn Fn(PaneId, Option<i32>, RunCommand) + Send>,
        _: Option<PathBuf>,
        _pane_env: &crate::os_input_output::PaneEnv,
    ) -> anyhow::Result<(u32, Box<dyn AsyncReader>, Option<u32>, Option<ChildPidFd>)> {
        if let Some(outcome) = self.spawn_plan.lock().unwrap().pop_front() {
            return match outcome {
                MockSpawnOutcome::Success {
                    terminal_id,
                    child_pid,
                } => {
                    *self.quit_cb.lock().unwrap() = Some(quit_cb);
                    Ok((
                        terminal_id,
                        Box::new(crate::os_input_output::NullAsyncReader),
                        Some(child_pid),
                        None,
                    ))
                },
                MockSpawnOutcome::Error(message) => Err(anyhow::anyhow!(message)),
                MockSpawnOutcome::CommandNotFound {
                    terminal_id,
                    command,
                } => Err(zellij_utils::errors::ZellijError::CommandNotFound {
                    terminal_id,
                    command,
                }
                .into()),
            };
        }
        if let Some(message) = self.spawn_error.lock().unwrap().clone() {
            return Err(anyhow::anyhow!(message));
        }
        let child_pid = self
            .spawn_pid
            .lock()
            .unwrap()
            .take()
            .expect("the test must set the pid to spawn with");
        *self.quit_cb.lock().unwrap() = Some(quit_cb);
        Ok((
            7,
            Box::new(crate::os_input_output::NullAsyncReader),
            Some(child_pid),
            None,
        ))
    }
    fn write_to_tty_stdin(&self, _: u32, buf: &[u8]) -> anyhow::Result<usize> {
        Ok(buf.len())
    }
    fn bind_terminal_generation(&self, terminal_id: u32, generation: u64) {
        self.bound_generations
            .lock()
            .unwrap()
            .insert(terminal_id, generation);
    }
    fn write_to_tty_stdin_for_run(
        &self,
        terminal_id: u32,
        generation: Option<u64>,
        buf: &[u8],
    ) -> anyhow::Result<usize> {
        // Same gate as `ServerOsInputOutput::terminal_fd_for_run`: a stamped
        // write applies only while the endpoint is still that generation.
        let bound = self.bound_generation(terminal_id);
        if let Some(expected) = generation {
            if bound != Some(expected) {
                return Err(anyhow::Error::new(
                    crate::os_input_output::TerminalRunChangedBeforeWrite,
                ));
            }
        }
        self.applied_writes
            .lock()
            .unwrap()
            .push((terminal_id, generation, buf.to_vec()));
        Ok(buf.len())
    }
    fn tcdrain(&self, _: u32) -> anyhow::Result<()> {
        Ok(())
    }
    fn kill(&self, pid: u32) -> anyhow::Result<()> {
        self.signals.lock().unwrap().push(("SIGHUP", pid));
        Ok(())
    }
    fn force_kill(&self, pid: u32) -> anyhow::Result<()> {
        self.signals.lock().unwrap().push(("SIGKILL", pid));
        Ok(())
    }
    fn send_sigint(&self, pid: u32) -> anyhow::Result<()> {
        self.signals.lock().unwrap().push(("SIGINT", pid));
        Ok(())
    }
    fn signal_terminal_foreground_group(
        &self,
        terminal_id: u32,
        signal: ForegroundSignal,
    ) -> anyhow::Result<bool> {
        let name = match signal {
            ForegroundSignal::Interrupt => "SIGINT",
            ForegroundSignal::Kill => "SIGKILL",
        };
        self.foreground_group_attempts
            .lock()
            .unwrap()
            .push((name, terminal_id));
        // `None` is tcgetpgrp returning no group: killpg is not attempted,
        // and this is not a numeric signal of the shell pid.
        let pgid = *self.foreground_pgrp.lock().unwrap();
        Ok(pgid.filter(|pgid| *pgid > 0).is_some())
    }
    fn box_clone(&self) -> Box<dyn ServerOsApi> {
        Box::new((*self).clone())
    }
    fn send_to_client(&self, _: ClientId, _: ServerToClientMsg) -> anyhow::Result<()> {
        Ok(())
    }
    fn register_client(
        &mut self,
        _: ClientId,
        _: &IpcReceiverWithContext<ClientToServerMsg>,
    ) -> anyhow::Result<()> {
        unimplemented!()
    }
    fn register_client_with_reply(
        &mut self,
        _: ClientId,
        _: LocalSocketStream,
    ) -> anyhow::Result<()> {
        unimplemented!()
    }
    fn remove_client(&mut self, _: ClientId) -> anyhow::Result<()> {
        Ok(())
    }
    fn load_palette(&self) -> Palette {
        Palette::default()
    }
    fn get_cwd(&self, pid: u32) -> Option<PathBuf> {
        self.pid_lookups.lock().unwrap().push(pid);
        self.cwds.lock().unwrap().get(&pid).cloned()
    }
    fn get_cwds(&self, pids: Vec<u32>) -> (HashMap<u32, PathBuf>, HashMap<u32, Vec<String>>) {
        self.pid_lookups
            .lock()
            .unwrap()
            .extend(pids.iter().copied());
        let cwds_lock = self.cwds.lock().unwrap();
        let cmds_lock = self.cmds.lock().unwrap();
        let cwds = pids
            .iter()
            .filter_map(|pid| cwds_lock.get(pid).map(|cwd| (*pid, cwd.clone())))
            .collect();
        let cmds = pids
            .iter()
            .filter_map(|pid| cmds_lock.get(pid).map(|cmd| (*pid, cmd.clone())))
            .collect();
        (cwds, cmds)
    }
    fn get_all_cmds_by_ppid(&self, _: &Option<String>) -> HashMap<String, Vec<String>> {
        self.cmds_by_ppid.lock().unwrap().clone()
    }
    fn get_foreground_cmds(
        &self,
        panes: &[(u32, u32)],
        _: &Option<String>,
    ) -> HashMap<u32, Vec<String>> {
        self.pid_lookups
            .lock()
            .unwrap()
            .extend(panes.iter().map(|(_, shell_pid)| *shell_pid));
        let cmds_by_ppid = self.cmds_by_ppid.lock().unwrap();
        panes
            .iter()
            .filter_map(|(terminal_id, shell_pid)| {
                cmds_by_ppid
                    .get(&shell_pid.to_string())
                    .map(|cmd| (*terminal_id, cmd.clone()))
            })
            .collect()
    }
    fn write_to_file(&mut self, _: String, _: Option<String>) -> anyhow::Result<()> {
        Ok(())
    }
    fn re_run_command_in_terminal(
        &self,
        _: u32,
        _: RunCommand,
        quit_cb: Box<dyn Fn(PaneId, Option<i32>, RunCommand) + Send>,
        _pane_env: &crate::os_input_output::PaneEnv,
    ) -> anyhow::Result<(Box<dyn AsyncReader>, Option<u32>, Option<ChildPidFd>)> {
        let Some((child_pid, pidfd)) = self.rerun_child.lock().unwrap().take() else {
            unimplemented!("the test must plan the replacement run");
        };
        *self.quit_cb.lock().unwrap() = Some(quit_cb);
        Ok((
            Box::new(crate::os_input_output::NullAsyncReader),
            Some(child_pid),
            pidfd,
        ))
    }
    fn clear_terminal_id(&self, terminal_id: u32) -> anyhow::Result<()> {
        self.cleared_terminal_ids.lock().unwrap().push(terminal_id);
        Ok(())
    }
}

fn make_pty_with_plugin_receiver(
    mock: MockOsApi,
) -> (Pty, channels::Receiver<(PluginInstruction, ErrorContext)>) {
    let (plugin_tx, plugin_rx) = channels::unbounded();
    let plugin_sender = SenderWithContext::new(plugin_tx);
    let mut bus: Bus<PtyInstruction> = Bus::empty().should_silently_fail();
    bus.os_input = Some(Box::new(mock));
    bus.senders.to_plugin = Some(plugin_sender);
    let pty = Pty::new(bus, false, None, None, Default::default());
    (pty, plugin_rx)
}

fn set_active_terminal(pty: &mut Pty, terminal_id: u32, child_pid: u32) {
    // Identity goes through the real registration path. The activity flag is
    // what the reader would publish; these tests do not start one. The probe
    // is set after registration so capture does not cache a cwd first.
    let generation = pty.mint_run_generation();
    pty.record_child_pid(terminal_id, child_pid, generation);
    pty.panes
        .get_mut(&terminal_id)
        .and_then(|pane| pane.run.as_mut())
        .expect("registration stores a run")
        .activity = Some(Arc::new(AtomicBool::new(true)));
    // The fixture means this number is still that task. Production learns
    // the same fact from the spawn-time pidfd; tests inject the probe.
    #[cfg(target_os = "linux")]
    {
        pty.test_shell_pid_probe = Some(Ok(()));
    }
}

fn activity_flag(pty: &Pty, terminal_id: u32) -> Arc<AtomicBool> {
    pty.panes
        .get(&terminal_id)
        .and_then(|pane| pane.run.as_ref())
        .and_then(|run| run.activity.clone())
        .unwrap_or_else(|| panic!("terminal {terminal_id} has no activity flag"))
}

fn stored_pid(pty: &Pty, terminal_id: u32) -> Option<u32> {
    pty.panes
        .get(&terminal_id)
        .and_then(|pane| pane.run.as_ref())
        .and_then(|run| run.child_pid)
}

fn stored_generation(pty: &Pty, terminal_id: u32) -> Option<u64> {
    pty.panes
        .get(&terminal_id)
        .and_then(|pane| pane.run.as_ref())
        .and_then(|run| run.generation)
}

fn generations_empty(pty: &Pty) -> bool {
    !pty.panes
        .values()
        .any(|pane| pane.run.as_ref().and_then(|run| run.generation).is_some())
}

#[cfg(target_os = "linux")]
fn stored_pidfd_raw(pty: &Pty, terminal_id: u32) -> Option<std::os::fd::RawFd> {
    pty.panes.get(&terminal_id).and_then(|pane| {
        pane.run
            .as_ref()
            .and_then(|run| run.pidfd.as_ref().map(std::os::fd::AsRawFd::as_raw_fd))
    })
}

fn collect_cwd_changed_events(
    rx: &channels::Receiver<(PluginInstruction, ErrorContext)>,
) -> Vec<(PaneId, PathBuf)> {
    let mut events = Vec::new();
    while let Ok((instruction, _)) = rx.try_recv() {
        if let PluginInstruction::Update(updates) = instruction {
            for (_, _, event) in updates {
                if let Event::CwdChanged(pane_id, cwd, _) = event {
                    events.push((pane_id.into(), cwd));
                }
            }
        }
    }
    events
}

fn collect_command_changed_events(
    rx: &channels::Receiver<(PluginInstruction, ErrorContext)>,
) -> Vec<(PaneId, Vec<String>, bool)> {
    let mut events = Vec::new();
    while let Ok((instruction, _)) = rx.try_recv() {
        if let PluginInstruction::Update(updates) = instruction {
            for (_, _, event) in updates {
                if let Event::CommandChanged(pane_id, cmd, is_foreground, _) = event {
                    events.push((pane_id.into(), cmd, is_foreground));
                }
            }
        }
    }
    events
}

#[test]
fn foreground_command_emitted_with_is_foreground_true() {
    let mock = MockOsApi::new();
    let child_pid = 100;
    mock.set_foreground_cmd(child_pid, vec!["vim".into(), "file.rs".into()]);
    let (mut pty, rx) = make_pty_with_plugin_receiver(mock);
    set_active_terminal(&mut pty, 1, child_pid);

    pty.update_and_report_cwds();

    let events = collect_command_changed_events(&rx);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].0, PaneId::Terminal(1));
    assert_eq!(events[0].1, vec!["vim", "file.rs"]);
    assert!(events[0].2, "expected is_foreground=true");
}

#[test]
fn empty_foreground_falls_back_to_shell_command() {
    let mock = MockOsApi::new();
    let child_pid = 100;
    mock.set_cmd(child_pid, vec!["/bin/bash".into()]);
    let (mut pty, rx) = make_pty_with_plugin_receiver(mock);
    set_active_terminal(&mut pty, 1, child_pid);

    pty.update_and_report_cwds();

    let events = collect_command_changed_events(&rx);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].0, PaneId::Terminal(1));
    assert_eq!(events[0].1, vec!["/bin/bash"]);
    assert!(!events[0].2, "expected is_foreground=false");
}

#[test]
fn foreground_clearing_emits_shell_fallback() {
    let mock = MockOsApi::new();
    let child_pid = 100;
    mock.set_cmd(child_pid, vec!["/bin/zsh".into()]);
    mock.set_foreground_cmd(child_pid, vec!["cargo".into(), "build".into()]);
    let (mut pty, rx) = make_pty_with_plugin_receiver(mock.clone());
    set_active_terminal(&mut pty, 1, child_pid);

    pty.update_and_report_cwds();
    let events = collect_command_changed_events(&rx);
    assert_eq!(events.len(), 1);
    assert!(events[0].2, "first event should be foreground");
    assert_eq!(events[0].1, vec!["cargo", "build"]);

    mock.clear_foreground_cmd(child_pid);
    activity_flag(&pty, 1).store(true, Ordering::Relaxed);

    pty.update_and_report_cwds();
    let events = collect_command_changed_events(&rx);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].1, vec!["/bin/zsh"]);
    assert!(
        !events[0].2,
        "after clearing foreground, should fall back to shell"
    );
}

#[test]
fn no_event_when_foreground_unchanged() {
    let mock = MockOsApi::new();
    let child_pid = 100;
    mock.set_foreground_cmd(child_pid, vec!["htop".into()]);
    let (mut pty, rx) = make_pty_with_plugin_receiver(mock);
    set_active_terminal(&mut pty, 1, child_pid);

    pty.update_and_report_cwds();
    let _ = collect_command_changed_events(&rx);

    activity_flag(&pty, 1).store(true, Ordering::Relaxed);
    pty.update_and_report_cwds();
    let events = collect_command_changed_events(&rx);
    assert!(
        events.is_empty(),
        "no event expected when command unchanged"
    );
}

#[test]
fn no_event_for_inactive_terminal() {
    let mock = MockOsApi::new();
    let child_pid = 100;
    mock.set_foreground_cmd(child_pid, vec!["vim".into()]);
    let (mut pty, rx) = make_pty_with_plugin_receiver(mock);
    set_active_terminal(&mut pty, 1, child_pid);
    activity_flag(&pty, 1).store(false, Ordering::Relaxed);

    pty.update_and_report_cwds();
    let events = collect_command_changed_events(&rx);
    assert!(
        events.is_empty(),
        "inactive terminal should produce no events"
    );
}

#[test]
fn foreground_change_between_two_commands() {
    let mock = MockOsApi::new();
    let child_pid = 100;
    mock.set_foreground_cmd(child_pid, vec!["vim".into()]);
    let (mut pty, rx) = make_pty_with_plugin_receiver(mock.clone());
    set_active_terminal(&mut pty, 1, child_pid);

    pty.update_and_report_cwds();
    let events = collect_command_changed_events(&rx);
    assert_eq!(events[0].1, vec!["vim"]);
    assert!(events[0].2);

    mock.set_foreground_cmd(child_pid, vec!["cargo".into(), "test".into()]);
    activity_flag(&pty, 1).store(true, Ordering::Relaxed);

    pty.update_and_report_cwds();
    let events = collect_command_changed_events(&rx);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].1, vec!["cargo", "test"]);
    assert!(events[0].2);
}

// --- Activity flag gating ---

#[test]
fn activity_flag_reset_after_poll() {
    let mock = MockOsApi::new();
    let child_pid = 100;
    let (mut pty, _rx) = make_pty_with_plugin_receiver(mock);
    set_active_terminal(&mut pty, 1, child_pid);
    assert!(activity_flag(&pty, 1).load(Ordering::Relaxed));

    pty.update_and_report_cwds();

    assert!(
        !activity_flag(&pty, 1).load(Ordering::Relaxed),
        "activity flag should be reset to false after poll"
    );
}

#[test]
fn multiple_terminals_only_active_ones_polled() {
    let mock = MockOsApi::new();
    let pid_active = 100;
    let pid_inactive = 200;
    mock.set_cwd(pid_active, PathBuf::from("/active"));
    mock.set_cwd(pid_inactive, PathBuf::from("/inactive"));
    let (mut pty, rx) = make_pty_with_plugin_receiver(mock);
    set_active_terminal(&mut pty, 1, pid_active);
    set_active_terminal(&mut pty, 2, pid_inactive);
    activity_flag(&pty, 2).store(false, Ordering::Relaxed);

    pty.update_and_report_cwds();

    let events = collect_cwd_changed_events(&rx);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].0, PaneId::Terminal(1));
    assert_eq!(events[0].1, PathBuf::from("/active"));
}

// --- CWD change events ---

#[test]
fn cwd_changed_event_emitted_on_change() {
    let mock = MockOsApi::new();
    let child_pid = 100;
    mock.set_cwd(child_pid, PathBuf::from("/home/user"));
    let (mut pty, rx) = make_pty_with_plugin_receiver(mock);
    set_active_terminal(&mut pty, 1, child_pid);

    pty.update_and_report_cwds();

    let events = collect_cwd_changed_events(&rx);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].0, PaneId::Terminal(1));
    assert_eq!(events[0].1, PathBuf::from("/home/user"));
}

#[test]
fn no_cwd_event_when_unchanged() {
    let mock = MockOsApi::new();
    let child_pid = 100;
    mock.set_cwd(child_pid, PathBuf::from("/home/user"));
    let (mut pty, rx) = make_pty_with_plugin_receiver(mock);
    set_active_terminal(&mut pty, 1, child_pid);
    pty.panes.get_mut(&1).unwrap().cwd = Some(PathBuf::from("/home/user"));

    pty.update_and_report_cwds();

    let events = collect_cwd_changed_events(&rx);
    assert!(events.is_empty(), "no event expected when cwd unchanged");
}

// --- OSC7 CWD notification ---

#[test]
fn osc7_emits_cwd_changed() {
    let mock = MockOsApi::new();
    let (mut pty, rx) = make_pty_with_plugin_receiver(mock);

    pty.notify_cwd_from_osc7(1, PathBuf::from("/tmp/new"));

    let events = collect_cwd_changed_events(&rx);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].0, PaneId::Terminal(1));
    assert_eq!(events[0].1, PathBuf::from("/tmp/new"));
    assert_eq!(
        pty.panes.get(&1).and_then(|pane| pane.cwd.as_ref()),
        Some(&PathBuf::from("/tmp/new")),
        "cache should be updated"
    );
}

#[test]
fn osc7_no_event_when_unchanged() {
    let mock = MockOsApi::new();
    let (mut pty, rx) = make_pty_with_plugin_receiver(mock);
    pty.panes.entry(1).or_default().cwd = Some(PathBuf::from("/same"));

    pty.notify_cwd_from_osc7(1, PathBuf::from("/same"));

    let events = collect_cwd_changed_events(&rx);
    assert!(events.is_empty(), "no event when osc7 path matches cache");
}

#[test]
fn osc7_clears_activity_flag() {
    let mock = MockOsApi::new();
    let (mut pty, _rx) = make_pty_with_plugin_receiver(mock);
    set_active_terminal(&mut pty, 1, 100);
    let flag = activity_flag(&pty, 1);

    pty.notify_cwd_from_osc7(1, PathBuf::from("/new"));

    assert!(
        !flag.load(Ordering::Relaxed),
        "osc7 should clear the activity flag"
    );
}

#[test]
fn osc7_then_poll_skips_terminal() {
    let mock = MockOsApi::new();
    let child_pid = 100;
    mock.set_cwd(child_pid, PathBuf::from("/from-proc"));
    mock.set_foreground_cmd(child_pid, vec!["vim".into()]);
    let (mut pty, rx) = make_pty_with_plugin_receiver(mock);
    set_active_terminal(&mut pty, 1, child_pid);

    pty.notify_cwd_from_osc7(1, PathBuf::from("/from-osc7"));
    let osc7_events = collect_cwd_changed_events(&rx);
    assert_eq!(osc7_events.len(), 1);

    pty.update_and_report_cwds();
    let cwd_events = collect_cwd_changed_events(&rx);
    let cmd_events = collect_command_changed_events(&rx);
    assert!(
        cwd_events.is_empty() && cmd_events.is_empty(),
        "poll after osc7 should skip terminal since flag was cleared"
    );
}

fn session_with_terminal_pane(terminal_id: u32) -> SessionLayoutMetadata {
    session_with_terminal_pane_run(terminal_id, None)
}

fn session_with_terminal_pane_run(terminal_id: u32, run: Option<Run>) -> SessionLayoutMetadata {
    let mut meta = SessionLayoutMetadata::default();
    let pane = crate::session_layout_metadata::PaneLayoutMetadata::new(
        PaneId::Terminal(terminal_id),
        zellij_utils::pane_size::PaneGeom::default(),
        false,
        run,
        None,
        false,
        None,
        vec![],
        None,
        None,
        Default::default(),
    );
    meta.add_tab("tab1".to_string(), true, false, vec![pane], vec![]);
    meta
}

fn serialized_run_and_cwd(pty: &mut Pty, terminal_id: u32) -> (Option<Run>, Option<PathBuf>) {
    let mut meta = session_with_terminal_pane(terminal_id);
    pty.populate_session_layout_metadata(&mut meta);
    let manifest: session_serialization::GlobalLayoutManifest = meta.into();
    let global_cwd = manifest.global_cwd;
    let pane = manifest
        .tabs
        .into_iter()
        .next()
        .unwrap()
        .1
        .tiled_panes
        .into_iter()
        .next()
        .unwrap();
    let cwd = match (global_cwd, pane.cwd) {
        (Some(global_cwd), Some(cwd)) => Some(global_cwd.join(cwd)),
        (None, cwd) => cwd,
        (global_cwd, None) => global_cwd,
    };
    (pane.run, cwd)
}

fn run_command(command: &str, args: Vec<&str>) -> Option<Run> {
    let mut run_command = RunCommand::new(PathBuf::from(command));
    run_command.args = args.into_iter().map(|a| a.to_string()).collect();
    Some(Run::Command(run_command))
}

fn resurrect_command(command: &str, args: Vec<&str>, cwd: Option<&str>) -> ResurrectCommand {
    ResurrectCommand {
        command: PathBuf::from(command),
        args: args.into_iter().map(|a| a.to_string()).collect(),
        cwd: cwd.map(PathBuf::from),
    }
}

#[test]
fn deliberate_launch_intent_is_not_replaced_by_observed_foreground() {
    // Layout/command panes carry Run::Command in invoked_with. A foreground
    // child (ssh, LSP, …) must not become the serialized restart command.
    let mock = MockOsApi::new();
    let child_pid = 100;
    mock.set_foreground_cmd(
        child_pid,
        vec!["ssh".into(), "user@host".into(), "interactive".into()],
    );
    mock.set_cwd(child_pid, PathBuf::from("/tmp/detected"));
    let (mut pty, _rx) = make_pty_with_plugin_receiver(mock);
    set_active_terminal(&mut pty, 1, child_pid);

    let mut meta =
        session_with_terminal_pane_run(1, run_command("opencode", vec!["-s", "ses_abc"]));
    pty.populate_session_layout_metadata(&mut meta);
    let manifest: session_serialization::GlobalLayoutManifest = meta.into();
    let pane = manifest
        .tabs
        .into_iter()
        .next()
        .unwrap()
        .1
        .tiled_panes
        .into_iter()
        .next()
        .unwrap();
    assert_eq!(
        pane.run,
        run_command("opencode", vec!["-s", "ses_abc"]),
        "observed foreground must not overwrite deliberate launch intent"
    );
}

#[test]
fn shell_pane_without_launch_intent_still_serializes_observed_foreground() {
    let mock = MockOsApi::new();
    let child_pid = 100;
    mock.set_foreground_cmd(child_pid, vec!["htop".into()]);
    let (mut pty, _rx) = make_pty_with_plugin_receiver(mock);
    set_active_terminal(&mut pty, 1, child_pid);

    let (run, _cwd) = serialized_run_and_cwd(&mut pty, 1);
    assert_eq!(run, run_command("htop", vec![]));
}

#[test]
fn resurrect_command_overrides_detected_command_and_cwd() {
    let mock = MockOsApi::new();
    let child_pid = 100;
    mock.set_foreground_cmd(child_pid, vec!["htop".into()]);
    mock.set_cwd(child_pid, PathBuf::from("/tmp/detected"));
    let (mut pty, _rx) = make_pty_with_plugin_receiver(mock);
    set_active_terminal(&mut pty, 1, child_pid);

    pty.set_resurrect_command(
        1,
        Some(resurrect_command(
            "opencode",
            vec!["-s", "ses_abc"],
            Some("/tmp/resurrected"),
        )),
    )
    .unwrap();

    let (run, cwd) = serialized_run_and_cwd(&mut pty, 1);
    assert_eq!(run, run_command("opencode", vec!["-s", "ses_abc"]));
    assert_eq!(cwd, Some(PathBuf::from("/tmp/resurrected")));
}

#[test]
fn resurrect_command_without_cwd_keeps_detected_cwd() {
    let mock = MockOsApi::new();
    let child_pid = 100;
    mock.set_foreground_cmd(child_pid, vec!["htop".into()]);
    mock.set_cwd(child_pid, PathBuf::from("/tmp/detected"));
    let (mut pty, _rx) = make_pty_with_plugin_receiver(mock);
    set_active_terminal(&mut pty, 1, child_pid);

    pty.set_resurrect_command(1, Some(resurrect_command("opencode", vec![], None)))
        .unwrap();

    let (run, cwd) = serialized_run_and_cwd(&mut pty, 1);
    assert_eq!(run, run_command("opencode", vec![]));
    assert_eq!(cwd, Some(PathBuf::from("/tmp/detected")));
}

#[test]
fn cleared_resurrect_command_falls_back_to_detected_command() {
    let mock = MockOsApi::new();
    let child_pid = 100;
    mock.set_foreground_cmd(child_pid, vec!["htop".into()]);
    let (mut pty, _rx) = make_pty_with_plugin_receiver(mock);
    set_active_terminal(&mut pty, 1, child_pid);

    pty.set_resurrect_command(1, Some(resurrect_command("opencode", vec![], None)))
        .unwrap();
    pty.set_resurrect_command(1, None).unwrap();

    let (run, _cwd) = serialized_run_and_cwd(&mut pty, 1);
    assert_eq!(run, run_command("htop", vec![]));
}

#[test]
fn resurrect_command_persists_after_foreground_command_changes() {
    let mock = MockOsApi::new();
    let child_pid = 100;
    mock.set_foreground_cmd(child_pid, vec!["htop".into()]);
    let (mut pty, _rx) = make_pty_with_plugin_receiver(mock.clone());
    set_active_terminal(&mut pty, 1, child_pid);

    pty.set_resurrect_command(1, Some(resurrect_command("opencode", vec![], None)))
        .unwrap();
    mock.set_foreground_cmd(child_pid, vec!["top".into()]);

    let (run, _cwd) = serialized_run_and_cwd(&mut pty, 1);
    assert_eq!(run, run_command("opencode", vec![]));
}

#[test]
fn closing_pane_removes_resurrect_command() {
    let mock = MockOsApi::new();
    let child_pid = 100;
    let (mut pty, _rx) = make_pty_with_plugin_receiver(mock);
    set_active_terminal(&mut pty, 1, child_pid);

    pty.set_resurrect_command(1, Some(resurrect_command("opencode", vec![], None)))
        .unwrap();
    pty.close_pane(PaneId::Terminal(1)).unwrap();

    assert!(pty
        .panes
        .get(&1)
        .and_then(|pane| pane.resurrect.as_ref())
        .is_none());
}

#[test]
fn closing_pane_forgets_plugin_and_prompt_marker_state() {
    let mock = MockOsApi::new();
    let child_pid = 100;
    let (mut pty, _rx) = make_pty_with_plugin_receiver(mock);
    set_active_terminal(&mut pty, 1, child_pid);
    {
        let pane = pty.panes.get_mut(&1).unwrap();
        pane.prompt_markers = true;
        pane.originating_plugin = Some(OriginatingPlugin::new(7, 0, Default::default()));
    }

    pty.close_pane(PaneId::Terminal(1)).unwrap();

    assert!(pty
        .panes
        .get(&1)
        .is_none_or(|pane| { !pane.prompt_markers && pane.originating_plugin.is_none() }));
}

#[test]
fn command_not_found_output_is_stamped_with_the_run_generation() {
    let (screen_tx, screen_rx) = channels::unbounded();
    let senders = ThreadSenders {
        to_screen: Some(SenderWithContext::new(screen_tx)),
        ..Default::default()
    };
    let run_command = RunCommand {
        command: PathBuf::from("zellij-command-that-does-not-exist"),
        ..Default::default()
    };

    send_command_not_found_to_screen(senders, 7, run_command, Some(42)).unwrap();

    let first = screen_rx.try_recv().unwrap().0;
    let second = screen_rx.try_recv().unwrap().0;
    assert!(matches!(first, ScreenInstruction::PtyRunBytes(7, 42, _)));
    assert!(matches!(
        second,
        ScreenInstruction::HoldPane(PaneId::Terminal(7), Some(2), _, Some(42))
    ));
}

#[test]
fn resurrect_command_for_unknown_terminal_is_rejected() {
    let mock = MockOsApi::new();
    let (mut pty, _rx) = make_pty_with_plugin_receiver(mock);

    let result = pty.set_resurrect_command(7, Some(resurrect_command("opencode", vec![], None)));

    assert!(result.is_err());
    assert!(pty.panes.values().all(|pane| pane.resurrect.is_none()));
}

#[test]
fn a_late_exit_from_a_replaced_run_does_not_erase_the_new_process() {
    // A running command pane is re-run: the old process is replaced by a new
    // one before the old run's hold-on-close notification is processed.
    let (mut pty, _plugin_rx) = make_pty_with_plugin_receiver(MockOsApi::new());
    let old_run = pty.mint_run_generation();
    pty.record_child_pid(3, 100, old_run);
    let new_run = pty.mint_run_generation();
    pty.record_child_pid(3, 200, new_run);

    pty.clear_exited_process_identity(3, old_run);
    assert_eq!(
        stored_pid(&pty, 3),
        Some(200),
        "the replacement run's process must stay addressable"
    );

    pty.clear_exited_process_identity(3, new_run);
    assert_eq!(stored_pid(&pty, 3), None);
    assert!(generations_empty(&pty));
}

#[test]
fn a_held_pane_still_accepts_a_resurrect_command() {
    // Forgetting the exited process must not make the held pane itself
    // unknown: its reader, started by the real registration path, lives
    // until the pane is closed.
    let (mut pty, _mock, quit_cb) = spawn_plain_command(500);
    quit_cb(PaneId::Terminal(7), Some(0), RunCommand::default());
    handle_forget_requests(&mut pty);
    assert_eq!(stored_pid(&pty, 7), None);
    assert!(pty
        .panes
        .get(&7)
        .and_then(|pane| pane.run.as_ref())
        .and_then(|run| run.task.as_ref())
        .is_some());
    assert!(pty.set_resurrect_command(7, None).is_ok());
    assert!(pty.set_resurrect_command(6, None).is_err());
}

#[test]
fn forgetting_a_child_drops_its_run_generation() {
    let (mut pty, _plugin_rx) = make_pty_with_plugin_receiver(MockOsApi::new());
    let run = pty.mint_run_generation();
    pty.record_child_pid(4, 400, run);
    assert_eq!(pty.forget_child_pid(4), Some(400));
    assert!(generations_empty(&pty));
    // A notification for the forgotten run is a no-op.
    pty.clear_exited_process_identity(4, run);
    assert_eq!(stored_pid(&pty, 4), None);
}

/// Spawns a plain (not held) command through the mock, which hands out the
/// terminal id 7 and the given child PID.
///
/// The bus is built the way `init_session` builds it: `pty_receiver` is the
/// receiver the pty thread reads, and `Bus::new`'s third sender argument is
/// `Some(&to_pty)`, the sender paired with that receiver. The channel is
/// unbounded. Returns the pty, the mock, and the exit callback the OS layer
/// would run once the child was reaped.
fn spawn_plain_command(
    child_pid: u32,
) -> (
    Pty,
    MockOsApi,
    Box<dyn Fn(PaneId, Option<i32>, RunCommand) + Send>,
) {
    let mock = MockOsApi::new();
    *mock.spawn_pid.lock().unwrap() = Some(child_pid);
    let (to_pty_tx, pty_receiver) = channels::unbounded();
    let to_pty = SenderWithContext::new(to_pty_tx);
    let bus: Bus<PtyInstruction> = Bus::new(
        vec![pty_receiver],
        None,
        Some(&to_pty),
        None,
        None,
        None,
        None,
        Some(Box::new(mock.clone())),
    )
    .should_silently_fail();
    let mut pty = Pty::new(bus, false, None, None, Default::default());
    let run_command = RunCommand {
        command: PathBuf::from("true"),
        args: vec![],
        cwd: None,
        hold_on_close: false,
        hold_on_start: false,
        originating_plugin: None,
        use_terminal_title: false,
        notify: false,
    };
    let run_generation = pty.mint_run_generation();
    let (terminal_id, _starts_held) = pty
        .spawn_terminal(
            Some(TerminalAction::RunCommand(run_command)),
            ClientTabIndexOrPaneId::TabIndex(0),
            None,
            run_generation,
        )
        .unwrap();
    assert_eq!(terminal_id, 7);
    assert_eq!(stored_pid(&pty, 7), Some(child_pid));
    let quit_cb = mock.quit_cb.lock().unwrap().take().unwrap();
    (pty, mock, quit_cb)
}

/// What the PTY thread does with the instructions it was sent.
fn handle_forget_requests(pty: &mut Pty) {
    for instruction in pty.bus.drain_available() {
        if let PtyInstruction::ClearPaneProcessIdentity(PaneId::Terminal(id), run_generation) =
            instruction
        {
            pty.clear_exited_process_identity(id, run_generation);
        }
    }
}

#[test]
fn clear_pane_process_identity_is_received_on_the_session_pty_bus() {
    // The exit callback must reach the receiver `init_session` gives the pty
    // thread. Installing a sender on an empty bus does not prove that.
    let (pty, _mock, quit_cb) = spawn_plain_command(4242);
    let run_generation = stored_generation(&pty, 7).unwrap();
    quit_cb(PaneId::Terminal(7), Some(0), RunCommand::default());

    let received = pty.bus.drain_available();
    assert!(
        received.iter().any(|instruction| {
            matches!(
                instruction,
                PtyInstruction::ClearPaneProcessIdentity(PaneId::Terminal(7), generation)
                    if *generation == run_generation
            )
        }),
        "ClearPaneProcessIdentity must arrive on the session pty receiver, got {received:?}"
    );
}

#[test]
fn close_pane_after_clear_pane_process_identity_does_not_kill() {
    // A command that is not held exits and is reaped; the pane is closed
    // afterwards. The PID may already belong to an unrelated process, so it
    // must not be signalled.
    let (mut pty, mock, quit_cb) = spawn_plain_command(4242);
    quit_cb(PaneId::Terminal(7), Some(0), RunCommand::default());
    handle_forget_requests(&mut pty);
    assert_eq!(stored_pid(&pty, 7), None);

    pty.send_sigint_to_pane(PaneId::Terminal(7));
    pty.send_sigkill_to_pane(PaneId::Terminal(7));
    pty.close_pane(PaneId::Terminal(7)).unwrap();
    assert_eq!(mock.signals(), vec![], "a reaped PID must not be signalled");
}

#[test]
#[cfg(not(target_os = "linux"))]
fn closing_a_pane_whose_child_is_running_still_hangs_it_up() {
    let (mut pty, mock, _quit_cb) = spawn_plain_command(4242);
    handle_forget_requests(&mut pty);
    pty.close_pane(PaneId::Terminal(7)).unwrap();
    assert_eq!(mock.signals(), vec![("SIGHUP", 4242)]);
}

#[test]
#[cfg(target_os = "linux")]
fn close_pane_without_pidfd_does_not_kill() {
    // The mock spawn supplies a pid and no pidfd. Close must not numeric-kill.
    let (mut pty, mock, _quit_cb) = spawn_plain_command(4242);
    handle_forget_requests(&mut pty);
    assert_eq!(stored_pid(&pty, 7), Some(4242));
    assert!(stored_pidfd_raw(&pty, 7).is_none());
    pty.close_pane(PaneId::Terminal(7)).unwrap();
    assert_eq!(mock.signals(), vec![]);
}

#[test]
#[cfg(target_os = "linux")]
fn close_pane_uses_the_supplied_pidfd_and_does_not_numeric_kill() {
    // The fd is supplied by the caller. Close must signal that fd and must
    // not fall through to a numeric kill. Opening `/dev/null` is not `pidfd_open`.
    let (mut pty, mock, _quit_cb) = spawn_plain_command(4242);
    handle_forget_requests(&mut pty);
    let fd: ChildPidFd = std::fs::File::open("/dev/null")
        .expect("open /dev/null")
        .into();
    let raw = std::os::fd::AsRawFd::as_raw_fd(&fd);
    let generation = stored_generation(&pty, 7).unwrap();
    pty.remember_run(7, Some(4242), Some(fd), generation);
    assert_eq!(stored_pidfd_raw(&pty, 7), Some(raw));
    pty.close_pane(PaneId::Terminal(7)).unwrap();
    assert!(stored_pidfd_raw(&pty, 7).is_none());
    assert_eq!(stored_pid(&pty, 7), None);
    assert_eq!(mock.signals(), vec![]);
}

#[test]
#[cfg(target_os = "linux")]
fn sigint_and_sigkill_without_a_pidfd_do_not_numeric_kill() {
    // The mock spawn stores a pid and no pidfd. The foreground group is
    // unset, so tcgetpgrp is None. Neither signal may fall back to kill(pid).
    let (mut pty, mock, _quit_cb) = spawn_plain_command(4242);
    handle_forget_requests(&mut pty);
    assert_eq!(stored_pid(&pty, 7), Some(4242));
    assert!(stored_pidfd_raw(&pty, 7).is_none());
    assert_eq!(*mock.foreground_pgrp.lock().unwrap(), None);

    pty.send_sigint_to_pane(PaneId::Terminal(7));
    pty.send_sigkill_to_pane(PaneId::Terminal(7));

    assert_eq!(
        mock.foreground_group_attempts(),
        vec![("SIGINT", 7), ("SIGKILL", 7)]
    );
    assert_eq!(mock.signals(), vec![]);
    assert_eq!(stored_pid(&pty, 7), Some(4242));
}

#[test]
#[cfg(target_os = "linux")]
fn sigint_and_sigkill_use_the_stored_pidfd_when_tcgetpgrp_is_none() {
    // Opening `/dev/null` is not `pidfd_open`. The mock cannot see the
    // pidfd syscall; the numeric vec must stay empty and the fd must stay
    // in the map because the signal borrows it.
    let (mut pty, mock, _quit_cb) = spawn_plain_command(4242);
    handle_forget_requests(&mut pty);
    let fd: ChildPidFd = std::fs::File::open("/dev/null")
        .expect("open /dev/null")
        .into();
    let raw = std::os::fd::AsRawFd::as_raw_fd(&fd);
    let generation = stored_generation(&pty, 7).unwrap();
    pty.remember_run(7, Some(4242), Some(fd), generation);
    assert_eq!(*mock.foreground_pgrp.lock().unwrap(), None);

    pty.send_sigint_to_pane(PaneId::Terminal(7));
    pty.send_sigkill_to_pane(PaneId::Terminal(7));

    assert_eq!(stored_pidfd_raw(&pty, 7), Some(raw));
    assert_eq!(
        mock.foreground_group_attempts(),
        vec![("SIGINT", 7), ("SIGKILL", 7)]
    );
    assert_eq!(mock.signals(), vec![]);
}

#[cfg(target_os = "linux")]
fn devnull_pidfd() -> ChildPidFd {
    std::fs::File::open("/dev/null")
        .expect("open /dev/null")
        .into()
}

#[cfg(target_os = "linux")]
fn query_cwd_and_command(pty: &Pty) -> (GetPaneCwdResponse, GetPaneRunningCommandResponse) {
    (
        pty.get_pane_cwd(PaneId::Terminal(7)),
        pty.get_pane_running_command(PaneId::Terminal(7)),
    )
}

#[test]
#[cfg(target_os = "linux")]
fn numeric_shell_lookup_requires_a_successful_probe() {
    assert!(
        !numeric_shell_lookup_allowed(None),
        "no stored pidfd is not a live task"
    );
    assert!(!numeric_shell_lookup_allowed(Some(Err(libc::ESRCH))));
    assert!(
        !numeric_shell_lookup_allowed(Some(Err(libc::EBADF))),
        "a non-pidfd's EBADF is not proof the pid is still that process"
    );
    assert!(!numeric_shell_lookup_allowed(Some(Err(libc::ENOTTY))));
    assert!(numeric_shell_lookup_allowed(Some(Ok(()))));
}

#[test]
#[cfg(target_os = "linux")]
fn stored_pid_without_a_live_pidfd_is_not_looked_up() {
    // The mock will answer for this pid. A query that reaches it would
    // report that answer, which is how a reused pid leaks into the pane.
    let (mut pty, mock, _quit_cb) = spawn_plain_command(4242);
    handle_forget_requests(&mut pty);
    mock.set_cwd(4242, PathBuf::from("/tmp/stolen"));
    mock.set_cmd(4242, vec!["stolen".into()]);
    mock.set_foreground_cmd(4242, vec!["stolen-fg".into()]);
    let not_running = (
        GetPaneCwdResponse::Err("Terminal pane 7 not found or not running".to_string()),
        GetPaneRunningCommandResponse::Err("Terminal pane 7 not found or not running".to_string()),
    );

    assert!(stored_pidfd_raw(&pty, 7).is_none());
    mock.clear_pid_lookups();
    assert_eq!(query_cwd_and_command(&pty), not_running);
    assert!(
        mock.pid_lookups().is_empty(),
        "absent pidfd must not look up the stored number"
    );

    let fd = devnull_pidfd();
    let raw = std::os::fd::AsRawFd::as_raw_fd(&fd);
    let generation = stored_generation(&pty, 7).unwrap();
    pty.remember_run(7, Some(4242), Some(fd), generation);
    // No injected probe: signal 0 on `/dev/null` fails with EBADF (or
    // ENOTTY). That is not a live pidfd.
    pty.test_shell_pid_probe = None;
    mock.clear_pid_lookups();
    assert_eq!(query_cwd_and_command(&pty), not_running);
    assert!(
        mock.pid_lookups().is_empty(),
        "a non-pidfd stand-in must not authorize a numeric lookup"
    );

    pty.test_shell_pid_probe = Some(Err(libc::ESRCH));
    mock.clear_pid_lookups();
    assert_eq!(query_cwd_and_command(&pty), not_running);
    assert!(
        mock.pid_lookups().is_empty(),
        "ESRCH means the task is gone; the number must not be looked up"
    );
    assert_eq!(
        stored_pidfd_raw(&pty, 7),
        Some(raw),
        "the probe borrows the pidfd"
    );
}

#[test]
#[cfg(target_os = "linux")]
fn stored_pidfd_with_a_live_probe_allows_cwd_and_command_lookup() {
    // `/dev/null` is not a pidfd. The successful probe is injected so this
    // test does not call `pidfd_open` and does not treat the stand-in as live.
    let (mut pty, mock, _quit_cb) = spawn_plain_command(4242);
    handle_forget_requests(&mut pty);
    mock.set_cwd(4242, PathBuf::from("/tmp/real"));
    mock.set_cmd(4242, vec!["bash".into()]);
    mock.set_foreground_cmd(4242, vec!["vim".into()]);
    let fd = devnull_pidfd();
    let raw = std::os::fd::AsRawFd::as_raw_fd(&fd);
    let generation = stored_generation(&pty, 7).unwrap();
    pty.remember_run(7, Some(4242), Some(fd), generation);
    pty.test_shell_pid_probe = Some(Ok(()));
    mock.clear_pid_lookups();

    assert_eq!(
        pty.get_pane_cwd(PaneId::Terminal(7)),
        GetPaneCwdResponse::Ok(PathBuf::from("/tmp/real"))
    );
    assert_eq!(
        pty.get_pane_running_command(PaneId::Terminal(7)),
        GetPaneRunningCommandResponse::Ok(vec!["vim".into()])
    );
    assert_eq!(mock.pid_lookups(), vec![4242, 4242, 4242]);
    assert_eq!(
        stored_pidfd_raw(&pty, 7),
        Some(raw),
        "the probe borrows the pidfd"
    );
    assert_eq!(mock.signals(), vec![]);
}

#[test]
#[cfg(target_os = "linux")]
fn a_failed_probe_blocks_layout_cwd_and_pid_reports() {
    let (mut pty, mock, _quit_cb) = spawn_plain_command(4242);
    handle_forget_requests(&mut pty);
    mock.set_cwd(4242, PathBuf::from("/tmp/stolen"));
    mock.set_cmd(4242, vec!["stolen".into()]);
    mock.set_foreground_cmd(4242, vec!["stolen-fg".into()]);
    let fd = devnull_pidfd();
    let generation = stored_generation(&pty, 7).unwrap();
    pty.remember_run(7, Some(4242), Some(fd), generation);
    pty.panes
        .get_mut(&7)
        .and_then(|pane| pane.run.as_mut())
        .unwrap()
        .activity = Some(Arc::new(AtomicBool::new(true)));
    pty.test_shell_pid_probe = Some(Err(libc::ESRCH));
    mock.clear_pid_lookups();

    pty.update_and_report_cwds();
    let mut meta = session_with_terminal_pane(7);
    pty.populate_session_layout_metadata(&mut meta);
    assert_eq!(
        pty.get_pane_pid(PaneId::Terminal(7)),
        GetPanePidResponse::Err("Terminal pane 7 not found or not running".to_string())
    );
    assert_eq!(
        query_cwd_and_command(&pty),
        (
            GetPaneCwdResponse::Err("Terminal pane 7 not found or not running".to_string()),
            GetPaneRunningCommandResponse::Err(
                "Terminal pane 7 not found or not running".to_string()
            ),
        )
    );
    assert!(
        mock.pid_lookups().is_empty(),
        "cwd refresh, layout serialization, and pid/cwd/command queries must not read the stored number"
    );
    assert!(pty
        .panes
        .get(&7)
        .and_then(|pane| pane.cwd.as_ref())
        .is_none());
}

#[test]
fn set_active_pane_none_removes_the_client() {
    let (mut pty, _rx) = make_pty_with_plugin_receiver(MockOsApi::new());
    let client = 3;
    let other = 4;
    pty.set_active_pane(Some(PaneId::Terminal(1)), client);
    pty.set_active_pane(Some(PaneId::Terminal(2)), other);

    pty.set_active_pane(None, client);

    assert!(!pty.active_panes.contains_key(&client));
    assert_eq!(pty.active_panes.get(&other), Some(&PaneId::Terminal(2)));
}

#[test]
fn the_plugin_that_started_a_command_hears_of_it_only_after_its_pane_was_queued() {
    // The plugin may close the terminal id it is told about right away. That
    // close can only find the pane if the pane was already queued for the
    // screen when the plugin learned of the id.
    let mock = MockOsApi::new();
    *mock.spawn_pid.lock().unwrap() = Some(4242);
    let (pty_tx, pty_rx) = channels::unbounded();
    let (screen_tx, screen_rx) = channels::unbounded();
    let (plugin_tx, plugin_rx) = channels::unbounded();
    let pty_sender = SenderWithContext::new(pty_tx);
    let screen_sender = SenderWithContext::new(screen_tx);
    let plugin_sender = SenderWithContext::new(plugin_tx);
    let bus: Bus<PtyInstruction> = Bus::new(
        vec![pty_rx],
        Some(&screen_sender),
        Some(&pty_sender),
        Some(&plugin_sender),
        None,
        None,
        None,
        Some(Box::new(mock)),
    )
    .should_silently_fail();
    let pty = Pty::new(bus, false, None, None, Default::default());
    let pty_thread = std::thread::spawn(move || {
        pty_thread_main(pty, Box::new(Layout::default()), "test-session".to_owned()).unwrap()
    });
    let run_command = RunCommand {
        command: PathBuf::from("true"),
        args: vec![],
        cwd: None,
        hold_on_close: false,
        hold_on_start: false,
        originating_plugin: Some(zellij_utils::data::OriginatingPlugin::new(
            3,
            1,
            Default::default(),
        )),
        use_terminal_title: false,
        notify: false,
    };
    pty_sender
        .send(PtyInstruction::SpawnTerminal(
            Some(TerminalAction::RunCommand(run_command)),
            None,
            Default::default(),
            false,
            ClientTabIndexOrPaneId::TabIndex(0),
            None,
            false,
            None,
        ))
        .unwrap();
    loop {
        let (instruction, _) = plugin_rx
            .recv_timeout(std::time::Duration::from_secs(10))
            .expect("the plugin must be told the command started");
        if let PluginInstruction::Update(updates) = instruction {
            if updates
                .iter()
                .any(|(_, _, event)| matches!(event, Event::CommandPaneOpened(..)))
            {
                break;
            }
        }
    }
    let mut pane_was_queued = false;
    while let Ok((instruction, _)) = screen_rx.try_recv() {
        if let crate::screen::ScreenInstruction::NewPane(PaneId::Terminal(7), ..) = instruction {
            pane_was_queued = true;
        }
    }
    let _ = pty_sender.send(PtyInstruction::Exit);
    let _ = pty_thread.join();
    assert!(
        pane_was_queued,
        "the plugin was told of the command before its pane was queued for the screen"
    );
}

#[test]
fn a_generic_spawn_failure_reports_an_error_completion() {
    let mock = MockOsApi::new();
    mock.set_spawn_error("simulated spawn failure");
    let (pty_tx, pty_rx) = channels::unbounded();
    let pty_sender = SenderWithContext::new(pty_tx);
    let bus: Bus<PtyInstruction> = Bus::new(
        vec![pty_rx],
        None,
        None,
        None,
        None,
        None,
        None,
        Some(Box::new(mock)),
    )
    .should_silently_fail();
    let pty = Pty::new(bus, false, None, None, Default::default());
    let pty_thread = std::thread::spawn(move || {
        pty_thread_main(pty, Box::new(Layout::default()), "test-session".to_owned()).unwrap()
    });
    let (completion_tx, completion_rx) = tokio::sync::oneshot::channel();

    pty_sender
        .send(PtyInstruction::SpawnTerminal(
            None,
            None,
            Default::default(),
            false,
            ClientTabIndexOrPaneId::TabIndex(0),
            Some(NotificationEnd::new(completion_tx)),
            false,
            None,
        ))
        .unwrap();

    let result = completion_rx
        .blocking_recv()
        .expect("the failed spawn must complete the action");
    assert!(
        result
            .error_message
            .as_deref()
            .is_some_and(|message| message.contains("simulated spawn failure")),
        "the completion must carry the underlying spawn failure: {:?}",
        result.error_message
    );

    let _ = pty_sender.send(PtyInstruction::Exit);
    pty_thread.join().unwrap();
}

#[test]
fn an_explicit_save_without_an_incarnation_reports_failure() {
    let mock = MockOsApi::new();
    let (pty_tx, pty_rx) = channels::unbounded();
    let pty_sender = SenderWithContext::new(pty_tx);
    let bus: Bus<PtyInstruction> = Bus::new(
        vec![pty_rx],
        None,
        Some(&pty_sender),
        None,
        None,
        None,
        None,
        Some(Box::new(mock)),
    )
    .should_silently_fail();
    let pty = Pty::new(bus, false, None, None, Default::default());
    let pty_thread = std::thread::spawn(move || {
        pty_thread_main(pty, Box::new(Layout::default()), "test-session".to_owned()).unwrap()
    });
    let (completion_tx, completion_rx) = tokio::sync::oneshot::channel();

    pty_sender
        .send(PtyInstruction::SaveSessionToDisk {
            session_name: "test-session".to_owned(),
            session_info: SessionInfo::default(),
            session_layout_metadata: SessionLayoutMetadata::default(),
            completion_tx: Some(NotificationEnd::new(completion_tx)),
        })
        .unwrap();

    let result = completion_rx
        .blocking_recv()
        .expect("the failed explicit save must complete the action");
    assert_eq!(result.exit_status, Some(1));
    assert!(
        result
            .error_message
            .as_deref()
            .is_some_and(|message| message.contains("without a captured incarnation")),
        "the completion must carry the persistence failure: {:?}",
        result.error_message
    );

    let _ = pty_sender.send(PtyInstruction::Exit);
    pty_thread.join().unwrap();
}

#[test]
fn the_persistence_worker_serializes_layout_off_the_pty_actor() {
    let (background_jobs_tx, background_jobs_rx) = channels::unbounded();
    let senders = ThreadSenders {
        to_background_jobs: Some(SenderWithContext::new(background_jobs_tx)),
        ..Default::default()
    };
    let persistence_sender = spawn_persistence_worker(senders);

    persistence_sender
        .try_send_background(PersistenceJob::ReportLayout {
            session_layout_metadata: SessionLayoutMetadata::default(),
            metadata_context: LayoutMetadataContext {
                panes: vec![],
                pids_to_cwds: Default::default(),
                pids_to_cmds: Default::default(),
                foreground_cmds: Default::default(),
                post_command_discovery_hook: None,
                resurrects: Default::default(),
                default_shell: PathBuf::from("/bin/sh"),
                default_editor: None,
            },
        })
        .expect("the worker lane accepts a layout snapshot");

    let (instruction, _) = background_jobs_rx
        .recv_timeout(std::time::Duration::from_secs(5))
        .expect("the worker reports serialized layout to the background jobs thread");
    assert!(matches!(instruction, BackgroundJob::ReportLayoutInfo(_)));
}

#[cfg(unix)]
#[test]
fn a_stalled_command_discovery_hook_does_not_block_pty_control() {
    let mock = MockOsApi::new();
    mock.set_foreground_cmd(100, vec!["sleep".to_owned()]);
    let (pty_tx, pty_rx) = channels::unbounded();
    let (background_jobs_tx, background_jobs_rx) = channels::unbounded();
    let pty_sender = SenderWithContext::new(pty_tx.clone());
    let background_jobs_sender = SenderWithContext::new(background_jobs_tx);
    let bus: Bus<PtyInstruction> = Bus::new(
        vec![pty_rx],
        None,
        Some(&pty_sender),
        None,
        None,
        None,
        Some(&background_jobs_sender),
        Some(Box::new(mock)),
    )
    .should_silently_fail();
    let mut pty = Pty::new(
        bus,
        false,
        None,
        Some("sleep 2; echo $RESURRECT_COMMAND".to_owned()),
        Default::default(),
    );
    pty.session_id = "test-session".to_owned();
    set_active_terminal(&mut pty, 1, 100);
    let session_layout_metadata =
        session_with_terminal_pane_run(1, run_command("sleep", vec!["30"]));
    let metadata_context = pty.layout_metadata_context(&session_layout_metadata);
    assert_eq!(
        metadata_context.foreground_cmds.get(&1),
        Some(&vec!["sleep".to_owned()]),
        "the fixture must have a foreground command for the hook to process"
    );
    assert_eq!(
        metadata_context.post_command_discovery_hook.as_deref(),
        Some("sleep 2; echo $RESURRECT_COMMAND"),
        "the configured hook must be captured in the job context"
    );

    let started_at = std::time::Instant::now();
    let pty_thread = std::thread::spawn(move || {
        pty_thread_main(pty, Box::new(Layout::default()), "test-session".to_owned()).unwrap()
    });
    pty_sender
        .send(PtyInstruction::LogLayoutToHd(session_layout_metadata))
        .unwrap();
    pty_sender.send(PtyInstruction::Exit).unwrap();
    pty_thread.join().unwrap();

    assert!(
        started_at.elapsed() < std::time::Duration::from_secs(1),
        "the pty actor waited for the command-discovery hook"
    );
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        let (instruction, _) = background_jobs_rx
            .recv_timeout(remaining)
            .expect("the queued layout job must complete after the hook");
        if matches!(instruction, BackgroundJob::ReportLayoutInfo(_)) {
            break;
        }
    }
    assert!(
        started_at.elapsed() >= std::time::Duration::from_secs(1),
        "the worker did not run the configured post-command hook"
    );
}

fn run_with_hold(command: &str, hold_on_close: bool) -> Run {
    let mut run_command = RunCommand::new(PathBuf::from(command));
    run_command.hold_on_close = hold_on_close;
    Run::Command(run_command)
}

fn tiled_layout_with_runs(runs: Vec<Run>) -> TiledPaneLayout {
    let mut layout = TiledPaneLayout::default();
    layout.children = runs
        .into_iter()
        .map(|run| TiledPaneLayout {
            run: Some(run),
            ..Default::default()
        })
        .collect();
    layout
}

#[test]
fn a_mid_layout_spawn_failure_rolls_everything_back_without_stopping_the_pty_thread() {
    let mock = MockOsApi::new();
    mock.plan_spawns(vec![
        MockSpawnOutcome::Success {
            terminal_id: 7,
            child_pid: 4242,
        },
        MockSpawnOutcome::Error("simulated layout spawn failure".to_string()),
    ]);
    let (pty_tx, pty_rx) = channels::unbounded();
    let (screen_tx, screen_rx) = channels::unbounded();
    let (writer_tx, writer_rx) = channels::unbounded();
    let pty_sender = SenderWithContext::new(pty_tx);
    let screen_sender: SenderWithContext<ScreenInstruction> = SenderWithContext::new(screen_tx);
    let writer_sender: SenderWithContext<PtyWriteInstruction> = SenderWithContext::new(writer_tx);
    let bus = Bus::new(
        vec![pty_rx],
        Some(&screen_sender),
        None,
        None,
        None,
        Some(&writer_sender),
        None,
        Some(Box::new(mock.clone())),
    )
    .should_silently_fail();
    let pty = Pty::new(bus, false, None, None, Default::default());
    let pty_thread = std::thread::spawn(move || {
        pty_thread_main(pty, Box::new(Layout::default()), "test-session".to_owned()).unwrap()
    });
    let (completion_tx, completion_rx) = tokio::sync::oneshot::channel();

    pty_sender
        .send(PtyInstruction::NewTab(
            None,
            None,
            Some(tiled_layout_with_runs(vec![
                run_with_hold("first-pane", false),
                run_with_hold("second-pane", false),
            ])),
            vec![],
            3,
            HashMap::new(),
            None,
            false,
            false,
            (0, false),
            Some(NotificationEnd::new(completion_tx)),
        ))
        .unwrap();

    let result = completion_rx
        .blocking_recv()
        .expect("the failed layout must complete the action");
    assert!(
        result
            .error_message
            .as_deref()
            .is_some_and(|message| message.contains("simulated layout spawn failure")),
        "the completion must carry the layout spawn failure: {:?}",
        result.error_message
    );
    // The mock spawn does not return a pidfd. On Linux a close then
    // refuses a numeric SIGHUP. The pane is still dropped: the terminal id
    // is cleared and the writer run is forgotten below.
    #[cfg(target_os = "linux")]
    assert!(
        mock.signals().is_empty(),
        "linux must not numeric-kill a layout pane that has no pidfd: {:?}",
        mock.signals()
    );
    #[cfg(not(target_os = "linux"))]
    assert_eq!(
        mock.signals(),
        vec![("SIGHUP", 4242)],
        "without pidfds the stored pid is still signaled"
    );
    assert_eq!(
        mock.cleared_terminal_ids(),
        vec![7],
        "the pane spawned before the failure must release its terminal id"
    );

    let writer_instructions: Vec<PtyWriteInstruction> = std::iter::from_fn(|| {
        writer_rx
            .try_recv()
            .ok()
            .map(|(instruction, _)| instruction)
    })
    .collect();
    assert!(writer_instructions
        .iter()
        .any(|instruction| matches!(instruction, PtyWriteInstruction::AdoptRun(7, _))));
    assert!(writer_instructions
        .iter()
        .any(|instruction| matches!(instruction, PtyWriteInstruction::ForgetRun(7))));

    let screen_instructions: Vec<ScreenInstruction> = std::iter::from_fn(|| {
        screen_rx
            .try_recv()
            .ok()
            .map(|(instruction, _)| instruction)
    })
    .collect();
    assert!(
        !screen_instructions
            .iter()
            .any(|instruction| matches!(instruction, ScreenInstruction::ApplyLayout(..))),
        "a failed layout must not be applied partially"
    );
    assert!(screen_instructions
        .iter()
        .any(|instruction| matches!(instruction, ScreenInstruction::TerminalClosed(7))));

    let _ = pty_sender.send(PtyInstruction::Exit);
    pty_thread.join().unwrap();
}

#[test]
fn command_not_found_layout_panes_are_stamped_and_get_a_close_event() {
    let mock = MockOsApi::new();
    mock.plan_spawns(vec![
        MockSpawnOutcome::CommandNotFound {
            terminal_id: 7,
            command: "missing-held".to_string(),
        },
        MockSpawnOutcome::CommandNotFound {
            terminal_id: 8,
            command: "missing-closed".to_string(),
        },
    ]);
    let (screen_tx, screen_rx) = channels::unbounded();
    let mut bus = Bus::empty().should_silently_fail();
    bus.os_input = Some(Box::new(mock));
    bus.senders.to_screen = Some(SenderWithContext::new(screen_tx));
    let mut pty = Pty::new(bus, false, None, None, Default::default());

    pty.spawn_terminals_for_layout(
        None,
        tiled_layout_with_runs(vec![
            run_with_hold("missing-held", true),
            run_with_hold("missing-closed", false),
        ]),
        vec![],
        None,
        HashMap::new(),
        None,
        0,
        false,
        false,
        (0, false),
        None,
    )
    .unwrap();

    let screen_instructions: Vec<ScreenInstruction> = std::iter::from_fn(|| {
        screen_rx
            .try_recv()
            .ok()
            .map(|(instruction, _)| instruction)
    })
    .collect();
    let applied_terminal_ids: Vec<u32> = screen_instructions
        .iter()
        .find_map(|instruction| match instruction {
            ScreenInstruction::ApplyLayout(_, _, tiled, _, ..) => {
                Some(tiled.iter().map(|(terminal_id, _)| *terminal_id).collect())
            },
            _ => None,
        })
        .expect("command-not-found panes still participate in the layout");
    assert_eq!(applied_terminal_ids, vec![7, 8]);
    assert!(screen_instructions
        .iter()
        .any(|instruction| matches!(instruction, ScreenInstruction::PtyRunBytes(7, 0, _))));
    assert!(screen_instructions.iter().any(|instruction| matches!(
        instruction,
        ScreenInstruction::HoldPane(PaneId::Terminal(7), Some(2), _, Some(0))
    )));
    assert!(screen_instructions.iter().any(|instruction| matches!(
        instruction,
        ScreenInstruction::ClosePane(PaneId::Terminal(8), _, _, _, _, Some(1))
    )));
}

#[test]
fn reader_eof_does_not_clear_process_identity_or_count_as_child_exit() {
    // The reader reaching EOF reports `PtyRunDrained`. The child exit is a
    // separate quit callback. Draining must not forget the pid.
    let mock = MockOsApi::new();
    *mock.spawn_pid.lock().unwrap() = Some(4242);
    let (to_pty_tx, pty_receiver) = channels::unbounded();
    let to_pty = SenderWithContext::new(to_pty_tx);
    let (screen_tx, screen_rx) = channels::unbounded();
    let screen_sender = SenderWithContext::new(screen_tx);
    let bus: Bus<PtyInstruction> = Bus::new(
        vec![pty_receiver],
        Some(&screen_sender),
        Some(&to_pty),
        None,
        None,
        None,
        None,
        Some(Box::new(mock.clone())),
    )
    .should_silently_fail();
    let mut pty = Pty::new(bus, false, None, None, Default::default());
    let generation = pty.mint_run_generation();
    pty.spawn_terminal(
        Some(TerminalAction::RunCommand(RunCommand {
            command: PathBuf::from("true"),
            ..Default::default()
        })),
        ClientTabIndexOrPaneId::TabIndex(0),
        None,
        generation,
    )
    .unwrap();
    let quit_cb = mock.quit_cb.lock().unwrap().take().unwrap();

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let mut saw_drained = false;
    let mut saw_exit = false;
    while std::time::Instant::now() < deadline {
        match screen_rx.recv_timeout(std::time::Duration::from_millis(50)) {
            Ok((ScreenInstruction::PtyRunDrained(7, drained_generation), _)) => {
                assert_eq!(drained_generation, generation);
                saw_drained = true;
                break;
            },
            Ok((ScreenInstruction::ChildRunExited { .. }, _)) => {
                saw_exit = true;
                break;
            },
            Ok(_) => {},
            Err(_) => {},
        }
    }
    assert!(saw_drained, "reader EOF must report PtyRunDrained");
    assert!(!saw_exit, "reader EOF is not a child exit");
    assert_eq!(stored_pid(&pty, 7), Some(4242));
    assert!(
        !pty.bus
            .drain_available()
            .iter()
            .any(|instruction| matches!(instruction, PtyInstruction::ClearPaneProcessIdentity(..))),
        "reader EOF must not forget the child"
    );

    quit_cb(PaneId::Terminal(7), Some(9), RunCommand::default());
    let mut saw_child_exit = false;
    while let Ok((instruction, _)) = screen_rx.try_recv() {
        if let ScreenInstruction::ChildRunExited {
            pane_id,
            run_generation,
            ..
        } = instruction
        {
            assert_eq!(pane_id, PaneId::Terminal(7));
            assert_eq!(run_generation, generation);
            saw_child_exit = true;
        }
    }
    assert!(saw_child_exit, "the quit callback is the child-exit path");
    assert_eq!(
        stored_pid(&pty, 7),
        Some(4242),
        "exit is applied on the pty thread, not inside the callback"
    );
    handle_forget_requests(&mut pty);
    assert_eq!(stored_pid(&pty, 7), None);
}

#[test]
fn a_replaced_run_keeps_one_record_and_rejects_the_old_write_and_signal() {
    let mock = MockOsApi::new();
    *mock.spawn_pid.lock().unwrap() = Some(100);
    #[cfg(target_os = "linux")]
    let (replacement_pidfd, replacement_raw) = {
        let fd = devnull_pidfd();
        let raw = std::os::fd::AsRawFd::as_raw_fd(&fd);
        (Some(fd), Some(raw))
    };
    #[cfg(not(target_os = "linux"))]
    let replacement_pidfd = None;
    mock.plan_rerun(200, replacement_pidfd);

    let (to_pty_tx, pty_receiver) = channels::unbounded();
    let to_pty = SenderWithContext::new(to_pty_tx);
    let (writer_tx, writer_rx) = channels::unbounded();
    let writer_sender: SenderWithContext<PtyWriteInstruction> = SenderWithContext::new(writer_tx);
    let bus: Bus<PtyInstruction> = Bus::new(
        vec![pty_receiver],
        None,
        Some(&to_pty),
        None,
        None,
        Some(&writer_sender),
        None,
        Some(Box::new(mock.clone())),
    )
    .should_silently_fail();
    let mut pty = Pty::new(bus, false, None, None, Default::default());
    let run_command = RunCommand {
        command: PathBuf::from("true"),
        ..Default::default()
    };
    let old_generation = pty.mint_run_generation();
    let (terminal_id, _) = pty
        .spawn_terminal(
            Some(TerminalAction::RunCommand(run_command.clone())),
            ClientTabIndexOrPaneId::TabIndex(0),
            None,
            old_generation,
        )
        .unwrap();
    assert_eq!(terminal_id, 7);
    let old_quit = mock.quit_cb.lock().unwrap().take().unwrap();
    assert_eq!(stored_pid(&pty, 7), Some(100));
    assert_eq!(stored_generation(&pty, 7), Some(old_generation));
    assert!(
        pty.panes
            .get(&7)
            .and_then(|pane| pane.run.as_ref())
            .and_then(|run| run.task.as_ref())
            .is_some(),
        "spawn registers one run and its reader"
    );
    assert_eq!(
        pty.panes.values().filter(|pane| pane.run.is_some()).count(),
        1
    );
    assert_eq!(mock.bound_generation(7), Some(old_generation));
    mock.clear_applied_writes();
    mock.write_to_tty_stdin_for_run(7, Some(old_generation), b"old")
        .unwrap();

    let new_generation = pty.mint_run_generation();
    pty.rerun_command_in_pane(PaneId::Terminal(7), run_command, new_generation)
        .unwrap();

    assert_eq!(stored_pid(&pty, 7), Some(200));
    assert_eq!(stored_generation(&pty, 7), Some(new_generation));
    assert!(
        pty.panes
            .get(&7)
            .and_then(|pane| pane.run.as_ref())
            .and_then(|run| run.task.as_ref())
            .is_some(),
        "the replacement has its own reader"
    );
    #[cfg(target_os = "linux")]
    assert_eq!(
        stored_pidfd_raw(&pty, 7),
        replacement_raw,
        "the run record holds the replacement pidfd, not a second map entry"
    );
    assert_eq!(
        pty.panes.values().filter(|pane| pane.run.is_some()).count(),
        1
    );
    assert_eq!(mock.bound_generation(7), Some(new_generation));

    mock.clear_applied_writes();
    assert!(
        mock.write_to_tty_stdin_for_run(7, Some(old_generation), b"stale")
            .is_err(),
        "a write stamped for the old run must not apply to the replacement"
    );
    assert!(mock.applied_writes().is_empty());
    mock.write_to_tty_stdin_for_run(7, Some(new_generation), b"new")
        .unwrap();
    assert_eq!(mock.applied_writes()[0].2, b"new");

    pty.send_sigint_to_pane(PaneId::Terminal(7));
    #[cfg(target_os = "linux")]
    {
        assert!(
            mock.signals().is_empty(),
            "the signal uses the replacement pidfd, not the old pid: {:?}",
            mock.signals()
        );
        assert_eq!(stored_pid(&pty, 7), Some(200));
    }
    #[cfg(not(target_os = "linux"))]
    assert_eq!(mock.signals(), vec![("SIGINT", 200)]);

    old_quit(PaneId::Terminal(7), Some(0), RunCommand::default());
    handle_forget_requests(&mut pty);
    assert_eq!(
        stored_pid(&pty, 7),
        Some(200),
        "the old run's exit must not clear the replacement"
    );

    let writer_instructions: Vec<PtyWriteInstruction> = std::iter::from_fn(|| {
        writer_rx
            .try_recv()
            .ok()
            .map(|(instruction, _)| instruction)
    })
    .collect();
    assert!(writer_instructions.iter().any(|instruction| matches!(
        instruction,
        PtyWriteInstruction::AdoptRun(7, generation) if *generation == old_generation
    )));
    assert!(writer_instructions.iter().any(|instruction| matches!(
        instruction,
        PtyWriteInstruction::AdoptRun(7, generation) if *generation == new_generation
    )));
}
