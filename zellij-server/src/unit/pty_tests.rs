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
    // (signal, pid) for every kill/force_kill/send_sigint request
    signals: Arc<Mutex<Vec<(&'static str, u32)>>>,
    // pid handed out by spawn_terminal, and the quit callback it was given
    spawn_pid: Arc<Mutex<Option<u32>>>,
    quit_cb: Arc<Mutex<Option<Box<dyn Fn(PaneId, Option<i32>, RunCommand) + Send>>>>,
}

impl MockOsApi {
    fn new() -> Self {
        MockOsApi {
            cwds: Arc::new(Mutex::new(HashMap::new())),
            cmds: Arc::new(Mutex::new(HashMap::new())),
            cmds_by_ppid: Arc::new(Mutex::new(HashMap::new())),
            signals: Arc::new(Mutex::new(Vec::new())),
            spawn_pid: Arc::new(Mutex::new(None)),
            quit_cb: Arc::new(Mutex::new(None)),
        }
    }
    fn signals(&self) -> Vec<(&'static str, u32)> {
        self.signals.lock().unwrap().clone()
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
    ) -> anyhow::Result<(u32, Box<dyn AsyncReader>, Option<u32>)> {
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
        ))
    }
    fn write_to_tty_stdin(&self, _: u32, buf: &[u8]) -> anyhow::Result<usize> {
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
    fn get_foreground_cmds(
        &self,
        panes: &[(u32, u32)],
        _: &Option<String>,
    ) -> HashMap<u32, Vec<String>> {
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
        _: Box<dyn Fn(PaneId, Option<i32>, RunCommand) + Send>,
        _pane_env: &crate::os_input_output::PaneEnv,
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
    let pty = Pty::new(bus, false, None, None, Default::default());
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
    assert!(pty
        .pane_activity_flags
        .get(&1)
        .unwrap()
        .load(Ordering::Relaxed));

    pty.update_and_report_cwds();

    assert!(
        !pty.pane_activity_flags
            .get(&1)
            .unwrap()
            .load(Ordering::Relaxed),
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

fn session_with_terminal_pane(terminal_id: u32) -> SessionLayoutMetadata {
    let mut meta = SessionLayoutMetadata::default();
    let pane = crate::session_layout_metadata::PaneLayoutMetadata::new(
        PaneId::Terminal(terminal_id),
        zellij_utils::pane_size::PaneGeom::default(),
        false,
        None,
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

    assert!(!pty.resurrect_commands.contains_key(&1));
}

#[test]
fn resurrect_command_for_unknown_terminal_is_rejected() {
    let mock = MockOsApi::new();
    let (mut pty, _rx) = make_pty_with_plugin_receiver(mock);

    let result = pty.set_resurrect_command(7, Some(resurrect_command("opencode", vec![], None)));

    assert!(result.is_err());
    assert!(pty.resurrect_commands.is_empty());
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
        pty.id_to_child_pid.get(&3),
        Some(&200),
        "the replacement run's process must stay addressable"
    );

    pty.clear_exited_process_identity(3, new_run);
    assert_eq!(pty.id_to_child_pid.get(&3), None);
    assert!(pty.child_run_generations.is_empty());
}

#[test]
fn a_held_pane_still_accepts_a_resurrect_command() {
    // Forgetting the exited process must not make the held pane itself
    // unknown: its terminal lives until the pane is closed.
    let (mut pty, _plugin_rx) = make_pty_with_plugin_receiver(MockOsApi::new());
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    pty.task_handles.insert(5, runtime.spawn(async {}));
    let run = pty.mint_run_generation();
    pty.record_child_pid(5, 500, run);
    pty.clear_exited_process_identity(5, run);
    assert_eq!(pty.id_to_child_pid.get(&5), None);
    assert!(pty.set_resurrect_command(5, None).is_ok());
    assert!(pty.set_resurrect_command(6, None).is_err());
}

#[test]
fn forgetting_a_child_drops_its_run_generation() {
    let (mut pty, _plugin_rx) = make_pty_with_plugin_receiver(MockOsApi::new());
    let run = pty.mint_run_generation();
    pty.record_child_pid(4, 400, run);
    assert_eq!(pty.forget_child_pid(4), Some(400));
    assert!(pty.child_run_generations.is_empty());
    // A notification for the forgotten run is a no-op.
    pty.clear_exited_process_identity(4, run);
    assert_eq!(pty.id_to_child_pid.get(&4), None);
}

/// Spawns a plain (not held) command through the mock, which hands out the
/// terminal id 7 and the given child PID. Returns the pty, the mock, the
/// receiver of the instructions the pty sends itself, and the exit
/// notification the OS layer would deliver once the child was reaped.
fn spawn_plain_command(
    child_pid: u32,
) -> (
    Pty,
    MockOsApi,
    channels::Receiver<(PtyInstruction, ErrorContext)>,
    Box<dyn Fn(PaneId, Option<i32>, RunCommand) + Send>,
) {
    let mock = MockOsApi::new();
    *mock.spawn_pid.lock().unwrap() = Some(child_pid);
    let (pty_tx, pty_rx) = channels::unbounded();
    let mut bus: Bus<PtyInstruction> = Bus::empty().should_silently_fail();
    bus.os_input = Some(Box::new(mock.clone()));
    bus.senders.to_pty = Some(SenderWithContext::new(pty_tx));
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
    let (terminal_id, _starts_held) = pty
        .spawn_terminal(
            Some(TerminalAction::RunCommand(run_command)),
            ClientTabIndexOrPaneId::TabIndex(0),
            None,
        )
        .unwrap();
    assert_eq!(terminal_id, 7);
    assert_eq!(pty.id_to_child_pid.get(&7), Some(&child_pid));
    let quit_cb = mock.quit_cb.lock().unwrap().take().unwrap();
    (pty, mock, pty_rx, quit_cb)
}

/// What the PTY thread does with the instructions it was sent.
fn handle_forget_requests(
    pty: &mut Pty,
    pty_rx: &channels::Receiver<(PtyInstruction, ErrorContext)>,
) {
    while let Ok((instruction, _)) = pty_rx.try_recv() {
        if let PtyInstruction::ClearPaneProcessIdentity(PaneId::Terminal(id), run_generation) =
            instruction
        {
            pty.clear_exited_process_identity(id, run_generation);
        }
    }
}

#[test]
fn closing_a_pane_whose_child_exited_signals_nothing() {
    // A command that is not held exits and is reaped; the pane is closed
    // afterwards. The PID may already belong to an unrelated process, so it
    // must not be signalled.
    let (mut pty, mock, pty_rx, quit_cb) = spawn_plain_command(4242);
    quit_cb(PaneId::Terminal(7), Some(0), RunCommand::default());
    handle_forget_requests(&mut pty, &pty_rx);
    assert_eq!(pty.id_to_child_pid.get(&7), None);

    pty.send_sigint_to_pane(PaneId::Terminal(7));
    pty.send_sigkill_to_pane(PaneId::Terminal(7));
    pty.close_pane(PaneId::Terminal(7)).unwrap();
    assert_eq!(mock.signals(), vec![], "a reaped PID must not be signalled");
}

#[test]
fn closing_a_pane_whose_child_is_running_still_hangs_it_up() {
    let (mut pty, mock, pty_rx, _quit_cb) = spawn_plain_command(4242);
    handle_forget_requests(&mut pty, &pty_rx);
    pty.close_pane(PaneId::Terminal(7)).unwrap();
    assert_eq!(mock.signals(), vec![("SIGHUP", 4242)]);
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
