use crate::os_input_output::ClientOsApi;
use crate::web_client::control_message::WebServerToWebClientControlMessage;
use crate::web_client::types::{ClientChannels, ClientConnectionBus, ConnectionTable};
use axum::extract::ws::{CloseFrame, Message};
use std::sync::{atomic::AtomicBool, Arc};
use tokio::sync::mpsc::UnboundedSender;
use tokio_util::sync::CancellationToken;

impl ConnectionTable {
    pub fn add_new_client(
        &mut self,
        client_id: String,
        client_os_api: Box<dyn ClientOsApi>,
        is_read_only: bool,
        session_token_hash: String,
    ) {
        self.client_id_to_channels
            .insert(client_id.clone(), ClientChannels::new(client_os_api));
        self.client_read_only_status
            .insert(client_id.clone(), is_read_only);
        self.client_session_token_hash
            .insert(client_id, session_token_hash);
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

    pub fn add_client_control_tx(
        &mut self,
        client_id: &str,
        control_channel_tx: UnboundedSender<Message>,
    ) {
        self.client_id_to_channels
            .get_mut(client_id)
            .map(|c| c.add_control_tx(control_channel_tx));
    }

    pub fn queue_client_control_message(&mut self, client_id: &str, message: Message) {
        self.client_id_to_channels
            .get_mut(client_id)
            .map(|c| c.queue_control_message(message));
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
        self.client_session_token_hash.remove(client_id);
    }

    pub fn get_should_not_reconnect_flag(&self, client_id: &str) -> Option<Arc<AtomicBool>> {
        self.client_id_to_channels
            .get(client_id)
            .map(|c| c.should_not_reconnect.clone())
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
                    self.connection_table
                        .lock()
                        .unwrap()
                        .queue_client_control_message(&self.web_client_id, message);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::os_input_output::ClientOsApi;
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
        connection_table.lock().unwrap().add_new_client(
            client_id.to_owned(),
            Box::new(StubOsInput::default()),
            false,
            "token".to_owned(),
        );
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

        let (control_channel_tx, mut control_channel_rx) = tokio::sync::mpsc::unbounded_channel();
        connection_table
            .lock()
            .unwrap()
            .add_client_control_tx("client", control_channel_tx);

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
        let (control_channel_tx, mut control_channel_rx) = tokio::sync::mpsc::unbounded_channel();
        connection_table
            .lock()
            .unwrap()
            .add_client_control_tx("client", control_channel_tx);

        let mut bus = ClientConnectionBus::new("client", &connection_table);
        bus.send_control(WebServerToWebClientControlMessage::QueryTerminalSize);

        let message = control_channel_rx
            .try_recv()
            .expect("a registered control channel must receive messages directly");
        assert!(message_text(&message).contains("QueryTerminalSize"));
    }

    #[test]
    fn buffered_control_messages_are_bounded() {
        let connection_table = connection_table_with_client("client");
        let mut bus = ClientConnectionBus::new("client", &connection_table);

        for _ in 0..200 {
            bus.send_control(WebServerToWebClientControlMessage::QueryTerminalSize);
        }

        let (control_channel_tx, mut control_channel_rx) = tokio::sync::mpsc::unbounded_channel();
        connection_table
            .lock()
            .unwrap()
            .add_client_control_tx("client", control_channel_tx);

        let mut flushed = 0;
        while control_channel_rx.try_recv().is_ok() {
            flushed += 1;
        }
        assert_eq!(flushed, 64);
    }
}
