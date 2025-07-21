use std::collections::BTreeMap;
use std::time::Instant;
use zellij_tile::prelude::actions::Action;
use zellij_tile::prelude::*;

#[derive(Debug, Default)]
pub struct App {
    own_plugin_id: Option<u32>,
    own_client_id: Option<ClientId>,
    own_tab_index: Option<usize>,
    total_tabs_in_session: Option<usize>,
    grouped_panes: Vec<PaneId>,
    grouped_panes_count: usize,
    all_client_grouped_panes: BTreeMap<ClientId, Vec<PaneId>>,
    mode_info: ModeInfo,
    closing: bool,
    highlighted_at: Option<Instant>,
    baseline_ui_width: usize,
    current_rows: usize,
    current_cols: usize,
    display_area_rows: usize,
    display_area_cols: usize,
    alternate_coordinates: bool,
}

register_plugin!(App);

impl ZellijPlugin for App {
    fn load(&mut self, _configuration: BTreeMap<String, String>) {
        subscribe(&[
            EventType::Key,
            EventType::InterceptedKeyPress,
            EventType::ModeUpdate,
            EventType::PaneUpdate,
            EventType::TabUpdate,
            EventType::Timer,
        ]);

        let plugin_ids = get_plugin_ids();
        self.own_plugin_id = Some(plugin_ids.plugin_id);
        self.own_client_id = Some(plugin_ids.client_id);

        intercept_key_presses();
        set_selectable(false);
    }

    fn update(&mut self, event: Event) -> bool {
        if self.closing {
            return false;
        }
        intercept_key_presses(); // we do this here so that all clients (even those connected after
                                 // load) will have their keys intercepted
        match event {
            Event::ModeUpdate(mode_info) => self.handle_mode_update(mode_info),
            Event::PaneUpdate(pane_manifest) => self.handle_pane_update(pane_manifest),
            Event::TabUpdate(tab_infos) => self.handle_tab_update(tab_infos),
            Event::InterceptedKeyPress(key) => self.handle_key_press(key),
            Event::Timer(_) => self.handle_timer(),
            _ => false,
        }
    }

    fn render(&mut self, rows: usize, cols: usize) {
        self.update_current_size(rows, cols);

        if self.grouped_panes_count == 0 {
            self.render_no_panes_message(rows, cols);
        } else {
            let ui_width = self.calculate_ui_width();
            self.update_baseline_ui_width(ui_width);
            let base_x = cols.saturating_sub(self.baseline_ui_width) / 2;
            let base_y = rows.saturating_sub(8) / 2;
            self.render_header(base_x, base_y);
            self.render_shortcuts(base_x, base_y + 2);
            self.render_controls(base_x, base_y + 7);
        }
    }
}

impl App {
    fn update_current_size(&mut self, new_rows: usize, new_cols: usize) {
        let size_changed = new_rows != self.current_rows || new_cols != self.current_cols;
        self.current_rows = new_rows;
        self.current_cols = new_cols;
        if size_changed {
            self.baseline_ui_width = 0;
        }
    }
    fn update_baseline_ui_width(&mut self, current_ui_width: usize) {
        if current_ui_width > self.baseline_ui_width {
            self.baseline_ui_width = current_ui_width;
        }
    }

    fn calculate_ui_width(&self) -> usize {
        let controls_width = group_controls_length(&self.mode_info);

        let header_width = Self::header_text().0.len();
        let shortcuts_max_width = self.shortcuts_max_width();

        std::cmp::max(
            controls_width,
            std::cmp::max(header_width, shortcuts_max_width),
        )
    }

    fn render_no_panes_message(&self, rows: usize, cols: usize) {
        let message = "PANES SELECTED FOR OTHER CLIENT";
        let message_component = Text::new(message).color_all(2);
        let base_x = cols.saturating_sub(message.len()) / 2;
        let base_y = rows / 2;
        print_text_with_coordinates(message_component, base_x, base_y, None, None);

        let esc_message = "<ESC> - close";
        let esc_message_component = Text::new(esc_message).color_substring(3, "<ESC>");
        let esc_base_x = cols.saturating_sub(esc_message.len()) / 2;
        let esc_base_y = base_y + 2;
        print_text_with_coordinates(esc_message_component, esc_base_x, esc_base_y, None, None);
    }

    fn header_text() -> (&'static str, Text) {
        let header_text = "<ESC> - cancel, <TAB> - move";
        let header_text_component = Text::new(header_text)
            .color_substring(3, "<ESC>")
            .color_substring(3, "<TAB>");
        (header_text, header_text_component)
    }

    fn shortcuts_max_width(&self) -> usize {
        std::cmp::max(
            std::cmp::max(
                self.group_actions_text().0.len(),
                Self::shortcuts_line1_text().0.len(),
            ),
            std::cmp::max(
                Self::shortcuts_line2_text().0.len(),
                Self::shortcuts_line3_text().0.len(),
            ),
        )
    }

    fn group_actions_text(&self) -> (&'static str, Text) {
        let count_text = if self.grouped_panes_count == 1 {
            format!("GROUP ACTIONS ({} SELECTED PANE)", self.grouped_panes_count)
        } else {
            format!(
                "GROUP ACTIONS ({} SELECTED PANES)",
                self.grouped_panes_count
            )
        };

        let component = Text::new(&count_text).color_all(2);
        (Box::leak(count_text.into_boxed_str()), component)
    }

    fn shortcuts_line1_text() -> (&'static str, Text) {
        let text = "<b> - break out, <s> - stack, <c> - close";
        let component = Text::new(text)
            .color_substring(3, "<b>")
            .color_substring(3, "<s>")
            .color_substring(3, "<c>");
        (text, component)
    }

    fn shortcuts_line2_text() -> (&'static str, Text) {
        let text = "<l> - break left, <r> - break right";
        let component = Text::new(text)
            .color_substring(3, "<l>")
            .color_substring(3, "<r>");
        (text, component)
    }

    fn shortcuts_line3_text() -> (&'static str, Text) {
        let text = "<e> - embed, <f> - float";
        let component = Text::new(text)
            .color_substring(3, "<e>")
            .color_substring(3, "<f>");
        (text, component)
    }

    fn handle_mode_update(&mut self, mode_info: ModeInfo) -> bool {
        if self.mode_info != mode_info {
            self.mode_info = mode_info;
            let ui_width = self.calculate_ui_width();
            self.update_baseline_ui_width(ui_width);
            true
        } else {
            false
        }
    }

    fn handle_pane_update(&mut self, pane_manifest: PaneManifest) -> bool {
        let Some(own_client_id) = self.own_client_id else {
            return false;
        };

        self.update_all_client_grouped_panes(&pane_manifest);
        self.update_own_grouped_panes(&pane_manifest, own_client_id);
        self.update_tab_info(&pane_manifest);
        self.total_tabs_in_session = Some(pane_manifest.panes.keys().count());

        true
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

    fn update_all_client_grouped_panes(&mut self, pane_manifest: &PaneManifest) {
        self.all_client_grouped_panes.clear();

        for (_tab_index, pane_infos) in &pane_manifest.panes {
            for pane_info in pane_infos {
                for (client_id, _index_in_pane_group) in &pane_info.index_in_pane_group {
                    let pane_id = if pane_info.is_plugin {
                        PaneId::Plugin(pane_info.id)
                    } else {
                        PaneId::Terminal(pane_info.id)
                    };

                    self.all_client_grouped_panes
                        .entry(*client_id)
                        .or_insert_with(Vec::new)
                        .push(pane_id);
                }
            }
        }
    }

    fn update_own_grouped_panes(&mut self, pane_manifest: &PaneManifest, own_client_id: ClientId) {
        self.grouped_panes.clear();
        let mut count = 0;
        let mut panes_with_index = Vec::new();

        for (_tab_index, pane_infos) in &pane_manifest.panes {
            for pane_info in pane_infos {
                if let Some(index_in_pane_group) = pane_info.index_in_pane_group.get(&own_client_id)
                {
                    let pane_id = if pane_info.is_plugin {
                        PaneId::Plugin(pane_info.id)
                    } else {
                        PaneId::Terminal(pane_info.id)
                    };
                    panes_with_index.push((*index_in_pane_group, pane_id));
                    count += 1;
                }
            }
        }

        panes_with_index.sort_by_key(|(index, _)| *index);

        for (_, pane_id) in panes_with_index {
            self.grouped_panes.push(pane_id);
        }

        if self.all_clients_have_empty_groups() {
            self.close_self();
        }

        let previous_count = self.grouped_panes_count;
        self.grouped_panes_count = count;
        if let Some(own_plugin_id) = self.own_plugin_id {
            if previous_count != count {
                rename_plugin_pane(own_plugin_id, "Multiple Pane Select".to_string());
            }
            if previous_count != 0 && count != 0 && previous_count != count {
                if self.doherty_threshold_elapsed_since_highlight() {
                    self.highlighted_at = Some(Instant::now());
                    highlight_and_unhighlight_panes(vec![PaneId::Plugin(own_plugin_id)], vec![]);
                    set_timeout(0.4);
                }
            }
        }
    }

    fn all_clients_have_empty_groups(&self) -> bool {
        self.all_client_grouped_panes
            .values()
            .all(|panes| panes.is_empty())
    }

    fn doherty_threshold_elapsed_since_highlight(&self) -> bool {
        self.highlighted_at
            .map(|h| h.elapsed() >= std::time::Duration::from_millis(400))
            .unwrap_or(true)
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
            BareKey::Tab => self.next_coordinates(),
            BareKey::Esc => {
                self.ungroup_panes_in_zellij();
                self.close_self();
            },
            _ => return false,
        }
        false
    }
    fn handle_timer(&mut self) -> bool {
        if let Some(own_plugin_id) = self.own_plugin_id {
            if self.doherty_threshold_elapsed_since_highlight() {
                highlight_and_unhighlight_panes(vec![], vec![PaneId::Plugin(own_plugin_id)]);
            }
        }
        false
    }

    fn render_header(&self, base_x: usize, base_y: usize) {
        let header_text = Self::header_text();

        print_text_with_coordinates(header_text.1, base_x, base_y, None, None);
    }

    fn render_shortcuts(&self, base_x: usize, base_y: usize) {
        let mut running_y = base_y;
        print_text_with_coordinates(self.group_actions_text().1, base_x, running_y, None, None);
        running_y += 1;

        print_text_with_coordinates(
            Self::shortcuts_line1_text().1,
            base_x,
            running_y,
            None,
            None,
        );
        running_y += 1;

        print_text_with_coordinates(
            Self::shortcuts_line2_text().1,
            base_x,
            running_y,
            None,
            None,
        );
        running_y += 1;

        print_text_with_coordinates(
            Self::shortcuts_line3_text().1,
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
        F: FnOnce(&[PaneId]),
    {
        let pane_ids = self.grouped_panes.clone();
        action(&pane_ids);
        self.close_self();
    }

    pub fn break_grouped_panes_to_new_tab(&mut self) {
        self.execute_action_and_close(|pane_ids| {
            break_panes_to_new_tab(pane_ids, None, true);
        });
        self.ungroup_panes_in_zellij();
    }

    pub fn stack_grouped_panes(&mut self) {
        self.execute_action_and_close(|pane_ids| {
            stack_panes(pane_ids.to_vec());
        });
        self.ungroup_panes_in_zellij();
    }

    pub fn float_grouped_panes(&mut self) {
        self.execute_action_and_close(|pane_ids| {
            float_multiple_panes(pane_ids.to_vec());
        });
        self.ungroup_panes_in_zellij();
    }

    pub fn embed_grouped_panes(&mut self) {
        self.execute_action_and_close(|pane_ids| {
            embed_multiple_panes(pane_ids.to_vec());
        });
        self.ungroup_panes_in_zellij();
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

        self.close_self();
    }

    pub fn break_grouped_panes_left(&mut self) {
        let Some(own_tab_index) = self.own_tab_index else {
            return;
        };

        let pane_ids = self.grouped_panes.clone();

        if own_tab_index > 0 {
            break_panes_to_tab_with_index(&pane_ids, own_tab_index.saturating_sub(1), true);
        } else {
            break_panes_to_new_tab(&pane_ids, None, true);
        }

        self.close_self();
    }

    pub fn close_grouped_panes(&mut self) {
        self.execute_action_and_close(|pane_ids| {
            close_multiple_panes(pane_ids.to_vec());
        });
    }

    pub fn ungroup_panes_in_zellij(&mut self) {
        let all_grouped_panes: Vec<PaneId> = self
            .all_client_grouped_panes
            .values()
            .flat_map(|panes| panes.iter().cloned())
            .collect();
        let for_all_clients = true;
        group_and_ungroup_panes(vec![], all_grouped_panes, for_all_clients);
    }

    pub fn close_self(&mut self) {
        self.closing = true;
        close_self();
    }
    pub fn next_coordinates(&mut self) {
        let width_30_percent = (self.display_area_cols as f64 * 0.3) as usize;
        let height_30_percent = (self.display_area_rows as f64 * 0.3) as usize;
        let width = std::cmp::max(width_30_percent, 48);
        let height = std::cmp::max(height_30_percent, 10);
        let y_position = self.display_area_rows.saturating_sub(height + 2);
        if let Some(own_plugin_id) = self.own_plugin_id {
            if self.alternate_coordinates {
                let x_position = 2;
                let Some(next_coordinates) = FloatingPaneCoordinates::new(
                    Some(format!("{}", x_position)),
                    Some(format!("{}", y_position)),
                    Some(format!("{}", width)),
                    Some(format!("{}", height)),
                    Some(true),
                ) else {
                    return;
                };
                change_floating_panes_coordinates(vec![(
                    PaneId::Plugin(own_plugin_id),
                    next_coordinates,
                )]);
                self.alternate_coordinates = false;
            } else {
                let x_position = self
                    .display_area_cols
                    .saturating_sub(width)
                    .saturating_sub(2);
                let Some(next_coordinates) = FloatingPaneCoordinates::new(
                    Some(format!("{}", x_position)),
                    Some(format!("{}", y_position)),
                    Some(format!("{}", width)),
                    Some(format!("{}", height)),
                    Some(true),
                ) else {
                    return;
                };
                change_floating_panes_coordinates(vec![(
                    PaneId::Plugin(own_plugin_id),
                    next_coordinates,
                )]);
                self.alternate_coordinates = true;
            }
        }
    }
}

fn render_group_controls(mode_info: &ModeInfo, base_x: usize, base_y: usize) {
    let keymap = mode_info.get_mode_keybinds();
    let (common_modifiers, pane_group_key, group_mark_key) = extract_key_bindings(&keymap);

    let pane_group_bound = pane_group_key != "UNBOUND";
    let group_mark_bound = group_mark_key != "UNBOUND";

    if !pane_group_bound && !group_mark_bound {
        return;
    }

    render_common_modifiers(&common_modifiers, base_x, base_y);

    let mut next_x = base_x + render_common_modifiers(&common_modifiers, base_x, base_y);

    if pane_group_bound {
        next_x = render_toggle_group_ribbon(&pane_group_key, next_x, base_y);
    }

    if group_mark_bound {
        render_follow_focus_ribbon(&group_mark_key, next_x, base_y, mode_info);
    }
}

fn group_controls_length(mode_info: &ModeInfo) -> usize {
    let keymap = mode_info.get_mode_keybinds();
    let (common_modifiers, pane_group_key, group_mark_key) = extract_key_bindings(&keymap);

    let pane_group_bound = pane_group_key != "UNBOUND";
    let group_mark_bound = group_mark_key != "UNBOUND";

    let mut length = 0;

    if !common_modifiers.is_empty() {
        let modifiers_text = format!(
            "{} + ",
            common_modifiers
                .iter()
                .map(|m| m.to_string())
                .collect::<Vec<_>>()
                .join(" ")
        );
        length += modifiers_text.chars().count();
    }

    if pane_group_bound {
        let toggle_text = format!("<{}> Toggle", pane_group_key);
        length += toggle_text.chars().count() + 4;
    }

    if group_mark_bound {
        let follow_text = format!("<{}> Follow Focus", group_mark_key);
        length += follow_text.chars().count() + 4;
    }

    length
}

fn extract_key_bindings(
    keymap: &[(KeyWithModifier, Vec<Action>)],
) -> (Vec<KeyModifier>, String, String) {
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

fn format_key_without_modifiers(
    keys: &[KeyWithModifier],
    common_modifiers: &[KeyModifier],
) -> String {
    keys.first()
        .map(|key| format!("{}", key.strip_common_modifiers(&common_modifiers.to_vec())))
        .unwrap_or_else(|| "UNBOUND".to_string())
}

fn render_common_modifiers(
    common_modifiers: &[KeyModifier],
    base_x: usize,
    base_y: usize,
) -> usize {
    if !common_modifiers.is_empty() {
        let modifiers_text = format!(
            "{} + ",
            common_modifiers
                .iter()
                .map(|m| m.to_string())
                .collect::<Vec<_>>()
                .join(" ")
        );

        print_text_with_coordinates(
            Text::new(&modifiers_text).color_all(0),
            base_x,
            base_y,
            None,
            None,
        );

        modifiers_text.chars().count()
    } else {
        0
    }
}

fn get_key_for_action(
    keymap: &[(KeyWithModifier, Vec<Action>)],
    target_action: &[Action],
) -> Vec<KeyWithModifier> {
    keymap
        .iter()
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

fn render_toggle_group_ribbon(pane_group_key: &str, base_x: usize, base_y: usize) -> usize {
    let toggle_text = format!("<{}> Toggle", pane_group_key);
    let key_highlight = format!("{}", pane_group_key);

    print_ribbon_with_coordinates(
        Text::new(&toggle_text).color_substring(0, &key_highlight),
        base_x,
        base_y,
        None,
        None,
    );

    base_x + toggle_text.len() + 4
}
