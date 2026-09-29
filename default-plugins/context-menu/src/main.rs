use std::collections::BTreeMap;
use zellij_tile::prelude::actions::Action;
use zellij_tile::prelude::*;

const MAX_SHORTCUT_DEPTH: usize = 4;
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

fn binding_matches(binding: &[Action], wanted: &[Action], base_mode: InputMode) -> bool {
    if wanted.is_empty() {
        return false;
    }
    if binding == wanted {
        return true;
    }
    binding.len() == wanted.len() + 1
        && &binding[..wanted.len()] == wanted
        && matches!(
            binding.last(),
            Some(Action::SwitchToMode { input_mode }) if *input_mode == base_mode
        )
}

fn shortcut_for(mode_info: &ModeInfo, wanted: &[Action]) -> Option<String> {
    let base_mode = mode_info.base_mode.unwrap_or(InputMode::Normal);
    let start_mode = mode_info.mode;
    let mode_binds = |mode: InputMode| -> Vec<(KeyWithModifier, Vec<Action>)> {
        let mut binds = mode_info.get_keybinds_for_mode(mode);
        binds.sort_by(|(a, _), (b, _)| a.cmp(b));
        binds
    };
    let mut visited = vec![start_mode];
    let mut queue: Vec<(InputMode, Vec<KeyWithModifier>)> = vec![(start_mode, vec![])];
    for _ in 0..MAX_SHORTCUT_DEPTH {
        for (mode, path) in &queue {
            for (key, actions) in mode_binds(*mode) {
                if binding_matches(&actions, wanted, base_mode) {
                    let mut shortcut = path.clone();
                    shortcut.push(key);
                    return Some(
                        shortcut
                            .iter()
                            .map(|key| key.to_string())
                            .collect::<Vec<_>>()
                            .join(", "),
                    );
                }
            }
        }
        let mut next_queue = vec![];
        for (mode, path) in &queue {
            for (key, actions) in mode_binds(*mode) {
                if let [Action::SwitchToMode {
                    input_mode: next_mode,
                }] = actions.as_slice()
                {
                    if !visited.contains(next_mode) {
                        visited.push(*next_mode);
                        let mut next_path = path.clone();
                        next_path.push(key.clone());
                        next_queue.push((*next_mode, next_path));
                    }
                }
            }
        }
        if next_queue.is_empty() {
            return None;
        }
        queue = next_queue;
    }
    None
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
