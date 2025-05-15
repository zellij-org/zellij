mod ui_components;
use std::net::IpAddr;
use zellij_tile::prelude::*;

use std::collections::{BTreeMap, HashMap};

use ui_components::{Usage, WebServerStatusSection, CurrentSessionSection};

#[derive(Debug, Default)]
struct App {
    web_server_started: bool,
    web_sharing: WebSharing,
    web_clients_allowed: bool,
    session_name: Option<String>,
    web_server_error: Option<String>,
    web_server_ip: Option<IpAddr>,
    web_server_port: Option<u16>,
    hover_coordinates: Option<(usize, usize)>, // x, y
    clickable_urls: HashMap<CoordinatesInLine, String>,
    link_executable: Option<&'static str>,
    currently_hovering_over_link: bool,
    own_plugin_id: Option<u32>,
    web_server_capability: bool,
}

register_plugin!(App);

impl ZellijPlugin for App {
    fn load(&mut self, _configuration: BTreeMap<String, String>) {
        subscribe(&[
            EventType::Key,
            EventType::ModeUpdate,
            EventType::WebServerStatus,
            EventType::Mouse,
            EventType::RunCommandResult,
        ]);
        self.own_plugin_id = Some(get_plugin_ids().plugin_id);
        self.query_link_executable();
        self.change_own_title();
    }
    fn update(&mut self, event: Event) -> bool {
        let mut should_render = false;
        match event {
            Event::ModeUpdate(mode_info) => {
                self.session_name = mode_info.session_name;
                if let Some(web_clients_allowed) = mode_info.web_clients_allowed {
                    self.web_clients_allowed = web_clients_allowed;
                    should_render = true;
                }
                if let Some(web_sharing) = mode_info.web_sharing {
                    self.web_sharing = web_sharing;
                    should_render = true;
                }
                if let Some(web_server_ip) = mode_info.web_server_ip {
                    self.web_server_ip = Some(web_server_ip);
                    should_render = true;
                }
                if let Some(web_server_port) = mode_info.web_server_port {
                    self.web_server_port = Some(web_server_port);
                    should_render = true;
                }
                if let Some(web_server_capability) = mode_info.web_server_capability {
                    self.web_server_capability = web_server_capability;
                    should_render = true;
                }
            },
            Event::WebServerStatus(web_server_status)=> {
                if !self.web_server_capability {
                    return false;
                }
                match web_server_status {
                    WebServerStatus::Online => {
                        self.web_server_started = true;
                        self.web_server_error = None;
                    },
                    WebServerStatus::Offline => {
                        self.web_server_started = false;
                    },
                    WebServerStatus::DifferentVersion(different_version) => {
                        self.web_server_started = false;
                        self.web_server_error = Some(format!(
                            "Server online with an incompatible Zellij version: {}",
                            different_version
                        ));
                    }
                }
                should_render = true;
            },
            Event::Key(key) => {
                if !self.web_server_capability {
                    return false;
                }
                match key.bare_key {
                    BareKey::Enter if key.has_no_modifiers() => {
                        start_web_server();
                    },
                    BareKey::Char('c') if key.has_modifiers(&[KeyModifier::Ctrl]) => {
                        stop_web_server();
                    },
                    BareKey::Char(' ') if key.has_no_modifiers() => {
                        match self.web_sharing {
                            WebSharing::Disabled => {
                                // no-op
                            },
                            WebSharing::On => {
                                stop_sharing_current_session();
                            },
                            WebSharing::Off => {
                                share_current_session();
                            },
                        }
                    },
                    _ => {},
                }
            },
            Event::Mouse(mouse_event) => {
                if !self.web_server_capability {
                    return false;
                }
                match mouse_event {
                    Mouse::LeftClick(line, column) => {
                        for (coordinates, url) in &self.clickable_urls {
                            if coordinates.contains(column, line as usize) {
                                if let Some(executable) = self.link_executable {
                                    run_command(&[&executable, &url], Default::default());
                                }
                                should_render = true;
                                break;
                            }
                        }
                    }
                    Mouse::Hover(line, column) => {
                        self.hover_coordinates = Some((column, line as usize));
                        should_render = true;
                    },
                    _ => {},
                }
            }
            Event::RunCommandResult(exit_code, _stdout, _stderr, context) => {
                if !self.web_server_capability {
                    return false;
                }
                let is_xdg_open = context.get("xdg_open_cli").is_some();
                let is_open = context.get("open_cli").is_some();
                if is_xdg_open {
                    if exit_code == Some(0) {
                        self.link_executable = Some("xdg-open");
                    }
                } else if is_open {
                    if exit_code == Some(0) {
                        self.link_executable = Some("open");
                    }
                }
            },
            _ => {},
        }
        should_render
    }
    fn render(&mut self, rows: usize, cols: usize) {
        if !self.web_server_capability {
            self.render_no_web_server_capability(rows, cols);
            return;
        }
        // reset rendered state
        self.currently_hovering_over_link = false;
        self.clickable_urls.clear();
        let usage = Usage::new();
        let mut web_server_status_section = WebServerStatusSection::new(self.web_server_started, self.web_server_ip, self.web_server_port);
        let mut current_session_section = CurrentSessionSection::new(
            self.web_server_started,
            self.web_server_ip,
            self.web_server_port,
            self.session_name.clone(),
            self.web_sharing,
        );

        let mut max_item_width = 0;
        let title_text = "Share Session Locally in the Browser";
        max_item_width = std::cmp::max(max_item_width, title_text.chars().count());

        let (web_server_items_width, web_server_items_height) = web_server_status_section.web_server_status_width_and_height();
        max_item_width = std::cmp::max(max_item_width, web_server_items_width);
        let (current_session_items_width, current_session_items_height) = current_session_section.current_session_status_width_and_height();
        max_item_width = std::cmp::max(max_item_width, current_session_items_width);
        let (usage_width, usage_height) = usage.usage_width_and_height(cols);
        max_item_width = std::cmp::max(max_item_width, usage_width);
        let line_count = 2 + web_server_items_height + 1 + current_session_items_height + 1 + usage_height;

        let base_x = cols.saturating_sub(max_item_width) / 2;
        let base_y = rows.saturating_sub(line_count) / 2; // the + 2 are the line spaces

        let mut current_y = base_y;
        let title = Text::new(title_text).color_range(2, ..);
        print_text_with_coordinates(title, cols.saturating_sub(title_text.chars().count()) / 2, current_y, None, None);
        current_y += 2;
        web_server_status_section.render_web_server_status(base_x, current_y, self.hover_coordinates);
        self.currently_hovering_over_link = web_server_status_section.currently_hovering_over_link;
        for (coordinates, url) in web_server_status_section.clickable_urls {
            self.clickable_urls.insert(coordinates, url);
        }
        current_y += web_server_items_height + 1;

        current_session_section.render_current_session_status(base_x, current_y, self.hover_coordinates);
        self.currently_hovering_over_link = self.currently_hovering_over_link || current_session_section.currently_hovering_over_link;
        for (coordinates, url) in current_session_section.clickable_urls {
            self.clickable_urls.insert(coordinates, url);
        }

        current_y += web_server_items_height + 1;
        usage.render_usage(base_x, current_y, cols);
        current_y += usage_height + 1;

        if self.currently_hovering_over_link {
            self.render_link_help(base_x, current_y);
        }
    }
}

impl App {
    pub fn render_link_help(&self, x: usize, y: usize) {
        let help_text = if self.link_executable.is_some() {
            let help_text = format!("Help: Click or Shift-Click to open in browser");
            Text::new(help_text)
                .color_range(3, 6..=10)
                .color_range(3, 15..=25)
        } else {
            let help_text = format!("Help: Shift-Click to open in browser");
            Text::new(help_text)
                .color_range(3, 6..=16)
        };
        print_text_with_coordinates(help_text, x, y, None, None);
    }
    pub fn query_link_executable(&self) {
        let mut xdg_open_context = BTreeMap::new();
        xdg_open_context.insert("xdg_open_cli".to_owned(), String::new());
        run_command(&["xdg-open", "--help"], xdg_open_context);
        let mut open_context = BTreeMap::new();
        open_context.insert("open_cli".to_owned(), String::new());
        run_command(&["open", "--help"], open_context);
    }
    pub fn change_own_title(&mut self) {
        if let Some(own_plugin_id) = self.own_plugin_id {
            rename_plugin_pane(own_plugin_id, "Share Session");
        }
    }
    pub fn render_no_web_server_capability(&self, rows: usize, cols: usize) {
        let text_full = "This version of Zellij was compiled without web sharing capabilities";
        let text_short = "No web server capabilities";
        let text = if cols >= text_full.chars().count() {
            text_full
        } else {
            text_short
        };
        let text_element = Text::new(text).color_range(3, ..);
        let text_x = cols.saturating_sub(text.chars().count()) / 2;
        let text_y = rows / 2;
        print_text_with_coordinates(text_element, text_x, text_y, None, None);
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct CoordinatesInLine {
    x: usize,
    y: usize,
    width: usize
}

impl CoordinatesInLine {
    pub fn new(x: usize, y: usize, width: usize) -> Self {
        CoordinatesInLine {
            x, y, width
        }
    }
    pub fn contains(&self, x: usize, y: usize) -> bool {
        x >= self.x && x <= self.x + self.width && self.y == y
    }
}
