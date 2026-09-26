use crate::os_input_output::ClientOsApi;
use crate::web_client::control_message::WebServerToWebClientControlMessage;
use crate::web_client::types::{ClientChannels, ClientConnectionBus, ConnectionTable};
use axum::extract::ws::{CloseFrame, Message};
use std::sync::{atomic::AtomicBool, Arc};
use tokio::sync::mpsc::UnboundedSender;
use tokio_util::sync::CancellationToken;
use zellij_utils::ipc::ClientToServerMsg;
use zellij_utils::pane_size::Size;

impl ConnectionTable {
    pub fn add_new_client(
        &mut self,
        client_id: String,
        client_os_api: Box<dyn ClientOsApi>,
        is_read_only: bool,
    ) {
        self.client_id_to_channels
            .insert(client_id.clone(), ClientChannels::new(client_os_api));
        self.client_read_only_status.insert(client_id, is_read_only);
    }

    pub fn is_client_read_only(&self, client_id: &str) -> bool {
        self.client_read_only_status
            .get(client_id)
            .copied()
            .unwrap_or(false)
    }

    pub fn add_client_control_tx(
        &mut self,
        client_id: &str,
        control_channel_tx: UnboundedSender<Message>,
    ) {
        self.client_id_to_channels
            .get_mut(client_id)
            .map(|c| c.add_control_tx(control_channel_tx));
    }

    pub fn add_client_terminal_tx(
        &mut self,
        client_id: &str,
        terminal_channel_tx: UnboundedSender<String>,
    ) {
        self.client_id_to_channels
            .get_mut(client_id)
            .map(|c| c.add_terminal_tx(terminal_channel_tx));
    }

    pub fn add_client_terminal_channel_cancellation_token(
        &mut self,
        client_id: &str,
        terminal_channel_cancellation_token: CancellationToken,
    ) {
        self.client_id_to_channels.get_mut(client_id).map(|c| {
            c.add_terminal_channel_cancellation_token(terminal_channel_cancellation_token)
        });
    }

    pub fn get_client_os_api(&self, client_id: &str) -> Option<&Box<dyn ClientOsApi>> {
        self.client_id_to_channels.get(client_id).map(|c| &c.os_api)
    }

    pub fn get_client_terminal_tx(&self, client_id: &str) -> Option<UnboundedSender<String>> {
        self.client_id_to_channels
            .get(client_id)
            .and_then(|c| c.terminal_channel_tx.clone())
    }

    pub fn get_client_control_tx(&self, client_id: &str) -> Option<UnboundedSender<Message>> {
        self.client_id_to_channels
            .get(client_id)
            .and_then(|c| c.control_channel_tx.clone())
    }

    pub fn remove_client(&mut self, client_id: &str) {
        if let Some(mut client_channels) = self.client_id_to_channels.remove(client_id).take() {
            client_channels.cleanup();
        }
        self.client_read_only_status.remove(client_id);
        self.client_id_to_session.remove(client_id);
        self.client_size.remove(client_id);
        // If this client owned a session's size, release ownership: the next
        // client to type takes it.
        self.session_size_owner
            .retain(|_, owner| owner.as_str() != client_id);
    }

    pub fn set_client_session(&mut self, client_id: &str, session_name: String) {
        self.client_id_to_session
            .insert(client_id.to_owned(), session_name);
    }

    // THE SESSION FOLLOWS ITS MOST RECENTLY ACTIVE WEB CLIENT (zellient#7; tmux's
    // `window-size latest`). The session server renders a session at the MINIMUM
    // of its clients' registered sizes (min_client_terminal_size), so one idle
    // web client (a phone, a laptop tab, a client that never sent TerminalResize
    // and sat at the daemon's fd0 fallback of 80x24) held every other window of
    // that session to a small box, and no client-side resize could lift it: the
    // old claim only dropped non-owners' FUTURE resizes, while the size they had
    // already registered still fed the minimum. Now the web layer RE-REGISTERS
    // every other web client of the session at the owner's size (a
    // TerminalResize sent on that client's own server connection), so the
    // minimum is the owner and the others mirror it. A client becomes the owner
    // by typing (or by an explicit DetachOtherClients claim). Native clients are
    // untouched: the orchestrator's headless ones attach at 2000x500 and never
    // clamp.

    /// The size a web client registers with when it attaches: the size owner's,
    /// else the size its session last took from a web client, else NO_CLAMP_SIZE.
    /// Never the fd0 fallback, which is how a silent attacher clamped everyone.
    pub fn attach_size(&self, client_id: &str) -> Size {
        let Some(session) = self.client_id_to_session.get(client_id) else {
            return NO_CLAMP_SIZE;
        };
        self.session_size_owner
            .get(session)
            .and_then(|owner| self.client_size.get(owner))
            .or_else(|| self.session_size.get(session))
            .copied()
            .unwrap_or(NO_CLAMP_SIZE)
    }

    /// A client reported its size. With no owner it is registered as-is; the
    /// owner's size is imposed on every other web client of the session; any
    /// other client's size is only remembered (it mirrors the owner until it
    /// becomes the owner itself). Returns the resizes to send.
    pub fn on_terminal_resize(&mut self, client_id: &str, size: Size) -> Vec<Resize> {
        self.client_size.insert(client_id.to_owned(), size);
        let Some(session) = self.client_id_to_session.get(client_id).cloned() else {
            return self.resize_for(client_id, size).into_iter().collect();
        };
        match self.session_size_owner.get(&session) {
            Some(owner) if owner != client_id => vec![],
            Some(_) => self.impose(&session, client_id, size),
            None => {
                self.session_size.insert(session, size);
                self.resize_for(client_id, size).into_iter().collect()
            },
        }
    }

    pub fn get_should_not_reconnect_flag(&self, client_id: &str) -> Option<Arc<AtomicBool>> {
        self.client_id_to_channels
            .get(client_id)
            .map(|c| c.should_not_reconnect.clone())
    }

    /// The client typed: it becomes its session's size owner, and the session
    /// follows it. Nothing to send when it already owns the session, or when it
    /// has never reported a size (it has no size to impose).
    pub fn on_user_input(&mut self, client_id: &str) -> Vec<Resize> {
        let Some(session) = self.client_id_to_session.get(client_id).cloned() else {
            return vec![];
        };
        if self.session_size_owner.get(&session).map(String::as_str) == Some(client_id) {
            return vec![];
        }
        let Some(size) = self.client_size.get(client_id).copied() else {
            return vec![];
        };
        self.session_size_owner
            .insert(session.clone(), client_id.to_owned());
        self.impose(&session, client_id, size)
    }

    /// An explicit claim (DetachOtherClients: Zellient's Ctrl+0 and Ctrl+Shift+O).
    /// Returns how many other web clients of the session now mirror the
    /// requester, and the resizes that make them.
    pub fn claim_session_size_owner(&mut self, requester_id: &str) -> (usize, Vec<Resize>) {
        let Some(session) = self.client_id_to_session.get(requester_id).cloned() else {
            return (0, vec![]);
        };
        let others = self.others_in_session(&session, requester_id).len();
        self.session_size_owner
            .insert(session.clone(), requester_id.to_owned());
        let resizes = match self.client_size.get(requester_id).copied() {
            Some(size) => self.impose(&session, requester_id, size),
            None => vec![],
        };
        (others, resizes)
    }

    /// Register `owner` and every other web client of `session` at `size`.
    fn impose(&mut self, session: &str, owner: &str, size: Size) -> Vec<Resize> {
        self.session_size.insert(session.to_owned(), size);
        let mut out: Vec<Resize> = self.resize_for(owner, size).into_iter().collect();
        for other in self.others_in_session(session, owner) {
            out.extend(self.resize_for(&other, size));
        }
        out
    }

    fn others_in_session(&self, session: &str, except: &str) -> Vec<String> {
        self.client_id_to_session
            .iter()
            .filter(|(id, sess)| id.as_str() != except && sess.as_str() == session)
            .map(|(id, _)| id.clone())
            .collect()
    }

    fn resize_for(&self, client_id: &str, size: Size) -> Option<Resize> {
        self.client_id_to_channels
            .get(client_id)
            .map(|c| (c.os_api.clone(), size))
    }
}

/// A web client that has not reported a size registers at this instead of the
/// daemon's fd0 fallback (80x24): too large ever to be its session's minimum.
/// The orchestrator's headless clients use the same size for the same reason.
pub const NO_CLAMP_SIZE: Size = Size {
    rows: 500,
    cols: 2000,
};

/// A resize to send: the client's own server connection, and the size to
/// register for it.
pub type Resize = (Box<dyn ClientOsApi>, Size);

/// Send resizes the connection table decided on. Called with the table's lock
/// released: each is an IPC write to the session server.
pub fn send_resizes(resizes: Vec<Resize>) {
    for (os_api, size) in resizes {
        os_api.send_to_server(ClientToServerMsg::TerminalResize { new_size: size });
    }
}

impl ClientConnectionBus {
    pub fn send_stdout(&mut self, stdout: String) {
        match self.stdout_channel_tx.as_ref() {
            Some(stdout_channel_tx) => {
                let _ = stdout_channel_tx.send(stdout);
            },
            None => {
                self.get_stdout_channel_tx();
                if let Some(stdout_channel_tx) = self.stdout_channel_tx.as_ref() {
                    let _ = stdout_channel_tx.send(stdout);
                } else {
                    log::error!("Failed to send STDOUT message to client");
                }
            },
        }
    }

    pub fn send_control(&mut self, message: WebServerToWebClientControlMessage) {
        let message = Message::Text(serde_json::to_string(&message).unwrap().into());
        match self.control_channel_tx.as_ref() {
            Some(control_channel_tx) => {
                let _ = control_channel_tx.send(message);
            },
            None => {
                self.get_control_channel_tx();
                if let Some(control_channel_tx) = self.control_channel_tx.as_ref() {
                    let _ = control_channel_tx.send(message);
                } else {
                    log::error!("Failed to send control message to client");
                }
            },
        }
    }
    pub fn close_connection(&mut self) {
        let should_not_reconnect = self
            .connection_table
            .lock()
            .unwrap()
            .get_should_not_reconnect_flag(&self.web_client_id)
            .map(|f| f.load(std::sync::atomic::Ordering::Relaxed))
            .unwrap_or(false);
        let code = if should_not_reconnect {
            4001u16
        } else {
            axum::extract::ws::close_code::NORMAL
        };
        let close_frame = CloseFrame {
            code,
            reason: "Connection closed".into(),
        };
        let close_message = Message::Close(Some(close_frame));
        match self.control_channel_tx.as_ref() {
            Some(control_channel_tx) => {
                let _ = control_channel_tx.send(close_message);
            },
            None => {
                self.get_control_channel_tx();
                if let Some(control_channel_tx) = self.control_channel_tx.as_ref() {
                    let _ = control_channel_tx.send(close_message);
                } else {
                    log::error!("Failed to send close message to client");
                }
            },
        }
        self.connection_table
            .lock()
            .unwrap()
            .remove_client(&self.web_client_id);
    }

    pub fn close_connection_kicked(&mut self) {
        if let Some(flag) = self
            .connection_table
            .lock()
            .unwrap()
            .get_should_not_reconnect_flag(&self.web_client_id)
        {
            flag.store(true, std::sync::atomic::Ordering::Relaxed);
        }
        self.close_connection();
    }

    fn get_control_channel_tx(&mut self) {
        if let Some(control_channel_tx) = self
            .connection_table
            .lock()
            .unwrap()
            .get_client_control_tx(&self.web_client_id)
        {
            self.control_channel_tx = Some(control_channel_tx);
        }
    }

    fn get_stdout_channel_tx(&mut self) {
        if let Some(stdout_channel_tx) = self
            .connection_table
            .lock()
            .unwrap()
            .get_client_terminal_tx(&self.web_client_id)
        {
            self.stdout_channel_tx = Some(stdout_channel_tx);
        }
    }
}
