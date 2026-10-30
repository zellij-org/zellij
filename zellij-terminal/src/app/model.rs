use zellij_utils::attention::{AttentionStatus, GlobalAttention};
use zellij_utils::input::attention::{
    AttentionConfig, StatusMap, StatusSet, BADGE_STATUSES, DEFAULT_BADGE_STATUSES, POPUP_STATUSES,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Pill {
    pub status: AttentionStatus,
    pub color: [u8; 3],
    pub count: usize,
}

impl Pill {
    pub fn label(&self) -> String {
        if self.count > 9 {
            "9+".to_owned()
        } else {
            self.count.to_string()
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BadgeSettings {
    pub statuses: StatusSet,
    pub colors: StatusMap<[u8; 3]>,
}

impl Default for BadgeSettings {
    fn default() -> Self {
        BadgeSettings {
            statuses: StatusSet::of(&DEFAULT_BADGE_STATUSES),
            colors: StatusMap::default(),
        }
    }
}

impl BadgeSettings {
    pub fn of(config: &AttentionConfig) -> Self {
        BadgeSettings {
            statuses: config.tray_badge(),
            colors: config.tray_badge_colors,
        }
    }
    pub fn color(&self, status: AttentionStatus) -> [u8; 3] {
        self.colors
            .get(status)
            .unwrap_or_else(|| crate::app::icon::default_badge_color(status))
    }
}

pub fn count_of(global: &GlobalAttention, status: AttentionStatus) -> usize {
    let counts = &global.counts;
    match status {
        AttentionStatus::NeedsYou => counts.needs_you,
        AttentionStatus::Failed => counts.failed,
        AttentionStatus::Done => counts.done,
        AttentionStatus::Working => counts.working,
        AttentionStatus::Quiet => counts.quiet,
        AttentionStatus::NewOutput => counts.new_output,
        AttentionStatus::Idle => 0,
    }
}

pub fn pill(global: &GlobalAttention, badge: &BadgeSettings) -> Option<Pill> {
    BADGE_STATUSES
        .iter()
        .filter(|status| badge.statuses.includes(**status))
        .find_map(|status| {
            let count = count_of(global, *status);
            (count > 0).then(|| Pill {
                status: *status,
                color: badge.color(*status),
                count,
            })
        })
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TrayLook {
    pub pill: Option<Pill>,
    pub dimmed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MenuState {
    pub paused: bool,
    pub popups: StatusSet,
    pub badge: BadgeSettings,
}

impl Default for MenuState {
    fn default() -> Self {
        MenuState {
            paused: false,
            popups: StatusSet::of(&POPUP_STATUSES),
            badge: BadgeSettings::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrayState {
    pub look: TrayLook,
    pub tooltip: String,
    pub menu: MenuState,
}

pub fn tray_state(global: &GlobalAttention, menu: MenuState) -> TrayState {
    TrayState {
        look: TrayLook {
            pill: pill(global, &menu.badge),
            dimmed: menu.paused,
        },
        tooltip: tooltip(global, menu.paused, menu.badge.statuses),
        menu,
    }
}

pub fn tooltip(global: &GlobalAttention, paused: bool, badge: StatusSet) -> String {
    let mut parts = vec!["Zellij".to_owned()];
    let needs_you = global.counts.needs_you;
    if needs_you > 0 {
        parts.push(if needs_you == 1 {
            "1 needs you".to_owned()
        } else {
            format!("{} need you", needs_you)
        });
    }
    if global.counts.failed > 0 {
        parts.push(format!("{} failed", global.counts.failed));
    }
    if global.counts.done > 0 {
        parts.push(format!("{} done", global.counts.done));
    }
    if global.counts.working > 0 {
        parts.push(format!("{} working", global.counts.working));
    }
    if badge.includes(AttentionStatus::Quiet) && global.counts.quiet > 0 {
        parts.push(format!("{} quiet", global.counts.quiet));
    }
    if badge.includes(AttentionStatus::NewOutput) && global.counts.new_output > 0 {
        parts.push(format!("{} with new output", global.counts.new_output));
    }
    parts.push(format!("{} running", global.running()));
    if paused {
        parts.push("notifications paused".to_owned());
    }
    parts.join(" · ")
}

#[cfg(test)]
pub mod fixtures {
    use zellij_utils::attention::{
        AttentionCounts, AttentionItem, AttentionStatus, GlobalAttention, MuteScope, PaneAttention,
        SessionAttention,
    };
    use zellij_utils::data::PaneId;

    pub fn item(pane: u32, status: AttentionStatus, text: &str, since_ms: u64) -> AttentionItem {
        AttentionItem {
            pane_id: PaneId::Terminal(pane),
            tab_id: 0,
            tab_name: "tab".to_owned(),
            pane_title: format!("pane{}", pane),
            attention: PaneAttention {
                status,
                text: text.to_owned(),
                since_ms,
                changed_ms: since_ms,
                ..Default::default()
            },
        }
    }

    pub fn muted(mut item: AttentionItem) -> AttentionItem {
        item.attention.muted = Some(MuteScope::Pane);
        item
    }

    pub fn seen(mut item: AttentionItem) -> AttentionItem {
        item.attention.seen = true;
        item
    }

    pub fn session(items: Vec<AttentionItem>) -> SessionAttention {
        let mut counts = AttentionCounts::default();
        for item in &items {
            counts.add(&item.attention);
        }
        SessionAttention {
            status: counts.highest_counted(),
            counts,
            items,
            ..Default::default()
        }
    }

    pub fn global(sessions: Vec<(&str, Vec<AttentionItem>)>) -> GlobalAttention {
        GlobalAttention::build(
            "",
            sessions
                .into_iter()
                .map(|(name, items)| (name.to_owned(), session(items)))
                .collect(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::fixtures::*;
    use super::*;
    use zellij_utils::attention::AttentionStatus;

    fn status_of(global: &GlobalAttention, badge: &BadgeSettings) -> Option<(AttentionStatus, usize)> {
        pill(global, badge).map(|p| (p.status, p.count))
    }

    #[test]
    fn the_pill_mirrors_the_top_status_the_badge_shows() {
        let badge = BadgeSettings::default();
        let calm = global(vec![(
            "a",
            vec![
                item(3, AttentionStatus::Quiet, "", 1),
                item(4, AttentionStatus::NewOutput, "", 1),
            ],
        )]);
        assert_eq!(pill(&calm, &badge), None);
        let working = global(vec![("a", vec![item(1, AttentionStatus::Working, "", 1)])]);
        assert_eq!(
            status_of(&working, &badge),
            Some((AttentionStatus::Working, 1))
        );
        let done = global(vec![(
            "a",
            vec![
                item(1, AttentionStatus::Working, "", 1),
                item(2, AttentionStatus::Done, "", 1),
                item(3, AttentionStatus::Done, "", 1),
            ],
        )]);
        assert_eq!(status_of(&done, &badge), Some((AttentionStatus::Done, 2)));
        let seen_done = global(vec![(
            "a",
            vec![seen(item(1, AttentionStatus::Done, "", 1))],
        )]);
        assert_eq!(
            pill(&seen_done, &badge).map(|p| p.status),
            Some(AttentionStatus::Done)
        );
        let failed = global(vec![("a", vec![item(1, AttentionStatus::Failed, "", 1)])]);
        assert_eq!(
            pill(&failed, &badge),
            Some(Pill {
                status: AttentionStatus::Failed,
                color: crate::app::icon::ERROR_COLOR,
                count: 1
            })
        );
        let mixed = global(vec![
            ("a", vec![item(1, AttentionStatus::Failed, "", 1)]),
            (
                "b",
                vec![
                    item(1, AttentionStatus::NeedsYou, "", 1),
                    item(2, AttentionStatus::NeedsYou, "", 1),
                ],
            ),
        ]);
        assert_eq!(
            status_of(&mixed, &badge),
            Some((AttentionStatus::NeedsYou, 2))
        );
        let only_muted = global(vec![(
            "a",
            vec![muted(item(1, AttentionStatus::NeedsYou, "", 1))],
        )]);
        assert_eq!(pill(&only_muted, &badge), None);
    }

    #[test]
    fn the_badge_skips_turned_off_statuses_and_uses_chosen_colours() {
        let mixed = global(vec![(
            "a",
            vec![
                item(1, AttentionStatus::NeedsYou, "", 1),
                item(2, AttentionStatus::Quiet, "", 1),
                item(3, AttentionStatus::Quiet, "", 1),
            ],
        )]);
        let mut colors = StatusMap::default();
        colors.set(AttentionStatus::Quiet, Some([1, 2, 3]));
        let badge = BadgeSettings {
            statuses: StatusSet::of(&[AttentionStatus::Quiet]),
            colors,
        };
        assert_eq!(
            pill(&mixed, &badge),
            Some(Pill {
                status: AttentionStatus::Quiet,
                color: [1, 2, 3],
                count: 2
            })
        );
        let nothing = BadgeSettings {
            statuses: StatusSet::default(),
            colors,
        };
        assert_eq!(pill(&mixed, &nothing), None);
        assert_eq!(
            tooltip(&mixed, false, badge.statuses),
            "Zellij · 1 needs you · 2 quiet · 1 running"
        );
    }

    #[test]
    fn the_pill_label_stops_at_nine_plus() {
        let label = |count| {
            Pill {
                status: AttentionStatus::NeedsYou,
                color: [0, 0, 0],
                count,
            }
            .label()
        };
        assert_eq!(label(1), "1");
        assert_eq!(label(9), "9");
        assert_eq!(label(10), "9+");
        assert_eq!(label(250), "9+");
    }

    #[test]
    fn the_tooltip_counts_what_needs_you_and_what_runs() {
        let global = global(vec![
            (
                "a",
                vec![
                    item(1, AttentionStatus::NeedsYou, "", 1),
                    item(2, AttentionStatus::NeedsYou, "", 1),
                ],
            ),
            ("b", vec![]),
            ("c", vec![]),
            ("d", vec![]),
            ("e", vec![]),
        ]);
        let badge = BadgeSettings::default().statuses;
        assert_eq!(
            tooltip(&global, false, badge),
            "Zellij · 2 need you · 5 running"
        );
        let one =
            super::fixtures::global(vec![("a", vec![item(1, AttentionStatus::NeedsYou, "", 1)])]);
        assert_eq!(
            tooltip(&one, false, badge),
            "Zellij · 1 needs you · 1 running"
        );
        assert_eq!(
            tooltip(&GlobalAttention::default(), true, badge),
            "Zellij · 0 running · notifications paused"
        );
    }
}
