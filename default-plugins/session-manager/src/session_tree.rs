use std::collections::{BTreeMap, BTreeSet};
use zellij_tile::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TreeNode {
    Exited,
    Recent,
    Session,
    Tab(usize),
    Pane(usize, usize),
}

#[derive(Clone, Debug)]
pub struct TreeRow {
    pub original_index: usize,
    pub session: String,
    pub node: TreeNode,
    pub depth: usize,
    pub expandable: bool,
    pub expanded: bool,
    pub covers: Vec<usize>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TreePane {
    pub id: u32,
    pub is_plugin: bool,
    pub title: String,
    pub focused: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TreeTab {
    pub name: String,
    pub active: bool,
    pub panes: Vec<TreePane>,
}

#[derive(Default)]
pub struct SessionTree {
    pub exited_expanded: bool,
    pub expanded: BTreeSet<String>,
    pub expanded_tabs: BTreeSet<(String, usize)>,
    pub children: BTreeMap<String, Vec<TreeTab>>,
    pub node: TreeNode,
    pub node_session: String,
    pub offset: usize,
    pub follow: bool,
    pub page_rows: usize,
    pub rows: Vec<TreeRow>,
    pub row_targets: Vec<(usize, std::ops::Range<usize>, usize)>,
    pub marker_targets: Vec<(usize, usize, usize)>,
    pub hovered: Option<(String, TreeNode)>,
}

pub const RECENT_EXITED: usize = 3;

impl Default for TreeNode {
    fn default() -> Self {
        TreeNode::Session
    }
}

impl SessionTree {
    #[cfg(test)]
    pub fn flatten(&self, sessions: &[(usize, String)]) -> Vec<TreeRow> {
        let sessions: Vec<(usize, String, bool)> = sessions
            .iter()
            .map(|(index, name)| (*index, name.clone(), false))
            .collect();
        self.flatten_grouped(&sessions, false, 0)
    }

    pub fn flatten_grouped(
        &self,
        sessions: &[(usize, String, bool)],
        expand_exited: bool,
        recent: usize,
    ) -> Vec<TreeRow> {
        let mut rows = vec![];
        for (original_index, name, _) in sessions.iter().filter(|(_, _, exited)| !exited) {
            self.push_session(&mut rows, *original_index, name, 0);
        }
        let mut exited: Vec<&(usize, String, bool)> =
            sessions.iter().filter(|(_, _, exited)| *exited).collect();
        let recent = recent.min(exited.len());
        if recent > 0 {
            let shown: Vec<&(usize, String, bool)> = exited.drain(..recent).collect();
            rows.push(TreeRow {
                original_index: shown[0].0,
                session: String::new(),
                node: TreeNode::Recent,
                depth: 0,
                expandable: false,
                expanded: true,
                covers: vec![],
            });
            for (original_index, name, _) in shown {
                self.push_session(&mut rows, *original_index, name, 1);
            }
        }
        let Some(first) = exited.first() else {
            return rows;
        };
        let expanded = self.exited_expanded || expand_exited;
        rows.push(TreeRow {
            original_index: first.0,
            session: String::new(),
            node: TreeNode::Exited,
            depth: 0,
            expandable: true,
            expanded,
            covers: exited.iter().map(|(index, _, _)| *index).collect(),
        });
        if expanded {
            for (original_index, name, _) in exited {
                self.push_session(&mut rows, *original_index, name, 1);
            }
        }
        rows
    }

    fn push_session(
        &self,
        rows: &mut Vec<TreeRow>,
        original_index: usize,
        name: &str,
        depth: usize,
    ) {
        let children = self.children.get(name);
        let expanded = self.expanded.contains(name);
        rows.push(TreeRow {
            original_index,
            session: name.to_owned(),
            node: TreeNode::Session,
            depth,
            expandable: children.map(|c| !c.is_empty()).unwrap_or(true),
            expanded,
            covers: vec![],
        });
        if !expanded {
            return;
        }
        let Some(tabs) = children else {
            return;
        };
        for (tab_index, tab) in tabs.iter().enumerate() {
            let tab_expanded = self.expanded_tabs.contains(&(name.to_owned(), tab_index));
            rows.push(TreeRow {
                original_index,
                session: name.to_owned(),
                node: TreeNode::Tab(tab_index),
                depth: depth + 1,
                expandable: !tab.panes.is_empty(),
                expanded: tab_expanded,
                covers: vec![],
            });
            if !tab_expanded {
                continue;
            }
            for pane_index in 0..tab.panes.len() {
                rows.push(TreeRow {
                    original_index,
                    session: name.to_owned(),
                    node: TreeNode::Pane(tab_index, pane_index),
                    depth: depth + 2,
                    expandable: false,
                    expanded: false,
                    covers: vec![],
                });
            }
        }
    }

    pub fn cursor_position(&self, rows: &[TreeRow], selected: Option<usize>) -> Option<usize> {
        let selected = selected?;
        rows.iter()
            .position(|r| {
                r.original_index == selected
                    && r.node == self.node
                    && r.session == self.node_session
            })
            .or_else(|| {
                rows.iter()
                    .position(|r| r.original_index == selected && r.node == TreeNode::Session)
            })
            .or_else(|| {
                rows.iter()
                    .position(|r| r.node == TreeNode::Exited && r.covers.contains(&selected))
            })
    }

    pub fn toggle(&mut self, row: &TreeRow) {
        match row.node {
            TreeNode::Exited => self.exited_expanded = !self.exited_expanded,
            TreeNode::Session => {
                if !self.expanded.remove(&row.session) {
                    self.expanded.insert(row.session.clone());
                }
            },
            TreeNode::Tab(tab) => {
                let key = (row.session.clone(), tab);
                if !self.expanded_tabs.remove(&key) {
                    self.expanded_tabs.insert(key);
                }
            },
            TreeNode::Recent | TreeNode::Pane(..) => {},
        }
    }

    pub fn is_heading(&self, index: usize) -> bool {
        self.rows
            .get(index)
            .map(|row| row.node == TreeNode::Recent)
            .unwrap_or(false)
    }

    pub fn identity(&self, index: usize) -> Option<(String, TreeNode)> {
        self.rows
            .get(index)
            .map(|row| (row.session.clone(), row.node))
    }

    pub fn set_children_from_tabs(&mut self, session: &str, tabs: &[SessionPreviewTab]) -> bool {
        let tabs: Vec<TreeTab> = tabs
            .iter()
            .map(|tab| TreeTab {
                name: tab.name.clone(),
                active: tab.active,
                panes: tab
                    .panes
                    .iter()
                    .map(|pane| TreePane {
                        id: pane.id,
                        is_plugin: pane.is_plugin,
                        title: pane.title.clone(),
                        focused: pane.focused,
                    })
                    .collect(),
            })
            .collect();
        self.store_children(session, tabs)
    }

    pub fn set_children_from_saved(&mut self, preview: &SavedSessionPreview) -> bool {
        if preview.error.is_some() {
            return false;
        }
        let tabs: Vec<TreeTab> = preview
            .tabs
            .iter()
            .map(|tab| TreeTab {
                name: tab.name.clone(),
                active: tab.focused,
                panes: tab
                    .panes
                    .iter()
                    .enumerate()
                    .map(|(index, pane)| TreePane {
                        id: index as u32,
                        is_plugin: false,
                        title: pane
                            .title
                            .clone()
                            .or_else(|| pane.command.clone())
                            .unwrap_or_else(|| format!("Pane #{}", index + 1)),
                        focused: pane.focused,
                    })
                    .collect(),
            })
            .collect();
        self.store_children(&preview.session_name, tabs)
    }

    pub fn live_pane(&self, session: &str, node: TreeNode) -> Option<(usize, usize, u32)> {
        let tabs = self.children.get(session)?;
        let (tab_index, pane_index) = match node {
            TreeNode::Exited | TreeNode::Recent => return None,
            TreeNode::Session => {
                let tab_index = tabs
                    .iter()
                    .position(|tab| tab.active)
                    .or_else(|| (!tabs.is_empty()).then_some(0))?;
                let pane_index = tabs[tab_index].panes.iter().position(|pane| pane.focused)?;
                (tab_index, pane_index)
            },
            TreeNode::Tab(tab_index) => {
                let panes = &tabs.get(tab_index)?.panes;
                let pane_index = panes
                    .iter()
                    .position(|pane| pane.focused && !pane.is_plugin)
                    .or_else(|| panes.iter().position(|pane| !pane.is_plugin))?;
                (tab_index, pane_index)
            },
            TreeNode::Pane(tab_index, pane_index) => (tab_index, pane_index),
        };
        let pane = tabs.get(tab_index)?.panes.get(pane_index)?;
        if pane.is_plugin {
            return None;
        }
        Some((tab_index, pane_index, pane.id))
    }

    fn store_children(&mut self, session: &str, tabs: Vec<TreeTab>) -> bool {
        if self.children.get(session) == Some(&tabs) {
            return false;
        }
        self.children.insert(session.to_owned(), tabs);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tabs() -> Vec<SessionPreviewTab> {
        vec![
            SessionPreviewTab {
                name: "editor".to_owned(),
                active: true,
                panes: vec![
                    SessionPreviewPane {
                        id: 1,
                        title: "vim".to_owned(),
                        focused: true,
                        ..Default::default()
                    },
                    SessionPreviewPane {
                        id: 2,
                        title: "shell".to_owned(),
                        ..Default::default()
                    },
                ],
            },
            SessionPreviewTab {
                name: "logs".to_owned(),
                active: false,
                panes: vec![],
            },
        ]
    }

    fn sessions() -> Vec<(usize, String)> {
        vec![(0, "api".to_owned()), (1, "web".to_owned())]
    }

    #[test]
    fn collapsed_sessions_show_one_row_each() {
        let tree = SessionTree::default();
        let rows = tree.flatten(&sessions());
        assert_eq!(rows.len(), 2);
        assert!(rows
            .iter()
            .all(|r| r.node == TreeNode::Session && r.depth == 0));
        assert!(rows.iter().all(|r| r.expandable && !r.expanded));
    }

    #[test]
    fn expanding_a_session_and_a_tab_reveals_their_children() {
        let mut tree = SessionTree::default();
        assert!(tree.set_children_from_tabs("api", &tabs()));
        assert!(!tree.set_children_from_tabs("api", &tabs()));
        let rows = tree.flatten(&sessions());
        tree.toggle(&rows[0]);
        let rows = tree.flatten(&sessions());
        let nodes: Vec<TreeNode> = rows.iter().map(|r| r.node).collect();
        assert_eq!(
            nodes,
            vec![
                TreeNode::Session,
                TreeNode::Tab(0),
                TreeNode::Tab(1),
                TreeNode::Session,
            ]
        );
        assert!(rows[1].expandable);
        assert!(!rows[2].expandable);
        tree.toggle(&rows[1]);
        let rows = tree.flatten(&sessions());
        let nodes: Vec<(TreeNode, usize)> = rows.iter().map(|r| (r.node, r.depth)).collect();
        assert_eq!(
            nodes,
            vec![
                (TreeNode::Session, 0),
                (TreeNode::Tab(0), 1),
                (TreeNode::Pane(0, 0), 2),
                (TreeNode::Pane(0, 1), 2),
                (TreeNode::Tab(1), 1),
                (TreeNode::Session, 0),
            ]
        );
        tree.toggle(&rows[0]);
        assert_eq!(tree.flatten(&sessions()).len(), 2);
    }

    #[test]
    fn exited_sessions_collapse_under_one_line() {
        let mut tree = SessionTree::default();
        let sessions = vec![
            (0, "api".to_owned(), false),
            (1, "old".to_owned(), true),
            (2, "web".to_owned(), false),
            (3, "older".to_owned(), true),
        ];
        let rows = tree.flatten_grouped(&sessions, false, 0);
        let shown: Vec<(TreeNode, &str, usize)> = rows
            .iter()
            .map(|r| (r.node, r.session.as_str(), r.depth))
            .collect();
        assert_eq!(
            shown,
            vec![
                (TreeNode::Session, "api", 0),
                (TreeNode::Session, "web", 0),
                (TreeNode::Exited, "", 0),
            ]
        );
        assert_eq!(rows[2].covers, vec![1, 3]);
        assert!(!rows[2].expanded);
        tree.node = TreeNode::Session;
        tree.node_session = "older".to_owned();
        assert_eq!(tree.cursor_position(&rows, Some(3)), Some(2));
        tree.toggle(&rows[2]);
        let rows = tree.flatten_grouped(&sessions, false, 0);
        let shown: Vec<(&str, usize)> =
            rows.iter().map(|r| (r.session.as_str(), r.depth)).collect();
        assert_eq!(
            shown,
            vec![("api", 0), ("web", 0), ("", 0), ("old", 1), ("older", 1)]
        );
        assert_eq!(tree.cursor_position(&rows, Some(3)), Some(4));
        tree.toggle(&rows[2]);
        assert_eq!(tree.flatten_grouped(&sessions, true, 0).len(), 5);
        assert_eq!(
            tree.flatten_grouped(&[(0, "api".to_owned(), false)], false, 0)
                .len(),
            1
        );
    }

    #[test]
    fn the_most_recent_exited_sessions_show_above_the_folded_rest() {
        let tree = SessionTree::default();
        let sessions = vec![
            (0, "api".to_owned(), false),
            (1, "a".to_owned(), true),
            (2, "b".to_owned(), true),
            (3, "c".to_owned(), true),
            (4, "d".to_owned(), true),
            (5, "e".to_owned(), true),
        ];
        let rows = tree.flatten_grouped(&sessions, false, RECENT_EXITED);
        let shown: Vec<(TreeNode, &str, usize)> = rows
            .iter()
            .map(|r| (r.node, r.session.as_str(), r.depth))
            .collect();
        assert_eq!(
            shown,
            vec![
                (TreeNode::Session, "api", 0),
                (TreeNode::Recent, "", 0),
                (TreeNode::Session, "a", 1),
                (TreeNode::Session, "b", 1),
                (TreeNode::Session, "c", 1),
                (TreeNode::Exited, "", 0),
            ]
        );
        assert_eq!(rows[5].covers, vec![4, 5]);
        let few = tree.flatten_grouped(&sessions[..3], false, RECENT_EXITED);
        let nodes: Vec<TreeNode> = few.iter().map(|r| r.node).collect();
        assert_eq!(
            nodes,
            vec![
                TreeNode::Session,
                TreeNode::Recent,
                TreeNode::Session,
                TreeNode::Session
            ],
            "with three or fewer exited sessions there is no folded group"
        );
    }

    #[test]
    fn a_session_without_tabs_cannot_be_expanded() {
        let mut tree = SessionTree::default();
        tree.set_children_from_tabs("api", &[]);
        let rows = tree.flatten(&sessions());
        assert!(!rows[0].expandable);
        assert!(rows[1].expandable);
    }

    #[test]
    fn the_cursor_falls_back_to_the_session_row() {
        let mut tree = SessionTree::default();
        tree.set_children_from_tabs("api", &tabs());
        tree.expanded.insert("api".to_owned());
        let rows = tree.flatten(&sessions());
        tree.node = TreeNode::Tab(1);
        tree.node_session = "api".to_owned();
        assert_eq!(tree.cursor_position(&rows, Some(0)), Some(2));
        tree.node_session = "web".to_owned();
        assert_eq!(tree.cursor_position(&rows, Some(0)), Some(0));
        assert_eq!(tree.cursor_position(&rows, Some(1)), Some(3));
        assert_eq!(tree.cursor_position(&rows, None), None);
    }

    #[test]
    fn saved_sessions_use_titles_or_commands_for_panes() {
        let mut tree = SessionTree::default();
        let saved = SavedSessionPreview {
            session_name: "old".to_owned(),
            tabs: vec![SavedTabPreview {
                name: "main".to_owned(),
                focused: true,
                panes: vec![
                    SavedPanePreview {
                        title: Some("server".to_owned()),
                        ..Default::default()
                    },
                    SavedPanePreview {
                        command: Some("htop".to_owned()),
                        ..Default::default()
                    },
                    SavedPanePreview::default(),
                ],
            }],
            ..Default::default()
        };
        assert!(tree.set_children_from_saved(&saved));
        let titles: Vec<String> = tree.children["old"][0]
            .panes
            .iter()
            .map(|p| p.title.clone())
            .collect();
        assert_eq!(titles, vec!["server", "htop", "Pane #3"]);
        let failed = SavedSessionPreview {
            session_name: "broken".to_owned(),
            error: Some("unreadable".to_owned()),
            ..Default::default()
        };
        assert!(!tree.set_children_from_saved(&failed));
    }

    fn pane(id: u32, title: &str, focused: bool, is_plugin: bool) -> SessionPreviewPane {
        SessionPreviewPane {
            id,
            title: title.to_owned(),
            focused,
            is_plugin,
            ..Default::default()
        }
    }

    fn mixed_tabs() -> Vec<SessionPreviewTab> {
        vec![
            SessionPreviewTab {
                name: "work".to_owned(),
                active: false,
                panes: vec![
                    pane(10, "status", false, true),
                    pane(11, "shell", false, false),
                    pane(12, "editor", true, false),
                ],
            },
            SessionPreviewTab {
                name: "tools".to_owned(),
                active: true,
                panes: vec![
                    pane(20, "logs", false, false),
                    pane(21, "picker", true, true),
                ],
            },
            SessionPreviewTab {
                name: "plain".to_owned(),
                active: false,
                panes: vec![
                    pane(30, "bar", true, true),
                    pane(31, "first", false, false),
                    pane(32, "second", false, false),
                ],
            },
        ]
    }

    #[test]
    fn a_session_tab_or_pane_row_selects_the_pane_the_live_preview_shows() {
        let mut tree = SessionTree::default();
        tree.set_children_from_tabs("api", &tabs());
        tree.set_children_from_tabs("web", &mixed_tabs());
        assert_eq!(
            tree.live_pane("api", TreeNode::Session),
            Some((0, 0, 1)),
            "a session row shows the focused pane of its active tab"
        );
        assert_eq!(
            tree.live_pane("web", TreeNode::Tab(0)),
            Some((0, 2, 12)),
            "a tab row shows its focused terminal pane"
        );
        assert_eq!(
            tree.live_pane("web", TreeNode::Tab(2)),
            Some((2, 1, 31)),
            "a tab whose focused pane is a plugin shows its first terminal pane"
        );
        assert_eq!(
            tree.live_pane("web", TreeNode::Pane(0, 1)),
            Some((0, 1, 11))
        );
        assert_eq!(tree.live_pane("api", TreeNode::Pane(0, 1)), Some((0, 1, 2)));
        let mut without_clients = mixed_tabs();
        for tab in without_clients.iter_mut() {
            tab.active = false;
        }
        tree.set_children_from_tabs("quiet", &without_clients);
        assert_eq!(
            tree.live_pane("quiet", TreeNode::Session),
            Some((0, 2, 12)),
            "a session without an active tab shows the focused pane of its first tab"
        );
    }

    #[test]
    fn plugin_panes_unknown_sessions_and_exited_sessions_have_no_live_pane() {
        let mut tree = SessionTree::default();
        tree.set_children_from_tabs("web", &mixed_tabs());
        assert_eq!(
            tree.live_pane("web", TreeNode::Session),
            None,
            "the focused pane of the active tab is a plugin"
        );
        assert_eq!(tree.live_pane("web", TreeNode::Pane(0, 0)), None);
        assert_eq!(tree.live_pane("web", TreeNode::Pane(1, 1)), None);
        assert_eq!(tree.live_pane("web", TreeNode::Pane(9, 0)), None);
        assert_eq!(tree.live_pane("unknown", TreeNode::Session), None);
        assert_eq!(tree.live_pane("web", TreeNode::Exited), None);
        tree.set_children_from_tabs(
            "empty",
            &[SessionPreviewTab {
                name: "only".to_owned(),
                active: true,
                panes: vec![pane(1, "plugin", false, true)],
            }],
        );
        assert_eq!(tree.live_pane("empty", TreeNode::Session), None);
        assert_eq!(tree.live_pane("empty", TreeNode::Tab(0)), None);
    }
}
