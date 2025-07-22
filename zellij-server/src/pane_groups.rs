use std::collections::{HashMap, HashSet};

use zellij_utils::data::FloatingPaneCoordinates;
use zellij_utils::input::layout::{RunPluginOrAlias, SplitSize};
use zellij_utils::pane_size::Size;

use crate::{panes::PaneId, pty::PtyInstruction, thread_bus::ThreadSenders, ClientId};

pub struct PaneGroups {
    panes_in_group: HashMap<ClientId, Vec<PaneId>>,
    senders: ThreadSenders,
}

impl std::fmt::Debug for PaneGroups {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PaneGroups")
            .field("panes_in_group", &self.panes_in_group)
            .finish_non_exhaustive()
    }
}

impl PaneGroups {
    pub fn new(senders: ThreadSenders) -> Self {
        PaneGroups {
            panes_in_group: HashMap::new(),
            senders,
        }
    }
    pub fn clone_inner(&self) -> HashMap<ClientId, Vec<PaneId>> {
        self.panes_in_group.clone()
    }
    pub fn get_client_pane_group(&self, client_id: &ClientId) -> HashSet<PaneId> {
        self.panes_in_group
            .get(client_id)
            .map(|p| p.iter().copied().collect())
            .unwrap_or_else(|| HashSet::new())
    }
    pub fn clear_pane_group(&mut self, client_id: &ClientId) {
        self.panes_in_group.get_mut(client_id).map(|p| p.clear());
    }
    pub fn toggle_pane_id_in_group(
        &mut self,
        pane_id: PaneId,
        screen_size: Size,
        client_id: &ClientId,
    ) {
        let previous_groups = self.clone_inner();
        let client_pane_group = self
            .panes_in_group
            .entry(*client_id)
            .or_insert_with(|| vec![]);
        if client_pane_group.contains(&pane_id) {
            client_pane_group.retain(|p| p != &pane_id);
        } else {
            client_pane_group.push(pane_id);
        };
        if self.should_launch_plugin(&previous_groups, client_id) {
            self.launch_plugin(screen_size, client_id);
        }
    }
    pub fn add_pane_id_to_group(
        &mut self,
        pane_id: PaneId,
        screen_size: Size,
        client_id: &ClientId,
    ) {
        let previous_groups = self.clone_inner();
        let client_pane_group = self
            .panes_in_group
            .entry(*client_id)
            .or_insert_with(|| vec![]);
        if !client_pane_group.contains(&pane_id) {
            client_pane_group.push(pane_id);
        }
        if self.should_launch_plugin(&previous_groups, client_id) {
            self.launch_plugin(screen_size, client_id);
        }
    }
    pub fn group_and_ungroup_panes(
        &mut self,
        mut pane_ids_to_group: Vec<PaneId>,
        pane_ids_to_ungroup: Vec<PaneId>,
        screen_size: Size,
        client_id: &ClientId,
    ) {
        let previous_groups = self.clone_inner();
        let client_pane_group = self
            .panes_in_group
            .entry(*client_id)
            .or_insert_with(|| vec![]);
        client_pane_group.append(&mut pane_ids_to_group);
        client_pane_group.retain(|p| !pane_ids_to_ungroup.contains(p));
        if self.should_launch_plugin(&previous_groups, client_id) {
            self.launch_plugin(screen_size, client_id);
        }
    }
    pub fn group_and_ungroup_panes_for_all_clients(
        &mut self,
        pane_ids_to_group: Vec<PaneId>,
        pane_ids_to_ungroup: Vec<PaneId>,
        screen_size: Size,
    ) {
        let previous_groups = self.clone_inner();
        let mut should_launch = false;
        let all_connected_clients: Vec<ClientId> = self.panes_in_group.keys().copied().collect();

        for client_id in &all_connected_clients {
            let client_pane_group = self
                .panes_in_group
                .entry(*client_id)
                .or_insert_with(|| vec![]);
            client_pane_group.append(&mut pane_ids_to_group.clone());
            client_pane_group.retain(|p| !pane_ids_to_ungroup.contains(p));

            if self.should_launch_plugin(&previous_groups, &client_id) {
                should_launch = true;
            }
        }

        if should_launch {
            if let Some(first_client) = all_connected_clients.first() {
                self.launch_plugin(screen_size, first_client);
            }
        }
    }
    pub fn override_groups_with(&mut self, new_pane_groups: HashMap<ClientId, Vec<PaneId>>) {
        self.panes_in_group = new_pane_groups;
    }
    fn should_launch_plugin(
        &self,
        previous_groups: &HashMap<ClientId, Vec<PaneId>>,
        client_id: &ClientId,
    ) -> bool {
        let mut should_launch = false;
        for (client_id, previous_panes) in previous_groups {
            let previous_panes_has_panes = !previous_panes.is_empty();
            let current_panes_has_panes = self
                .panes_in_group
                .get(&client_id)
                .map(|g| !g.is_empty())
                .unwrap_or(false);
            if !previous_panes_has_panes && current_panes_has_panes {
                should_launch = true;
            }
        }
        should_launch || previous_groups.get(&client_id).is_none()
    }
    fn launch_plugin(&self, screen_size: Size, client_id: &ClientId) {
        if let Ok(run_plugin) =
            RunPluginOrAlias::from_url("zellij:multiple-select", &None, None, None)
        {
            let tab_index = 1;
            let size = Size::default();
            let should_float = Some(true);
            let should_be_opened_in_place = false;
            let pane_title = None;
            let skip_cache = false;
            let cwd = None;
            let should_focus_plugin = Some(false);
            let width_30_percent = (screen_size.cols as f64 * 0.3) as usize;
            let height_30_percent = (screen_size.rows as f64 * 0.3) as usize;
            let width = std::cmp::max(width_30_percent, 48);
            let height = std::cmp::max(height_30_percent, 10);
            let y_position = screen_size.rows.saturating_sub(height + 2);
            let floating_pane_coordinates = FloatingPaneCoordinates {
                x: Some(SplitSize::Fixed(2)),
                y: Some(SplitSize::Fixed(y_position)),
                width: Some(SplitSize::Fixed(width)),
                height: Some(SplitSize::Fixed(height)),
                pinned: Some(true),
            };
            let _ = self.senders.send_to_pty(PtyInstruction::FillPluginCwd(
                should_float,
                should_be_opened_in_place,
                pane_title,
                run_plugin,
                tab_index,
                None,
                *client_id,
                size,
                skip_cache,
                cwd,
                should_focus_plugin,
                Some(floating_pane_coordinates),
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn create_mock_senders() -> ThreadSenders {
        let mut mock = ThreadSenders::default();
        mock.should_silently_fail = true;
        mock
    }

    fn create_test_pane_groups() -> PaneGroups {
        PaneGroups::new(create_mock_senders())
    }

    fn create_test_screen_size() -> Size {
        Size { rows: 24, cols: 80 }
    }

    #[test]
    fn new_creates_empty_pane_groups() {
        let pane_groups = create_test_pane_groups();
        assert!(pane_groups.panes_in_group.is_empty());
    }

    #[test]
    fn clone_inner_returns_copy_of_internal_map() {
        let mut pane_groups = create_test_pane_groups();
        let client_id: ClientId = 1;
        let pane_id = PaneId::Terminal(10);
        let screen_size = create_test_screen_size();

        pane_groups.add_pane_id_to_group(pane_id, screen_size, &client_id);
        let cloned = pane_groups.clone_inner();

        assert_eq!(cloned.len(), 1);
        assert!(cloned.contains_key(&client_id));
        assert_eq!(cloned[&client_id], vec![pane_id]);
    }

    #[test]
    fn get_client_pane_group_returns_empty_set_for_nonexistent_client() {
        let pane_groups = create_test_pane_groups();
        let client_id: ClientId = 999;

        let result = pane_groups.get_client_pane_group(&client_id);
        assert!(result.is_empty());
    }

    #[test]
    fn get_client_pane_group_returns_correct_panes() {
        let mut pane_groups = create_test_pane_groups();
        let client_id: ClientId = 1;
        let pane_ids = vec![
            PaneId::Terminal(10),
            PaneId::Plugin(20),
            PaneId::Terminal(30),
        ];
        let screen_size = create_test_screen_size();

        for pane_id in &pane_ids {
            pane_groups.add_pane_id_to_group(*pane_id, screen_size, &client_id);
        }

        let result = pane_groups.get_client_pane_group(&client_id);
        assert_eq!(result.len(), 3);
        for pane_id in pane_ids {
            assert!(result.contains(&pane_id));
        }
    }

    #[test]
    fn clear_pane_group_clears_existing_group() {
        let mut pane_groups = create_test_pane_groups();
        let client_id: ClientId = 1;
        let pane_ids = vec![
            PaneId::Terminal(10),
            PaneId::Plugin(20),
            PaneId::Terminal(30),
        ];
        let screen_size = create_test_screen_size();

        for pane_id in pane_ids {
            pane_groups.add_pane_id_to_group(pane_id, screen_size, &client_id);
        }

        assert!(!pane_groups.get_client_pane_group(&client_id).is_empty());

        pane_groups.clear_pane_group(&client_id);

        assert!(pane_groups.get_client_pane_group(&client_id).is_empty());
    }

    #[test]
    fn clear_pane_group_handles_nonexistent_client() {
        let mut pane_groups = create_test_pane_groups();
        let client_id: ClientId = 999;

        pane_groups.clear_pane_group(&client_id);
        assert!(pane_groups.get_client_pane_group(&client_id).is_empty());
    }

    #[test]
    fn toggle_pane_id_adds_new_pane() {
        let mut pane_groups = create_test_pane_groups();
        let client_id: ClientId = 1;
        let pane_id = PaneId::Terminal(10);
        let screen_size = create_test_screen_size();

        pane_groups.toggle_pane_id_in_group(pane_id, screen_size, &client_id);

        let result = pane_groups.get_client_pane_group(&client_id);
        assert!(result.contains(&pane_id));
    }

    #[test]
    fn toggle_pane_id_removes_existing_pane() {
        let mut pane_groups = create_test_pane_groups();
        let client_id: ClientId = 1;
        let pane_id = PaneId::Plugin(10);
        let screen_size = create_test_screen_size();

        pane_groups.add_pane_id_to_group(pane_id, screen_size, &client_id);
        assert!(pane_groups
            .get_client_pane_group(&client_id)
            .contains(&pane_id));

        pane_groups.toggle_pane_id_in_group(pane_id, screen_size, &client_id);
        assert!(!pane_groups
            .get_client_pane_group(&client_id)
            .contains(&pane_id));
    }

    #[test]
    fn add_pane_id_to_group_adds_new_pane() {
        let mut pane_groups = create_test_pane_groups();
        let client_id: ClientId = 1;
        let pane_id = PaneId::Terminal(10);
        let screen_size = create_test_screen_size();

        pane_groups.add_pane_id_to_group(pane_id, screen_size, &client_id);

        let result = pane_groups.get_client_pane_group(&client_id);
        assert!(result.contains(&pane_id));
    }

    #[test]
    fn add_pane_id_to_group_does_not_duplicate() {
        let mut pane_groups = create_test_pane_groups();
        let client_id: ClientId = 1;
        let pane_id = PaneId::Plugin(10);
        let screen_size = create_test_screen_size();

        pane_groups.add_pane_id_to_group(pane_id, screen_size, &client_id);
        pane_groups.add_pane_id_to_group(pane_id, screen_size, &client_id);

        let result = pane_groups.get_client_pane_group(&client_id);
        assert_eq!(result.len(), 1);
        assert!(result.contains(&pane_id));
    }

    #[test]
    fn group_and_ungroup_panes_adds_and_removes_correctly() {
        let mut pane_groups = create_test_pane_groups();
        let client_id: ClientId = 1;
        let screen_size = create_test_screen_size();

        let initial_panes = vec![PaneId::Terminal(1), PaneId::Plugin(2), PaneId::Terminal(3)];
        for pane_id in &initial_panes {
            pane_groups.add_pane_id_to_group(*pane_id, screen_size, &client_id);
        }

        let panes_to_add = vec![PaneId::Plugin(4), PaneId::Terminal(5)];
        let panes_to_remove = vec![PaneId::Plugin(2), PaneId::Terminal(3)];

        pane_groups.group_and_ungroup_panes(panes_to_add, panes_to_remove, screen_size, &client_id);

        let result = pane_groups.get_client_pane_group(&client_id);

        assert!(result.contains(&PaneId::Terminal(1)));
        assert!(result.contains(&PaneId::Plugin(4)));
        assert!(result.contains(&PaneId::Terminal(5)));
        assert!(!result.contains(&PaneId::Plugin(2)));
        assert!(!result.contains(&PaneId::Terminal(3)));
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn override_groups_with_replaces_all_groups() {
        let mut pane_groups = create_test_pane_groups();
        let client_id1: ClientId = 1;
        let client_id2: ClientId = 2;
        let screen_size = create_test_screen_size();

        pane_groups.add_pane_id_to_group(PaneId::Terminal(10), screen_size, &client_id1);

        let mut new_groups = HashMap::new();
        new_groups.insert(client_id2, vec![PaneId::Plugin(20), PaneId::Terminal(30)]);

        pane_groups.override_groups_with(new_groups);

        assert!(pane_groups.get_client_pane_group(&client_id1).is_empty());

        let result = pane_groups.get_client_pane_group(&client_id2);
        assert!(result.contains(&PaneId::Plugin(20)));
        assert!(result.contains(&PaneId::Terminal(30)));
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn multiple_clients_independent_groups() {
        let mut pane_groups = create_test_pane_groups();
        let client_id1: ClientId = 1;
        let client_id2: ClientId = 2;
        let screen_size = create_test_screen_size();

        pane_groups.add_pane_id_to_group(PaneId::Terminal(10), screen_size, &client_id1);
        pane_groups.add_pane_id_to_group(PaneId::Plugin(20), screen_size, &client_id2);

        let group1 = pane_groups.get_client_pane_group(&client_id1);
        let group2 = pane_groups.get_client_pane_group(&client_id2);

        assert!(group1.contains(&PaneId::Terminal(10)));
        assert!(!group1.contains(&PaneId::Plugin(20)));

        assert!(group2.contains(&PaneId::Plugin(20)));
        assert!(!group2.contains(&PaneId::Terminal(10)));
    }

    #[test]
    fn pane_id_variants_work_correctly() {
        let mut pane_groups = create_test_pane_groups();
        let client_id: ClientId = 1;
        let screen_size = create_test_screen_size();

        let terminal_pane = PaneId::Terminal(100);
        let plugin_pane = PaneId::Plugin(200);

        pane_groups.add_pane_id_to_group(terminal_pane, screen_size, &client_id);
        pane_groups.add_pane_id_to_group(plugin_pane, screen_size, &client_id);

        let result = pane_groups.get_client_pane_group(&client_id);
        assert!(result.contains(&terminal_pane));
        assert!(result.contains(&plugin_pane));
        assert_eq!(result.len(), 2);

        let another_terminal = PaneId::Terminal(200);
        assert!(!result.contains(&another_terminal));
    }

    #[test]
    fn should_launch_plugin_returns_true_when_first_pane_added() {
        let pane_groups = create_test_pane_groups();
        let client_id: ClientId = 1;
        let previous_groups = HashMap::new();

        assert!(pane_groups.should_launch_plugin(&previous_groups, &client_id));
    }

    #[test]
    fn should_launch_plugin_returns_true_when_empty_to_non_empty() {
        let mut pane_groups = create_test_pane_groups();
        let client_id: ClientId = 1;
        let screen_size = create_test_screen_size();

        let mut previous_groups = HashMap::new();
        previous_groups.insert(client_id, vec![]);

        pane_groups.add_pane_id_to_group(PaneId::Terminal(10), screen_size, &client_id);

        assert!(pane_groups.should_launch_plugin(&previous_groups, &client_id));
    }

    #[test]
    fn should_launch_plugin_returns_false_when_non_empty_to_non_empty() {
        let mut pane_groups = create_test_pane_groups();
        let client_id: ClientId = 1;
        let screen_size = create_test_screen_size();

        pane_groups.add_pane_id_to_group(PaneId::Terminal(10), screen_size, &client_id);
        let previous_groups = pane_groups.clone_inner();

        pane_groups.add_pane_id_to_group(PaneId::Plugin(20), screen_size, &client_id);

        assert!(!pane_groups.should_launch_plugin(&previous_groups, &client_id));
    }

    #[test]
    fn should_launch_plugin_returns_false_when_non_empty_to_empty() {
        let pane_groups = create_test_pane_groups();
        let client_id: ClientId = 1;

        let mut previous_groups = HashMap::new();
        previous_groups.insert(client_id, vec![PaneId::Terminal(10)]);

        assert!(!pane_groups.should_launch_plugin(&previous_groups, &client_id));
    }

    #[test]
    fn should_launch_plugin_returns_false_when_empty_to_empty() {
        let pane_groups = create_test_pane_groups();
        let client_id: ClientId = 1;

        let mut previous_groups = HashMap::new();
        previous_groups.insert(client_id, vec![]);

        assert!(!pane_groups.should_launch_plugin(&previous_groups, &client_id));
    }
}
