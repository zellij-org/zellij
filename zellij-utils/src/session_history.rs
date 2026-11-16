use serde::{Deserialize, Serialize};

use crate::data::PaneId;
use crate::input::attention::StartupCard;

pub const SESSION_HISTORY_LIMIT: usize = 20;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct SessionHistory {
    pub back: Vec<String>,
    pub forward: Vec<String>,
    pub last: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum HistoryMove {
    Toggle,
    Back,
    Forward,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SessionArrival {
    Switch,
    Toggle,
    Back,
    Forward,
    Attention { pane_id: PaneId },
}

impl From<HistoryMove> for SessionArrival {
    fn from(history_move: HistoryMove) -> Self {
        match history_move {
            HistoryMove::Toggle => SessionArrival::Toggle,
            HistoryMove::Back => SessionArrival::Back,
            HistoryMove::Forward => SessionArrival::Forward,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct SwitchContext {
    pub history: SessionHistory,
    pub arrival: Option<SessionArrival>,
    pub from_session: Option<String>,
    pub startup_card: Option<StartupCard>,
    #[serde(default)]
    pub disconnect_others: bool,
}

impl SwitchContext {
    pub fn with_startup_card(startup_card: StartupCard) -> Self {
        SwitchContext {
            startup_card: Some(startup_card),
            ..Default::default()
        }
    }
    pub fn local_switch(
        history: &SessionHistory,
        from: &str,
        to: &str,
        arrival: SessionArrival,
    ) -> Self {
        let mut history = history.clone();
        history.record_switch(from, to);
        SwitchContext {
            history,
            arrival: Some(arrival),
            from_session: Some(from.to_owned()),
            startup_card: None,
            disconnect_others: false,
        }
    }
    pub fn from_plugin_request(
        startup_card: Option<StartupCard>,
        disconnect_others: bool,
    ) -> Option<Self> {
        if startup_card.is_none() && !disconnect_others {
            return None;
        }
        Some(SwitchContext {
            startup_card,
            disconnect_others,
            ..Default::default()
        })
    }
    pub fn is_a_switch(&self) -> bool {
        self.from_session.is_some()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct SessionHistoryInfo {
    pub current: String,
    pub previous: Option<String>,
    pub back: Vec<String>,
    pub forward: Vec<String>,
    pub arrived_from: Option<String>,
    pub arrival: Option<SessionArrival>,
    pub arrived_at_ms: u64,
}

impl SessionHistoryInfo {
    pub fn can_go_back(&self) -> bool {
        !self.back.is_empty()
    }
    pub fn can_go_forward(&self) -> bool {
        !self.forward.is_empty()
    }
}

impl SessionHistory {
    pub fn previous(&self) -> Option<&str> {
        self.last
            .as_deref()
            .or_else(|| self.back.last().map(|s| s.as_str()))
    }
    pub fn can_go_back(&self) -> bool {
        !self.back.is_empty()
    }
    pub fn can_go_forward(&self) -> bool {
        !self.forward.is_empty()
    }
    pub fn is_empty(&self) -> bool {
        self.back.is_empty() && self.forward.is_empty() && self.last.is_none()
    }
    pub fn record_switch(&mut self, from: &str, to: &str) {
        if from == to {
            return;
        }
        self.back.retain(|name| name != from && name != to);
        self.forward.clear();
        self.back.push(from.to_owned());
        self.last = Some(from.to_owned());
        self.trim();
    }
    pub fn target(&self, history_move: HistoryMove) -> Option<&str> {
        match history_move {
            HistoryMove::Toggle => self.previous(),
            HistoryMove::Back => self.back.last().map(|s| s.as_str()),
            HistoryMove::Forward => self.forward.last().map(|s| s.as_str()),
        }
    }
    pub fn apply(&mut self, history_move: HistoryMove, current: &str) -> Option<String> {
        match history_move {
            HistoryMove::Toggle => {
                let target = self.previous()?.to_owned();
                self.record_switch(current, &target);
                Some(target)
            },
            HistoryMove::Back => self.go_back(current),
            HistoryMove::Forward => self.go_forward(current),
        }
    }
    pub fn go_back(&mut self, current: &str) -> Option<String> {
        let target = self.back.pop()?;
        self.back.retain(|name| name != current);
        self.forward
            .retain(|name| name != current && name != &target);
        self.forward.push(current.to_owned());
        self.last = Some(current.to_owned());
        self.trim();
        Some(target)
    }
    pub fn go_forward(&mut self, current: &str) -> Option<String> {
        let target = self.forward.pop()?;
        self.forward.retain(|name| name != current);
        self.back.retain(|name| name != current && name != &target);
        self.back.push(current.to_owned());
        self.last = Some(current.to_owned());
        self.trim();
        Some(target)
    }
    pub fn remove_session(&mut self, name: &str) {
        self.back.retain(|n| n != name);
        self.forward.retain(|n| n != name);
        if self.last.as_deref() == Some(name) {
            self.last = None;
        }
    }
    pub fn retain_sessions(&mut self, keep: impl Fn(&str) -> bool) {
        self.back.retain(|n| keep(n));
        self.forward.retain(|n| keep(n));
        if let Some(last) = self.last.as_deref() {
            if !keep(last) {
                self.last = None;
            }
        }
    }
    pub fn rename_session(&mut self, old_name: &str, new_name: &str) {
        for name in self.back.iter_mut().chain(self.forward.iter_mut()) {
            if name == old_name {
                *name = new_name.to_owned();
            }
        }
        if self.last.as_deref() == Some(old_name) {
            self.last = Some(new_name.to_owned());
        }
    }
    pub fn adopt(&mut self, handed: SessionHistory, current: &str) {
        let came_from = handed.last.clone().filter(|name| name != current);
        let mut handed_back: Vec<String> = handed.back;
        handed_back.extend(came_from.clone());
        let mut forward: Vec<String> = Vec::new();
        for name in handed.forward.into_iter().rev() {
            if name != current && !handed_back.contains(&name) && !forward.contains(&name) {
                forward.insert(0, name);
            }
        }
        let mut back: Vec<String> = Vec::new();
        for name in std::mem::take(&mut self.back)
            .into_iter()
            .chain(handed_back)
            .rev()
        {
            if name != current && !forward.contains(&name) && !back.contains(&name) {
                back.insert(0, name);
            }
        }
        self.back = back;
        self.forward = forward;
        self.last = came_from.or_else(|| self.last.take().filter(|name| name != current));
        while self.back.len() + self.forward.len() > SESSION_HISTORY_LIMIT {
            let keep = self.last.clone();
            if let Some(index) = self
                .back
                .iter()
                .position(|name| Some(name) != keep.as_ref())
            {
                self.back.remove(index);
            } else if let Some(index) = self
                .forward
                .iter()
                .position(|name| Some(name) != keep.as_ref())
            {
                self.forward.remove(index);
            } else {
                break;
            }
        }
        if let Some(last) = self.last.clone() {
            if !self.back.contains(&last) && !self.forward.contains(&last) {
                self.last = None;
            }
        }
    }
    pub fn info(&self, current: &str) -> SessionHistoryInfo {
        let mut history = self.clone();
        history.remove_session(current);
        SessionHistoryInfo {
            current: current.to_owned(),
            previous: history.previous().map(|s| s.to_owned()),
            back: history.back,
            forward: history.forward,
            ..Default::default()
        }
    }
    fn trim(&mut self) {
        while self.back.len() + self.forward.len() > SESSION_HISTORY_LIMIT {
            if !self.back.is_empty() {
                self.back.remove(0);
            } else {
                self.forward.remove(0);
            }
        }
        if let Some(last) = self.last.clone() {
            if !self.back.contains(&last) && !self.forward.contains(&last) {
                self.last = None;
            }
        }
    }
}

pub fn next_session_after<'a>(
    names: impl IntoIterator<Item = &'a String>,
    current: &str,
) -> Option<String> {
    let mut names: Vec<&String> = names
        .into_iter()
        .filter(|name| !crate::consts::is_app_socket_name(name))
        .collect();
    names.sort();
    names.dedup();
    names
        .iter()
        .find(|name| name.as_str() > current)
        .or_else(|| names.first())
        .filter(|name| name.as_str() != current)
        .map(|name| (*name).clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_session_follows_name_order_and_wraps() {
        let names: Vec<String> = ["b", "a", "c", ".zellij-app-panel"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(next_session_after(&names, "a"), Some("b".to_owned()));
        assert_eq!(next_session_after(&names, "c"), Some("a".to_owned()));
        assert_eq!(next_session_after(&names, "bb"), Some("c".to_owned()));
        let only: Vec<String> = vec!["a".to_owned()];
        assert_eq!(next_session_after(&only, "a"), None);
    }

    fn visit(history: &mut SessionHistory, path: &[&str]) {
        for pair in path.windows(2) {
            history.record_switch(pair[0], pair[1]);
        }
    }

    #[test]
    fn switching_records_the_session_left() {
        let mut history = SessionHistory::default();
        visit(&mut history, &["a", "b", "c"]);
        assert_eq!(history.back, vec!["a", "b"]);
        assert_eq!(history.previous(), Some("b"));
        assert!(history.forward.is_empty());
    }

    #[test]
    fn revisiting_a_session_moves_it_instead_of_repeating_it() {
        let mut history = SessionHistory::default();
        visit(&mut history, &["a", "b", "a", "c"]);
        assert_eq!(history.back, vec!["b", "a"]);
    }

    #[test]
    fn toggle_twice_returns_to_the_start() {
        let mut history = SessionHistory::default();
        visit(&mut history, &["a", "b"]);
        assert_eq!(
            history.apply(HistoryMove::Toggle, "b"),
            Some("a".to_owned())
        );
        assert_eq!(history.previous(), Some("b"));
        assert_eq!(
            history.apply(HistoryMove::Toggle, "a"),
            Some("b".to_owned())
        );
        assert_eq!(history.previous(), Some("a"));
    }

    #[test]
    fn back_and_forward_walk_the_history() {
        let mut history = SessionHistory::default();
        visit(&mut history, &["a", "b", "c"]);
        assert_eq!(history.go_back("c"), Some("b".to_owned()));
        assert_eq!(history.go_back("b"), Some("a".to_owned()));
        assert!(!history.can_go_back());
        assert_eq!(history.forward, vec!["c", "b"]);
        assert_eq!(history.go_forward("a"), Some("b".to_owned()));
        assert_eq!(history.go_forward("b"), Some("c".to_owned()));
        assert!(!history.can_go_forward());
        assert_eq!(history.back, vec!["a", "b"]);
    }

    #[test]
    fn toggle_after_going_back_returns_to_the_session_left() {
        let mut history = SessionHistory::default();
        visit(&mut history, &["a", "b"]);
        assert_eq!(history.go_back("b"), Some("a".to_owned()));
        assert_eq!(history.previous(), Some("b"));
        assert_eq!(
            history.apply(HistoryMove::Toggle, "a"),
            Some("b".to_owned())
        );
    }

    #[test]
    fn a_new_switch_clears_forward() {
        let mut history = SessionHistory::default();
        visit(&mut history, &["a", "b", "c"]);
        history.go_back("c");
        history.record_switch("b", "d");
        assert!(history.forward.is_empty());
        assert_eq!(history.back, vec!["a", "b"]);
    }

    #[test]
    fn ended_sessions_are_removed() {
        let mut history = SessionHistory::default();
        visit(&mut history, &["a", "b", "c"]);
        history.retain_sessions(|name| name != "b");
        assert_eq!(history.back, vec!["a"]);
        assert_eq!(history.previous(), Some("a"));
        history.remove_session("a");
        assert_eq!(history.previous(), None);
        assert!(!history.can_go_back());
    }

    #[test]
    fn history_is_capped() {
        let mut history = SessionHistory::default();
        let names: Vec<String> = (0..30).map(|i| format!("s{}", i)).collect();
        let refs: Vec<&str> = names.iter().map(|s| s.as_str()).collect();
        visit(&mut history, &refs);
        assert_eq!(history.back.len(), SESSION_HISTORY_LIMIT);
        assert_eq!(history.back.first().map(|s| s.as_str()), Some("s9"));
        assert_eq!(history.previous(), Some("s28"));
    }

    #[test]
    fn renaming_keeps_the_place() {
        let mut history = SessionHistory::default();
        visit(&mut history, &["a", "b", "c"]);
        history.rename_session("b", "beta");
        assert_eq!(history.back, vec!["a", "beta"]);
        assert_eq!(history.previous(), Some("beta"));
    }

    #[test]
    fn an_adopted_history_is_the_most_recent_and_points_back_where_the_user_came_from() {
        let mut own = SessionHistory::default();
        visit(&mut own, &["notes", "logs", "api"]);
        let mut handed = SessionHistory::default();
        visit(&mut handed, &["docs", "web", "api"]);
        own.adopt(handed, "api");
        assert_eq!(own.back, vec!["notes", "logs", "docs", "web"]);
        assert!(own.forward.is_empty());
        assert_eq!(own.last.as_deref(), Some("web"));
        assert_eq!(own.previous(), Some("web"));
        assert_eq!(
            own.apply(HistoryMove::Toggle, "api"),
            Some("web".to_owned())
        );
    }

    #[test]
    fn adopting_replaces_own_forward_entries_and_drops_the_current_one() {
        let mut own = SessionHistory {
            back: vec!["a".to_owned(), "here".to_owned()],
            forward: vec!["f1".to_owned(), "b".to_owned()],
            last: Some("a".to_owned()),
        };
        let handed = SessionHistory {
            back: vec!["b".to_owned(), "from".to_owned()],
            forward: vec!["here".to_owned(), "f0".to_owned()],
            last: Some("from".to_owned()),
        };
        own.adopt(handed, "here");
        assert_eq!(own.back, vec!["a", "b", "from"]);
        assert_eq!(own.forward, vec!["f0"]);
        assert_eq!(own.last.as_deref(), Some("from"));
    }

    #[test]
    fn going_back_after_adopting_visits_the_handed_over_sessions_before_the_own_ones() {
        let mut own = SessionHistory::default();
        visit(&mut own, &["o1", "o2", "here"]);
        let mut handed = SessionHistory::default();
        visit(&mut handed, &["h1", "h2", "from", "here"]);
        own.adopt(handed, "here");
        let mut current = "here".to_owned();
        let mut visited = vec![];
        while let Some(target) = own.go_back(&current) {
            visited.push(target.clone());
            current = target;
        }
        assert_eq!(visited, vec!["from", "h2", "h1", "o2", "o1"]);
    }

    #[test]
    fn adopting_keeps_each_session_at_its_most_recent_place() {
        let mut own = SessionHistory {
            back: vec!["x".to_owned(), "y".to_owned(), "z".to_owned()],
            forward: vec!["ahead".to_owned()],
            last: Some("z".to_owned()),
        };
        let handed = SessionHistory {
            back: vec!["y".to_owned(), "w".to_owned(), "x".to_owned()],
            forward: vec!["z".to_owned(), "f".to_owned(), "z".to_owned()],
            last: Some("x".to_owned()),
        };
        own.adopt(handed, "here");
        assert_eq!(own.back, vec!["y", "w", "x"]);
        assert_eq!(own.forward, vec!["f", "z"]);
        assert_eq!(own.last.as_deref(), Some("x"));
        assert_eq!(own.go_back("here"), Some("x".to_owned()));
    }

    #[test]
    fn adopting_at_the_limit_drops_the_oldest_own_entries_and_keeps_where_the_user_came_from() {
        let mut own = SessionHistory {
            back: (0..10).map(|i| format!("o{}", i)).collect(),
            forward: vec![],
            last: Some("o9".to_owned()),
        };
        let handed_names: Vec<String> = (0..15).map(|i| format!("h{}", i)).collect();
        let handed = SessionHistory {
            back: handed_names.clone(),
            forward: vec![],
            last: handed_names.last().cloned(),
        };
        own.adopt(handed, "here");
        let mut expected: Vec<String> = (5..10).map(|i| format!("o{}", i)).collect();
        expected.extend(handed_names);
        assert_eq!(own.back, expected);
        assert_eq!(own.back.len(), SESSION_HISTORY_LIMIT);
        assert_eq!(own.last.as_deref(), Some("h14"));
        assert_eq!(own.go_back("here"), Some("h14".to_owned()));
    }

    #[test]
    fn adopting_at_the_limit_never_drops_where_the_user_came_from() {
        let mut own = SessionHistory {
            back: vec![],
            forward: vec![],
            last: None,
        };
        let handed = SessionHistory {
            back: vec!["from".to_owned()],
            forward: (0..25).map(|i| format!("f{}", i)).collect(),
            last: Some("from".to_owned()),
        };
        own.adopt(handed, "here");
        assert_eq!(own.back, vec!["from"]);
        assert_eq!(own.forward.len(), SESSION_HISTORY_LIMIT - 1);
        assert_eq!(own.forward.first().map(|s| s.as_str()), Some("f6"));
        assert_eq!(own.last.as_deref(), Some("from"));
    }

    #[test]
    fn an_adopted_history_stays_capped_without_losing_where_the_user_came_from() {
        let names: Vec<String> = (0..20).map(|i| format!("h{}", i)).collect();
        let handed = SessionHistory {
            back: names.clone(),
            forward: vec![],
            last: names.last().cloned(),
        };
        let mut own = SessionHistory {
            back: (0..20).map(|i| format!("o{}", i)).collect(),
            forward: vec![],
            last: Some("o19".to_owned()),
        };
        own.adopt(handed, "here");
        assert_eq!(own.back.len(), SESSION_HISTORY_LIMIT);
        assert_eq!(own.last.as_deref(), Some("h19"));
        assert_eq!(own.back, names);
        assert_eq!(own.back.last().map(|s| s.as_str()), Some("h19"));
    }

    #[test]
    fn info_never_lists_the_current_session() {
        let mut history = SessionHistory::default();
        visit(&mut history, &["a", "b", "a"]);
        let info = history.info("a");
        assert_eq!(info.previous.as_deref(), Some("b"));
        assert_eq!(info.back, vec!["b"]);
    }
}
