use std::net::IpAddr;
use zellij_tile::prelude::*;

use crate::CoordinatesInLine;
use std::collections::HashMap;

pub const USAGE_TITLE: &str = "How it works:";
pub const FIRST_TIME_USAGE_TITLE: &str = "Before logging in for the first time:";
pub const FIRST_TIME_BULLETIN_1: &str = "- Press <t> to generate a login token";
pub const BULLETIN_1_FULL: &str = "- Visit base URL to start a new session";
pub const BULLETIN_1_SHORT: &str = "- Base URL: new session";
pub const BULLETIN_2_FULL: &str = "- Follow base URL with a session name to attach to or create it";
pub const BULLETIN_2_SHORT: &str = "- Base URL + session name: attach or create";
pub const BULLETIN_3_FULL: &str =
    "- By default sessions not started from the web must be explicitly shared";
pub const BULLETIN_3_SHORT: &str = "- Sessions not started from the web must be explicitly shared";
pub const BULLETIN_4: &str = "- <t> manage login tokens";

pub const WEB_SERVER_TITLE: &str = "Web server: ";
pub const WEB_SERVER_RUNNING: &str = "RUNNING ";
pub const WEB_SERVER_NOT_RUNNING: &str = "NOT RUNNING";
pub const WEB_SERVER_INCOMPATIBLE_PREFIX: &str = "RUNNING INCOMPATIBLE VERSION ";
pub const CTRL_C_STOP: &str = "(<Ctrl c> - Stop)";
pub const CTRL_C_STOP_OTHER: &str = "<Ctrl c> - Stop other server";
pub const PRESS_ENTER_START: &str = "Press <ENTER> to start";
pub const ERROR_PREFIX: &str = "ERROR: ";
pub const URL_TITLE: &str = "URL: ";
pub const UNENCRYPTED_MARKER: &str = " [*]";

pub const CURRENT_SESSION_TITLE: &str = "Current session: ";
pub const SESSION_URL_TITLE: &str = "Session URL: ";
pub const SHARING_STATUS: &str = "SHARING (<SPACE> - Stop Sharing)";
pub const SHARING_DISABLED: &str = "SHARING IS DISABLED";
pub const NOT_SHARING: &str = "NOT SHARING";
pub const PRESS_SPACE_SHARE: &str = "Press <SPACE> to share";
pub const WEB_SERVER_OFFLINE: &str = "...but web server is offline";

pub const COLOR_INDEX_0: usize = 0;
pub const COLOR_INDEX_1: usize = 1;
pub const COLOR_INDEX_2: usize = 2;
pub const COLOR_HIGHLIGHT: usize = 3;

#[derive(Debug, Clone)]
pub struct ColorRange {
    pub start: usize,
    pub end: usize,
    pub color: usize,
}

// TODO: move this API to zellij-tile
#[derive(Debug)]
pub struct ColoredTextBuilder {
    text: String,
    ranges: Vec<ColorRange>,
}

impl ColoredTextBuilder {
    pub fn new(text: String) -> Self {
        Self {
            text,
            ranges: Vec::new(),
        }
    }

    pub fn highlight_substring(mut self, substring: &str, color: usize) -> Self {
        if let Some(start) = self.text.find(substring) {
            let end = start + substring.chars().count();
            self.ranges.push(ColorRange { start, end, color });
        }
        self
    }

    pub fn highlight_range(mut self, start: usize, end: usize, color: usize) -> Self {
        self.ranges.push(ColorRange { start, end, color });
        self
    }

    pub fn highlight_from_start(mut self, start: usize, color: usize) -> Self {
        let end = self.text.chars().count();
        self.ranges.push(ColorRange { start, end, color });
        self
    }

    pub fn highlight_all(mut self, color: usize) -> Self {
        let end = self.text.chars().count();
        self.ranges.push(ColorRange {
            start: 0,
            end,
            color,
        });
        self
    }

    pub fn build(self) -> (Text, usize) {
        let length = self.text.chars().count();
        let mut text_component = Text::new(self.text);

        for range in self.ranges {
            text_component = text_component.color_range(range.color, range.start..range.end);
        }

        (text_component, length)
    }
}

// create titled text with different colors for title and value
fn create_titled_text(
    title: &str,
    value: &str,
    title_color: usize,
    value_color: usize,
) -> (Text, usize) {
    let full_text = format!("{}{}", title, value);
    ColoredTextBuilder::new(full_text)
        .highlight_range(0, title.chars().count(), title_color)
        .highlight_from_start(title.chars().count(), value_color)
        .build()
}

// to create text with a highlighted shortcut key
fn create_highlighted_shortcut(text: &str, shortcut: &str, color: usize) -> (Text, usize) {
    ColoredTextBuilder::new(text.to_string())
        .highlight_substring(shortcut, color)
        .build()
}

fn get_text_with_fallback(
    full_text: &'static str,
    short_text: &'static str,
    max_width: usize,
) -> &'static str {
    if full_text.chars().count() <= max_width {
        full_text
    } else {
        short_text
    }
}

fn calculate_max_length(texts: &[&str]) -> usize {
    texts
        .iter()
        .map(|text| text.chars().count())
        .max()
        .unwrap_or(0)
}

fn format_url_with_encryption_marker(base_url: &str, is_unencrypted: bool) -> String {
    if is_unencrypted {
        format!("{}{}", base_url, UNENCRYPTED_MARKER)
    } else {
        base_url.to_string()
    }
}

#[derive(Debug)]
pub struct Usage {
    has_login_tokens: bool,
    first_time_usage_title: &'static str,
    first_time_bulletin_1: &'static str,
    usage_title: &'static str,
    bulletin_1_full: &'static str,
    bulletin_1_short: &'static str,
    bulletin_2_full: &'static str,
    bulletin_2_short: &'static str,
    bulletin_3_full: &'static str,
    bulletin_3_short: &'static str,
    bulletin_4: &'static str,
}

impl Usage {
    pub fn new(has_login_tokens: bool) -> Self {
        Usage {
            has_login_tokens,
            usage_title: USAGE_TITLE,
            bulletin_1_full: BULLETIN_1_FULL,
            bulletin_1_short: BULLETIN_1_SHORT,
            bulletin_2_full: BULLETIN_2_FULL,
            bulletin_2_short: BULLETIN_2_SHORT,
            bulletin_3_full: BULLETIN_3_FULL,
            bulletin_3_short: BULLETIN_3_SHORT,
            bulletin_4: BULLETIN_4,
            first_time_usage_title: FIRST_TIME_USAGE_TITLE,
            first_time_bulletin_1: FIRST_TIME_BULLETIN_1,
        }
    }

    pub fn usage_width_and_height(&self, max_width: usize) -> (usize, usize) {
        if self.has_login_tokens {
            self.full_usage_width_and_height(max_width)
        } else {
            self.first_time_usage_width_and_height(max_width)
        }
    }

    pub fn full_usage_width_and_height(&self, max_width: usize) -> (usize, usize) {
        let bulletin_1 =
            get_text_with_fallback(self.bulletin_1_full, self.bulletin_1_short, max_width);
        let bulletin_2 =
            get_text_with_fallback(self.bulletin_2_full, self.bulletin_2_short, max_width);
        let bulletin_3 =
            get_text_with_fallback(self.bulletin_3_full, self.bulletin_3_short, max_width);

        let texts = &[
            self.usage_title,
            bulletin_1,
            bulletin_2,
            bulletin_3,
            self.bulletin_4,
        ];
        let width = calculate_max_length(texts);
        let height = 5;
        (width, height)
    }

    pub fn first_time_usage_width_and_height(&self, _max_width: usize) -> (usize, usize) {
        let texts = &[self.first_time_usage_title, self.first_time_bulletin_1];
        let width = calculate_max_length(texts);
        let height = 2;
        (width, height)
    }

    pub fn render_usage(&self, x: usize, y: usize, max_width: usize) {
        if self.has_login_tokens {
            self.render_full_usage(x, y, max_width)
        } else {
            self.render_first_time_usage(x, y)
        }
    }

    pub fn render_full_usage(&self, x: usize, y: usize, max_width: usize) {
        let bulletin_1 =
            get_text_with_fallback(self.bulletin_1_full, self.bulletin_1_short, max_width);
        let bulletin_2 =
            get_text_with_fallback(self.bulletin_2_full, self.bulletin_2_short, max_width);
        let bulletin_3 =
            get_text_with_fallback(self.bulletin_3_full, self.bulletin_3_short, max_width);

        let usage_title = ColoredTextBuilder::new(self.usage_title.to_string())
            .highlight_all(COLOR_INDEX_2)
            .build()
            .0;

        let bulletin_1_text = Text::new(bulletin_1);
        let bulletin_2_text = Text::new(bulletin_2);
        let bulletin_3_text = Text::new(bulletin_3);

        let bulletin_4_text =
            create_highlighted_shortcut(self.bulletin_4, "<t>", COLOR_HIGHLIGHT).0;

        let texts_and_positions = vec![
            (usage_title, y),
            (bulletin_1_text, y + 1),
            (bulletin_2_text, y + 2),
            (bulletin_3_text, y + 3),
            (bulletin_4_text, y + 4),
        ];

        for (text, y_pos) in texts_and_positions {
            print_text_with_coordinates(text, x, y_pos, None, None);
        }
    }

    pub fn render_first_time_usage(&self, x: usize, y: usize) {
        let usage_title = ColoredTextBuilder::new(self.first_time_usage_title.to_string())
            .highlight_all(COLOR_INDEX_1)
            .build()
            .0;

        let bulletin_1 =
            create_highlighted_shortcut(self.first_time_bulletin_1, "<t>", COLOR_HIGHLIGHT).0;

        print_text_with_coordinates(usage_title, x, y, None, None);
        print_text_with_coordinates(bulletin_1, x, y + 1, None, None);
    }
}

#[derive(Debug)]
pub struct WebServerStatusSection {
    web_server_started: bool,
    web_server_base_url: String,
    web_server_error: Option<String>,
    web_server_different_version_error: Option<String>,
    connection_is_unencrypted: bool,
    pub clickable_urls: HashMap<CoordinatesInLine, String>,
    pub currently_hovering_over_link: bool,
    pub currently_hovering_over_unencrypted: bool,
}

impl WebServerStatusSection {
    pub fn new(
        web_server_started: bool,
        web_server_error: Option<String>,
        web_server_different_version_error: Option<String>,
        web_server_base_url: String,
        connection_is_unencrypted: bool,
    ) -> Self {
        WebServerStatusSection {
            web_server_started,
            clickable_urls: HashMap::new(),
            currently_hovering_over_link: false,
            currently_hovering_over_unencrypted: false,
            web_server_error,
            web_server_different_version_error,
            web_server_base_url,
            connection_is_unencrypted,
        }
    }

    pub fn web_server_status_width_and_height(&self) -> (usize, usize) {
        let mut max_len = self.web_server_status_line().1;

        if let Some(error) = &self.web_server_error {
            max_len = std::cmp::max(max_len, self.web_server_error_component(error).1);
        } else if let Some(different_version) = &self.web_server_different_version_error {
            max_len = std::cmp::max(
                max_len,
                self.web_server_different_version_error_component(different_version)
                    .1,
            );
        } else if self.web_server_started {
            let url_display = format_url_with_encryption_marker(
                &self.web_server_base_url,
                self.connection_is_unencrypted,
            );
            max_len = std::cmp::max(
                max_len,
                URL_TITLE.chars().count() + url_display.chars().count(),
            );
        } else {
            max_len = std::cmp::max(max_len, self.start_server_line().1);
        }

        (max_len, 2)
    }

    pub fn render_web_server_status(
        &mut self,
        x: usize,
        y: usize,
        hover_coordinates: Option<(usize, usize)>,
    ) {
        let web_server_status_line = self.web_server_status_line().0;
        print_text_with_coordinates(web_server_status_line, x, y, None, None);

        if let Some(error) = &self.web_server_error {
            let error_component = self.web_server_error_component(error).0;
            print_text_with_coordinates(error_component, x, y + 1, None, None);
        } else if let Some(different_version) = &self.web_server_different_version_error {
            let version_error_component = self
                .web_server_different_version_error_component(different_version)
                .0;
            print_text_with_coordinates(version_error_component, x, y + 1, None, None);
        } else if self.web_server_started {
            self.render_server_url(x, y, hover_coordinates);
        } else {
            let info_line = self.start_server_line().0;
            print_text_with_coordinates(info_line, x, y + 1, None, None);
        }
    }

    fn render_server_url(&mut self, x: usize, y: usize, hover_coordinates: Option<(usize, usize)>) {
        let server_url = &self.web_server_base_url;
        let url_x = x + URL_TITLE.chars().count();
        let url_width = server_url.chars().count();
        let url_y = y + 1;

        self.clickable_urls.insert(
            CoordinatesInLine::new(url_x, url_y, url_width),
            server_url.clone(),
        );

        let info_line = if self.connection_is_unencrypted {
            let full_text = format!("{}{}{}", URL_TITLE, server_url, UNENCRYPTED_MARKER);
            ColoredTextBuilder::new(full_text)
                .highlight_range(0, URL_TITLE.chars().count(), COLOR_INDEX_0)
                .highlight_substring(UNENCRYPTED_MARKER, COLOR_INDEX_1)
                .build()
                .0
        } else {
            create_titled_text(URL_TITLE, server_url, COLOR_INDEX_0, COLOR_INDEX_1).0
        };

        print_text_with_coordinates(info_line, x, y + 1, None, None);

        if hovering_on_line(url_x, url_y, url_width, hover_coordinates) {
            self.currently_hovering_over_link = true;
            render_text_with_underline(url_x, url_y, server_url);
        }
    }

    fn web_server_status_line(&self) -> (Text, usize) {
        if self.web_server_started {
            self.create_running_status_line()
        } else if let Some(different_version) = &self.web_server_different_version_error {
            self.create_incompatible_version_line(different_version)
        } else {
            create_titled_text(
                WEB_SERVER_TITLE,
                WEB_SERVER_NOT_RUNNING,
                COLOR_INDEX_0,
                COLOR_HIGHLIGHT,
            )
        }
    }

    fn create_running_status_line(&self) -> (Text, usize) {
        let full_text = format!("{}{}{}", WEB_SERVER_TITLE, WEB_SERVER_RUNNING, CTRL_C_STOP);
        ColoredTextBuilder::new(full_text)
            .highlight_range(0, WEB_SERVER_TITLE.chars().count(), COLOR_INDEX_0)
            .highlight_substring(WEB_SERVER_RUNNING.trim(), COLOR_HIGHLIGHT)
            .highlight_substring("<Ctrl c>", COLOR_HIGHLIGHT)
            .build()
    }

    fn create_incompatible_version_line(&self, different_version: &str) -> (Text, usize) {
        let value = format!("{}{}", WEB_SERVER_INCOMPATIBLE_PREFIX, different_version);
        create_titled_text(WEB_SERVER_TITLE, &value, COLOR_INDEX_0, COLOR_HIGHLIGHT)
    }

    fn start_server_line(&self) -> (Text, usize) {
        create_highlighted_shortcut(PRESS_ENTER_START, "<ENTER>", COLOR_HIGHLIGHT)
    }

    fn web_server_error_component(&self, error: &str) -> (Text, usize) {
        let text = format!("{}{}", ERROR_PREFIX, error);
        ColoredTextBuilder::new(text)
            .highlight_all(COLOR_HIGHLIGHT)
            .build()
    }

    fn web_server_different_version_error_component(&self, _version: &str) -> (Text, usize) {
        create_highlighted_shortcut(CTRL_C_STOP_OTHER, "<Ctrl c>", COLOR_HIGHLIGHT)
    }
}

#[derive(Debug)]
pub struct CurrentSessionSection {
    web_server_started: bool,
    web_server_ip: Option<IpAddr>,
    web_server_port: Option<u16>,
    web_sharing: WebSharing,
    session_name: Option<String>,
    connection_is_unencrypted: bool,
    pub clickable_urls: HashMap<CoordinatesInLine, String>,
    pub currently_hovering_over_link: bool,
}

impl CurrentSessionSection {
    pub fn new(
        web_server_started: bool,
        web_server_ip: Option<IpAddr>,
        web_server_port: Option<u16>,
        session_name: Option<String>,
        web_sharing: WebSharing,
        connection_is_unencrypted: bool,
    ) -> Self {
        CurrentSessionSection {
            web_server_started,
            web_server_ip,
            web_server_port,
            session_name,
            web_sharing,
            clickable_urls: HashMap::new(),
            currently_hovering_over_link: false,
            connection_is_unencrypted,
        }
    }

    pub fn current_session_status_width_and_height(&self) -> (usize, usize) {
        let mut max_len = self.get_session_status_line_length();

        if self.web_sharing.web_clients_allowed() && self.web_server_started {
            let url_display = format_url_with_encryption_marker(
                &self.session_url(),
                self.connection_is_unencrypted,
            );
            max_len = std::cmp::max(
                max_len,
                SESSION_URL_TITLE.chars().count() + url_display.chars().count(),
            );
        } else if self.web_sharing.web_clients_allowed() {
            max_len = std::cmp::max(max_len, WEB_SERVER_OFFLINE.chars().count());
        } else {
            max_len = std::cmp::max(max_len, self.press_space_to_share().1);
        }

        (max_len, 2)
    }

    fn get_session_status_line_length(&self) -> usize {
        match self.web_sharing {
            WebSharing::On => self.render_current_session_sharing().1,
            WebSharing::Disabled => self.render_sharing_is_disabled().1,
            WebSharing::Off => self.render_not_sharing().1,
        }
    }

    pub fn render_current_session_status(
        &mut self,
        x: usize,
        y: usize,
        hover_coordinates: Option<(usize, usize)>,
    ) {
        let status_line = match self.web_sharing {
            WebSharing::On => self.render_current_session_sharing().0,
            WebSharing::Disabled => self.render_sharing_is_disabled().0,
            WebSharing::Off => self.render_not_sharing().0,
        };

        print_text_with_coordinates(status_line, x, y, None, None);

        if self.web_sharing.web_clients_allowed() && self.web_server_started {
            self.render_session_url(x, y, hover_coordinates);
        } else if self.web_sharing.web_clients_allowed() {
            let info_line = Text::new(WEB_SERVER_OFFLINE);
            print_text_with_coordinates(info_line, x, y + 1, None, None);
        } else if !self.web_sharing.sharing_is_disabled() {
            let info_line = self.press_space_to_share().0;
            print_text_with_coordinates(info_line, x, y + 1, None, None);
        }
    }

    fn render_session_url(
        &mut self,
        x: usize,
        y: usize,
        hover_coordinates: Option<(usize, usize)>,
    ) {
        let session_url = self.session_url();
        let url_x = x + SESSION_URL_TITLE.chars().count();
        let url_width = session_url.chars().count();
        let url_y = y + 1;

        self.clickable_urls.insert(
            CoordinatesInLine::new(url_x, url_y, url_width),
            session_url.clone(),
        );

        let info_line = if self.connection_is_unencrypted {
            let full_text = format!("{}{}{}", SESSION_URL_TITLE, session_url, UNENCRYPTED_MARKER);
            ColoredTextBuilder::new(full_text)
                .highlight_range(0, SESSION_URL_TITLE.chars().count(), COLOR_INDEX_0)
                .highlight_substring(UNENCRYPTED_MARKER, COLOR_INDEX_1)
                .build()
                .0
        } else {
            create_titled_text(
                SESSION_URL_TITLE,
                &session_url,
                COLOR_INDEX_0,
                COLOR_INDEX_1,
            )
            .0
        };

        print_text_with_coordinates(info_line, x, y + 1, None, None);

        if hovering_on_line(url_x, url_y, url_width, hover_coordinates) {
            self.currently_hovering_over_link = true;
            render_text_with_underline(url_x, url_y, &session_url);
        }
    }

    fn render_current_session_sharing(&self) -> (Text, usize) {
        let full_text = format!("{}{}", CURRENT_SESSION_TITLE, SHARING_STATUS);
        ColoredTextBuilder::new(full_text)
            .highlight_range(0, CURRENT_SESSION_TITLE.chars().count(), COLOR_INDEX_0)
            .highlight_substring("SHARING", COLOR_HIGHLIGHT)
            .highlight_substring("<SPACE>", COLOR_HIGHLIGHT)
            .build()
    }

    fn render_sharing_is_disabled(&self) -> (Text, usize) {
        create_titled_text(
            CURRENT_SESSION_TITLE,
            SHARING_DISABLED,
            COLOR_INDEX_0,
            COLOR_HIGHLIGHT,
        )
    }

    fn render_not_sharing(&self) -> (Text, usize) {
        create_titled_text(
            CURRENT_SESSION_TITLE,
            NOT_SHARING,
            COLOR_INDEX_0,
            COLOR_HIGHLIGHT,
        )
    }

    fn session_url(&self) -> String {
        let web_server_ip = self
            .web_server_ip
            .map(|i| i.to_string())
            .unwrap_or_else(|| "UNDEFINED".to_owned());
        let web_server_port = self
            .web_server_port
            .map(|p| p.to_string())
            .unwrap_or_else(|| "UNDEFINED".to_owned());
        let prefix = if self.connection_is_unencrypted {
            "http"
        } else {
            "https"
        };
        let session_name = self.session_name.as_deref().unwrap_or("");

        format!(
            "{}://{}:{}/{}",
            prefix, web_server_ip, web_server_port, session_name
        )
    }

    fn press_space_to_share(&self) -> (Text, usize) {
        create_highlighted_shortcut(PRESS_SPACE_SHARE, "<SPACE>", COLOR_HIGHLIGHT)
    }
}

pub fn hovering_on_line(
    x: usize,
    y: usize,
    width: usize,
    hover_coordinates: Option<(usize, usize)>,
) -> bool {
    match hover_coordinates {
        Some((hover_x, hover_y)) => hover_y == y && hover_x <= x + width && hover_x > x,
        None => false,
    }
}

pub fn render_text_with_underline(url_x: usize, url_y: usize, url_text: &str) {
    print!(
        "\u{1b}[{};{}H\u{1b}[m\u{1b}[1;4m{}",
        url_y + 1,
        url_x + 1,
        url_text,
    );
}
