use super::*;
use crate::os_input_output::ServerOsApi;
use crate::plugins::PluginInstruction;
use crate::thread_bus::Bus;
use interprocess::local_socket::Stream as LocalSocketStream;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use zellij_utils::channels::{self, SenderWithContext};
use zellij_utils::data::{Event, Palette};
use zellij_utils::errors::ErrorContext;
use zellij_utils::input::command::RunCommand;
use zellij_utils::ipc::{ClientToServerMsg, IpcReceiverWithContext, ServerToClientMsg};

#[derive(Clone)]
struct MockOsApi {
    cwds: Arc<Mutex<HashMap<u32, PathBuf>>>,
    cmds: Arc<Mutex<HashMap<u32, Vec<String>>>>,
    cmds_by_ppid: Arc<Mutex<HashMap<String, Vec<String>>>>,
}

impl MockOsApi {
    fn new() -> Self {
        MockOsApi {
            cwds: Arc::new(Mutex::new(HashMap::new())),
            cmds: Arc::new(Mutex::new(HashMap::new())),
            cmds_by_ppid: Arc::new(Mutex::new(HashMap::new())),
        }
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
        self.cmds_by_ppid
            .lock()
            .unwrap()
            .remove(&ppid.to_string());
    }
}

impl ServerOsApi for MockOsApi {
    fn set_terminal_size_using_terminal_id(
        &self, _: u32, _: u16, _: u16, _: Option<u16>, _: Option<u16>,
    ) -> anyhow::Result<()> {
        Ok(())
    }
    fn spawn_terminal(
        &self, _: TerminalAction, _: Box<dyn Fn(PaneId, Option<i32>, RunCommand) + Send>,
        _: Option<PathBuf>,
    ) -> anyhow::Result<(u32, Box<dyn AsyncReader>, Option<u32>)> {
        unimplemented!()
    }
    fn write_to_tty_stdin(&self, _: u32, buf: &[u8]) -> anyhow::Result<usize> {
        Ok(buf.len())
    }
    fn tcdrain(&self, _: u32) -> anyhow::Result<()> {
        Ok(())
    }
    fn kill(&self, _: u32) -> anyhow::Result<()> {
        Ok(())
    }
    fn force_kill(&self, _: u32) -> anyhow::Result<()> {
        Ok(())
    }
    fn send_sigint(&self, _: u32) -> anyhow::Result<()> {
        Ok(())
    }
    fn box_clone(&self) -> Box<dyn ServerOsApi> {
        Box::new((*self).clone())
    }
    fn send_to_client(&self, _: ClientId, _: ServerToClientMsg) -> anyhow::Result<()> {
        Ok(())
    }
    fn new_client(
        &mut self, _: ClientId, _: LocalSocketStream,
    ) -> anyhow::Result<IpcReceiverWithContext<ClientToServerMsg>> {
        unimplemented!()
    }
    fn new_client_with_reply(
        &mut self, _: ClientId, _: LocalSocketStream,
        _: LocalSocketStream,
    ) -> anyhow::Result<IpcReceiverWithContext<ClientToServerMsg>> {
        unimplemented!()
    }
    fn remove_client(&mut self, _: ClientId) -> anyhow::Result<()> {
        Ok(())
    }
    fn load_palette(&self) -> Palette {
        Palette::default()
    }
    fn get_cwd(&self, pid: u32) -> Option<PathBuf> {
        self.cwds.lock().unwrap().get(&pid).cloned()
    }
    fn get_cwds(&self, pids: Vec<u32>) -> (HashMap<u32, PathBuf>, HashMap<u32, Vec<String>>) {
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
    fn write_to_file(&mut self, _: String, _: Option<String>) -> anyhow::Result<()> {
        Ok(())
    }
    fn re_run_command_in_terminal(
        &self, _: u32, _: RunCommand,
        _: Box<dyn Fn(PaneId, Option<i32>, RunCommand) + Send>,
    ) -> anyhow::Result<(Box<dyn AsyncReader>, Option<u32>)> {
        unimplemented!()
    }
    fn clear_terminal_id(&self, _: u32) -> anyhow::Result<()> {
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
    let pty = Pty::new(bus, false, None, None);
    (pty, plugin_rx)
}

fn set_active_terminal(pty: &mut Pty, terminal_id: u32, child_pid: u32) {
    let flag = Arc::new(AtomicBool::new(true));
    pty.id_to_child_pid.insert(terminal_id, child_pid);
    pty.pane_activity_flags.insert(terminal_id, flag);
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
    pty.pane_activity_flags
        .get(&1)
        .unwrap()
        .store(true, Ordering::Relaxed);

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

    pty.pane_activity_flags
        .get(&1)
        .unwrap()
        .store(true, Ordering::Relaxed);
    pty.update_and_report_cwds();
    let events = collect_command_changed_events(&rx);
    assert!(events.is_empty(), "no event expected when command unchanged");
}

#[test]
fn no_event_for_inactive_terminal() {
    let mock = MockOsApi::new();
    let child_pid = 100;
    mock.set_foreground_cmd(child_pid, vec!["vim".into()]);
    let (mut pty, rx) = make_pty_with_plugin_receiver(mock);
    set_active_terminal(&mut pty, 1, child_pid);
    pty.pane_activity_flags
        .get(&1)
        .unwrap()
        .store(false, Ordering::Relaxed);

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
    pty.pane_activity_flags
        .get(&1)
        .unwrap()
        .store(true, Ordering::Relaxed);

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
    assert!(pty.pane_activity_flags.get(&1).unwrap().load(Ordering::Relaxed));

    pty.update_and_report_cwds();

    assert!(
        !pty.pane_activity_flags.get(&1).unwrap().load(Ordering::Relaxed),
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
    pty.pane_activity_flags
        .get(&2)
        .unwrap()
        .store(false, Ordering::Relaxed);

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
    pty.terminal_cwds.insert(1, PathBuf::from("/home/user"));

    pty.update_and_report_cwds();

    let events = collect_cwd_changed_events(&rx);
    assert!(events.is_empty(), "no event expected when cwd unchanged");
}

// --- OSC7 CWD notification ---

#[test]
fn osc7_emits_cwd_changed() {
    let mock = MockOsApi::new();
    let (mut pty, rx) = make_pty_with_plugin_receiver(mock);
    pty.id_to_child_pid.insert(1, 100);

    pty.notify_cwd_from_osc7(1, PathBuf::from("/tmp/new"));

    let events = collect_cwd_changed_events(&rx);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].0, PaneId::Terminal(1));
    assert_eq!(events[0].1, PathBuf::from("/tmp/new"));
    assert_eq!(
        pty.terminal_cwds.get(&1),
        Some(&PathBuf::from("/tmp/new")),
        "cache should be updated"
    );
}

#[test]
fn osc7_no_event_when_unchanged() {
    let mock = MockOsApi::new();
    let (mut pty, rx) = make_pty_with_plugin_receiver(mock);
    pty.id_to_child_pid.insert(1, 100);
    pty.terminal_cwds.insert(1, PathBuf::from("/same"));

    pty.notify_cwd_from_osc7(1, PathBuf::from("/same"));

    let events = collect_cwd_changed_events(&rx);
    assert!(events.is_empty(), "no event when osc7 path matches cache");
}

#[test]
fn osc7_clears_activity_flag() {
    let mock = MockOsApi::new();
    let (mut pty, _rx) = make_pty_with_plugin_receiver(mock);
    let flag = Arc::new(AtomicBool::new(true));
    pty.id_to_child_pid.insert(1, 100);
    pty.pane_activity_flags.insert(1, flag.clone());

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
