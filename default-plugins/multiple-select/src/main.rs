use std::collections::BTreeMap;
use zellij_tile::prelude::*;
use zellij_tile::prelude::actions::Action;

#[derive(Debug, Default)]
pub struct App {
    own_plugin_id: Option<u32>,
    own_client_id: Option<ClientId>,
    own_tab_index: Option<usize>,
    total_tabs_in_session: Option<usize>,
    grouped_panes: Vec<PaneId>,
    grouped_panes_count: usize,
    mode_info: ModeInfo,
}

register_plugin!(App);

impl ZellijPlugin for App {
    fn load(&mut self, _configuration: BTreeMap<String, String>) {
        subscribe(&[
            EventType::Key,
            EventType::InterceptedKeyPress,
            EventType::ModeUpdate,
            EventType::PaneUpdate,
            EventType::BeforeClose,
        ]);
        
        let plugin_ids = get_plugin_ids();
        self.own_plugin_id = Some(plugin_ids.plugin_id);
        self.own_client_id = Some(plugin_ids.client_id);
        
        rename_plugin_pane(plugin_ids.plugin_id, "Multiple Select");
        intercept_key_presses();
        set_selectable(false);
    }

    fn update(&mut self, event: Event) -> bool {
        match event {
            Event::ModeUpdate(mode_info) => self.handle_mode_update(mode_info),
            Event::PaneUpdate(pane_manifest) => self.handle_pane_update(pane_manifest),
            Event::InterceptedKeyPress(key) => self.handle_key_press(key),
            Event::BeforeClose => {
                clear_key_presses_intercepts();
                false
            }
            _ => false,
        }
    }

    fn render(&mut self, rows: usize, cols: usize) {
        let base_x = cols.saturating_sub(group_controls_length(&self.mode_info)) / 2;
        let base_y = rows.saturating_sub(8) / 2;
        self.render_header(base_x, base_y);
        self.render_shortcuts(base_x, base_y + 2);
        self.render_controls(base_x, base_y + 7);
    }
}

impl App {
    fn handle_mode_update(&mut self, mode_info: ModeInfo) -> bool {
        if self.mode_info != mode_info {
            self.mode_info = mode_info;
            true
        } else {
            false
        }
    }

    fn handle_pane_update(&mut self, pane_manifest: PaneManifest) -> bool {
        let Some(own_client_id) = self.own_client_id else {
            return false;
        };

        self.update_grouped_panes(&pane_manifest, own_client_id);
        self.update_tab_info(&pane_manifest);
        self.total_tabs_in_session = Some(pane_manifest.panes.keys().count());
        
        true
    }

    fn update_grouped_panes(&mut self, pane_manifest: &PaneManifest, own_client_id: ClientId) {
        self.grouped_panes.clear();
        let mut count = 0;

        for (_tab_index, pane_infos) in &pane_manifest.panes {
            for pane_info in pane_infos {
                if pane_info.index_in_pane_group.get(&own_client_id).is_some() {
                    let pane_id = if pane_info.is_plugin {
                        PaneId::Plugin(pane_info.id)
                    } else {
                        PaneId::Terminal(pane_info.id)
                    };
                    self.grouped_panes.push(pane_id);
                    count += 1;
                }
            }
        }
        if count == 0 {
            close_self();
        }
        
        self.grouped_panes_count = count;
    }

    fn update_tab_info(&mut self, pane_manifest: &PaneManifest) {
        for (tab_index, pane_infos) in &pane_manifest.panes {
            for pane_info in pane_infos {
                if pane_info.is_plugin && Some(pane_info.id) == self.own_plugin_id {
                    self.own_tab_index = Some(*tab_index);
                    return;
                }
            }
        }
    }

    fn handle_key_press(&mut self, key: KeyWithModifier) -> bool {
        if !key.has_no_modifiers() {
            return false;
        }

        match key.bare_key {
            BareKey::Char('b') => self.break_grouped_panes_to_new_tab(),
            BareKey::Char('s') => self.stack_grouped_panes(),
            BareKey::Char('f') => self.float_grouped_panes(),
            BareKey::Char('e') => self.embed_grouped_panes(),
            BareKey::Char('r') => self.break_grouped_panes_right(),
            BareKey::Char('l') => self.break_grouped_panes_left(),
            BareKey::Char('c') => self.close_grouped_panes(),
            BareKey::Esc => {
                self.ungroup_panes_in_zellij(&self.grouped_panes.clone());
                close_self();
            }
            _ => return false,
        }
        false
    }

    fn render_header(&self, base_x: usize, base_y: usize) {
        let header_text = format!(
            "{} SELECTED PANES (<ESC> - cancel)", 
            self.grouped_panes_count
        );
        
        print_text_with_coordinates(
            Text::new(header_text).color_all(0).color_substring(3, "<ESC>"),
            base_x, base_y, None, None
        );
    }

    fn render_shortcuts(&self, base_x: usize, base_y: usize) {
        let mut running_y = base_y;
        print_text_with_coordinates(
            Text::new("GROUP ACTIONS").color_all(1),
            base_x, running_y, None, None
        );
        running_y += 1;

        print_text_with_coordinates(
            Text::new("<b> - break out, <s> - stack, <c> - close")
            .color_substring(3, "<b>")
            .color_substring(3, "<s>")
            .color_substring(3, "<c>"),
            base_x,
            running_y,
            None,
            None,
        );
        running_y += 1;

        print_text_with_coordinates(
            Text::new("<r> - break right, <l> - break left")
            .color_substring(3, "<r>")
            .color_substring(3, "<l>"),
            base_x,
            running_y,
            None,
            None,
        );
        running_y += 1;

        print_text_with_coordinates(
            Text::new("<e> - embed, <f> - float")
            .color_substring(3, "<e>")
            .color_substring(3, "<f>"),
            base_x,
            running_y,
            None,
            None,
        );
    }

    fn render_controls(&self, base_x: usize, base_y: usize) {
        render_group_controls(&self.mode_info, base_x, base_y);
    }

    fn execute_action_and_close<F>(&mut self, action: F) 
    where 
        F: FnOnce(&[PaneId])
    {
        let pane_ids = self.grouped_panes.clone();
        action(&pane_ids);
        close_self();
    }

    pub fn break_grouped_panes_to_new_tab(&mut self) {
        self.execute_action_and_close(|pane_ids| {
            break_panes_to_new_tab(pane_ids, None, true);
        });
        self.ungroup_panes_in_zellij(&self.grouped_panes.clone());
    }

    pub fn stack_grouped_panes(&mut self) {
        self.execute_action_and_close(|pane_ids| {
            stack_panes(pane_ids.to_vec());
        });
        self.ungroup_panes_in_zellij(&self.grouped_panes.clone());
    }

    pub fn float_grouped_panes(&mut self) {
        self.execute_action_and_close(|pane_ids| {
            float_multiple_panes(pane_ids.to_vec());
        });
        self.ungroup_panes_in_zellij(&self.grouped_panes.clone());
    }

    pub fn embed_grouped_panes(&mut self) {
        self.execute_action_and_close(|pane_ids| {
            embed_multiple_panes(pane_ids.to_vec());
        });
        self.ungroup_panes_in_zellij(&self.grouped_panes.clone());
    }

    pub fn break_grouped_panes_right(&mut self) {
        let Some(own_tab_index) = self.own_tab_index else {
            return;
        };

        let pane_ids = self.grouped_panes.clone();
        
        if Some(own_tab_index + 1) < self.total_tabs_in_session {
            break_panes_to_tab_with_index(&pane_ids, own_tab_index + 1, true);
        } else {
            break_panes_to_new_tab(&pane_ids, None, true);
        }
        
        close_self();
    }

    pub fn break_grouped_panes_left(&mut self) {
        let Some(own_tab_index) = self.own_tab_index else {
            return;
        };

        let pane_ids = self.grouped_panes.clone();
        
        if own_tab_index > 0 {
            break_panes_to_tab_with_index(&pane_ids, own_tab_index - 1, true);
        } else {
            break_panes_to_new_tab(&pane_ids, None, true);
        }
        
        close_self();
    }

    pub fn close_grouped_panes(&mut self) {
        self.execute_action_and_close(|pane_ids| {
            close_multiple_panes(pane_ids.to_vec());
        });
    }

    pub fn ungroup_panes_in_zellij(&mut self, pane_ids: &[PaneId]) {
        group_and_ungroup_panes(vec![], pane_ids.to_vec());
    }
}

fn render_group_controls(
    mode_info: &ModeInfo,
    base_x: usize,
    base_y: usize,
) {
    let keymap = mode_info.get_mode_keybinds();
    let (common_modifiers, pane_group_key, group_mark_key) = extract_key_bindings(&keymap);
    
    render_common_modifiers(&common_modifiers, base_x, base_y);
    let next_x = render_toggle_group_ribbon(&pane_group_key, base_x, base_y, &common_modifiers);
    render_follow_focus_ribbon(&group_mark_key, next_x, base_y, mode_info);
}

fn group_controls_length(
    mode_info: &ModeInfo,
) -> usize {
    let keymap = mode_info.get_mode_keybinds();
    let (common_modifiers, pane_group_key, group_mark_key) = extract_key_bindings(&keymap);
    common_modifiers_length(&common_modifiers) +
    toggle_group_ribbon_length(&pane_group_key) +
    follow_focus_length(&group_mark_key)
}

fn extract_key_bindings(keymap: &[(KeyWithModifier, Vec<Action>)]) -> (Vec<KeyModifier>, String, String) {
    let pane_group_keys = get_key_for_action(keymap, &[Action::TogglePaneInGroup]);
    let group_mark_keys = get_key_for_action(keymap, &[Action::ToggleGroupMarking]);
    
    let key_refs: Vec<&KeyWithModifier> = [pane_group_keys.first(), group_mark_keys.first()]
        .into_iter()
        .flatten()
        .collect();
    
    let common_modifiers = get_common_modifiers(key_refs);
    
    let pane_group_key = format_key_without_modifiers(&pane_group_keys, &common_modifiers);
    let group_mark_key = format_key_without_modifiers(&group_mark_keys, &common_modifiers);
    
    (common_modifiers, pane_group_key, group_mark_key)
}

fn format_key_without_modifiers(keys: &[KeyWithModifier], common_modifiers: &[KeyModifier]) -> String {
    keys.first()
        .map(|key| format!("{}", key.strip_common_modifiers(&common_modifiers.to_vec())))
        .unwrap_or_else(|| "UNBOUND".to_string())
}

fn render_common_modifiers(common_modifiers: &[KeyModifier], base_x: usize, base_y: usize) {
    if !common_modifiers.is_empty() {
        let modifiers_text = format!(
            "{} +", 
            common_modifiers.iter()
                .map(|m| m.to_string())
                .collect::<Vec<_>>()
                .join(" ")
        );
        
        print_text_with_coordinates(
            Text::new(&modifiers_text).color_all(0),
            base_x, base_y, None, None
        );
    }
}

fn common_modifiers_length(common_modifiers: &[KeyModifier]) -> usize {
    if !common_modifiers.is_empty() {
        let modifiers_text = format!(
            "{} +", 
            common_modifiers.iter()
                .map(|m| m.to_string())
                .collect::<Vec<_>>()
                .join(" ")
        );
        modifiers_text.chars().count()
    } else {
        0
    }
}

fn render_toggle_group_ribbon(
    pane_group_key: &str, 
    base_x: usize, 
    base_y: usize, 
    common_modifiers: &[KeyModifier]
) -> usize {
    let modifiers_width = if common_modifiers.is_empty() {
        0
    } else {
        common_modifiers.iter()
            .map(|m| m.to_string().len())
            .sum::<usize>() + common_modifiers.len() + 2 // spaces and " +"
    };
    
    let toggle_text = format!("<{}> Toggle", pane_group_key);
    let key_highlight = format!("{}", pane_group_key);
    
    print_ribbon_with_coordinates(
        Text::new(&toggle_text).color_substring(0, &key_highlight),
        base_x + modifiers_width,
        base_y,
        None,
        None
    );
    
    base_x + modifiers_width + toggle_text.len() + 4
}

fn toggle_group_ribbon_length(
    pane_group_key: &str, 
) -> usize {
    let toggle_text = format!("<{}> Toggle", pane_group_key);
    toggle_text.chars().count() + 4
}

fn render_follow_focus_ribbon(
    group_mark_key: &str,
    x_position: usize,
    base_y: usize,
    mode_info: &ModeInfo,
) {
    let follow_text = format!("<{}> Follow Focus", group_mark_key);
    let key_highlight = format!("{}", group_mark_key);
    
    let mut ribbon = Text::new(&follow_text).color_substring(0, &key_highlight);
    
    if mode_info.currently_marking_pane_group.unwrap_or(false) {
        ribbon = ribbon.selected();
    }
    
    print_ribbon_with_coordinates(ribbon, x_position, base_y, None, None);
}

fn follow_focus_length(
    group_mark_key: &str,
) -> usize {
    let follow_text = format!("<{}> Follow Focus", group_mark_key);
    follow_text.chars().count() + 4
}

fn get_key_for_action(keymap: &[(KeyWithModifier, Vec<Action>)], target_action: &[Action]) -> Vec<KeyWithModifier> {
    keymap.iter()
        .find_map(|(key, actions)| {
            if actions.first() == target_action.first() {
                Some(key.clone())
            } else {
                None
            }
        })
        .map(|key| vec![key])
        .unwrap_or_default()
}

fn get_common_modifiers(keys: Vec<&KeyWithModifier>) -> Vec<KeyModifier> {
    if keys.is_empty() {
        return vec![];
    }
    
    let mut common = keys[0].key_modifiers.clone();
    
    for key in keys.iter().skip(1) {
        common = common.intersection(&key.key_modifiers).cloned().collect();
    }
    
    common.into_iter().collect()
}
