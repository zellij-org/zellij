use std::collections::BTreeMap;
use zellij_tile::prelude::actions::Action;
use zellij_tile::prelude::*;
use zellij_utils::input::context_menu::context_menu_shortcut;

const MAX_MENU_HEIGHT: usize = 200;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ItemState {
    Enabled,
    Disabled,
    Hidden,
}

#[derive(Default)]
struct ContextMenuPlugin {
    context: Option<ContextMenuContext>,
    entries: Vec<ContextMenuEntry>,
    mode_info: Option<ModeInfo>,
    menu: Option<MenuList>,
    item_entry_indices: Vec<Option<usize>>,
    requested_size: Option<(usize, usize)>,
}

register_plugin!(ContextMenuPlugin);

fn item_state(actions: &[Action], context: &ContextMenuContext) -> ItemState {
    let has_pane_target = context.target_pane_id().is_some();
    let has_tab_target = context.target_tab_id().is_some();
    let mut state = ItemState::Enabled;
    for action in actions {
        if action.has_missing_target() {
            if action.targets_pane() && !has_pane_target {
                return ItemState::Hidden;
            }
            if action.targets_tab() && !has_tab_target {
                return ItemState::Hidden;
            }
        }
        match action {
            Action::TogglePanePinnedByPaneId { pane_id: None } if !context.pane_is_floating => {
                return ItemState::Hidden;
            },
            Action::MoveTabByTabId {
                id: None,
                direction: Direction::Left,
            } if context.tab_index == Some(0) => {
                state = ItemState::Disabled;
            },
            Action::MoveTabByTabId {
                id: None,
                direction: Direction::Right,
            } if context
                .tab_index
                .map(|tab_index| tab_index + 1 >= context.tab_count)
                .unwrap_or(false) =>
            {
                state = ItemState::Disabled;
            },
            _ => {},
        }
    }
    state
}

fn shortcut_for(mode_info: &ModeInfo, wanted: &[Action]) -> Option<String> {
    context_menu_shortcut(
        &mode_info.keybinds,
        mode_info.base_mode.unwrap_or(InputMode::Normal),
        wanted,
    )
}

impl ContextMenuPlugin {
    fn rebuild(&mut self) {
        let Some(context) = self.context.as_ref() else {
            return;
        };
        let mut items = vec![];
        let mut item_entry_indices = vec![];
        for (entry_index, entry) in self.entries.iter().enumerate() {
            match entry {
                ContextMenuEntry::Separator => {
                    let previous_is_separator = items
                        .last()
                        .map(|item: &MenuItem| item.is_separator())
                        .unwrap_or(true);
                    if !previous_is_separator {
                        items.push(MenuItem::separator());
                        item_entry_indices.push(None);
                    }
                },
                ContextMenuEntry::Item { label, actions } => {
                    let state = item_state(actions, context);
                    if state == ItemState::Hidden {
                        continue;
                    }
                    let mut item = MenuItem::new(label.clone());
                    if let Some(shortcut) = self
                        .mode_info
                        .as_ref()
                        .and_then(|mode_info| shortcut_for(mode_info, actions))
                    {
                        item = item.shortcut(shortcut);
                    }
                    if state == ItemState::Disabled {
                        item = item.disabled();
                    }
                    items.push(item);
                    item_entry_indices.push(Some(entry_index));
                },
            }
        }
        while items
            .last()
            .map(|item| item.is_separator())
            .unwrap_or(false)
        {
            items.pop();
            item_entry_indices.pop();
        }
        let highlighted = self.menu.as_ref().and_then(|menu| menu.highlighted_index());
        let mut menu = MenuList::new(items).with_border().focused();
        if let Some(highlighted) = highlighted {
            menu.set_highlighted(Some(highlighted));
        }
        let size = (menu.natural_width(), menu.height_for(MAX_MENU_HEIGHT));
        if self.requested_size != Some(size) {
            self.requested_size = Some(size);
            set_popup_size(size.0, size.1);
        }
        self.menu = Some(menu);
        self.item_entry_indices = item_entry_indices;
    }
    fn run_item(&mut self, index: usize) {
        if let Some(Some(entry_index)) = self.item_entry_indices.get(index) {
            run_context_menu_item(*entry_index);
        }
        close_self();
    }
    fn handle_response(&mut self, response: UiResponse) -> bool {
        match response {
            UiResponse::Submitted(UiValue::Choice { index, .. }) => {
                self.run_item(index);
                false
            },
            UiResponse::Cancelled => {
                close_self();
                false
            },
            UiResponse::NotHandled => false,
            _ => true,
        }
    }
    fn handle_key(&mut self, key: KeyWithModifier) -> bool {
        let Some(menu) = self.menu.as_mut() else {
            if key.is_key_without_modifier(BareKey::Esc) {
                close_self();
            }
            return false;
        };
        let response = menu.handle_key(&key);
        self.handle_response(response)
    }
    fn handle_mouse(&mut self, mouse: Mouse) -> bool {
        let Some(menu) = self.menu.as_mut() else {
            return false;
        };
        match mouse {
            Mouse::Release(..) | Mouse::RightClick(..) => false,
            mouse => {
                let response = menu.handle_mouse(mouse);
                self.handle_response(response)
            },
        }
    }
}

impl ZellijPlugin for ContextMenuPlugin {
    fn load(&mut self, _configuration: BTreeMap<String, String>) {
        subscribe(&[
            EventType::Key,
            EventType::Mouse,
            EventType::Timer,
            EventType::ModeUpdate,
            EventType::ContextMenu,
        ]);
    }
    fn update(&mut self, event: Event) -> bool {
        match event {
            Event::ContextMenu(context, entries) => {
                self.context = Some(context);
                self.entries = entries;
                self.rebuild();
                true
            },
            Event::ModeUpdate(mode_info) => {
                self.mode_info = Some(mode_info);
                self.rebuild();
                true
            },
            Event::Key(key) => self.handle_key(key),
            Event::Mouse(mouse) => self.handle_mouse(mouse),
            Event::Timer(_) => self
                .menu
                .as_mut()
                .map(|menu| menu.handle_timer())
                .unwrap_or(false),
            _ => false,
        }
    }
    fn render(&mut self, rows: usize, cols: usize) {
        if let Some(menu) = self.menu.as_mut() {
            menu.render(0, 0, cols, rows);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zellij_utils::input::layout::{PluginAlias, RunPluginOrAlias};

    fn key(c: char) -> KeyWithModifier {
        KeyWithModifier::new(BareKey::Char(c))
    }

    fn ctrl(c: char) -> KeyWithModifier {
        KeyWithModifier::new(BareKey::Char(c)).with_ctrl_modifier()
    }

    fn alt(c: char) -> KeyWithModifier {
        KeyWithModifier::new(BareKey::Char(c)).with_alt_modifier()
    }

    fn switch_to(input_mode: InputMode) -> Action {
        Action::SwitchToMode { input_mode }
    }

    fn plugin_launch(name: &str, configuration: &[(&str, &str)], should_float: bool) -> Action {
        let configuration: BTreeMap<String, String> = configuration
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        Action::LaunchOrFocusPlugin {
            plugin: RunPluginOrAlias::Alias(PluginAlias::new(
                name,
                &Some(configuration),
                None,
            )),
            should_float,
            move_to_focused_tab: true,
            should_open_in_place: false,
            close_replaced_pane: false,
            skip_cache: false,
            tab_id: None,
        }
    }

    fn mode_info(base_mode: InputMode) -> ModeInfo {
        let back = switch_to(base_mode);
        let normal = vec![
            (ctrl('p'), vec![switch_to(InputMode::Pane)]),
            (ctrl('t'), vec![switch_to(InputMode::Tab)]),
            (ctrl('o'), vec![switch_to(InputMode::Session)]),
            (
                alt('n'),
                vec![Action::NewPane {
                    direction: Some(Direction::Right),
                    pane_name: None,
                    start_suppressed: false,
                }],
            ),
            (
                alt('i'),
                vec![Action::MoveTab {
                    direction: Direction::Left,
                }],
            ),
        ];
        let pane = vec![
            (key('f'), vec![Action::ToggleFocusFullscreen, back.clone()]),
            (key('x'), vec![Action::CloseFocus, back.clone()]),
            (
                key('c'),
                vec![
                    switch_to(InputMode::RenamePane),
                    Action::PaneNameInput { input: vec![0] },
                ],
            ),
        ];
        let tab = vec![
            (
                key('r'),
                vec![
                    switch_to(InputMode::RenameTab),
                    Action::TabNameInput { input: vec![0] },
                ],
            ),
            (key('x'), vec![Action::CloseTab, back.clone()]),
        ];
        let session = vec![
            (key('d'), vec![Action::Detach]),
            (
                key('c'),
                vec![
                    plugin_launch("configuration", &[], true),
                    back.clone(),
                ],
            ),
        ];
        ModeInfo {
            mode: base_mode,
            base_mode: Some(base_mode),
            keybinds: vec![
                (base_mode, normal),
                (InputMode::Pane, pane),
                (InputMode::Tab, tab),
                (InputMode::Session, session),
            ],
            ..Default::default()
        }
    }

    #[test]
    fn by_id_pane_actions_match_their_focused_pane_bindings() {
        let mode_info = mode_info(InputMode::Normal);
        assert_eq!(
            shortcut_for(
                &mode_info,
                &[Action::ToggleFocusFullscreenByPaneId { pane_id: None }]
            ),
            Some("Ctrl p, f".to_owned())
        );
        assert_eq!(
            shortcut_for(&mode_info, &[Action::CloseFocusByPaneId { pane_id: None }]),
            Some("Ctrl p, x".to_owned())
        );
    }

    #[test]
    fn by_id_tab_actions_match_their_focused_tab_bindings() {
        let mode_info = mode_info(InputMode::Normal);
        assert_eq!(
            shortcut_for(&mode_info, &[Action::CloseTabById { id: None }]),
            Some("Ctrl t, x".to_owned())
        );
        assert_eq!(
            shortcut_for(
                &mode_info,
                &[Action::MoveTabByTabId {
                    id: None,
                    direction: Direction::Left
                }]
            ),
            Some("Alt i".to_owned())
        );
        assert_eq!(
            shortcut_for(
                &mode_info,
                &[Action::MoveTabByTabId {
                    id: None,
                    direction: Direction::Right
                }]
            ),
            None
        );
    }

    #[test]
    fn rename_actions_match_their_key_sequences() {
        let mode_info = mode_info(InputMode::Normal);
        assert_eq!(
            shortcut_for(
                &mode_info,
                &[Action::StartRenamePaneByPaneId { pane_id: None }]
            ),
            Some("Ctrl p, c".to_owned())
        );
        assert_eq!(
            shortcut_for(&mode_info, &[Action::StartRenameTabByTabId { id: None }]),
            Some("Ctrl t, r".to_owned())
        );
    }

    #[test]
    fn plugin_items_match_by_location_only() {
        let mode_info = mode_info(InputMode::Normal);
        assert_eq!(
            shortcut_for(
                &mode_info,
                &[plugin_launch("configuration", &[("x", "y")], false)]
            ),
            Some("Ctrl o, c".to_owned())
        );
        assert_eq!(
            shortcut_for(&mode_info, &[plugin_launch("session-manager", &[], true)]),
            None
        );
    }

    #[test]
    fn new_pane_without_a_direction_matches_any_new_pane_binding() {
        let mode_info = mode_info(InputMode::Normal);
        let new_pane = |direction| Action::NewPane {
            direction,
            pane_name: None,
            start_suppressed: false,
        };
        assert_eq!(
            shortcut_for(&mode_info, &[new_pane(None)]),
            Some("Alt n".to_owned())
        );
        assert_eq!(shortcut_for(&mode_info, &[new_pane(Some(Direction::Down))]), None);
    }

    #[test]
    fn shortcuts_follow_mode_switches_from_the_base_mode() {
        let mut mode_info = mode_info(InputMode::Locked);
        mode_info.mode = InputMode::Pane;
        assert_eq!(
            shortcut_for(&mode_info, &[Action::Detach]),
            Some("Ctrl o, d".to_owned())
        );
        assert_eq!(
            shortcut_for(
                &mode_info,
                &[Action::ToggleFocusFullscreenByPaneId { pane_id: None }]
            ),
            Some("Ctrl p, f".to_owned())
        );
    }

    #[test]
    fn the_tmux_mode_is_only_used_when_no_other_mode_has_the_key() {
        let mut mode_info = mode_info(InputMode::Normal);
        mode_info.keybinds[0]
            .1
            .push((ctrl('b'), vec![switch_to(InputMode::Tmux)]));
        mode_info.keybinds.push((
            InputMode::Tmux,
            vec![
                (
                    key('x'),
                    vec![Action::CloseFocus, switch_to(InputMode::Normal)],
                ),
                (
                    key('z'),
                    vec![Action::TogglePaneInGroup, switch_to(InputMode::Normal)],
                ),
            ],
        ));
        assert_eq!(
            shortcut_for(&mode_info, &[Action::CloseFocusByPaneId { pane_id: None }]),
            Some("Ctrl p, x".to_owned())
        );
        assert_eq!(
            shortcut_for(
                &mode_info,
                &[Action::TogglePaneInGroupByPaneId { pane_id: None }]
            ),
            Some("Ctrl b, z".to_owned())
        );
    }

    #[test]
    fn actions_without_a_binding_show_nothing() {
        let mode_info = mode_info(InputMode::Normal);
        assert_eq!(
            shortcut_for(
                &mode_info,
                &[Action::TogglePaneInGroupByPaneId { pane_id: None }]
            ),
            None
        );
        assert_eq!(shortcut_for(&mode_info, &[]), None);
    }
}
