use super::RemoteTransport;
use crate::RemoteClientError;
use std::collections::VecDeque;
use std::io::{self, Read};
use std::net::TcpListener;
use std::net::{SocketAddr, TcpStream};
use std::path::Path;
#[cfg(unix)]
use std::path::PathBuf;
use std::process::{Child, ChildStderr, Command, Stdio};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use url::Url;

#[cfg(unix)]
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
#[cfg(unix)]
use std::os::unix::net::UnixStream;
#[cfg(target_os = "linux")]
use std::os::unix::process::CommandExt;
#[cfg(unix)]
use zellij_utils::consts::ZELLIJ_SOCK_MAX_LENGTH;

const DEFAULT_SSH_PORT: u16 = 22;
const DEFAULT_REMOTE_WEB_PORT: u16 = 8082;
const TUNNEL_STARTUP_TIMEOUT: Duration = Duration::from_secs(30);

/// The SSH and remote web-server parameters encoded by an `ssh://` session URL.
///
/// The URL's port is the SSH port. The remote Zellij web-server port can be
/// selected with the `web_port` query parameter, for example:
/// `ssh://user@example.com:2222/my-session?web_port=9000`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SshRemoteSession {
    pub(crate) host: String,
    pub(crate) destination: String,
    pub(crate) ssh_port: Option<u16>,
    pub(crate) remote_web_port: u16,
    pub(crate) web_scheme: String,
    pub(crate) session_name: String,
    pub(crate) token_cache_key: String,
}

impl SshRemoteSession {
    pub(crate) fn parse(remote_session_url: &str) -> Result<Self, RemoteClientError> {
        let parsed = Url::parse(remote_session_url)?;
        if !parsed.scheme().eq_ignore_ascii_case("ssh") {
            return Err(RemoteClientError::InvalidSshUrl(format!(
                "expected an ssh:// URL, got {}://",
                parsed.scheme()
            )));
        }

        if parsed.password().is_some() {
            return Err(RemoteClientError::InvalidSshUrl(
                "passwords must not be included in an ssh:// URL; use your SSH configuration"
                    .to_owned(),
            ));
        }

        let host = parsed
            .host_str()
            .ok_or_else(|| RemoteClientError::InvalidSshUrl("SSH URL has no host".to_owned()))?;
        if host.is_empty() || host.contains('\0') {
            return Err(RemoteClientError::InvalidSshUrl(
                "SSH URL has an invalid host".to_owned(),
            ));
        }

        let username = if parsed.username().is_empty() {
            None
        } else {
            let username = urlencoding::decode(parsed.username()).map_err(|_| {
                RemoteClientError::InvalidSshUrl("SSH URL has an invalid username".to_owned())
            })?;
            if username.is_empty() || username.contains('\0') {
                return Err(RemoteClientError::InvalidSshUrl(
                    "SSH URL has an invalid username".to_owned(),
                ));
            }
            Some(username.into_owned())
        };

        // OpenSSH expects a bare IPv6 address as its destination. Brackets are
        // URL syntax and are treated as part of the hostname by ssh.
        let host_for_ssh = host
            .strip_prefix('[')
            .and_then(|host| host.strip_suffix(']'))
            .unwrap_or(host);
        let destination = match username {
            Some(username) => format!("{username}@{host_for_ssh}"),
            None => host_for_ssh.to_owned(),
        };

        let mut remote_web_port = DEFAULT_REMOTE_WEB_PORT;
        let mut web_scheme = "http".to_owned();
        let mut web_port_seen = false;
        let mut web_scheme_seen = false;
        for (key, value) in parsed.query_pairs() {
            match key.as_ref() {
                "web_port" if !web_port_seen => {
                    remote_web_port = value.parse().map_err(|_| {
                        RemoteClientError::InvalidSshUrl(format!(
                            "invalid remote web-server port: {value}"
                        ))
                    })?;
                    if remote_web_port == 0 {
                        return Err(RemoteClientError::InvalidSshUrl(
                            "remote web-server port must not be zero".to_owned(),
                        ));
                    }
                    web_port_seen = true;
                },
                "web_port" => {
                    return Err(RemoteClientError::InvalidSshUrl(
                        "remote web-server port was specified more than once".to_owned(),
                    ));
                },
                "web_scheme" if !web_scheme_seen => {
                    if !value.eq_ignore_ascii_case("http") && !value.eq_ignore_ascii_case("https") {
                        return Err(RemoteClientError::InvalidSshUrl(format!(
                            "unsupported remote web-server scheme: {value}"
                        )));
                    }
                    web_scheme = value.to_ascii_lowercase();
                    web_scheme_seen = true;
                },
                "web_scheme" => {
                    return Err(RemoteClientError::InvalidSshUrl(
                        "remote web-server scheme was specified more than once".to_owned(),
                    ));
                },
                unknown => {
                    return Err(RemoteClientError::InvalidSshUrl(format!(
                        "unsupported ssh:// URL parameter: {unknown}"
                    )));
                },
            }
        }

        if parsed.fragment().is_some() {
            return Err(RemoteClientError::InvalidSshUrl(
                "fragments are not supported in ssh:// session URLs".to_owned(),
            ));
        }

        let encoded_session_name = parsed
            .path()
            .strip_prefix('/')
            .unwrap_or(parsed.path())
            .trim_end_matches('/');
        let session_name = urlencoding::decode(encoded_session_name)
            .map_err(|_| {
                RemoteClientError::InvalidSshUrl(
                    "SSH URL session name is not valid UTF-8".to_owned(),
                )
            })?
            .trim_end_matches('/')
            .to_owned();
        let ssh_port = parsed.port();
        if ssh_port == Some(0) {
            return Err(RemoteClientError::InvalidSshUrl(
                "SSH port must not be zero".to_owned(),
            ));
        }
        let token_cache_key = format!(
            "ssh://{}?ssh_port={}&web_port={}&web_scheme={}",
            urlencoding::encode(&destination),
            ssh_port.unwrap_or(DEFAULT_SSH_PORT),
            remote_web_port,
            web_scheme
        );

        Ok(Self {
            host: host_for_ssh.to_owned(),
            destination,
            ssh_port,
            remote_web_port,
            web_scheme,
            session_name,
            token_cache_key,
        })
    }

    pub(crate) fn remote_server_url(&self) -> String {
        let host = if self.host.contains(':') && !self.host.starts_with('[') {
            format!("[{}]", self.host)
        } else {
            self.host.clone()
        };
        format!("{}://{host}:{}", self.web_scheme, self.remote_web_port)
    }
}

/// A supervised OpenSSH local forward.
///
/// On Unix, the forward is exposed through a private Unix socket; other
/// platforms use a loopback TCP port. Either endpoint carries the existing
/// HTTP authentication flow and both remote-client WebSockets. Keeping this
/// process alive for the lifetime of the returned value gives one SSH
/// transport to a remote Zellij web server, regardless of how many remote
/// panes or tabs are open. The TCP fallback is reachable by other local
/// processes, so it is used only when a private Unix socket is unavailable.
/// Linux terminates the child when its parent dies; other platforms rely on
/// the normal tunnel cleanup paths.
#[derive(Debug)]
pub(crate) struct SshTunnel {
    child: Child,
    local_forward: RemoteTransport,
    #[cfg(unix)]
    socket_directory: Option<PathBuf>,
    stderr_thread: Option<JoinHandle<()>>,
}

impl SshTunnel {
    pub(crate) fn connect(remote_session: &SshRemoteSession) -> Result<Self, RemoteClientError> {
        Self::connect_with_ssh_binary(remote_session, Path::new("ssh"))
    }

    fn connect_with_ssh_binary(
        remote_session: &SshRemoteSession,
        ssh_binary: &Path,
    ) -> Result<Self, RemoteClientError> {
        #[cfg(unix)]
        let (socket_directory, local_forward) = match create_private_socket_path() {
            Ok((socket_directory, local_socket_path)) => (
                Some(socket_directory),
                RemoteTransport::Unix(local_socket_path),
            ),
            Err(error) => {
                log::warn!(
                    "unable to create a private Unix socket for the SSH tunnel; falling back to a loopback TCP port: {error}"
                );
                (None, RemoteTransport::Tcp(find_free_local_port()?))
            },
        };
        #[cfg(not(unix))]
        let local_forward = RemoteTransport::Tcp(find_free_local_port()?);

        let mut command = Command::new(ssh_binary);
        command
            .arg("-N")
            .arg("-T")
            .arg("-n")
            .arg("-o")
            .arg("ControlMaster=no")
            .arg("-o")
            .arg("ControlPath=none")
            .arg("-o")
            .arg("ExitOnForwardFailure=yes")
            .arg("-o")
            .arg("BatchMode=yes")
            .arg("-o")
            .arg("ForwardAgent=no")
            .arg("-o")
            .arg("PermitLocalCommand=no")
            .arg("-o")
            .arg("ServerAliveInterval=15")
            .arg("-o")
            .arg("ServerAliveCountMax=3");

        if matches!(&local_forward, RemoteTransport::Tcp(_)) {
            command.arg("-v");
        }

        if let Some(ssh_port) = remote_session.ssh_port {
            command.arg("-p").arg(ssh_port.to_string());
        }

        command
            .arg("-L")
            .arg(local_forward_spec(
                &local_forward,
                remote_session.remote_web_port,
            ))
            .arg("--")
            .arg(&remote_session.destination)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped());

        #[cfg(target_os = "linux")]
        {
            // Do not leave an unauthenticated local forward behind if the
            // client is terminated before its normal Drop path runs.
            let parent_pid = std::process::id();
            unsafe {
                command.pre_exec(move || {
                    if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM) == -1 {
                        return Err(io::Error::last_os_error());
                    }
                    if libc::getppid() as u32 != parent_pid {
                        libc::kill(libc::getpid(), libc::SIGTERM);
                    }
                    Ok(())
                });
            }
        }

        let mut child = command.spawn().map_err(|error| {
            #[cfg(unix)]
            if let Some(socket_directory) = &socket_directory {
                let _ = std::fs::remove_dir_all(socket_directory);
            }
            RemoteClientError::ConnectionFailed(format!(
                "failed to start SSH tunnel using `ssh`: {error}"
            ))
        })?;

        let stderr = match child.stderr.take() {
            Some(stderr) => stderr,
            None => {
                terminate_child(&mut child, None);
                #[cfg(unix)]
                if let Some(socket_directory) = &socket_directory {
                    let _ = std::fs::remove_dir_all(socket_directory);
                }
                return Err(RemoteClientError::ConnectionFailed(
                    "SSH tunnel did not provide a stderr stream".to_owned(),
                ));
            },
        };
        let diagnostics = Arc::new(Mutex::new(VecDeque::with_capacity(8)));
        let ssh_reported_ready = Arc::new(AtomicBool::new(false));
        let monitor_diagnostics = diagnostics.clone();
        let monitor_forward = local_forward.clone();
        let monitor_ssh_reported_ready = ssh_reported_ready.clone();
        let stderr_thread = thread::spawn(move || {
            monitor_ssh_stderr(
                stderr,
                monitor_forward,
                monitor_ssh_reported_ready,
                monitor_diagnostics,
            )
        });

        let startup_result = wait_for_tunnel(
            &mut child,
            &local_forward,
            &ssh_reported_ready,
            &diagnostics,
        );
        if let Err(error) = startup_result {
            terminate_child(&mut child, Some(stderr_thread));
            #[cfg(unix)]
            if let Some(socket_directory) = &socket_directory {
                let _ = std::fs::remove_dir_all(socket_directory);
            }
            return Err(error);
        }

        Ok(Self {
            child,
            local_forward,
            #[cfg(unix)]
            socket_directory,
            stderr_thread: Some(stderr_thread),
        })
    }

    pub(crate) fn transport(&self) -> &RemoteTransport {
        &self.local_forward
    }
}

impl Drop for SshTunnel {
    fn drop(&mut self) {
        terminate_child(&mut self.child, self.stderr_thread.take());
        #[cfg(unix)]
        if let Some(socket_directory) = self.socket_directory.take() {
            let _ = std::fs::remove_dir_all(socket_directory);
        }
    }
}

fn local_forward_spec(local_forward: &RemoteTransport, remote_web_port: u16) -> String {
    match local_forward {
        RemoteTransport::Tcp(local_port) => {
            format!("127.0.0.1:{local_port}:127.0.0.1:{remote_web_port}")
        },
        #[cfg(unix)]
        RemoteTransport::Unix(path) => {
            format!("{}:127.0.0.1:{remote_web_port}", path.display())
        },
    }
}

#[cfg(unix)]
fn create_private_socket_path() -> Result<(PathBuf, PathBuf), RemoteClientError> {
    let identifier = uuid::Uuid::new_v4().simple().to_string();
    let socket_directory = std::env::temp_dir().join(format!("zellij-ssh-{}", &identifier[..16]));
    let mut directory_builder = std::fs::DirBuilder::new();
    directory_builder.mode(0o700);
    directory_builder.create(&socket_directory)?;
    if let Err(error) =
        std::fs::set_permissions(&socket_directory, std::fs::Permissions::from_mode(0o700))
    {
        let _ = std::fs::remove_dir_all(&socket_directory);
        return Err(RemoteClientError::IoError(error));
    }
    let socket_path = socket_directory.join("forward.sock");
    let Some(socket_path_string) = socket_path.to_str() else {
        let _ = std::fs::remove_dir_all(&socket_directory);
        return Err(RemoteClientError::ConnectionFailed(
            "SSH tunnel socket path is not valid UTF-8; use a UTF-8 temporary directory".to_owned(),
        ));
    };
    if socket_path_string.contains(':') || socket_path_string.contains('\n') {
        let _ = std::fs::remove_dir_all(&socket_directory);
        return Err(RemoteClientError::ConnectionFailed(
            "SSH tunnel socket path contains a character unsupported by OpenSSH forwarding"
                .to_owned(),
        ));
    }
    if socket_path.as_os_str().len() >= ZELLIJ_SOCK_MAX_LENGTH {
        let _ = std::fs::remove_dir_all(&socket_directory);
        return Err(RemoteClientError::ConnectionFailed(format!(
            "SSH tunnel socket path is too long (maximum is {} bytes)",
            ZELLIJ_SOCK_MAX_LENGTH - 1
        )));
    }
    Ok((socket_directory, socket_path))
}

fn find_free_local_port() -> Result<u16, RemoteClientError> {
    let listener = TcpListener::bind(("127.0.0.1", 0))?;
    Ok(listener.local_addr()?.port())
}

fn monitor_ssh_stderr(
    stderr: ChildStderr,
    local_forward: RemoteTransport,
    ssh_reported_ready: Arc<AtomicBool>,
    diagnostics: Arc<Mutex<VecDeque<String>>>,
) {
    let mut stderr = stderr;
    let mut output = String::new();
    let mut diagnostic_line = String::new();
    let mut pending_utf8 = Vec::new();
    let mut read_buffer = [0; 1024];
    loop {
        let bytes_read = match stderr.read(&mut read_buffer) {
            Ok(bytes_read) => bytes_read,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => {
                remember_diagnostic(
                    &diagnostics,
                    &format!("failed to read SSH tunnel status: {error}"),
                );
                return;
            },
        };
        if bytes_read == 0 {
            break;
        }

        let chunk = sanitize_ssh_output(&decode_utf8_chunk(
            &mut pending_utf8,
            &read_buffer[..bytes_read],
        ));
        output.push_str(&chunk);
        trim_output_buffer(&mut output);
        if !ssh_reported_ready.load(Ordering::Acquire)
            && output
                .lines()
                .any(|line| is_tunnel_ready_line(line, &local_forward))
        {
            ssh_reported_ready.store(true, Ordering::Release);
        }
        remember_stderr_lines(&diagnostics, &mut diagnostic_line, &chunk);
        log::debug!("ssh: {}", chunk.trim_end());
    }
    if !pending_utf8.is_empty() {
        let chunk = sanitize_ssh_output(&String::from_utf8_lossy(&pending_utf8));
        remember_stderr_lines(&diagnostics, &mut diagnostic_line, &chunk);
    }
    if !diagnostic_line.is_empty() {
        remember_diagnostic(&diagnostics, &diagnostic_line);
    }
}

fn decode_utf8_chunk(pending: &mut Vec<u8>, bytes: &[u8]) -> String {
    pending.extend_from_slice(bytes);
    let complete_bytes = match std::str::from_utf8(pending) {
        Ok(_) => pending.len(),
        Err(error) if error.error_len().is_none() => error.valid_up_to(),
        Err(_) => pending.len(),
    };
    let output = String::from_utf8_lossy(&pending[..complete_bytes]).into_owned();
    pending.drain(..complete_bytes);
    output
}

fn trim_output_buffer(output: &mut String) {
    if output.len() > 8192 {
        let target = output.len() - 4096;
        let trim_from = output
            .char_indices()
            .find_map(|(index, _)| (index >= target).then_some(index))
            .unwrap_or(0);
        output.drain(..trim_from);
    }
}

fn remember_stderr_lines(
    diagnostics: &Arc<Mutex<VecDeque<String>>>,
    pending_line: &mut String,
    chunk: &str,
) {
    pending_line.push_str(chunk);
    while let Some(newline) = pending_line.find('\n') {
        let line = pending_line[..newline].trim_end_matches('\r');
        remember_diagnostic(diagnostics, line);
        pending_line.drain(..newline + 1);
    }
    trim_output_buffer(pending_line);
}

fn remember_diagnostic(diagnostics: &Arc<Mutex<VecDeque<String>>>, line: &str) {
    let line = sanitize_ssh_output(line);
    if line.trim_start().starts_with("debug") || line.trim().is_empty() {
        return;
    }
    if let Ok(mut diagnostics) = diagnostics.lock() {
        if diagnostics.len() == 8 {
            diagnostics.pop_front();
        }
        diagnostics.push_back(line);
    }
}

fn sanitize_ssh_output(output: &str) -> String {
    output
        .chars()
        .filter(|character| {
            matches!(character, '\n' | '\t')
                || (!character.is_control()
                    && !matches!(
                        character,
                        '\u{061c}'
                            | '\u{200b}'..='\u{200f}'
                            | '\u{202a}'..='\u{202e}'
                            | '\u{2060}'..='\u{206f}'
                            | '\u{feff}'
                    ))
        })
        .collect()
}

fn diagnostics_suffix(diagnostics: &Arc<Mutex<VecDeque<String>>>) -> String {
    diagnostics
        .lock()
        .ok()
        .filter(|diagnostics| !diagnostics.is_empty())
        .map(|diagnostics| {
            format!(
                ": {}",
                diagnostics.iter().cloned().collect::<Vec<_>>().join(" | ")
            )
        })
        .unwrap_or_default()
}

fn is_tunnel_ready_line(line: &str, local_forward: &RemoteTransport) -> bool {
    let line = line.trim().trim_end_matches('\r').trim_end_matches('.');
    let Some((_, line)) = line.split_once("Local forwarding listening on ") else {
        return false;
    };

    match local_forward {
        RemoteTransport::Tcp(local_port) => {
            let expected_port = local_port.to_string();
            let mut words = line.split_whitespace();
            while let Some(word) = words.next() {
                if word == "port" && words.next() == Some(expected_port.as_str()) {
                    return true;
                }
            }
            false
        },
        #[cfg(unix)]
        RemoteTransport::Unix(_) => false,
    }
}

fn wait_for_tunnel(
    child: &mut Child,
    local_forward: &RemoteTransport,
    ssh_reported_ready: &AtomicBool,
    diagnostics: &Arc<Mutex<VecDeque<String>>>,
) -> Result<(), RemoteClientError> {
    let deadline = Instant::now() + TUNNEL_STARTUP_TIMEOUT;
    let requires_ssh_report = matches!(local_forward, RemoteTransport::Tcp(_));
    let mut poll_delay = Duration::from_millis(20);

    loop {
        if let Some(status) = child.try_wait()? {
            return Err(RemoteClientError::ConnectionFailed(format!(
                "SSH tunnel exited before it was ready ({status}){}",
                diagnostics_suffix(diagnostics)
            )));
        }

        // A TCP port is allocated before SSH starts and can be claimed by a
        // different process in between. Require SSH's own local-listener
        // confirmation for TCP so that process cannot receive credentials.
        if (!requires_ssh_report || ssh_reported_ready.load(Ordering::Acquire))
            && local_forward_is_reachable(local_forward)
            && child.try_wait()?.is_none()
        {
            return Ok(());
        }

        thread::sleep(poll_delay);
        poll_delay = (poll_delay * 2).min(Duration::from_millis(250));

        if Instant::now() >= deadline {
            return Err(RemoteClientError::ConnectionFailed(format!(
                "timed out waiting for the SSH tunnel to start{}",
                diagnostics_suffix(diagnostics)
            )));
        }
    }
}

fn local_forward_is_reachable(local_forward: &RemoteTransport) -> bool {
    match local_forward {
        RemoteTransport::Tcp(local_port) => TcpStream::connect_timeout(
            &SocketAddr::from(([127, 0, 0, 1], *local_port)),
            Duration::from_millis(50),
        )
        .is_ok(),
        #[cfg(unix)]
        RemoteTransport::Unix(path) => UnixStream::connect(path).is_ok(),
    }
}

fn terminate_child(child: &mut Child, stderr_thread: Option<JoinHandle<()>>) {
    let _ = child.kill();
    let _ = child.wait();
    if let Some(stderr_thread) =
        stderr_thread.filter(|thread| thread.thread().id() != thread::current().id())
    {
        let _ = stderr_thread.join();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ssh_destination_and_remote_web_port() {
        let session =
            SshRemoteSession::parse("ssh://alice@example.com:2222/my-session?web_port=9000")
                .unwrap();

        assert_eq!(session.host, "example.com");
        assert_eq!(session.destination, "alice@example.com");
        assert_eq!(session.ssh_port, Some(2222));
        assert_eq!(session.remote_web_port, 9000);
        assert_eq!(session.web_scheme, "http");
        assert_eq!(session.session_name, "my-session");
        assert_eq!(session.remote_server_url(), "http://example.com:9000");
        assert_eq!(
            session.token_cache_key,
            "ssh://alice%40example.com?ssh_port=2222&web_port=9000&web_scheme=http"
        );
    }

    #[test]
    fn parses_ipv6_ssh_destination() {
        let session = SshRemoteSession::parse("ssh://[::1]/session").unwrap();

        assert_eq!(session.host, "::1");
        assert_eq!(session.destination, "::1");
        assert_eq!(session.ssh_port, None);
        assert_eq!(session.session_name, "session");
    }

    #[test]
    fn decodes_ssh_session_name_once() {
        let session = SshRemoteSession::parse("ssh://example.com/my%20session").unwrap();

        assert_eq!(session.session_name, "my session");
    }

    #[test]
    fn escapes_destination_in_token_cache_key() {
        let session = SshRemoteSession::parse("ssh://user%3Fname@example.com/session").unwrap();

        assert_eq!(session.destination, "user?name@example.com");
        assert!(!session.token_cache_key.contains("user?name"));
    }

    #[test]
    fn normalizes_remote_web_scheme() {
        let session =
            SshRemoteSession::parse("ssh://example.com/session?web_scheme=HTTPS").unwrap();

        assert_eq!(session.web_scheme, "https");
    }

    #[test]
    fn formats_ipv6_server_url() {
        let session = SshRemoteSession::parse("ssh://[2001:db8::1]/session").unwrap();

        assert_eq!(session.remote_server_url(), "http://[2001:db8::1]:8082");
    }

    #[test]
    fn strips_terminal_control_sequences_from_ssh_output() {
        assert_eq!(
            sanitize_ssh_output("warning\u{1b}[31m: bad\u{7}\r\u{202e}spoof"),
            "warning[31m: badspoof"
        );
    }

    #[test]
    fn trims_diagnostic_buffer_at_a_utf8_boundary() {
        let mut output = "€".repeat(5000);
        trim_output_buffer(&mut output);

        assert!(output.len() <= 4098);
        assert!(output.is_char_boundary(0));
    }

    #[test]
    fn preserves_utf8_codepoints_across_stderr_reads() {
        let mut pending = Vec::new();
        let euro = "€".as_bytes();

        assert_eq!(decode_utf8_chunk(&mut pending, &euro[..1]), "");
        assert_eq!(decode_utf8_chunk(&mut pending, &euro[1..]), "€");
    }

    #[test]
    fn recognizes_forwarding_readiness_without_a_specific_debug_level() {
        let local_forward = RemoteTransport::Tcp(43123);

        assert!(is_tunnel_ready_line(
            "debug2: Local forwarding listening on 127.0.0.1 port 43123",
            &local_forward
        ));
        assert!(!is_tunnel_ready_line(
            "debug2: Local forwarding listening on 127.0.0.1 port 43124",
            &local_forward
        ));
    }

    #[cfg(unix)]
    #[test]
    fn formats_unix_socket_forwarding_spec() {
        let path = PathBuf::from("/tmp/zellij-ssh-test/forward.sock");
        let local_forward = RemoteTransport::Unix(path);
        assert_eq!(
            local_forward_spec(&local_forward, 9000),
            "/tmp/zellij-ssh-test/forward.sock:127.0.0.1:9000"
        );
    }

    #[cfg(unix)]
    #[test]
    fn creates_private_socket_directory() {
        use std::os::unix::fs::PermissionsExt;

        let (directory, socket_path) = create_private_socket_path().unwrap();
        let mode = std::fs::metadata(&directory).unwrap().permissions().mode() & 0o777;

        assert_eq!(socket_path, directory.join("forward.sock"));
        assert!(socket_path.as_os_str().len() < ZELLIJ_SOCK_MAX_LENGTH);
        assert_eq!(mode, 0o700);

        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn rejects_passwords_and_unknown_parameters() {
        assert!(matches!(
            SshRemoteSession::parse("ssh://alice:secret@example.com/session"),
            Err(RemoteClientError::InvalidSshUrl(_))
        ));
        assert!(matches!(
            SshRemoteSession::parse("ssh://example.com/session?unknown=value"),
            Err(RemoteClientError::InvalidSshUrl(_))
        ));
    }

    #[test]
    fn rejects_ambiguous_or_invalid_parameters() {
        for url in [
            "ssh://example.com/session?web_port=0",
            "ssh://example.com:0/session",
            "ssh://example.com/session#fragment",
            "ssh://example.com/session?web_port=9000&web_port=9001",
            "ssh://example.com/session?web_scheme=http&web_scheme=https",
        ] {
            assert!(matches!(
                SshRemoteSession::parse(url),
                Err(RemoteClientError::InvalidSshUrl(_)),
            ));
        }
    }

    #[test]
    fn rejects_non_ssh_urls() {
        assert!(matches!(
            SshRemoteSession::parse("https://example.com/session"),
            Err(RemoteClientError::InvalidSshUrl(_))
        ));
    }

    #[cfg(unix)]
    #[test]
    fn reports_ssh_startup_failure() {
        use std::os::unix::fs::PermissionsExt;

        let fake_ssh_directory = tempfile::tempdir().unwrap();
        let fake_ssh = fake_ssh_directory.path().join("ssh");
        std::fs::write(
            &fake_ssh,
            "#!/bin/sh\nprintf 'startup failed\\n' >&2\nexit 1\n",
        )
        .unwrap();
        std::fs::set_permissions(&fake_ssh, std::fs::Permissions::from_mode(0o700)).unwrap();

        let session = SshRemoteSession::parse("ssh://example.com/session").unwrap();
        let result = SshTunnel::connect_with_ssh_binary(&session, &fake_ssh);

        assert!(matches!(
            result,
            Err(RemoteClientError::ConnectionFailed(message))
                if message.contains("SSH tunnel exited before it was ready")
        ));
    }

    #[cfg(unix)]
    #[test]
    fn drop_removes_socket_directory() {
        let (socket_directory, socket_path) = create_private_socket_path().unwrap();
        let child = Command::new("sleep")
            .arg("60")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let tunnel = SshTunnel {
            child,
            local_forward: RemoteTransport::Unix(socket_path),
            socket_directory: Some(socket_directory.clone()),
            stderr_thread: None,
        };

        drop(tunnel);

        assert!(!socket_directory.exists());
    }
}
