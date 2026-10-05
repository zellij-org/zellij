use crate::data::{
    ContextMenuAction, ContextMenuEntry, ContextMenuKind, InputMode, KeyWithModifier, KeybindsVec,
};
use crate::input::actions::Action;
use serde::{Deserialize, Serialize};

const MAX_SHORTCUT_DEPTH: usize = 4;

fn actions_match(binding: &[Action], wanted: &[Action]) -> bool {
    binding.len() == wanted.len()
        && wanted
            .iter()
            .zip(binding.iter())
            .all(|(wanted, bound)| wanted.matches_binding_for_shortcut(bound))
}

fn binding_matches(binding: &[Action], wanted: &[Action], base_mode: InputMode) -> bool {
    if wanted.is_empty() {
        return false;
    }
    if actions_match(binding, wanted) {
        return true;
    }
    binding.len() == wanted.len() + 1
        && actions_match(&binding[..wanted.len()], wanted)
        && matches!(
            binding.last(),
            Some(Action::SwitchToMode { input_mode }) if *input_mode == base_mode
        )
}

pub fn context_menu_shortcut(
    keybinds: &KeybindsVec,
    base_mode: InputMode,
    actions: &[ContextMenuAction],
) -> Option<String> {
    let wanted: Vec<Action> = actions
        .iter()
        .flat_map(|action| action.keybinding_equivalent())
        .collect();
    if wanted.is_empty() {
        return None;
    }
    let mode_binds = |mode: InputMode| -> Vec<(KeyWithModifier, Vec<Action>)> {
        let mut binds = keybinds
            .iter()
            .find(|(bind_mode, _)| *bind_mode == mode)
            .map(|(_, binds)| binds.clone())
            .unwrap_or_default();
        binds.sort_by(|(a, _), (b, _)| a.cmp(b));
        binds
    };
    let mut visited = vec![base_mode];
    let mut queue: Vec<(InputMode, Vec<KeyWithModifier>)> = vec![(base_mode, vec![])];
    for _ in 0..MAX_SHORTCUT_DEPTH {
        for (mode, path) in &queue {
            for (key, bound) in mode_binds(*mode) {
                if binding_matches(&bound, &wanted, base_mode) {
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
            for (key, bound) in mode_binds(*mode) {
                if let [Action::SwitchToMode {
                    input_mode: next_mode,
                }] = bound.as_slice()
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
        next_queue.sort_by_key(|(mode, _)| *mode == InputMode::Tmux);
        queue = next_queue;
    }
    None
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ContextMenuConfig {
    pub pane: Vec<ContextMenuEntry>,
    pub tab: Vec<ContextMenuEntry>,
    pub bar: Vec<ContextMenuEntry>,
    pub common: Vec<ContextMenuEntry>,
}

pub const CONTEXT_MENU_SECTIONS: [&str; 4] = ["pane", "tab", "bar", "common"];

impl ContextMenuConfig {
    pub fn section(&self, name: &str) -> Option<&Vec<ContextMenuEntry>> {
        match name {
            "pane" => Some(&self.pane),
            "tab" => Some(&self.tab),
            "bar" => Some(&self.bar),
            "common" => Some(&self.common),
            _ => None,
        }
    }
    pub fn section_mut(&mut self, name: &str) -> Option<&mut Vec<ContextMenuEntry>> {
        match name {
            "pane" => Some(&mut self.pane),
            "tab" => Some(&mut self.tab),
            "bar" => Some(&mut self.bar),
            "common" => Some(&mut self.common),
            _ => None,
        }
    }
    pub fn merge(&self, other: ContextMenuConfig) -> ContextMenuConfig {
        let pick = |own: &Vec<ContextMenuEntry>, theirs: Vec<ContextMenuEntry>| {
            if theirs.is_empty() {
                own.clone()
            } else {
                theirs
            }
        };
        ContextMenuConfig {
            pane: pick(&self.pane, other.pane),
            tab: pick(&self.tab, other.tab),
            bar: pick(&self.bar, other.bar),
            common: pick(&self.common, other.common),
        }
    }
    pub fn entries_for(&self, kind: ContextMenuKind) -> Vec<ContextMenuEntry> {
        let section = match kind {
            ContextMenuKind::Pane | ContextMenuKind::PaneFrame => &self.pane,
            ContextMenuKind::Tab => &self.tab,
            ContextMenuKind::Bar => &self.bar,
        };
        let mut entries = section.clone();
        entries.push(ContextMenuEntry::Separator);
        entries.extend(self.common.iter().cloned());
        normalize_separators(entries)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MenuPlacement {
    After(String),
    Before(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum MenuStatement<T> {
    Entry {
        entry: T,
        placement: Option<MenuPlacement>,
    },
    Remove(String),
}

impl<T> MenuStatement<T> {
    pub fn plain(entry: T) -> Self {
        MenuStatement::Entry {
            entry,
            placement: None,
        }
    }
}

fn label_position<T>(
    list: &[T],
    label: &str,
    label_of: &impl Fn(&T) -> Option<String>,
) -> Option<usize> {
    list.iter()
        .position(|item| label_of(item).as_deref() == Some(label))
}

pub fn merge_menu_statements<T: Clone>(
    base: &[T],
    statements: Vec<MenuStatement<T>>,
    label_of: impl Fn(&T) -> Option<String>,
    replace_by_label: bool,
) -> Vec<T> {
    let mut merged = base.to_vec();
    for statement in statements {
        match statement {
            MenuStatement::Remove(label) => {
                if let Some(position) = label_position(&merged, &label, &label_of) {
                    merged.remove(position);
                }
            },
            MenuStatement::Entry {
                entry,
                placement: None,
            } => {
                let existing = if replace_by_label {
                    label_of(&entry).and_then(|label| label_position(&merged, &label, &label_of))
                } else {
                    None
                };
                match existing {
                    Some(position) => merged[position] = entry,
                    None => merged.push(entry),
                }
            },
            MenuStatement::Entry {
                entry,
                placement: Some(placement),
            } => {
                if let Some(label) = label_of(&entry) {
                    if let Some(position) = label_position(&merged, &label, &label_of) {
                        merged.remove(position);
                    }
                }
                let target = match &placement {
                    MenuPlacement::After(anchor) => {
                        label_position(&merged, anchor, &label_of).map(|position| position + 1)
                    },
                    MenuPlacement::Before(anchor) => label_position(&merged, anchor, &label_of),
                };
                match target {
                    Some(position) => merged.insert(position, entry),
                    None => merged.push(entry),
                }
            },
        }
    }
    merged
}

fn context_menu_label(entry: &ContextMenuEntry) -> Option<String> {
    entry.label().map(|label| label.to_owned())
}

pub fn merge_context_menu_entries(
    base: &[ContextMenuEntry],
    statements: Vec<MenuStatement<ContextMenuEntry>>,
) -> Vec<ContextMenuEntry> {
    merge_menu_statements(base, statements, context_menu_label, true)
}

pub fn context_menu_entries_without_defaults(
    statements: Vec<MenuStatement<ContextMenuEntry>>,
) -> Vec<ContextMenuEntry> {
    merge_menu_statements(&[], statements, context_menu_label, false)
}

fn statement_for(
    working: &[ContextMenuEntry],
    wanted: &[ContextMenuEntry],
    index: usize,
) -> Option<MenuStatement<ContextMenuEntry>> {
    let entry = wanted[index].clone();
    let current = working.get(index);
    if entry.label().is_some() && current.and_then(|c| c.label()) == entry.label() {
        return Some(MenuStatement::plain(entry));
    }
    let after = index
        .checked_sub(1)
        .and_then(|previous| wanted[previous].label())
        .map(|label| MenuPlacement::After(label.to_owned()));
    let before = || {
        current
            .and_then(|c| c.label())
            .map(|label| MenuPlacement::Before(label.to_owned()))
    };
    match after.or_else(before) {
        Some(placement) => Some(MenuStatement::Entry {
            entry,
            placement: Some(placement),
        }),
        None if index == working.len() && entry.label().is_some() => {
            Some(MenuStatement::plain(entry))
        },
        None => None,
    }
}

pub fn context_menu_statements_against(
    entries: &[ContextMenuEntry],
    defaults: &[ContextMenuEntry],
) -> Option<Vec<MenuStatement<ContextMenuEntry>>> {
    let labels: Vec<&str> = entries.iter().filter_map(|e| e.label()).collect();
    let mut unique = labels.clone();
    unique.sort();
    unique.dedup();
    if unique.len() != labels.len() {
        return None;
    }
    let mut statements: Vec<MenuStatement<ContextMenuEntry>> = defaults
        .iter()
        .filter_map(|e| e.label())
        .filter(|label| !labels.contains(label))
        .map(|label| MenuStatement::Remove(label.to_owned()))
        .collect();
    let mut working = merge_context_menu_entries(defaults, statements.clone());
    for index in 0..entries.len() {
        if working.get(index) == Some(&entries[index]) {
            continue;
        }
        let statement = statement_for(&working, entries, index)?;
        working = merge_context_menu_entries(&working, vec![statement.clone()]);
        statements.push(statement);
        if working.get(..=index) != Some(&entries[..=index]) {
            return None;
        }
    }
    if working != entries || merge_context_menu_entries(defaults, statements.clone()) != entries {
        return None;
    }
    Some(statements)
}

pub fn normalize_separators(entries: Vec<ContextMenuEntry>) -> Vec<ContextMenuEntry> {
    let mut normalized: Vec<ContextMenuEntry> = vec![];
    for entry in entries {
        let is_separator = matches!(entry, ContextMenuEntry::Separator);
        let previous_is_separator_or_empty = normalized
            .last()
            .map(|e| matches!(e, ContextMenuEntry::Separator))
            .unwrap_or(true);
        if is_separator && previous_is_separator_or_empty {
            continue;
        }
        normalized.push(entry);
    }
    while matches!(normalized.last(), Some(ContextMenuEntry::Separator)) {
        normalized.pop();
    }
    normalized
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::actions::Action;

    fn item(label: &str) -> ContextMenuEntry {
        ContextMenuEntry::item(label, vec![Action::Detach.into()])
    }

    #[test]
    fn common_items_are_added_after_a_separator_for_every_kind() {
        let config = ContextMenuConfig {
            pane: vec![item("p")],
            tab: vec![item("t")],
            bar: vec![item("b")],
            common: vec![item("c")],
        };
        for (kind, first) in [
            (ContextMenuKind::Pane, "p"),
            (ContextMenuKind::PaneFrame, "p"),
            (ContextMenuKind::Tab, "t"),
            (ContextMenuKind::Bar, "b"),
        ] {
            assert_eq!(
                config.entries_for(kind),
                vec![item(first), ContextMenuEntry::Separator, item("c")]
            );
        }
    }

    #[test]
    fn redundant_separators_are_dropped() {
        let config = ContextMenuConfig {
            pane: vec![
                ContextMenuEntry::Separator,
                item("a"),
                ContextMenuEntry::Separator,
                ContextMenuEntry::Separator,
                item("b"),
                ContextMenuEntry::Separator,
            ],
            tab: vec![],
            bar: vec![],
            common: vec![],
        };
        assert_eq!(
            config.entries_for(ContextMenuKind::Pane),
            vec![item("a"), ContextMenuEntry::Separator, item("b")]
        );
        assert!(config.entries_for(ContextMenuKind::Tab).is_empty());
    }

    #[test]
    fn merging_entries_replaces_by_label_and_appends_new_ones() {
        let merged = merge_context_menu_entries(
            &[item("a"), ContextMenuEntry::Separator, item("b")],
            vec![
                MenuStatement::plain(ContextMenuEntry::item("a", vec![Action::Quit.into()])),
                MenuStatement::plain(item("c")),
            ],
        );
        assert_eq!(
            merged,
            vec![
                ContextMenuEntry::item("a", vec![Action::Quit.into()]),
                ContextMenuEntry::Separator,
                item("b"),
                item("c"),
            ]
        );
    }

    #[test]
    fn merging_removes_and_places_items_relative_to_other_items() {
        let placed = |entry: ContextMenuEntry, placement: MenuPlacement| MenuStatement::Entry {
            entry,
            placement: Some(placement),
        };
        let merged = merge_context_menu_entries(
            &[item("a"), item("b"), item("c"), item("d")],
            vec![
                MenuStatement::Remove("b".to_owned()),
                MenuStatement::Remove("missing".to_owned()),
                placed(item("x"), MenuPlacement::After("a".to_owned())),
                placed(item("d"), MenuPlacement::Before("a".to_owned())),
                placed(
                    ContextMenuEntry::Separator,
                    MenuPlacement::After("c".to_owned()),
                ),
                placed(item("y"), MenuPlacement::Before("missing".to_owned())),
            ],
        );
        assert_eq!(
            merged,
            vec![
                item("d"),
                item("a"),
                item("x"),
                item("c"),
                ContextMenuEntry::Separator,
                item("y"),
            ]
        );
    }

    #[test]
    fn entries_without_defaults_keep_duplicate_labels() {
        let entries = context_menu_entries_without_defaults(vec![
            MenuStatement::plain(item("a")),
            MenuStatement::plain(item("a")),
        ]);
        assert_eq!(entries, vec![item("a"), item("a")]);
    }

    #[test]
    fn clicked_targets_are_filled_with_the_clicked_pane_or_tab() {
        use crate::data::{ClickedPaneAction, ClickedTabAction, Direction, PaneId};
        let close_pane = ContextMenuAction::ClickedPane(ClickedPaneAction::CloseFocus);
        let move_tab = ContextMenuAction::ClickedTab(ClickedTabAction::Move(Direction::Left));
        let explicit = ContextMenuAction::Action(Action::CloseFocusByPaneId {
            pane_id: PaneId::Terminal(9),
        });
        assert_eq!(
            close_pane
                .clone()
                .into_action(Some(PaneId::Terminal(2)), Some(5)),
            Some(Action::CloseFocusByPaneId {
                pane_id: PaneId::Terminal(2)
            })
        );
        assert_eq!(close_pane.into_action(None, Some(5)), None);
        assert_eq!(
            move_tab.clone().into_action(None, Some(5)),
            Some(Action::MoveTabByTabId {
                id: 5,
                direction: Direction::Left
            })
        );
        assert_eq!(move_tab.into_action(Some(PaneId::Terminal(2)), None), None);
        assert_eq!(
            explicit.into_action(Some(PaneId::Terminal(2)), Some(5)),
            Some(Action::CloseFocusByPaneId {
                pane_id: PaneId::Terminal(9)
            })
        );
    }

    fn statements_rebuild(entries: Vec<ContextMenuEntry>, defaults: Vec<ContextMenuEntry>) {
        let statements = context_menu_statements_against(&entries, &defaults).unwrap();
        assert_eq!(merge_context_menu_entries(&defaults, statements), entries);
    }

    #[test]
    fn statements_against_the_defaults_rebuild_the_entries() {
        let defaults = vec![item("a"), item("b"), ContextMenuEntry::Separator, item("c")];
        assert_eq!(
            context_menu_statements_against(&defaults, &defaults),
            Some(vec![])
        );
        statements_rebuild(
            vec![item("a"), ContextMenuEntry::Separator, item("c")],
            defaults.clone(),
        );
        statements_rebuild(
            vec![
                item("x"),
                item("a"),
                ContextMenuEntry::item("b", vec![Action::Quit.into()]),
                ContextMenuEntry::Separator,
                item("c"),
                ContextMenuEntry::Separator,
                item("y"),
            ],
            defaults.clone(),
        );
        statements_rebuild(
            vec![item("b"), item("a"), ContextMenuEntry::Separator, item("c")],
            defaults.clone(),
        );
        statements_rebuild(vec![item("z")], vec![]);
        assert_eq!(
            context_menu_statements_against(&[item("a"), item("a")], &defaults),
            None
        );
        assert_eq!(
            context_menu_statements_against(&[item("a"), item("b"), item("c")], &defaults),
            None
        );
    }
}
