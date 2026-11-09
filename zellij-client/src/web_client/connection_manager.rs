use crate::os_input_output::ClientOsApi;
use crate::web_client::client_channel::{ClientSender, SendFailure};
use crate::web_client::control_message::WebServerToWebClientControlMessage;
use crate::web_client::types::{
    ClientChannels, ClientConnectionBus, ClientLimitExceeded, ConnectionTable,
    MAX_UNATTACHED_WEB_CLIENTS_PER_SESSION, MAX_WEB_CLIENTS, UNATTACHED_WEB_CLIENT_TTL,
};
use axum::extract::ws::{CloseFrame, Message};
use std::sync::{atomic::AtomicBool, Arc};
use std::time::Instant;
use tokio_util::sync::CancellationToken;

/// The result of a terminal websocket trying to claim a client id.
pub enum TerminalAttachOutcome {
    Attached(Box<dyn ClientOsApi>),
    AlreadyAttached,
    UnknownClient,
}

impl ConnectionTable {
    pub fn try_add_new_client(
        &mut self,
        client_id: String,
        client_os_api: Box<dyn ClientOsApi>,
        is_read_only: bool,
        session_token_hash: String,
    ) -> Result<(), ClientLimitExceeded> {
        self.try_add_new_client_at(
            Instant::now(),
            client_id,
            client_os_api,
            is_read_only,
            session_token_hash,
        )
    }

    fn try_add_new_client_at(
        &mut self,
        now: Instant,
        client_id: String,
        client_os_api: Box<dyn ClientOsApi>,
        is_read_only: bool,
        session_token_hash: String,
    ) -> Result<(), ClientLimitExceeded> {
        // ids nobody came back for do not count against anyone
        self.remove_expired_unattached_clients(now);
        if self.client_id_to_channels.len() >= MAX_WEB_CLIENTS {
            return Err(ClientLimitExceeded::TooManyClients);
        }
        let unattached_for_this_session = self
            .client_id_to_channels
            .iter()
            .filter(|(id, channels)| {
                !channels.is_attached()
                    && self.client_session_token_hash.get(*id) == Some(&session_token_hash)
            })
            .count();
        if unattached_for_this_session >= MAX_UNATTACHED_WEB_CLIENTS_PER_SESSION {
            return Err(ClientLimitExceeded::TooManyUnattachedClients);
        }
        let mut channels = ClientChannels::new(client_os_api);
        channels.created_at = now;
        self.client_id_to_channels
            .insert(client_id.clone(), channels);
        self.client_read_only_status
            .insert(client_id.clone(), is_read_only);
        self.client_session_token_hash
            .insert(client_id, session_token_hash);
        Ok(())
    }

    /// Forgets the clients that were handed an id but never opened their
    /// terminal websocket within `UNATTACHED_WEB_CLIENT_TTL`.
    pub fn remove_expired_unattached_clients(&mut self, now: Instant) -> usize {
        let expired: Vec<String> = self
            .client_id_to_channels
            .iter()
            .filter(|(_, channels)| {
                !channels.is_attached()
                    && now.saturating_duration_since(channels.created_at)
                        >= UNATTACHED_WEB_CLIENT_TTL
            })
            .map(|(id, _)| id.clone())
            .collect();
        for client_id in &expired {
            self.remove_client(client_id);
        }
        expired.len()
    }

    pub fn verify_client_ownership(&self, client_id: &str, session_token_hash: &str) -> bool {
        self.client_session_token_hash
            .get(client_id)
            .map(|hash| hash == session_token_hash)
            .unwrap_or(false)
    }

    pub fn is_client_read_only(&self, client_id: &str) -> bool {
        self.client_read_only_status
            .get(client_id)
            .copied()
            .unwrap_or(false)
    }

    pub fn is_client_structured(&self, client_id: &str) -> bool {
        self.client_structured_status
            .get(client_id)
            .copied()
            .unwrap_or(false)
    }

    /// Registers the control websocket of a client, replacing (and cancelling)
    /// the one it had. Returns false if there is no such client any more.
    pub fn add_client_control_tx(
        &mut self,
        client_id: &str,
        control_channel_tx: ClientSender,
        cancellation_token: CancellationToken,
    ) -> bool {
        match self.client_id_to_channels.get_mut(client_id) {
            Some(channels) => {
                channels.add_control_tx(control_channel_tx, cancellation_token);
                true
            },
            None => false,
        }
    }

    pub fn queue_client_control_message(&mut self, client_id: &str, message: Message) {
        self.client_id_to_channels
            .get_mut(client_id)
            .map(|c| c.queue_control_message(message));
    }

    /// Claims the client id for a terminal websocket, atomically: only the
    /// first one gets the client's os api (and with it the right to start a
    /// session connection for it).
    pub fn attach_terminal(
        &mut self,
        client_id: &str,
        is_structured: bool,
        terminal_channel_tx: ClientSender,
    ) -> TerminalAttachOutcome {
        match self.client_id_to_channels.get_mut(client_id) {
            None => TerminalAttachOutcome::UnknownClient,
            Some(channels) if channels.is_attached() => TerminalAttachOutcome::AlreadyAttached,
            Some(channels) => {
                channels.add_terminal_tx(terminal_channel_tx);
                let os_api = channels.os_api.clone();
                self.client_structured_status
                    .insert(client_id.to_owned(), is_structured);
                TerminalAttachOutcome::Attached(os_api)
            },
        }
    }

    #[cfg(test)]
    pub fn add_client_terminal_tx(&mut self, client_id: &str, terminal_channel_tx: ClientSender) {
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

    pub fn get_client_terminal_tx(&self, client_id: &str) -> Option<ClientSender> {
        self.client_id_to_channels
            .get(client_id)
            .and_then(|c| c.terminal_channel_tx.clone())
    }

    pub fn get_client_control_tx(&self, client_id: &str) -> Option<ClientSender> {
        self.client_id_to_channels
            .get(client_id)
            .and_then(|c| c.control_channel_tx.clone())
    }

    pub fn remove_client(&mut self, client_id: &str) {
        if let Some(mut client_channels) = self.client_id_to_channels.remove(client_id).take() {
            client_channels.cleanup();
        }
        self.client_read_only_status.remove(client_id);
        self.client_session_token_hash.remove(client_id);
        self.client_structured_status.remove(client_id);
    }

    pub fn get_should_not_reconnect_flag(&self, client_id: &str) -> Option<Arc<AtomicBool>> {
        self.client_id_to_channels
            .get(client_id)
            .map(|c| c.should_not_reconnect.clone())
    }
}

impl ClientConnectionBus {
    pub fn send_stdout(&mut self, stdout: String) {
        self.send_terminal_message(Message::Text(stdout.into()));
    }

    pub fn send_frame(&mut self, frame: Vec<u8>) {
        self.send_terminal_message(Message::Binary(frame.into()));
    }

    fn send_terminal_message(&mut self, message: Message) {
        if self.stdout_channel_tx.is_none() {
            self.get_stdout_channel_tx();
        }
        match self.stdout_channel_tx.as_ref() {
            Some(stdout_channel_tx) => {
                if let Err(SendFailure::Full) = stdout_channel_tx.try_send(message) {
                    self.disconnect_slow_client("terminal output");
                }
            },
            None => log::error!("Failed to send STDOUT message to client"),
        }
    }

    /// The client is not reading what we send it. Rather than queue output for it
    /// without bound, drop the connection (the browser reconnects with a fresh
    /// screen if it is still there).
    fn disconnect_slow_client(&mut self, what: &str) {
        log::warn!(
            "Web client {} is not keeping up with its {} queue, disconnecting it",
            self.web_client_id,
            what
        );
        self.close_connection();
    }

    pub fn send_control(&mut self, message: WebServerToWebClientControlMessage) {
        let message = Message::Text(serde_json::to_string(&message).unwrap().into());
        if self.control_channel_tx.is_none() {
            self.get_control_channel_tx();
        }
        match self.control_channel_tx.as_ref() {
            Some(control_channel_tx) => {
                if let Err(SendFailure::Full) = control_channel_tx.try_send(message) {
                    self.disconnect_slow_client("control message");
                }
            },
            None => {
                self.connection_table
                    .lock()
                    .unwrap()
                    .queue_client_control_message(&self.web_client_id, message);
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
        if self.control_channel_tx.is_none() {
            self.get_control_channel_tx();
        }
        match self.control_channel_tx.as_ref() {
            // if the queue is full the client is being dropped anyway
            Some(control_channel_tx) => {
                let _ = control_channel_tx.try_send(close_message);
            },
            None => log::error!("Failed to send close message to client"),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::os_input_output::ClientOsApi;
    use crate::web_client::client_channel::client_channel;
    use std::io::{BufRead, Cursor, Write};
    use std::path::Path;
    use std::sync::Mutex;
    use zellij_utils::{
        data::Palette,
        errors::ErrorContext,
        ipc::{ClientToServerMsg, ServerToClientMsg},
        pane_size::Size,
    };

    #[derive(Clone, Debug, Default)]
    struct StubOsInput;

    impl ClientOsApi for StubOsInput {
        fn get_terminal_size(&self) -> Size {
            Size::default()
        }
        fn set_raw_mode(&mut self) {}
        fn unset_raw_mode(&self) -> Result<(), std::io::Error> {
            Ok(())
        }
        fn get_stdout_writer(&self) -> Box<dyn Write> {
            Box::new(std::io::sink())
        }
        fn get_stdin_reader(&self) -> Box<dyn BufRead> {
            Box::new(Cursor::new(Vec::<u8>::new()))
        }
        fn update_session_name(&mut self, _new_session_name: String) {}
        fn read_from_stdin(&mut self) -> Result<Vec<u8>, &'static str> {
            Ok(vec![])
        }
        fn box_clone(&self) -> Box<dyn ClientOsApi> {
            Box::new(self.clone())
        }
        fn send_to_server(&self, _msg: ClientToServerMsg) {}
        fn recv_from_server(&self) -> Option<(ServerToClientMsg, ErrorContext)> {
            None
        }
        fn handle_signals(
            &self,
            _sigwinch_cb: Box<dyn Fn()>,
            _quit_cb: Box<dyn Fn()>,
            _resize_receiver: Option<std::sync::mpsc::Receiver<()>>,
        ) {
        }
        fn connect_to_server(&self, _path: &Path) {}
        fn load_palette(&self) -> Palette {
            Palette::default()
        }
        fn enable_mouse(&self) -> anyhow::Result<()> {
            Ok(())
        }
        fn disable_mouse(&self) -> anyhow::Result<()> {
            Ok(())
        }
    }

    fn connection_table_with_client(client_id: &str) -> Arc<Mutex<ConnectionTable>> {
        let connection_table = Arc::new(Mutex::new(ConnectionTable::default()));
        connection_table
            .lock()
            .unwrap()
            .try_add_new_client(
                client_id.to_owned(),
                Box::new(StubOsInput::default()),
                false,
                "token".to_owned(),
            )
            .unwrap();
        connection_table
    }

    fn message_text(message: &Message) -> String {
        match message {
            Message::Text(text) => text.to_string(),
            other => panic!("expected a text control message, got {other:?}"),
        }
    }

    #[test]
    fn control_messages_sent_before_registration_are_buffered_and_flushed_in_order() {
        let connection_table = connection_table_with_client("client");
        let mut bus = ClientConnectionBus::new("client", &connection_table);

        bus.send_control(WebServerToWebClientControlMessage::QueryTerminalSize);
        bus.send_control(WebServerToWebClientControlMessage::SetSoftKeyboard { on: true });

        let (control_channel_tx, mut control_channel_rx) = client_channel(256, 1024 * 1024);
        connection_table.lock().unwrap().add_client_control_tx(
            "client",
            control_channel_tx,
            CancellationToken::new(),
        );

        let first = control_channel_rx
            .try_recv()
            .expect("the first buffered control message must be flushed on registration");
        let second = control_channel_rx
            .try_recv()
            .expect("the second buffered control message must be flushed on registration");

        assert!(message_text(&first).contains("QueryTerminalSize"));
        assert!(message_text(&second).contains("SetSoftKeyboard"));
        assert!(control_channel_rx.try_recv().is_err());
    }

    #[test]
    fn control_messages_are_delivered_directly_once_registered() {
        let connection_table = connection_table_with_client("client");
        let (control_channel_tx, mut control_channel_rx) = client_channel(256, 1024 * 1024);
        connection_table.lock().unwrap().add_client_control_tx(
            "client",
            control_channel_tx,
            CancellationToken::new(),
        );

        let mut bus = ClientConnectionBus::new("client", &connection_table);
        bus.send_control(WebServerToWebClientControlMessage::QueryTerminalSize);

        let message = control_channel_rx
            .try_recv()
            .expect("a registered control channel must receive messages directly");
        assert!(message_text(&message).contains("QueryTerminalSize"));
    }

    #[test]
    fn a_client_that_stops_reading_terminal_output_is_disconnected_not_buffered() {
        let connection_table = connection_table_with_client("client");
        // the receiving end stays alive but is never read from
        let (stdout_channel_tx, _stdout_channel_rx) = client_channel(4, 1024);
        connection_table
            .lock()
            .unwrap()
            .add_client_terminal_tx("client", stdout_channel_tx);
        let mut bus = ClientConnectionBus::new("client", &connection_table);

        for _ in 0..4 {
            bus.send_stdout("output".to_owned());
        }
        assert!(
            connection_table
                .lock()
                .unwrap()
                .get_client_os_api("client")
                .is_some(),
            "a client within its queue limit stays connected"
        );

        bus.send_stdout("one too many".to_owned());
        assert!(
            connection_table
                .lock()
                .unwrap()
                .get_client_os_api("client")
                .is_none(),
            "a client past its queue limit is dropped"
        );
    }

    #[test]
    fn a_client_that_stops_reading_control_messages_is_disconnected_not_buffered() {
        let connection_table = connection_table_with_client("client");
        let (control_channel_tx, _control_channel_rx) = client_channel(2, 1024);
        connection_table.lock().unwrap().add_client_control_tx(
            "client",
            control_channel_tx,
            CancellationToken::new(),
        );
        let mut bus = ClientConnectionBus::new("client", &connection_table);

        bus.send_control(WebServerToWebClientControlMessage::QueryTerminalSize);
        bus.send_control(WebServerToWebClientControlMessage::QueryTerminalSize);
        assert!(connection_table
            .lock()
            .unwrap()
            .get_client_os_api("client")
            .is_some());
        bus.send_control(WebServerToWebClientControlMessage::QueryTerminalSize);
        assert!(connection_table
            .lock()
            .unwrap()
            .get_client_os_api("client")
            .is_none());
    }

    fn add_client_for_session(
        table: &mut ConnectionTable,
        now: Instant,
        client_id: &str,
        session_token_hash: &str,
    ) -> Result<(), ClientLimitExceeded> {
        table.try_add_new_client_at(
            now,
            client_id.to_owned(),
            Box::new(StubOsInput::default()),
            false,
            session_token_hash.to_owned(),
        )
    }

    #[test]
    fn a_session_can_only_hold_so_many_unattached_client_ids() {
        let mut table = ConnectionTable::default();
        let now = Instant::now();
        for n in 0..MAX_UNATTACHED_WEB_CLIENTS_PER_SESSION {
            add_client_for_session(&mut table, now, &format!("greedy-{n}"), "greedy").unwrap();
        }
        assert_eq!(
            add_client_for_session(&mut table, now, "one-more", "greedy"),
            Err(ClientLimitExceeded::TooManyUnattachedClients)
        );
        // other sessions are not affected by it
        assert_eq!(
            add_client_for_session(&mut table, now, "other", "someone-else"),
            Ok(())
        );
        // and once a client attaches it no longer counts as unattached
        let (terminal_tx, _terminal_rx) = client_channel(4, 1024);
        table.add_client_terminal_tx("greedy-0", terminal_tx);
        assert_eq!(
            add_client_for_session(&mut table, now, "now-it-fits", "greedy"),
            Ok(())
        );
    }

    #[test]
    fn the_total_number_of_clients_is_capped() {
        let mut table = ConnectionTable::default();
        let now = Instant::now();
        for n in 0..MAX_WEB_CLIENTS {
            // spread over sessions so that only the total cap can trigger
            let session = format!("session-{}", n / MAX_UNATTACHED_WEB_CLIENTS_PER_SESSION);
            add_client_for_session(&mut table, now, &format!("client-{n}"), &session).unwrap();
        }
        assert_eq!(
            add_client_for_session(&mut table, now, "one-more", "fresh-session"),
            Err(ClientLimitExceeded::TooManyClients)
        );
        table.remove_client("client-0");
        assert_eq!(
            add_client_for_session(&mut table, now, "one-more", "fresh-session"),
            Ok(())
        );
    }

    #[test]
    fn unattached_client_ids_expire_but_attached_clients_do_not() {
        let mut table = ConnectionTable::default();
        let start = Instant::now();
        add_client_for_session(&mut table, start, "never-attached", "token").unwrap();
        add_client_for_session(&mut table, start, "attached", "token").unwrap();
        let (terminal_tx, _terminal_rx) = client_channel(4, 1024);
        table.add_client_terminal_tx("attached", terminal_tx);

        let just_before = start + UNATTACHED_WEB_CLIENT_TTL - std::time::Duration::from_secs(1);
        assert_eq!(table.remove_expired_unattached_clients(just_before), 0);
        assert!(table.verify_client_ownership("never-attached", "token"));

        let after = start + UNATTACHED_WEB_CLIENT_TTL;
        assert_eq!(table.remove_expired_unattached_clients(after), 1);
        assert!(!table.verify_client_ownership("never-attached", "token"));
        assert!(table.get_client_os_api("never-attached").is_none());
        assert!(table.verify_client_ownership("attached", "token"));
    }

    #[test]
    fn expired_ids_free_up_room_for_new_ones() {
        let mut table = ConnectionTable::default();
        let start = Instant::now();
        for n in 0..MAX_UNATTACHED_WEB_CLIENTS_PER_SESSION {
            add_client_for_session(&mut table, start, &format!("c-{n}"), "token").unwrap();
        }
        assert!(add_client_for_session(&mut table, start, "extra", "token").is_err());
        let later = start + UNATTACHED_WEB_CLIENT_TTL;
        assert_eq!(
            add_client_for_session(&mut table, later, "extra", "token"),
            Ok(())
        );
    }

    #[test]
    fn buffered_control_messages_are_bounded() {
        let connection_table = connection_table_with_client("client");
        let mut bus = ClientConnectionBus::new("client", &connection_table);

        for _ in 0..200 {
            bus.send_control(WebServerToWebClientControlMessage::QueryTerminalSize);
        }

        let (control_channel_tx, mut control_channel_rx) = client_channel(256, 1024 * 1024);
        connection_table.lock().unwrap().add_client_control_tx(
            "client",
            control_channel_tx,
            CancellationToken::new(),
        );

        let mut flushed = 0;
        while control_channel_rx.try_recv().is_ok() {
            flushed += 1;
        }
        assert_eq!(flushed, 64);
    }

    #[test]
    fn removing_a_client_cancels_both_of_its_websockets() {
        let connection_table = connection_table_with_client("client");
        let (control_channel_tx, _control_channel_rx) = client_channel(256, 1024 * 1024);
        let control_token = CancellationToken::new();
        let terminal_token = CancellationToken::new();
        {
            let mut table = connection_table.lock().unwrap();
            assert!(table.add_client_control_tx(
                "client",
                control_channel_tx,
                control_token.clone()
            ));
            table.add_client_terminal_channel_cancellation_token("client", terminal_token.clone());
        }
        assert!(!control_token.is_cancelled());
        assert!(!terminal_token.is_cancelled());

        connection_table.lock().unwrap().remove_client("client");
        assert!(control_token.is_cancelled());
        assert!(
            terminal_token.is_cancelled(),
            "the reader of a dropped client has to be told, not only its writer"
        );
    }

    #[test]
    fn a_client_dropped_for_not_reading_has_its_websockets_cancelled() {
        let connection_table = connection_table_with_client("client");
        let (stdout_channel_tx, _stdout_channel_rx) = client_channel(1, 1024);
        let terminal_token = CancellationToken::new();
        {
            let mut table = connection_table.lock().unwrap();
            table.add_client_terminal_tx("client", stdout_channel_tx);
            table.add_client_terminal_channel_cancellation_token("client", terminal_token.clone());
        }
        let mut bus = ClientConnectionBus::new("client", &connection_table);
        bus.send_stdout("one".to_owned());
        assert!(!terminal_token.is_cancelled());
        bus.send_stdout("two".to_owned());
        assert!(terminal_token.is_cancelled());
    }

    #[test]
    fn a_second_control_channel_replaces_and_cancels_the_first() {
        let connection_table = connection_table_with_client("client");
        let first_token = CancellationToken::new();
        let second_token = CancellationToken::new();
        let (first_tx, _first_rx) = client_channel(256, 1024 * 1024);
        let (second_tx, _second_rx) = client_channel(256, 1024 * 1024);
        let mut table = connection_table.lock().unwrap();
        assert!(table.add_client_control_tx("client", first_tx, first_token.clone()));
        assert!(table.add_client_control_tx("client", second_tx, second_token.clone()));
        assert!(first_token.is_cancelled());
        assert!(!second_token.is_cancelled());
        // and a client that is not there cannot be given one
        let (third_tx, _third_rx) = client_channel(256, 1024 * 1024);
        assert!(!table.add_client_control_tx("nobody", third_tx, CancellationToken::new()));
    }

    #[test]
    fn only_the_first_terminal_websocket_can_claim_a_client_id() {
        let connection_table = connection_table_with_client("client");
        let mut table = connection_table.lock().unwrap();
        let (first_tx, _first_rx) = client_channel(4, 1024);
        let (second_tx, _second_rx) = client_channel(4, 1024);
        assert!(matches!(
            table.attach_terminal("client", true, first_tx),
            TerminalAttachOutcome::Attached(_)
        ));
        assert!(table.is_client_structured("client"));
        assert!(matches!(
            table.attach_terminal("client", false, second_tx),
            TerminalAttachOutcome::AlreadyAttached
        ));
        // the refused one did not change anything
        assert!(table.is_client_structured("client"));
        let (third_tx, _third_rx) = client_channel(4, 1024);
        assert!(matches!(
            table.attach_terminal("nobody", false, third_tx),
            TerminalAttachOutcome::UnknownClient
        ));
    }
}
