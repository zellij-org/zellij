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
        }
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

#[cfg(test)]
mod tests {
    use super::*;

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
    fn info_never_lists_the_current_session() {
        let mut history = SessionHistory::default();
        visit(&mut history, &["a", "b", "a"]);
        let info = history.info("a");
        assert_eq!(info.previous.as_deref(), Some("b"));
        assert_eq!(info.back, vec!["b"]);
    }
}
