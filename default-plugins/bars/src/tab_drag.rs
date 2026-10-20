use std::collections::BTreeMap;

use zellij_tile::prelude::actions::Action;
use zellij_tile::prelude::*;

pub const TAB_DRAG_CONTEXT_KEY: &str = "bars_tab_drag";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TabSegment {
    pub start: usize,
    pub end: usize,
    pub position: usize,
}

impl TabSegment {
    fn len(&self) -> usize {
        self.end.saturating_sub(self.start)
    }
    fn contains(&self, col: usize) -> bool {
        col >= self.start && col < self.end
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TabPart {
    Tab(usize),
    Marker { left: bool, position: usize },
}

pub fn tab_parts_in_line<'a>(
    parts: impl IntoIterator<Item = (usize, Option<usize>, &'a str)>,
    rendered_tabs: &[String],
    active_tab_position: usize,
) -> Vec<(usize, usize, TabPart)> {
    let mut found = vec![];
    let mut start = 0;
    for (len, tab_index, text) in parts {
        let end = start + len;
        if let Some(position) = tab_index {
            if len > 0 {
                let kind = if rendered_tabs.get(position).map(|t| t.as_str()) == Some(text) {
                    TabPart::Tab(position)
                } else {
                    TabPart::Marker {
                        left: position < active_tab_position,
                        position,
                    }
                };
                found.push((start, end, kind));
            }
        }
        start = end;
    }
    found
}

pub fn segments_of(parts: &[(usize, usize, TabPart)]) -> Vec<TabSegment> {
    parts
        .iter()
        .filter_map(|(start, end, kind)| match kind {
            TabPart::Tab(position) => Some(TabSegment {
                start: *start,
                end: *end,
                position: *position,
            }),
            TabPart::Marker { .. } => None,
        })
        .collect()
}

pub fn segment_at(segments: &[TabSegment], col: usize) -> Option<TabSegment> {
    segments.iter().copied().find(|s| s.contains(col))
}

pub fn drop_target(segments: &[TabSegment], dragged_position: usize, col: usize) -> Option<usize> {
    let dragged = segments.iter().find(|s| s.position == dragged_position)?;
    let hovered = segment_at(segments, col)?;
    let dragged_len = dragged.len();
    if hovered.position > dragged_position {
        if col >= hovered.end.saturating_sub(dragged_len) {
            Some(hovered.position)
        } else {
            None
        }
    } else if hovered.position < dragged_position {
        if col < hovered.start + dragged_len {
            Some(hovered.position)
        } else {
            None
        }
    } else {
        None
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReleaseOutcome {
    Nothing,
    SwitchTo(u32),
    SwitchWhenSettled(usize),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TabDrag {
    pub slot_id: Option<SlotId>,
    pub tab_id: usize,
    pub pending_target: Option<usize>,
    pub awaiting_layout: bool,
}

impl TabDrag {
    pub fn new(slot_id: Option<SlotId>, tab_id: usize) -> Self {
        TabDrag {
            slot_id,
            tab_id,
            pending_target: None,
            awaiting_layout: false,
        }
    }

    pub fn hold(
        &mut self,
        tabs: &[TabInfo],
        segments: &[TabSegment],
        col: usize,
    ) -> Option<(u64, usize)> {
        if self.pending_target.is_some() || self.awaiting_layout {
            return None;
        }
        let dragged_position = tabs.iter().find(|t| t.tab_id == self.tab_id)?.position;
        let target = drop_target(segments, dragged_position, col)?;
        self.pending_target = Some(target);
        Some((self.tab_id as u64, target))
    }

    pub fn tabs_updated(&mut self, tabs: &[TabInfo]) {
        let Some(target) = self.pending_target else {
            return;
        };
        let reached = tabs
            .iter()
            .find(|t| t.tab_id == self.tab_id)
            .map(|t| t.position == target)
            .unwrap_or(true);
        if reached {
            self.pending_target = None;
            self.awaiting_layout = true;
        }
    }

    pub fn move_completed(&mut self) -> bool {
        if self.pending_target.take().is_some() {
            self.awaiting_layout = true;
            true
        } else {
            false
        }
    }

    pub fn laid_out(&mut self) {
        if self.pending_target.is_none() {
            self.awaiting_layout = false;
        }
    }

    pub fn release(&self, tabs: &[TabInfo]) -> ReleaseOutcome {
        if self.pending_target.is_some() {
            return ReleaseOutcome::SwitchWhenSettled(self.tab_id);
        }
        switch_outcome(tabs, self.tab_id)
    }
}

pub fn switch_outcome(tabs: &[TabInfo], tab_id: usize) -> ReleaseOutcome {
    match tabs.iter().find(|t| t.tab_id == tab_id) {
        Some(tab) if !tab.active => ReleaseOutcome::SwitchTo(tab.position as u32 + 1),
        _ => ReleaseOutcome::Nothing,
    }
}

pub fn request_tab_move(tab_id: u64, position: usize) {
    let mut context = BTreeMap::new();
    context.insert(TAB_DRAG_CONTEXT_KEY.to_owned(), tab_id.to_string());
    run_action(
        Action::MoveTabToPosition {
            id: tab_id,
            position: position as u64,
        },
        context,
    );
}

pub fn is_tab_move_completion(event: &Event) -> bool {
    matches!(
        event,
        Event::ActionComplete(Action::MoveTabToPosition { .. }, _, context)
            if context.contains_key(TAB_DRAG_CONTEXT_KEY)
    )
}

pub fn apply_release(outcome: ReleaseOutcome) -> Option<usize> {
    match outcome {
        ReleaseOutcome::Nothing => None,
        ReleaseOutcome::SwitchTo(position) => {
            switch_tab_to(position);
            None
        },
        ReleaseOutcome::SwitchWhenSettled(tab_id) => Some(tab_id),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn segments(widths: &[usize]) -> Vec<TabSegment> {
        let mut start = 0;
        widths
            .iter()
            .enumerate()
            .map(|(position, width)| {
                let segment = TabSegment {
                    start,
                    end: start + width,
                    position,
                };
                start += width;
                segment
            })
            .collect()
    }

    fn tabs(ids: &[usize], active_id: usize) -> Vec<TabInfo> {
        ids.iter()
            .enumerate()
            .map(|(position, id)| TabInfo {
                position,
                tab_id: *id,
                active: *id == active_id,
                ..Default::default()
            })
            .collect()
    }

    #[test]
    fn a_narrow_tab_only_moves_past_a_wide_one_once_it_would_stay_under_the_mouse() {
        let segments = segments(&[4, 12]);
        assert_eq!(drop_target(&segments, 0, 2), None);
        assert_eq!(drop_target(&segments, 0, 5), None);
        assert_eq!(drop_target(&segments, 0, 11), None);
        assert_eq!(drop_target(&segments, 0, 12), Some(1));
        assert_eq!(drop_target(&segments, 0, 15), Some(1));
    }

    #[test]
    fn moving_left_requires_the_mouse_to_be_where_the_dragged_tab_would_land() {
        let segments = segments(&[12, 4]);
        assert_eq!(drop_target(&segments, 1, 13), None);
        assert_eq!(drop_target(&segments, 1, 4), None);
        assert_eq!(drop_target(&segments, 1, 3), Some(0));
        assert_eq!(drop_target(&segments, 1, 0), Some(0));
    }

    #[test]
    fn a_wide_tab_moves_as_soon_as_it_reaches_a_narrow_one() {
        let segments = segments(&[12, 4]);
        assert_eq!(drop_target(&segments, 0, 12), Some(1));
        let segments = super::tests::segments(&[4, 12]);
        assert_eq!(drop_target(&segments, 1, 3), Some(0));
    }

    #[test]
    fn after_a_move_the_mouse_is_over_the_dragged_tab_so_it_does_not_flip_back() {
        let before = segments(&[4, 12]);
        let col = 12;
        assert_eq!(drop_target(&before, 0, col), Some(1));
        let after = vec![
            TabSegment {
                start: 0,
                end: 12,
                position: 0,
            },
            TabSegment {
                start: 12,
                end: 16,
                position: 1,
            },
        ];
        assert_eq!(drop_target(&after, 1, col), None);
    }

    #[test]
    fn dragging_across_several_tabs_targets_the_tab_under_the_mouse() {
        let segments = segments(&[5, 5, 5, 5]);
        assert_eq!(drop_target(&segments, 0, 17), Some(3));
        assert_eq!(drop_target(&segments, 3, 1), Some(0));
    }

    #[test]
    fn nothing_happens_outside_tabs_or_for_a_hidden_dragged_tab() {
        let segments = segments(&[5, 5]);
        assert_eq!(drop_target(&segments, 0, 30), None);
        assert_eq!(drop_target(&segments, 4, 7), None);
    }

    #[test]
    fn a_drag_waits_for_its_move_and_a_new_layout_before_moving_again() {
        let segments = segments(&[5, 5, 5]);
        let mut tabs = tabs(&[10, 11, 12], 10);
        let mut drag = TabDrag::new(None, 10);
        assert_eq!(drag.hold(&tabs, &segments, 7), Some((10, 1)));
        assert_eq!(drag.hold(&tabs, &segments, 12), None);
        drag.tabs_updated(&tabs);
        assert_eq!(drag.pending_target, Some(1));
        tabs[0].position = 1;
        tabs[1].position = 0;
        drag.tabs_updated(&tabs);
        assert_eq!(drag.pending_target, None);
        assert_eq!(drag.hold(&tabs, &segments, 12), None);
        drag.laid_out();
        assert_eq!(drag.hold(&tabs, &segments, 12), Some((10, 2)));
    }

    #[test]
    fn a_finished_move_without_a_tab_update_unblocks_the_drag() {
        let segments = segments(&[5, 5]);
        let tabs = tabs(&[10, 11], 10);
        let mut drag = TabDrag::new(None, 10);
        assert!(!drag.move_completed());
        drag.hold(&tabs, &segments, 7);
        assert!(drag.move_completed());
        drag.laid_out();
        assert_eq!(drag.hold(&tabs, &segments, 7), Some((10, 1)));
    }

    #[test]
    fn releasing_switches_to_the_dragged_tab_unless_it_is_active() {
        let tabs = tabs(&[10, 11, 12], 10);
        assert_eq!(
            TabDrag::new(None, 10).release(&tabs),
            ReleaseOutcome::Nothing
        );
        assert_eq!(
            TabDrag::new(None, 12).release(&tabs),
            ReleaseOutcome::SwitchTo(3)
        );
        assert_eq!(
            TabDrag::new(None, 99).release(&tabs),
            ReleaseOutcome::Nothing
        );
    }

    #[test]
    fn releasing_during_a_move_switches_once_the_move_settles() {
        let segments = segments(&[5, 5]);
        let tabs = tabs(&[10, 11], 10);
        let mut drag = TabDrag::new(None, 11);
        drag.hold(&tabs, &segments, 2);
        assert_eq!(drag.release(&tabs), ReleaseOutcome::SwitchWhenSettled(11));
    }

    #[test]
    fn tab_parts_are_told_apart_from_hidden_tab_markers() {
        let rendered = vec!["T0".to_owned(), "T1".to_owned(), "T2".to_owned()];
        let line = vec![
            (3, None, "abc"),
            (2, Some(0), "<1"),
            (2, Some(1), "T1"),
            (0, Some(9), ""),
            (2, Some(2), ">1"),
        ];
        let parts = tab_parts_in_line(line, &rendered, 1);
        assert_eq!(
            parts,
            vec![
                (
                    3,
                    5,
                    TabPart::Marker {
                        left: true,
                        position: 0
                    }
                ),
                (5, 7, TabPart::Tab(1)),
                (
                    7,
                    9,
                    TabPart::Marker {
                        left: false,
                        position: 2
                    }
                ),
            ]
        );
        assert_eq!(
            segments_of(&parts),
            vec![TabSegment {
                start: 5,
                end: 7,
                position: 1
            }]
        );
    }

    #[test]
    fn only_tab_moves_requested_by_a_drag_count_as_completions() {
        let mut context = BTreeMap::new();
        context.insert(TAB_DRAG_CONTEXT_KEY.to_owned(), "3".to_owned());
        let action = Action::MoveTabToPosition { id: 3, position: 0 };
        assert!(is_tab_move_completion(&Event::ActionComplete(
            action.clone(),
            None,
            context
        )));
        assert!(!is_tab_move_completion(&Event::ActionComplete(
            action,
            None,
            BTreeMap::new()
        )));
    }
}
