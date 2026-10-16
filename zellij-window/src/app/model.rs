use zellij_utils::attention::GlobalAttention;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PillColor {
    Attention,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Pill {
    pub color: PillColor,
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

pub fn pill(global: &GlobalAttention) -> Option<Pill> {
    if global.counts.needs_you > 0 {
        return Some(Pill {
            color: PillColor::Attention,
            count: global.counts.needs_you,
        });
    }
    if global.counts.failed > 0 {
        return Some(Pill {
            color: PillColor::Error,
            count: global.counts.failed,
        });
    }
    None
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TrayLook {
    pub pill: Option<Pill>,
    pub dimmed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrayState {
    pub look: TrayLook,
    pub tooltip: String,
    pub paused: bool,
}

pub fn tray_state(global: &GlobalAttention, paused: bool) -> TrayState {
    TrayState {
        look: TrayLook {
            pill: pill(global),
            dimmed: paused,
        },
        tooltip: tooltip(global, paused),
        paused,
    }
}

pub fn tooltip(global: &GlobalAttention, paused: bool) -> String {
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

    #[test]
    fn the_pill_mirrors_the_top_counted_status_only_for_needs_you_and_failed() {
        let calm = global(vec![(
            "a",
            vec![
                item(1, AttentionStatus::Done, "", 1),
                item(2, AttentionStatus::Working, "", 1),
                item(3, AttentionStatus::Quiet, "", 1),
                item(4, AttentionStatus::NewOutput, "", 1),
            ],
        )]);
        assert_eq!(pill(&calm), None);
        let failed = global(vec![("a", vec![item(1, AttentionStatus::Failed, "", 1)])]);
        assert_eq!(
            pill(&failed),
            Some(Pill {
                color: PillColor::Error,
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
            pill(&mixed),
            Some(Pill {
                color: PillColor::Attention,
                count: 2
            })
        );
        let only_muted = global(vec![(
            "a",
            vec![muted(item(1, AttentionStatus::NeedsYou, "", 1))],
        )]);
        assert_eq!(pill(&only_muted), None);
    }

    #[test]
    fn the_pill_label_stops_at_nine_plus() {
        let label = |count| {
            Pill {
                color: PillColor::Attention,
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
        assert_eq!(tooltip(&global, false), "Zellij · 2 need you · 5 running");
        let one =
            super::fixtures::global(vec![("a", vec![item(1, AttentionStatus::NeedsYou, "", 1)])]);
        assert_eq!(tooltip(&one, false), "Zellij · 1 needs you · 1 running");
        assert_eq!(
            tooltip(&GlobalAttention::default(), true),
            "Zellij · 0 running · notifications paused"
        );
    }
}
