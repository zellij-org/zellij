use std::net::IpAddr;
use zellij_tile::prelude::*;

use std::collections::{BTreeMap, HashMap};
use crate::CoordinatesInLine;

#[derive(Debug)]
pub struct Usage {
    usage_title: &'static str,
    bulletin_1_full: &'static str,
    bulletin_1_short: &'static str,
    bulletin_2_full: &'static str,
    bulletin_2_short: &'static str,
    bulletin_3_full: &'static str,
    bulletin_3_short: &'static str,
}

impl Usage {
    pub fn new() -> Self {
        Usage {
            usage_title: "How it works:",
            bulletin_1_full: "- Visit base URL to start a new session",
            bulletin_1_short: "- Base URL: new session",
            bulletin_2_full: "- Follow base URL with a session name to attach to or create it",
            bulletin_2_short: "- Base URL + session name: attach or create",
            bulletin_3_full: "- Sessions must be explicitly shared unless specified otherwise in the config",
            bulletin_3_short: "- Sessions must be explicitly shared",
        }
    }
    pub fn usage_width_and_height(&self, max_width: usize) -> (usize, usize) {
        let mut max_len = 0;
        max_len = std::cmp::max(max_len, self.usage_title.chars().count());
        let bulletin_1 = if self.bulletin_1_full.chars().count() <= max_width {
            self.bulletin_1_full
        } else {
            self.bulletin_1_short
        };
        max_len = std::cmp::max(max_len, bulletin_1.chars().count());
        let bulletin_2 = if self.bulletin_2_full.chars().count() <= max_width {
            self.bulletin_2_full
        } else {
            self.bulletin_2_short
        };
        max_len = std::cmp::max(max_len, bulletin_2.chars().count());
        let bulletin_3 = if self.bulletin_3_full.chars().count() <= max_width {
            self.bulletin_3_full
        } else {
            self.bulletin_3_short
        };
        max_len = std::cmp::max(max_len, bulletin_3.chars().count());
        let width = max_len;
        let height = 4;
        (width, height)
    }
    pub fn render_usage(&self, x: usize, y: usize, max_width: usize) {
        let bulletin_1 = if self.bulletin_1_full.chars().count() <= max_width {
            self.bulletin_1_full
        } else {
            self.bulletin_1_short
        };
        let bulletin_2 = if self.bulletin_2_full.chars().count() <= max_width {
            self.bulletin_2_full
        } else {
            self.bulletin_2_short
        };
        let bulletin_3 = if self.bulletin_3_full.chars().count() <= max_width {
            self.bulletin_3_full
        } else {
            self.bulletin_3_short
        };
        let usage_title = Text::new(self.usage_title).color_range(2, ..);
        let bulletin_1 = Text::new(bulletin_1);
        let bulletin_2 = Text::new(bulletin_2);
        let bulletin_3 = Text::new(bulletin_3);
        print_text_with_coordinates(usage_title, x, y, None, None);
        print_text_with_coordinates(bulletin_1, x, y + 1, None, None);
        print_text_with_coordinates(bulletin_2, x, y + 2, None, None);
        print_text_with_coordinates(bulletin_3, x, y + 3, None, None);

    }
}

#[derive(Debug)]
pub struct WebServerStatusSection {
    web_server_started: bool,
    web_server_ip: Option<IpAddr>,
    web_server_port: Option<u16>,
    pub clickable_urls: HashMap<CoordinatesInLine, String>,
    pub currently_hovering_over_link: bool,
}

impl WebServerStatusSection {
    pub fn new(web_server_started: bool, web_server_ip: Option<IpAddr>, web_server_port: Option<u16>) -> Self {
        WebServerStatusSection {
            web_server_started,
            web_server_ip,
            web_server_port,
            clickable_urls: HashMap::new(),
            currently_hovering_over_link: false
        }
    }
    pub fn web_server_status_width_and_height(&self) -> (usize, usize) {
        let mut max_len = 0;
        let web_server_status_line_len = self.web_server_status_line().1;
        max_len = std::cmp::max(max_len, web_server_status_line_len);
        if self.web_server_started {
            let title = "URL: ";
            let value = self.server_url();
            max_len = std::cmp::max(max_len, title.chars().count() + value.chars().count());
        } else {
            let text_length = self.start_server_line().1;
            max_len = std::cmp::max(max_len, text_length);
        };
        let width = max_len;
        let height = 2;
        (width, height)
    }
    pub fn render_web_server_status(&mut self, x: usize, y: usize, hover_coordinates: Option<(usize, usize)>) {
        let web_server_status_line = self.web_server_status_line().0;
        print_text_with_coordinates(web_server_status_line, x, y, None, None);
        if self.web_server_started {
            let title = "URL: ";
            let server_url = self.server_url();
            let url_x = x + title.chars().count();
            let url_width = server_url.chars().count();
            let url_y = y + 1;
            self.clickable_urls.insert(CoordinatesInLine::new(url_x, url_y, url_width), server_url.clone());
            if hovering_on_line(url_x, url_y, url_width, hover_coordinates) {
                self.currently_hovering_over_link = true;
                let title = Text::new(title).color_range(0, ..title.chars().count());
                print_text_with_coordinates(title, x, y + 1, None, None);
                render_text_with_underline(url_x, url_y, server_url);
            } else {
                let info_line = Text::new(format!("{}{}", title, server_url)).color_range(0, ..title.chars().count());
                print_text_with_coordinates(info_line, x, y + 1, None, None);
            }
        } else {
            let info_line = self.start_server_line().0;
            print_text_with_coordinates(info_line, x, y + 1, None, None);
        };
    }
    fn web_server_status_line(&self) -> (Text, usize) { // (component, length)
        if self.web_server_started {
            let title = "Web server: ";
            let value = "RUNNING ";
            let shortcut = "(<Ctrl c> - Stop)";
            let value_start_position = title.chars().count();
            let value_end_position = value_start_position + value.chars().count();
            let ctrl_c_start_position = value_end_position + 1;
            let ctrl_c_end_position = ctrl_c_start_position + 8;
            (
                Text::new(format!("{}{}{}", title, value, shortcut))
                    .color_range(0, ..title.chars().count())
                    .color_range(3, value_start_position..value_end_position)
                    .color_range(3, ctrl_c_start_position..ctrl_c_end_position),
                title.chars().count() + value.chars().count() + shortcut.chars().count()
            )
        } else {
            let title = "Web server status: ";
            let value = "NOT RUNNING";
            (
                Text::new(format!("{}{}", title, value))
                    .color_range(0, ..title.chars().count())
                    .color_range(3, title.chars().count()..),
                title.chars().count() + value.chars().count()
            )
        }
    }
    fn server_url(&self) -> String {
        let web_server_ip = self
            .web_server_ip
            .map(|i| format!("{}", i))
            .unwrap_or("UNDEFINED".to_owned());
        let web_server_port = self
            .web_server_port
            .map(|p| format!("{}", p))
            .unwrap_or("UNDEFINED".to_owned());
        format!("http://{}:{}/", web_server_ip, web_server_port)
    }
    fn start_server_line(&self) -> (Text, usize) { // (component, length)
        let text = "Press <ENTER> to start";
        let length = text.chars().count();
        let component = Text::new(text).color_range(3, 6..=12);
        (component, length)
    }
}

#[derive(Debug)]
pub struct CurrentSessionSection {
    web_server_started: bool,
    web_server_ip: Option<IpAddr>,
    web_server_port: Option<u16>,
    web_sharing: WebSharing,
    session_name: Option<String>,
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
    ) -> Self {
        CurrentSessionSection {
            web_server_started,
            web_server_ip,
            web_server_port,
            session_name,
            web_sharing,
            clickable_urls: HashMap::new(),
            currently_hovering_over_link: false
        }
    }
    pub fn current_session_status_width_and_height(&self) -> (usize, usize) {
        let mut max_len = 0;
        match self.web_sharing {
            WebSharing::On => {
                let length = self.render_current_session_sharing().1;
                max_len = std::cmp::max(max_len, length);
            },
            WebSharing::Disabled => {
                let length = self.render_sharing_is_disabled().1;
                max_len = std::cmp::max(max_len, length);
            },
            WebSharing::Off => {
                let length = self.render_not_sharing().1;
                max_len = std::cmp::max(max_len, length);
            },
        };
        if self.web_sharing.web_clients_allowed() && self.web_server_started {
            let title = "Session URL: ";
            let session_url = self.session_url();
            max_len = std::cmp::max(max_len, title.chars().count() + session_url.chars().count());
        } else if self.web_sharing.web_clients_allowed() {
            let text = self.web_server_is_offline();
            max_len = std::cmp::max(max_len, text.chars().count());
        } else {
            let length = self.press_space_to_share().1;
            max_len = std::cmp::max(max_len, length);
        };
        let width = max_len;
        let height = 2;
        (width, height)
    }
    pub fn render_current_session_status(&mut self, x: usize, y: usize, hover_coordinates: Option<(usize, usize)>) {
        let status_line = match self.web_sharing {
            WebSharing::On => {
                self.render_current_session_sharing().0
            },
            WebSharing::Disabled => {
                self.render_sharing_is_disabled().0
            },
            WebSharing::Off => {
                self.render_not_sharing().0
            },
        };
        print_text_with_coordinates(status_line, x, y, None, None);
        if self.web_sharing.web_clients_allowed() && self.web_server_started {
            let title = "Session URL: ";
            let session_url = self.session_url();
            let url_x = x + title.chars().count();
            let url_width = session_url.chars().count();
            let url_y = y + 1;
            self.clickable_urls.insert(CoordinatesInLine::new(url_x, url_y, url_width), session_url.clone());
            if hovering_on_line(url_x, url_y, url_width, hover_coordinates) {
                self.currently_hovering_over_link = true;
                let title = Text::new(title).color_range(0, ..title.chars().count());
                print_text_with_coordinates(title, x, y + 1, None, None);
                render_text_with_underline(url_x, url_y, session_url);
            } else {
                let info_line = Text::new(format!("{}{}", title, session_url)).color_range(0, ..title.chars().count());
                print_text_with_coordinates(info_line, x, y + 1, None, None);
            }
        } else if self.web_sharing.web_clients_allowed() {
            let text = self.web_server_is_offline();
            let info_line = Text::new(text);
            print_text_with_coordinates(info_line, x, y + 1, None, None);
        } else {
            let info_line = self.press_space_to_share().0;
            print_text_with_coordinates(info_line, x, y + 1, None, None);
        }
    }
    fn render_current_session_sharing(&self) -> (Text, usize) { // (component, length)
        let title = "Current session: ";
        let value = "SHARING (<SPACE> - Stop Sharing)";
        let length = title.chars().count() + value.chars().count();
        let sharing_start_pos = title.chars().count();
        let sharing_end_pos = sharing_start_pos + 7;
        let space_start_position = sharing_end_pos + 2;
        let space_end_position = space_start_position + 6;
        let component = Text::new(format!("{}{}", title, value))
            .color_range(0, ..sharing_start_pos)
            .color_range(3, sharing_start_pos..sharing_end_pos)
            .color_range(3, space_start_position..=space_end_position);
        (component, length)
    }
    fn render_sharing_is_disabled(&self) -> (Text, usize) { // (component, length)
        let title = "Current session: ";
        let value = "SHARING IS DISABLED";
        let length = title.chars().count() + value.chars().count();
        let component = Text::new(format!("{}{}", title, value))
            .color_range(0, ..title.chars().count())
            .color_range(3, title.chars().count()..);
        (component, length)
    }
    fn render_not_sharing(&self) -> (Text, usize) { // (component, length)
        let title = "Current session: ";
        let value = "NOT SHARING";
        let length = title.chars().count() + value.chars().count();
        let component = Text::new(format!("{}{}", title, value))
            .color_range(0, ..title.chars().count())
            .color_range(3, title.chars().count()..);
        (component, length)
    }
    fn session_url(&self) -> String {
        let web_server_ip = self
            .web_server_ip
            .map(|i| format!("{}", i))
            .unwrap_or("UNDEFINED".to_owned());
        let web_server_port = self
            .web_server_port
            .map(|p| format!("{}", p))
            .unwrap_or("UNDEFINED".to_owned());
        format!(
            "http://{}:{}/{}",
            web_server_ip,
            web_server_port,
            self.session_name.clone().unwrap_or_else(|| "".to_owned())
        )
    }
    fn web_server_is_offline(&self) -> String {
        format!("...but web server is offline")
    }
    fn press_space_to_share(&self) -> (Text, usize) { // (component, length)
        let text = "Press <SPACE> to share";
        let length = text.chars().count();
        let component = Text::new(text).color_range(3, 6..=12);
        (component, length)
    }
}

fn hovering_on_line(x: usize, y: usize, width: usize, hover_coordinates: Option<(usize, usize)>) -> bool {
    match hover_coordinates {
        Some((hover_x, hover_y)) => {
            hover_y == y && hover_x <= x + width && hover_x > x
        },
        None => false
    }
}

fn render_text_with_underline(url_x: usize, url_y: usize, url_text: String) {
    print!(
        "\u{1b}[{};{}H\u{1b}[m\u{1b}[1;4m{}",
        url_y + 1,
        url_x + 1,
        url_text,
    );
}
