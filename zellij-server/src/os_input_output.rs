use crate::{panes::PaneId, ClientId};

use interprocess::local_socket::Stream as LocalSocketStream;

#[cfg(not(windows))]
use crate::os_input_output_unix::UnixPtyBackend as PtyBackendImpl;
#[cfg(windows)]
use crate::os_input_output_windows::WindowsPtyBackend as PtyBackendImpl;

use interprocess;
use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};
use tempfile::tempfile;
use zellij_utils::{
    channels,
    channels::TrySendError,
    data::Palette,
    errors::prelude::*,
    input::command::{RunCommand, TerminalAction},
    ipc::{
        ClientToServerMsg, ExitReason, IpcReceiverWithContext, IpcSenderWithContext,
        ServerToClientMsg,
    },
    shared::default_palette,
};

use std::{
    collections::{BTreeMap, HashMap},
    env,
    fs::File,
    io::{self, Write},
    path::PathBuf,
    process::Command,
    sync::{Arc, Mutex},
};

pub use async_trait::async_trait;

pub(crate) fn command_exists(cmd: &RunCommand) -> bool {
    let command = &cmd.command;
    match cmd.cwd.as_ref() {
        Some(cwd) => {
            let full_command = cwd.join(&command);
            if full_command.exists() && full_command.is_file() {
                return true;
            }
        },
        None => {
            if command.exists() && command.is_file() {
                return true;
            }
        },
    }

    if let Some(paths) = env::var_os("PATH") {
        for path in env::split_paths(&paths) {
            let full_command = path.join(command);
            if full_command.exists() && full_command.is_file() {
                return true;
            }
        }
    }
    false
}

// this is a utility method to separate the arguments from a pathbuf before we turn it into a
// Command. eg. "/usr/bin/vim -e" ==> "/usr/bin/vim" + "-e" (the latter will be pushed to args)
fn separate_command_arguments(command: &mut PathBuf, args: &mut Vec<String>) {
    let mut parts = vec![];
    let mut current_part = String::new();
    for part in command.display().to_string().split_ascii_whitespace() {
        current_part.push_str(part);
        if current_part.ends_with('\\') {
            let _ = current_part.pop();
            current_part.push(' ');
        } else {
            let current_part = std::mem::replace(&mut current_part, String::new());
            parts.push(current_part);
        }
    }
    if !parts.is_empty() {
        *command = PathBuf::from(parts.remove(0));
        args.append(&mut parts);
    }
}

/// If a [`TerminalAction::OpenFile(file)`] is given, the text editor specified by environment variable `EDITOR`
/// (or `VISUAL`, if `EDITOR` is not set) will be started in the new terminal, with the given
/// file open.
/// If [`TerminalAction::RunCommand(RunCommand)`] is given, the command will be started
/// in the new terminal.
/// If None is given, the shell specified by environment variable `SHELL` will
/// be started in the new terminal.
///
/// Returns (cmd, failover_cmd).
fn build_command(
    terminal_action: TerminalAction,
    default_editor: Option<PathBuf>,
) -> (RunCommand, Option<RunCommand>) {
    let mut failover_cmd_args = None;
    let cmd = match terminal_action {
        TerminalAction::OpenFile(mut payload) => {
            if payload.path.is_relative() {
                if let Some(cwd) = payload.cwd.as_ref() {
                    payload.path = cwd.join(payload.path);
                }
            }
            let mut command = default_editor.unwrap_or_else(|| {
                PathBuf::from(
                    env::var("EDITOR")
                        .unwrap_or_else(|_| env::var("VISUAL").unwrap_or_else(|_| "vi".into())),
                )
            });

            let mut args = vec![];

            if !command.is_dir() {
                separate_command_arguments(&mut command, &mut args);
            }
            let file_to_open = payload
                .path
                .into_os_string()
                .into_string()
                .expect("Not valid Utf8 Encoding");
            if let Some(line_number) = payload.line_number {
                if command.ends_with("vim")
                    || command.ends_with("nvim")
                    || command.ends_with("emacs")
                    || command.ends_with("nano")
                    || command.ends_with("kak")
                {
                    failover_cmd_args = Some(vec![file_to_open.clone()]);
                    args.push(format!("+{}", line_number));
                    args.push(file_to_open);
                } else if command.ends_with("hx") || command.ends_with("helix") {
                    // at the time of writing, helix only supports this syntax
                    // and it might be a good idea to leave this here anyway
                    // to keep supporting old versions
                    args.push(format!("{}:{}", file_to_open, line_number));
                } else {
                    args.push(file_to_open);
                }
            } else {
                args.push(file_to_open);
            }
            RunCommand {
                command,
                args,
                cwd: payload.cwd,
                hold_on_close: false,
                hold_on_start: false,
                ..Default::default()
            }
        },
        TerminalAction::RunCommand(command) => command,
    };
    let failover_cmd = if let Some(failover_cmd_args) = failover_cmd_args {
        let mut failover = cmd.clone();
        failover.args = failover_cmd_args;
        Some(failover)
    } else {
        None
    };
    (cmd, failover_cmd)
}

// The ClientSender is in charge of sending messages to the client on a special thread
// This is done so that when the unix socket buffer is full, we won't block the entire router
// thread
// When the above happens, the ClientSender buffers messages in hopes that the congestion will be
// freed until we runs out of buffer space.
// If we run out of buffer space, we bubble up an error sot hat the router thread will give up on
// this client and we'll stop sending messages to it.
// If the client ever becomes responsive again, we'll send one final "Buffer full" message so it
// knows what happened.
#[derive(Clone)]
struct ClientSender {
    client_id: ClientId,
    client_buffer_sender: channels::Sender<ServerToClientMsg>,
}

impl ClientSender {
    pub fn new(client_id: ClientId, mut sender: IpcSenderWithContext<ServerToClientMsg>) -> Self {
        // FIXME(hartan): This queue is responsible for buffering messages between server and
        // client. If it fills up, the client is disconnected with a "Buffer full" sort of error
        // message. It was previously found to be too small (with depth 50), so it was increased to
        // 5000 instead. This decision was made because it was found that a queue of depth 5000
        // doesn't cause noticable increase in RAM usage, but there's no reason beyond that. If in
        // the future this is found to fill up too quickly again, it may be worthwhile to increase
        // the size even further (or better yet, implement a redraw-on-backpressure mechanism).
        // We, the zellij maintainers, have decided against an unbounded
        // queue for the time being because we want to prevent e.g. the whole session being killed
        // (by OOM-killers or some other mechanism) just because a single client doesn't respond.
        let (client_buffer_sender, client_buffer_receiver) = channels::bounded(5000);
        std::thread::spawn(move || {
            let err_context = || format!("failed to send message to client {client_id}");
            for msg in client_buffer_receiver.iter() {
                sender
                    .send_server_msg(msg)
                    .with_context(err_context)
                    .non_fatal();
            }
            let _ = sender.send_server_msg(ServerToClientMsg::Exit {
                exit_reason: ExitReason::Disconnect,
            });
        });
        ClientSender {
            client_id,
            client_buffer_sender,
        }
    }
    pub fn send_or_buffer(&self, msg: ServerToClientMsg) -> Result<()> {
        let err_context = || {
            format!(
                "failed to send or buffer message for client {}",
                self.client_id
            )
        };

        self.client_buffer_sender
            .try_send(msg)
            .or_else(|err| {
                if let TrySendError::Full(_) = err {
                    log::warn!(
                        "client {} is processing server messages too slow",
                        self.client_id
                    );
                }
                Err(err)
            })
            .with_context(err_context)
    }
}

#[derive(Clone)]
pub struct ServerOsInputOutput {
    pty_backend: PtyBackendImpl,
    client_senders: Arc<Mutex<HashMap<ClientId, ClientSender>>>,
    cached_resizes: Arc<Mutex<Option<BTreeMap<u32, (u16, u16, Option<u16>, Option<u16>)>>>>,
}

/// A null `AsyncReader` for held panes (produces EOF immediately).
pub(crate) struct NullAsyncReader;

// async fn in traits is not supported by rust, so dtolnay's excellent async_trait macro is being
// used. See https://smallcultfollowing.com/babysteps/blog/2019/10/26/async-fn-in-traits-are-hard/
#[async_trait]
pub trait AsyncReader: Send + Sync {
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize, io::Error>;
}

#[async_trait]
impl AsyncReader for NullAsyncReader {
    async fn read(&mut self, _buf: &mut [u8]) -> Result<usize, io::Error> {
        Ok(0) // EOF
    }
}

/// The `ServerOsApi` trait represents an abstract interface to the features of an operating system that
/// Zellij server requires.
pub trait ServerOsApi: Send + Sync {
    fn set_terminal_size_using_terminal_id(
        &self,
        id: u32,
        cols: u16,
        rows: u16,
        width_in_pixels: Option<u16>,
        height_in_pixels: Option<u16>,
    ) -> Result<()>;
    /// Spawn a new terminal, with a terminal action. The returned tuple contains:
    /// - terminal_id (u32)
    /// - an async reader for the PTY output
    /// - the child process PID, if available (Option<u32>)
    fn spawn_terminal(
        &self,
        terminal_action: TerminalAction,
        quit_cb: Box<dyn Fn(PaneId, Option<i32>, RunCommand) + Send>,
        default_editor: Option<PathBuf>,
    ) -> Result<(u32, Box<dyn AsyncReader>, Option<u32>)>;
    // reserves a terminal id without actually opening a terminal
    fn reserve_terminal_id(&self) -> Result<u32> {
        unimplemented!()
    }
    /// Write bytes to the standard input of the virtual terminal referred to by `terminal_id`.
    fn write_to_tty_stdin(&self, terminal_id: u32, buf: &[u8]) -> Result<usize>;
    /// Wait until all output written to the terminal has been transmitted.
    fn tcdrain(&self, terminal_id: u32) -> Result<()>;
    /// Terminate the process with process ID `pid`. (SIGHUP)
    fn kill(&self, pid: u32) -> Result<()>;
    /// Terminate the process with process ID `pid`. (SIGKILL)
    fn force_kill(&self, pid: u32) -> Result<()>;
    /// Send SIGINT to the process with process ID `pid`
    fn send_sigint(&self, pid: u32) -> Result<()>;
    /// Returns a [`Box`] pointer to this [`ServerOsApi`] struct.
    fn box_clone(&self) -> Box<dyn ServerOsApi>;
    fn send_to_client(&self, client_id: ClientId, msg: ServerToClientMsg) -> Result<()>;
    fn new_client(
        &mut self,
        client_id: ClientId,
        stream: LocalSocketStream,
    ) -> Result<IpcReceiverWithContext<ClientToServerMsg>>;
    /// Create a new client with a separate reply stream (Windows dual-pipe IPC).
    fn new_client_with_reply(
        &mut self,
        client_id: ClientId,
        stream: LocalSocketStream,
        reply_stream: LocalSocketStream,
    ) -> Result<IpcReceiverWithContext<ClientToServerMsg>>;
    fn remove_client(&mut self, client_id: ClientId) -> Result<()>;
    fn load_palette(&self) -> Palette;
    /// Returns the current working directory for a given pid
    fn get_cwd(&self, pid: u32) -> Option<PathBuf>;
    /// Returns the current working directory for multiple pids
    fn get_cwds(&self, _pids: Vec<u32>) -> (HashMap<u32, PathBuf>, HashMap<u32, Vec<String>>) {
        (HashMap::new(), HashMap::new())
    }
    /// Get a list of all running commands by their parent process id
    fn get_all_cmds_by_ppid(&self, _post_hook: &Option<String>) -> HashMap<String, Vec<String>> {
        HashMap::new()
    }
    /// Writes the given buffer to a string
    fn write_to_file(&mut self, buf: String, file: Option<String>) -> Result<()>;

    fn re_run_command_in_terminal(
        &self,
        terminal_id: u32,
        run_command: RunCommand,
        quit_cb: Box<dyn Fn(PaneId, Option<i32>, RunCommand) + Send>,
    ) -> Result<(Box<dyn AsyncReader>, Option<u32>)>;
    fn clear_terminal_id(&self, terminal_id: u32) -> Result<()>;
    fn cache_resizes(&mut self) {}
    fn apply_cached_resizes(&mut self) {}
}

impl ServerOsApi for ServerOsInputOutput {
    fn set_terminal_size_using_terminal_id(
        &self,
        id: u32,
        cols: u16,
        rows: u16,
        width_in_pixels: Option<u16>,
        height_in_pixels: Option<u16>,
    ) -> Result<()> {
        if let Some(cached_resizes) = self.cached_resizes.lock().unwrap().as_mut() {
            cached_resizes.insert(id, (cols, rows, width_in_pixels, height_in_pixels));
            return Ok(());
        }
        self.pty_backend
            .set_terminal_size(id, cols, rows, width_in_pixels, height_in_pixels)
    }
    fn spawn_terminal(
        &self,
        terminal_action: TerminalAction,
        quit_cb: Box<dyn Fn(PaneId, Option<i32>, RunCommand) + Send>,
        default_editor: Option<PathBuf>,
    ) -> Result<(u32, Box<dyn AsyncReader>, Option<u32>)> {
        let err_context = || "failed to spawn terminal".to_string();

        let terminal_id = self
            .pty_backend
            .next_terminal_id()
            .context("no more terminal IDs left to allocate")?;

        self.pty_backend.reserve_terminal_id(terminal_id);

        let (cmd, failover_cmd) = build_command(terminal_action, default_editor);

        let (async_reader, child_fd) = self
            .pty_backend
            .spawn_terminal(cmd, failover_cmd, quit_cb, terminal_id)
            .with_context(err_context)?;

        Ok((terminal_id, async_reader, Some(child_fd as u32)))
    }
    fn reserve_terminal_id(&self) -> Result<u32> {
        let terminal_id = self
            .pty_backend
            .next_terminal_id()
            .context("no more terminal IDs available")?;
        self.pty_backend.reserve_terminal_id(terminal_id);
        Ok(terminal_id)
    }
    fn write_to_tty_stdin(&self, terminal_id: u32, buf: &[u8]) -> Result<usize> {
        self.pty_backend.write_to_tty_stdin(terminal_id, buf)
    }
    fn tcdrain(&self, terminal_id: u32) -> Result<()> {
        self.pty_backend.tcdrain(terminal_id)
    }
    fn box_clone(&self) -> Box<dyn ServerOsApi> {
        Box::new((*self).clone())
    }
    fn kill(&self, pid: u32) -> Result<()> {
        self.pty_backend.kill(pid)
    }
    fn force_kill(&self, pid: u32) -> Result<()> {
        self.pty_backend.force_kill(pid)
    }
    fn send_sigint(&self, pid: u32) -> Result<()> {
        self.pty_backend.send_sigint(pid)
    }
    fn send_to_client(&self, client_id: ClientId, msg: ServerToClientMsg) -> Result<()> {
        let err_context = || format!("failed to send message to client {client_id}");

        if let Some(sender) = self
            .client_senders
            .lock()
            .to_anyhow()
            .with_context(err_context)?
            .get_mut(&client_id)
        {
            sender.send_or_buffer(msg).with_context(err_context)
        } else {
            Ok(())
        }
    }

    fn new_client(
        &mut self,
        client_id: ClientId,
        stream: LocalSocketStream,
    ) -> Result<IpcReceiverWithContext<ClientToServerMsg>> {
        let receiver = IpcReceiverWithContext::new(stream);
        let sender = ClientSender::new(client_id, receiver.get_sender());
        self.client_senders
            .lock()
            .to_anyhow()
            .with_context(|| format!("failed to create new client {client_id}"))?
            .insert(client_id, sender);
        Ok(receiver)
    }

    fn new_client_with_reply(
        &mut self,
        client_id: ClientId,
        stream: LocalSocketStream,
        reply_stream: LocalSocketStream,
    ) -> Result<IpcReceiverWithContext<ClientToServerMsg>> {
        let receiver = IpcReceiverWithContext::new(stream);
        let sender = ClientSender::new(client_id, IpcSenderWithContext::new(reply_stream));
        self.client_senders
            .lock()
            .to_anyhow()
            .with_context(|| format!("failed to create new client {client_id}"))?
            .insert(client_id, sender);
        Ok(receiver)
    }

    fn remove_client(&mut self, client_id: ClientId) -> Result<()> {
        let mut client_senders = self
            .client_senders
            .lock()
            .to_anyhow()
            .with_context(|| format!("failed to remove client {client_id}"))?;
        if client_senders.contains_key(&client_id) {
            client_senders.remove(&client_id);
        }
        Ok(())
    }

    fn load_palette(&self) -> Palette {
        default_palette()
    }

    fn get_cwd(&self, pid: u32) -> Option<PathBuf> {
        let mut system_info = System::new();
        let sysinfo_pid = sysinfo::Pid::from_u32(pid);
        let refresh_kind = ProcessRefreshKind::nothing().with_cwd(UpdateKind::Always);
        system_info.refresh_processes_specifics(
            ProcessesToUpdate::Some(&[sysinfo_pid]),
            false,
            refresh_kind,
        );

        if let Some(process) = system_info.process(sysinfo_pid) {
            if let Some(cwd) = process.cwd() {
                return Some(cwd.to_path_buf());
            }
        }
        None
    }

    fn get_cwds(&self, pids: Vec<u32>) -> (HashMap<u32, PathBuf>, HashMap<u32, Vec<String>>) {
        let mut system_info = System::new();
        let mut cwds = HashMap::new();
        let mut cmds = HashMap::new();

        let sysinfo_pids: Vec<sysinfo::Pid> =
            pids.iter().map(|&p| sysinfo::Pid::from_u32(p)).collect();
        let refresh_kind = ProcessRefreshKind::nothing()
            .with_cwd(UpdateKind::Always)
            .with_cmd(UpdateKind::Always);
        system_info.refresh_processes_specifics(
            ProcessesToUpdate::Some(&sysinfo_pids),
            false,
            refresh_kind,
        );

        for pid in pids {
            let sysinfo_pid = sysinfo::Pid::from_u32(pid);
            if let Some(process) = system_info.process(sysinfo_pid) {
                if let Some(cwd) = process.cwd() {
                    cwds.insert(pid, cwd.to_path_buf());
                }
                let cmd = process.cmd();
                if !cmd.is_empty() {
                    cmds.insert(
                        pid,
                        cmd.iter()
                            .map(|s| s.to_string_lossy().into_owned())
                            .collect(),
                    );
                }
            }
        }

        (cwds, cmds)
    }
    #[cfg(unix)]
    fn get_all_cmds_by_ppid(&self, post_hook: &Option<String>) -> HashMap<String, Vec<String>> {
        // the key is the stringified ppid
        let mut cmds = HashMap::new();
        if let Some(output) = Command::new("ps")
            .args(vec!["-ao", "ppid,args"])
            .output()
            .ok()
        {
            let output = String::from_utf8(output.stdout.clone())
                .unwrap_or_else(|_| String::from_utf8_lossy(&output.stdout).to_string());
            for line in output.lines() {
                let line_parts: Vec<String> = line
                    .trim()
                    .split_ascii_whitespace()
                    .map(|p| p.to_owned())
                    .collect();
                let mut line_parts = line_parts.into_iter();
                let ppid = line_parts.next();
                if let Some(ppid) = ppid {
                    match &post_hook {
                        Some(post_hook) => {
                            let command: Vec<String> = line_parts.clone().collect();
                            let stringified = command.join(" ");
                            let cmd = match run_command_hook(&stringified, post_hook) {
                                Ok(command) => command,
                                Err(e) => {
                                    log::error!("Post command hook failed to run: {}", e);
                                    stringified.to_owned()
                                },
                            };
                            let line_parts: Vec<String> = cmd
                                .trim()
                                .split_ascii_whitespace()
                                .map(|p| p.to_owned())
                                .collect();
                            cmds.insert(ppid.into(), line_parts);
                        },
                        None => {
                            cmds.insert(ppid.into(), line_parts.collect());
                        },
                    }
                }
            }
        }
        cmds
    }

    #[cfg(not(unix))]
    fn get_all_cmds_by_ppid(&self, _post_hook: &Option<String>) -> HashMap<String, Vec<String>> {
        unimplemented!("Windows get_all_cmds_by_ppid not yet implemented")
    }

    fn write_to_file(&mut self, buf: String, name: Option<String>) -> Result<()> {
        let err_context = || "failed to write to file".to_string();

        let mut f: File = match name {
            Some(x) => File::create(x).with_context(err_context)?,
            None => tempfile().with_context(err_context)?,
        };
        write!(f, "{}", buf).with_context(err_context)
    }

    fn re_run_command_in_terminal(
        &self,
        terminal_id: u32,
        run_command: RunCommand,
        quit_cb: Box<dyn Fn(PaneId, Option<i32>, RunCommand) + Send>,
    ) -> Result<(Box<dyn AsyncReader>, Option<u32>)> {
        let (async_reader, child_fd) =
            self.pty_backend
                .spawn_terminal(run_command, None, quit_cb, terminal_id)?;
        Ok((async_reader, Some(child_fd as u32)))
    }
    fn clear_terminal_id(&self, terminal_id: u32) -> Result<()> {
        self.pty_backend.clear_terminal_id(terminal_id);
        Ok(())
    }
    fn cache_resizes(&mut self) {
        if self.cached_resizes.lock().unwrap().is_none() {
            *self.cached_resizes.lock().unwrap() = Some(BTreeMap::new());
        }
    }
    fn apply_cached_resizes(&mut self) {
        let mut cached_resizes = self.cached_resizes.lock().unwrap().take();
        if let Some(cached_resizes) = cached_resizes.as_mut() {
            for (terminal_id, (cols, rows, width_in_pixels, height_in_pixels)) in
                cached_resizes.iter()
            {
                let _ = self.set_terminal_size_using_terminal_id(
                    *terminal_id,
                    *cols,
                    *rows,
                    width_in_pixels.clone(),
                    height_in_pixels.clone(),
                );
            }
        }
    }
}

impl Clone for Box<dyn ServerOsApi> {
    fn clone(&self) -> Box<dyn ServerOsApi> {
        self.box_clone()
    }
}

pub fn get_server_os_input() -> Result<ServerOsInputOutput, std::io::Error> {
    Ok(ServerOsInputOutput {
        pty_backend: PtyBackendImpl::new()?,
        client_senders: Arc::new(Mutex::new(HashMap::new())),
        cached_resizes: Arc::new(Mutex::new(None)),
    })
}

use crate::pty_writer::PtyWriteInstruction;
use crate::thread_bus::ThreadSenders;

pub struct ResizeCache {
    senders: ThreadSenders,
}

impl ResizeCache {
    pub fn new(senders: ThreadSenders) -> Self {
        senders
            .send_to_pty_writer(PtyWriteInstruction::StartCachingResizes)
            .unwrap_or_else(|e| {
                log::error!("Failed to cache resizes: {}", e);
            });
        ResizeCache { senders }
    }
}

impl Drop for ResizeCache {
    fn drop(&mut self) {
        self.senders
            .send_to_pty_writer(PtyWriteInstruction::ApplyCachedResizes)
            .unwrap_or_else(|e| {
                log::error!("Failed to apply cached resizes: {}", e);
            });
    }
}

#[cfg(not(windows))]
fn run_command_hook(
    original_command: &str,
    hook_script: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    let output = Command::new("sh")
        .arg("-c")
        .arg(hook_script)
        .env("RESURRECT_COMMAND", original_command)
        .output()?;

    if !output.status.success() {
        return Err(format!("Hook failed: {}", String::from_utf8_lossy(&output.stderr)).into());
    }
    Ok(String::from_utf8(output.stdout)?.trim().to_string())
}

#[cfg(windows)]
fn run_command_hook(
    _original_command: &str,
    _hook_script: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    unimplemented!("Windows run_command_hook not yet implemented")
}

#[cfg(test)]
#[path = "./unit/os_input_output_tests.rs"]
mod os_input_output_tests;
