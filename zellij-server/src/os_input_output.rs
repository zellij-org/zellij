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
        ClientToServerMsg, ExitReason, IpcReceiverWithContext, IpcSenderWithContext, IpcStream,
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
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc::RecvTimeoutError,
        Arc, Mutex,
    },
    time::Duration,
};

pub use async_trait::async_trait;

/// Check whether a candidate path refers to an executable file, considering
/// PATHEXT extensions on Windows (e.g. `.exe`, `.cmd`).
///
/// On Windows, when the candidate has no extension we try each PATHEXT
/// variant BEFORE the bare match, mirroring cmd.exe's resolution. Tools like
/// Composer install both `composer` (a Unix launcher) and `composer.bat`
/// (the Windows launcher) side by side; returning the bare file would send
/// a non-PE binary to CreateProcessW and fail with ERROR_BAD_EXE_FORMAT.
fn find_executable(candidate: &std::path::Path) -> Option<PathBuf> {
    #[cfg(windows)]
    {
        if candidate.extension().is_none() {
            if let Some(pathext) = env::var_os("PATHEXT") {
                let pathext = pathext.to_string_lossy();
                for ext in pathext.split(';') {
                    let ext = ext.trim();
                    if ext.is_empty() {
                        continue;
                    }
                    let mut with_ext = candidate.as_os_str().to_os_string();
                    with_ext.push(ext);
                    let with_ext_path = PathBuf::from(with_ext);
                    if with_ext_path.exists() && with_ext_path.is_file() {
                        return Some(with_ext_path);
                    }
                }
            }
        }
    }
    if candidate.exists() && candidate.is_file() {
        return Some(candidate.to_path_buf());
    }
    None
}

pub type PaneEnv = BTreeMap<String, Option<String>>;

pub fn env_value(pane_env: &PaneEnv, name: &str) -> Option<String> {
    match pane_env.get(name) {
        Some(value) => value.clone(),
        None => env::var(name).ok(),
    }
}

/// Resolve a command to its absolute path, searching the working directory,
/// then PATH (and PATHEXT on Windows).
pub(crate) fn resolve_command(cmd: &RunCommand, pane_env: &PaneEnv) -> Option<PathBuf> {
    let command = &cmd.command;
    match cmd.cwd.as_ref() {
        Some(cwd) => {
            if let Some(resolved) = find_executable(&cwd.join(command)) {
                return Some(resolved);
            }
        },
        None => {
            if let Some(resolved) = find_executable(command) {
                return Some(resolved);
            }
        },
    }
    let paths = match pane_env.get("PATH") {
        Some(value) => value.clone().map(std::ffi::OsString::from),
        None => env::var_os("PATH"),
    };
    if let Some(paths) = paths {
        for path in env::split_paths(&paths) {
            if let Some(resolved) = find_executable(&path.join(command)) {
                return Some(resolved);
            }
        }
    }
    None
}

#[cfg(not(windows))]
pub(crate) fn command_exists(cmd: &RunCommand, pane_env: &PaneEnv) -> bool {
    resolve_command(cmd, pane_env).is_some()
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
    pane_env: &PaneEnv,
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
                    env_value(pane_env, "EDITOR")
                        .or_else(|| env_value(pane_env, "VISUAL"))
                        .unwrap_or_else(|| "vi".into()),
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
const CLIENT_BUFFER_LIMIT: usize = 5000;
// A message-count cap does not bound retained payload: 5000 render
// frames can hold gigabytes. The queue is therefore also bounded by
// buffered payload bytes. A message is always admitted into a queue that
// holds no payload, so a single large message cannot disconnect a client.
const CLIENT_BUFFER_BYTE_LIMIT: usize = 256 * 1024 * 1024;

/// Heap overhead charged per `String` held in a `Vec<String>`: the 24-byte
/// header in the vector's backing allocation, plus allocator rounding. A
/// message of many short lines is dominated by this, not by the text.
const STRING_OVERHEAD: usize = std::mem::size_of::<String>() + 16;

fn strings_bytes(strings: &[String]) -> usize {
    strings
        .iter()
        .map(|string| string.len().saturating_add(STRING_OVERHEAD))
        .sum()
}

fn opt_str_bytes(string: &Option<String>) -> usize {
    string.as_ref().map(String::len).unwrap_or(0)
}

/// Size of the JSON form of a value, as an estimate for deeply nested
/// types that are not worth walking by hand. Never less than the size of
/// the value itself.
fn serialized_bytes<T: serde::Serialize>(value: &T) -> usize {
    struct Counter(usize);
    impl std::io::Write for Counter {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0 = self.0.saturating_add(buf.len());
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut counter = Counter(0);
    let _ = serde_json::to_writer(&mut counter, value);
    counter.0.max(std::mem::size_of::<T>())
}

fn session_preview_bytes(preview: &zellij_utils::data::SessionPreview) -> usize {
    use zellij_utils::data::{SessionPreviewPane, SessionPreviewTab};
    let panes_bytes = |panes: &[SessionPreviewPane]| -> usize {
        panes
            .iter()
            .map(|pane| {
                std::mem::size_of::<SessionPreviewPane>()
                    .saturating_add(pane.title.len())
                    .saturating_add(opt_str_bytes(&pane.contents))
            })
            .sum()
    };
    let tabs_bytes: usize = preview
        .tabs
        .iter()
        .map(|tab| {
            std::mem::size_of::<SessionPreviewTab>()
                .saturating_add(tab.name.len())
                .saturating_add(panes_bytes(&tab.panes))
        })
        .sum();
    preview
        .contents
        .len()
        .saturating_add(preview.session_name.len())
        .saturating_add(strings_bytes(&preview.tab_names))
        .saturating_add(opt_str_bytes(&preview.error))
        .saturating_add(tabs_bytes)
}

fn mobile_state_bytes(payload: &zellij_utils::ipc::MobileStatePayload) -> usize {
    use zellij_utils::ipc::{MobilePanePayload, MobileSessionPayload, MobileTabPayload};
    payload
        .session_name
        .len()
        .saturating_add(
            payload
                .tabs
                .iter()
                .map(|tab| std::mem::size_of::<MobileTabPayload>() + tab.name.len())
                .sum::<usize>(),
        )
        .saturating_add(
            payload
                .panes
                .iter()
                .map(|pane| std::mem::size_of::<MobilePanePayload>() + pane.title.len())
                .sum::<usize>(),
        )
        .saturating_add(
            payload
                .sessions
                .iter()
                .map(|session| std::mem::size_of::<MobileSessionPayload>() + session.name.len())
                .sum::<usize>(),
        )
}

/// Approximate heap payload of a client-bound message, so the queue byte
/// budget accounts for what it retains and not just message envelopes.
/// Deliberately exhaustive: a new variant must decide what it is charged.
fn server_msg_payload_bytes(msg: &ServerToClientMsg) -> usize {
    match msg {
        ServerToClientMsg::Render { content } => content.len(),
        ServerToClientMsg::RenderFrame { frame } => frame.len(),
        ServerToClientMsg::Log { lines } | ServerToClientMsg::LogError { lines } => {
            strings_bytes(lines)
        },
        ServerToClientMsg::CliPipeOutput { pipe_name, output } => {
            pipe_name.len().saturating_add(output.len())
        },
        ServerToClientMsg::UnblockCliPipeInput { pipe_name, .. } => pipe_name.len(),
        ServerToClientMsg::PaneRenderUpdate {
            viewport,
            scrollback,
            ..
        } => strings_bytes(viewport).saturating_add(
            scrollback
                .as_ref()
                .map(|scrollback| strings_bytes(scrollback))
                .unwrap_or(0),
        ),
        ServerToClientMsg::EmitNestedSessionFrame { payload_bytes } => payload_bytes.len(),
        ServerToClientMsg::PaneMirror { payload } => payload.len(),
        ServerToClientMsg::ForwardQueryToHost { query_bytes, .. } => query_bytes.len(),
        ServerToClientMsg::RenamedSession { name } => name.len(),
        ServerToClientMsg::SessionPreview { preview } => {
            preview.as_ref().map(session_preview_bytes).unwrap_or(0)
        },
        ServerToClientMsg::MobileState { payload } => mobile_state_bytes(payload),
        ServerToClientMsg::Exit { exit_reason } => match exit_reason {
            ExitReason::Error(error) => error.len(),
            _ => 0,
        },
        ServerToClientMsg::SwitchSession { connect_to_session } => {
            serialized_bytes(connect_to_session)
        },
        ServerToClientMsg::TerminalSettingsChanged { settings } => serialized_bytes(&**settings),
        ServerToClientMsg::SessionAttention { attention } => serialized_bytes(attention),
        ServerToClientMsg::HostProgramStatus { status } => status
            .as_ref()
            .map(|status| {
                status
                    .state
                    .len()
                    .saturating_add(opt_str_bytes(&status.kind))
                    .saturating_add(status.msg.len())
            })
            .unwrap_or(0),
        // No owned heap data.
        ServerToClientMsg::UnblockInputThread
        | ServerToClientMsg::Connected
        | ServerToClientMsg::QueryTerminalSize
        | ServerToClientMsg::SetSoftKeyboard { .. }
        | ServerToClientMsg::PasteFromClipboard { .. }
        | ServerToClientMsg::StartWebServer
        | ServerToClientMsg::ConfigFileUpdated
        | ServerToClientMsg::HostTerminalThemeChanged { .. }
        | ServerToClientMsg::SubscribedPaneClosed { .. } => 0,
    }
}

#[derive(Clone)]
struct ClientBuffer {
    sender: channels::Sender<(ServerToClientMsg, usize)>,
    queued: Arc<AtomicUsize>,
    queued_bytes: Arc<AtomicUsize>,
    limit: usize,
    byte_limit: usize,
}

struct ClientBufferReceiver {
    receiver: channels::Receiver<(ServerToClientMsg, usize)>,
    queued: Arc<AtomicUsize>,
    queued_bytes: Arc<AtomicUsize>,
}

fn client_buffer(limit: usize, byte_limit: usize) -> (ClientBuffer, ClientBufferReceiver) {
    let (sender, receiver) = channels::unbounded();
    let queued = Arc::new(AtomicUsize::new(0));
    let queued_bytes = Arc::new(AtomicUsize::new(0));
    (
        ClientBuffer {
            sender,
            queued: queued.clone(),
            queued_bytes: queued_bytes.clone(),
            limit,
            byte_limit,
        },
        ClientBufferReceiver {
            receiver,
            queued,
            queued_bytes,
        },
    )
}

impl ClientBuffer {
    fn try_send(&self, msg: ServerToClientMsg) -> Result<(), TrySendError<ServerToClientMsg>> {
        let payload = server_msg_payload_bytes(&msg);
        if self.queued.fetch_add(1, Ordering::AcqRel) >= self.limit {
            self.queued.fetch_sub(1, Ordering::AcqRel);
            return Err(TrySendError::Full(msg));
        }
        let queued_bytes = self.queued_bytes.fetch_add(payload, Ordering::AcqRel);
        if queued_bytes > 0 && queued_bytes.saturating_add(payload) > self.byte_limit {
            self.queued_bytes.fetch_sub(payload, Ordering::AcqRel);
            self.queued.fetch_sub(1, Ordering::AcqRel);
            return Err(TrySendError::Full(msg));
        }
        self.sender.send((msg, payload)).map_err(|err| {
            self.queued.fetch_sub(1, Ordering::AcqRel);
            self.queued_bytes.fetch_sub(payload, Ordering::AcqRel);
            TrySendError::Disconnected(err.0 .0)
        })
    }
}

impl ClientBufferReceiver {
    fn recv(&self) -> Option<ServerToClientMsg> {
        let (msg, payload) = self.receiver.recv().ok()?;
        self.queued.fetch_sub(1, Ordering::AcqRel);
        self.queued_bytes.fetch_sub(payload, Ordering::AcqRel);
        Some(msg)
    }
}

/// How long the writer of a removed client gets to flush what is already
/// queued for it, such as the exit notice the server sends right before
/// removing the client. If it is still stuck in a socket write after
/// that, its queue is dropped and the socket shut down.
const REMOVED_CLIENT_FLUSH_GRACE: Duration = Duration::from_secs(2);
/// How long, after the socket was shut down, the writer gets to notice and
/// exit before the removal gives up waiting for it.
const REMOVED_CLIENT_WRITER_EXIT_GRACE: Duration = Duration::from_secs(5);

struct ClientSender {
    client_id: ClientId,
    client_buffer_sender: ClientBuffer,
    // Dropped by the writer thread when it exits.
    writer_done: std::sync::mpsc::Receiver<()>,
    // Set to make the writer drop whatever is still queued.
    abandon_queue: Arc<AtomicBool>,
    // Another handle to the client's socket, to unblock a stuck write.
    shutdown_handle: Option<Box<dyn IpcStream>>,
}

impl ClientSender {
    pub fn new(client_id: ClientId, mut sender: IpcSenderWithContext<ServerToClientMsg>) -> Self {
        // FIXME(hartan): This queue is responsible for buffering messages between server and
        // client. If it fills up, the client is disconnected with a "Buffer full" sort of error
        // message. It was previously found to be too small (with depth 50), so it was increased to
        // 5000 instead. This decision was made because it was found that a queue of depth 5000
        // doesn't cause noticeable increase in RAM usage, but there's no reason beyond that. If in
        // the future this is found to fill up too quickly again, it may be worthwhile to increase
        // the size even further (or better yet, implement a redraw-on-backpressure mechanism).
        // We, the zellij maintainers, have decided against an unbounded
        // queue for the time being because we want to prevent e.g. the whole session being killed
        // (by OOM-killers or some other mechanism) just because a single client doesn't respond.
        let (client_buffer_sender, client_buffer_receiver) =
            client_buffer(CLIENT_BUFFER_LIMIT, CLIENT_BUFFER_BYTE_LIMIT);
        let (writer_done_sender, writer_done) = std::sync::mpsc::channel::<()>();
        let abandon_queue = Arc::new(AtomicBool::new(false));
        let shutdown_handle = sender.try_clone_stream().ok();
        let writer_abandon_queue = abandon_queue.clone();
        std::thread::spawn(move || {
            let _writer_done_sender = writer_done_sender;
            let err_context = || format!("failed to send message to client {client_id}");
            let mut stream_failed = false;
            while let Some(msg) = client_buffer_receiver.recv() {
                if writer_abandon_queue.load(Ordering::Acquire) {
                    // The client was removed while this writer was stuck;
                    // drop the queue instead of sending it to a socket
                    // that nobody is reading.
                    break;
                }
                if let Err(error) = sender.send_server_msg(msg).with_context(err_context) {
                    if writer_abandon_queue.load(Ordering::Acquire) {
                        log::warn!(
                            "abandoned the queue of removed client {client_id} that was not reading"
                        );
                    } else {
                        log::error!("{:?}; stopping client writer", error);
                    }
                    stream_failed = true;
                    break;
                }
            }
            if !stream_failed && !writer_abandon_queue.load(Ordering::Acquire) {
                let _ = sender.send_server_msg(ServerToClientMsg::Exit {
                    exit_reason: ExitReason::Disconnect,
                });
            }
        });
        ClientSender {
            client_id,
            client_buffer_sender,
            writer_done,
            abandon_queue,
            shutdown_handle,
        }
    }
    /// Called when the client is removed. The writer keeps flushing what is
    /// already queued, so that a final message sent just before the removal
    /// still goes out. If it has not finished within `grace` it is stuck
    /// writing to a client that is not reading: have it drop its queue and
    /// shut the socket down to fail the write it is blocked in. The
    /// returned handle finishes once the writer is done, or once it is
    /// given up on, and yields whether the writer actually exited.
    ///
    /// Platform note: shutting the socket down is what unblocks a stuck
    /// write, and `IpcStream::shutdown_stream` does that on Unix only. On
    /// Windows it is a no-op, so a writer blocked in a write to a named pipe
    /// whose peer is not reading is released only when the peer reads or
    /// the pipe closes; `abandon_queue` is checked between writes, which a
    /// blocked write never reaches. The retained memory is still bounded by
    /// the client queue caps, and the handle then yields `false`.
    fn remove(self, grace: Duration) -> std::thread::JoinHandle<bool> {
        let ClientSender {
            client_id,
            client_buffer_sender,
            writer_done,
            abandon_queue,
            shutdown_handle,
        } = self;
        // With the sending half gone, the writer ends once the queue is empty.
        drop(client_buffer_sender);
        std::thread::spawn(move || {
            // Nothing is ever sent: this returns once the writer drops its half.
            match writer_done.recv_timeout(grace) {
                Err(RecvTimeoutError::Timeout) => {},
                _ => return true,
            }
            log::warn!("client {client_id} was removed while not reading; dropping its queue");
            abandon_queue.store(true, Ordering::Release);
            if let Some(Err(error)) = shutdown_handle.map(|handle| handle.shutdown_stream()) {
                log::error!("failed to shut down the socket of client {client_id}: {error}");
            }
            match writer_done.recv_timeout(REMOVED_CLIENT_WRITER_EXIT_GRACE) {
                Err(RecvTimeoutError::Timeout) => {
                    log::warn!(
                        "the writer of removed client {client_id} is still blocked in a write; \
                         it exits when the client reads or closes its end"
                    );
                    false
                },
                _ => true,
            }
        })
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
    async fn read_chunk(&mut self, max: usize) -> Result<Vec<u8>, io::Error>;
}

#[async_trait]
impl AsyncReader for NullAsyncReader {
    async fn read_chunk(&mut self, _max: usize) -> Result<Vec<u8>, io::Error> {
        Ok(Vec::new())
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
        pane_env: &PaneEnv,
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
    fn register_client(
        &mut self,
        client_id: ClientId,
        receiver: &IpcReceiverWithContext<ClientToServerMsg>,
    ) -> Result<()>;
    fn register_client_with_reply(
        &mut self,
        client_id: ClientId,
        reply_stream: LocalSocketStream,
    ) -> Result<()>;
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
    /// For each `(terminal_id, shell_pid)` pane, return the foreground command running in its
    /// controlling terminal, keyed by `terminal_id`.
    fn get_foreground_cmds(
        &self,
        _panes: &[(u32, u32)],
        _post_hook: &Option<String>,
    ) -> HashMap<u32, Vec<String>> {
        HashMap::new()
    }
    /// Writes the given buffer to a string
    fn write_to_file(&mut self, buf: String, file: Option<String>) -> Result<()>;

    fn re_run_command_in_terminal(
        &self,
        terminal_id: u32,
        run_command: RunCommand,
        quit_cb: Box<dyn Fn(PaneId, Option<i32>, RunCommand) + Send>,
        pane_env: &PaneEnv,
    ) -> Result<(Box<dyn AsyncReader>, Option<u32>)>;
    fn clear_terminal_id(&self, terminal_id: u32) -> Result<()>;
    fn cache_resizes(&mut self) {}
    fn apply_cached_resizes(&mut self) {}
}

impl ServerOsInputOutput {
    fn replace_client_sender(&self, client_id: ClientId, sender: ClientSender) -> Result<()> {
        let previous = self
            .client_senders
            .lock()
            .to_anyhow()
            .with_context(|| format!("failed to create new client {client_id}"))?
            .insert(client_id, sender);
        if let Some(previous) = previous {
            // A new connection under the same id must not leave the old
            // writer behind.
            previous.remove(REMOVED_CLIENT_FLUSH_GRACE);
        }
        Ok(())
    }
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
        pane_env: &PaneEnv,
    ) -> Result<(u32, Box<dyn AsyncReader>, Option<u32>)> {
        let err_context = || "failed to spawn terminal".to_string();

        let terminal_id = self
            .pty_backend
            .next_terminal_id()
            .context("no more terminal IDs left to allocate")?;

        self.pty_backend.reserve_terminal_id(terminal_id);
        if !crate::screen::try_admit_uninstalled_terminal(terminal_id) {
            self.pty_backend.clear_terminal_id(terminal_id);
            return Err(anyhow!("too many panes are still opening"));
        }

        let (cmd, failover_cmd) = build_command(terminal_action, default_editor, pane_env);

        let (async_reader, child_fd) =
            match self
                .pty_backend
                .spawn_terminal(cmd, failover_cmd, quit_cb, terminal_id, pane_env)
            {
                Ok(spawned) => spawned,
                Err(err) => {
                    crate::screen::release_uninstalled_terminal(terminal_id);
                    return Err(err).with_context(err_context);
                },
            };

        Ok((terminal_id, async_reader, Some(child_fd as u32)))
    }
    fn reserve_terminal_id(&self) -> Result<u32> {
        let terminal_id = self
            .pty_backend
            .next_terminal_id()
            .context("no more terminal IDs available")?;
        self.pty_backend.reserve_terminal_id(terminal_id);
        if !crate::screen::try_admit_uninstalled_terminal(terminal_id) {
            self.pty_backend.clear_terminal_id(terminal_id);
            return Err(anyhow!("too many panes are still opening"));
        }
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

    fn register_client(
        &mut self,
        client_id: ClientId,
        receiver: &IpcReceiverWithContext<ClientToServerMsg>,
    ) -> Result<()> {
        let sender = ClientSender::new(client_id, receiver.get_sender());
        self.replace_client_sender(client_id, sender)
    }

    fn register_client_with_reply(
        &mut self,
        client_id: ClientId,
        reply_stream: LocalSocketStream,
    ) -> Result<()> {
        let sender = ClientSender::new(client_id, IpcSenderWithContext::new(reply_stream));
        self.replace_client_sender(client_id, sender)
    }

    fn remove_client(&mut self, client_id: ClientId) -> Result<()> {
        let mut client_senders = self
            .client_senders
            .lock()
            .to_anyhow()
            .with_context(|| format!("failed to remove client {client_id}"))?;
        if let Some(sender) = client_senders.remove(&client_id) {
            sender.remove(REMOVED_CLIENT_FLUSH_GRACE);
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
    #[cfg(not(unix))]
    fn get_all_cmds_by_ppid(&self, post_hook: &Option<String>) -> HashMap<String, Vec<String>> {
        let mut system_info = System::new();
        let refresh_kind = ProcessRefreshKind::nothing().with_cmd(UpdateKind::Always);
        system_info.refresh_processes_specifics(ProcessesToUpdate::All, true, refresh_kind);
        let mut cmds = HashMap::new();
        for (_pid, process) in system_info.processes() {
            if let Some(parent_pid) = process.parent() {
                let ppid_str = format!("{}", parent_pid);
                let command: Vec<String> = process
                    .cmd()
                    .iter()
                    .map(|s| s.to_string_lossy().into_owned())
                    .collect();
                if command.is_empty() {
                    continue;
                }
                cmds.insert(ppid_str, apply_post_command_hook(command, post_hook));
            }
        }
        cmds
    }

    #[cfg(unix)]
    fn get_foreground_cmds(
        &self,
        panes: &[(u32, u32)],
        post_hook: &Option<String>,
    ) -> HashMap<u32, Vec<String>> {
        let mut terminal_to_fg_pid: HashMap<u32, u32> = HashMap::new();
        for &(terminal_id, shell_pid) in panes {
            if let Some(fpgid) = self.pty_backend.tcgetpgrp(terminal_id) {
                if fpgid > 0 && fpgid as u32 != shell_pid {
                    terminal_to_fg_pid.insert(terminal_id, fpgid as u32);
                }
            }
        }
        if terminal_to_fg_pid.is_empty() {
            return HashMap::new();
        }

        let sysinfo_pids: Vec<sysinfo::Pid> = terminal_to_fg_pid
            .values()
            .map(|&p| sysinfo::Pid::from_u32(p))
            .collect();
        let mut system_info = System::new();
        let refresh_kind = ProcessRefreshKind::nothing().with_cmd(UpdateKind::Always);
        system_info.refresh_processes_specifics(
            ProcessesToUpdate::Some(&sysinfo_pids),
            false,
            refresh_kind,
        );

        let mut cmds = HashMap::new();
        for (terminal_id, fg_pid) in terminal_to_fg_pid {
            let Some(process) = system_info.process(sysinfo::Pid::from_u32(fg_pid)) else {
                continue;
            };
            let command: Vec<String> = process
                .cmd()
                .iter()
                .map(|s| s.to_string_lossy().into_owned())
                .collect();
            if command.is_empty() {
                continue;
            }
            let command = apply_post_command_hook(command, post_hook);
            cmds.insert(terminal_id, command);
        }
        cmds
    }

    #[cfg(not(unix))]
    fn get_foreground_cmds(
        &self,
        panes: &[(u32, u32)],
        post_hook: &Option<String>,
    ) -> HashMap<u32, Vec<String>> {
        // Use ppid-based discovery on Windows, because it has no controlling-terminal
        // foreground group.
        let ppids_to_cmds = self.get_all_cmds_by_ppid(post_hook);
        let mut cmds = HashMap::new();
        for &(terminal_id, shell_pid) in panes {
            if let Some(cmd) = ppids_to_cmds.get(&shell_pid.to_string()) {
                cmds.insert(terminal_id, cmd.clone());
            }
        }
        cmds
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
        pane_env: &PaneEnv,
    ) -> Result<(Box<dyn AsyncReader>, Option<u32>)> {
        let (async_reader, child_fd) =
            self.pty_backend
                .spawn_terminal(run_command, None, quit_cb, terminal_id, pane_env)?;
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

fn apply_post_command_hook(command: Vec<String>, post_hook: &Option<String>) -> Vec<String> {
    let Some(post_hook) = post_hook else {
        return command;
    };
    let stringified = command.join(" ");
    let cmd = match run_command_hook(&stringified, post_hook) {
        Ok(command) => command,
        Err(e) => {
            Err::<(), _>(anyhow!("post command discovery hook failed to run: {e}")).non_fatal();
            stringified
        },
    };
    cmd.trim()
        .split_ascii_whitespace()
        .map(|p| p.to_owned())
        .collect()
}

/// The post-command discovery hook runs synchronously on the PTY thread for
/// every foreground command lookup. A hook that hangs, or that leaves a
/// background process holding its output open, must not wedge that thread,
/// and its output must not grow without limit.
const COMMAND_HOOK_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);
const COMMAND_HOOK_MAX_OUTPUT: usize = 64 * 1024;

fn run_command_hook(
    original_command: &str,
    hook_script: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    #[cfg(not(windows))]
    let mut command = {
        let mut command = Command::new("sh");
        command.arg("-c");
        command
    };
    #[cfg(windows)]
    let mut command = {
        let mut command = Command::new("cmd");
        command.arg("/C");
        command
    };
    command
        .arg(hook_script)
        .env("RESURRECT_COMMAND", original_command);
    let output = run_bounded(command, COMMAND_HOOK_TIMEOUT, COMMAND_HOOK_MAX_OUTPUT)?;
    if !output.status.success() {
        return Err(format!("Hook failed: {}", String::from_utf8_lossy(&output.stderr)).into());
    }
    Ok(String::from_utf8(output.stdout)?.trim().to_string())
}

struct BoundedOutput {
    status: std::process::ExitStatus,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

/// Run `command` to completion within `timeout`, keeping at most
/// `max_output` bytes of each output stream.
///
/// On Unix the child runs in its own process group, and the whole group is
/// killed if the child is still running at the deadline. Its output pipes
/// are drained on helper threads so a chatty child cannot block on a full
/// pipe; both streams must also reach end-of-file before the deadline,
/// since a background process that inherited them can keep them open after
/// the child itself exits. Such a process is killed with the group. One
/// that left the group (e.g. with `setsid`) keeps the helper threads alive
/// until it closes the pipes, and the run reports a timeout either way. The
/// number of such threads that exist at once is capped globally; a run that
/// would exceed the cap fails without starting the command.
fn run_bounded(
    command: Command,
    timeout: std::time::Duration,
    max_output: usize,
) -> io::Result<BoundedOutput> {
    run_bounded_with(
        command,
        timeout,
        max_output,
        &HOOK_READER_THREADS,
        &|name, work| {
            std::thread::Builder::new()
                .name(name.to_owned())
                .spawn(work)
                .map(|_| ())
        },
    )
}

/// Upper bound on hook output reader threads that have not finished yet.
///
/// A process that left the hook's process group (e.g. via `setsid`) cannot
/// be killed on timeout and keeps the output pipes, and with them the two
/// reader threads of that run, alive for as long as it likes. Without a
/// bound, a hook that does this on every lookup would leak two threads per
/// lookup. Lookups that would exceed the bound skip the hook instead.
const MAX_OUTSTANDING_HOOK_READER_THREADS: usize = 8;
const HOOK_READER_THREADS_PER_RUN: usize = 2;

static HOOK_READER_THREADS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// One unit of the reader thread budget. Released when dropped, which
/// happens when the thread finishes, or at once if it could not be started.
struct ReaderSlot(&'static std::sync::atomic::AtomicUsize);

impl ReaderSlot {
    fn acquire_many(
        counter: &'static std::sync::atomic::AtomicUsize,
        count: usize,
        max: usize,
    ) -> Option<Vec<ReaderSlot>> {
        use std::sync::atomic::Ordering;
        counter
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
                (current + count <= max).then_some(current + count)
            })
            .ok()?;
        Some((0..count).map(|_| ReaderSlot(counter)).collect())
    }
}

impl Drop for ReaderSlot {
    fn drop(&mut self) {
        self.0.fetch_sub(1, std::sync::atomic::Ordering::AcqRel);
    }
}

type ReaderSpawner<'a> = &'a dyn Fn(&str, Box<dyn FnOnce() + Send + 'static>) -> io::Result<()>;

fn run_bounded_with(
    mut command: Command,
    timeout: std::time::Duration,
    max_output: usize,
    reader_threads: &'static std::sync::atomic::AtomicUsize,
    spawn_reader: ReaderSpawner,
) -> io::Result<BoundedOutput> {
    use std::io::Read;
    use std::process::Stdio;
    use std::sync::mpsc;
    use std::time::Instant;

    let deadline = Instant::now() + timeout;
    let mut slots = ReaderSlot::acquire_many(
        reader_threads,
        HOOK_READER_THREADS_PER_RUN,
        MAX_OUTSTANDING_HOOK_READER_THREADS,
    )
    .ok_or_else(|| {
        log::warn!(
            "Skipping command hook: {} of its output reader threads are still blocked",
            MAX_OUTSTANDING_HOOK_READER_THREADS
        );
        io::Error::new(
            io::ErrorKind::WouldBlock,
            "too many earlier hook runs still hold output pipes open",
        )
    })?;
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(&mut command, 0);
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let (output_tx, output_rx) = mpsc::channel::<(bool, Vec<u8>)>();
    let mut drain = |mut stream: Box<dyn Read + Send>, is_stdout: bool| -> io::Result<()> {
        let output_tx = output_tx.clone();
        let slot = slots.pop();
        spawn_reader(
            "command-hook-reader",
            Box::new(move || {
                let _slot = slot;
                let mut retained = Vec::new();
                let mut chunk = [0u8; 8192];
                loop {
                    match stream.read(&mut chunk) {
                        Ok(0) => break,
                        Ok(n) => {
                            let room = max_output.saturating_sub(retained.len());
                            retained.extend_from_slice(&chunk[..n.min(room)]);
                        },
                        Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                        Err(_) => break,
                    }
                }
                let _ = output_tx.send((is_stdout, retained));
            }),
        )
    };
    let started = match child.stdout.take() {
        Some(stdout) => drain(Box::new(stdout), true),
        None => Ok(()),
    }
    .and_then(|_| match child.stderr.take() {
        Some(stderr) => drain(Box::new(stderr), false),
        None => Ok(()),
    });
    drop(output_tx);
    if let Err(e) = started {
        // Without both readers the output cannot be collected, and a child
        // that fills a pipe nobody reads would hang until the deadline.
        kill_process_group(&mut child);
        let _ = child.wait();
        return Err(e);
    }

    let timed_out = || io::Error::new(io::ErrorKind::TimedOut, "command timed out");
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if Instant::now() >= deadline {
            kill_process_group(&mut child);
            let _ = child.wait();
            return Err(timed_out());
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    };

    let (mut stdout, mut stderr) = (None, None);
    while stdout.is_none() || stderr.is_none() {
        let remaining = deadline.saturating_duration_since(Instant::now());
        match output_rx.recv_timeout(remaining) {
            Ok((true, bytes)) => stdout = Some(bytes),
            Ok((false, bytes)) => stderr = Some(bytes),
            Err(mpsc::RecvTimeoutError::Timeout) => {
                // The child has exited, but processes it left behind still
                // hold its output open; the group outlives its leader. (If
                // all of them left the group, its id is unused and could only
                // have been reused if the pid space wrapped within the
                // deadline.)
                kill_process_group(&mut child);
                return Err(timed_out());
            },
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    Ok(BoundedOutput {
        status,
        stdout: stdout.unwrap_or_default(),
        stderr: stderr.unwrap_or_default(),
    })
}

/// Kill `child` together with any processes it started in its group.
fn kill_process_group(child: &mut std::process::Child) {
    #[cfg(unix)]
    {
        use nix::sys::signal::{killpg, Signal};
        use nix::unistd::Pid;
        if let Ok(pgid) = i32::try_from(child.id()) {
            let _ = killpg(Pid::from_raw(pgid), Signal::SIGKILL);
        }
    }
    let _ = child.kill();
}

#[cfg(test)]
#[path = "./unit/os_input_output_tests.rs"]
mod os_input_output_tests;
