use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use uuid::Uuid;
use zellij_tile::prelude::*;

use std::collections::{BTreeMap, HashMap};

pub struct SearchResult {
    plugin_id: u32,
    plugin_info: PluginInfo,
    indices: Vec<usize>,
    score: i64,
}

impl SearchResult {
    pub fn new(plugin_id: u32, plugin_info: &PluginInfo, indices: Vec<usize>, score: i64) -> Self {
        SearchResult {
            plugin_id,
            plugin_info: plugin_info.clone(),
            indices,
            score,
        }
    }
}

pub struct NewPluginScreen {
    new_plugin_url: String,
    new_plugin_config: Vec<(String, String)>, // key/val for easy in-place manipulation
    new_config_key: String,
    new_config_val: String,
    entering_plugin_url: bool,
    entering_config_key: bool,
    entering_config_val: bool,
    selected_config_index: Option<usize>,
    request_ids: Vec<String>,
    load_in_background: bool,
    colors: Styling,
}

impl Default for NewPluginScreen {
    fn default() -> Self {
        NewPluginScreen {
            new_plugin_url: String::new(),
            new_plugin_config: vec![],
            new_config_key: String::new(),
            new_config_val: String::new(),
            entering_plugin_url: true,
            entering_config_key: false,
            entering_config_val: false,
            selected_config_index: None,
            request_ids: vec![],
            load_in_background: false,
            colors: Palette::default().into(),
        }
    }
}

impl NewPluginScreen {
    pub fn new(colors: Styling) -> Self {
        Self {
            colors,
            ..Default::default()
        }
    }

    pub fn render(&self, rows: usize, cols: usize) {
        self.render_title(cols);
        self.render_url_field(cols);
        self.render_configuration_title();
        let config_list_len = self.render_config_list(cols, rows.saturating_sub(10)); // 10 - the rest
        self.render_background_toggle(6 + config_list_len + 1);
        if !self.editing_configuration() {
            self.render_help(rows);
        }
    }
    fn render_title(&self, cols: usize) {
        let title_text = format!("LOAD NEW PLUGIN");
        let title_text_len = title_text.chars().count();
        let title = Text::new(title_text);
        print_text_with_coordinates(
            title,
            (cols / 2).saturating_sub(title_text_len / 2),
            0,
            None,
            None,
        );
    }
    fn render_url_field(&self, cols: usize) {
        let url_field = if self.entering_plugin_url {
            let truncated_url =
                truncate_string_start(&self.new_plugin_url, cols.saturating_sub(19)); // 17 the length of the prompt + 2 for padding and cursor
            let text = format!("Enter Plugin URL: {}_", truncated_url);
            Text::new(text).color_range(2, ..=16).color_range(3, 18..)
        } else {
            let truncated_url =
                truncate_string_start(&self.new_plugin_url, cols.saturating_sub(18)); // 17 the length of the prompt + 1 for padding
            let text = format!("Enter Plugin URL: {}", truncated_url);
            Text::new(text).color_range(2, ..=16).color_range(0, 18..)
        };
        print_text_with_coordinates(url_field, 0, 2, None, None);
        let url_helper =
            NestedListItem::new(format!("<Ctrl f> - Load from Disk")).color_range(3, ..=8);
        print_nested_list_with_coordinates(vec![url_helper], 0, 3, None, None);
    }
    fn render_configuration_title(&self) {
        let configuration_title =
            if !self.editing_configuration() && self.new_plugin_config.is_empty() {
                Text::new(format!("Plugin Configuration: <TAB> - Edit"))
                    .color_range(2, ..=20)
                    .color_range(3, 22..=26)
            } else if !self.editing_configuration() {
                Text::new(format!(
                    "Plugin Configuration: <TAB> - Edit, <↓↑> - Navigate, <Del> - Delete"
                ))
                .color_range(2, ..=20)
                .color_range(3, 22..=26)
                .color_range(3, 36..=39)
                .color_range(3, 53..=57)
            } else {
                Text::new(format!(
                    "Plugin Configuration: [Editing: <TAB> - Next, <ENTER> - Accept]"
                ))
                .color_range(2, ..=20)
                .color_range(3, 32..=36)
                .color_range(3, 46..=52)
            };
        print_text_with_coordinates(configuration_title, 0, 5, None, None);
    }
    fn editing_configuration(&self) -> bool {
        self.entering_config_key || self.entering_config_val
    }
    fn render_config_list(&self, cols: usize, rows: usize) -> usize {
        let mut items = vec![];
        let mut more_config_items = 0;
        for (i, (config_key, config_val)) in self.new_plugin_config.iter().enumerate() {
            let is_selected = Some(i) == self.selected_config_index;
            if i >= rows {
                more_config_items += 1;
            } else if is_selected && self.editing_config_line() {
                items.push(self.render_editing_config_line(config_key, config_val, cols));
            } else {
                items.push(self.render_config_line(config_key, config_val, is_selected, cols));
            }
        }
        if self.editing_new_config_line() {
            items.push(self.render_editing_config_line(
                &self.new_config_key,
                &self.new_config_val,
                cols,
            ));
        } else if items.is_empty() {
            items.push(NestedListItem::new("<NO CONFIGURATION>").color_range(0, ..));
        }
        let config_list_len = items.len();
        print_nested_list_with_coordinates(items, 0, 6, Some(cols), None);
        if more_config_items > 0 {
            let more_text = format!("[+{}]", more_config_items);
            print_text_with_coordinates(
                Text::new(more_text).color_range(1, ..),
                0,
                6 + config_list_len,
                None,
                None,
            );
        }
        config_list_len
    }
    fn editing_config_line(&self) -> bool {
        self.entering_config_key || self.entering_config_val
    }
    fn editing_new_config_line(&self) -> bool {
        (self.entering_config_key || self.entering_config_val)
            && self.selected_config_index.is_none()
    }
    fn render_editing_config_line(
        &self,
        config_key: &str,
        config_val: &str,
        config_line_max_len: usize,
    ) -> NestedListItem {
        let config_line_max_len = config_line_max_len.saturating_sub(6); // 3 - line padding, 1 -
                                                                         // cursor, 2 ": "
        let config_key_max_len = config_line_max_len / 2;
        let config_val_max_len = config_line_max_len.saturating_sub(config_key_max_len);
        let config_key = if config_key.chars().count() > config_key_max_len {
            truncate_string_start(&config_key, config_key_max_len)
        } else {
            config_key.to_owned()
        };
        let config_val = if config_val.chars().count() > config_val_max_len {
            truncate_string_start(&config_val, config_val_max_len)
        } else {
            config_val.to_owned()
        };
        if self.entering_config_key {
            let val = if config_val.is_empty() {
                "<EMPTY>".to_owned()
            } else {
                config_val
            };
            NestedListItem::new(format!("{}_: {}", config_key, val))
                .color_range(3, ..=config_key.chars().count())
                .color_range(1, config_key.chars().count() + 3..)
        } else {
            let key = if config_key.is_empty() {
                "<EMPTY>".to_owned()
            } else {
                config_key
            };
            NestedListItem::new(format!("{}: {}_", key, config_val))
                .color_range(0, ..key.chars().count())
                .color_range(3, key.chars().count() + 2..)
        }
    }
    fn render_config_line(
        &self,
        config_key: &str,
        config_val: &str,
        is_selected: bool,
        config_line_max_len: usize,
    ) -> NestedListItem {
        let config_line_max_len = config_line_max_len.saturating_sub(5); // 3 - line padding,
                                                                         // 2 - ": "
        let config_key = if config_key.is_empty() {
            "<EMPTY>"
        } else {
            config_key
        };
        let config_val = if config_val.is_empty() {
            "<EMPTY>"
        } else {
            config_val
        };
        let config_key_max_len = config_line_max_len / 2;
        let config_val_max_len = config_line_max_len.saturating_sub(config_key_max_len);
        let config_key = if config_key.chars().count() > config_key_max_len {
            truncate_string_start(&config_key, config_key_max_len)
        } else {
            config_key.to_owned()
        };

        let config_val = if config_val.chars().count() > config_val_max_len {
            truncate_string_start(&config_val, config_val_max_len)
        } else {
            config_val.to_owned()
        };
        let mut item = NestedListItem::new(format!("{}: {}", config_key, config_val))
            .color_range(0, ..config_key.chars().count())
            .color_range(1, config_key.chars().count() + 2..);
        if is_selected {
            item = item.selected()
        }
        item
    }
    fn render_background_toggle(&self, y_coordinates: usize) {
        let key_shortcuts_text = format!("Ctrl l");
        print_text_with_coordinates(
            Text::new(&key_shortcuts_text).color_range(3, ..).opaque(),
            0,
            y_coordinates,
            None,
            None,
        );
        let background = self.colors.text_unselected.background;
        let bg_color = match background {
            PaletteColor::Rgb((r, g, b)) => format!("\u{1b}[48;2;{};{};{}m\u{1b}[0K", r, g, b),
            PaletteColor::EightBit(color) => format!("\u{1b}[48;5;{}m\u{1b}[0K", color),
        };
        println!(
            "\u{1b}[{};{}H{}",
            y_coordinates + 1,
            key_shortcuts_text.chars().count() + 1,
            bg_color
        );
        let load_in_background_text = format!("Load in Background");
        let load_in_foreground_text = format!("Load in Foreground");
        let (load_in_background_ribbon, load_in_foreground_ribbon) = if self.load_in_background {
            (
                Text::new(&load_in_background_text).selected(),
                Text::new(&load_in_foreground_text),
            )
        } else {
            (
                Text::new(&load_in_background_text),
                Text::new(&load_in_foreground_text).selected(),
            )
        };
        print_ribbon_with_coordinates(
            load_in_background_ribbon,
            key_shortcuts_text.chars().count() + 1,
            y_coordinates,
            None,
            None,
        );
        print_ribbon_with_coordinates(
            load_in_foreground_ribbon,
            key_shortcuts_text.chars().count() + 1 + load_in_background_text.chars().count() + 4,
            y_coordinates,
            None,
            None,
        );
    }
    fn render_help(&self, rows: usize) {
        let enter_line = Text::new(format!(
            "Help: <ENTER> - Accept and Load Plugin, <ESC> - Cancel"
        ))
        .color_range(3, 6..=12)
        .color_range(3, 40..=44);
        print_text_with_coordinates(enter_line, 0, rows, None, None);
    }
    fn get_field_being_edited_mut(&mut self) -> Option<&mut String> {
        if self.entering_plugin_url {
            Some(&mut self.new_plugin_url)
        } else {
            match self.selected_config_index {
                Some(selected_config_index) => {
                    if self.entering_config_key {
                        self.new_plugin_config
                            .get_mut(selected_config_index)
                            .map(|(key, _val)| key)
                    } else if self.entering_config_val {
                        self.new_plugin_config
                            .get_mut(selected_config_index)
                            .map(|(_key, val)| val)
                    } else {
                        None
                    }
                },
                None => {
                    if self.entering_config_key {
                        Some(&mut self.new_config_key)
                    } else if self.entering_config_val {
                        Some(&mut self.new_config_val)
                    } else {
                        None
                    }
                },
            }
        }
    }
    fn add_edit_buffer_to_config(&mut self) {
        if !self.new_config_key.is_empty() || !self.new_config_val.is_empty() {
            self.new_plugin_config.push((
                self.new_config_key.drain(..).collect(),
                self.new_config_val.drain(..).collect(),
            ));
        }
    }
    pub fn handle_key(&mut self, key: KeyWithModifier) -> (bool, bool) {
        let (mut should_render, mut should_close) = (false, false);

        match key.bare_key {
            BareKey::Char(character) if key.has_no_modifiers() => {
                if let Some(field) = self.get_field_being_edited_mut() {
                    field.push(character);
                }
                should_render = true;
            },
            BareKey::Backspace if key.has_no_modifiers() => {
                if let Some(field) = self.get_field_being_edited_mut() {
                    field.pop();
                }
                should_render = true;
            },
            BareKey::Enter if key.has_no_modifiers() => {
                if self.editing_configuration() {
                    self.add_edit_buffer_to_config();
                    self.entering_config_key = false;
                    self.entering_config_val = false;
                    self.entering_plugin_url = true;
                    should_render = true;
                } else {
                    let plugin_url: String = self.new_plugin_url.drain(..).collect();
                    self.add_edit_buffer_to_config();
                    let config = self.new_plugin_config.drain(..).into_iter().collect();
                    let load_in_background = self.load_in_background;
                    let skip_plugin_cache = true;
                    load_new_plugin(plugin_url, config, load_in_background, skip_plugin_cache);
                    should_render = true;
                    should_close = true;
                }
            },
            BareKey::Tab if key.has_no_modifiers() => {
                if self.entering_plugin_url {
                    self.entering_plugin_url = false;
                    self.entering_config_key = true;
                } else if self.entering_config_key {
                    self.entering_config_key = false;
                    self.entering_config_val = true;
                } else if self.entering_config_val {
                    self.entering_config_val = false;
                    if self.selected_config_index.is_none() {
                        // new config, add it to the map
                        self.add_edit_buffer_to_config();
                        self.entering_config_key = true;
                    } else {
                        self.entering_plugin_url = true;
                    }
                    self.selected_config_index = None;
                } else if self.selected_config_index.is_some() {
                    self.entering_config_key = true;
                } else {
                    self.entering_plugin_url = true;
                }
                should_render = true;
            },
            BareKey::Esc if key.has_no_modifiers() => {
                if self.entering_config_key
                    || self.entering_config_val
                    || self.selected_config_index.is_some()
                {
                    self.entering_plugin_url = true;
                    self.entering_config_key = false;
                    self.entering_config_val = false;
                    self.selected_config_index = None;
                    self.add_edit_buffer_to_config();
                    should_render = true;
                } else {
                    should_close = true;
                }
            },
            BareKey::Down if key.has_no_modifiers() => {
                if !self.editing_configuration() {
                    let max_len = self.new_plugin_config.len().saturating_sub(1);
                    let has_config_values = !self.new_plugin_config.is_empty();
                    if self.selected_config_index.is_none() && has_config_values {
                        self.selected_config_index = Some(0);
                    } else if self.selected_config_index == Some(max_len) {
                        self.selected_config_index = None;
                    } else {
                        self.selected_config_index = self.selected_config_index.map(|s| s + 1);
                    }
                }
                should_render = true;
            },
            BareKey::Up if key.has_no_modifiers() => {
                if !self.editing_configuration() {
                    let max_len = self.new_plugin_config.len().saturating_sub(1);
                    let has_config_values = !self.new_plugin_config.is_empty();
                    if self.selected_config_index.is_none() && has_config_values {
                        self.selected_config_index = Some(max_len);
                    } else if self.selected_config_index == Some(0) {
                        self.selected_config_index = None;
                    } else {
                        self.selected_config_index =
                            self.selected_config_index.map(|s| s.saturating_sub(1));
                    }
                }
                should_render = true;
            },
            BareKey::Delete if key.has_no_modifiers() => {
                if let Some(selected_config_index) = self.selected_config_index.take() {
                    self.new_plugin_config.remove(selected_config_index);
                    should_render = true;
                }
            },
            BareKey::Char('f') if key.has_modifiers(&[KeyModifier::Ctrl]) => {
                let mut args = BTreeMap::new();
                let request_id = Uuid::new_v4();
                self.request_ids.push(request_id.to_string());
                let mut config = BTreeMap::new();
                config.insert("request_id".to_owned(), request_id.to_string());
                args.insert("request_id".to_owned(), request_id.to_string());
                pipe_message_to_plugin(
                    MessageToPlugin::new("filepicker")
                        .with_plugin_url("filepicker")
                        .with_plugin_config(config)
                        .new_plugin_instance_should_have_pane_title(
                            "Select a .wasm file to load as a plugin...",
                        )
                        .new_plugin_instance_should_be_focused()
                        .with_args(args),
                );
            },
            BareKey::Char('l') if key.has_modifiers(&[KeyModifier::Ctrl]) => {
                self.load_in_background = !self.load_in_background;
                should_render = true;
            },
            _ => {},
        }

        (should_render, should_close)
    }
}

#[derive(Default)]
struct State {
    userspace_configuration: BTreeMap<String, String>,
    plugins: BTreeMap<u32, PluginInfo>,
    search_results: Vec<SearchResult>,
    selected_index: Option<usize>,
    expanded_indices: Vec<usize>,
    tab_position_to_tab_name: HashMap<usize, String>,
    plugin_id_to_tab_position: HashMap<u32, usize>,
    search_term: String,
    new_plugin_screen: Option<NewPluginScreen>,
    colors: Styling,
}

register_plugin!(State);

impl ZellijPlugin for State {
    fn load(&mut self, configuration: BTreeMap<String, String>) {
        self.userspace_configuration = configuration;
        subscribe(&[
            EventType::ModeUpdate,
            EventType::PaneUpdate,
            EventType::TabUpdate,
            EventType::Key,
            EventType::SessionUpdate,
        ]);
        let own_plugin_id = get_plugin_ids().plugin_id;
        rename_plugin_pane(own_plugin_id, "Plugin Manager");
    }
    fn pipe(&mut self, pipe_message: PipeMessage) -> bool {
        if pipe_message.name == "filepicker_result" {
            match (pipe_message.payload, pipe_message.args.get("request_id")) {
                (Some(payload), Some(request_id)) => {
                    match self
                        .new_plugin_screen
                        .as_mut()
                        .and_then(|n| n.request_ids.iter().position(|p| p == request_id))
                    {
                        Some(request_id_position) => {
                            self.new_plugin_screen
                                .as_mut()
                                .map(|n| n.request_ids.remove(request_id_position));
                            let chosen_plugin_location = std::path::PathBuf::from(payload);
                            self.new_plugin_screen.as_mut().map(|n| {
                                n.new_plugin_url =
                                    format!("file:{}", chosen_plugin_location.display())
                            });
                        },
                        None => {
                            eprintln!("request id not found");
                        },
                    }
                },
                _ => {},
            }
            true
        } else {
            false
        }
    }
    fn update(&mut self, event: Event) -> bool {
        let mut should_render = false;
        match event {
            Event::ModeUpdate(mode_info) => {
                self.colors = mode_info.style.colors;
                should_render = true;
            },
            Event::SessionUpdate(live_sessions, _dead_sessions) => {
                for session in live_sessions {
                    if session.is_current_session {
                        if session.plugins != self.plugins {
                            self.plugins = session.plugins;
                            self.reset_selection();
                            self.update_search_term();
                        }
                        for tab in session.tabs {
                            self.tab_position_to_tab_name.insert(tab.position, tab.name);
                        }
                    }
                }
                should_render = true;
            },
            Event::PaneUpdate(pane_manifest) => {
                for (tab_position, panes) in pane_manifest.panes {
                    for pane_info in panes {
                        if pane_info.is_plugin {
                            self.plugin_id_to_tab_position
                                .insert(pane_info.id, tab_position);
                        }
                    }
                }
            },
            Event::Key(key) => match self.new_plugin_screen.as_mut() {
                Some(new_plugin_screen) => {
                    let (should_render_new_plugin_screen, should_close_new_plugin_screen) =
                        new_plugin_screen.handle_key(key);
                    if should_close_new_plugin_screen {
                        self.new_plugin_screen = None;
                        should_render = true;
                    } else {
                        should_render = should_render_new_plugin_screen;
                    }
                },
                None => should_render = self.handle_main_screen_key(key),
            },
            _ => (),
        };
        should_render
    }

    fn render(&mut self, rows: usize, cols: usize) {
        match &self.new_plugin_screen {
            Some(new_plugin_screen) => {
                new_plugin_screen.render(rows, cols);
            },
            None => {
                self.render_search(cols);
                let list_y = 2;
                let max_list_items = rows.saturating_sub(4); // 2 top padding, 2 bottom padding
                let (selected_index_in_list, plugin_list) = if self.is_searching() {
                    self.render_search_results(cols)
                } else {
                    self.render_plugin_list(cols)
                };
                let (more_above, more_below, truncated_list) = self.truncate_list_to_screen(
                    selected_index_in_list,
                    plugin_list,
                    max_list_items,
                );
                self.render_more_indication(
                    more_above,
                    more_below,
                    cols,
                    list_y,
                    truncated_list.len(),
                );
                print_nested_list_with_coordinates(truncated_list, 0, list_y, Some(cols), None);
                self.render_help(rows, cols);
            },
        }
    }
}

impl State {
    fn render_search_results(&self, cols: usize) -> (Option<usize>, Vec<NestedListItem>) {
        let mut selected_index_in_list = None;
        let mut plugin_list = vec![];
        for (i, search_result) in self.search_results.iter().enumerate() {
            let is_selected = Some(i) == self.selected_index;
            if is_selected {
                selected_index_in_list = Some(plugin_list.len());
            }
            let is_expanded = self.expanded_indices.contains(&i);
            plugin_list.append(&mut self.render_search_result(
                search_result,
                is_selected,
                is_expanded,
                cols,
                None,
            ));
        }
        (selected_index_in_list, plugin_list)
    }
    fn render_plugin_list(&self, cols: usize) -> (Option<usize>, Vec<NestedListItem>) {
        let mut selected_index_in_list = None;
        let mut plugin_list = vec![];
        for (i, (plugin_id, plugin_info)) in self.plugins.iter().enumerate() {
            let is_selected = Some(i) == self.selected_index;
            let is_expanded = self.expanded_indices.contains(&i);
            if is_selected {
                selected_index_in_list = Some(plugin_list.len());
            }
            plugin_list.append(&mut self.render_plugin(
                *plugin_id,
                plugin_info,
                is_selected,
                is_expanded,
                cols,
            ));
        }
        (selected_index_in_list, plugin_list)
    }
    fn render_more_indication(
        &self,
        more_above: usize,
        more_below: usize,
        cols: usize,
        list_y: usize,
        list_len: usize,
    ) {
        if more_above > 0 {
            let text = format!("↑ [+{}]", more_above);
            let text_len = text.chars().count();
            print_text_with_coordinates(
                Text::new(text).color_range(1, ..),
                cols.saturating_sub(text_len),
                list_y.saturating_sub(1),
                None,
                None,
            );
        }
        if more_below > 0 {
            let text = format!("↓ [+{}]", more_below);
            let text_len = text.chars().count();
            print_text_with_coordinates(
                Text::new(text).color_range(1, ..),
                cols.saturating_sub(text_len),
                list_y + list_len,
                None,
                None,
            );
        }
    }
    pub fn render_search(&self, cols: usize) {
        let text = format!(" SEARCH: {}_", self.search_term);
        if text.chars().count() <= cols {
            let text = Text::new(text).color_range(3, 9..);
            print_text_with_coordinates(text, 0, 0, None, None);
        } else {
            let truncated_search_term =
                truncate_string_start(&self.search_term, cols.saturating_sub(10)); // 9 the length of the SEARCH prompt + 1 for the cursor
            let text = format!(" SEARCH: {}_", truncated_search_term);
            let text = Text::new(text).color_range(3, 9..);
            print_text_with_coordinates(text, 0, 0, None, None);
        }
    }
    pub fn render_plugin(
        &self,
        plugin_id: u32,
        plugin_info: &PluginInfo,
        is_selected: bool,
        is_expanded: bool,
        cols: usize,
    ) -> Vec<NestedListItem> {
        let mut items = vec![];
        let plugin_location_len = plugin_info.location.chars().count();
        let max_location_len = cols.saturating_sub(3); // 3 for the bulletin
        let location_string = if plugin_location_len > max_location_len {
            truncate_string_start(&plugin_info.location, max_location_len)
        } else {
            plugin_info.location.clone()
        };
        let mut item = self.render_plugin_line(location_string, None);
        if is_selected {
            item = item.selected();
        }
        items.push(item);
        if is_expanded {
            let tab_line = self.render_tab_line(plugin_id, cols);
            items.push(tab_line);
            if !plugin_info.configuration.is_empty() {
                let config_line = NestedListItem::new(format!("Configuration:"))
                    .color_range(2, ..=13)
                    .indent(1);
                items.push(config_line);
                for (config_key, config_val) in &plugin_info.configuration {
                    items.push(self.render_config_line(config_key, config_val, cols))
                }
            }
        }
        items
    }
    fn render_config_line(
        &self,
        config_key: &str,
        config_val: &str,
        cols: usize,
    ) -> NestedListItem {
        let config_line_padding = 9; // 7, left padding + 2 for the ": " between key/val
        let config_line_max_len = cols.saturating_sub(config_line_padding);
        let config_key_max_len = config_line_max_len / 2;
        let config_val_max_len = config_line_max_len.saturating_sub(config_key_max_len);
        let config_key = if config_key.chars().count() > config_key_max_len {
            truncate_string_start(&config_key, config_key_max_len)
        } else {
            config_key.to_owned()
        };

        let config_val = if config_val.chars().count() > config_val_max_len {
            truncate_string_start(&config_val, config_val_max_len)
        } else {
            config_val.to_owned()
        };
        NestedListItem::new(format!("{}: {}", config_key, config_val))
            .indent(2)
            .color_range(0, ..config_key.chars().count())
            .color_range(1, config_key.chars().count() + 2..)
    }
    pub fn render_search_result(
        &self,
        search_result: &SearchResult,
        is_selected: bool,
        is_expanded: bool,
        cols: usize,
        plus_indication: Option<usize>,
    ) -> Vec<NestedListItem> {
        let mut items = vec![];
        let plugin_info = &search_result.plugin_info;
        let plugin_id = search_result.plugin_id;
        let indices = &search_result.indices;
        let plus_indication_len = plus_indication
            .map(|p| p.to_string().chars().count() + 4)
            .unwrap_or(0); // 4 for the plus indication decorators and space
        let max_location_len = cols.saturating_sub(plus_indication_len + 3); // 3 for the bulletin
        let (location_string, indices) = if plugin_info.location.chars().count() <= max_location_len
        {
            (plugin_info.location.clone(), indices.clone())
        } else {
            truncate_search_result(&plugin_info.location, max_location_len, indices)
        };
        let mut item = match plus_indication {
            Some(plus_indication) => self.render_plugin_line_with_plus_indication(
                location_string,
                plus_indication,
                Some(indices),
            ),
            None => self.render_plugin_line(location_string, Some(indices)),
        };
        if is_selected {
            item = item.selected();
        }
        items.push(item);
        if is_expanded {
            let tab_line = self.render_tab_line(plugin_id, cols);
            items.push(tab_line);
            if !plugin_info.configuration.is_empty() {
                let config_line = NestedListItem::new(format!("Configuration:"))
                    .color_range(2, ..=13)
                    .indent(1);
                items.push(config_line);
                for (config_key, config_value) in &plugin_info.configuration {
                    items.push(self.render_config_line(config_key, config_value, cols))
                }
            }
        }
        items
    }
    fn render_plugin_line_with_plus_indication(
        &self,
        location_string: String,
        plus_indication: usize,
        indices: Option<Vec<usize>>,
    ) -> NestedListItem {
        let mut item = NestedListItem::new(&format!("{} [+{}]", location_string, plus_indication))
            .color_range(0, ..)
            .color_range(1, location_string.chars().count() + 1..);
        if let Some(indices) = indices {
            item = item.color_indices(3, indices);
        }
        item
    }
    fn render_plugin_line(
        &self,
        location_string: String,
        indices: Option<Vec<usize>>,
    ) -> NestedListItem {
        let mut item = NestedListItem::new(location_string).color_range(0, ..);
        if let Some(indices) = indices {
            item = item.color_indices(3, indices);
        }
        item
    }
    fn render_tab_line(&self, plugin_id: u32, max_width: usize) -> NestedListItem {
        let tab_of_plugin_id = self
            .get_tab_of_plugin_id(plugin_id)
            .unwrap_or_else(|| "N/A".to_owned());
        let tab_line_padding_count = 10; // 5 the length of the "Tab: " + 5 for the left padding

        let tab_of_plugin_id =
            if tab_of_plugin_id.chars().count() + tab_line_padding_count > max_width {
                truncate_string_start(
                    &tab_of_plugin_id,
                    max_width.saturating_sub(tab_line_padding_count),
                )
            } else {
                tab_of_plugin_id
            };

        let tab_line = NestedListItem::new(format!("Tab: {}", tab_of_plugin_id))
            .color_range(2, ..=3)
            .indent(1);
        tab_line
    }
    pub fn render_help(&self, y: usize, cols: usize) {
        let full_text = "Help: <←↓↑→> - Navigate/Expand, <ENTER> - focus, <TAB> - Reload, <Del> - Close, <Ctrl a> - New, <ESC> - Exit";
        let middle_text =
            "Help: <←↓↑→/ENTER> - Navigate, <TAB> - Reload, <Del> - Close, <Ctrl a> - New, <ESC> - Exit";
        let short_text =
            "<←↓↑→/ENTER/TAB/Del> - Navigate/Expand/Reload/Close, <Ctrl a> - New, <ESC> - Exit";
        if cols >= full_text.chars().count() {
            let text = Text::new(full_text)
                .color_range(3, 5..=11)
                .color_range(3, 32..=38)
                .color_range(3, 49..=53)
                .color_range(3, 65..=69)
                .color_range(3, 80..=87)
                .color_range(3, 96..=100);
            print_text_with_coordinates(text, 0, y, Some(cols), None);
        } else if cols >= middle_text.chars().count() {
            let text = Text::new(middle_text)
                .color_range(3, 6..=17)
                .color_range(3, 31..=35)
                .color_range(3, 47..=51)
                .color_range(3, 62..=69)
                .color_range(3, 78..=82);
            print_text_with_coordinates(text, 0, y, Some(cols), None);
        } else {
            let text = Text::new(short_text)
                .color_range(3, ..=21)
                .color_range(3, 53..=60)
                .color_range(3, 69..=73);
            print_text_with_coordinates(text, 0, y, Some(cols), None);
        }
    }
    pub fn selected_plugin_id(&self) -> Option<u32> {
        if self.is_searching() {
            self.selected_index
                .and_then(|i| self.search_results.iter().nth(i))
                .map(|search_result| search_result.plugin_id)
        } else {
            self.selected_index
                .and_then(|i| self.plugins.iter().nth(i))
                .map(|(id, _)| *id)
        }
    }
    pub fn focus_selected(&self) {
        if let Some(selected_plugin_id) = self.selected_plugin_id() {
            focus_pane_with_id(PaneId::Plugin(selected_plugin_id), true);
        }
    }
    pub fn reload_selected(&self) {
        if let Some(selected_plugin_id) = self.selected_plugin_id() {
            reload_plugin_with_id(selected_plugin_id);
        }
    }
    pub fn close_selected(&self) {
        if let Some(selected_plugin_id) = self.selected_plugin_id() {
            close_plugin_pane(selected_plugin_id);
        }
    }
    pub fn reset_selection(&mut self) {
        self.selected_index = None;
        self.expanded_indices.clear();
    }
    pub fn expand_selected(&mut self) {
        if let Some(selected_index) = &self.selected_index {
            self.expanded_indices.push(*selected_index);
        }
    }
    pub fn collapse_selected(&mut self) {
        if let Some(selected_index) = &self.selected_index {
            self.expanded_indices.retain(|i| i != selected_index);
        }
    }
    pub fn get_tab_of_plugin_id(&self, plugin_id: u32) -> Option<String> {
        self.plugin_id_to_tab_position
            .get(&plugin_id)
            .and_then(|plugin_id| self.tab_position_to_tab_name.get(plugin_id))
            .cloned()
    }
    pub fn update_search_term(&mut self) {
        if self.search_term.is_empty() {
            self.search_results.clear();
        } else {
            let mut matches = vec![];
            let matcher = SkimMatcherV2::default().use_cache(true);
            for (plugin_id, plugin_info) in &self.plugins {
                if let Some((score, indices)) =
                    matcher.fuzzy_indices(&plugin_info.location, &self.search_term)
                {
                    matches.push(SearchResult::new(*plugin_id, plugin_info, indices, score));
                }
            }
            matches.sort_by(|a, b| b.score.cmp(&a.score));
            self.search_results = matches;
        }
    }
    pub fn handle_main_screen_key(&mut self, key: KeyWithModifier) -> bool {
        let mut should_render = false;
        match key.bare_key {
            BareKey::Char(character) if key.has_no_modifiers() => {
                self.search_term.push(character);
                self.update_search_term();
                self.reset_selection();
                should_render = true;
            },
            BareKey::Backspace if key.has_no_modifiers() => {
                self.search_term.pop();
                self.update_search_term();
                self.reset_selection();
                should_render = true;
            },
            BareKey::Down if key.has_no_modifiers() => {
                let max_len = if self.is_searching() {
                    self.search_results.len().saturating_sub(1)
                } else {
                    self.plugins.keys().len().saturating_sub(1)
                };
                if self.selected_index.is_none() {
                    self.selected_index = Some(0);
                } else if self.selected_index == Some(max_len) {
                    self.selected_index = None;
                } else {
                    self.selected_index = self.selected_index.map(|s| s + 1);
                }
                should_render = true;
            },
            BareKey::Up if key.has_no_modifiers() => {
                let max_len = if self.is_searching() {
                    self.plugins.keys().len().saturating_sub(1)
                } else {
                    self.search_results.len().saturating_sub(1)
                };
                if self.selected_index.is_none() {
                    self.selected_index = Some(max_len);
                } else if self.selected_index == Some(0) {
                    self.selected_index = None;
                } else {
                    self.selected_index = self.selected_index.map(|s| s.saturating_sub(1));
                }
                should_render = true;
            },
            BareKey::Enter if key.has_no_modifiers() => {
                self.focus_selected();
            },
            BareKey::Right if key.has_no_modifiers() => {
                self.expand_selected();
                should_render = true;
            },
            BareKey::Left if key.has_no_modifiers() => {
                self.collapse_selected();
                should_render = true;
            },
            BareKey::Tab if key.has_no_modifiers() => {
                self.reload_selected();
            },
            BareKey::Char('a') if key.has_modifiers(&[KeyModifier::Ctrl]) => {
                self.new_plugin_screen = Some(NewPluginScreen::new(self.colors));
                should_render = true;
            },
            BareKey::Delete if key.has_no_modifiers() => {
                self.close_selected();
            },
            BareKey::Esc if key.has_no_modifiers() => {
                if !self.search_term.is_empty() {
                    self.search_term.clear();
                    self.update_search_term();
                    should_render = true;
                } else {
                    close_self();
                }
            },
            _ => {},
        }
        should_render
    }
    pub fn is_searching(&self) -> bool {
        self.search_term.len() > 0
    }
    fn truncate_list_to_screen(
        &self,
        selected_index_in_list: Option<usize>,
        mut plugin_list: Vec<NestedListItem>,
        max_list_items: usize,
    ) -> (usize, usize, Vec<NestedListItem>) {
        let mut more_above = 0;
        let mut more_below = 0;
        if plugin_list.len() > max_list_items {
            let anchor_line = selected_index_in_list.unwrap_or(0);
            let list_start = anchor_line.saturating_sub(max_list_items / 2);
            let list_end = (list_start + max_list_items).saturating_sub(1);
            let mut to_render = vec![];
            for (i, item) in plugin_list.drain(..).enumerate() {
                if i >= list_start && i < list_end {
                    to_render.push(item);
                } else if i >= list_end {
                    more_below += 1;
                } else if i < list_start {
                    more_above += 1;
                }
            }
            plugin_list = to_render;
        }
        (more_above, more_below, plugin_list)
    }
}

fn truncate_string_start(string_to_truncate: &str, max_len: usize) -> String {
    let mut truncated_string = string_to_truncate.to_owned();
    let count_to_remove = truncated_string.chars().count().saturating_sub(max_len) + 5;
    if truncated_string.chars().count() > max_len {
        truncated_string.replace_range(0..count_to_remove, "[...]");
    }
    truncated_string
}

fn truncate_search_result(
    plugin_location: &str,
    max_location_len: usize,
    indices: &Vec<usize>,
) -> (String, Vec<usize>) {
    let truncated_location = truncate_string_start(&plugin_location, max_location_len);
    let truncated_count = plugin_location
        .chars()
        .count()
        .saturating_sub(max_location_len);
    let adjusted_indices = indices
        .iter()
        .filter_map(|i| {
            if i.saturating_sub(truncated_count) >= 5 {
                Some(i.saturating_sub(truncated_count))
            } else {
                None
            }
        })
        .collect();
    (truncated_location, adjusted_indices)
}
