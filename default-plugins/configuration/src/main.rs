use zellij_tile::prelude::*;

use std::collections::{BTreeMap, BTreeSet};

static UI_SIZE: usize = 15;

struct State {
    userspace_configuration: BTreeMap<String, String>,
    selected_index: Option<usize>,
    selected_primary_key_index: usize,
    selected_secondary_key_index: usize,
    remapping_leaders: bool,
    primary_modifier: BTreeSet<KeyModifier>,
    secondary_modifier: BTreeSet<KeyModifier>,
    possible_modifiers: Vec<KeyModifier>,
    browsing_secondary_modifier: bool,
    mode_color_index: usize,
    preset_color_index: usize,
    primary_leader_key_color_index: usize,
    secondary_leader_key_color_index: usize,
    notification: Option<String>,
    is_setup_wizard: bool,
    ui_size: usize,
}

impl Default for State {
    fn default() -> Self {
        let mut primary_modifier = BTreeSet::new();
        primary_modifier.insert(KeyModifier::Ctrl);
        let mut secondary_modifier = BTreeSet::new();
        secondary_modifier.insert(KeyModifier::Alt);
        State {
            userspace_configuration: BTreeMap::new(),
            selected_index: None,
            selected_primary_key_index: 0,
            selected_secondary_key_index: 0,
            remapping_leaders: false,
            primary_modifier,
            secondary_modifier,
            possible_modifiers: vec![
                KeyModifier::Ctrl,
                KeyModifier::Alt,
                KeyModifier::Super,
                KeyModifier::Shift,
            ],
            browsing_secondary_modifier: false,
            primary_leader_key_color_index: 3,
            secondary_leader_key_color_index: 0,
            mode_color_index: 2,
            preset_color_index: 1,
            notification: None,
            is_setup_wizard: false,
            ui_size: UI_SIZE,
        }
    }
}

register_plugin!(State);

impl ZellijPlugin for State {
    fn load(&mut self, configuration: BTreeMap<String, String>) {
        self.is_setup_wizard = configuration
            .get("is_setup_wizard")
            .map(|v| v == "true")
            .unwrap_or(false);
        self.userspace_configuration = configuration;
        subscribe(&[EventType::Key, EventType::FailedToWriteConfigToDisk]);
        let own_plugin_id = get_plugin_ids().plugin_id;
        if self.is_setup_wizard {
            self.ui_size = 18;
            self.selected_index = Some(0);
            rename_plugin_pane(own_plugin_id, "First Run Setup Wizard (Step 1/1)");
            resize_focused_pane(Resize::Increase);
            resize_focused_pane(Resize::Increase);
            resize_focused_pane(Resize::Increase);
        } else {
            rename_plugin_pane(own_plugin_id, "Configuration");
        }
    }
    fn update(&mut self, event: Event) -> bool {
        let mut should_render = false;
        match event {
            Event::Key(key) => {
                if self.remapping_leaders {
                    should_render = self.handle_remapping_screen_key(key);
                } else if self.is_setup_wizard {
                    should_render = self.handle_setup_wizard_key(key);
                } else {
                    should_render = self.handle_main_screen_key(key);
                }
            },
            Event::FailedToWriteConfigToDisk(config_file_path) => {
                match config_file_path {
                    Some(failed_path) => {
                        self.notification = Some(format!(
                            "Failed to write configuration file: {}",
                            failed_path
                        ));
                    },
                    None => {
                        self.notification = Some(format!("Failed to write configuration file."));
                    },
                }
                should_render = true;
            },
            _ => (),
        };
        should_render
    }
    fn render(&mut self, rows: usize, cols: usize) {
        if self.remapping_leaders {
            self.render_remapping_leaders_screen(rows, cols);
        } else if self.is_setup_wizard {
            self.render_setup_wizard_screen(rows, cols);
        } else {
            self.render_main_screen(rows, cols);
        }
    }
}

impl State {
    fn handle_remapping_screen_key(&mut self, key: KeyWithModifier) -> bool {
        let mut should_render = false;
        if self.browsing_secondary_modifier {
            if key.bare_key == BareKey::Left && key.has_no_modifiers() {
                self.browsing_secondary_modifier = false;
                self.selected_primary_key_index = self.selected_secondary_key_index;
                should_render = true;
            } else if key.bare_key == BareKey::Right && key.has_no_modifiers() {
                self.browsing_secondary_modifier = false;
                self.selected_primary_key_index = self.selected_secondary_key_index;
                should_render = true;
            } else if key.bare_key == BareKey::Down && key.has_no_modifiers() {
                if self.selected_secondary_key_index
                    < self.possible_modifiers.len().saturating_sub(1)
                {
                    self.selected_secondary_key_index += 1;
                } else {
                    self.selected_secondary_key_index = 0;
                }
                should_render = true;
            } else if key.bare_key == BareKey::Up && key.has_no_modifiers() {
                if self.selected_secondary_key_index > 0 {
                    self.selected_secondary_key_index -= 1;
                } else {
                    self.selected_secondary_key_index =
                        self.possible_modifiers.len().saturating_sub(1);
                }
                should_render = true;
            } else if key.bare_key == BareKey::Char(' ') && key.has_no_modifiers() {
                if let Some(selected_modifier) = self
                    .possible_modifiers
                    .get(self.selected_secondary_key_index)
                {
                    if self.secondary_modifier.contains(selected_modifier) {
                        self.secondary_modifier.remove(selected_modifier);
                    } else {
                        self.secondary_modifier.insert(*selected_modifier);
                    }
                    should_render = true;
                }
            }
        } else {
            if key.bare_key == BareKey::Left && key.has_no_modifiers() {
                self.browsing_secondary_modifier = true;
                self.selected_secondary_key_index = self.selected_primary_key_index;
                should_render = true;
            } else if key.bare_key == BareKey::Right && key.has_no_modifiers() {
                self.browsing_secondary_modifier = true;
                self.selected_secondary_key_index = self.selected_primary_key_index;
                should_render = true;
            } else if key.bare_key == BareKey::Down && key.has_no_modifiers() {
                if self.selected_primary_key_index < self.possible_modifiers.len().saturating_sub(1)
                {
                    self.selected_primary_key_index += 1;
                } else {
                    self.selected_primary_key_index = 0;
                }
                should_render = true;
            } else if key.bare_key == BareKey::Up && key.has_no_modifiers() {
                if self.selected_primary_key_index > 0 {
                    self.selected_primary_key_index -= 1;
                } else {
                    self.selected_primary_key_index =
                        self.possible_modifiers.len().saturating_sub(1);
                }
                should_render = true;
            } else if key.bare_key == BareKey::Char(' ') && key.has_no_modifiers() {
                if let Some(selected_modifier) =
                    self.possible_modifiers.get(self.selected_primary_key_index)
                {
                    if self.primary_modifier.contains(selected_modifier) {
                        self.primary_modifier.remove(selected_modifier);
                    } else {
                        self.primary_modifier.insert(*selected_modifier);
                    }
                    should_render = true;
                }
            }
        }
        if key.bare_key == BareKey::Enter {
            self.remapping_leaders = false;
            should_render = true;
        }
        should_render
    }
    fn handle_main_screen_key(&mut self, key: KeyWithModifier) -> bool {
        let mut should_render = false;
        if self.notification.is_some() {
            self.notification = None;
            should_render = true;
        } else if key.bare_key == BareKey::Down && key.has_no_modifiers() {
            if self.selected_index.is_none() {
                self.selected_index = Some(0);
            } else if self.selected_index < Some(1) {
                self.selected_index = Some(1);
            } else {
                self.selected_index = None;
            }
            should_render = true;
        } else if key.bare_key == BareKey::Up && key.has_no_modifiers() {
            if self.selected_index.is_none() {
                self.selected_index = Some(1);
            } else if self.selected_index == Some(1) {
                self.selected_index = Some(0);
            } else {
                self.selected_index = None;
            }
            should_render = true;
        } else if key.bare_key == BareKey::Enter && key.has_no_modifiers() {
            if let Some(selected) = self.selected_index.take() {
                let write_to_disk = false;
                self.reconfigure(selected, write_to_disk);
                self.notification = Some("Configuration applied to current session.".to_owned());
                should_render = true;
            } else {
                self.selected_index = Some(0);
                should_render = true;
            }
        } else if key.bare_key == BareKey::Char(' ') && key.has_no_modifiers() {
            if let Some(selected) = self.selected_index.take() {
                let write_to_disk = true;
                self.reconfigure(selected, write_to_disk);
                self.notification = Some("Configuration applied and saved to disk.".to_owned());
                should_render = true;
            }
        } else if key.bare_key == BareKey::Char('l') && key.has_no_modifiers() {
            self.remapping_leaders = true;
            should_render = true;
        } else if (key.bare_key == BareKey::Esc && key.has_no_modifiers())
            || key.is_key_with_ctrl_modifier(BareKey::Char('c'))
        {
            close_self();
            should_render = true;
        }
        should_render
    }
    fn handle_setup_wizard_key(&mut self, key: KeyWithModifier) -> bool {
        let mut should_render = false;
        if self.notification.is_some() {
            self.notification = None;
            should_render = true;
        } else if key.bare_key == BareKey::Down && key.has_no_modifiers() {
            if self.selected_index.is_none() {
                self.selected_index = Some(0);
            } else if self.selected_index < Some(1) {
                self.selected_index = Some(1);
            } else {
                self.selected_index = None;
            }
            should_render = true;
        } else if key.bare_key == BareKey::Up && key.has_no_modifiers() {
            if self.selected_index.is_none() {
                self.selected_index = Some(1);
            } else if self.selected_index == Some(1) {
                self.selected_index = Some(0);
            } else {
                self.selected_index = None;
            }
            should_render = true;
        } else if key.bare_key == BareKey::Enter && key.has_no_modifiers() {
            if let Some(selected) = self.selected_index.take() {
                let write_to_disk = true;
                self.reconfigure(selected, write_to_disk);
                close_self();
            } else {
                self.selected_index = Some(0);
                should_render = true;
            }
        } else if key.bare_key == BareKey::Char('l') && key.has_no_modifiers() {
            self.remapping_leaders = true;
            should_render = true;
        } else if (key.bare_key == BareKey::Esc && key.has_no_modifiers())
            || key.is_key_with_ctrl_modifier(BareKey::Char('c'))
        {
            close_self();
            should_render = true;
        }
        should_render
    }
    fn render_selection_keymap(&self, rows: usize, cols: usize) {
        let widths = self.remapping_screen_widths();
        if cols >= widths.0 {
            let mut x = cols.saturating_sub(10) / 2;
            let mut y = rows.saturating_sub(7) / 2;
            if self.browsing_secondary_modifier {
                x += 31;
                y += self.selected_secondary_key_index;
            } else {
                y += self.selected_primary_key_index;
            }
            let text = "<←↓↑→> / <SPACE> ";
            let text_len = text.chars().count();
            let text = Text::new(text)
                .color_range(2, 1..5)
                .color_range(2, 10..15)
                .selected();
            print_text_with_coordinates(text, x.saturating_sub(text_len), y + 3, None, None);
        }
    }
    fn render_remapping_screen_title(&self, rows: usize, cols: usize) {
        let widths = self.remapping_screen_widths();
        let screen_width = if cols >= widths.0 {
            widths.0
        } else if cols >= widths.1 {
            widths.1
        } else {
            widths.2
        };
        let leader_keys_text = if cols >= widths.0 {
            "Adjust leader keys for the presets in the previous screen:"
        } else {
            "Adjust leader keys:"
        };
        let base_x = cols.saturating_sub(screen_width) / 2;
        let base_y = rows.saturating_sub(7) / 2;
        print_text_with_coordinates(
            Text::new(leader_keys_text).color_range(2, ..),
            base_x,
            base_y,
            None,
            None,
        );
    }
    fn render_primary_modifier_selector(&self, rows: usize, cols: usize) {
        let widths = self.remapping_screen_widths();
        let screen_width = if cols >= widths.0 {
            widths.0
        } else if cols >= widths.1 {
            widths.1
        } else {
            widths.2
        };
        self.render_remapping_screen_title(rows, cols);
        let base_x = cols.saturating_sub(screen_width) / 2;
        let base_y = rows.saturating_sub(7) / 2;
        let primary_modifier_key_text = self.primary_modifier_text();
        let (primary_modifier_text, primary_modifier_start_position) = if cols >= widths.0 {
            (format!("Primary: {}", primary_modifier_key_text), 9)
        } else {
            (format!("{}", primary_modifier_key_text), 0)
        };
        print_text_with_coordinates(
            Text::new(primary_modifier_text).color_range(
                self.primary_leader_key_color_index,
                primary_modifier_start_position..,
            ),
            base_x,
            base_y + 2,
            None,
            None,
        );
        print_nested_list_with_coordinates(
            self.possible_modifiers
                .iter()
                .enumerate()
                .map(|(i, m)| {
                    let item = if self.primary_modifier.contains(m) {
                        NestedListItem::new(m.to_string())
                            .color_range(self.primary_leader_key_color_index, ..)
                    } else {
                        NestedListItem::new(m.to_string())
                    };
                    if !self.browsing_secondary_modifier && self.selected_primary_key_index == i {
                        item.selected()
                    } else {
                        item
                    }
                })
                .collect(),
            base_x,
            base_y + 3,
            Some(screen_width / 2),
            None,
        );
    }
    fn render_secondary_modifier_selector(&mut self, rows: usize, cols: usize) {
        let widths = self.remapping_screen_widths();
        let screen_width = if cols >= widths.0 {
            widths.0
        } else if cols >= widths.1 {
            widths.1
        } else {
            widths.2
        };
        let base_x = cols.saturating_sub(screen_width) / 2;
        let base_y = rows.saturating_sub(7) / 2;
        let secondary_modifier_key_text = self.secondary_modifier_text();
        let (secondary_modifier_text, secondary_modifier_start_position) = if cols >= widths.0 {
            (format!("Secondary: {}", secondary_modifier_key_text), 10)
        } else {
            (format!("{}", secondary_modifier_key_text), 0)
        };
        let secondary_modifier_menu_x_coords = base_x + (screen_width / 2);
        print_text_with_coordinates(
            Text::new(secondary_modifier_text).color_range(
                self.secondary_leader_key_color_index,
                secondary_modifier_start_position..,
            ),
            secondary_modifier_menu_x_coords,
            base_y + 2,
            None,
            None,
        );
        print_nested_list_with_coordinates(
            self.possible_modifiers
                .iter()
                .enumerate()
                .map(|(i, m)| {
                    let item = if self.secondary_modifier.contains(m) {
                        NestedListItem::new(m.to_string())
                            .color_range(self.secondary_leader_key_color_index, ..)
                    } else {
                        NestedListItem::new(m.to_string())
                    };
                    if self.browsing_secondary_modifier && self.selected_secondary_key_index == i {
                        item.selected()
                    } else {
                        item
                    }
                })
                .collect(),
            secondary_modifier_menu_x_coords,
            base_y + 3,
            Some(screen_width / 2),
            None,
        );
    }
    fn render_remapping_leaders_screen(&mut self, rows: usize, cols: usize) {
        self.render_remapping_screen_title(rows, cols);
        self.render_primary_modifier_selector(rows, cols);
        self.render_secondary_modifier_selector(rows, cols);
        self.render_selection_keymap(rows, cols);
        self.render_help_text_remapping(rows, cols);
    }
    fn render_override_title(&self, rows: usize, cols: usize, primary_modifier_key_text: &str) {
        let widths = self.main_screen_widths(primary_modifier_key_text);
        if cols >= widths.0 {
            let title_text = "Override keybindings with one of the following presets:";
            let left_padding = cols.saturating_sub(widths.0) / 2;
            print_text_with_coordinates(
                Text::new(title_text).color_range(2, ..),
                left_padding,
                rows.saturating_sub(self.ui_size) / 2,
                None,
                None,
            );
        } else {
            let title_text = "Override keybindings:";
            let left_padding = if cols >= widths.1 {
                cols.saturating_sub(widths.1) / 2
            } else {
                cols.saturating_sub(widths.2) / 2
            };
            print_text_with_coordinates(
                Text::new(title_text).color_range(2, ..),
                left_padding,
                rows.saturating_sub(self.ui_size) / 2,
                None,
                None,
            );
        }
    }
    fn render_setup_wizard_title(&self, rows: usize, cols: usize, primary_modifier_key_text: &str) {
        let widths = self.main_screen_widths(primary_modifier_key_text);
        if cols >= widths.0 {
            let title_text_1 = "Hi there! How would you like to interact with Zellij?";
            let title_text_2 = "Not sure? Press <ENTER> to choose Default.";
            let title_text_3 = "Everything can always be changed later.";
            let title_text_4 = "Tips appear on screen - you don't need to remember anything.";
            let left_padding = cols.saturating_sub(widths.0) / 2;
            let first_row_coords = (rows.saturating_sub(self.ui_size) / 2).saturating_sub(1);
            print_text_with_coordinates(
                Text::new(title_text_1).color_range(2, ..),
                left_padding,
                first_row_coords,
                None,
                None,
            );
            print_text_with_coordinates(
                Text::new(title_text_2)
                    .color_range(0, ..10)
                    .color_range(2, 16..23)
                    .color_range(self.preset_color_index, 34..41),
                left_padding,
                first_row_coords + 2,
                None,
                None,
            );
            print_text_with_coordinates(
                Text::new(title_text_3),
                left_padding,
                first_row_coords + 4,
                None,
                None,
            );
            print_text_with_coordinates(
                Text::new(title_text_4),
                left_padding,
                first_row_coords + 5,
                None,
                None,
            );
        } else {
            let title_text_1 = "Hi there! Which do you prefer?";
            let title_text_2 = "Not sure? Press <ENTER>";
            let title_text_3 = "Can be changed later. Tips appear";
            let title_text_4 = "on screen - no need to remember";
            let left_padding = if cols >= widths.1 {
                cols.saturating_sub(widths.1) / 2
            } else {
                cols.saturating_sub(widths.2) / 2
            };
            let first_row_coords = (rows.saturating_sub(self.ui_size) / 2).saturating_sub(1);
            print_text_with_coordinates(
                Text::new(title_text_1).color_range(2, ..),
                left_padding,
                first_row_coords,
                None,
                None,
            );
            print_text_with_coordinates(
                Text::new(title_text_2)
                    .color_range(0, ..10)
                    .color_range(2, 16..23)
                    .color_range(self.preset_color_index, 40..49),
                left_padding,
                first_row_coords + 2,
                None,
                None,
            );
            print_text_with_coordinates(
                Text::new(title_text_3),
                left_padding,
                first_row_coords + 4,
                None,
                None,
            );
            print_text_with_coordinates(
                Text::new(title_text_4),
                left_padding,
                first_row_coords + 5,
                None,
                None,
            );
        }
    }
    fn render_first_bulletin(&self, rows: usize, cols: usize, primary_modifier_key_text: &str) {
        let widths = self.main_screen_widths(primary_modifier_key_text);
        let primary_modifier_key_text_len = primary_modifier_key_text.chars().count();
        let default_text = "1. Default";
        let (mut list_items, max_width) = if cols >= widths.0 {
            let list_items = vec![
                NestedListItem::new(default_text).color_range(self.preset_color_index, ..),
                NestedListItem::new("All modes available directly from the base mode, eg.:")
                    .indent(1),
                NestedListItem::new(format!(
                    "{} p - to enter PANE mode",
                    primary_modifier_key_text
                ))
                .color_range(
                    self.primary_leader_key_color_index,
                    ..primary_modifier_key_text_len + 3,
                )
                .color_range(
                    self.mode_color_index,
                    primary_modifier_key_text_len + 14..primary_modifier_key_text_len + 18,
                )
                .indent(1),
                NestedListItem::new(format!(
                    "{} t - to enter TAB mode",
                    primary_modifier_key_text
                ))
                .color_range(
                    self.primary_leader_key_color_index,
                    ..primary_modifier_key_text_len + 3,
                )
                .color_range(
                    self.mode_color_index,
                    primary_modifier_key_text_len + 14..primary_modifier_key_text_len + 17,
                )
                .indent(1),
            ];
            let max_width = widths.0;
            (list_items, max_width)
        } else if cols >= widths.1 {
            let list_items = vec![
                NestedListItem::new(default_text).color_range(self.preset_color_index, ..),
                NestedListItem::new("Modes available directly, eg.:").indent(1),
                NestedListItem::new(format!(
                    "{} p - to enter PANE mode",
                    primary_modifier_key_text
                ))
                .indent(1)
                .color_range(
                    self.primary_leader_key_color_index,
                    ..primary_modifier_key_text_len + 3,
                )
                .color_range(
                    self.mode_color_index,
                    primary_modifier_key_text_len + 14..primary_modifier_key_text_len + 18,
                ),
                NestedListItem::new(format!(
                    "{} t - to enter TAB mode",
                    primary_modifier_key_text
                ))
                .indent(1)
                .color_range(
                    self.primary_leader_key_color_index,
                    ..primary_modifier_key_text_len + 3,
                )
                .color_range(
                    self.mode_color_index,
                    primary_modifier_key_text_len + 14..primary_modifier_key_text_len + 17,
                ),
            ];
            let max_width = widths.1;
            (list_items, max_width)
        } else {
            let list_items = vec![
                NestedListItem::new(default_text).color_range(self.preset_color_index, ..),
                NestedListItem::new("Directly, eg.:").indent(1),
                NestedListItem::new(format!("{} p - PANE mode", primary_modifier_key_text))
                    .color_range(
                        self.primary_leader_key_color_index,
                        ..primary_modifier_key_text_len + 3,
                    )
                    .color_range(
                        self.mode_color_index,
                        primary_modifier_key_text_len + 5..primary_modifier_key_text_len + 10,
                    )
                    .indent(1),
                NestedListItem::new(format!("{} t - TAB mode", primary_modifier_key_text))
                    .color_range(
                        self.primary_leader_key_color_index,
                        ..primary_modifier_key_text_len + 3,
                    )
                    .color_range(
                        self.mode_color_index,
                        primary_modifier_key_text_len + 5..primary_modifier_key_text_len + 9,
                    )
                    .indent(1),
            ];
            let max_width = widths.2;
            (list_items, max_width)
        };
        if self.selected_index == Some(0) {
            list_items = list_items.drain(..).map(|i| i.selected()).collect();
        }
        let left_padding = cols.saturating_sub(max_width) / 2;
        let top_coordinates = if rows > 14 {
            (rows.saturating_sub(self.ui_size) / 2) + 2
        } else {
            (rows.saturating_sub(self.ui_size) / 2) + 1
        };
        print_nested_list_with_coordinates(
            list_items,
            left_padding,
            top_coordinates,
            Some(max_width),
            None,
        );
    }
    fn render_second_bulletin(&self, rows: usize, cols: usize, primary_modifier_key_text: &str) {
        let unlock_first_text = "2. Unlock First (non-colliding)";
        let widths = self.main_screen_widths(primary_modifier_key_text);
        let primary_modifier_key_text_len = primary_modifier_key_text.chars().count();
        let (mut list_items, max_width) = if cols >= widths.0 {
            let list_items = vec![
                NestedListItem::new(unlock_first_text).color_range(self.preset_color_index, ..),
                NestedListItem::new(format!(
                    "Single key modes available after unlocking with {} g, eg.:",
                    primary_modifier_key_text
                ))
                .indent(1),
                NestedListItem::new(format!(
                    "{} g + p to enter PANE mode",
                    primary_modifier_key_text
                ))
                .indent(1)
                .color_range(
                    self.primary_leader_key_color_index,
                    ..primary_modifier_key_text_len + 3,
                )
                .color_range(
                    self.primary_leader_key_color_index,
                    primary_modifier_key_text_len + 5..primary_modifier_key_text_len + 7,
                )
                .color_range(
                    self.mode_color_index,
                    primary_modifier_key_text_len + 16..primary_modifier_key_text_len + 21,
                ),
                NestedListItem::new(format!(
                    "{} g + t to enter TAB mode",
                    primary_modifier_key_text
                ))
                .indent(1)
                .color_range(
                    self.primary_leader_key_color_index,
                    ..primary_modifier_key_text_len + 3,
                )
                .color_range(
                    self.primary_leader_key_color_index,
                    primary_modifier_key_text_len + 5..primary_modifier_key_text_len + 7,
                )
                .color_range(
                    self.mode_color_index,
                    primary_modifier_key_text_len + 16..primary_modifier_key_text_len + 20,
                ),
            ];
            let max_width = widths.0;
            (list_items, max_width)
        } else if cols >= widths.1 {
            let list_items = vec![
                NestedListItem::new(unlock_first_text).color_range(self.preset_color_index, ..),
                NestedListItem::new(format!(
                    "Single key modes after {} g, eg.:",
                    primary_modifier_key_text
                ))
                .indent(1),
                NestedListItem::new(format!(
                    "{} g + p to enter PANE mode",
                    primary_modifier_key_text
                ))
                .color_range(
                    self.primary_leader_key_color_index,
                    ..primary_modifier_key_text_len + 3,
                )
                .color_range(
                    self.primary_leader_key_color_index,
                    primary_modifier_key_text_len + 5..primary_modifier_key_text_len + 7,
                )
                .color_range(
                    self.mode_color_index,
                    primary_modifier_key_text_len + 16..primary_modifier_key_text_len + 21,
                )
                .indent(1),
                NestedListItem::new(format!(
                    "{} g + t to enter TAB mode",
                    primary_modifier_key_text
                ))
                .color_range(
                    self.primary_leader_key_color_index,
                    ..primary_modifier_key_text_len + 3,
                )
                .color_range(
                    self.primary_leader_key_color_index,
                    primary_modifier_key_text_len + 5..primary_modifier_key_text_len + 7,
                )
                .color_range(
                    self.mode_color_index,
                    primary_modifier_key_text_len + 16..primary_modifier_key_text_len + 20,
                )
                .indent(1),
            ];
            let max_width = widths.1;
            (list_items, max_width)
        } else {
            let list_items = vec![
                NestedListItem::new("2. Unlock First").color_range(self.preset_color_index, ..),
                NestedListItem::new(format!(
                    "{} g + single key, eg.:",
                    primary_modifier_key_text
                ))
                .indent(1),
                NestedListItem::new(format!("{} g + p PANE mode", primary_modifier_key_text))
                    .color_range(
                        self.primary_leader_key_color_index,
                        ..primary_modifier_key_text_len + 3,
                    )
                    .color_range(
                        self.primary_leader_key_color_index,
                        primary_modifier_key_text_len + 5..primary_modifier_key_text_len + 7,
                    )
                    .color_range(
                        self.mode_color_index,
                        primary_modifier_key_text_len + 7..primary_modifier_key_text_len + 11,
                    )
                    .indent(1),
                NestedListItem::new(format!("{} g + t TAB mode", primary_modifier_key_text))
                    .color_range(
                        self.primary_leader_key_color_index,
                        ..primary_modifier_key_text_len + 3,
                    )
                    .color_range(
                        self.primary_leader_key_color_index,
                        primary_modifier_key_text_len + 5..primary_modifier_key_text_len + 7,
                    )
                    .color_range(
                        self.mode_color_index,
                        primary_modifier_key_text_len + 7..primary_modifier_key_text_len + 10,
                    )
                    .indent(1),
            ];
            let max_width = widths.2;
            (list_items, max_width)
        };
        if self.selected_index == Some(1) {
            list_items = list_items.drain(..).map(|i| i.selected()).collect();
        }
        let left_padding = cols.saturating_sub(max_width) / 2;
        let top_coordinates = if rows > 14 {
            (rows.saturating_sub(self.ui_size) / 2) + 7
        } else {
            (rows.saturating_sub(self.ui_size) / 2) + 5
        };
        print_nested_list_with_coordinates(
            list_items,
            left_padding,
            top_coordinates,
            Some(max_width),
            None,
        );
    }
    fn render_leader_keys_indication(
        &self,
        rows: usize,
        cols: usize,
        primary_modifier_key_text: &str,
        secondary_modifier_key_text: &str,
    ) {
        let widths = self.main_screen_widths(primary_modifier_key_text);
        let primary_modifier_key_text_len = primary_modifier_key_text.chars().count();
        let secondary_modifier_key_text_len = secondary_modifier_key_text.chars().count();
        let top_coordinates = if rows > 14 {
            (rows.saturating_sub(self.ui_size) / 2) + 12
        } else {
            (rows.saturating_sub(self.ui_size) / 2) + 9
        };

        if cols >= widths.0 {
            let leader_key_text = format!(
                "Leader keys: {} - modes, {} - quicknav and shortcuts",
                primary_modifier_key_text, secondary_modifier_key_text
            );
            let left_padding = cols.saturating_sub(widths.0) / 2;
            print_text_with_coordinates(
                Text::new(leader_key_text)
                    .color_range(2, ..12)
                    .color_range(
                        self.primary_leader_key_color_index,
                        13..primary_modifier_key_text_len + 14,
                    )
                    .color_range(
                        self.secondary_leader_key_color_index,
                        primary_modifier_key_text_len + 23
                            ..primary_modifier_key_text_len + 23 + secondary_modifier_key_text_len,
                    ),
                left_padding,
                top_coordinates,
                None,
                None,
            )
        } else {
            let leader_key_text = format!(
                "Leaders: {}, {}",
                primary_modifier_key_text, secondary_modifier_key_text
            );
            let left_padding = if cols >= widths.1 {
                cols.saturating_sub(widths.1) / 2
            } else {
                cols.saturating_sub(widths.2) / 2
            };
            print_text_with_coordinates(
                Text::new(leader_key_text)
                    .color_range(2, ..8)
                    .color_range(
                        self.primary_leader_key_color_index,
                        9..primary_modifier_key_text_len + 10,
                    )
                    .color_range(
                        self.secondary_leader_key_color_index,
                        primary_modifier_key_text_len + 11
                            ..primary_modifier_key_text_len + 12 + secondary_modifier_key_text_len,
                    ),
                left_padding,
                top_coordinates,
                None,
                None,
            )
        };
    }
    fn render_info_line(&self, rows: usize, cols: usize, primary_modifier_key_text: &str) {
        let widths = self.main_screen_widths(primary_modifier_key_text);
        let top_coordinates = if rows > 14 {
            (rows.saturating_sub(self.ui_size) / 2) + 14
        } else {
            (rows.saturating_sub(self.ui_size) / 2) + 10
        };
        let left_padding = if cols >= widths.0 {
            cols.saturating_sub(widths.0) / 2
        } else if cols >= widths.1 {
            cols.saturating_sub(widths.1) / 2
        } else {
            cols.saturating_sub(widths.2) / 2
        };
        if let Some(notification) = &self.notification {
            print_text_with_coordinates(
                Text::new(notification).color_range(3, ..),
                left_padding,
                top_coordinates,
                None,
                None,
            );
        } else if let Some(warning_text) = self.warning_text(cols) {
            print_text_with_coordinates(
                Text::new(warning_text).color_range(3, ..),
                left_padding,
                top_coordinates,
                None,
                None,
            );
        }
    }
    fn render_help_text_main(&self, rows: usize, cols: usize) {
        let full_help_text = "Help: <↓↑> - navigate, <ENTER> - apply, <SPACE> - apply & save, <l> - leaders, <ESC> - close";
        let short_help_text = "Help: <↓↑> / <ENTER> / <SPACE> / <l> / <ESC>";
        if cols >= full_help_text.chars().count() {
            print_text_with_coordinates(
                Text::new(full_help_text)
                    .color_range(2, 6..10)
                    .color_range(2, 23..30)
                    .color_range(2, 40..47)
                    .color_range(2, 64..67)
                    .color_range(2, 79..84),
                0,
                rows,
                None,
                None,
            );
        } else {
            print_text_with_coordinates(
                Text::new(short_help_text)
                    .color_range(2, 6..10)
                    .color_range(2, 13..20)
                    .color_range(2, 23..30)
                    .color_range(2, 33..36)
                    .color_range(2, 39..44),
                0,
                rows,
                None,
                None,
            );
        }
    }
    fn render_help_text_setup_wizard(&self, rows: usize, cols: usize) {
        let full_help_text =
            "Help: <↓↑> - navigate, <ENTER> - apply & save, <l> - change leaders, <ESC> - close";
        let short_help_text = "Help: <↓↑> / <ENTER> / <l> / <ESC>";
        if cols >= full_help_text.chars().count() {
            print_text_with_coordinates(
                Text::new(full_help_text)
                    .color_range(2, 6..10)
                    .color_range(2, 23..30)
                    .color_range(2, 47..50)
                    .color_range(2, 69..74),
                0,
                rows,
                None,
                None,
            );
        } else {
            print_text_with_coordinates(
                Text::new(short_help_text)
                    .color_range(2, 6..10)
                    .color_range(2, 13..20)
                    .color_range(2, 23..26)
                    .color_range(2, 29..34),
                0,
                rows,
                None,
                None,
            );
        }
    }
    fn render_help_text_remapping(&self, rows: usize, cols: usize) {
        let widths = self.remapping_screen_widths();
        if cols >= widths.0 {
            let help_text = "Help: <ENTER> - when done";
            print_text_with_coordinates(
                Text::new(help_text).color_range(2, 6..13),
                0,
                rows,
                None,
                None,
            );
        } else {
            let help_text = "Help: <ENTER> / <←↓↑→> / <SPACE>";
            print_text_with_coordinates(
                Text::new(help_text)
                    .color_range(2, 6..13)
                    .color_range(2, 16..22)
                    .color_range(2, 25..32),
                0,
                rows,
                None,
                None,
            );
        }
    }
    fn primary_modifier_text(&self) -> String {
        if self.primary_modifier.is_empty() {
            "<UNBOUND>".to_owned()
        } else {
            self.primary_modifier
                .iter()
                .map(|m| m.to_string())
                .collect::<Vec<_>>()
                .join("-")
        }
    }
    fn secondary_modifier_text(&self) -> String {
        if self.secondary_modifier.is_empty() {
            "<UNBOUND>".to_owned()
        } else {
            self.secondary_modifier
                .iter()
                .map(|m| m.to_string())
                .collect::<Vec<_>>()
                .join("-")
        }
    }
    fn main_screen_widths(&self, primary_modifier_text: &str) -> (usize, usize, usize) {
        let primary_modifier_key_text_len = primary_modifier_text.chars().count();
        let full_width = 61 + primary_modifier_key_text_len;
        let mid_width = 36 + primary_modifier_key_text_len;
        let min_width = 26 + primary_modifier_key_text_len;
        (full_width, mid_width, min_width)
    }
    fn remapping_screen_widths(&self) -> (usize, usize, usize) {
        let full_width = 62;
        let mid_width = 42;
        let min_width = 30;
        (full_width, mid_width, min_width)
    }
    fn render_main_screen(&mut self, rows: usize, cols: usize) {
        let primary_modifier_key_text = self.primary_modifier_text();
        let secondary_modifier_key_text = self.secondary_modifier_text();
        self.render_override_title(rows, cols, &primary_modifier_key_text);
        self.render_first_bulletin(rows, cols, &primary_modifier_key_text);
        self.render_second_bulletin(rows, cols, &primary_modifier_key_text);
        self.render_leader_keys_indication(
            rows,
            cols,
            &primary_modifier_key_text,
            &secondary_modifier_key_text,
        );
        self.render_info_line(rows, cols, &primary_modifier_key_text);
        self.render_help_text_main(rows, cols);
    }
    fn render_setup_wizard_screen(&mut self, rows: usize, cols: usize) {
        let primary_modifier_key_text = self.primary_modifier_text();
        let secondary_modifier_key_text = self.secondary_modifier_text();
        self.render_setup_wizard_title(rows, cols, &primary_modifier_key_text);
        self.render_first_bulletin(rows + 8, cols, &primary_modifier_key_text);
        self.render_second_bulletin(rows + 8, cols, &primary_modifier_key_text);
        self.render_leader_keys_indication(
            rows + 8,
            cols,
            &primary_modifier_key_text,
            &secondary_modifier_key_text,
        );
        self.render_info_line(rows + 8, cols, &primary_modifier_key_text);
        self.render_help_text_setup_wizard(rows + 8, cols);
    }
    fn warning_text(&self, max_width: usize) -> Option<String> {
        if self.needs_kitty_support() {
            // TODO: some widget to test support by detecting pressed keys
            if max_width >= 38 {
                Some(String::from("Warning: requires supporting terminal."))
            } else {
                Some(String::from("Requires supporting terminal"))
            }
        } else if self.primary_modifier.is_empty() && self.secondary_modifier.is_empty() {
            if max_width >= 49 {
                Some(String::from(
                    "Warning: no leaders defined. UI will be disabled.",
                ))
            } else {
                Some(String::from("No leaders. UI will be disabled."))
            }
        } else {
            None
        }
    }
    fn needs_kitty_support(&self) -> bool {
        self.primary_modifier.len() > 1
            || self.secondary_modifier.len() > 1
            || self.primary_modifier.contains(&KeyModifier::Super)
            || self.secondary_modifier.contains(&KeyModifier::Super)
    }
    fn reconfigure(&self, selected: usize, write_to_disk: bool) {
        if selected == 0 {
            // TODO: these should be part of a "transaction" when they are
            // implemented
            reconfigure(
                default_keybinds(
                    self.primary_modifier
                        .iter()
                        .map(|m| m.to_string())
                        .collect::<Vec<_>>()
                        .join(" "),
                    self.secondary_modifier
                        .iter()
                        .map(|m| m.to_string())
                        .collect::<Vec<_>>()
                        .join(" "),
                ),
                write_to_disk,
            );
            switch_to_input_mode(&InputMode::Normal);
        } else if selected == 1 {
            // TODO: these should be part of a "transaction" when they are
            // implemented
            reconfigure(
                unlock_first_keybinds(
                    self.primary_modifier
                        .iter()
                        .map(|m| m.to_string())
                        .collect::<Vec<_>>()
                        .join(" "),
                    self.secondary_modifier
                        .iter()
                        .map(|m| m.to_string())
                        .collect::<Vec<_>>()
                        .join(" "),
                ),
                write_to_disk,
            );
            switch_to_input_mode(&InputMode::Locked);
        }
    }
}

fn unlock_first_keybinds(primary_modifier: String, secondary_modifier: String) -> String {
    format!(
        r#"
default_mode "locked"
keybinds clear-defaults=true {{
    normal {{
    }}
    locked {{
        bind "{primary_modifier} g" {{ SwitchToMode "Normal"; }}
    }}
    resize {{
        bind "r" {{ SwitchToMode "Normal"; }}
        bind "h" "Left" {{ Resize "Increase Left"; }}
        bind "j" "Down" {{ Resize "Increase Down"; }}
        bind "k" "Up" {{ Resize "Increase Up"; }}
        bind "l" "Right" {{ Resize "Increase Right"; }}
        bind "H" {{ Resize "Decrease Left"; }}
        bind "J" {{ Resize "Decrease Down"; }}
        bind "K" {{ Resize "Decrease Up"; }}
        bind "L" {{ Resize "Decrease Right"; }}
        bind "=" "+" {{ Resize "Increase"; }}
        bind "-" {{ Resize "Decrease"; }}
    }}
    pane {{
        bind "p" {{ SwitchToMode "Normal"; }}
        bind "h" "Left" {{ MoveFocus "Left"; }}
        bind "l" "Right" {{ MoveFocus "Right"; }}
        bind "j" "Down" {{ MoveFocus "Down"; }}
        bind "k" "Up" {{ MoveFocus "Up"; }}
        bind "Tab" {{ SwitchFocus; }}
        bind "n" {{ NewPane; SwitchToMode "Locked"; }}
        bind "d" {{ NewPane "Down"; SwitchToMode "Locked"; }}
        bind "r" {{ NewPane "Right"; SwitchToMode "Locked"; }}
        bind "x" {{ CloseFocus; SwitchToMode "Locked"; }}
        bind "f" {{ ToggleFocusFullscreen; SwitchToMode "Locked"; }}
        bind "z" {{ TogglePaneFrames; SwitchToMode "Locked"; }}
        bind "w" {{ ToggleFloatingPanes; SwitchToMode "Locked"; }}
        bind "e" {{ TogglePaneEmbedOrFloating; SwitchToMode "Locked"; }}
        bind "c" {{ SwitchToMode "RenamePane"; PaneNameInput 0;}}
    }}
    move {{
        bind "m" {{ SwitchToMode "Normal"; }}
        bind "n" "Tab" {{ MovePane; }}
        bind "p" {{ MovePaneBackwards; }}
        bind "h" "Left" {{ MovePane "Left"; }}
        bind "j" "Down" {{ MovePane "Down"; }}
        bind "k" "Up" {{ MovePane "Up"; }}
        bind "l" "Right" {{ MovePane "Right"; }}
    }}
    tab {{
        bind "t" {{ SwitchToMode "Normal"; }}
        bind "r" {{ SwitchToMode "RenameTab"; TabNameInput 0; }}
        bind "h" "Left" "Up" "k" {{ GoToPreviousTab; }}
        bind "l" "Right" "Down" "j" {{ GoToNextTab; }}
        bind "n" {{ NewTab; SwitchToMode "Locked"; }}
        bind "x" {{ CloseTab; SwitchToMode "Locked"; }}
        bind "s" {{ ToggleActiveSyncTab; SwitchToMode "Locked"; }}
        bind "b" {{ BreakPane; SwitchToMode "Locked"; }}
        bind "]" {{ BreakPaneRight; SwitchToMode "Locked"; }}
        bind "[" {{ BreakPaneLeft; SwitchToMode "Locked"; }}
        bind "1" {{ GoToTab 1; SwitchToMode "Locked"; }}
        bind "2" {{ GoToTab 2; SwitchToMode "Locked"; }}
        bind "3" {{ GoToTab 3; SwitchToMode "Locked"; }}
        bind "4" {{ GoToTab 4; SwitchToMode "Locked"; }}
        bind "5" {{ GoToTab 5; SwitchToMode "Locked"; }}
        bind "6" {{ GoToTab 6; SwitchToMode "Locked"; }}
        bind "7" {{ GoToTab 7; SwitchToMode "Locked"; }}
        bind "8" {{ GoToTab 8; SwitchToMode "Locked"; }}
        bind "9" {{ GoToTab 9; SwitchToMode "Locked"; }}
        bind "Tab" {{ ToggleTab; }}
    }}
    scroll {{
        bind "s" {{ SwitchToMode "Normal"; }}
        bind "e" {{ EditScrollback; SwitchToMode "Locked"; }}
        bind "f" {{ SwitchToMode "EnterSearch"; SearchInput 0; }}
        bind "Ctrl c" {{ ScrollToBottom; SwitchToMode "Locked"; }}
        bind "j" "Down" {{ ScrollDown; }}
        bind "k" "Up" {{ ScrollUp; }}
        bind "Ctrl f" "PageDown" "Right" "l" {{ PageScrollDown; }}
        bind "Ctrl b" "PageUp" "Left" "h" {{ PageScrollUp; }}
        bind "d" {{ HalfPageScrollDown; }}
        bind "u" {{ HalfPageScrollUp; }}
    }}
    search {{
        bind "Ctrl c" {{ ScrollToBottom; SwitchToMode "Locked"; }}
        bind "j" "Down" {{ ScrollDown; }}
        bind "k" "Up" {{ ScrollUp; }}
        bind "Ctrl f" "PageDown" "Right" "l" {{ PageScrollDown; }}
        bind "Ctrl b" "PageUp" "Left" "h" {{ PageScrollUp; }}
        bind "d" {{ HalfPageScrollDown; }}
        bind "u" {{ HalfPageScrollUp; }}
        bind "n" {{ Search "down"; }}
        bind "p" {{ Search "up"; }}
        bind "c" {{ SearchToggleOption "CaseSensitivity"; }}
        bind "w" {{ SearchToggleOption "Wrap"; }}
        bind "o" {{ SearchToggleOption "WholeWord"; }}
    }}
    entersearch {{
        bind "Ctrl c" "Esc" {{ SwitchToMode "Scroll"; }}
        bind "Enter" {{ SwitchToMode "Search"; }}
    }}
    renametab {{
        bind "Ctrl c" "Enter" {{ SwitchToMode "Locked"; }}
        bind "Esc" {{ UndoRenameTab; SwitchToMode "Tab"; }}
    }}
    renamepane {{
        bind "Ctrl c" "Enter" {{ SwitchToMode "Locked"; }}
        bind "Esc" {{ UndoRenamePane; SwitchToMode "Pane"; }}
    }}
    session {{
        bind "o" {{ SwitchToMode "Normal"; }}
        bind "{primary_modifier} s" {{ SwitchToMode "Scroll"; }}
        bind "d" {{ Detach; }}
        bind "w" {{
            LaunchOrFocusPlugin "session-manager" {{
                floating true
                move_to_focused_tab true
            }};
            SwitchToMode "Locked"
        }}
        bind "c" {{
            LaunchOrFocusPlugin "configuration" {{
                floating true
                move_to_focused_tab true
            }};
            SwitchToMode "Locked"
        }}
        bind "p" {{
            LaunchOrFocusPlugin "plugin-manager" {{
                floating true
                move_to_focused_tab true
            }};
            SwitchToMode "Locked"
        }}
    }}
    shared_except "locked" "renametab" "renamepane" {{
        bind "{primary_modifier} g" {{ SwitchToMode "Locked"; }}
        bind "{primary_modifier} q" {{ Quit; }}
    }}
    shared_except "renamepane" "renametab" "entersearch" "locked" {{
        bind "esc" {{ SwitchToMode "locked"; }}
    }}
    shared_among "normal" "locked" {{
        bind "{secondary_modifier} n" {{ NewPane; }}
        bind "{secondary_modifier} f" {{ ToggleFloatingPanes; }}
        bind "{secondary_modifier} i" {{ MoveTab "Left"; }}
        bind "{secondary_modifier} o" {{ MoveTab "Right"; }}
        bind "{secondary_modifier} h" "{secondary_modifier} Left" {{ MoveFocusOrTab "Left"; }}
        bind "{secondary_modifier} l" "{secondary_modifier} Right" {{ MoveFocusOrTab "Right"; }}
        bind "{secondary_modifier} j" "{secondary_modifier} Down" {{ MoveFocus "Down"; }}
        bind "{secondary_modifier} k" "{secondary_modifier} Up" {{ MoveFocus "Up"; }}
        bind "{secondary_modifier} =" "{secondary_modifier} +" {{ Resize "Increase"; }}
        bind "{secondary_modifier} -" {{ Resize "Decrease"; }}
        bind "{secondary_modifier} [" {{ PreviousSwapLayout; }}
        bind "{secondary_modifier} ]" {{ NextSwapLayout; }}
    }}
    shared_except "locked" "renametab" "renamepane" {{
        bind "Enter" {{ SwitchToMode "Locked"; }}
    }}
    shared_except "pane" "locked" "renametab" "renamepane" "entersearch" {{
        bind "p" {{ SwitchToMode "Pane"; }}
    }}
    shared_except "resize" "locked" "renametab" "renamepane" "entersearch" {{
        bind "r" {{ SwitchToMode "Resize"; }}
    }}
    shared_except "scroll" "locked" "renametab" "renamepane" "entersearch" {{
        bind "s" {{ SwitchToMode "Scroll"; }}
    }}
    shared_except "session" "locked" "renametab" "renamepane" "entersearch" {{
        bind "o" {{ SwitchToMode "Session"; }}
    }}
    shared_except "tab" "locked" "renametab" "renamepane" "entersearch" {{
        bind "t" {{ SwitchToMode "Tab"; }}
    }}
    shared_except "move" "locked" "renametab" "renamepane" "entersearch" {{
        bind "m" {{ SwitchToMode "Move"; }}
    }}
}}"#
    )
}

fn default_keybinds(primary_modifier: String, secondary_modifier: String) -> String {
    if primary_modifier.is_empty() && secondary_modifier.is_empty() {
        return default_keybinds_no_modifiers();
    } else if primary_modifier == secondary_modifier {
        return non_colliding_default_keybinds(primary_modifier, secondary_modifier);
    } else if primary_modifier.is_empty() {
        return default_keybinds_no_primary_modifier(secondary_modifier);
    } else if secondary_modifier.is_empty() {
        return default_keybinds_no_secondary_modifier(primary_modifier);
    }
    format!(
        r#"
default_mode "normal"
keybinds clear-defaults=true {{
    normal {{}}
    locked {{
        bind "{primary_modifier} g" {{ SwitchToMode "Normal"; }}
    }}
    resize {{
        bind "{primary_modifier} n" {{ SwitchToMode "Normal"; }}
        bind "h" "Left" {{ Resize "Increase Left"; }}
        bind "j" "Down" {{ Resize "Increase Down"; }}
        bind "k" "Up" {{ Resize "Increase Up"; }}
        bind "l" "Right" {{ Resize "Increase Right"; }}
        bind "H" {{ Resize "Decrease Left"; }}
        bind "J" {{ Resize "Decrease Down"; }}
        bind "K" {{ Resize "Decrease Up"; }}
        bind "L" {{ Resize "Decrease Right"; }}
        bind "=" "+" {{ Resize "Increase"; }}
        bind "-" {{ Resize "Decrease"; }}
    }}
    pane {{
        bind "{primary_modifier} p" {{ SwitchToMode "Normal"; }}
        bind "h" "Left" {{ MoveFocus "Left"; }}
        bind "l" "Right" {{ MoveFocus "Right"; }}
        bind "j" "Down" {{ MoveFocus "Down"; }}
        bind "k" "Up" {{ MoveFocus "Up"; }}
        bind "p" {{ SwitchFocus; }}
        bind "n" {{ NewPane; SwitchToMode "Normal"; }}
        bind "d" {{ NewPane "Down"; SwitchToMode "Normal"; }}
        bind "r" {{ NewPane "Right"; SwitchToMode "Normal"; }}
        bind "x" {{ CloseFocus; SwitchToMode "Normal"; }}
        bind "f" {{ ToggleFocusFullscreen; SwitchToMode "Normal"; }}
        bind "z" {{ TogglePaneFrames; SwitchToMode "Normal"; }}
        bind "w" {{ ToggleFloatingPanes; SwitchToMode "Normal"; }}
        bind "e" {{ TogglePaneEmbedOrFloating; SwitchToMode "Normal"; }}
        bind "c" {{ SwitchToMode "RenamePane"; PaneNameInput 0;}}
    }}
    move {{
        bind "{primary_modifier} h" {{ SwitchToMode "Normal"; }}
        bind "n" "Tab" {{ MovePane; }}
        bind "p" {{ MovePaneBackwards; }}
        bind "h" "Left" {{ MovePane "Left"; }}
        bind "j" "Down" {{ MovePane "Down"; }}
        bind "k" "Up" {{ MovePane "Up"; }}
        bind "l" "Right" {{ MovePane "Right"; }}
    }}
    tab {{
        bind "{primary_modifier} t" {{ SwitchToMode "Normal"; }}
        bind "r" {{ SwitchToMode "RenameTab"; TabNameInput 0; }}
        bind "h" "Left" "Up" "k" {{ GoToPreviousTab; }}
        bind "l" "Right" "Down" "j" {{ GoToNextTab; }}
        bind "n" {{ NewTab; SwitchToMode "Normal"; }}
        bind "x" {{ CloseTab; SwitchToMode "Normal"; }}
        bind "s" {{ ToggleActiveSyncTab; SwitchToMode "Normal"; }}
        bind "b" {{ BreakPane; SwitchToMode "Normal"; }}
        bind "]" {{ BreakPaneRight; SwitchToMode "Normal"; }}
        bind "[" {{ BreakPaneLeft; SwitchToMode "Normal"; }}
        bind "1" {{ GoToTab 1; SwitchToMode "Normal"; }}
        bind "2" {{ GoToTab 2; SwitchToMode "Normal"; }}
        bind "3" {{ GoToTab 3; SwitchToMode "Normal"; }}
        bind "4" {{ GoToTab 4; SwitchToMode "Normal"; }}
        bind "5" {{ GoToTab 5; SwitchToMode "Normal"; }}
        bind "6" {{ GoToTab 6; SwitchToMode "Normal"; }}
        bind "7" {{ GoToTab 7; SwitchToMode "Normal"; }}
        bind "8" {{ GoToTab 8; SwitchToMode "Normal"; }}
        bind "9" {{ GoToTab 9; SwitchToMode "Normal"; }}
        bind "Tab" {{ ToggleTab; }}
    }}
    scroll {{
        bind "{primary_modifier} s" {{ SwitchToMode "Normal"; }}
        bind "e" {{ EditScrollback; SwitchToMode "Normal"; }}
        bind "s" {{ SwitchToMode "EnterSearch"; SearchInput 0; }}
        bind "Ctrl c" {{ ScrollToBottom; SwitchToMode "Normal"; }}
        bind "j" "Down" {{ ScrollDown; }}
        bind "k" "Up" {{ ScrollUp; }}
        bind "Ctrl f" "PageDown" "Right" "l" {{ PageScrollDown; }}
        bind "Ctrl b" "PageUp" "Left" "h" {{ PageScrollUp; }}
        bind "d" {{ HalfPageScrollDown; }}
        bind "u" {{ HalfPageScrollUp; }}
    }}
    search {{
        bind "{primary_modifier} s" {{ SwitchToMode "Normal"; }}
        bind "Ctrl c" {{ ScrollToBottom; SwitchToMode "Normal"; }}
        bind "j" "Down" {{ ScrollDown; }}
        bind "k" "Up" {{ ScrollUp; }}
        bind "Ctrl f" "PageDown" "Right" "l" {{ PageScrollDown; }}
        bind "Ctrl b" "PageUp" "Left" "h" {{ PageScrollUp; }}
        bind "d" {{ HalfPageScrollDown; }}
        bind "u" {{ HalfPageScrollUp; }}
        bind "n" {{ Search "down"; }}
        bind "p" {{ Search "up"; }}
        bind "c" {{ SearchToggleOption "CaseSensitivity"; }}
        bind "w" {{ SearchToggleOption "Wrap"; }}
        bind "o" {{ SearchToggleOption "WholeWord"; }}
    }}
    entersearch {{
        bind "Ctrl c" "Esc" {{ SwitchToMode "Scroll"; }}
        bind "Enter" {{ SwitchToMode "Search"; }}
    }}
    renametab {{
        bind "Ctrl c" {{ SwitchToMode "Normal"; }}
        bind "Esc" {{ UndoRenameTab; SwitchToMode "Tab"; }}
    }}
    renamepane {{
        bind "Ctrl c" {{ SwitchToMode "Normal"; }}
        bind "Esc" {{ UndoRenamePane; SwitchToMode "Pane"; }}
    }}
    session {{
        bind "{primary_modifier} o" {{ SwitchToMode "Normal"; }}
        bind "{primary_modifier} s" {{ SwitchToMode "Scroll"; }}
        bind "d" {{ Detach; }}
        bind "w" {{
            LaunchOrFocusPlugin "session-manager" {{
                floating true
                move_to_focused_tab true
            }};
            SwitchToMode "Normal"
        }}
        bind "c" {{
            LaunchOrFocusPlugin "configuration" {{
                floating true
                move_to_focused_tab true
            }};
            SwitchToMode "Normal"
        }}
        bind "p" {{
            LaunchOrFocusPlugin "plugin-manager" {{
                floating true
                move_to_focused_tab true
            }};
            SwitchToMode "Normal"
        }}
    }}
    tmux {{
        bind "[" {{ SwitchToMode "Scroll"; }}
        bind "{primary_modifier} b" {{ Write 2; SwitchToMode "Normal"; }}
        bind "\"" {{ NewPane "Down"; SwitchToMode "Normal"; }}
        bind "%" {{ NewPane "Right"; SwitchToMode "Normal"; }}
        bind "z" {{ ToggleFocusFullscreen; SwitchToMode "Normal"; }}
        bind "c" {{ NewTab; SwitchToMode "Normal"; }}
        bind "," {{ SwitchToMode "RenameTab"; }}
        bind "p" {{ GoToPreviousTab; SwitchToMode "Normal"; }}
        bind "n" {{ GoToNextTab; SwitchToMode "Normal"; }}
        bind "Left" {{ MoveFocus "Left"; SwitchToMode "Normal"; }}
        bind "Right" {{ MoveFocus "Right"; SwitchToMode "Normal"; }}
        bind "Down" {{ MoveFocus "Down"; SwitchToMode "Normal"; }}
        bind "Up" {{ MoveFocus "Up"; SwitchToMode "Normal"; }}
        bind "h" {{ MoveFocus "Left"; SwitchToMode "Normal"; }}
        bind "l" {{ MoveFocus "Right"; SwitchToMode "Normal"; }}
        bind "j" {{ MoveFocus "Down"; SwitchToMode "Normal"; }}
        bind "k" {{ MoveFocus "Up"; SwitchToMode "Normal"; }}
        bind "o" {{ FocusNextPane; }}
        bind "d" {{ Detach; }}
        bind "Space" {{ NextSwapLayout; }}
        bind "x" {{ CloseFocus; SwitchToMode "Normal"; }}
    }}
    shared_except "locked" {{
        bind "{primary_modifier} g" {{ SwitchToMode "Locked"; }}
        bind "{primary_modifier} q" {{ Quit; }}
        bind "{secondary_modifier} f" {{ ToggleFloatingPanes; }}
        bind "{secondary_modifier} n" {{ NewPane; }}
        bind "{secondary_modifier} i" {{ MoveTab "Left"; }}
        bind "{secondary_modifier} o" {{ MoveTab "Right"; }}
        bind "{secondary_modifier} h" "{secondary_modifier} Left" {{ MoveFocusOrTab "Left"; }}
        bind "{secondary_modifier} l" "{secondary_modifier} Right" {{ MoveFocusOrTab "Right"; }}
        bind "{secondary_modifier} j" "{secondary_modifier} Down" {{ MoveFocus "Down"; }}
        bind "{secondary_modifier} k" "{secondary_modifier} Up" {{ MoveFocus "Up"; }}
        bind "{secondary_modifier} =" "{secondary_modifier} +" {{ Resize "Increase"; }}
        bind "{secondary_modifier} -" {{ Resize "Decrease"; }}
        bind "{secondary_modifier} [" {{ PreviousSwapLayout; }}
        bind "{secondary_modifier} ]" {{ NextSwapLayout; }}
    }}
    shared_except "normal" "locked" {{
        bind "Enter" "Esc" {{ SwitchToMode "Normal"; }}
    }}
    shared_except "pane" "locked" {{
        bind "{primary_modifier} p" {{ SwitchToMode "Pane"; }}
    }}
    shared_except "resize" "locked" {{
        bind "{primary_modifier} n" {{ SwitchToMode "Resize"; }}
    }}
    shared_except "scroll" "locked" {{
        bind "{primary_modifier} s" {{ SwitchToMode "Scroll"; }}
    }}
    shared_except "session" "locked" {{
        bind "{primary_modifier} o" {{ SwitchToMode "Session"; }}
    }}
    shared_except "tab" "locked" {{
        bind "{primary_modifier} t" {{ SwitchToMode "Tab"; }}
    }}
    shared_except "move" "locked" {{
        bind "{primary_modifier} h" {{ SwitchToMode "Move"; }}
    }}
    shared_except "tmux" "locked" {{
        bind "{primary_modifier} b" {{ SwitchToMode "Tmux"; }}
    }}
}}
"#
    )
}

fn default_keybinds_no_primary_modifier(secondary_modifier: String) -> String {
    format!(
        r#"
default_mode "normal"
keybinds clear-defaults=true {{
    normal {{}}
    locked {{}}
    resize {{
        bind "h" "Left" {{ Resize "Increase Left"; }}
        bind "j" "Down" {{ Resize "Increase Down"; }}
        bind "k" "Up" {{ Resize "Increase Up"; }}
        bind "l" "Right" {{ Resize "Increase Right"; }}
        bind "H" {{ Resize "Decrease Left"; }}
        bind "J" {{ Resize "Decrease Down"; }}
        bind "K" {{ Resize "Decrease Up"; }}
        bind "L" {{ Resize "Decrease Right"; }}
        bind "=" "+" {{ Resize "Increase"; }}
        bind "-" {{ Resize "Decrease"; }}
    }}
    pane {{
        bind "h" "Left" {{ MoveFocus "Left"; }}
        bind "l" "Right" {{ MoveFocus "Right"; }}
        bind "j" "Down" {{ MoveFocus "Down"; }}
        bind "k" "Up" {{ MoveFocus "Up"; }}
        bind "p" {{ SwitchFocus; }}
        bind "n" {{ NewPane; SwitchToMode "Normal"; }}
        bind "d" {{ NewPane "Down"; SwitchToMode "Normal"; }}
        bind "r" {{ NewPane "Right"; SwitchToMode "Normal"; }}
        bind "x" {{ CloseFocus; SwitchToMode "Normal"; }}
        bind "f" {{ ToggleFocusFullscreen; SwitchToMode "Normal"; }}
        bind "z" {{ TogglePaneFrames; SwitchToMode "Normal"; }}
        bind "w" {{ ToggleFloatingPanes; SwitchToMode "Normal"; }}
        bind "e" {{ TogglePaneEmbedOrFloating; SwitchToMode "Normal"; }}
        bind "c" {{ SwitchToMode "RenamePane"; PaneNameInput 0;}}
    }}
    move {{
        bind "n" "Tab" {{ MovePane; }}
        bind "p" {{ MovePaneBackwards; }}
        bind "h" "Left" {{ MovePane "Left"; }}
        bind "j" "Down" {{ MovePane "Down"; }}
        bind "k" "Up" {{ MovePane "Up"; }}
        bind "l" "Right" {{ MovePane "Right"; }}
    }}
    tab {{
        bind "r" {{ SwitchToMode "RenameTab"; TabNameInput 0; }}
        bind "h" "Left" "Up" "k" {{ GoToPreviousTab; }}
        bind "l" "Right" "Down" "j" {{ GoToNextTab; }}
        bind "n" {{ NewTab; SwitchToMode "Normal"; }}
        bind "x" {{ CloseTab; SwitchToMode "Normal"; }}
        bind "s" {{ ToggleActiveSyncTab; SwitchToMode "Normal"; }}
        bind "b" {{ BreakPane; SwitchToMode "Normal"; }}
        bind "]" {{ BreakPaneRight; SwitchToMode "Normal"; }}
        bind "[" {{ BreakPaneLeft; SwitchToMode "Normal"; }}
        bind "1" {{ GoToTab 1; SwitchToMode "Normal"; }}
        bind "2" {{ GoToTab 2; SwitchToMode "Normal"; }}
        bind "3" {{ GoToTab 3; SwitchToMode "Normal"; }}
        bind "4" {{ GoToTab 4; SwitchToMode "Normal"; }}
        bind "5" {{ GoToTab 5; SwitchToMode "Normal"; }}
        bind "6" {{ GoToTab 6; SwitchToMode "Normal"; }}
        bind "7" {{ GoToTab 7; SwitchToMode "Normal"; }}
        bind "8" {{ GoToTab 8; SwitchToMode "Normal"; }}
        bind "9" {{ GoToTab 9; SwitchToMode "Normal"; }}
        bind "Tab" {{ ToggleTab; }}
    }}
    scroll {{
        bind "e" {{ EditScrollback; SwitchToMode "Normal"; }}
        bind "s" {{ SwitchToMode "EnterSearch"; SearchInput 0; }}
        bind "j" "Down" {{ ScrollDown; }}
        bind "k" "Up" {{ ScrollUp; }}
        bind "d" {{ HalfPageScrollDown; }}
        bind "u" {{ HalfPageScrollUp; }}
    }}
    search {{
        bind "Ctrl c" {{ ScrollToBottom; SwitchToMode "Normal"; }}
        bind "j" "Down" {{ ScrollDown; }}
        bind "k" "Up" {{ ScrollUp; }}
        bind "Ctrl f" "PageDown" "Right" "l" {{ PageScrollDown; }}
        bind "Ctrl b" "PageUp" "Left" "h" {{ PageScrollUp; }}
        bind "d" {{ HalfPageScrollDown; }}
        bind "u" {{ HalfPageScrollUp; }}
        bind "n" {{ Search "down"; }}
        bind "p" {{ Search "up"; }}
        bind "c" {{ SearchToggleOption "CaseSensitivity"; }}
        bind "w" {{ SearchToggleOption "Wrap"; }}
        bind "o" {{ SearchToggleOption "WholeWord"; }}
    }}
    entersearch {{
        bind "Ctrl c" "Esc" {{ SwitchToMode "Scroll"; }}
        bind "Enter" {{ SwitchToMode "Search"; }}
    }}
    renametab {{
        bind "Ctrl c" {{ SwitchToMode "Normal"; }}
        bind "Esc" {{ UndoRenameTab; SwitchToMode "Tab"; }}
    }}
    renamepane {{
        bind "Ctrl c" {{ SwitchToMode "Normal"; }}
        bind "Esc" {{ UndoRenamePane; SwitchToMode "Pane"; }}
    }}
    session {{
        bind "d" {{ Detach; }}
        bind "w" {{
            LaunchOrFocusPlugin "session-manager" {{
                floating true
                move_to_focused_tab true
            }};
            SwitchToMode "Normal"
        }}
        bind "c" {{
            LaunchOrFocusPlugin "configuration" {{
                floating true
                move_to_focused_tab true
            }};
            SwitchToMode "Normal"
        }}
        bind "p" {{
            LaunchOrFocusPlugin "plugin-manager" {{
                floating true
                move_to_focused_tab true
            }};
            SwitchToMode "Normal"
        }}
    }}
    tmux {{
        bind "[" {{ SwitchToMode "Scroll"; }}
        bind "\"" {{ NewPane "Down"; SwitchToMode "Normal"; }}
        bind "%" {{ NewPane "Right"; SwitchToMode "Normal"; }}
        bind "z" {{ ToggleFocusFullscreen; SwitchToMode "Normal"; }}
        bind "c" {{ NewTab; SwitchToMode "Normal"; }}
        bind "," {{ SwitchToMode "RenameTab"; }}
        bind "p" {{ GoToPreviousTab; SwitchToMode "Normal"; }}
        bind "n" {{ GoToNextTab; SwitchToMode "Normal"; }}
        bind "Left" {{ MoveFocus "Left"; SwitchToMode "Normal"; }}
        bind "Right" {{ MoveFocus "Right"; SwitchToMode "Normal"; }}
        bind "Down" {{ MoveFocus "Down"; SwitchToMode "Normal"; }}
        bind "Up" {{ MoveFocus "Up"; SwitchToMode "Normal"; }}
        bind "h" {{ MoveFocus "Left"; SwitchToMode "Normal"; }}
        bind "l" {{ MoveFocus "Right"; SwitchToMode "Normal"; }}
        bind "j" {{ MoveFocus "Down"; SwitchToMode "Normal"; }}
        bind "k" {{ MoveFocus "Up"; SwitchToMode "Normal"; }}
        bind "o" {{ FocusNextPane; }}
        bind "d" {{ Detach; }}
        bind "Space" {{ NextSwapLayout; }}
        bind "x" {{ CloseFocus; SwitchToMode "Normal"; }}
    }}
    shared_except "locked" {{
        bind "{secondary_modifier} n" {{ NewPane; }}
        bind "{secondary_modifier} f" {{ ToggleFloatingPanes; }}
        bind "{secondary_modifier} i" {{ MoveTab "Left"; }}
        bind "{secondary_modifier} o" {{ MoveTab "Right"; }}
        bind "{secondary_modifier} h" "{secondary_modifier} Left" {{ MoveFocusOrTab "Left"; }}
        bind "{secondary_modifier} l" "{secondary_modifier} Right" {{ MoveFocusOrTab "Right"; }}
        bind "{secondary_modifier} j" "{secondary_modifier} Down" {{ MoveFocus "Down"; }}
        bind "{secondary_modifier} k" "{secondary_modifier} Up" {{ MoveFocus "Up"; }}
        bind "{secondary_modifier} =" "{secondary_modifier} +" {{ Resize "Increase"; }}
        bind "{secondary_modifier} -" {{ Resize "Decrease"; }}
        bind "{secondary_modifier} [" {{ PreviousSwapLayout; }}
        bind "{secondary_modifier} ]" {{ NextSwapLayout; }}
    }}
    shared_except "normal" "locked" {{
        bind "Enter" "Esc" {{ SwitchToMode "Normal"; }}
    }}
}}
"#
    )
}

fn default_keybinds_no_secondary_modifier(primary_modifier: String) -> String {
    format!(
        r#"
default_mode "normal"
keybinds clear-defaults=true {{
    normal {{}}
    locked {{
        bind "{primary_modifier} g" {{ SwitchToMode "Normal"; }}
    }}
    resize {{
        bind "{primary_modifier} n" {{ SwitchToMode "Normal"; }}
        bind "h" "Left" {{ Resize "Increase Left"; }}
        bind "j" "Down" {{ Resize "Increase Down"; }}
        bind "k" "Up" {{ Resize "Increase Up"; }}
        bind "l" "Right" {{ Resize "Increase Right"; }}
        bind "H" {{ Resize "Decrease Left"; }}
        bind "J" {{ Resize "Decrease Down"; }}
        bind "K" {{ Resize "Decrease Up"; }}
        bind "L" {{ Resize "Decrease Right"; }}
        bind "=" "+" {{ Resize "Increase"; }}
        bind "-" {{ Resize "Decrease"; }}
    }}
    pane {{
        bind "{primary_modifier} p" {{ SwitchToMode "Normal"; }}
        bind "h" "Left" {{ MoveFocus "Left"; }}
        bind "l" "Right" {{ MoveFocus "Right"; }}
        bind "j" "Down" {{ MoveFocus "Down"; }}
        bind "k" "Up" {{ MoveFocus "Up"; }}
        bind "p" {{ SwitchFocus; }}
        bind "n" {{ NewPane; SwitchToMode "Normal"; }}
        bind "d" {{ NewPane "Down"; SwitchToMode "Normal"; }}
        bind "r" {{ NewPane "Right"; SwitchToMode "Normal"; }}
        bind "x" {{ CloseFocus; SwitchToMode "Normal"; }}
        bind "f" {{ ToggleFocusFullscreen; SwitchToMode "Normal"; }}
        bind "z" {{ TogglePaneFrames; SwitchToMode "Normal"; }}
        bind "w" {{ ToggleFloatingPanes; SwitchToMode "Normal"; }}
        bind "e" {{ TogglePaneEmbedOrFloating; SwitchToMode "Normal"; }}
        bind "c" {{ SwitchToMode "RenamePane"; PaneNameInput 0;}}
    }}
    move {{
        bind "{primary_modifier} h" {{ SwitchToMode "Normal"; }}
        bind "n" "Tab" {{ MovePane; }}
        bind "p" {{ MovePaneBackwards; }}
        bind "h" "Left" {{ MovePane "Left"; }}
        bind "j" "Down" {{ MovePane "Down"; }}
        bind "k" "Up" {{ MovePane "Up"; }}
        bind "l" "Right" {{ MovePane "Right"; }}
    }}
    tab {{
        bind "{primary_modifier} t" {{ SwitchToMode "Normal"; }}
        bind "r" {{ SwitchToMode "RenameTab"; TabNameInput 0; }}
        bind "h" "Left" "Up" "k" {{ GoToPreviousTab; }}
        bind "l" "Right" "Down" "j" {{ GoToNextTab; }}
        bind "n" {{ NewTab; SwitchToMode "Normal"; }}
        bind "x" {{ CloseTab; SwitchToMode "Normal"; }}
        bind "s" {{ ToggleActiveSyncTab; SwitchToMode "Normal"; }}
        bind "b" {{ BreakPane; SwitchToMode "Normal"; }}
        bind "]" {{ BreakPaneRight; SwitchToMode "Normal"; }}
        bind "[" {{ BreakPaneLeft; SwitchToMode "Normal"; }}
        bind "1" {{ GoToTab 1; SwitchToMode "Normal"; }}
        bind "2" {{ GoToTab 2; SwitchToMode "Normal"; }}
        bind "3" {{ GoToTab 3; SwitchToMode "Normal"; }}
        bind "4" {{ GoToTab 4; SwitchToMode "Normal"; }}
        bind "5" {{ GoToTab 5; SwitchToMode "Normal"; }}
        bind "6" {{ GoToTab 6; SwitchToMode "Normal"; }}
        bind "7" {{ GoToTab 7; SwitchToMode "Normal"; }}
        bind "8" {{ GoToTab 8; SwitchToMode "Normal"; }}
        bind "9" {{ GoToTab 9; SwitchToMode "Normal"; }}
        bind "Tab" {{ ToggleTab; }}
    }}
    scroll {{
        bind "{primary_modifier} s" {{ SwitchToMode "Normal"; }}
        bind "e" {{ EditScrollback; SwitchToMode "Normal"; }}
        bind "s" {{ SwitchToMode "EnterSearch"; SearchInput 0; }}
        bind "Ctrl c" {{ ScrollToBottom; SwitchToMode "Normal"; }}
        bind "j" "Down" {{ ScrollDown; }}
        bind "k" "Up" {{ ScrollUp; }}
        bind "Ctrl f" "PageDown" "Right" "l" {{ PageScrollDown; }}
        bind "Ctrl b" "PageUp" "Left" "h" {{ PageScrollUp; }}
        bind "d" {{ HalfPageScrollDown; }}
        bind "u" {{ HalfPageScrollUp; }}
    }}
    search {{
        bind "{primary_modifier} s" {{ SwitchToMode "Normal"; }}
        bind "Ctrl c" {{ ScrollToBottom; SwitchToMode "Normal"; }}
        bind "j" "Down" {{ ScrollDown; }}
        bind "k" "Up" {{ ScrollUp; }}
        bind "Ctrl f" "PageDown" "Right" "l" {{ PageScrollDown; }}
        bind "Ctrl b" "PageUp" "Left" "h" {{ PageScrollUp; }}
        bind "d" {{ HalfPageScrollDown; }}
        bind "u" {{ HalfPageScrollUp; }}
        bind "n" {{ Search "down"; }}
        bind "p" {{ Search "up"; }}
        bind "c" {{ SearchToggleOption "CaseSensitivity"; }}
        bind "w" {{ SearchToggleOption "Wrap"; }}
        bind "o" {{ SearchToggleOption "WholeWord"; }}
    }}
    entersearch {{
        bind "Ctrl c" "Esc" {{ SwitchToMode "Scroll"; }}
        bind "Enter" {{ SwitchToMode "Search"; }}
    }}
    renametab {{
        bind "Ctrl c" {{ SwitchToMode "Normal"; }}
        bind "Esc" {{ UndoRenameTab; SwitchToMode "Tab"; }}
    }}
    renamepane {{
        bind "Ctrl c" {{ SwitchToMode "Normal"; }}
        bind "Esc" {{ UndoRenamePane; SwitchToMode "Pane"; }}
    }}
    session {{
        bind "{primary_modifier} o" {{ SwitchToMode "Normal"; }}
        bind "{primary_modifier} s" {{ SwitchToMode "Scroll"; }}
        bind "d" {{ Detach; }}
        bind "w" {{
            LaunchOrFocusPlugin "session-manager" {{
                floating true
                move_to_focused_tab true
            }};
            SwitchToMode "Normal"
        }}
        bind "c" {{
            LaunchOrFocusPlugin "configuration" {{
                floating true
                move_to_focused_tab true
            }};
            SwitchToMode "Normal"
        }}
        bind "p" {{
            LaunchOrFocusPlugin "plugin-manager" {{
                floating true
                move_to_focused_tab true
            }};
            SwitchToMode "Normal"
        }}
    }}
    tmux {{
        bind "[" {{ SwitchToMode "Scroll"; }}
        bind "{primary_modifier} b" {{ Write 2; SwitchToMode "Normal"; }}
        bind "\"" {{ NewPane "Down"; SwitchToMode "Normal"; }}
        bind "%" {{ NewPane "Right"; SwitchToMode "Normal"; }}
        bind "z" {{ ToggleFocusFullscreen; SwitchToMode "Normal"; }}
        bind "c" {{ NewTab; SwitchToMode "Normal"; }}
        bind "," {{ SwitchToMode "RenameTab"; }}
        bind "p" {{ GoToPreviousTab; SwitchToMode "Normal"; }}
        bind "n" {{ GoToNextTab; SwitchToMode "Normal"; }}
        bind "Left" {{ MoveFocus "Left"; SwitchToMode "Normal"; }}
        bind "Right" {{ MoveFocus "Right"; SwitchToMode "Normal"; }}
        bind "Down" {{ MoveFocus "Down"; SwitchToMode "Normal"; }}
        bind "Up" {{ MoveFocus "Up"; SwitchToMode "Normal"; }}
        bind "h" {{ MoveFocus "Left"; SwitchToMode "Normal"; }}
        bind "l" {{ MoveFocus "Right"; SwitchToMode "Normal"; }}
        bind "j" {{ MoveFocus "Down"; SwitchToMode "Normal"; }}
        bind "k" {{ MoveFocus "Up"; SwitchToMode "Normal"; }}
        bind "o" {{ FocusNextPane; }}
        bind "d" {{ Detach; }}
        bind "Space" {{ NextSwapLayout; }}
        bind "x" {{ CloseFocus; SwitchToMode "Normal"; }}
    }}
    shared_except "locked" {{
        bind "{primary_modifier} g" {{ SwitchToMode "Locked"; }}
        bind "{primary_modifier} q" {{ Quit; }}
    }}
    shared_except "normal" "locked" {{
        bind "Enter" "Esc" {{ SwitchToMode "Normal"; }}
    }}
    shared_except "pane" "locked" {{
        bind "{primary_modifier} p" {{ SwitchToMode "Pane"; }}
    }}
    shared_except "resize" "locked" {{
        bind "{primary_modifier} n" {{ SwitchToMode "Resize"; }}
    }}
    shared_except "scroll" "locked" {{
        bind "{primary_modifier} s" {{ SwitchToMode "Scroll"; }}
    }}
    shared_except "session" "locked" {{
        bind "{primary_modifier} o" {{ SwitchToMode "Session"; }}
    }}
    shared_except "tab" "locked" {{
        bind "{primary_modifier} t" {{ SwitchToMode "Tab"; }}
    }}
    shared_except "move" "locked" {{
        bind "{primary_modifier} h" {{ SwitchToMode "Move"; }}
    }}
    shared_except "tmux" "locked" {{
        bind "{primary_modifier} b" {{ SwitchToMode "Tmux"; }}
    }}
}}
"#
    )
}

fn default_keybinds_no_modifiers() -> String {
    format!(
        r#"
default_mode "normal"
keybinds clear-defaults=true {{
    normal {{}}
    locked {{}}
    resize {{
        bind "h" "Left" {{ Resize "Increase Left"; }}
        bind "j" "Down" {{ Resize "Increase Down"; }}
        bind "k" "Up" {{ Resize "Increase Up"; }}
        bind "l" "Right" {{ Resize "Increase Right"; }}
        bind "H" {{ Resize "Decrease Left"; }}
        bind "J" {{ Resize "Decrease Down"; }}
        bind "K" {{ Resize "Decrease Up"; }}
        bind "L" {{ Resize "Decrease Right"; }}
        bind "=" "+" {{ Resize "Increase"; }}
        bind "-" {{ Resize "Decrease"; }}
    }}
    pane {{
        bind "h" "Left" {{ MoveFocus "Left"; }}
        bind "l" "Right" {{ MoveFocus "Right"; }}
        bind "j" "Down" {{ MoveFocus "Down"; }}
        bind "k" "Up" {{ MoveFocus "Up"; }}
        bind "p" {{ SwitchFocus; }}
        bind "n" {{ NewPane; SwitchToMode "Normal"; }}
        bind "d" {{ NewPane "Down"; SwitchToMode "Normal"; }}
        bind "r" {{ NewPane "Right"; SwitchToMode "Normal"; }}
        bind "x" {{ CloseFocus; SwitchToMode "Normal"; }}
        bind "f" {{ ToggleFocusFullscreen; SwitchToMode "Normal"; }}
        bind "z" {{ TogglePaneFrames; SwitchToMode "Normal"; }}
        bind "w" {{ ToggleFloatingPanes; SwitchToMode "Normal"; }}
        bind "e" {{ TogglePaneEmbedOrFloating; SwitchToMode "Normal"; }}
        bind "c" {{ SwitchToMode "RenamePane"; PaneNameInput 0;}}
    }}
    move {{
        bind "n" "Tab" {{ MovePane; }}
        bind "p" {{ MovePaneBackwards; }}
        bind "h" "Left" {{ MovePane "Left"; }}
        bind "j" "Down" {{ MovePane "Down"; }}
        bind "k" "Up" {{ MovePane "Up"; }}
        bind "l" "Right" {{ MovePane "Right"; }}
    }}
    tab {{
        bind "r" {{ SwitchToMode "RenameTab"; TabNameInput 0; }}
        bind "h" "Left" "Up" "k" {{ GoToPreviousTab; }}
        bind "l" "Right" "Down" "j" {{ GoToNextTab; }}
        bind "n" {{ NewTab; SwitchToMode "Normal"; }}
        bind "x" {{ CloseTab; SwitchToMode "Normal"; }}
        bind "s" {{ ToggleActiveSyncTab; SwitchToMode "Normal"; }}
        bind "b" {{ BreakPane; SwitchToMode "Normal"; }}
        bind "]" {{ BreakPaneRight; SwitchToMode "Normal"; }}
        bind "[" {{ BreakPaneLeft; SwitchToMode "Normal"; }}
        bind "1" {{ GoToTab 1; SwitchToMode "Normal"; }}
        bind "2" {{ GoToTab 2; SwitchToMode "Normal"; }}
        bind "3" {{ GoToTab 3; SwitchToMode "Normal"; }}
        bind "4" {{ GoToTab 4; SwitchToMode "Normal"; }}
        bind "5" {{ GoToTab 5; SwitchToMode "Normal"; }}
        bind "6" {{ GoToTab 6; SwitchToMode "Normal"; }}
        bind "7" {{ GoToTab 7; SwitchToMode "Normal"; }}
        bind "8" {{ GoToTab 8; SwitchToMode "Normal"; }}
        bind "9" {{ GoToTab 9; SwitchToMode "Normal"; }}
        bind "Tab" {{ ToggleTab; }}
    }}
    scroll {{
        bind "e" {{ EditScrollback; SwitchToMode "Normal"; }}
        bind "s" {{ SwitchToMode "EnterSearch"; SearchInput 0; }}
        bind "Ctrl c" {{ ScrollToBottom; SwitchToMode "Normal"; }}
        bind "j" "Down" {{ ScrollDown; }}
        bind "k" "Up" {{ ScrollUp; }}
        bind "Ctrl f" "PageDown" "Right" "l" {{ PageScrollDown; }}
        bind "Ctrl b" "PageUp" "Left" "h" {{ PageScrollUp; }}
        bind "d" {{ HalfPageScrollDown; }}
        bind "u" {{ HalfPageScrollUp; }}
    }}
    search {{
        bind "Ctrl c" {{ ScrollToBottom; SwitchToMode "Normal"; }}
        bind "j" "Down" {{ ScrollDown; }}
        bind "k" "Up" {{ ScrollUp; }}
        bind "Ctrl f" "PageDown" "Right" "l" {{ PageScrollDown; }}
        bind "Ctrl b" "PageUp" "Left" "h" {{ PageScrollUp; }}
        bind "d" {{ HalfPageScrollDown; }}
        bind "u" {{ HalfPageScrollUp; }}
        bind "n" {{ Search "down"; }}
        bind "p" {{ Search "up"; }}
        bind "c" {{ SearchToggleOption "CaseSensitivity"; }}
        bind "w" {{ SearchToggleOption "Wrap"; }}
        bind "o" {{ SearchToggleOption "WholeWord"; }}
    }}
    entersearch {{
        bind "Ctrl c" "Esc" {{ SwitchToMode "Scroll"; }}
        bind "Enter" {{ SwitchToMode "Search"; }}
    }}
    renametab {{
        bind "Ctrl c" {{ SwitchToMode "Normal"; }}
        bind "Esc" {{ UndoRenameTab; SwitchToMode "Tab"; }}
    }}
    renamepane {{
        bind "Ctrl c" {{ SwitchToMode "Normal"; }}
        bind "Esc" {{ UndoRenamePane; SwitchToMode "Pane"; }}
    }}
    session {{
        bind "d" {{ Detach; }}
        bind "w" {{
            LaunchOrFocusPlugin "session-manager" {{
                floating true
                move_to_focused_tab true
            }};
            SwitchToMode "Normal"
        }}
        bind "c" {{
            LaunchOrFocusPlugin "configuration" {{
                floating true
                move_to_focused_tab true
            }};
            SwitchToMode "Normal"
        }}
        bind "p" {{
            LaunchOrFocusPlugin "plugin-manager" {{
                floating true
                move_to_focused_tab true
            }};
            SwitchToMode "Normal"
        }}
    }}
    tmux {{
        bind "[" {{ SwitchToMode "Scroll"; }}
        bind "\"" {{ NewPane "Down"; SwitchToMode "Normal"; }}
        bind "%" {{ NewPane "Right"; SwitchToMode "Normal"; }}
        bind "z" {{ ToggleFocusFullscreen; SwitchToMode "Normal"; }}
        bind "c" {{ NewTab; SwitchToMode "Normal"; }}
        bind "," {{ SwitchToMode "RenameTab"; }}
        bind "p" {{ GoToPreviousTab; SwitchToMode "Normal"; }}
        bind "n" {{ GoToNextTab; SwitchToMode "Normal"; }}
        bind "Left" {{ MoveFocus "Left"; SwitchToMode "Normal"; }}
        bind "Right" {{ MoveFocus "Right"; SwitchToMode "Normal"; }}
        bind "Down" {{ MoveFocus "Down"; SwitchToMode "Normal"; }}
        bind "Up" {{ MoveFocus "Up"; SwitchToMode "Normal"; }}
        bind "h" {{ MoveFocus "Left"; SwitchToMode "Normal"; }}
        bind "l" {{ MoveFocus "Right"; SwitchToMode "Normal"; }}
        bind "j" {{ MoveFocus "Down"; SwitchToMode "Normal"; }}
        bind "k" {{ MoveFocus "Up"; SwitchToMode "Normal"; }}
        bind "o" {{ FocusNextPane; }}
        bind "d" {{ Detach; }}
        bind "Space" {{ NextSwapLayout; }}
        bind "x" {{ CloseFocus; SwitchToMode "Normal"; }}
    }}
    shared_except "normal" "locked" {{
        bind "Enter" "Esc" {{ SwitchToMode "Normal"; }}
    }}
}}
"#
    )
}

fn non_colliding_default_keybinds(primary_modifier: String, secondary_modifier: String) -> String {
    format!(
        r#"
default_mode "normal"
keybinds clear-defaults=true {{
    normal {{}}
    locked {{
        bind "{primary_modifier} g" {{ SwitchToMode "Normal"; }}
    }}
    resize {{
        bind "{primary_modifier} r" {{ SwitchToMode "Normal"; }}
        bind "h" "Left" {{ Resize "Increase Left"; }}
        bind "j" "Down" {{ Resize "Increase Down"; }}
        bind "k" "Up" {{ Resize "Increase Up"; }}
        bind "l" "Right" {{ Resize "Increase Right"; }}
        bind "H" {{ Resize "Decrease Left"; }}
        bind "J" {{ Resize "Decrease Down"; }}
        bind "K" {{ Resize "Decrease Up"; }}
        bind "L" {{ Resize "Decrease Right"; }}
        bind "=" "+" {{ Resize "Increase"; }}
        bind "-" {{ Resize "Decrease"; }}
    }}
    pane {{
        bind "{primary_modifier} p" {{ SwitchToMode "Normal"; }}
        bind "h" "Left" {{ MoveFocus "Left"; }}
        bind "l" "Right" {{ MoveFocus "Right"; }}
        bind "j" "Down" {{ MoveFocus "Down"; }}
        bind "k" "Up" {{ MoveFocus "Up"; }}
        bind "p" {{ SwitchFocus; }}
        bind "n" {{ NewPane; SwitchToMode "Normal"; }}
        bind "d" {{ NewPane "Down"; SwitchToMode "Normal"; }}
        bind "r" {{ NewPane "Right"; SwitchToMode "Normal"; }}
        bind "x" {{ CloseFocus; SwitchToMode "Normal"; }}
        bind "f" {{ ToggleFocusFullscreen; SwitchToMode "Normal"; }}
        bind "z" {{ TogglePaneFrames; SwitchToMode "Normal"; }}
        bind "w" {{ ToggleFloatingPanes; SwitchToMode "Normal"; }}
        bind "e" {{ TogglePaneEmbedOrFloating; SwitchToMode "Normal"; }}
        bind "c" {{ SwitchToMode "RenamePane"; PaneNameInput 0;}}
    }}
    move {{
        bind "{primary_modifier} m" {{ SwitchToMode "Normal"; }}
        bind "n" "Tab" {{ MovePane; }}
        bind "p" {{ MovePaneBackwards; }}
        bind "h" "Left" {{ MovePane "Left"; }}
        bind "j" "Down" {{ MovePane "Down"; }}
        bind "k" "Up" {{ MovePane "Up"; }}
        bind "l" "Right" {{ MovePane "Right"; }}
    }}
    tab {{
        bind "{primary_modifier} t" {{ SwitchToMode "Normal"; }}
        bind "r" {{ SwitchToMode "RenameTab"; TabNameInput 0; }}
        bind "h" "Left" "Up" "k" {{ GoToPreviousTab; }}
        bind "l" "Right" "Down" "j" {{ GoToNextTab; }}
        bind "n" {{ NewTab; SwitchToMode "Normal"; }}
        bind "x" {{ CloseTab; SwitchToMode "Normal"; }}
        bind "s" {{ ToggleActiveSyncTab; SwitchToMode "Normal"; }}
        bind "b" {{ BreakPane; SwitchToMode "Normal"; }}
        bind "]" {{ BreakPaneRight; SwitchToMode "Normal"; }}
        bind "[" {{ BreakPaneLeft; SwitchToMode "Normal"; }}
        bind "1" {{ GoToTab 1; SwitchToMode "Normal"; }}
        bind "2" {{ GoToTab 2; SwitchToMode "Normal"; }}
        bind "3" {{ GoToTab 3; SwitchToMode "Normal"; }}
        bind "4" {{ GoToTab 4; SwitchToMode "Normal"; }}
        bind "5" {{ GoToTab 5; SwitchToMode "Normal"; }}
        bind "6" {{ GoToTab 6; SwitchToMode "Normal"; }}
        bind "7" {{ GoToTab 7; SwitchToMode "Normal"; }}
        bind "8" {{ GoToTab 8; SwitchToMode "Normal"; }}
        bind "9" {{ GoToTab 9; SwitchToMode "Normal"; }}
        bind "Tab" {{ ToggleTab; }}
    }}
    scroll {{
        bind "{primary_modifier} s" {{ SwitchToMode "Normal"; }}
        bind "e" {{ EditScrollback; SwitchToMode "Normal"; }}
        bind "s" {{ SwitchToMode "EnterSearch"; SearchInput 0; }}
        bind "Ctrl c" {{ ScrollToBottom; SwitchToMode "Normal"; }}
        bind "j" "Down" {{ ScrollDown; }}
        bind "k" "Up" {{ ScrollUp; }}
        bind "Ctrl f" "PageDown" "Right" "l" {{ PageScrollDown; }}
        bind "Ctrl b" "PageUp" "Left" "h" {{ PageScrollUp; }}
        bind "d" {{ HalfPageScrollDown; }}
        bind "u" {{ HalfPageScrollUp; }}
    }}
    search {{
        bind "{primary_modifier} s" {{ SwitchToMode "Normal"; }}
        bind "Ctrl c" {{ ScrollToBottom; SwitchToMode "Normal"; }}
        bind "j" "Down" {{ ScrollDown; }}
        bind "k" "Up" {{ ScrollUp; }}
        bind "Ctrl f" "PageDown" "Right" "l" {{ PageScrollDown; }}
        bind "Ctrl b" "PageUp" "Left" "h" {{ PageScrollUp; }}
        bind "d" {{ HalfPageScrollDown; }}
        bind "u" {{ HalfPageScrollUp; }}
        bind "n" {{ Search "down"; }}
        bind "p" {{ Search "up"; }}
        bind "c" {{ SearchToggleOption "CaseSensitivity"; }}
        bind "w" {{ SearchToggleOption "Wrap"; }}
        bind "o" {{ SearchToggleOption "WholeWord"; }}
    }}
    entersearch {{
        bind "Ctrl c" "Esc" {{ SwitchToMode "Scroll"; }}
        bind "Enter" {{ SwitchToMode "Search"; }}
    }}
    renametab {{
        bind "Ctrl c" {{ SwitchToMode "Normal"; }}
        bind "Esc" {{ UndoRenameTab; SwitchToMode "Tab"; }}
    }}
    renamepane {{
        bind "Ctrl c" {{ SwitchToMode "Normal"; }}
        bind "Esc" {{ UndoRenamePane; SwitchToMode "Pane"; }}
    }}
    session {{
        bind "{primary_modifier} o" {{ SwitchToMode "Normal"; }}
        bind "{primary_modifier} s" {{ SwitchToMode "Scroll"; }}
        bind "d" {{ Detach; }}
        bind "w" {{
            LaunchOrFocusPlugin "session-manager" {{
                floating true
                move_to_focused_tab true
            }};
            SwitchToMode "Normal"
        }}
        bind "c" {{
            LaunchOrFocusPlugin "configuration" {{
                floating true
                move_to_focused_tab true
            }};
            SwitchToMode "Normal"
        }}
        bind "p" {{
            LaunchOrFocusPlugin "plugin-manager" {{
                floating true
                move_to_focused_tab true
            }};
            SwitchToMode "Normal"
        }}
    }}
    tmux {{
        bind "[" {{ SwitchToMode "Scroll"; }}
        bind "{primary_modifier} b" {{ Write 2; SwitchToMode "Normal"; }}
        bind "\"" {{ NewPane "Down"; SwitchToMode "Normal"; }}
        bind "%" {{ NewPane "Right"; SwitchToMode "Normal"; }}
        bind "z" {{ ToggleFocusFullscreen; SwitchToMode "Normal"; }}
        bind "c" {{ NewTab; SwitchToMode "Normal"; }}
        bind "," {{ SwitchToMode "RenameTab"; }}
        bind "p" {{ GoToPreviousTab; SwitchToMode "Normal"; }}
        bind "n" {{ GoToNextTab; SwitchToMode "Normal"; }}
        bind "Left" {{ MoveFocus "Left"; SwitchToMode "Normal"; }}
        bind "Right" {{ MoveFocus "Right"; SwitchToMode "Normal"; }}
        bind "Down" {{ MoveFocus "Down"; SwitchToMode "Normal"; }}
        bind "Up" {{ MoveFocus "Up"; SwitchToMode "Normal"; }}
        bind "h" {{ MoveFocus "Left"; SwitchToMode "Normal"; }}
        bind "l" {{ MoveFocus "Right"; SwitchToMode "Normal"; }}
        bind "j" {{ MoveFocus "Down"; SwitchToMode "Normal"; }}
        bind "k" {{ MoveFocus "Up"; SwitchToMode "Normal"; }}
        bind "o" {{ FocusNextPane; }}
        bind "d" {{ Detach; }}
        bind "Space" {{ NextSwapLayout; }}
        bind "x" {{ CloseFocus; SwitchToMode "Normal"; }}
    }}
    shared_except "locked" {{
        bind "{primary_modifier} g" {{ SwitchToMode "Locked"; }}
        bind "{primary_modifier} q" {{ Quit; }}
        bind "{secondary_modifier} f" {{ ToggleFloatingPanes; }}
        bind "{secondary_modifier} n" {{ NewPane; }}
        bind "{secondary_modifier} i" {{ MoveTab "Left"; }}
        bind "{secondary_modifier} o" {{ MoveTab "Right"; }}
        bind "{secondary_modifier} h" "{secondary_modifier} Left" {{ MoveFocusOrTab "Left"; }}
        bind "{secondary_modifier} l" "{secondary_modifier} Right" {{ MoveFocusOrTab "Right"; }}
        bind "{secondary_modifier} j" "{secondary_modifier} Down" {{ MoveFocus "Down"; }}
        bind "{secondary_modifier} k" "{secondary_modifier} Up" {{ MoveFocus "Up"; }}
        bind "{secondary_modifier} =" "{secondary_modifier} +" {{ Resize "Increase"; }}
        bind "{secondary_modifier} -" {{ Resize "Decrease"; }}
        bind "{secondary_modifier} [" {{ PreviousSwapLayout; }}
        bind "{secondary_modifier} ]" {{ NextSwapLayout; }}
    }}
    shared_except "normal" "locked" {{
        bind "Enter" "Esc" {{ SwitchToMode "Normal"; }}
    }}
    shared_except "pane" "locked" {{
        bind "{primary_modifier} p" {{ SwitchToMode "Pane"; }}
    }}
    shared_except "resize" "locked" {{
        bind "{primary_modifier} r" {{ SwitchToMode "Resize"; }}
    }}
    shared_except "scroll" "locked" {{
        bind "{primary_modifier} s" {{ SwitchToMode "Scroll"; }}
    }}
    shared_except "session" "locked" "tab" {{
        bind "{primary_modifier} o" {{ SwitchToMode "Session"; }}
    }}
    shared_except "tab" "locked" {{
        bind "{primary_modifier} t" {{ SwitchToMode "Tab"; }}
    }}
    shared_except "move" "locked" {{
        bind "{primary_modifier} m" {{ SwitchToMode "Move"; }}
    }}
    shared_except "tmux" "locked" {{
        bind "{primary_modifier} b" {{ SwitchToMode "Tmux"; }}
    }}
}}
"#
    )
}
