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
    pub theme: SetConfigPayloadTheme,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub(super) struct SetConfigPayloadTheme {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub background: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub foreground: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub black: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blue: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bright_black: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bright_blue: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bright_cyan: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bright_green: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bright_magenta: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bright_red: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bright_white: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bright_yellow: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor_accent: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cyan: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub green: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub magenta: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub red: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selection_background: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selection_foreground: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selection_inactive_background: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub white: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub yellow: Option<String>,
}

impl From<(&Config, &Options)> for SetConfigPayload {
    fn from((config, options): (&Config, &Options)) -> Self {
        let font = config.web_client.font.clone();

        let palette = config.theme_config(options.theme.as_ref());
        let theme_from_config = config.web_client.theme.as_ref();

        let mut theme = SetConfigPayloadTheme::default();

        theme.background = theme_from_config
            .and_then(|theme| theme.background.clone())
            .or_else(|| palette.map(|p| p.text_unselected.background.as_rgb_str()));
        theme.foreground = theme_from_config
            .and_then(|theme| theme.foreground.clone())
            .or_else(|| palette.map(|p| p.text_unselected.base.as_rgb_str()));
        theme.black = theme_from_config.and_then(|theme| theme.black.clone());
        theme.blue = theme_from_config.and_then(|theme| theme.blue.clone());
        theme.bright_black = theme_from_config.and_then(|theme| theme.bright_black.clone());
        theme.bright_blue = theme_from_config.and_then(|theme| theme.bright_blue.clone());
        theme.bright_cyan = theme_from_config.and_then(|theme| theme.bright_cyan.clone());
        theme.bright_green = theme_from_config.and_then(|theme| theme.bright_green.clone());
        theme.bright_magenta = theme_from_config.and_then(|theme| theme.bright_magenta.clone());
        theme.bright_red = theme_from_config.and_then(|theme| theme.bright_red.clone());
        theme.bright_white = theme_from_config.and_then(|theme| theme.bright_white.clone());
        theme.bright_yellow = theme_from_config.and_then(|theme| theme.bright_yellow.clone());
        theme.cursor = theme_from_config.and_then(|theme| theme.cursor.clone());
        theme.cursor_accent = theme_from_config.and_then(|theme| theme.cursor_accent.clone());
        theme.cyan = theme_from_config.and_then(|theme| theme.cyan.clone());
        theme.green = theme_from_config.and_then(|theme| theme.green.clone());
        theme.magenta = theme_from_config.and_then(|theme| theme.magenta.clone());
        theme.red = theme_from_config.and_then(|theme| theme.red.clone());
        theme.selection_background = theme_from_config
            .and_then(|theme| theme.selection_background.clone())
            .or_else(|| palette.map(|p| p.text_selected.background.as_rgb_str()));
        theme.selection_foreground = theme_from_config
            .and_then(|theme| theme.selection_foreground.clone())
            .or_else(|| palette.map(|p| p.text_selected.base.as_rgb_str()));
        theme.selection_inactive_background =
            theme_from_config.and_then(|theme| theme.selection_inactive_background.clone());
        theme.white = theme_from_config.and_then(|theme| theme.white.clone());
        theme.yellow = theme_from_config.and_then(|theme| theme.yellow.clone());

        SetConfigPayload { font, theme }
    }
}
