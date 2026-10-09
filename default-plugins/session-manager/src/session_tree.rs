use std::collections::{BTreeMap, BTreeSet};
use zellij_tile::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TreeNode {
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
}

impl Default for TreeNode {
    fn default() -> Self {
        TreeNode::Session
    }
}

impl SessionTree {
    pub fn flatten(&self, sessions: &[(usize, String)]) -> Vec<TreeRow> {
        let mut rows = vec![];
        for (original_index, name) in sessions {
            let children = self.children.get(name);
            let expanded = self.expanded.contains(name);
            rows.push(TreeRow {
                original_index: *original_index,
                session: name.clone(),
                node: TreeNode::Session,
                depth: 0,
                expandable: children.map(|c| !c.is_empty()).unwrap_or(true),
                expanded,
            });
            if !expanded {
                continue;
            }
            let Some(tabs) = children else {
                continue;
            };
            for (tab_index, tab) in tabs.iter().enumerate() {
                let tab_expanded = self.expanded_tabs.contains(&(name.clone(), tab_index));
                rows.push(TreeRow {
                    original_index: *original_index,
                    session: name.clone(),
                    node: TreeNode::Tab(tab_index),
                    depth: 1,
                    expandable: !tab.panes.is_empty(),
                    expanded: tab_expanded,
                });
                if !tab_expanded {
                    continue;
                }
                for pane_index in 0..tab.panes.len() {
                    rows.push(TreeRow {
                        original_index: *original_index,
                        session: name.clone(),
                        node: TreeNode::Pane(tab_index, pane_index),
                        depth: 2,
                        expandable: false,
                        expanded: false,
                    });
                }
            }
        }
        rows
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
    }

    pub fn toggle(&mut self, row: &TreeRow) {
        match row.node {
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
            TreeNode::Pane(..) => {},
        }
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
}
