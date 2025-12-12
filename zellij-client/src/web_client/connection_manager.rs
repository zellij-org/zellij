use crate::os_input_output::ClientOsApi;
use crate::web_client::control_message::WebServerToWebClientControlMessage;
use crate::web_client::types::{ClientChannels, ClientConnectionBus, ConnectionTable};
use axum::extract::ws::{CloseFrame, Message};
use tokio::sync::mpsc::UnboundedSender;
use tokio_util::sync::CancellationToken;

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
        let close_frame = CloseFrame {
            code: axum::extract::ws::close_code::NORMAL,
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
