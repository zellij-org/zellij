pub mod ui;
pub mod state;

use ui::{PaneItem};
use state::{MarkedIndex, VisibilityAndFocus};
use zellij_tile::prelude::*;
use std::collections::BTreeMap;

#[derive(Debug, Default)]
pub struct App {
    own_plugin_id: Option<u32>,
    own_client_id: Option<ClientId>,
    own_tab_index: Option<usize>,
    total_tabs_in_session: Option<usize>,
    search_string: String,
    previous_search_string: String, // used eg. for the new tab title when breaking panes
    left_side_panes: Vec<PaneItem>,
    right_side_panes: Vec<PaneItem>,
    search_results: Option<Vec<PaneItem>>,
    visibility_and_focus: VisibilityAndFocus,
    marked_index: Option<MarkedIndex>,
}

register_plugin!(App);

impl ZellijPlugin for App {
    fn load(&mut self, _configuration: BTreeMap<String, String>) {
        subscribe(&[
            EventType::Key,
            EventType::Mouse,
            EventType::ModeUpdate,
            EventType::RunCommandResult,
            EventType::TabUpdate,
            EventType::PaneUpdate,
            EventType::FailedToWriteConfigToDisk,
            EventType::ConfigWasWrittenToDisk,
            EventType::BeforeClose,
        ]);
        let plugin_ids = get_plugin_ids();
        self.own_plugin_id = Some(plugin_ids.plugin_id);
        self.own_client_id = Some(plugin_ids.client_id);
        rename_plugin_pane(plugin_ids.plugin_id, "Multiple Select");
    }
    fn update(&mut self, event: Event) -> bool {
        let mut should_render = false;
        match event {
            Event::PaneUpdate(pane_manifest) => {
                self.react_to_zellij_state_update(pane_manifest);
                should_render = true;
            }
            Event::Key(key) => {
                match key.bare_key {
                    BareKey::Tab if key.has_no_modifiers() => {
                        self.visibility_and_focus.toggle_focus();
                        self.marked_index = None;
                        self.update_highlighted_panes();
                        should_render = true;
                    }
                    BareKey::Char(character) if key.has_no_modifiers() && self.visibility_and_focus.left_side_is_focused() && self.marked_index.is_none() => {
                        self.search_string.push(character);
                        self.update_search_results();
                        should_render = true;
                    }
                    BareKey::Backspace if key.has_no_modifiers() && self.visibility_and_focus.left_side_is_focused() && self.marked_index.is_none() => {
                        self.search_string.pop();
                        self.update_search_results();
                        should_render = true;
                    }
                    BareKey::Enter if key.has_no_modifiers() => {
                        if self.visibility_and_focus.left_side_is_focused() {
                            if let Some(marked_index) = self.marked_index.take() {
                                let keep_left_side_focused = false;
                                self.group_panes(marked_index, keep_left_side_focused);
                            } else {
                                match self.search_results.take() {
                                    Some(search_results) => self.group_search_results(search_results),
                                    None => self.group_all_panes()
                                }
                                self.handle_left_side_emptied();
                            }
                        }
                        should_render = true;
                    }
                    BareKey::Right if key.has_no_modifiers() && self.visibility_and_focus.left_side_is_focused() => {
                        if let Some(marked_index) = self.marked_index.take() {
                            let keep_left_side_focused = true;
                            self.group_panes(marked_index, keep_left_side_focused);
                            should_render = true;
                        }
                    }
                    BareKey::Left if key.has_no_modifiers() && self.visibility_and_focus.right_side_is_focused() => {
                        if self.visibility_and_focus.right_side_is_focused() {
                            if let Some(marked_index) = self.marked_index.take() {
                                self.ungroup_panes(marked_index);
                                should_render = true;
                            }
                        }
                    }
                    BareKey::Char('c') if key.has_modifiers(&[KeyModifier::Ctrl]) => {
                        if self.visibility_and_focus.right_side_is_focused() {
                            // this means we're in the selection panes part and we want to clear
                            // them
                            self.ungroup_all_panes();
                        } else if self.visibility_and_focus.left_side_is_focused() {
                            if self.marked_index.is_some() {
                                self.marked_index = None;
                                self.update_highlighted_panes();
                            } else {
                                self.ungroup_all_panes_and_close_self();
                            }
                        }
                        should_render = true;
                    }
                    BareKey::Down if key.has_no_modifiers() => {
                        self.move_marked_index_down();
                        should_render = true;
                    }
                    BareKey::Up if key.has_no_modifiers() => {
                        self.move_marked_index_up();
                        should_render = true;
                    }
                    BareKey::Char(' ') if key.has_no_modifiers() && self.marked_index.is_some() => {
                        self.mark_entry();
                        should_render = true;
                    }
                    BareKey::Char('b') if key.has_no_modifiers() && self.visibility_and_focus.right_side_is_focused() => {
                        self.break_grouped_panes_to_new_tab();
                    }
                    BareKey::Char('s') if key.has_no_modifiers() && self.visibility_and_focus.right_side_is_focused() => {
                        self.stack_grouped_panes();
                    }
                    BareKey::Char('f') if key.has_no_modifiers() && self.visibility_and_focus.right_side_is_focused() => {
                        self.float_grouped_panes();
                    }
                    BareKey::Char('e') if key.has_no_modifiers() && self.visibility_and_focus.right_side_is_focused() => {
                        self.embed_grouped_panes();
                    }
                    BareKey::Char('r') if key.has_no_modifiers() && self.visibility_and_focus.right_side_is_focused() => {
                        self.break_grouped_panes_right();
                    }
                    BareKey::Char('l') if key.has_no_modifiers() && self.visibility_and_focus.right_side_is_focused() => {
                        self.break_grouped_panes_left();
                    }
                    BareKey::Char('c') if key.has_no_modifiers() && self.visibility_and_focus.right_side_is_focused() => {
                        self.close_grouped_panes();
                    }
                    _ => {}
                }
            },
            Event::BeforeClose => {
                self.unhighlight_all_panes();
            }
            _ => {},
        }
        should_render
    }
    fn render(&mut self, rows: usize, cols: usize) {
        self.render_close_shortcut(cols);
        self.render_tab_shortcut(cols, rows);
        match self.visibility_and_focus {
            VisibilityAndFocus::OnlyLeftSideVisible => self.render_left_side(rows, cols, true),
            VisibilityAndFocus::OnlyRightSideVisible => self.render_right_side(rows, cols, true),
            VisibilityAndFocus::BothSidesVisibleLeftSideFocused => {
                self.render_left_side(rows, cols, true);
                self.render_right_side(rows, cols, false);
            }
            VisibilityAndFocus::BothSidesVisibleRightSideFocused => {
                self.render_left_side(rows, cols, false);
                self.render_right_side(rows, cols, true);
            }
        }
        self.render_focus_boundary(rows, cols);
        self.render_help_line(rows, cols);
    }
}
