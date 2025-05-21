use serde::{Deserialize, Serialize};
use zellij_utils::pane_size::Size;

#[derive(Serialize, Deserialize, Debug, Clone)]

pub(super) struct WebClientToWebServerControlMessage {
    pub web_client_id: String,
    pub payload: WebClientToWebServerControlMessagePayload,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type")]
pub(super) enum WebClientToWebServerControlMessagePayload {
    TerminalResize(Size),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type")]
pub(super) enum WebServerToWebClientControlMessage {
    SetConfig { font: String },
    QueryTerminalSize,
    Log { lines: Vec<String> },
    LogError { lines: Vec<String> },
    SwitchedSession { new_session_name: String },
}
