use std::collections::BTreeSet;
use zellij_tile::prelude::*;

use crate::ui_components::{back_to_presets, info_line};

use crate::{POSSIBLE_MODIFIERS, WIDTH_BREAKPOINTS};

#[derive(Debug)]
pub struct RebindLeadersScreen {
    selected_primary_key_index: usize,
    selected_secondary_key_index: usize,
    main_leader_selected: bool,
    rebinding_main_leader: bool,
    browsing_primary_modifier: bool,
    browsing_secondary_modifier: bool,
    main_leader: Option<KeyWithModifier>,
    primary_modifier: BTreeSet<KeyModifier>,
    secondary_modifier: BTreeSet<KeyModifier>,
    latest_mode_info: Option<ModeInfo>,
    notification: Option<String>,
    is_rebinding_for_presets: bool,
    ui_is_dirty: bool,
}

impl Default for RebindLeadersScreen {
    fn default() -> Self {
        let mut primary_modifier = BTreeSet::new();
        primary_modifier.insert(KeyModifier::Ctrl);
        let mut secondary_modifier = BTreeSet::new();
        secondary_modifier.insert(KeyModifier::Alt);
        RebindLeadersScreen {
            selected_primary_key_index: 0,
            selected_secondary_key_index: 0,
            main_leader_selected: false,
            rebinding_main_leader: false,
            browsing_primary_modifier: false,
            browsing_secondary_modifier: false,
            main_leader: None,
            primary_modifier,
            secondary_modifier,
            latest_mode_info: None,
            notification: None,
            is_rebinding_for_presets: false,
            ui_is_dirty: false,
        }
    }
}

impl RebindLeadersScreen {
    // temporarily commented out for the time being because the extra leaders screen was deemed a bit
    // confusing, see commend in <l> key
    //     pub fn with_rebinding_for_presets(mut self) -> Self {
    //         self.is_rebinding_for_presets = true;
    //         self
    //     }
    pub fn with_mode_info(mut self, latest_mode_info: Option<ModeInfo>) -> Self {
        self.latest_mode_info = latest_mode_info;
        self.hard_reset_ui_state();
        self
    }
    pub fn update_mode_info(&mut self, mode_info: ModeInfo) {
        self.latest_mode_info = Some(mode_info);
        if !self.ui_is_dirty {
            self.set_main_leader_from_keybindings();
            self.set_primary_and_secondary_modifiers_from_keybindings();
        }
    }
    pub fn primary_and_secondary_modifiers(
        &self,
    ) -> (BTreeSet<KeyModifier>, BTreeSet<KeyModifier>) {
        (
            self.primary_modifier.clone(),
            self.secondary_modifier.clone(),
        )
    }
    fn set_main_leader_from_keybindings(&mut self) {
        self.main_leader = self.latest_mode_info.as_ref().and_then(|mode_info| {
            mode_info
                .keybinds
                .iter()
                .find_map(|m| {
                    if m.0 == InputMode::Locked {
                        Some(m.1.clone())
                    } else {
                        None
                    }
                })
                .and_then(|k| {
                    k.into_iter().find_map(|(k, a)| {
                        if a == &[actions::Action::SwitchToMode {
                            input_mode: InputMode::Normal,
                        }] {
                            Some(k)
                        } else {
                            None
                        }
                    })
                })
        });
    }
    fn set_primary_and_secondary_modifiers_from_keybindings(&mut self) {
        let mut primary_modifier = self.latest_mode_info.as_ref().and_then(|mode_info| {
            mode_info
                .keybinds
                .iter()
                .find_map(|m| {
                    if m.0 == InputMode::Locked {
                        Some(m.1.clone())
                    } else {
                        None
                    }
                })
                .and_then(|k| {
                    k.into_iter().find_map(|(k, a)| {
                        if a == &[actions::Action::SwitchToMode {
                            input_mode: InputMode::Normal,
                        }] {
                            Some(k.key_modifiers.clone())
                        } else {
                            None
                        }
                    })
                })
        });
        let mut secondary_modifier = self.latest_mode_info.as_ref().and_then(|mode_info| {
            let base_mode = mode_info.base_mode.unwrap_or(InputMode::Normal);
            mode_info
                .keybinds
                .iter()
                .find_map(|m| {
                    if m.0 == base_mode {
                        Some(m.1.clone())
                    } else {
                        None
                    }
                })
                .and_then(|k| {
                    k.into_iter().find_map(|(k, a)| {
                        if a == &[actions::Action::NewPane {
                            direction: None,
                            pane_name: None,
                            start_suppressed: false,
                        }] {
                            Some(k.key_modifiers.clone())
                        } else {
                            None
                        }
                    })
                })
        });
        if let Some(primary_modifier) = primary_modifier.take() {
            self.primary_modifier = primary_modifier;
        }
        if let Some(secondary_modifier) = secondary_modifier.take() {
            self.secondary_modifier = secondary_modifier;
        }
    }
    pub fn set_unlock_toggle_selected(&mut self) {
        self.main_leader_selected = true;
        self.selected_primary_key_index = 0;
        self.selected_secondary_key_index = 0;
        self.rebinding_main_leader = false;
        self.browsing_primary_modifier = false;
        self.browsing_secondary_modifier = false;
    }
    pub fn set_primary_modifier_selected(&mut self) {
        self.browsing_primary_modifier = true;
        self.main_leader_selected = false;
        self.selected_primary_key_index = 0;
        self.selected_secondary_key_index = 0;
        self.rebinding_main_leader = false;
        self.browsing_secondary_modifier = false;
    }
    pub fn set_secondary_modifier_selected(&mut self) {
        self.browsing_secondary_modifier = true;
        self.browsing_primary_modifier = false;
        self.main_leader_selected = false;
        self.selected_primary_key_index = 0;
        self.selected_secondary_key_index = 0;
        self.rebinding_main_leader = false;
    }
    pub fn set_rebinding_unlock_toggle(&mut self) {
        self.rebinding_main_leader = true;
        self.browsing_secondary_modifier = false;
        self.browsing_primary_modifier = false;
        self.main_leader_selected = false;
        self.selected_primary_key_index = 0;
        self.selected_secondary_key_index = 0;
    }
    pub fn move_secondary_index_down(&mut self) {
        if self.selected_secondary_key_index < POSSIBLE_MODIFIERS.len().saturating_sub(1) {
            self.selected_secondary_key_index += 1;
        } else {
            self.set_unlock_toggle_selected();
        }
    }
    pub fn move_secondary_index_up(&mut self) {
        if self.selected_secondary_key_index > 0 {
            self.selected_secondary_key_index -= 1;
        } else {
            self.set_unlock_toggle_selected();
        }
    }
    pub fn move_selection_for_default_preset(&mut self, key: &KeyWithModifier) {
        if self.browsing_primary_modifier {
            if key.bare_key == BareKey::Left && key.has_no_modifiers() {
                self.browsing_primary_modifier = false;
                self.browsing_secondary_modifier = true;
                self.selected_secondary_key_index = self.selected_primary_key_index;
            } else if key.bare_key == BareKey::Right && key.has_no_modifiers() {
                self.browsing_primary_modifier = false;
                self.browsing_secondary_modifier = true;
                self.selected_secondary_key_index = self.selected_primary_key_index;
            } else if key.bare_key == BareKey::Down && key.has_no_modifiers() {
                if self.selected_primary_key_index < POSSIBLE_MODIFIERS.len().saturating_sub(1) {
                    self.selected_primary_key_index += 1;
                }
            } else if key.bare_key == BareKey::Up && key.has_no_modifiers() {
                if self.selected_primary_key_index > 0 {
                    self.selected_primary_key_index -= 1;
                }
            }
        } else if self.browsing_secondary_modifier {
            if key.bare_key == BareKey::Left && key.has_no_modifiers() {
                self.browsing_secondary_modifier = false;
                self.browsing_primary_modifier = true;
                self.selected_primary_key_index = self.selected_secondary_key_index;
            } else if key.bare_key == BareKey::Right && key.has_no_modifiers() {
                self.browsing_secondary_modifier = false;
                self.browsing_primary_modifier = true;
                self.selected_primary_key_index = self.selected_secondary_key_index;
            } else if key.bare_key == BareKey::Down && key.has_no_modifiers() {
                if self.selected_secondary_key_index < POSSIBLE_MODIFIERS.len().saturating_sub(1) {
                    self.selected_secondary_key_index += 1;
                }
            } else if key.bare_key == BareKey::Up && key.has_no_modifiers() {
                if self.selected_secondary_key_index > 0 {
                    self.selected_secondary_key_index -= 1;
                }
            }
        } else {
            self.set_primary_modifier_selected();
        }
    }
    pub fn move_selection_for_unlock_first(&mut self, key: &KeyWithModifier) {
        if self.browsing_secondary_modifier {
            if (key.bare_key == BareKey::Left || key.bare_key == BareKey::Right)
                && key.has_no_modifiers()
            {
                self.set_unlock_toggle_selected();
            } else if key.bare_key == BareKey::Down && key.has_no_modifiers() {
                self.move_secondary_index_down();
            } else if key.bare_key == BareKey::Up && key.has_no_modifiers() {
                self.move_secondary_index_up();
            }
        } else if self.main_leader_selected {
            if (key.bare_key == BareKey::Down
                || key.bare_key == BareKey::Up
                || key.bare_key == BareKey::Right
                || key.bare_key == BareKey::Left)
                && key.has_no_modifiers()
            {
                self.set_secondary_modifier_selected();
            }
        } else {
            self.set_unlock_toggle_selected();
        }
    }
    pub fn render(
        &mut self,
        rows: usize,
        cols: usize,
        ui_size: usize,
        notification: &Option<String>,
    ) {
        if self.is_rebinding_for_presets {
            back_to_presets();
        }
        let notification = notification.clone().or_else(|| self.notification.clone());
        if self.currently_in_unlock_first() {
            self.render_unlock_first(rows, cols);
        } else {
            self.render_default_preset(rows, cols);
        }
        let warning_text = self.warning_text(cols);
        info_line(rows, cols, ui_size, &notification, &warning_text, None);
    }
    fn render_unlock_first(&mut self, rows: usize, cols: usize) {
        self.render_screen_title_unlock_first(rows, cols);
        self.render_unlock_toggle(rows, cols);
        self.render_secondary_modifier_selector(rows, cols);
        self.render_help_text(rows, cols);
    }
    fn render_default_preset(&mut self, rows: usize, cols: usize) {
        self.render_screen_title_default_preset(rows, cols);
        self.render_primary_modifier_selector(rows, cols);
        self.render_secondary_modifier_selector(rows, cols);
        self.render_help_text(rows, cols);
    }
    fn render_primary_modifier_selector(&self, rows: usize, cols: usize) {
        let screen_width = if cols >= WIDTH_BREAKPOINTS.0 {
            WIDTH_BREAKPOINTS.0
        } else {
            WIDTH_BREAKPOINTS.1
        };
        let base_x = cols.saturating_sub(screen_width) / 2;
        let base_y = rows.saturating_sub(10) / 2;
        let primary_modifier_key_text = self.primary_modifier_text();
        let (primary_modifier_text, primary_modifier_start_position) =
            if cols >= WIDTH_BREAKPOINTS.0 {
                (format!("Primary: {}", primary_modifier_key_text), 9)
            } else {
                (format!("{}", primary_modifier_key_text), 0)
            };
        let primary_modifier_menu_width = primary_modifier_text.chars().count();
        print_text_with_coordinates(
            Text::new(primary_modifier_text).color_range(3, primary_modifier_start_position..),
            base_x,
            base_y + 5,
            None,
            None,
        );
        print_nested_list_with_coordinates(
            POSSIBLE_MODIFIERS
                .iter()
                .enumerate()
                .map(|(i, m)| {
                    let item = if self.primary_modifier.contains(m) {
                        NestedListItem::new(m.to_string()).color_range(3, ..)
                    } else {
                        NestedListItem::new(m.to_string())
                    };
                    if self.browsing_primary_modifier && self.selected_primary_key_index == i {
                        item.selected()
                    } else {
                        item
                    }
                })
                .collect(),
            base_x,
            base_y + 6,
            Some(primary_modifier_menu_width),
            None,
        );
    }
    pub fn render_screen_title_unlock_first(&self, rows: usize, cols: usize) {
        let screen_width = if cols >= WIDTH_BREAKPOINTS.0 {
            WIDTH_BREAKPOINTS.0
        } else {
            WIDTH_BREAKPOINTS.1
        };
        let leader_keys_text = if cols >= WIDTH_BREAKPOINTS.0 {
            "Rebind leader keys (Non-Colliding preset)"
        } else if cols >= WIDTH_BREAKPOINTS.1 {
            "Rebind leader keys (Non-Colliding)"
        } else {
            "Rebind leader keys"
        };
        let base_x = cols.saturating_sub(screen_width) / 2;
        let base_y = rows.saturating_sub(10) / 2;
        let explanation_text_1 = if cols >= WIDTH_BREAKPOINTS.0 {
            "Unlock toggle - used to expose the other modes (eg. PANE, TAB)"
        } else if cols >= WIDTH_BREAKPOINTS.1 {
            "Unlock toggle - expose other modes"
        } else {
            ""
        };
        let explanation_text_2 = if cols >= WIDTH_BREAKPOINTS.0 {
            "Secondary modifier - prefixes common actions (eg. New Pane)"
        } else if cols >= WIDTH_BREAKPOINTS.1 {
            "Secondary modifier - common actions"
        } else {
            ""
        };
        print_text_with_coordinates(
            Text::new(leader_keys_text).color_range(2, ..),
            base_x,
            base_y,
            None,
            None,
        );
        print_text_with_coordinates(
            Text::new(explanation_text_1).color_range(1, ..=12),
            base_x,
            base_y + 2,
            None,
            None,
        );
        print_text_with_coordinates(
            Text::new(explanation_text_2).color_range(1, ..=17),
            base_x,
            base_y + 3,
            None,
            None,
        );
    }
    fn render_screen_title_default_preset(&self, rows: usize, cols: usize) {
        let screen_width = if cols >= WIDTH_BREAKPOINTS.0 {
            WIDTH_BREAKPOINTS.0
        } else {
            WIDTH_BREAKPOINTS.1
        };
        let leader_keys_text = if cols >= WIDTH_BREAKPOINTS.0 {
            "Rebind leader keys (Default preset)"
        } else {
            "Rebind leader keys"
        };
        let base_x = cols.saturating_sub(screen_width) / 2;
        let base_y = rows.saturating_sub(10) / 2;
        let explanation_text_1 = if cols >= WIDTH_BREAKPOINTS.0 {
            "Primary - the modifier used to switch modes (eg. PANE, TAB)"
        } else if cols >= WIDTH_BREAKPOINTS.1 {
            "Primary - used to switch modes"
        } else {
            ""
        };
        let explanation_text_2 = if cols >= WIDTH_BREAKPOINTS.0 {
            "Secondary - the modifier used for common actions (eg. New Pane)"
        } else if cols >= WIDTH_BREAKPOINTS.1 {
            "Secondary - common actions"
        } else {
            ""
        };
        print_text_with_coordinates(
            Text::new(leader_keys_text).color_range(2, ..),
            base_x,
            base_y,
            None,
            None,
        );
        print_text_with_coordinates(
            Text::new(explanation_text_1).color_range(1, ..=6),
            base_x,
            base_y + 2,
            None,
            None,
        );
        print_text_with_coordinates(
            Text::new(explanation_text_2).color_range(1, ..=8),
            base_x,
            base_y + 3,
            None,
            None,
        );
    }
    fn render_unlock_toggle(&self, rows: usize, cols: usize) {
        let screen_width = if cols >= WIDTH_BREAKPOINTS.0 {
            WIDTH_BREAKPOINTS.0
        } else {
            WIDTH_BREAKPOINTS.1
        };
        let base_x = cols.saturating_sub(screen_width) / 2;
        let base_y = rows.saturating_sub(10) / 2;
        if let Some(main_leader_key_text) = self.main_leader_text() {
            let main_leader_key_text = if self.rebinding_main_leader {
                "...".to_owned()
            } else {
                main_leader_key_text
            };
            let (primary_modifier_text, primary_modifier_start_position) =
                if cols >= WIDTH_BREAKPOINTS.0 {
                    (format!("Unlock Toggle: {}", main_leader_key_text), 15)
                } else {
                    (format!("{}", main_leader_key_text), 0)
                };
            let mut primary_modifier =
                Text::new(primary_modifier_text).color_range(3, primary_modifier_start_position..);
            if self.main_leader_selected {
                primary_modifier = primary_modifier.selected();
            }
            print_text_with_coordinates(primary_modifier, base_x, base_y + 5, None, None);
            if self.rebinding_main_leader {
                let first_bulletin = "[Enter new key] eg.";
                let second_bulletin = "\"Ctrl g\", \"Alt g\",";
                let third_bulletin = "\"Alt ESC\", \"Ctrl SPACE\"";
                print_nested_list_with_coordinates(
                    vec![
                        NestedListItem::new(first_bulletin).color_range(3, ..=14),
                        NestedListItem::new(second_bulletin),
                        NestedListItem::new(third_bulletin),
                    ],
                    base_x,
                    base_y + 6,
                    None,
                    None,
                );
            }
        }
    }
    fn main_leader_text(&self) -> Option<String> {
        self.main_leader.as_ref().map(|m| format!("{}", m))
    }
    fn render_secondary_modifier_selector(&mut self, rows: usize, cols: usize) {
        let screen_width = if cols >= WIDTH_BREAKPOINTS.0 {
            WIDTH_BREAKPOINTS.0
        } else {
            WIDTH_BREAKPOINTS.1
        };
        let base_x = cols.saturating_sub(screen_width) / 2;
        let base_y = rows.saturating_sub(10) / 2;
        let secondary_modifier_key_text = self.secondary_modifier_text();
        let (secondary_modifier_text, secondary_modifier_start_position) =
            if cols >= WIDTH_BREAKPOINTS.0 {
                if self.currently_in_unlock_first() {
                    (
                        format!("Secondary Modifier: {}", secondary_modifier_key_text),
                        20,
                    )
                } else {
                    (format!("Secondary: {}", secondary_modifier_key_text), 11)
                }
            } else {
                (format!("{}", secondary_modifier_key_text), 0)
            };
        let secondary_modifier_menu_x_coords = base_x + (screen_width / 2);
        let secondary_modifier_menu_width = secondary_modifier_text.chars().count();
        print_text_with_coordinates(
            Text::new(secondary_modifier_text).color_range(0, secondary_modifier_start_position..),
            secondary_modifier_menu_x_coords,
            base_y + 5,
            None,
            None,
        );
        print_nested_list_with_coordinates(
            POSSIBLE_MODIFIERS
                .iter()
                .enumerate()
                .map(|(i, m)| {
                    let item = if self.secondary_modifier.contains(m) {
                        NestedListItem::new(m.to_string()).color_range(0, ..)
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
            base_y + 6,
            Some(secondary_modifier_menu_width),
            None,
        );
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
    fn render_help_text(&self, rows: usize, cols: usize) {
        if self.is_rebinding_for_presets {
            return self.render_help_text_for_presets_rebinding(rows, cols);
        }
        let help_text_long = "Help: <←↓↑→> - navigate, <SPACE> - select, <ENTER> - apply, <Ctrl a> - save, <Ctrl c> - reset, <ESC> - close";
        let help_text_medium = "Help: <←↓↑→/SPACE> - navigate/select, <ENTER/Ctrl a> - apply/save, <Ctrl c> - reset, <ESC> - close";
        let help_text_short =
            "Help: <←↓↑→>/<SPACE>/<ENTER> select/<Ctrl a> save/<Ctrl c> reset/<ESC>";
        let help_text_minimum = "<←↓↑→>/<SPACE>/<ENTER>/<Ctrl a>/<Ctrl c>/<ESC>";
        if cols >= help_text_long.chars().count() {
            print_text_with_coordinates(
                Text::new(help_text_long)
                    .color_range(2, 6..=12)
                    .color_range(2, 25..=31)
                    .color_range(2, 43..=49)
                    .color_range(2, 60..=67)
                    .color_range(2, 77..=84)
                    .color_range(2, 95..=99),
                0,
                rows,
                None,
                None,
            );
        } else if cols >= help_text_medium.chars().count() {
            print_text_with_coordinates(
                Text::new(help_text_medium)
                    .color_range(2, 6..=17)
                    .color_range(2, 38..=51)
                    .color_range(2, 67..=75)
                    .color_range(2, 85..=89),
                0,
                rows,
                None,
                None,
            );
        } else if cols >= help_text_short.chars().count() {
            print_text_with_coordinates(
                Text::new(help_text_short)
                    .color_range(2, 6..=11)
                    .color_range(2, 13..=19)
                    .color_range(2, 21..=27)
                    .color_range(2, 36..=43)
                    .color_range(2, 50..=57)
                    .color_range(2, 65..=69),
                0,
                rows,
                None,
                None,
            );
        } else {
            print_text_with_coordinates(
                Text::new(help_text_minimum)
                    .color_range(2, ..=5)
                    .color_range(2, 7..=13)
                    .color_range(2, 15..=21)
                    .color_range(2, 23..=30)
                    .color_range(2, 32..=39)
                    .color_range(2, 41..=45),
                0,
                rows,
                None,
                None,
            );
        }
    }
    fn render_help_text_for_presets_rebinding(&self, rows: usize, cols: usize) {
        let help_text_long = "Help: <←↓↑→> - navigate, <SPACE> - select, <ENTER> - apply to presets in previous screen";
        let help_text_medium = "Help: <←↓↑→> - navigate, <SPACE> - select, <ENTER> - apply";
        let help_text_short = "<←↓↑→/SPACE> - navigate/select, <ENTER> - apply";
        let help_text_minimum = "<←↓↑→>/<SPACE>/<ENTER>";
        if cols >= help_text_long.chars().count() {
            print_text_with_coordinates(
                Text::new(help_text_long)
                    .color_range(2, 6..=12)
                    .color_range(2, 25..=31)
                    .color_range(2, 43..=49),
                0,
                rows,
                None,
                None,
            );
        } else if cols >= help_text_medium.chars().count() {
            print_text_with_coordinates(
                Text::new(help_text_medium)
                    .color_range(2, 6..=12)
                    .color_range(2, 25..=31)
                    .color_range(2, 43..=49),
                0,
                rows,
                None,
                None,
            );
        } else if cols >= help_text_short.chars().count() {
            print_text_with_coordinates(
                Text::new(help_text_short)
                    .color_range(2, 1..=4)
                    .color_range(2, 6..=10)
                    .color_range(2, 32..=38),
                0,
                rows,
                None,
                None,
            );
        } else {
            print_text_with_coordinates(
                Text::new(help_text_minimum)
                    .color_range(2, ..=5)
                    .color_range(2, 7..=13)
                    .color_range(2, 15..=21),
                0,
                rows,
                None,
                None,
            );
        }
    }
    pub fn handle_key(&mut self, key: KeyWithModifier) -> bool {
        if let Some(notification) = self.notification.take() {
            drop(notification);
            true
        } else if self.currently_in_unlock_first() {
            self.handle_unlock_first_key(key)
        } else {
            self.handle_default_preset_key(key)
        }
    }
    pub fn drain_notification(&mut self) -> Option<String> {
        self.notification.take()
    }
    pub fn set_notification(&mut self, notification: Option<String>) {
        self.notification = notification;
    }
    fn currently_in_unlock_first(&self) -> bool {
        if self.is_rebinding_for_presets {
            false
        } else {
            self.latest_mode_info
                .as_ref()
                .map(|m| m.base_mode == Some(InputMode::Locked))
                .unwrap_or(false)
        }
    }
    fn handle_default_preset_key(&mut self, key: KeyWithModifier) -> bool {
        let should_render = true;
        if key.bare_key == BareKey::Char('a')
            && key.has_modifiers(&[KeyModifier::Ctrl])
            && !self.is_rebinding_for_presets
        {
            let write_to_disk = true;
            self.rebind_keys(write_to_disk);
            self.hard_reset_ui_state();
        } else if key.bare_key == BareKey::Enter && key.has_no_modifiers() {
            let write_to_disk = false;
            self.rebind_keys(write_to_disk);
            self.hard_reset_ui_state();
        } else if key.is_key_with_ctrl_modifier(BareKey::Char('c')) {
            self.hard_reset_ui_state();
        } else if key.bare_key == BareKey::Esc && key.has_no_modifiers() {
            close_self();
        } else if key.bare_key == BareKey::Char(' ') && key.has_no_modifiers() {
            if self.browsing_primary_modifier {
                let selected_primary_key_index = self.selected_primary_key_index;
                self.toggle_primary_modifier(selected_primary_key_index);
            } else if self.browsing_secondary_modifier {
                let selected_secondary_key_index = self.selected_secondary_key_index;
                self.toggle_secondary_modifier(selected_secondary_key_index);
            }
        } else if (key.bare_key == BareKey::Left
            || key.bare_key == BareKey::Right
            || key.bare_key == BareKey::Up
            || key.bare_key == BareKey::Down)
            && key.has_no_modifiers()
        {
            self.move_selection_for_default_preset(&key);
        } else if self.rebinding_main_leader {
            self.soft_reset_ui_state();
            self.main_leader = Some(key.clone());
            self.ui_is_dirty = true;
        }
        should_render
    }
    fn toggle_secondary_modifier(&mut self, secondary_modifier_index: usize) {
        if let Some(selected_modifier) = POSSIBLE_MODIFIERS.get(secondary_modifier_index) {
            if self.secondary_modifier.contains(selected_modifier) {
                self.secondary_modifier.remove(selected_modifier);
            } else {
                self.secondary_modifier.insert(*selected_modifier);
            }
            self.ui_is_dirty = true;
        }
    }
    fn toggle_primary_modifier(&mut self, primary_modifier_index: usize) {
        if let Some(selected_modifier) = POSSIBLE_MODIFIERS.get(primary_modifier_index) {
            if self.primary_modifier.contains(selected_modifier) {
                self.primary_modifier.remove(selected_modifier);
            } else {
                self.primary_modifier.insert(*selected_modifier);
            }
            self.ui_is_dirty = true;
        }
    }
    fn rebind_keys(&mut self, write_to_disk: bool) {
        let mut keys_to_unbind = vec![];
        let mut keys_to_bind = vec![];
        if self.currently_in_unlock_first() {
            if let Some(unlock_key) = &self.main_leader {
                self.bind_unlock_key(&mut keys_to_unbind, &mut keys_to_bind, unlock_key);
            }
            self.bind_all_secondary_actions(&mut keys_to_unbind, &mut keys_to_bind);
        } else {
            self.bind_all_secondary_actions(&mut keys_to_unbind, &mut keys_to_bind);
            self.bind_all_primary_actions(&mut keys_to_unbind, &mut keys_to_bind);
        }
        if write_to_disk {
            self.notification = Some("Configuration applied and saved to disk.".to_owned());
        } else {
            self.notification = Some("Configuration applied to current session.".to_owned());
        }
        rebind_keys(keys_to_unbind, keys_to_bind, write_to_disk);
    }
    fn bind_all_primary_actions(
        &self,
        keys_to_unbind: &mut Vec<(InputMode, KeyWithModifier)>,
        keys_to_bind: &mut Vec<(InputMode, KeyWithModifier, Vec<actions::Action>)>,
    ) {
        self.bind_primary_switch_to_mode_action(
            keys_to_unbind,
            keys_to_bind,
            InputMode::Locked,
            KeyWithModifier::new_with_modifiers(BareKey::Char('g'), self.primary_modifier.clone()),
        );
        self.bind_primary_switch_to_mode_action(
            keys_to_unbind,
            keys_to_bind,
            InputMode::Pane,
            KeyWithModifier::new_with_modifiers(BareKey::Char('p'), self.primary_modifier.clone()),
        );
        self.bind_primary_switch_to_mode_action(
            keys_to_unbind,
            keys_to_bind,
            InputMode::Tab,
            KeyWithModifier::new_with_modifiers(BareKey::Char('t'), self.primary_modifier.clone()),
        );
        self.bind_primary_switch_to_mode_action(
            keys_to_unbind,
            keys_to_bind,
            InputMode::Resize,
            KeyWithModifier::new_with_modifiers(BareKey::Char('n'), self.primary_modifier.clone()),
        );
        self.bind_primary_switch_to_mode_action(
            keys_to_unbind,
            keys_to_bind,
            InputMode::Move,
            KeyWithModifier::new_with_modifiers(BareKey::Char('h'), self.primary_modifier.clone()),
        );
        self.bind_primary_switch_to_mode_action(
            keys_to_unbind,
            keys_to_bind,
            InputMode::Scroll,
            KeyWithModifier::new_with_modifiers(BareKey::Char('s'), self.primary_modifier.clone()),
        );
        self.bind_primary_switch_to_mode_action(
            keys_to_unbind,
            keys_to_bind,
            InputMode::Session,
            KeyWithModifier::new_with_modifiers(BareKey::Char('o'), self.primary_modifier.clone()),
        );
        self.bind_quit_action(
            keys_to_unbind,
            keys_to_bind,
            KeyWithModifier::new_with_modifiers(BareKey::Char('q'), self.primary_modifier.clone()),
        );
    }
    fn bind_quit_action(
        &self,
        keys_to_unbind: &mut Vec<(InputMode, KeyWithModifier)>,
        keys_to_bind: &mut Vec<(InputMode, KeyWithModifier, Vec<actions::Action>)>,
        new_key: KeyWithModifier,
    ) {
        let all_relevant_modes = vec![
            InputMode::Normal,
            InputMode::Pane,
            InputMode::Tab,
            InputMode::Resize,
            InputMode::Move,
            InputMode::Search,
            InputMode::Scroll,
            InputMode::Session,
        ];
        for mode in &all_relevant_modes {
            for current_keybind in self.get_current_keybinds(*mode, &[actions::Action::Quit]) {
                keys_to_unbind.push((*mode, current_keybind));
            }
            keys_to_bind.push((*mode, new_key.clone(), vec![actions::Action::Quit]));
        }
    }
    fn get_current_keybinds(
        &self,
        in_mode: InputMode,
        actions: &[actions::Action],
    ) -> Vec<KeyWithModifier> {
        self.latest_mode_info
            .as_ref()
            .and_then(|m_i| {
                m_i.keybinds
                    .iter()
                    .find_map(|m| if m.0 == in_mode { Some(&m.1) } else { None })
            })
            .map(|k| {
                k.into_iter()
                    .filter_map(|(k, a)| if a == actions { Some(k.clone()) } else { None })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_else(Default::default)
    }
    fn bind_primary_switch_to_mode_action(
        &self,
        keys_to_unbind: &mut Vec<(InputMode, KeyWithModifier)>,
        keys_to_bind: &mut Vec<(InputMode, KeyWithModifier, Vec<actions::Action>)>,
        target_mode: InputMode,
        new_key: KeyWithModifier,
    ) {
        let all_relevant_modes = vec![
            InputMode::Locked,
            InputMode::Normal,
            InputMode::Pane,
            InputMode::Tab,
            InputMode::Resize,
            InputMode::Move,
            InputMode::Search,
            InputMode::Scroll,
            InputMode::Session,
        ];
        for mode in &all_relevant_modes {
            if mode == &target_mode {
                for current_keybind in self.get_current_keybinds(
                    *mode,
                    &[actions::Action::SwitchToMode {
                        input_mode: InputMode::Normal,
                    }],
                ) {
                    if current_keybind.bare_key != BareKey::Enter
                        && current_keybind.bare_key != BareKey::Esc
                    {
                        keys_to_unbind.push((*mode, current_keybind));
                    }
                }
            } else {
                for current_keybind in self.get_current_keybinds(
                    *mode,
                    &[actions::Action::SwitchToMode {
                        input_mode: target_mode,
                    }],
                ) {
                    keys_to_unbind.push((*mode, current_keybind));
                }
            }
        }
        for mode in &all_relevant_modes {
            if mode == &target_mode {
                keys_to_bind.push((
                    *mode,
                    new_key.clone(),
                    vec![actions::Action::SwitchToMode {
                        input_mode: InputMode::Normal,
                    }],
                ));
            } else if mode != &InputMode::Locked {
                keys_to_bind.push((
                    *mode,
                    new_key.clone(),
                    vec![actions::Action::SwitchToMode {
                        input_mode: target_mode,
                    }],
                ));
            }
        }
    }
    fn bind_all_secondary_actions(
        &self,
        keys_to_unbind: &mut Vec<(InputMode, KeyWithModifier)>,
        keys_to_bind: &mut Vec<(InputMode, KeyWithModifier, Vec<actions::Action>)>,
    ) {
        self.bind_actions(
            keys_to_unbind,
            keys_to_bind,
            &[actions::Action::NewPane {
                direction: None,
                pane_name: None,
                start_suppressed: false,
            }],
            KeyWithModifier::new_with_modifiers(
                BareKey::Char('n'),
                self.secondary_modifier.clone(),
            ),
        );
        self.bind_actions(
            keys_to_unbind,
            keys_to_bind,
            &[actions::Action::ToggleFloatingPanes],
            KeyWithModifier::new_with_modifiers(
                BareKey::Char('f'),
                self.secondary_modifier.clone(),
            ),
        );
        self.bind_actions(
            keys_to_unbind,
            keys_to_bind,
            &[actions::Action::MoveTab {
                direction: Direction::Left,
            }],
            KeyWithModifier::new_with_modifiers(
                BareKey::Char('i'),
                self.secondary_modifier.clone(),
            ),
        );
        self.bind_actions(
            keys_to_unbind,
            keys_to_bind,
            &[actions::Action::MoveTab {
                direction: Direction::Right,
            }],
            KeyWithModifier::new_with_modifiers(
                BareKey::Char('o'),
                self.secondary_modifier.clone(),
            ),
        );
        self.bind_actions(
            keys_to_unbind,
            keys_to_bind,
            &[actions::Action::MoveFocusOrTab {
                direction: Direction::Left,
            }],
            KeyWithModifier::new_with_modifiers(
                BareKey::Char('h'),
                self.secondary_modifier.clone(),
            ),
        );
        self.bind_actions(
            keys_to_unbind,
            keys_to_bind,
            &[actions::Action::MoveFocusOrTab {
                direction: Direction::Left,
            }],
            KeyWithModifier::new_with_modifiers(BareKey::Left, self.secondary_modifier.clone()),
        );
        self.bind_actions(
            keys_to_unbind,
            keys_to_bind,
            &[actions::Action::MoveFocusOrTab {
                direction: Direction::Right,
            }],
            KeyWithModifier::new_with_modifiers(
                BareKey::Char('l'),
                self.secondary_modifier.clone(),
            ),
        );
        self.bind_actions(
            keys_to_unbind,
            keys_to_bind,
            &[actions::Action::MoveFocusOrTab {
                direction: Direction::Right,
            }],
            KeyWithModifier::new_with_modifiers(BareKey::Right, self.secondary_modifier.clone()),
        );
        self.bind_actions(
            keys_to_unbind,
            keys_to_bind,
            &[actions::Action::MoveFocus {
                direction: Direction::Down,
            }],
            KeyWithModifier::new_with_modifiers(
                BareKey::Char('j'),
                self.secondary_modifier.clone(),
            ),
        );
        self.bind_actions(
            keys_to_unbind,
            keys_to_bind,
            &[actions::Action::MoveFocus {
                direction: Direction::Down,
            }],
            KeyWithModifier::new_with_modifiers(BareKey::Down, self.secondary_modifier.clone()),
        );
        self.bind_actions(
            keys_to_unbind,
            keys_to_bind,
            &[actions::Action::MoveFocus {
                direction: Direction::Up,
            }],
            KeyWithModifier::new_with_modifiers(
                BareKey::Char('k'),
                self.secondary_modifier.clone(),
            ),
        );
        self.bind_actions(
            keys_to_unbind,
            keys_to_bind,
            &[actions::Action::MoveFocus {
                direction: Direction::Up,
            }],
            KeyWithModifier::new_with_modifiers(BareKey::Up, self.secondary_modifier.clone()),
        );
        self.bind_actions(
            keys_to_unbind,
            keys_to_bind,
            &[actions::Action::Resize {
                resize: Resize::Increase,
                direction: None,
            }],
            KeyWithModifier::new_with_modifiers(
                BareKey::Char('+'),
                self.secondary_modifier.clone(),
            ),
        );
        self.bind_actions(
            keys_to_unbind,
            keys_to_bind,
            &[actions::Action::Resize {
                resize: Resize::Increase,
                direction: None,
            }],
            KeyWithModifier::new_with_modifiers(
                BareKey::Char('='),
                self.secondary_modifier.clone(),
            ),
        );
        self.bind_actions(
            keys_to_unbind,
            keys_to_bind,
            &[actions::Action::Resize {
                resize: Resize::Decrease,
                direction: None,
            }],
            KeyWithModifier::new_with_modifiers(
                BareKey::Char('-'),
                self.secondary_modifier.clone(),
            ),
        );
        self.bind_actions(
            keys_to_unbind,
            keys_to_bind,
            &[actions::Action::PreviousSwapLayout],
            KeyWithModifier::new_with_modifiers(
                BareKey::Char('['),
                self.secondary_modifier.clone(),
            ),
        );
        self.bind_actions(
            keys_to_unbind,
            keys_to_bind,
            &[actions::Action::NextSwapLayout],
            KeyWithModifier::new_with_modifiers(
                BareKey::Char(']'),
                self.secondary_modifier.clone(),
            ),
        );
    }
    fn bind_actions(
        &self,
        keys_to_unbind: &mut Vec<(InputMode, KeyWithModifier)>,
        keys_to_bind: &mut Vec<(InputMode, KeyWithModifier, Vec<actions::Action>)>,
        actions: &[actions::Action],
        key: KeyWithModifier,
    ) {
        for current_keybind in self.get_current_keybinds(InputMode::Normal, actions) {
            keys_to_unbind.push((InputMode::Normal, current_keybind));
        }
        for current_keybind in self.get_current_keybinds(InputMode::Locked, actions) {
            keys_to_unbind.push((InputMode::Locked, current_keybind));
        }
        keys_to_bind.push((InputMode::Normal, key.clone(), actions.to_vec()));
        keys_to_bind.push((InputMode::Locked, key, actions.to_vec()));
    }
    fn bind_unlock_key(
        &self,
        keys_to_unbind: &mut Vec<(InputMode, KeyWithModifier)>,
        keys_to_bind: &mut Vec<(InputMode, KeyWithModifier, Vec<actions::Action>)>,
        unlock_key: &KeyWithModifier,
    ) {
        if let Some(previous_unlock_key) = self.get_current_keybind(
            InputMode::Locked,
            &[actions::Action::SwitchToMode {
                input_mode: InputMode::Normal,
            }],
        ) {
            keys_to_unbind.push((InputMode::Locked, previous_unlock_key.clone()));
            keys_to_unbind.push((InputMode::Normal, previous_unlock_key.clone()));
            keys_to_unbind.push((InputMode::Pane, previous_unlock_key.clone()));
            keys_to_unbind.push((InputMode::Tab, previous_unlock_key.clone()));
            keys_to_unbind.push((InputMode::Resize, previous_unlock_key.clone()));
            keys_to_unbind.push((InputMode::Move, previous_unlock_key.clone()));
            keys_to_unbind.push((InputMode::Search, previous_unlock_key.clone()));
            keys_to_unbind.push((InputMode::Scroll, previous_unlock_key.clone()));
            keys_to_unbind.push((InputMode::Session, previous_unlock_key.clone()));
        }
        keys_to_bind.push((
            InputMode::Locked,
            unlock_key.clone(),
            vec![actions::Action::SwitchToMode {
                input_mode: InputMode::Normal,
            }],
        ));
        keys_to_bind.push((
            InputMode::Normal,
            unlock_key.clone(),
            vec![actions::Action::SwitchToMode {
                input_mode: InputMode::Locked,
            }],
        ));
        keys_to_bind.push((
            InputMode::Pane,
            unlock_key.clone(),
            vec![actions::Action::SwitchToMode {
                input_mode: InputMode::Locked,
            }],
        ));
        keys_to_bind.push((
            InputMode::Tab,
            unlock_key.clone(),
            vec![actions::Action::SwitchToMode {
                input_mode: InputMode::Locked,
            }],
        ));
        keys_to_bind.push((
            InputMode::Resize,
            unlock_key.clone(),
            vec![actions::Action::SwitchToMode {
                input_mode: InputMode::Locked,
            }],
        ));
        keys_to_bind.push((
            InputMode::Move,
            unlock_key.clone(),
            vec![actions::Action::SwitchToMode {
                input_mode: InputMode::Locked,
            }],
        ));
        keys_to_bind.push((
            InputMode::Search,
            unlock_key.clone(),
            vec![actions::Action::SwitchToMode {
                input_mode: InputMode::Locked,
            }],
        ));
        keys_to_bind.push((
            InputMode::Scroll,
            unlock_key.clone(),
            vec![actions::Action::SwitchToMode {
                input_mode: InputMode::Locked,
            }],
        ));
        keys_to_bind.push((
            InputMode::Session,
            unlock_key.clone(),
            vec![actions::Action::SwitchToMode {
                input_mode: InputMode::Locked,
            }],
        ));
    }
    fn get_current_keybind(
        &self,
        in_mode: InputMode,
        actions: &[actions::Action],
    ) -> Option<KeyWithModifier> {
        self.latest_mode_info
            .as_ref()
            .and_then(|m_i| {
                m_i.keybinds
                    .iter()
                    .find_map(|m| if m.0 == in_mode { Some(&m.1) } else { None })
            })
            .and_then(|k| {
                k.into_iter()
                    .find_map(|(k, a)| if a == actions { Some(k) } else { None })
            })
            .cloned()
    }
    fn soft_reset_ui_state(&mut self) {
        let mut latest_mode_info = self.latest_mode_info.take();
        let notification = self.notification.take();
        let primary_modifier = self.primary_modifier.clone();
        let secondary_modifier = self.secondary_modifier.clone();
        let main_leader = self.main_leader.clone();
        let ui_is_dirty = self.ui_is_dirty;
        let is_rebinding_for_presets = self.is_rebinding_for_presets;
        *self = Default::default();
        if let Some(latest_mode_info) = latest_mode_info.take() {
            self.update_mode_info(latest_mode_info);
        }
        self.notification = notification;
        self.primary_modifier = primary_modifier;
        self.secondary_modifier = secondary_modifier;
        self.main_leader = main_leader;
        self.ui_is_dirty = ui_is_dirty;
        self.is_rebinding_for_presets = is_rebinding_for_presets;
    }
    fn hard_reset_ui_state(&mut self) {
        let mut latest_mode_info = self.latest_mode_info.take();
        let notification = self.notification.take();
        let is_rebinding_for_presets = self.is_rebinding_for_presets;
        *self = Default::default();
        if let Some(latest_mode_info) = latest_mode_info.take() {
            self.update_mode_info(latest_mode_info);
        }
        self.notification = notification;
        self.is_rebinding_for_presets = is_rebinding_for_presets;
    }
    fn handle_unlock_first_key(&mut self, key: KeyWithModifier) -> bool {
        if key.bare_key == BareKey::Char('a')
            && key.has_modifiers(&[KeyModifier::Ctrl])
            && !self.is_rebinding_for_presets
        {
            let write_to_disk = true;
            self.rebind_keys(write_to_disk);
            self.hard_reset_ui_state();
        } else if key.bare_key == BareKey::Enter && key.has_no_modifiers() {
            let write_to_disk = false;
            self.rebind_keys(write_to_disk);
            self.hard_reset_ui_state();
        } else if key.is_key_with_ctrl_modifier(BareKey::Char('c')) {
            self.hard_reset_ui_state();
        } else if key.bare_key == BareKey::Esc && key.has_no_modifiers() {
            if self.rebinding_main_leader {
                self.soft_reset_ui_state();
            } else {
                close_self();
            }
        } else if key.bare_key == BareKey::Char(' ') && key.has_no_modifiers() {
            if self.main_leader_selected {
                self.set_rebinding_unlock_toggle();
            } else if self.browsing_secondary_modifier {
                let selected_secondary_key_index = self.selected_secondary_key_index;
                self.toggle_secondary_modifier(selected_secondary_key_index);
            }
        } else if (key.bare_key == BareKey::Left
            || key.bare_key == BareKey::Right
            || key.bare_key == BareKey::Up
            || key.bare_key == BareKey::Down)
            && key.has_no_modifiers()
        {
            self.move_selection_for_unlock_first(&key);
        } else if self.rebinding_main_leader {
            self.soft_reset_ui_state();
            self.main_leader = Some(key.clone());
            self.ui_is_dirty = true;
        }
        true
    }
    fn warning_text(&self, max_width: usize) -> Option<String> {
        if self.needs_kitty_support() {
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
                Some(String::from("No leaders. UI will be unusable."))
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
}
