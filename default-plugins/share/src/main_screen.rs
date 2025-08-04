use super::CoordinatesInLine;
use crate::ui_components::{
    hovering_on_line, render_text_with_underline, CurrentSessionSection, Usage,
    WebServerStatusSection,
};
use zellij_tile::prelude::*;

use std::collections::HashMap;
use url::Url;

pub struct MainScreenState {
    pub currently_hovering_over_link: bool,
    pub currently_hovering_over_unencrypted: bool,
    pub clickable_urls: HashMap<CoordinatesInLine, String>,
}

pub struct MainScreen<'a> {
    token_list_is_empty: bool,
    web_server_started: bool,
    web_server_error: &'a Option<String>,
    web_server_different_version_error: &'a Option<String>,
    web_server_base_url: &'a String,
    web_server_ip: Option<std::net::IpAddr>,
    web_server_port: Option<u16>,
    session_name: &'a Option<String>,
    web_sharing: WebSharing,
    hover_coordinates: Option<(usize, usize)>,
    info: &'a Option<String>,
    link_executable: &'a Option<&'a str>,
}

impl<'a> MainScreen<'a> {
    const TITLE_TEXT: &'static str = "Share Session Locally in the Browser";
    const WARNING_TEXT: &'static str =
        "[*] Connection unencrypted. Consider using an SSL certificate.";
    const MORE_INFO_TEXT: &'static str = "More info: ";
    const SSL_URL: &'static str = "https://zellij.dev/documentation/web-client.html#https";
    const HELP_TEXT_WITH_CLICK: &'static str = "Help: Click or Shift-Click to open in browser";
    const HELP_TEXT_SHIFT_ONLY: &'static str = "Help: Shift-Click to open in browser";
    pub fn new(
        token_list_is_empty: bool,
        web_server_started: bool,
        web_server_error: &'a Option<String>,
        web_server_different_version_error: &'a Option<String>,
        web_server_base_url: &'a String,
        web_server_ip: Option<std::net::IpAddr>,
        web_server_port: Option<u16>,
        session_name: &'a Option<String>,
        web_sharing: WebSharing,
        hover_coordinates: Option<(usize, usize)>,
        info: &'a Option<String>,
        link_executable: &'a Option<&'a str>,
    ) -> Self {
        Self {
            token_list_is_empty,
            web_server_started,
            web_server_error,
            web_server_different_version_error,
            web_server_base_url,
            web_server_ip,
            web_server_port,
            session_name,
            web_sharing,
            hover_coordinates,
            info,
            link_executable,
        }
    }

    pub fn render(self, rows: usize, cols: usize) -> MainScreenState {
        let mut state = MainScreenState {
            currently_hovering_over_link: false,
            currently_hovering_over_unencrypted: false,
            clickable_urls: HashMap::new(),
        };

        let layout = self.calculate_layout(rows, cols);
        self.render_content(&layout, &mut state);

        state
    }

    fn calculate_layout(&self, rows: usize, cols: usize) -> Layout {
        let usage = Usage::new(!self.token_list_is_empty);
        let web_server_status_section = WebServerStatusSection::new(
            self.web_server_started,
            self.web_server_error.clone(),
            self.web_server_different_version_error.clone(),
            self.web_server_base_url.clone(),
            self.connection_is_unencrypted(),
        );
        let current_session_section = CurrentSessionSection::new(
            self.web_server_started,
            self.web_server_ip,
            self.web_server_port,
            self.session_name.clone(),
            self.web_sharing,
            self.connection_is_unencrypted(),
        );

        let title_width = Self::TITLE_TEXT.chars().count();

        let (web_server_width, web_server_height) =
            web_server_status_section.web_server_status_width_and_height();
        let (current_session_width, current_session_height) =
            current_session_section.current_session_status_width_and_height();
        let (usage_width, usage_height) = usage.usage_width_and_height(cols);

        let mut max_width = title_width
            .max(web_server_width)
            .max(current_session_width)
            .max(usage_width);

        let mut total_height =
            2 + web_server_height + 1 + current_session_height + 1 + usage_height;

        if self.connection_is_unencrypted() {
            let warning_width = self.unencrypted_warning_width();
            max_width = max_width.max(warning_width);
            total_height += 3;
        }

        Layout {
            base_x: cols.saturating_sub(max_width) / 2,
            base_y: rows.saturating_sub(total_height) / 2,
            title_text: Self::TITLE_TEXT,
            web_server_height,
            usage_height,
            usage_width,
        }
    }

    fn render_content(&self, layout: &Layout, state: &mut MainScreenState) {
        let mut current_y = layout.base_y;

        self.render_title(layout, current_y);
        current_y += 2;

        current_y = self.render_web_server_section(layout, current_y, state);
        current_y = self.render_current_session_section(layout, current_y, state);
        current_y = self.render_usage_section(layout, current_y);
        current_y = self.render_warnings_and_help(layout, current_y, state);

        self.render_info(layout, current_y);
    }

    fn render_title(&self, layout: &Layout, y: usize) {
        let title = Text::new(layout.title_text).color_range(2, ..);
        print_text_with_coordinates(title, layout.base_x, y, None, None);
    }

    fn render_web_server_section(
        &self,
        layout: &Layout,
        y: usize,
        state: &mut MainScreenState,
    ) -> usize {
        let mut web_server_status_section = WebServerStatusSection::new(
            self.web_server_started,
            self.web_server_error.clone(),
            self.web_server_different_version_error.clone(),
            self.web_server_base_url.clone(),
            self.connection_is_unencrypted(),
        );

        web_server_status_section.render_web_server_status(
            layout.base_x,
            y,
            self.hover_coordinates,
        );

        state.currently_hovering_over_link |=
            web_server_status_section.currently_hovering_over_link;
        state.currently_hovering_over_unencrypted |=
            web_server_status_section.currently_hovering_over_unencrypted;

        for (coordinates, url) in web_server_status_section.clickable_urls {
            state.clickable_urls.insert(coordinates, url);
        }

        y + layout.web_server_height + 1
    }

    fn render_current_session_section(
        &self,
        layout: &Layout,
        y: usize,
        state: &mut MainScreenState,
    ) -> usize {
        let mut current_session_section = CurrentSessionSection::new(
            self.web_server_started,
            self.web_server_ip,
            self.web_server_port,
            self.session_name.clone(),
            self.web_sharing,
            self.connection_is_unencrypted(),
        );

        current_session_section.render_current_session_status(
            layout.base_x,
            y,
            self.hover_coordinates,
        );

        state.currently_hovering_over_link |= current_session_section.currently_hovering_over_link;

        for (coordinates, url) in current_session_section.clickable_urls {
            state.clickable_urls.insert(coordinates, url);
        }

        y + layout.web_server_height + 1
    }

    fn render_usage_section(&self, layout: &Layout, y: usize) -> usize {
        let usage = Usage::new(!self.token_list_is_empty);
        usage.render_usage(layout.base_x, y, layout.usage_width);
        y + layout.usage_height + 1
    }

    fn render_warnings_and_help(
        &self,
        layout: &Layout,
        mut y: usize,
        state: &mut MainScreenState,
    ) -> usize {
        if self.connection_is_unencrypted() && self.web_server_started {
            self.render_unencrypted_warning(layout.base_x, y, state);
            y += 3;
        }

        if state.currently_hovering_over_link {
            self.render_link_help(layout.base_x, y);
            y += 3;
        }

        y
    }

    fn render_info(&self, layout: &Layout, y: usize) {
        if let Some(info) = self.info {
            let info_text = Text::new(info).color_range(1, ..);
            print_text_with_coordinates(info_text, layout.base_x, y, None, None);
        }
    }

    fn unencrypted_warning_width(&self) -> usize {
        let more_info_line = format!("{}{}", Self::MORE_INFO_TEXT, Self::SSL_URL);
        std::cmp::max(
            Self::WARNING_TEXT.chars().count(),
            more_info_line.chars().count(),
        )
    }

    fn render_unencrypted_warning(&self, x: usize, y: usize, state: &mut MainScreenState) {
        let warning_text = Text::new(Self::WARNING_TEXT).color_range(1, ..3);
        let more_info_line = Text::new(format!("{}{}", Self::MORE_INFO_TEXT, Self::SSL_URL));

        let url_x = x + Self::MORE_INFO_TEXT.chars().count();
        let url_y = y + 1;
        let url_width = Self::SSL_URL.chars().count();

        state.clickable_urls.insert(
            CoordinatesInLine::new(url_x, url_y, url_width),
            Self::SSL_URL.to_owned(),
        );

        print_text_with_coordinates(warning_text, x, y, None, None);
        print_text_with_coordinates(more_info_line, x, y + 1, None, None);

        if hovering_on_line(url_x, url_y, url_width, self.hover_coordinates) {
            state.currently_hovering_over_link = true;
            render_text_with_underline(url_x, url_y, Self::SSL_URL);
        }
    }

    fn render_link_help(&self, x: usize, y: usize) {
        let help_text = if self.link_executable.is_some() {
            Text::new(Self::HELP_TEXT_WITH_CLICK)
                .color_range(3, 6..=10)
                .color_range(3, 15..=25)
        } else {
            Text::new(Self::HELP_TEXT_SHIFT_ONLY).color_range(3, 6..=16)
        };
        print_text_with_coordinates(help_text, x, y, None, None);
    }
    pub fn connection_is_unencrypted(&self) -> bool {
        Url::parse(&self.web_server_base_url)
            .ok()
            .map(|b| b.scheme() == "http")
            .unwrap_or(false)
    }
}

struct Layout<'a> {
    base_x: usize,
    base_y: usize,
    title_text: &'a str,
    web_server_height: usize,
    usage_height: usize,
    usage_width: usize,
}
