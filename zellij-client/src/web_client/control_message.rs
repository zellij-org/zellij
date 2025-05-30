use serde::{Deserialize, Serialize};
use zellij_utils::{
    input::{config::Config, options::Options},
    pane_size::Size,
};

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
    SetConfig(SetConfigPayload),
    QueryTerminalSize,
    Log { lines: Vec<String> },
    LogError { lines: Vec<String> },
    SwitchedSession { new_session_name: String },
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub(super) struct SetConfigPayload {
    pub font: String,
    pub background: Option<String>,
    pub foreground: Option<String>,
}

impl From<(&Config, &Options)> for SetConfigPayload {
    fn from((config, options): (&Config, &Options)) -> Self {
        let font = config.web_client.font.clone();

        let palette = config.theme_config(options.theme.as_ref());
        let theme = config.web_client.theme.as_ref();

        let background = theme
            .and_then(|theme| theme.background.clone())
            .or_else(|| palette.map(|p| p.text_unselected.background.as_rgb_str()));

        let foreground = theme
            .and_then(|theme| theme.foreground.clone())
            .or_else(|| palette.map(|p| p.text_unselected.base.as_rgb_str()));

        SetConfigPayload {
            font,
            background,
            foreground,
        }
    }
}
