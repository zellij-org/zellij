use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Result};
use zellij_utils::consts::{ipc_connect, ZELLIJ_SOCK_DIR, ZELLIJ_SOCK_MAX_LENGTH};
use zellij_utils::data::{BareKey, KeyWithModifier};
use zellij_utils::ipc::{
    ClientToServerMsg, IpcReceiveError, IpcReceiverWithContext, IpcSenderWithContext,
    PixelDimensions, ServerToClientMsg,
};
use zellij_utils::pane_size::{Size, SizeInPixels};

use crate::color::Paints;
use crate::palette;

const CONNECT_RETRY_INTERVAL: Duration = Duration::from_millis(50);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(2);
const STARTUP_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, Copy)]
pub struct Geometry {
    pub rows: usize,
    pub cols: usize,
    pub cell_width: usize,
    pub cell_height: usize,
}

impl Geometry {
    pub fn size(&self) -> Size {
        Size {
            rows: self.rows,
            cols: self.cols,
        }
    }

    pub fn pixel_dimensions(&self) -> PixelDimensions {
        PixelDimensions {
            text_area_size: Some(SizeInPixels {
                width: self.cols * self.cell_width,
                height: self.rows * self.cell_height,
            }),
            character_cell_size: Some(SizeInPixels {
                width: self.cell_width,
                height: self.cell_height,
            }),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Capabilities {
    pub local_media: bool,
    pub paints: Paints,
}

impl Default for Capabilities {
    fn default() -> Self {
        Self {
            local_media: true,
            paints: Paints::default(),
        }
    }
}

pub trait MessageSink: Send + Sync {
    fn send_msg(&self, msg: ClientToServerMsg) -> Result<()>;
}

pub trait MessageSource: Send {
    fn recv_msg(&mut self) -> std::result::Result<ServerToClientMsg, IpcReceiveError>;
}

pub type SharedSender = Arc<dyn MessageSink>;

struct IpcSink(Mutex<IpcSenderWithContext<ClientToServerMsg>>);

impl MessageSink for IpcSink {
    fn send_msg(&self, msg: ClientToServerMsg) -> Result<()> {
        let mut sender = self
            .0
            .lock()
            .map_err(|_| anyhow!("sender mutex was poisoned"))?;
        sender
            .send_client_msg(msg)
            .map_err(|e| anyhow!("failed to send message to server: {:?}", e))
    }
}

impl MessageSource for IpcReceiverWithContext<ServerToClientMsg> {
    fn recv_msg(&mut self) -> std::result::Result<ServerToClientMsg, IpcReceiveError> {
        self.try_recv_server_msg().map(|(msg, _context)| msg)
    }
}

#[derive(Clone)]
pub struct GeometryHandle(Arc<Mutex<Geometry>>);

impl GeometryHandle {
    pub fn new(geometry: Geometry) -> Self {
        Self(Arc::new(Mutex::new(geometry)))
    }

    pub fn get(&self) -> Geometry {
        *self.0.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn replace(&self, next: Geometry) -> Geometry {
        let mut current = self.0.lock().unwrap_or_else(|e| e.into_inner());
        std::mem::replace(&mut current, next)
    }
}

pub struct Connection {
    pub sender: SharedSender,
    pub receiver: Box<dyn MessageSource>,
    pub session_name: String,
    pub geometry: GeometryHandle,
    pub role: Role,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Participant,
    Watcher,
}

pub fn socket_path(session_name: &str) -> Result<PathBuf> {
    let path = ZELLIJ_SOCK_DIR.join(session_name);
    let length = path.as_os_str().len();
    if length >= ZELLIJ_SOCK_MAX_LENGTH {
        bail!(
            "socket path {:?} is {} bytes, exceeding the {} byte limit",
            path,
            length,
            ZELLIJ_SOCK_MAX_LENGTH
        );
    }
    Ok(path)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Opening {
    Participant(Capabilities),
    Watcher,
}

impl Opening {
    fn role(&self) -> Role {
        match self {
            Opening::Participant(_) => Role::Participant,
            Opening::Watcher => Role::Watcher,
        }
    }

    fn connect_timeout(&self, starts_server: bool) -> Duration {
        if starts_server {
            STARTUP_CONNECT_TIMEOUT
        } else {
            match self {
                Opening::Participant(_) => CONNECT_TIMEOUT,
                Opening::Watcher => CONNECT_TIMEOUT,
            }
        }
    }
}

pub fn open(
    session_name: &str,
    geometry: Geometry,
    opening: Opening,
    first_message: ClientToServerMsg,
    starts_server: bool,
) -> Result<Connection> {
    let path = socket_path(session_name)?;
    open_at(
        &path,
        session_name,
        geometry,
        opening,
        first_message,
        starts_server,
    )
}

pub fn open_at(
    path: &Path,
    session_name: &str,
    geometry: Geometry,
    opening: Opening,
    first_message: ClientToServerMsg,
    starts_server: bool,
) -> Result<Connection> {
    let deadline = Instant::now() + opening.connect_timeout(starts_server);
    let mut sender = connect_with_retry(path, deadline)?;
    let receiver = reply_channel(&sender, path, deadline)?;

    sender
        .send_client_msg(first_message)
        .map_err(|e| anyhow!("failed to send the handshake message: {:?}", e))?;

    let declarations = match opening {
        Opening::Participant(capabilities) => asserted_capabilities(geometry, capabilities),
        Opening::Watcher => vec![ClientToServerMsg::StructuredRenderSupport { supported: true }],
    };
    for msg in declarations {
        sender
            .send_client_msg(msg)
            .map_err(|e| anyhow!("failed to send capability message: {:?}", e))?;
    }

    Ok(Connection {
        sender: Arc::new(IpcSink(Mutex::new(sender))),
        receiver: Box::new(receiver),
        session_name: session_name.to_owned(),
        geometry: GeometryHandle::new(geometry),
        role: opening.role(),
    })
}

fn asserted_capabilities(geometry: Geometry, capabilities: Capabilities) -> Vec<ClientToServerMsg> {
    let mut msgs = vec![
        ClientToServerMsg::TerminalPixelDimensions {
            pixel_dimensions: geometry.pixel_dimensions(),
        },
        ClientToServerMsg::KittyGraphicsSupport {
            supported: true,
            local_media: capabilities.local_media,
        },
        ClientToServerMsg::SixelSupport { supported: true },
        ClientToServerMsg::StructuredRenderSupport { supported: true },
    ];
    msgs.extend(palette::seed_messages(&capabilities.paints));
    msgs
}

fn connect_with_retry(
    path: &Path,
    deadline: Instant,
) -> Result<IpcSenderWithContext<ClientToServerMsg>> {
    retry_until(deadline, || ipc_connect(path))
        .map(IpcSenderWithContext::new)
        .map_err(|e| anyhow!("failed to connect to {:?}: {}", path, e))
}

fn retry_until<T>(
    deadline: Instant,
    mut attempt: impl FnMut() -> std::io::Result<T>,
) -> std::io::Result<T> {
    loop {
        match attempt() {
            Ok(value) => return Ok(value),
            Err(e) => {
                if Instant::now() >= deadline {
                    return Err(e);
                }
                thread::sleep(CONNECT_RETRY_INTERVAL);
            },
        }
    }
}

#[cfg(not(windows))]
fn reply_channel(
    sender: &IpcSenderWithContext<ClientToServerMsg>,
    _path: &Path,
    _deadline: Instant,
) -> Result<IpcReceiverWithContext<ServerToClientMsg>> {
    Ok(sender.get_receiver())
}

#[cfg(windows)]
fn reply_channel(
    _sender: &IpcSenderWithContext<ClientToServerMsg>,
    path: &Path,
    deadline: Instant,
) -> Result<IpcReceiverWithContext<ServerToClientMsg>> {
    retry_until(deadline, || zellij_utils::consts::ipc_connect_reply(path))
        .map(IpcReceiverWithContext::new)
        .map_err(|e| anyhow!("failed to connect to the reply pipe of {:?}: {}", path, e))
}

pub fn send(sender: &SharedSender, msg: ClientToServerMsg) -> Result<()> {
    sender.send_msg(msg)
}

pub fn resize(sender: &SharedSender, geometry: &GeometryHandle, next: Geometry) -> Result<bool> {
    let previous = geometry.replace(next);
    if previous.size() == next.size() && previous.pixel_dimensions() == next.pixel_dimensions() {
        return Ok(false);
    }
    if previous.size() != next.size() {
        send(
            sender,
            ClientToServerMsg::TerminalResize {
                new_size: next.size(),
            },
        )?;
    }
    if previous.pixel_dimensions() != next.pixel_dimensions() {
        send(
            sender,
            ClientToServerMsg::TerminalPixelDimensions {
                pixel_dimensions: next.pixel_dimensions(),
            },
        )?;
    }
    Ok(true)
}

#[derive(Clone)]
pub struct Detacher(Arc<Mutex<Option<(SharedSender, Role)>>>);

impl Detacher {
    pub fn new(sender: SharedSender, role: Role) -> Self {
        Detacher(Arc::new(Mutex::new(Some((sender, role)))))
    }

    pub fn follow(&self, sender: SharedSender, role: Role) {
        let mut current = self.0.lock().unwrap_or_else(|e| e.into_inner());
        *current = Some((sender, role));
    }

    pub fn abandon(&self) {
        let mut current = self.0.lock().unwrap_or_else(|e| e.into_inner());
        *current = None;
    }

    pub fn detach(&self) -> Result<()> {
        let held = {
            let current = self.0.lock().unwrap_or_else(|e| e.into_inner());
            current.clone()
        };
        let Some((sender, role)) = held else {
            bail!("the window is showing no session to detach from");
        };
        request_detach(&sender, role)
    }
}

pub fn request_detach(sender: &SharedSender, role: Role) -> Result<()> {
    let msg = match role {
        Role::Participant => ClientToServerMsg::Action {
            action: zellij_utils::input::actions::Action::Detach,
            terminal_id: None,
            client_id: None,
            is_cli_client: false,
        },
        Role::Watcher => {
            let key = KeyWithModifier::new(BareKey::Esc);
            let raw_bytes = key
                .serialize_non_kitty()
                .map(String::into_bytes)
                .unwrap_or_default();
            ClientToServerMsg::Key {
                key,
                raw_bytes,
                is_kitty_keyboard_protocol: false,
            }
        },
    };
    send(sender, msg)
}

#[cfg(test)]
pub fn test_attach_at(
    path: &Path,
    session_name: &str,
    geometry: Geometry,
    capabilities: Capabilities,
) -> Result<Connection> {
    use zellij_client::{first_message, ClientInfo};
    use zellij_utils::cli::CliArgs;
    use zellij_utils::input::options::Options;

    let message = first_message(
        &ClientInfo::Attach(session_name.to_owned(), Options::default()),
        &CliArgs::default(),
        &Options::default(),
        geometry.size(),
        crate::spawn::advertised_host_terminal_env(),
        None,
        None,
    )
    .message;
    open_at(
        path,
        session_name,
        geometry,
        Opening::Participant(capabilities),
        message,
        false,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_server::FakeServer;
    use zellij_client::{first_message, ClientInfo};
    use zellij_utils::cli::CliArgs;
    use zellij_utils::consts::session_layout_cache_file_name;
    use zellij_utils::data::{LayoutInfo, LayoutMetadata};
    use zellij_utils::input::options::Options;

    pub fn test_geometry() -> Geometry {
        Geometry {
            rows: 40,
            cols: 120,
            cell_width: 10,
            cell_height: 20,
        }
    }

    fn message_for(info: &ClientInfo, geometry: Geometry) -> ClientToServerMsg {
        first_message(
            info,
            &CliArgs::default(),
            &Options::default(),
            geometry.size(),
            crate::spawn::advertised_host_terminal_env(),
            None,
            None,
        )
        .message
    }

    fn attach_message(geometry: Geometry) -> ClientToServerMsg {
        message_for(
            &ClientInfo::Attach("window-test".to_owned(), Options::default()),
            geometry,
        )
    }

    fn create_message(geometry: Geometry) -> ClientToServerMsg {
        message_for(
            &ClientInfo::New(
                "window-test".to_owned(),
                None,
                Some(PathBuf::from("/tmp/project")),
                None,
            ),
            geometry,
        )
    }

    fn attach_at(path: &Path, geometry: Geometry) -> Result<Connection> {
        open_at(
            path,
            "window-test",
            geometry,
            Opening::Participant(Capabilities::default()),
            attach_message(geometry),
            false,
        )
    }

    fn watch_at(path: &Path, geometry: Geometry) -> Result<Connection> {
        open_at(
            path,
            "window-test",
            geometry,
            Opening::Watcher,
            message_for(
                &ClientInfo::Watch("window-test".to_owned(), Options::default()),
                geometry,
            ),
            false,
        )
    }

    #[test]
    fn geometry_maps_cells_to_pixels() {
        let geometry = test_geometry();
        assert_eq!(
            geometry.size(),
            Size {
                rows: 40,
                cols: 120
            }
        );
        let pixels = geometry.pixel_dimensions();
        assert_eq!(
            pixels.text_area_size,
            Some(SizeInPixels {
                width: 1200,
                height: 800
            })
        );
        assert_eq!(
            pixels.character_cell_size,
            Some(SizeInPixels {
                width: 10,
                height: 20
            })
        );
    }

    #[test]
    fn a_client_that_reads_no_media_asks_for_none() {
        let msgs = asserted_capabilities(
            test_geometry(),
            Capabilities {
                local_media: false,
                ..Capabilities::default()
            },
        );
        assert_eq!(
            msgs[1],
            ClientToServerMsg::KittyGraphicsSupport {
                supported: true,
                local_media: false
            }
        );
    }

    #[test]
    fn socket_path_rejects_names_that_overflow_the_sockaddr_limit() {
        let long_name = "a".repeat(ZELLIJ_SOCK_MAX_LENGTH);
        assert!(socket_path(&long_name).is_err());
    }

    #[test]
    fn socket_path_joins_the_contract_versioned_socket_dir() {
        let path = socket_path("window-test").unwrap();
        assert!(path.starts_with(&*ZELLIJ_SOCK_DIR));
        assert_eq!(path.file_name().unwrap(), "window-test");
    }

    #[test]
    fn capabilities_are_asserted_rather_than_probed() {
        let msgs = asserted_capabilities(test_geometry(), Capabilities::default());
        assert_eq!(
            msgs[0],
            ClientToServerMsg::TerminalPixelDimensions {
                pixel_dimensions: test_geometry().pixel_dimensions()
            }
        );
        assert_eq!(
            msgs[1],
            ClientToServerMsg::KittyGraphicsSupport {
                supported: true,
                local_media: true
            }
        );
        assert_eq!(msgs[2], ClientToServerMsg::SixelSupport { supported: true });
        assert_eq!(
            msgs[3],
            ClientToServerMsg::StructuredRenderSupport { supported: true }
        );
        assert!(matches!(msgs[4], ClientToServerMsg::ForegroundColor { .. }));
        assert!(matches!(msgs[5], ClientToServerMsg::BackgroundColor { .. }));
        assert!(matches!(msgs[6], ClientToServerMsg::ColorRegisters { .. }));
        assert_eq!(msgs.len(), 7);
    }

    #[test]
    fn attaching_sends_the_handshake_then_the_asserted_capabilities() {
        let server = FakeServer::spawn(|side| side.expect(8));
        let geometry = test_geometry();
        let connection = attach_at(&server.path, geometry).unwrap();
        drop(connection);

        let received = server.finish();
        match &received[0] {
            ClientToServerMsg::AttachClient {
                cli_assets,
                tab_position_to_focus,
                pane_to_focus,
                is_web_client,
            } => {
                assert_eq!(cli_assets.terminal_window_size, geometry.size());
                assert_eq!(*tab_position_to_focus, None);
                assert_eq!(*pane_to_focus, None);
                assert!(!is_web_client);
            },
            other => panic!("expected an attach message, got {:?}", other),
        }
        assert_eq!(
            &received[1..],
            &asserted_capabilities(geometry, Capabilities::default())[..]
        );
    }

    #[test]
    fn a_resurrection_starts_the_session_from_the_layout_it_left_behind() {
        let layout_path = session_layout_cache_file_name("window-test");
        let info = ClientInfo::Resurrect(
            "window-test".to_owned(),
            layout_path.clone(),
            false,
            Some(PathBuf::from("/tmp/project")),
        );
        match message_for(&info, test_geometry()) {
            ClientToServerMsg::FirstClientConnected { cli_assets, .. } => {
                assert_eq!(
                    cli_assets.layout,
                    Some(LayoutInfo::File(
                        layout_path.display().to_string(),
                        LayoutMetadata::default()
                    ))
                );
                assert!(
                    !cli_assets.force_run_layout_commands,
                    "resurrection must not re-run the commands the panes held"
                );
            },
            other => panic!("expected a session creation message, got {:?}", other),
        }
    }

    #[test]
    fn a_resurrection_that_was_asked_to_run_its_commands_says_so() {
        let info = ClientInfo::Resurrect(
            "window-test".to_owned(),
            session_layout_cache_file_name("window-test"),
            true,
            None,
        );
        match message_for(&info, test_geometry()) {
            ClientToServerMsg::FirstClientConnected { cli_assets, .. } => {
                assert!(cli_assets.force_run_layout_commands)
            },
            other => panic!("expected a session creation message, got {:?}", other),
        }
    }

    #[test]
    fn the_declared_colors_are_the_ones_the_client_paints_with() {
        let paints = Paints {
            background: [4, 5, 6],
            ..Paints::default()
        };
        let msgs = asserted_capabilities(
            test_geometry(),
            Capabilities {
                local_media: true,
                paints,
            },
        );
        assert_eq!(
            msgs[5],
            ClientToServerMsg::BackgroundColor {
                color: palette::xparse_color([4, 5, 6])
            }
        );
    }

    #[test]
    fn creation_declares_the_working_directory_the_first_pane_inherits() {
        let server = FakeServer::spawn(|side| side.expect(8));
        let geometry = test_geometry();
        let connection = open_at(
            &server.path,
            "window-test",
            geometry,
            Opening::Participant(Capabilities::default()),
            create_message(geometry),
            true,
        )
        .unwrap();
        drop(connection);

        let received = server.finish();
        match &received[0] {
            ClientToServerMsg::FirstClientConnected {
                cli_assets,
                is_web_client,
            } => {
                assert_eq!(cli_assets.cwd, Some(PathBuf::from("/tmp/project")));
                assert_eq!(cli_assets.terminal_window_size, geometry.size());
                assert!(!is_web_client);
            },
            other => panic!("expected a session creation message, got {:?}", other),
        }
        assert_eq!(
            &received[1..],
            &asserted_capabilities(geometry, Capabilities::default())[..]
        );
    }

    #[test]
    fn both_handshakes_name_the_terminal_the_window_emulates() {
        let attach = match attach_message(test_geometry()) {
            ClientToServerMsg::AttachClient { cli_assets, .. } => cli_assets.host_terminal_env,
            other => panic!("expected an attach message, got {:?}", other),
        };
        let create = match create_message(test_geometry()) {
            ClientToServerMsg::FirstClientConnected { cli_assets, .. } => {
                cli_assets.host_terminal_env
            },
            other => panic!("expected a creation message, got {:?}", other),
        };
        assert_eq!(attach, crate::spawn::advertised_host_terminal_env());
        assert_eq!(create, attach);
    }

    #[test]
    fn attaching_sends_no_working_directory_and_leaves_the_servers_own_in_place() {
        match attach_message(test_geometry()) {
            ClientToServerMsg::AttachClient { cli_assets, .. } => {
                assert_eq!(cli_assets.cwd, None);
            },
            other => panic!("expected an attach message, got {:?}", other),
        }
    }

    #[test]
    fn a_grid_change_sends_the_size_and_the_pixel_dimensions() {
        let server = FakeServer::spawn(|side| side.expect(10));
        let connection = attach_at(&server.path, test_geometry()).unwrap();
        let next = Geometry {
            rows: 24,
            cols: 80,
            ..test_geometry()
        };
        assert!(resize(&connection.sender, &connection.geometry, next).unwrap());
        assert_eq!(connection.geometry.get().size(), next.size());
        drop(connection);

        let received = server.finish();
        assert_eq!(
            &received[8..],
            &[
                ClientToServerMsg::TerminalResize {
                    new_size: next.size()
                },
                ClientToServerMsg::TerminalPixelDimensions {
                    pixel_dimensions: next.pixel_dimensions()
                },
            ]
        );
    }

    #[test]
    fn a_cell_metric_change_alone_sends_only_the_pixel_dimensions() {
        let server = FakeServer::spawn(|side| side.expect(9));
        let connection = attach_at(&server.path, test_geometry()).unwrap();
        let next = Geometry {
            cell_width: 8,
            ..test_geometry()
        };
        assert!(resize(&connection.sender, &connection.geometry, next).unwrap());
        drop(connection);

        let received = server.finish();
        assert_eq!(
            &received[8..],
            &[ClientToServerMsg::TerminalPixelDimensions {
                pixel_dimensions: next.pixel_dimensions()
            }]
        );
    }

    #[test]
    fn an_unchanged_geometry_sends_nothing() {
        let server = FakeServer::spawn(|side| side.expect(9));
        let connection = attach_at(&server.path, test_geometry()).unwrap();
        assert!(!resize(&connection.sender, &connection.geometry, test_geometry()).unwrap());
        request_detach(&connection.sender, connection.role).unwrap();
        drop(connection);

        let received = server.finish();
        assert!(matches!(
            received[8],
            ClientToServerMsg::Action {
                action: zellij_utils::input::actions::Action::Detach,
                ..
            }
        ));
    }

    #[test]
    fn a_watcher_attaches_read_only_and_declares_structured_rendering() {
        let server = FakeServer::spawn(|side| side.expect(2));
        let connection = watch_at(&server.path, test_geometry()).unwrap();
        assert_eq!(connection.role, Role::Watcher);
        drop(connection);

        let received = server.finish();
        assert_eq!(
            received,
            vec![
                ClientToServerMsg::AttachWatcherClient {
                    terminal_size: test_geometry().size(),
                    is_web_client: false,
                },
                ClientToServerMsg::StructuredRenderSupport { supported: true },
            ]
        );
    }

    #[test]
    fn a_watcher_leaves_with_a_key_because_the_detach_action_is_ignored_for_watchers() {
        let server = FakeServer::spawn(|side| side.expect(3));
        let connection = watch_at(&server.path, test_geometry()).unwrap();
        request_detach(&connection.sender, connection.role).unwrap();
        drop(connection);

        let received = server.finish();
        assert!(
            matches!(
                received.last().unwrap(),
                ClientToServerMsg::Key { key, .. } if key.bare_key == BareKey::Esc
            ),
            "got {:?}",
            received.last()
        );
    }

    #[test]
    fn detach_is_requested_as_a_typed_action() {
        let server = FakeServer::spawn(|side| side.expect(9));
        let connection = attach_at(&server.path, test_geometry()).unwrap();
        request_detach(&connection.sender, connection.role).unwrap();
        drop(connection);

        let received = server.finish();
        assert_eq!(
            received.last().unwrap(),
            &ClientToServerMsg::Action {
                action: zellij_utils::input::actions::Action::Detach,
                terminal_id: None,
                client_id: None,
                is_cli_client: false,
            }
        );
    }

    #[test]
    fn an_abandoned_detacher_says_it_has_no_session_rather_than_writing_to_a_dead_one() {
        let server = FakeServer::spawn(|side| side.expect(8));
        let connection = attach_at(&server.path, test_geometry()).unwrap();
        let detacher = Detacher::new(connection.sender.clone(), connection.role);
        detacher.abandon();

        let refused = detacher.detach().unwrap_err().to_string();
        drop(connection);

        assert!(refused.contains("no session"), "{}", refused);
        assert_eq!(
            server.finish().len(),
            8,
            "an abandoned detacher sends nothing"
        );
    }

    #[test]
    fn a_detacher_follows_the_session_the_window_is_showing() {
        let first = FakeServer::spawn(|side| side.expect(8));
        let second = FakeServer::spawn(|side| side.expect(9));
        let before = attach_at(&first.path, test_geometry()).unwrap();
        let detacher = Detacher::new(before.sender.clone(), before.role);

        let after = attach_at(&second.path, test_geometry()).unwrap();
        detacher.follow(after.sender.clone(), after.role);
        detacher.detach().unwrap();
        drop(before);
        drop(after);

        assert_eq!(
            first.finish().len(),
            8,
            "the session left behind is untouched"
        );
        assert!(matches!(
            second.finish().last().unwrap(),
            ClientToServerMsg::Action {
                action: zellij_utils::input::actions::Action::Detach,
                ..
            }
        ));
    }
}
