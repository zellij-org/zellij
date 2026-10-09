use std::collections::BTreeMap;

use zellij_tile::prelude::actions::Action;
use zellij_tile::prelude::*;

const CONFIG_DOUBLE_CLICK_TAB: &str = "double_click_tab";
const CONFIG_DOUBLE_CLICK_EMPTY: &str = "double_click_empty";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DoubleClickTabAction {
    #[default]
    Rename,
    Nothing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DoubleClickEmptyAction {
    #[default]
    NewTab,
    Nothing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DoubleClickConfig {
    pub tab: DoubleClickTabAction,
    pub empty: DoubleClickEmptyAction,
}

impl DoubleClickConfig {
    pub fn from_configuration(configuration: &BTreeMap<String, String>) -> Self {
        let tab = match configuration.get(CONFIG_DOUBLE_CLICK_TAB).map(|s| s.trim()) {
            Some("none") => DoubleClickTabAction::Nothing,
            _ => DoubleClickTabAction::Rename,
        };
        let empty = match configuration
            .get(CONFIG_DOUBLE_CLICK_EMPTY)
            .map(|s| s.trim())
        {
            Some("none") => DoubleClickEmptyAction::Nothing,
            _ => DoubleClickEmptyAction::NewTab,
        };
        DoubleClickConfig { tab, empty }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClickTarget {
    Tab(usize),
    Empty,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DoubleClickOutcome {
    RenameTab(usize),
    NewTab,
    Nothing,
}

pub fn classify_click(
    clicked_part: Option<(Option<usize>, &str)>,
    in_reserved_range: bool,
) -> ClickTarget {
    if in_reserved_range {
        return ClickTarget::Other;
    }
    match clicked_part {
        None => ClickTarget::Empty,
        Some((Some(tab_index), _)) => ClickTarget::Tab(tab_index),
        Some((None, text)) if is_blank(text) => ClickTarget::Empty,
        Some((None, _)) => ClickTarget::Other,
    }
}

pub fn double_click_outcome(
    config: &DoubleClickConfig,
    last_left_click: Option<ClickTarget>,
    target: ClickTarget,
) -> DoubleClickOutcome {
    if last_left_click != Some(target) {
        return DoubleClickOutcome::Nothing;
    }
    match target {
        ClickTarget::Tab(tab_index) if config.tab == DoubleClickTabAction::Rename => {
            DoubleClickOutcome::RenameTab(tab_index)
        },
        ClickTarget::Empty if config.empty == DoubleClickEmptyAction::NewTab => {
            DoubleClickOutcome::NewTab
        },
        _ => DoubleClickOutcome::Nothing,
    }
}

pub fn apply_double_click_outcome(
    outcome: DoubleClickOutcome,
    tabs: &[TabInfo],
    active_tab_idx: usize,
) {
    match outcome {
        DoubleClickOutcome::RenameTab(tab_index) => {
            let Some(tab) = tabs.iter().find(|tab| tab.position == tab_index) else {
                return;
            };
            let tab_position = tab_index + 1;
            if tab_position != active_tab_idx {
                switch_tab_to(tab_position as u32);
            }
            run_action(
                Action::StartRenameTabByTabId {
                    id: tab.tab_id as u64,
                },
                BTreeMap::new(),
            );
        },
        DoubleClickOutcome::NewTab => {
            new_tab::<&str>(None, None);
        },
        DoubleClickOutcome::Nothing => {},
    }
}

fn is_blank(text: &str) -> bool {
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            if chars.peek() == Some(&'[') {
                chars.next();
                while let Some(next) = chars.next() {
                    if ('\u{40}'..='\u{7e}').contains(&next) {
                        break;
                    }
                }
            }
        } else if !c.is_whitespace() {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(tab: &str, empty: &str) -> DoubleClickConfig {
        let mut configuration = BTreeMap::new();
        configuration.insert(CONFIG_DOUBLE_CLICK_TAB.to_owned(), tab.to_owned());
        configuration.insert(CONFIG_DOUBLE_CLICK_EMPTY.to_owned(), empty.to_owned());
        DoubleClickConfig::from_configuration(&configuration)
    }

    #[test]
    fn defaults_are_rename_and_new_tab() {
        let config = DoubleClickConfig::from_configuration(&BTreeMap::new());
        assert_eq!(config.tab, DoubleClickTabAction::Rename);
        assert_eq!(config.empty, DoubleClickEmptyAction::NewTab);
    }

    #[test]
    fn none_values_disable_both_actions() {
        let config = config("none", "none");
        assert_eq!(config.tab, DoubleClickTabAction::Nothing);
        assert_eq!(config.empty, DoubleClickEmptyAction::Nothing);
    }

    #[test]
    fn explicit_values_enable_both_actions() {
        let config = config("rename", "new_tab");
        assert_eq!(config.tab, DoubleClickTabAction::Rename);
        assert_eq!(config.empty, DoubleClickEmptyAction::NewTab);
    }

    #[test]
    fn double_click_on_tab_renames_it() {
        let outcome = double_click_outcome(
            &DoubleClickConfig::default(),
            Some(ClickTarget::Tab(2)),
            ClickTarget::Tab(2),
        );
        assert_eq!(outcome, DoubleClickOutcome::RenameTab(2));
    }

    #[test]
    fn double_click_on_tab_does_nothing_when_disabled() {
        let outcome = double_click_outcome(
            &config("none", "new_tab"),
            Some(ClickTarget::Tab(2)),
            ClickTarget::Tab(2),
        );
        assert_eq!(outcome, DoubleClickOutcome::Nothing);
    }

    #[test]
    fn double_click_on_empty_space_opens_new_tab() {
        let outcome = double_click_outcome(
            &DoubleClickConfig::default(),
            Some(ClickTarget::Empty),
            ClickTarget::Empty,
        );
        assert_eq!(outcome, DoubleClickOutcome::NewTab);
    }

    #[test]
    fn double_click_on_empty_space_does_nothing_when_disabled() {
        let outcome = double_click_outcome(
            &config("rename", "none"),
            Some(ClickTarget::Empty),
            ClickTarget::Empty,
        );
        assert_eq!(outcome, DoubleClickOutcome::Nothing);
    }

    #[test]
    fn double_click_on_different_target_than_last_click_is_ignored() {
        let config = DoubleClickConfig::default();
        assert_eq!(
            double_click_outcome(&config, Some(ClickTarget::Tab(1)), ClickTarget::Tab(2)),
            DoubleClickOutcome::Nothing
        );
        assert_eq!(
            double_click_outcome(&config, Some(ClickTarget::Tab(1)), ClickTarget::Empty),
            DoubleClickOutcome::Nothing
        );
        assert_eq!(
            double_click_outcome(&config, None, ClickTarget::Tab(1)),
            DoubleClickOutcome::Nothing
        );
    }

    #[test]
    fn double_click_on_other_elements_does_nothing() {
        let outcome = double_click_outcome(
            &DoubleClickConfig::default(),
            Some(ClickTarget::Other),
            ClickTarget::Other,
        );
        assert_eq!(outcome, DoubleClickOutcome::Nothing);
    }

    #[test]
    fn classify_click_targets() {
        assert_eq!(classify_click(None, false), ClickTarget::Empty);
        assert_eq!(
            classify_click(Some((Some(3), "tab")), false),
            ClickTarget::Tab(3)
        );
        assert_eq!(
            classify_click(Some((None, "\u{1b}[48;5;0m   \u{1b}[0m")), false),
            ClickTarget::Empty
        );
        assert_eq!(
            classify_click(Some((None, "Zellij")), false),
            ClickTarget::Other
        );
        assert_eq!(
            classify_click(Some((Some(3), "tab")), true),
            ClickTarget::Other
        );
        assert_eq!(classify_click(None, true), ClickTarget::Other);
    }
}
