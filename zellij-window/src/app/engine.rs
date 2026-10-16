use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::time::{Duration, Instant};

use zellij_utils::attention::{AttentionStatus, GlobalAttention, GlobalAttentionItem};
use zellij_utils::data::PaneId;
use zellij_utils::input::attention::DesktopNotifications;

pub const FIRST_SHOW_DELAY: Duration = Duration::from_millis(1500);
pub const WITHDRAW_GRACE: Duration = Duration::from_millis(2000);
pub const RATE_WINDOW: Duration = Duration::from_secs(10);
pub const RATE_LIMIT: usize = 4;
pub const GROUP_AT: usize = 3;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ItemKey {
    pub session: String,
    pub pane: PaneId,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum NotifyId {
    Item(ItemKey),
    Group(String),
}

impl NotifyId {
    #[cfg_attr(not(any(target_os = "macos", windows)), allow(dead_code))]
    pub fn session(&self) -> &str {
        match self {
            NotifyId::Item(key) => &key.session,
            NotifyId::Group(session) => session,
        }
    }

    pub fn encode(&self) -> String {
        match self {
            NotifyId::Item(key) => format!("item/{}/{}", key.pane, key.session),
            NotifyId::Group(session) => format!("group//{}", session),
        }
    }

    #[cfg_attr(not(any(target_os = "macos", test)), allow(dead_code))]
    pub fn decode(text: &str) -> Option<NotifyId> {
        let mut parts = text.splitn(3, '/');
        let kind = parts.next()?;
        let pane = parts.next()?;
        let session = parts.next()?.to_owned();
        match kind {
            "item" => Some(NotifyId::Item(ItemKey {
                session,
                pane: pane.parse().ok()?,
            })),
            "group" => Some(NotifyId::Group(session)),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Content {
    pub title: String,
    pub body: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Show { id: NotifyId, content: Content },
    Withdraw { id: NotifyId },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Policy {
    pub statuses: DesktopNotifications,
    pub details: bool,
    pub paused: bool,
    pub do_not_disturb: bool,
}

impl Default for Policy {
    fn default() -> Self {
        Policy {
            statuses: DesktopNotifications::default(),
            details: true,
            paused: false,
            do_not_disturb: false,
        }
    }
}

impl Policy {
    fn quiet(&self) -> bool {
        self.paused || self.do_not_disturb
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Eligible {
    content: Content,
    place: String,
    status: AttentionStatus,
}

pub fn content_of(item: &GlobalAttentionItem, details: bool) -> Content {
    let title = item.place();
    let status = item.item.attention.status;
    let text = item.text();
    let body = if details && !text.is_empty() {
        text
    } else {
        format!("{} {}", title, verb(status))
    };
    Content { title, body }
}

fn verb(status: AttentionStatus) -> &'static str {
    match status {
        AttentionStatus::NeedsYou => "needs you",
        AttentionStatus::Failed => "failed",
        AttentionStatus::Done => "is done",
        _ => "changed",
    }
}

#[derive(Debug, Default)]
pub struct Engine {
    started: bool,
    pending: BTreeMap<ItemKey, (Eligible, Instant)>,
    shown: BTreeMap<ItemKey, Eligible>,
    quiet: BTreeMap<ItemKey, Eligible>,
    grouped: BTreeMap<String, BTreeMap<ItemKey, Eligible>>,
    group_content: BTreeMap<String, Content>,
    group_sizes: BTreeMap<String, usize>,
    vanishing: BTreeMap<ItemKey, Instant>,
    shows: VecDeque<Instant>,
    current: BTreeMap<ItemKey, Eligible>,
    present: BTreeSet<ItemKey>,
    seen: BTreeSet<ItemKey>,
    policy: Policy,
}

impl Engine {
    pub fn new() -> Self {
        Engine::default()
    }

    pub fn update(
        &mut self,
        global: &GlobalAttention,
        policy: Policy,
        now: Instant,
    ) -> Vec<Command> {
        self.policy = policy;
        let mut current = BTreeMap::new();
        let mut present = BTreeSet::new();
        let mut seen = BTreeSet::new();
        for item in &global.items {
            let key = ItemKey {
                session: item.session.clone(),
                pane: item.item.pane_id,
            };
            present.insert(key.clone());
            let attention = &item.item.attention;
            if attention.seen {
                seen.insert(key.clone());
            }
            if attention.is_muted() || attention.seen || !policy.statuses.includes(attention.status)
            {
                continue;
            }
            current.insert(
                key,
                Eligible {
                    content: content_of(item, policy.details),
                    place: item.place(),
                    status: attention.status,
                },
            );
        }
        self.current = current;
        self.present = present;
        self.seen = seen;
        if !self.started {
            self.started = true;
            self.quiet = self.current.clone();
            return vec![];
        }
        self.reconcile(now)
    }

    pub fn set_policy(&mut self, policy: Policy, now: Instant) -> Vec<Command> {
        self.policy = policy;
        self.reconcile(now)
    }

    pub fn tick(&mut self, now: Instant) -> Vec<Command> {
        self.reconcile(now)
    }

    pub fn next_deadline(&self) -> Option<Instant> {
        let mut pending = self.pending.values().map(|(_, due)| *due).min();
        if self.shows.len() >= RATE_LIMIT {
            if let (Some(due), Some(first)) = (pending, self.shows.front()) {
                pending = Some(due.max(*first + RATE_WINDOW));
            }
        }
        let vanishing = self.vanishing.values().copied().min();
        [pending, vanishing].into_iter().flatten().min()
    }

    pub fn forget_dismissed(&mut self, id: &NotifyId) {
        match id {
            NotifyId::Item(key) => {
                self.vanishing.remove(key);
                if let Some(eligible) = self.shown.remove(key) {
                    self.quiet.insert(key.clone(), eligible);
                }
            },
            NotifyId::Group(session) => {
                if let Some(members) = self.grouped.remove(session) {
                    self.quiet.extend(members);
                }
                self.group_content.remove(session);
                self.group_sizes.remove(session);
            },
        }
    }

    #[cfg(test)]
    pub fn shown_ids(&self) -> Vec<NotifyId> {
        let mut ids: Vec<NotifyId> = self.shown.keys().cloned().map(NotifyId::Item).collect();
        ids.extend(self.group_content.keys().cloned().map(NotifyId::Group));
        ids
    }

    fn reconcile(&mut self, now: Instant) -> Vec<Command> {
        let mut commands = vec![];
        self.retire_shows(now);
        self.withdraw_gone(now, &mut commands);
        self.refresh_shown(&mut commands);
        self.refresh_groups(&mut commands);
        self.queue_new(now);
        self.flush_due(now, &mut commands);
        commands
    }

    fn retire_shows(&mut self, now: Instant) {
        while let Some(first) = self.shows.front() {
            if now.duration_since(*first) >= RATE_WINDOW {
                self.shows.pop_front();
            } else {
                break;
            }
        }
    }

    fn gone_now(&self, key: &ItemKey) -> bool {
        !self.present.contains(key) || self.seen.contains(key)
    }

    fn withdraw_gone(&mut self, now: Instant, commands: &mut Vec<Command>) {
        let shown: Vec<ItemKey> = self.shown.keys().cloned().collect();
        for key in shown {
            if self.current.contains_key(&key) {
                self.vanishing.remove(&key);
                continue;
            }
            let immediately = self.gone_now(&key);
            let due = *self
                .vanishing
                .entry(key.clone())
                .or_insert(now + WITHDRAW_GRACE);
            if immediately || now >= due {
                self.vanishing.remove(&key);
                self.shown.remove(&key);
                commands.push(Command::Withdraw {
                    id: NotifyId::Item(key),
                });
            }
        }
        self.pending.retain(|key, _| self.current.contains_key(key));
        let current = &self.current;
        self.quiet.retain(|key, _| current.contains_key(key));
        let sessions: Vec<String> = self.grouped.keys().cloned().collect();
        for session in sessions {
            let Some(members) = self.grouped.get_mut(&session) else {
                continue;
            };
            members.retain(|key, _| current.contains_key(key));
            if members.is_empty() {
                self.grouped.remove(&session);
                self.group_sizes.remove(&session);
                if self.group_content.remove(&session).is_some() {
                    commands.push(Command::Withdraw {
                        id: NotifyId::Group(session),
                    });
                }
            }
        }
    }

    fn refresh_shown(&mut self, commands: &mut Vec<Command>) {
        let keys: Vec<ItemKey> = self.shown.keys().cloned().collect();
        for key in keys {
            let Some(next) = self.current.get(&key).cloned() else {
                continue;
            };
            if self.shown.get(&key) != Some(&next) {
                if self.policy.quiet() {
                    self.shown.insert(key, next);
                    continue;
                }
                commands.push(Command::Show {
                    id: NotifyId::Item(key.clone()),
                    content: next.content.clone(),
                });
                self.shown.insert(key, next);
            }
        }
        let quiet: Vec<ItemKey> = self.quiet.keys().cloned().collect();
        for key in quiet {
            let Some(next) = self.current.get(&key).cloned() else {
                continue;
            };
            let previous = self.quiet.get(&key);
            if previous.map(|p| p.status) != Some(next.status) {
                self.quiet.remove(&key);
            } else {
                self.quiet.insert(key, next);
            }
        }
    }

    fn refresh_groups(&mut self, commands: &mut Vec<Command>) {
        let sessions: Vec<String> = self.grouped.keys().cloned().collect();
        for session in sessions {
            let Some(members) = self.grouped.get_mut(&session) else {
                continue;
            };
            for (key, eligible) in members.iter_mut() {
                if let Some(next) = self.current.get(key) {
                    *eligible = next.clone();
                }
            }
            let content = group_content(&session, members);
            let grew = self
                .group_sizes
                .get(&session)
                .map(|size| members.len() > *size)
                .unwrap_or(true);
            self.group_sizes.insert(session.clone(), members.len());
            if self.group_content.get(&session) != Some(&content) {
                if grew && !self.policy.quiet() {
                    commands.push(Command::Show {
                        id: NotifyId::Group(session.clone()),
                        content: content.clone(),
                    });
                }
                self.group_content.insert(session, content);
            }
        }
    }

    fn queue_new(&mut self, now: Instant) {
        let keys: Vec<ItemKey> = self.current.keys().cloned().collect();
        for key in keys {
            let known = self.shown.contains_key(&key)
                || self.quiet.contains_key(&key)
                || self
                    .grouped
                    .get(&key.session)
                    .map(|members| members.contains_key(&key))
                    .unwrap_or(false);
            if known {
                continue;
            }
            let Some(eligible) = self.current.get(&key).cloned() else {
                continue;
            };
            if self.policy.quiet() {
                self.pending.remove(&key);
                self.quiet.insert(key, eligible);
                continue;
            }
            match self.pending.get_mut(&key) {
                Some(entry) => entry.0 = eligible,
                None => {
                    self.pending.insert(key, (eligible, now + FIRST_SHOW_DELAY));
                },
            }
        }
    }

    fn flush_due(&mut self, now: Instant, commands: &mut Vec<Command>) {
        if self.policy.quiet() {
            return;
        }
        let due: Vec<ItemKey> = self
            .pending
            .iter()
            .filter(|(_, (_, at))| *at <= now)
            .map(|(key, _)| key.clone())
            .collect();
        if due.is_empty() {
            return;
        }
        let mut by_session: BTreeMap<String, Vec<ItemKey>> = BTreeMap::new();
        for key in due {
            by_session.entry(key.session.clone()).or_default().push(key);
        }
        for (session, keys) in by_session {
            let has_group = self.grouped.contains_key(&session);
            let crowded = keys.len() >= GROUP_AT;
            if has_group || crowded || self.shows.len() >= RATE_LIMIT {
                if !has_group && !crowded && keys.len() + self.shown_in(&session) < 2 {
                    continue;
                }
                if !has_group && self.shows.len() >= RATE_LIMIT {
                    continue;
                }
                self.group(&session, keys, now, commands);
                continue;
            }
            for key in keys {
                if self.shows.len() >= RATE_LIMIT {
                    break;
                }
                let Some((eligible, _)) = self.pending.remove(&key) else {
                    continue;
                };
                commands.push(Command::Show {
                    id: NotifyId::Item(key.clone()),
                    content: eligible.content.clone(),
                });
                self.shown.insert(key, eligible);
                self.shows.push_back(now);
            }
        }
    }

    fn shown_in(&self, session: &str) -> usize {
        self.shown
            .keys()
            .filter(|key| key.session == session)
            .count()
    }

    fn group(
        &mut self,
        session: &str,
        keys: Vec<ItemKey>,
        now: Instant,
        commands: &mut Vec<Command>,
    ) {
        let fresh_group = !self.group_content.contains_key(session);
        let mut members = self.grouped.remove(session).unwrap_or_default();
        for key in keys {
            if let Some((eligible, _)) = self.pending.remove(&key) {
                members.insert(key, eligible);
            }
        }
        let individually: Vec<ItemKey> = self
            .shown
            .keys()
            .filter(|key| key.session == session)
            .cloned()
            .collect();
        for key in individually {
            if let Some(eligible) = self.shown.remove(&key) {
                self.vanishing.remove(&key);
                commands.push(Command::Withdraw {
                    id: NotifyId::Item(key.clone()),
                });
                members.insert(key, eligible);
            }
        }
        let content = group_content(session, &members);
        commands.push(Command::Show {
            id: NotifyId::Group(session.to_owned()),
            content: content.clone(),
        });
        if fresh_group {
            self.shows.push_back(now);
        }
        self.group_content.insert(session.to_owned(), content);
        self.group_sizes.insert(session.to_owned(), members.len());
        self.grouped.insert(session.to_owned(), members);
    }
}

fn group_content(session: &str, members: &BTreeMap<ItemKey, Eligible>) -> Content {
    let count = members.len();
    let mut places: Vec<&Eligible> = members.values().collect();
    places.sort_by(|a, b| b.status.cmp(&a.status).then(a.place.cmp(&b.place)));
    let listed: Vec<String> = places.iter().take(4).map(|e| e.place.clone()).collect();
    let mut body = listed.join(", ");
    if count > listed.len() {
        body.push_str(&format!(" and {} more", count - listed.len()));
    }
    Content {
        title: format!(
            "{}: {} {}",
            zellij_utils::attention::sanitize_display_text(session),
            count,
            if count == 1 { "item" } else { "items" }
        ),
        body,
    }
}

pub fn target_of(id: &NotifyId, global: &GlobalAttention) -> (String, Option<PaneId>) {
    match id {
        NotifyId::Item(key) => (key.session.clone(), Some(key.pane)),
        NotifyId::Group(session) => {
            let pane = global
                .counted_items()
                .into_iter()
                .find(|item| &item.session == session)
                .map(|item| item.item.pane_id);
            (session.clone(), pane)
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::model::fixtures::*;

    fn policy() -> Policy {
        Policy::default()
    }

    fn shows(commands: &[Command]) -> Vec<NotifyId> {
        commands
            .iter()
            .filter_map(|c| match c {
                Command::Show { id, .. } => Some(id.clone()),
                _ => None,
            })
            .collect()
    }

    fn withdraws(commands: &[Command]) -> Vec<NotifyId> {
        commands
            .iter()
            .filter_map(|c| match c {
                Command::Withdraw { id } => Some(id.clone()),
                _ => None,
            })
            .collect()
    }

    fn key(session: &str, pane: u32) -> NotifyId {
        NotifyId::Item(ItemKey {
            session: session.to_owned(),
            pane: PaneId::Terminal(pane),
        })
    }

    fn started(now: Instant) -> Engine {
        let mut engine = Engine::new();
        assert!(engine
            .update(&global(vec![("b", vec![])]), policy(), now)
            .is_empty());
        engine
    }

    fn blocked(text: &str) -> zellij_utils::attention::GlobalAttention {
        global(vec![(
            "b",
            vec![item(1, AttentionStatus::NeedsYou, text, 10)],
        )])
    }

    #[test]
    fn items_present_when_the_app_starts_are_not_notified() {
        let now = Instant::now();
        let mut engine = Engine::new();
        assert!(engine.update(&blocked("x"), policy(), now).is_empty());
        assert!(engine.tick(now + Duration::from_secs(5)).is_empty());
    }

    #[test]
    fn a_new_item_is_shown_once_after_the_coalescing_delay() {
        let now = Instant::now();
        let mut engine = started(now);
        assert!(engine
            .update(&blocked("Allow edit?"), policy(), now)
            .is_empty());
        assert_eq!(engine.next_deadline(), Some(now + FIRST_SHOW_DELAY));
        let shown = engine.tick(now + FIRST_SHOW_DELAY);
        assert_eq!(
            shown,
            vec![Command::Show {
                id: key("b", 1),
                content: Content {
                    title: "b pane1".to_owned(),
                    body: "Allow edit?".to_owned(),
                },
            }]
        );
        assert!(engine
            .update(
                &blocked("Allow edit?"),
                policy(),
                now + Duration::from_secs(3)
            )
            .is_empty());
    }

    #[test]
    fn a_changed_item_replaces_its_notification_instead_of_stacking() {
        let now = Instant::now();
        let mut engine = started(now);
        engine.update(&blocked("first"), policy(), now);
        engine.tick(now + FIRST_SHOW_DELAY);
        let changed = engine.update(&blocked("second"), policy(), now + Duration::from_secs(3));
        assert_eq!(shows(&changed), vec![key("b", 1)]);
        assert!(withdraws(&changed).is_empty());
    }

    #[test]
    fn seeing_the_pane_withdraws_at_once_and_removal_too() {
        let now = Instant::now();
        let mut engine = started(now);
        engine.update(&blocked("x"), policy(), now);
        engine.tick(now + FIRST_SHOW_DELAY);
        let seen_global = global(vec![(
            "b",
            vec![seen(item(1, AttentionStatus::NeedsYou, "x", 10))],
        )]);
        let commands = engine.update(&seen_global, policy(), now + Duration::from_secs(2));
        assert_eq!(withdraws(&commands), vec![key("b", 1)]);

        let mut engine = started(now);
        engine.update(&blocked("x"), policy(), now);
        engine.tick(now + FIRST_SHOW_DELAY);
        let commands = engine.update(
            &global(vec![("b", vec![])]),
            policy(),
            now + Duration::from_secs(2),
        );
        assert_eq!(withdraws(&commands), vec![key("b", 1)]);
    }

    #[test]
    fn a_flicker_before_the_first_show_never_shows() {
        let now = Instant::now();
        let mut engine = started(now);
        engine.update(&blocked("x"), policy(), now);
        let working = global(vec![("b", vec![item(1, AttentionStatus::Working, "", 10)])]);
        assert!(engine
            .update(&working, policy(), now + Duration::from_millis(500))
            .is_empty());
        assert!(engine.tick(now + Duration::from_secs(5)).is_empty());
    }

    #[test]
    fn a_flicker_after_showing_keeps_the_notification_during_the_grace() {
        let now = Instant::now();
        let mut engine = started(now);
        engine.update(&blocked("x"), policy(), now);
        engine.tick(now + FIRST_SHOW_DELAY);
        let working = global(vec![("b", vec![item(1, AttentionStatus::Working, "", 10)])]);
        let t = now + Duration::from_secs(3);
        assert!(engine.update(&working, policy(), t).is_empty());
        assert!(engine
            .update(&blocked("x"), policy(), t + Duration::from_millis(500))
            .is_empty());
        engine.update(&working, policy(), t + Duration::from_secs(1));
        let commands = engine.tick(t + Duration::from_secs(1) + WITHDRAW_GRACE);
        assert_eq!(withdraws(&commands), vec![key("b", 1)]);
    }

    #[test]
    fn statuses_outside_the_setting_and_muted_items_are_never_notified() {
        let now = Instant::now();
        let mut engine = started(now);
        let quiet = global(vec![(
            "b",
            vec![
                item(1, AttentionStatus::Done, "", 10),
                item(2, AttentionStatus::Quiet, "", 10),
                item(3, AttentionStatus::NewOutput, "", 10),
                item(4, AttentionStatus::Working, "", 10),
                muted(item(5, AttentionStatus::NeedsYou, "", 10)),
                seen(item(6, AttentionStatus::NeedsYou, "", 10)),
            ],
        )]);
        engine.update(&quiet, policy(), now);
        assert!(engine.tick(now + Duration::from_secs(5)).is_empty());

        let mut engine = started(now);
        let all = Policy {
            statuses: DesktopNotifications::NeedsYouFailedDone,
            ..policy()
        };
        engine.update(&quiet, all, now);
        assert_eq!(
            shows(&engine.tick(now + FIRST_SHOW_DELAY)),
            vec![key("b", 1)]
        );
    }

    #[test]
    fn without_details_only_place_and_status_are_sent() {
        let now = Instant::now();
        let mut engine = started(now);
        let terse = Policy {
            details: false,
            ..policy()
        };
        let failed = global(vec![(
            "b",
            vec![item(1, AttentionStatus::Failed, "cargo test exited 1", 10)],
        )]);
        engine.update(&failed, terse, now);
        let commands = engine.tick(now + FIRST_SHOW_DELAY);
        assert_eq!(
            commands,
            vec![Command::Show {
                id: key("b", 1),
                content: Content {
                    title: "b pane1".to_owned(),
                    body: "b pane1 failed".to_owned(),
                },
            }]
        );
    }

    #[test]
    fn pause_suppresses_new_items_and_they_stay_quiet_after_resuming() {
        let now = Instant::now();
        let mut engine = started(now);
        let paused = Policy {
            paused: true,
            ..policy()
        };
        engine.update(&blocked("x"), paused, now);
        assert!(engine.tick(now + Duration::from_secs(5)).is_empty());
        assert!(engine
            .set_policy(policy(), now + Duration::from_secs(6))
            .is_empty());
        assert!(engine.tick(now + Duration::from_secs(9)).is_empty());
        let next = global(vec![(
            "b",
            vec![item(1, AttentionStatus::Failed, "boom", 10)],
        )]);
        engine.update(&next, policy(), now + Duration::from_secs(10));
        assert_eq!(
            shows(&engine.tick(now + Duration::from_secs(12))),
            vec![key("b", 1)]
        );
    }

    #[test]
    fn do_not_disturb_behaves_like_pause_but_still_withdraws() {
        let now = Instant::now();
        let mut engine = started(now);
        engine.update(&blocked("x"), policy(), now);
        engine.tick(now + FIRST_SHOW_DELAY);
        let dnd = Policy {
            do_not_disturb: true,
            ..policy()
        };
        let next = global(vec![
            ("b", vec![]),
            ("c", vec![item(1, AttentionStatus::NeedsYou, "y", 10)]),
        ]);
        let commands = engine.update(&next, dnd, now + Duration::from_secs(3));
        assert_eq!(withdraws(&commands), vec![key("b", 1)]);
        assert!(shows(&engine.tick(now + Duration::from_secs(8))).is_empty());
    }

    #[test]
    fn many_items_from_one_session_become_one_group() {
        let now = Instant::now();
        let mut engine = started(now);
        let crowd = global(vec![(
            "api",
            vec![
                item(1, AttentionStatus::NeedsYou, "a", 10),
                item(2, AttentionStatus::NeedsYou, "b", 10),
                item(3, AttentionStatus::Failed, "c", 10),
            ],
        )]);
        engine.update(&crowd, policy(), now);
        let commands = engine.tick(now + FIRST_SHOW_DELAY);
        assert_eq!(shows(&commands), vec![NotifyId::Group("api".to_owned())]);
        match &commands[0] {
            Command::Show { content, .. } => {
                assert_eq!(content.title, "api: 3 items");
                assert_eq!(content.body, "api pane1, api pane2, api pane3");
            },
            other => panic!("{:?}", other),
        }
        let fewer = global(vec![(
            "api",
            vec![item(1, AttentionStatus::NeedsYou, "a", 10)],
        )]);
        let commands = engine.update(&fewer, policy(), now + Duration::from_secs(3));
        assert!(
            commands.is_empty(),
            "a group that shrinks is not shown again: {:?}",
            commands
        );
        let none = global(vec![("api", vec![])]);
        let commands = engine.update(&none, policy(), now + Duration::from_secs(4));
        assert_eq!(
            withdraws(&commands),
            vec![NotifyId::Group("api".to_owned())]
        );
    }

    #[test]
    fn the_rate_limit_holds_further_shows_back() {
        let now = Instant::now();
        let mut engine = started(now);
        let sessions: Vec<(String, Vec<_>)> = (0..6)
            .map(|i| {
                (
                    format!("s{}", i),
                    vec![item(1, AttentionStatus::NeedsYou, "x", 10)],
                )
            })
            .collect();
        let many = global(
            sessions
                .iter()
                .map(|(name, items)| (name.as_str(), items.clone()))
                .collect(),
        );
        engine.update(&many, policy(), now);
        let first = engine.tick(now + FIRST_SHOW_DELAY);
        assert_eq!(shows(&first).len(), RATE_LIMIT);
        assert!(engine
            .tick(now + FIRST_SHOW_DELAY + Duration::from_secs(1))
            .is_empty());
        let deadline = engine.next_deadline().unwrap();
        assert_eq!(deadline, now + FIRST_SHOW_DELAY + RATE_WINDOW);
        let later = engine.tick(deadline);
        assert_eq!(shows(&later).len(), 2);
    }

    #[test]
    fn os_dismissal_is_forgotten_without_marking_anything_seen() {
        let now = Instant::now();
        let mut engine = started(now);
        engine.update(&blocked("x"), policy(), now);
        engine.tick(now + FIRST_SHOW_DELAY);
        engine.forget_dismissed(&key("b", 1));
        assert!(engine.shown_ids().is_empty());
        assert!(engine
            .update(&blocked("x"), policy(), now + Duration::from_secs(3))
            .is_empty());
        let failed = global(vec![("b", vec![item(1, AttentionStatus::Failed, "x", 10)])]);
        engine.update(&failed, policy(), now + Duration::from_secs(4));
        assert_eq!(
            shows(&engine.tick(now + Duration::from_secs(6))),
            vec![key("b", 1)]
        );
    }

    #[test]
    fn a_dismissal_during_the_withdraw_grace_leaves_no_past_deadline_behind() {
        let now = Instant::now();
        let mut engine = started(now);
        engine.update(&blocked("x"), policy(), now);
        engine.tick(now + FIRST_SHOW_DELAY);
        let working = global(vec![("b", vec![item(1, AttentionStatus::Working, "", 10)])]);
        let t = now + Duration::from_secs(3);
        assert!(engine.update(&working, policy(), t).is_empty());
        engine.forget_dismissed(&key("b", 1));
        let later = t + WITHDRAW_GRACE + Duration::from_secs(60);
        assert!(engine.tick(later).is_empty());
        let deadline = engine.next_deadline();
        assert!(
            deadline.map(|at| at > later).unwrap_or(true),
            "next_deadline stays {:?} before now after every tick",
            deadline.map(|at| later - at)
        );
    }

    #[test]
    fn notification_ids_survive_their_text_form() {
        for id in [
            key("api", 3),
            NotifyId::Item(ItemKey {
                session: "a/b c".to_owned(),
                pane: PaneId::Plugin(9),
            }),
            NotifyId::Group("api/x".to_owned()),
        ] {
            assert_eq!(NotifyId::decode(&id.encode()), Some(id));
        }
        assert_eq!(NotifyId::decode("nonsense"), None);
    }

    #[test]
    fn a_group_click_goes_to_its_top_item() {
        let crowd = global(vec![(
            "api",
            vec![
                item(1, AttentionStatus::Failed, "a", 10),
                item(2, AttentionStatus::NeedsYou, "b", 10),
            ],
        )]);
        assert_eq!(
            target_of(&NotifyId::Group("api".to_owned()), &crowd),
            ("api".to_owned(), Some(PaneId::Terminal(2)))
        );
        assert_eq!(
            target_of(&key("api", 1), &crowd),
            ("api".to_owned(), Some(PaneId::Terminal(1)))
        );
    }
}
