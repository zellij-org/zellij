use serde::{Deserialize, Serialize};
use zellij_utils::{input::config::Config, pane_size::Size};

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
    pub cursor_blink: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor_inactive_style: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor_style: Option<String>,
    pub mac_option_is_meta: bool,
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

impl From<&Config> for SetConfigPayload {
    fn from(config: &Config) -> Self {
        let font = config.web_client.font.clone();

        let palette = config.theme_config(config.options.theme.as_ref());
        let web_client_theme_from_config = config.web_client.theme.as_ref();

        let mut theme = SetConfigPayloadTheme::default();

        theme.background = web_client_theme_from_config
            .and_then(|theme| theme.background.clone())
            .or_else(|| palette.map(|p| p.text_unselected.background.as_rgb_str()));
        theme.foreground = web_client_theme_from_config
            .and_then(|theme| theme.foreground.clone())
            .or_else(|| palette.map(|p| p.text_unselected.base.as_rgb_str()));
        theme.black = web_client_theme_from_config.and_then(|theme| theme.black.clone());
        theme.blue = web_client_theme_from_config.and_then(|theme| theme.blue.clone());
        theme.bright_black =
            web_client_theme_from_config.and_then(|theme| theme.bright_black.clone());
        theme.bright_blue =
            web_client_theme_from_config.and_then(|theme| theme.bright_blue.clone());
        theme.bright_cyan =
            web_client_theme_from_config.and_then(|theme| theme.bright_cyan.clone());
        theme.bright_green =
            web_client_theme_from_config.and_then(|theme| theme.bright_green.clone());
        theme.bright_magenta =
            web_client_theme_from_config.and_then(|theme| theme.bright_magenta.clone());
        theme.bright_red = web_client_theme_from_config.and_then(|theme| theme.bright_red.clone());
        theme.bright_white =
            web_client_theme_from_config.and_then(|theme| theme.bright_white.clone());
        theme.bright_yellow =
            web_client_theme_from_config.and_then(|theme| theme.bright_yellow.clone());
        theme.cursor = web_client_theme_from_config.and_then(|theme| theme.cursor.clone());
        theme.cursor_accent =
            web_client_theme_from_config.and_then(|theme| theme.cursor_accent.clone());
        theme.cyan = web_client_theme_from_config.and_then(|theme| theme.cyan.clone());
        theme.green = web_client_theme_from_config.and_then(|theme| theme.green.clone());
        theme.magenta = web_client_theme_from_config.and_then(|theme| theme.magenta.clone());
        theme.red = web_client_theme_from_config.and_then(|theme| theme.red.clone());
        theme.selection_background = web_client_theme_from_config
            .and_then(|theme| theme.selection_background.clone())
            .or_else(|| palette.map(|p| p.text_selected.background.as_rgb_str()));
        theme.selection_foreground = web_client_theme_from_config
            .and_then(|theme| theme.selection_foreground.clone())
            .or_else(|| palette.map(|p| p.text_selected.base.as_rgb_str()));
        theme.selection_inactive_background = web_client_theme_from_config
            .and_then(|theme| theme.selection_inactive_background.clone());
        theme.white = web_client_theme_from_config.and_then(|theme| theme.white.clone());
        theme.yellow = web_client_theme_from_config.and_then(|theme| theme.yellow.clone());

        let cursor_blink = config.web_client.cursor_blink;
        let mac_option_is_meta = config.web_client.mac_option_is_meta;
        let cursor_style = config
            .web_client
            .cursor_style
            .as_ref()
            .map(|s| s.to_string());
        let cursor_inactive_style = config
            .web_client
            .cursor_inactive_style
            .as_ref()
            .map(|s| s.to_string());

        SetConfigPayload {
            font,
            theme,
            cursor_blink,
            mac_option_is_meta,
            cursor_style,
            cursor_inactive_style,
        }
    }
}
