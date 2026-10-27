use super::Screen;
use crate::panes::PaneId;
use crate::plugins::{PluginInstruction, PromptCaller, SERVER_NOTICE_CALLER_ID};
use crate::tab::{PopupKind, PopupPlacement};
use crate::ClientId;
use std::collections::BTreeMap;
use zellij_utils::attention::sanitize_display_text;
use zellij_utils::data::{
    bar_role_label, ClickedPaneAction, ContextMenuAction, ContextMenuContext, ContextMenuEntry,
    ContextMenuKind, HiddenBars, InputMode, PopupCorner,
};
use zellij_utils::errors::prelude::*;
use zellij_utils::input::actions::Action;
use zellij_utils::input::keybinds::shortcut_for_action;
use zellij_utils::input::layout::{Run, RunPluginOrAlias};

pub const MODE_HINTS_ROLE: &str = "mode-hints";
pub const BARS_PLUGIN_URL: &str = "zellij:bars";
pub const HIDDEN_BARS_NOTIFICATION_MS: u64 = 4000;
const MODE_HINTS_INITIAL_COLS: usize = 30;
const MODE_HINTS_INITIAL_ROWS: usize = 6;

pub fn is_mode_hints_run(run: &Option<Run>) -> bool {
    run.as_ref()
        .and_then(|run| run.get_run_plugin())
        .map(|run_plugin| {
            run_plugin.location.display() == BARS_PLUGIN_URL
                && run_plugin
                    .configuration
                    .inner()
                    .get("role")
                    .map(|r| r.as_str())
                    == Some(MODE_HINTS_ROLE)
        })
        .unwrap_or(false)
}

fn hidden_bars_description(hidden_bars: &HiddenBars) -> String {
    match hidden_bars {
        HiddenBars::Named(names) if names.len() == 1 => names
            .iter()
            .next()
            .and_then(|name| bar_role_label(name))
            .map(|label| {
                let mut chars = label.chars();
                match chars.next() {
                    Some(first) => first.to_uppercase().chain(chars).collect(),
                    None => String::new(),
                }
            })
            .unwrap_or_else(|| "Bars".to_owned()),
        _ => "Bars".to_owned(),
    }
}

impl Screen {
    pub fn toggle_bars(&mut self, bars: &[String], client_id: ClientId) {
        if self.hidden_bars.is_empty() {
            self.hide_bars(bars, client_id);
        } else {
            self.show_bars();
        }
    }
    pub fn hide_bars(&mut self, bars: &[String], _client_id: ClientId) {
        let requested = HiddenBars::all_unless_named(bars);
        let hidden_bars = self.hidden_bars.union(&requested);
        if hidden_bars == self.hidden_bars {
            return;
        }
        self.set_hidden_bars(hidden_bars);
        self.show_hidden_bars_notification(&requested);
    }
    pub fn show_bars(&mut self) {
        if self.hidden_bars.is_empty() {
            return;
        }
        self.set_hidden_bars(HiddenBars::None);
    }
    pub fn hide_bar_by_pane_id(&mut self, pane_id: PaneId, client_id: ClientId) {
        let bar_name = self
            .tabs
            .values()
            .find_map(|tab| tab.bar_names_of_pane(pane_id))
            .and_then(|names| names.into_iter().next());
        if let Some(bar_name) = bar_name {
            self.hide_bars(&[bar_name], client_id);
        }
    }
    pub fn set_hidden_bars(&mut self, hidden_bars: HiddenBars) {
        self.hidden_bars = hidden_bars;
        self.default_mode_info.hidden_bars = self.hidden_bars.clone();
        for mode_info in self.mode_info.values_mut() {
            mode_info.hidden_bars = self.hidden_bars.clone();
        }
        self.apply_hidden_bars_to_all_tabs();
        self.render(None).non_fatal();
    }
    pub fn apply_hidden_bars_to_all_tabs(&mut self) {
        let hidden_bars = self.hidden_bars.clone();
        for tab in self.tabs.values_mut() {
            tab.apply_hidden_bars(&hidden_bars);
        }
        self.update_mode_hints_for_all_clients();
    }
    pub fn apply_hidden_bars_to_tab(&mut self, tab_id: usize) {
        let hidden_bars = self.hidden_bars.clone();
        if let Some(tab) = self.tabs.get_mut(&tab_id) {
            tab.apply_hidden_bars(&hidden_bars);
        }
        self.update_mode_hints_for_all_clients();
    }
    pub fn apply_hidden_bars_to_tab_with_pane(&mut self, pane_id: PaneId) {
        let tab_id = self
            .tabs
            .iter()
            .find(|(_, tab)| tab.has_pane_with_pid(&pane_id))
            .map(|(tab_id, _)| *tab_id);
        if let Some(tab_id) = tab_id {
            self.apply_hidden_bars_to_tab(tab_id);
        }
    }
    pub fn update_mode_hints_for_all_clients(&mut self) {
        let client_ids: Vec<ClientId> = self.active_tab_ids.keys().copied().collect();
        for client_id in client_ids {
            self.update_mode_hints(client_id);
        }
    }
    fn mode_hints_wanted(&self, client_id: ClientId) -> bool {
        if !self.mode_hints_when_bars_hidden || self.hidden_bars.is_empty() {
            return false;
        }
        let Some(mode_info) = self.mode_info.get(&client_id) else {
            return false;
        };
        let base_mode = mode_info
            .base_mode
            .unwrap_or_else(|| self.base_input_mode());
        if mode_info.mode == base_mode || mode_info.mode == InputMode::Prompt {
            return false;
        }
        let Ok(tab) = self.get_active_tab(client_id) else {
            return false;
        };
        tab.has_hidden_bar() && !tab.mode_is_shown_by_a_visible_bar()
    }
    fn mode_hints_popup_ids(&self, client_id: ClientId) -> Vec<u32> {
        self.tabs
            .values()
            .flat_map(|tab| {
                tab.popup_plugin_ids_for_client(client_id)
                    .into_iter()
                    .filter(|plugin_id| {
                        tab.popup_pane(*plugin_id)
                            .map(|pane| is_mode_hints_run(pane.invoked_with()))
                            .unwrap_or(false)
                    })
                    .collect::<Vec<u32>>()
            })
            .collect()
    }
    pub fn update_mode_hints(&mut self, client_id: ClientId) {
        let wanted = self.mode_hints_wanted(client_id);
        let open_popups = self.mode_hints_popup_ids(client_id);
        if wanted {
            if open_popups.is_empty() && !self.mode_hints_requested.contains(&client_id) {
                self.request_mode_hints_popup(client_id);
            }
            return;
        }
        self.mode_hints_requested.remove(&client_id);
        for plugin_id in open_popups {
            if self.close_popup_with_plugin_id(plugin_id) {
                let _ = self
                    .bus
                    .senders
                    .send_to_plugin(PluginInstruction::Unload(plugin_id));
            }
        }
    }
    fn request_mode_hints_popup(&mut self, client_id: ClientId) {
        let configuration = BTreeMap::from([("role".to_owned(), MODE_HINTS_ROLE.to_owned())]);
        let run_plugin_or_alias =
            match RunPluginOrAlias::from_url(BARS_PLUGIN_URL, &Some(configuration), None, None) {
                Ok(run_plugin_or_alias) => run_plugin_or_alias,
                Err(e) => {
                    log::error!("Failed to open the mode hints: {}", e);
                    return;
                },
            };
        self.mode_hints_requested.insert(client_id);
        self.request_popup(
            client_id,
            run_plugin_or_alias,
            PopupPlacement::Corner(PopupCorner::BottomRight),
            PopupKind::Info,
            None,
            MODE_HINTS_INITIAL_COLS,
            MODE_HINTS_INITIAL_ROWS,
            vec![],
        );
    }
    pub(super) fn accept_mode_hints_popup(
        &mut self,
        run_plugin_or_alias: &RunPluginOrAlias,
        client_id: ClientId,
    ) -> bool {
        let run = Some(Run::Plugin(run_plugin_or_alias.clone()));
        if !is_mode_hints_run(&run) {
            return true;
        }
        let was_requested = self.mode_hints_requested.remove(&client_id);
        was_requested
            && self.mode_hints_wanted(client_id)
            && self.mode_hints_popup_ids(client_id).is_empty()
    }
    fn hidden_bars_notice(&self, client_id: ClientId, hidden_bars: &HiddenBars) -> String {
        let keybinds = self.keybinds_for_client(client_id);
        let base_mode = self
            .mode_info
            .get(&client_id)
            .and_then(|mode_info| mode_info.base_mode)
            .unwrap_or_else(|| self.base_input_mode());
        let shortcut = shortcut_for_action(&keybinds, base_mode, |action| {
            matches!(action, Action::ToggleBars { .. } | Action::ShowBars)
        })
        .filter(|keys| !keys.is_empty())
        .map(|keys| {
            keys.iter()
                .map(|key| key.to_string())
                .collect::<Vec<String>>()
                .join(", ")
        })
        .unwrap_or_else(|| "zellij action show-bars".to_owned());
        let description = hidden_bars_description(hidden_bars);
        if self.context_menu_enabled {
            format!(
                "{} hidden. {} or right-click → Show bars",
                description, shortcut
            )
        } else {
            format!("{} hidden. {} to show", description, shortcut)
        }
    }
    fn send_hidden_bars_notice(&self, client_id: ClientId, hidden_bars: &HiddenBars) {
        let request = zellij_utils::prompt::PromptRequest::notify(
            self.hidden_bars_notice(client_id, hidden_bars),
        )
        .timeout(std::time::Duration::from_millis(
            HIDDEN_BARS_NOTIFICATION_MS,
        ));
        let _ = self
            .bus
            .senders
            .send_to_plugin(PluginInstruction::PromptRequest {
                caller: PromptCaller {
                    plugin_id: SERVER_NOTICE_CALLER_ID,
                    client_id,
                    request_id: 0,
                },
                owner_client_id: client_id,
                caller_pane_id: None,
                request,
            });
    }
    pub fn show_hidden_bars_notification(&mut self, hidden_bars: &HiddenBars) {
        if !self.hidden_bars_notification {
            return;
        }
        let client_ids: Vec<ClientId> = self.active_tab_ids.keys().copied().collect();
        for client_id in client_ids {
            self.send_hidden_bars_notice(client_id, hidden_bars);
        }
    }
    pub fn show_hidden_bars_notification_on_attach(&mut self, client_id: ClientId) {
        if !self.hidden_bars_notification || self.hidden_bars.is_empty() {
            return;
        }
        let hides_a_bar = self
            .get_active_tab(client_id)
            .map(|tab| tab.has_hidden_bar())
            .unwrap_or(false);
        if hides_a_bar {
            self.send_hidden_bars_notice(client_id, &self.hidden_bars);
        }
    }
    pub(super) fn resolve_hidden_bars_context_menu_entries(
        &self,
        entries: Vec<ContextMenuEntry>,
        context: &ContextMenuContext,
    ) -> Vec<ContextMenuEntry> {
        let clicked_bar_label = match context.kind {
            ContextMenuKind::Bar => context.pane_id.and_then(|pane_id| {
                self.tabs
                    .values()
                    .find_map(|tab| tab.bar_label_of_pane(PaneId::from(pane_id)))
            }),
            _ => None,
        };
        let (bar_count, visible_bar_count) = self
            .get_active_tab(context.client_id)
            .map(|tab| (tab.bar_count(), tab.visible_bar_count()))
            .unwrap_or((0, 0));
        let is_bar_menu = context.kind == ContextMenuKind::Bar;
        let nothing_is_hidden = self.hidden_bars.is_empty();
        entries
            .into_iter()
            .filter_map(|entry| match entry {
                ContextMenuEntry::Item {
                    label,
                    actions,
                    toggled_label,
                } => {
                    let shows_bars = actions
                        .iter()
                        .any(|a| matches!(a, ContextMenuAction::Action(Action::ShowBars)));
                    if shows_bars && nothing_is_hidden {
                        return None;
                    }
                    let hides_all_bars = actions.iter().any(|a| {
                        matches!(a, ContextMenuAction::Action(Action::HideBars { bars }) if bars.is_empty())
                    });
                    if hides_all_bars
                        && (visible_bar_count == 0 || (is_bar_menu && bar_count <= 1))
                    {
                        return None;
                    }
                    let hides_clicked_bar = actions.iter().any(|a| {
                        matches!(a, ContextMenuAction::ClickedPane(ClickedPaneAction::HideBar))
                    });
                    let label = if hides_clicked_bar {
                        let bar_label = sanitize_display_text(clicked_bar_label.as_ref()?);
                        if label.contains("{}") {
                            label.replace("{}", &bar_label)
                        } else {
                            label
                        }
                    } else {
                        label
                    };
                    Some(ContextMenuEntry::Item {
                        label,
                        actions,
                        toggled_label,
                    })
                },
                other => Some(other),
            })
            .collect()
    }
}
