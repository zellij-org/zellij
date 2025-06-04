mod line;
mod tab;

use std::cmp::{max, min};
use std::collections::BTreeMap;
use std::convert::TryInto;

use tab::get_tab_to_focus;
use zellij_tile::prelude::*;
use zellij_tile::prelude::actions::Action;

use crate::line::tab_line;
use crate::tab::tab_style;

#[derive(Debug, Default)]
pub struct LinePart {
    part: String,
    len: usize,
    tab_index: Option<usize>,
}

#[derive(Default)]
struct State {
    tabs: Vec<TabInfo>,
    active_tab_idx: usize,
    mode_info: ModeInfo,
    tab_line: Vec<LinePart>,
    text_copy_destination: Option<CopyDestination>,
    display_system_clipboard_failure: bool,
    is_tooltip: bool,
    config: BTreeMap<String, String>,
    display_area_rows: usize,
    display_area_cols: usize,
    own_plugin_id: Option<u32>,
}

static ARROW_SEPARATOR: &str = "";

register_plugin!(State);

impl ZellijPlugin for State {
    fn load(&mut self, configuration: BTreeMap<String, String>) {
        self.config = configuration.clone();
        self.is_tooltip = configuration
            .get("is_tooltip")
            .and_then(|v| v.parse().ok())
            .unwrap_or(false);
        
        set_selectable(false);
        subscribe(&[
            EventType::TabUpdate,
            EventType::ModeUpdate,
            EventType::Mouse,
            EventType::CopyToClipboard,
            EventType::InputReceived,
            EventType::SystemClipboardFailure,
        ]);
        self.own_plugin_id = Some(get_plugin_ids().plugin_id);
    }

    fn update(&mut self, event: Event) -> bool {
        let mut should_render = false;
        match event {
            Event::ModeUpdate(mode_info) => {
                let old_mode = self.mode_info.mode;
                let new_mode = mode_info.mode;
                let base_mode = mode_info.base_mode.unwrap_or(InputMode::Normal);
                
                if self.mode_info != mode_info {
                    should_render = true;
                }
                self.mode_info = mode_info;
                
                // Handle tooltip logic
                if !self.is_tooltip {
                    // If not tooltip and mode changed from base mode to another mode, launch tooltip
                    if old_mode == base_mode && new_mode != base_mode {
                        let mut tooltip_config = self.config.clone();
                        tooltip_config.insert("is_tooltip".to_string(), "true".to_string());
                        
                        pipe_message_to_plugin(
                            MessageToPlugin::new("launch_tooltip")
                                .with_plugin_url("zellij:OWN_URL")
                                .with_plugin_config(tooltip_config)
                                .with_floating_pane_coordinates(self.tooltip_coordinates())
                                .new_plugin_instance_should_have_pane_title("Keys")
                        );
                    }
                } else {
                    // If tooltip and mode changed to base mode, close self
                    if new_mode == base_mode {
                        close_self();
                    } else if new_mode != old_mode {
                      if let Some(own_plugin_id) = self.own_plugin_id {
                        let tooltip_coordinates = self.tooltip_coordinates();
                        change_floating_panes_coordinates(vec![(
                          PaneId::Plugin(own_plugin_id),
                          tooltip_coordinates
                        )]);
                      }
                    }
                }
            },
            Event::TabUpdate(tabs) => {
                for tab in &tabs {
                    if tab.active {
                        self.display_area_rows = tab.display_area_rows;
                        self.display_area_cols = tab.display_area_columns;
                        break;
                    }
                }
                if let Some(active_tab_index) = tabs.iter().position(|t| t.active) {
                    // tabs are indexed starting from 1 so we need to add 1
                    let active_tab_idx = active_tab_index + 1;
                    if self.active_tab_idx != active_tab_idx || self.tabs != tabs {
                        should_render = true;
                    }
                    self.active_tab_idx = active_tab_idx;
                    self.tabs = tabs;
                } else {
                    eprintln!("Could not find active tab.");
                }
            },
            Event::Mouse(me) => {
                // Only handle mouse events if not tooltip
                if !self.is_tooltip {
                    match me {
                        Mouse::LeftClick(_, col) => {
                            let tab_to_focus = get_tab_to_focus(&self.tab_line, self.active_tab_idx, col);
                            if let Some(idx) = tab_to_focus {
                                switch_tab_to(idx.try_into().unwrap());
                            }
                        },
                        Mouse::ScrollUp(_) => {
                            switch_tab_to(min(self.active_tab_idx + 1, self.tabs.len()) as u32);
                        },
                        Mouse::ScrollDown(_) => {
                            switch_tab_to(max(self.active_tab_idx.saturating_sub(1), 1) as u32);
                        },
                        _ => {},
                    }
                }
            },
            Event::CopyToClipboard(copy_destination) => {
                // Only handle clipboard events if not tooltip
                if !self.is_tooltip {
                    match self.text_copy_destination {
                        Some(text_copy_destination) => {
                            if text_copy_destination != copy_destination {
                                should_render = true;
                            }
                        },
                        None => {
                            should_render = true;
                        },
                    }
                    self.text_copy_destination = Some(copy_destination);
                }
            },
            Event::SystemClipboardFailure => {
                // Only handle clipboard failure if not tooltip
                if !self.is_tooltip {
                    should_render = true;
                    self.display_system_clipboard_failure = true;
                }
            },
            Event::InputReceived => {
                // Only handle input received if not tooltip
                if !self.is_tooltip {
                    if self.text_copy_destination.is_some()
                        || self.display_system_clipboard_failure == true
                    {
                        should_render = true;
                    }
                    self.text_copy_destination = None;
                    self.display_system_clipboard_failure = false;
                }
            },
            _ => {
                eprintln!("Got unrecognized event: {:?}", event);
            },
        };
        should_render
    }

    fn render(&mut self, rows: usize, cols: usize) {
        if self.is_tooltip {
            self.render_tooltip(rows, cols);
        } else {
            self.render_tab_line(cols);
        }
    }
}

impl State {

    fn find_predetermined_actions<F>(&self, mode: InputMode, predicates: Vec<F>) -> Vec<(String, String)>
    where
        F: Fn(&Action) -> bool,
    {
        let mut result = Vec::new();
        let keybinds = self.mode_info.get_keybinds_for_mode(mode);
        let mut processed_action_types = std::collections::HashSet::new();
        
        // Iterate through predicates in order to maintain the desired sequence
        for predicate in predicates {
            // Find the first matching action for this predicate
            let mut found_match = false;
            for (key, actions) in &keybinds {
                if let Some(first_action) = actions.first() {
                    if predicate(first_action) {
                        let action_type = self.get_action_type(first_action);
                        
                        // Skip if we've already processed this action type
                        if processed_action_types.contains(&action_type) {
                            found_match = true;
                            break;
                        }
                        
                        let mut matching_keys = Vec::new();
                        
                        // Find all keys that match this action type (including different directions)
                        for (inner_key, inner_actions) in &keybinds {
                            if let Some(inner_first_action) = inner_actions.first() {
                                if self.get_action_type(inner_first_action) == action_type {
                                    matching_keys.push(format!("{}", inner_key));
                                }
                            }
                        }
                        
                        if !matching_keys.is_empty() {
                            let description = self.get_action_description(&action_type);
                            let should_add_brackets_to_keys = mode != InputMode::Normal;
                            let grouped_keys = self.group_key_sets(&matching_keys, should_add_brackets_to_keys);
                            result.push((grouped_keys, description));
                            processed_action_types.insert(action_type);
                        }
                        
                        found_match = true;
                        break;
                    }
                }
            }
            
            // If we found a match for this predicate, we've processed it
            if found_match {
                continue;
            }
        }
        
        result
    }

    /// Get a canonical action type identifier, treating None direction as separate
    fn get_action_type(&self, action: &Action) -> String {
        match action {
            Action::MoveFocus(_) => "MoveFocus(Direction)".to_string(),
            Action::MovePane(Some(_)) => "MovePane(Direction)".to_string(),
            Action::MovePane(None) => "MovePane(None)".to_string(),
            Action::Resize(resize_type, Some(_)) => format!("Resize({:?}, Direction)", resize_type),
            Action::Resize(_resize_type, None) => format!("Resize(any)"),
            Action::Search(_) => "Search(Direction)".to_string(),
            Action::SearchToggleOption(option) => format!("SearchToggleOption({:?})", option),
            Action::NewPane(Some(_), _, _) => "NewPane(Direction)".to_string(),
            Action::NewPane(None, _, _) => "NewPane(None)".to_string(),
            Action::BreakPaneLeft | Action::BreakPaneRight => "BreakPaneLeftOrRight".to_string(),
            Action::GoToPreviousTab | Action::GoToNextTab => "GoToAdjacentTab".to_string(),
            Action::ScrollUp | Action::ScrollDown => "Scroll".to_string(),
            Action::PageScrollUp | Action::PageScrollDown => "PageScroll".to_string(),
            Action::HalfPageScrollUp | Action::HalfPageScrollDown => "HalfPageScroll".to_string(),
            _ if action.launches_plugin("session-manager") => "SessionManager".to_string(),
            _ if action.launches_plugin("configuration") => "Configuration".to_string(),
            _ if action.launches_plugin("plugin-manager") => "PluginManager".to_string(),
            _ if action.launches_plugin("zellij:about") => "About".to_string(),
            _ => format!("{:?}", action),
        }
    }
    
    /// Get a user-friendly description for an action type
    fn get_action_description(&self, action_type: &str) -> String {
        match action_type {
            "MoveFocus(Direction)" => "Move focus".to_string(),
            "MovePane(Direction)" => "Move pane".to_string(),
            "Resize(Increase, Direction)" => "Increase size in direction".to_string(),
            "Resize(any)" => "Increase or decrease size".to_string(),
            "Resize(Decrease, Direction)" => "Decrease size in direction".to_string(),
            "Search(Direction)" => "Search".to_string(),
            "NewPane(Direction)" => "Split right/down".to_string(),
            "NewPane(None)" => "New pane".to_string(),
            "BreakPaneLeftOrRight" => "Break pane to adjacent tab".to_string(),
            "GoToAdjacentTab" => "Move tab focus".to_string(),
            "Scroll" => "Scroll".to_string(),
            "PageScroll" => "Scroll page".to_string(),
            "HalfPageScroll" => "Scroll half Page".to_string(),
            "SessionManager" => "Session manager".to_string(),
            "PluginManager" => "Plugin manager".to_string(),
            "Configuration" => "Configuration".to_string(),
            "About" => "About Zellij".to_string(),
            "SwitchToMode(RenamePane)" => "Rename pane".to_string(),
            "SwitchToMode(RenameTab)" => "Rename tab".to_string(),
            "TogglePaneEmbedOrFloating" => "Float or embed".to_string(),
            "ToggleFocusFullscreen" => "Toggle fullscreen".to_string(),
            "ToggleFloatingPanes" => "Show/hide floating panes".to_string(),
            "CloseFocus" => "Close pane".to_string(),
            "CloseTab" => "Close tab".to_string(),
            "ToggleActiveSyncTab" => "Sync panes in tab".to_string(),
            "ToggleTab" => "Circle tab focus".to_string(),
            "BreakPane" => "Break pane to new tab".to_string(),
            "SwitchToMode(EnterSearch)" => "Search".to_string(),
            "EditScrollback" => "Open pane scrollback in editor".to_string(),
            s if s.starts_with("NewTab") => "New tab".to_string(),
            s if s.starts_with("SearchToggleOption") => "Toggle search option".to_string(),
            s if s.starts_with("SwitchToMode") => {
                if let Some(mode_part) = s.strip_prefix("SwitchToMode(") {
                    if let Some(mode_name) = mode_part.strip_suffix(")") {
                        format!("{}", mode_name.to_uppercase())
                    } else {
                        s.to_string()
                    }
                } else {
                    s.to_string()
                }
            },
            _ => action_type.to_string(),
        }
    }
    
    /// Group keys into sets and separate different key types with '|'
    fn group_key_sets(&self, keys: &[String], should_add_brackets_to_keys: bool) -> String {
        if keys.is_empty() {
            return String::new();
        }
        
        if keys.len() == 1 {
          if should_add_brackets_to_keys {
            return format!("<{}>", keys[0]);
          } else {
            return format!("{}", keys[0]);
          }
        }
        
        // Group keys by type
        let mut arrow_keys = Vec::new();
        let mut hjkl_lower = Vec::new();
        let mut hjkl_upper = Vec::new();
        let mut square_bracket_keys = Vec::new();
        let mut plus_minus_keys = Vec::new();
        let mut pgup_pgdown = Vec::new();
        let mut other_keys = Vec::new();
        
        for key in keys {
            match key.as_str() {
                "Left" | "←" => arrow_keys.push("←"),
                "Down" | "↓" => arrow_keys.push("↓"),
                "Up" | "↑" => arrow_keys.push("↑"),
                "Right" | "→" => arrow_keys.push("→"),
                "h" => hjkl_lower.push("h"),
                "j" => hjkl_lower.push("j"),
                "k" => hjkl_lower.push("k"),
                "l" => hjkl_lower.push("l"),
                "H" => hjkl_upper.push("H"),
                "J" => hjkl_upper.push("J"),
                "K" => hjkl_upper.push("K"),
                "L" => hjkl_upper.push("L"),
                "[" => square_bracket_keys.push("["),
                "]" => square_bracket_keys.push("]"),
                "+" => plus_minus_keys.push("+"),
                "-" => plus_minus_keys.push("-"),
                "=" => plus_minus_keys.push("="),
                "PgUp" => pgup_pgdown.push("PgUp"),
                "PgDn" => pgup_pgdown.push("PgDn"),
                // _ => other_keys.push(key.clone()),
                _ => if should_add_brackets_to_keys {
                  other_keys.push(format!("<{}>", key))
                } else {
                  other_keys.push(key.clone())
                }
            }
        }
        
        let mut groups = Vec::new();
        
        // Add hjkl group if present (prioritize hjkl over arrows)
        if !hjkl_lower.is_empty() {
            // Sort in logical order: h, j, k, l (left, down, up, right)
            hjkl_lower.sort_by(|a, b| {
                let order = ["h", "j", "k", "l"];
                let pos_a = order.iter().position(|&x| &x == a).unwrap_or(usize::MAX);
                let pos_b = order.iter().position(|&x| &x == b).unwrap_or(usize::MAX);
                pos_a.cmp(&pos_b)
            });
            if should_add_brackets_to_keys {
              groups.push(format!("<{}>", hjkl_lower.join("")));
            } else {
              groups.push(hjkl_lower.join(""));
            }
        }
        
        // Add HJKL group if present
        if !hjkl_upper.is_empty() {
            // Sort in logical order: H, J, K, L
            hjkl_upper.sort_by(|a, b| {
                let order = ["H", "J", "K", "L"];
                let pos_a = order.iter().position(|&x| &x == a).unwrap_or(usize::MAX);
                let pos_b = order.iter().position(|&x| &x == b).unwrap_or(usize::MAX);
                pos_a.cmp(&pos_b)
            });
            if should_add_brackets_to_keys {
              groups.push(format!("<{}>", hjkl_upper.join("")));
            } else {
              groups.push(hjkl_upper.join(""));
            }
        }
        
        // Add arrow keys group if present (and not redundant with hjkl)
        if !arrow_keys.is_empty() {
            // Remove duplicates and sort in logical order: ←, ↓, ↑, →
            arrow_keys.sort();
            arrow_keys.dedup();
            arrow_keys.sort_by(|a, b| {
                let order = ["←", "↓", "↑", "→"];
                let pos_a = order.iter().position(|&x| &x == a).unwrap_or(usize::MAX);
                let pos_b = order.iter().position(|&x| &x == b).unwrap_or(usize::MAX);
                pos_a.cmp(&pos_b)
            });
            if should_add_brackets_to_keys {
              groups.push(format!("<{}>", arrow_keys.join("")));
            } else {
              groups.push(arrow_keys.join(""));
            }
        }

        if !square_bracket_keys.is_empty() {
            square_bracket_keys.sort_by(|a, b| {
                let order = ["[", "]"];
                let pos_a = order.iter().position(|&x| &x == a).unwrap_or(usize::MAX);
                let pos_b = order.iter().position(|&x| &x == b).unwrap_or(usize::MAX);
                pos_a.cmp(&pos_b)
            });
            if should_add_brackets_to_keys {
              groups.push(format!("<{}>", square_bracket_keys.join("")));
            } else {
              groups.push(square_bracket_keys.join(""));
            }
        }

        if !plus_minus_keys.is_empty() {
            plus_minus_keys.sort_by(|a, b| {
                let order = ["+", "-"];
                let pos_a = order.iter().position(|&x| &x == a).unwrap_or(usize::MAX);
                let pos_b = order.iter().position(|&x| &x == b).unwrap_or(usize::MAX);
                pos_a.cmp(&pos_b)
            });
            if plus_minus_keys.contains(&"+") && plus_minus_keys.contains(&"=") {
              plus_minus_keys.retain(|k| k != &"=");
            }
            if should_add_brackets_to_keys {
              groups.push(format!("<{}>", plus_minus_keys.join("")));
            } else {
              groups.push(plus_minus_keys.join(""));
            }
        }

        if !pgup_pgdown.is_empty() {
            pgup_pgdown.sort_by(|a, b| {
                let order = ["PgUp", "PgDn"];
                let pos_a = order.iter().position(|&x| &x == a).unwrap_or(usize::MAX);
                let pos_b = order.iter().position(|&x| &x == b).unwrap_or(usize::MAX);
                pos_a.cmp(&pos_b)
            });
            // here we separate with a pipe because otherwise its unclear
            if should_add_brackets_to_keys {
              groups.push(format!("<{}>", pgup_pgdown.join("|")));
            } else {
              groups.push(pgup_pgdown.join("|"));
            }
        }
        
        // Add other keys with | separator
        if !other_keys.is_empty() {
          groups.push(other_keys.join("/"));
        }

        groups.join("/")
    }

    fn get_predetermined_actions(&self, mode: InputMode) -> Vec<(String, String)> {
        match mode {
            InputMode::Normal => {
                let ordered_predicates = vec![
                    |action: &Action| matches!(action, Action::SwitchToMode(InputMode::Pane)),
                    |action: &Action| matches!(action, Action::SwitchToMode(InputMode::Tab)),
                    |action: &Action| matches!(action, Action::SwitchToMode(InputMode::Resize)),
                    |action: &Action| matches!(action, Action::SwitchToMode(InputMode::Move)),
                    |action: &Action| matches!(action, Action::SwitchToMode(InputMode::Scroll)),
                    |action: &Action| matches!(action, Action::SwitchToMode(InputMode::Session)),
                    |action: &Action| matches!(action, Action::Quit),
                ];
                self.find_predetermined_actions(mode, ordered_predicates)
            },
            InputMode::Pane => {
                let ordered_predicates = vec![
                    |action: &Action| matches!(action, Action::NewPane(None, None, false)),
                    |action: &Action| matches!(action, Action::MoveFocus(Direction::Left)),
                    |action: &Action| matches!(action, Action::MoveFocus(Direction::Down)),
                    |action: &Action| matches!(action, Action::MoveFocus(Direction::Up)),
                    |action: &Action| matches!(action, Action::MoveFocus(Direction::Right)),
                    |action: &Action| matches!(action, Action::CloseFocus),
                    |action: &Action| matches!(action, Action::SwitchToMode(InputMode::RenamePane)),
                    |action: &Action| matches!(action, Action::ToggleFocusFullscreen),
                    |action: &Action| matches!(action, Action::ToggleFloatingPanes),
                    |action: &Action| matches!(action, Action::TogglePaneEmbedOrFloating),
                    |action: &Action| matches!(action, Action::NewPane(Some(Direction::Right), None, false)),
                    |action: &Action| matches!(action, Action::NewPane(Some(Direction::Down), None, false)),
                ];
                self.find_predetermined_actions(mode, ordered_predicates)
            },
            InputMode::Tab => {
                let ordered_predicates = vec![
                    |action: &Action| matches!(action, Action::GoToPreviousTab),
                    |action: &Action| matches!(action, Action::GoToNextTab),
                    |action: &Action| matches!(action, Action::NewTab(None, _, None, None, None, true)),
                    |action: &Action| matches!(action, Action::CloseTab),
                    |action: &Action| matches!(action, Action::SwitchToMode(InputMode::RenameTab)),
                    |action: &Action| matches!(action, Action::TabNameInput(_)),
                    |action: &Action| matches!(action, Action::ToggleActiveSyncTab),
                    |action: &Action| matches!(action, Action::BreakPane),
                    |action: &Action| matches!(action, Action::BreakPaneLeft),
                    |action: &Action| matches!(action, Action::BreakPaneRight),
                    |action: &Action| matches!(action, Action::ToggleTab),
                ];
                self.find_predetermined_actions(mode, ordered_predicates)
            },
            InputMode::Resize => {
                let ordered_predicates = vec![
                    |action: &Action| matches!(action, Action::Resize(Resize::Increase, None)),
                    |action: &Action| matches!(action, Action::Resize(Resize::Decrease, None)),
                    |action: &Action| matches!(action, Action::Resize(Resize::Increase, Some(Direction::Left))),
                    |action: &Action| matches!(action, Action::Resize(Resize::Increase, Some(Direction::Down))),
                    |action: &Action| matches!(action, Action::Resize(Resize::Increase, Some(Direction::Up))),
                    |action: &Action| matches!(action, Action::Resize(Resize::Increase, Some(Direction::Right))),
                    |action: &Action| matches!(action, Action::Resize(Resize::Decrease, Some(Direction::Left))),
                    |action: &Action| matches!(action, Action::Resize(Resize::Decrease, Some(Direction::Down))),
                    |action: &Action| matches!(action, Action::Resize(Resize::Decrease, Some(Direction::Up))),
                    |action: &Action| matches!(action, Action::Resize(Resize::Decrease, Some(Direction::Right))),
                ];
                self.find_predetermined_actions(mode, ordered_predicates)
            },
            InputMode::Move => {
                let ordered_predicates = vec![
                    |action: &Action| matches!(action, Action::MovePane(Some(Direction::Left))),
                    |action: &Action| matches!(action, Action::MovePane(Some(Direction::Down))),
                    |action: &Action| matches!(action, Action::MovePane(Some(Direction::Up))),
                    |action: &Action| matches!(action, Action::MovePane(Some(Direction::Right))),
                ];
                self.find_predetermined_actions(mode, ordered_predicates)
            },
            InputMode::Scroll => {
                let ordered_predicates = vec![
                    |action: &Action| matches!(action, Action::ScrollDown),
                    |action: &Action| matches!(action, Action::ScrollUp),
                    |action: &Action| matches!(action, Action::HalfPageScrollDown),
                    |action: &Action| matches!(action, Action::HalfPageScrollUp),
                    |action: &Action| matches!(action, Action::PageScrollDown),
                    |action: &Action| matches!(action, Action::PageScrollUp),
                    |action: &Action| matches!(action, Action::SwitchToMode(InputMode::EnterSearch)),
                    |action: &Action| matches!(action, Action::EditScrollback),
                ];
                self.find_predetermined_actions(mode, ordered_predicates)
            },
            InputMode::Search => {
                let ordered_predicates = vec![
                    |action: &Action| matches!(action, Action::SwitchToMode(InputMode::EnterSearch)),
                    |action: &Action| matches!(action, Action::SearchInput(_)),
                    |action: &Action| matches!(action, Action::ScrollDown),
                    |action: &Action| matches!(action, Action::ScrollUp),
                    |action: &Action| matches!(action, Action::PageScrollDown),
                    |action: &Action| matches!(action, Action::PageScrollUp),
                    |action: &Action| matches!(action, Action::HalfPageScrollDown),
                    |action: &Action| matches!(action, Action::HalfPageScrollUp),
                    |action: &Action| matches!(action, Action::Search(actions::SearchDirection::Down)),
                    |action: &Action| matches!(action, Action::Search(actions::SearchDirection::Up)),
                    |action: &Action| matches!(action, Action::SearchToggleOption(actions::SearchOption::CaseSensitivity)),
                    |action: &Action| matches!(action, Action::SearchToggleOption(actions::SearchOption::Wrap)),
                    |action: &Action| matches!(action, Action::SearchToggleOption(actions::SearchOption::WholeWord)),
                ];
                self.find_predetermined_actions(mode, ordered_predicates)
            },
            InputMode::Session => {
                let ordered_predicates = vec![
                    |action: &Action| matches!(action, Action::Detach),
                    |action: &Action| action.launches_plugin("session-manager"),
                    |action: &Action| action.launches_plugin("plugin-manager"),
                    |action: &Action| action.launches_plugin("configuration"),
                    |action: &Action| action.launches_plugin("zellij:about"),
                ];
                self.find_predetermined_actions(mode, ordered_predicates)
            },
            InputMode::Locked => Vec::new(),
            InputMode::EnterSearch => Vec::new(),
            InputMode::RenameTab => Vec::new(),
            InputMode::RenamePane => Vec::new(),
            InputMode::Prompt => Vec::new(),
            InputMode::Tmux => Vec::new(),
        }
    }

    fn render_tooltip(&self, rows: usize, cols: usize) {
        let current_mode = self.mode_info.mode;

        if current_mode == InputMode::Normal {
            let (text_components, tooltip_rows, tooltip_columns) = self.normal_mode_toolip(current_mode);
            // Render each text component at its calculated position
            let base_x = cols.saturating_sub(tooltip_columns) / 2;
            let base_y = rows.saturating_sub(tooltip_rows) / 2;
            for (text, ribbon, x, y) in text_components {
                let text_width = text.content().chars().count();
                print_text_with_coordinates(text, base_x + x, base_y + y, None, None);
                print_ribbon_with_coordinates(ribbon, base_x + x + text_width + 1, base_y + y, None, None);
            }
        } else {
            let (table, tooltip_rows, tooltip_columns) = self.other_mode_tooltip(current_mode);
            let base_x = cols.saturating_sub(tooltip_columns) / 2;
            let base_y = rows.saturating_sub(tooltip_rows) / 2;
            print_table_with_coordinates(table, base_x, base_y, None, None);
        }
    }

    fn normal_mode_toolip(&self, current_mode: InputMode) -> (Vec<(Text, Text, usize, usize)>, usize, usize) {
        let actions = self.get_predetermined_actions(current_mode);
        let y = 0;
        let mut running_x = 0;
        let mut components = Vec::new();
        let mut max_columns = 0;

        for (key, description) in actions {
            let text = Text::new(&key).color_all(3);
            let ribbon = Text::new(&description);
            
            let line_length = key.chars().count() + 1 + description.chars().count();
            
            components.push((text, ribbon, running_x, y));
            running_x += line_length + 5;
            max_columns = max_columns.max(running_x);
        }

        let total_rows = 1;
        (components, total_rows, max_columns)
    }
    fn other_mode_tooltip(&self, current_mode: InputMode) -> (Table, usize, usize) {
        let actions = self.get_predetermined_actions(current_mode);
        let actions_vec: Vec<_> = actions.into_iter().collect();
        
        let mut table = Table::new().add_row(vec![" ".to_owned(); 2]);
        let mut key_width = 0;
        let mut action_width = 0;
        let mut row_count = 1; // Start with header row
        
        for (key, description) in actions_vec.into_iter() {
            let description_formatted = format!("- {}", description);
            key_width = key_width.max(key.chars().count());
            action_width = action_width.max(description_formatted.chars().count());
            table = table.add_styled_row(vec![
                Text::new(&key).color_all(3),
                Text::new(description_formatted)
            ]);
            row_count += 1;
        }
        
        let total_width = key_width + action_width + 1; // +1 for separator
        (table, row_count, total_width)
    }

    fn render_tab_line(&mut self, cols: usize) {
      // TODO: why do we render simplified ui?
        if let Some(copy_destination) = self.text_copy_destination {
            let hint = text_copied_hint(copy_destination).part;

            let background = self.mode_info.style.colors.text_unselected.background;
            match background {
                PaletteColor::Rgb((r, g, b)) => {
                    print!("{}\u{1b}[48;2;{};{};{}m\u{1b}[0K", hint, r, g, b);
                },
                PaletteColor::EightBit(color) => {
                    print!("{}\u{1b}[48;5;{}m\u{1b}[0K", hint, color);
                },
            }
        } else if self.display_system_clipboard_failure {
            let hint = system_clipboard_error().part;
            let background = self.mode_info.style.colors.text_unselected.background;
            match background {
                PaletteColor::Rgb((r, g, b)) => {
                    print!("{}\u{1b}[48;2;{};{};{}m\u{1b}[0K", hint, r, g, b);
                },
                PaletteColor::EightBit(color) => {
                    print!("{}\u{1b}[48;5;{}m\u{1b}[0K", hint, color);
                },
            }
        } else {
            if self.tabs.is_empty() {
                return;
            }
            let mut all_tabs: Vec<LinePart> = vec![];
            let mut active_tab_index = 0;
            let mut active_swap_layout_name = None;
            let mut is_swap_layout_dirty = false;
            let mut is_alternate_tab = false;
            for t in &mut self.tabs {
                let mut tabname = t.name.clone();
                if t.active && self.mode_info.mode == InputMode::RenameTab {
                    if tabname.is_empty() {
                        tabname = String::from("Enter name...");
                    }
                    active_tab_index = t.position;
                } else if t.active {
                    active_tab_index = t.position;
                    is_swap_layout_dirty = t.is_swap_layout_dirty;
                    active_swap_layout_name = t.active_swap_layout_name.clone();
                }
                eprintln!("mode_info.capabilities: {:?}", self.mode_info.capabilities);
                let tab = tab_style(
                    tabname,
                    t,
                    is_alternate_tab,
                    self.mode_info.style.colors,
                    self.mode_info.capabilities,
                );
                is_alternate_tab = !is_alternate_tab;
                all_tabs.push(tab);
            }
            self.tab_line = tab_line(
                self.mode_info.session_name.as_deref(),
                all_tabs,
                active_tab_index,
                cols.saturating_sub(1),
                self.mode_info.style.colors,
                self.mode_info.capabilities,
                self.mode_info.style.hide_session_name,
                self.mode_info.mode,
                &active_swap_layout_name,
                is_swap_layout_dirty,
            );
            let output = self
                .tab_line
                .iter()
                .fold(String::new(), |output, part| output + &part.part);
            let background = self.mode_info.style.colors.text_unselected.background;
            match background {
                PaletteColor::Rgb((r, g, b)) => {
                    print!("{}\u{1b}[48;2;{};{};{}m\u{1b}[0K", output, r, g, b);
                },
                PaletteColor::EightBit(color) => {
                    print!("{}\u{1b}[48;5;{}m\u{1b}[0K", output, color);
                },
            }
        }
    }
    fn tooltip_coordinates(&self) -> FloatingPaneCoordinates {
      let current_mode = self.mode_info.mode;
      let (tooltip_rows, tooltip_cols) = match current_mode {
        InputMode::Normal => {
          let (_, tooltip_rows, tooltip_cols) = self.normal_mode_toolip(current_mode);
          (tooltip_rows, tooltip_cols)
        }
        _ => {
          let (_, tooltip_rows, tooltip_cols) = self.other_mode_tooltip(current_mode);
          (tooltip_rows + 1, tooltip_cols) // + 1 for the invisible table title
        }
      };
        let width = tooltip_cols + 4; // 2 for the borders, 2 for padding inside the pane
        let height = tooltip_rows + 2; // 2 for the borders
        let x_position = 2;
        let y_position = self.display_area_rows.saturating_sub(height + 2);
        FloatingPaneCoordinates::new(
            Some(format!("{}", x_position)),
            Some(format!("{}", y_position)),
            Some(format!("{}", width)),
            Some(format!("{}", height)),
            Some(true),
        ).unwrap_or_else(Default::default)
    }
    fn handle_tab_update(&mut self, tab_infos: Vec<TabInfo>) -> bool {
        for tab in tab_infos {
            if tab.active {
                self.display_area_rows = tab.display_area_rows;
                self.display_area_cols = tab.display_area_columns;
                break;
            }
        }

        false
    }
}

fn format_actions(actions: &[Action]) -> String {
    actions
        .iter()
        .map(|action| format!("{:?}", action))
        .collect::<Vec<_>>()
        .join(", ")
}

pub fn text_copied_hint(copy_destination: CopyDestination) -> LinePart {
    let hint = match copy_destination {
        CopyDestination::Command => "Text piped to external command",
        #[cfg(not(target_os = "macos"))]
        CopyDestination::Primary => "Text copied to system primary selection",
        #[cfg(target_os = "macos")] // primary selection does not exist on macos
        CopyDestination::Primary => "Text copied to system clipboard",
        CopyDestination::System => "Text copied to system clipboard",
    };
    LinePart {
        part: serialize_text(&Text::new(&hint).color_range(2, ..).opaque()),
        len: hint.len(),
        tab_index: None,
    }
}

pub fn system_clipboard_error() -> LinePart {
    let hint = " Error using the system clipboard.";
    LinePart {
        part: serialize_text(&Text::new(&hint).color_range(2, ..).opaque()),
        len: hint.len(),
        tab_index: None,
    }
}
