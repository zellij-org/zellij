use crate::data::{ContextMenuEntry, ContextMenuKind, InputMode, KeyWithModifier, KeybindsVec};
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
    actions: &[Action],
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

pub fn merge_context_menu_entries(
    base: &[ContextMenuEntry],
    additions: Vec<ContextMenuEntry>,
) -> Vec<ContextMenuEntry> {
    let mut merged = base.to_vec();
    for entry in additions {
        let existing_position = entry
            .label()
            .and_then(|label| merged.iter().position(|e| e.label() == Some(label)));
        match existing_position {
            Some(position) => merged[position] = entry,
            None => merged.push(entry),
        }
    }
    merged
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
        ContextMenuEntry::item(label, vec![Action::Detach])
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
            vec![ContextMenuEntry::item("a", vec![Action::Quit]), item("c")],
        );
        assert_eq!(
            merged,
            vec![
                ContextMenuEntry::item("a", vec![Action::Quit]),
                ContextMenuEntry::Separator,
                item("b"),
                item("c"),
            ]
        );
    }

    #[test]
    fn missing_targets_are_filled_with_the_clicked_pane_or_tab() {
        use crate::data::{Direction, PaneId};
        let mut close_pane = Action::CloseFocusByPaneId { pane_id: None };
        let mut move_tab = Action::MoveTabByTabId {
            id: None,
            direction: Direction::Left,
        };
        let mut explicit = Action::CloseFocusByPaneId {
            pane_id: Some(PaneId::Terminal(9)),
        };
        assert!(close_pane.has_missing_target());
        assert!(move_tab.has_missing_target());
        assert!(!explicit.has_missing_target());
        assert!(!Action::CloseFocus.has_missing_target());
        for action in [&mut close_pane, &mut move_tab, &mut explicit] {
            action.fill_context_menu_target(Some(PaneId::Terminal(2)), Some(5));
        }
        assert_eq!(
            close_pane,
            Action::CloseFocusByPaneId {
                pane_id: Some(PaneId::Terminal(2))
            }
        );
        assert_eq!(
            move_tab,
            Action::MoveTabByTabId {
                id: Some(5),
                direction: Direction::Left
            }
        );
        assert_eq!(
            explicit,
            Action::CloseFocusByPaneId {
                pane_id: Some(PaneId::Terminal(9))
            }
        );
    }
}
