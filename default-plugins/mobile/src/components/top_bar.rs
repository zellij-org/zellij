use std::ops::Range;
use unicode_width::UnicodeWidthStr;
use zellij_tile::prelude::*;

use crate::ansi::pad_or_truncate;
use crate::click::{ClickAction, ClickRegion};
use crate::frame::Frame;
use crate::screens::ActiveScreen;
use crate::workspace::Workspace;

const PREFIX: &str = "Zellij ";
const HAMBURGER: &str = "\u{2630}";
const SESSION_PANE_SEP: &str = " ";

const HAMBURGER_SLOP_CELLS: usize = 3;

pub(crate) fn render(ws: &Workspace, frame: &mut Frame, active: ActiveScreen, row: usize, cols: usize) {
    let pane_name = current_pane_name(ws);
    let session_name = ws.session_name.clone();
    let content_max = cols.saturating_sub(HAMBURGER_SLOP_CELLS + width(HAMBURGER));
    let plan = Plan::compute(
        session_name.as_deref().map(width),
        width(&pane_name),
        width(PREFIX),
        width(SESSION_PANE_SEP),
        content_max,
    );

    let mut bar = BarBuilder::default();
    if plan.show_prefix {
        bar.push(PREFIX);
    }

    let session_seg = session_name
        .as_ref()
        .filter(|_| plan.session_target > 0)
        .map(|session| {
            let seg = bar.push(&pad_or_truncate(session, plan.session_target));
            bar.push(SESSION_PANE_SEP);
            seg
        });

    let pane_seg = bar.push(&pad_or_truncate(&pane_name, plan.pane_target));
    let pane_end = bar.cells;

    let pad = cols.saturating_sub(bar.cells + width(HAMBURGER)).max(HAMBURGER_SLOP_CELLS);
    bar.push(&" ".repeat(pad));

    let hamburger_start = bar.cells;
    let hamburger_seg = bar.push(HAMBURGER);

    paint(&bar.text, row, cols, &pane_seg, &hamburger_seg, session_seg.as_ref());

    let (pane_action, session_action) = actions_for(active);
    let session_cells = session_seg.map(|s| (s.cells.start, s.cells.end));
    for region in click_regions(row, cols, pane_end, hamburger_start, pane_action, session_cells, session_action) {
        frame.click_regions.push(region);
    }
}

fn paint(
    bar: &str,
    row: usize,
    cols: usize,
    pane_seg: &Segment,
    hamburger_seg: &Segment,
    session_seg: Option<&Segment>,
) {
    let mut text = Text::new(bar)
        .selected()
        .color_range(2, pane_seg.chars.clone())
        .color_range(3, hamburger_seg.chars.clone());
    if let Some(seg) = session_seg {
        text = text.color_range(0, seg.chars.clone());
    }
    print_text_with_coordinates(text, 0, row, Some(cols), None);
}

fn actions_for(active: ActiveScreen) -> (ClickAction, ClickAction) {
    if active == ActiveScreen::Viewport {
        (ClickAction::ExpandPanes, ClickAction::ExpandSessions)
    } else {
        (ClickAction::CollapseSelector, ClickAction::CollapseSelector)
    }
}

fn current_pane_name(ws: &Workspace) -> String {
    ws.current_pane()
        .map(|p| {
            if p.title.is_empty() {
                format!("#{}", p.id)
            } else {
                p.title.clone()
            }
        })
        .unwrap_or_else(|| "—".to_string())
}

fn width(s: &str) -> usize {
    UnicodeWidthStr::width(s)
}

#[derive(Default)]
struct BarBuilder {
    text: String,
    chars: usize,
    cells: usize,
}

struct Segment {
    chars: Range<usize>,
    cells: Range<usize>,
}

impl BarBuilder {
    fn push(&mut self, s: &str) -> Segment {
        let chars = self.chars..self.chars + s.chars().count();
        let cells = self.cells..self.cells + width(s);
        self.text.push_str(s);
        self.chars = chars.end;
        self.cells = cells.end;
        Segment { chars, cells }
    }
}

struct Plan {
    show_prefix: bool,
    session_target: usize,
    pane_target: usize,
}

impl Plan {
    fn compute(session_w: Option<usize>, pane_w: usize, prefix_w: usize, sep_w: usize, content_max: usize) -> Self {
        let Some(session_w) = session_w else {
            return Plan {
                show_prefix: true,
                session_target: 0,
                pane_target: pane_w.min(content_max.saturating_sub(prefix_w)),
            };
        };
        if prefix_w + session_w + sep_w + pane_w <= content_max {
            return Plan { show_prefix: true, session_target: session_w, pane_target: pane_w };
        }
        if session_w + sep_w + pane_w <= content_max {
            return Plan { show_prefix: false, session_target: session_w, pane_target: pane_w };
        }
        let available = content_max.saturating_sub(sep_w);
        let half = available / 2;
        let (session_target, pane_target) = if session_w <= half {
            (session_w, available.saturating_sub(session_w))
        } else if pane_w <= half {
            (available.saturating_sub(pane_w), pane_w)
        } else {
            (half, available.saturating_sub(half))
        };
        Plan { show_prefix: false, session_target, pane_target }
    }
}

pub fn click_regions(
    row: usize,
    cols: usize,
    pane_end: usize,
    hamburger_start: usize,
    pane_action: ClickAction,
    session_cells: Option<(usize, usize)>,
    session_action: ClickAction,
) -> Vec<ClickRegion> {
    let mut regions = Vec::with_capacity(5);
    push_content_regions(&mut regions, row, pane_end, session_cells, pane_action, session_action);

    regions.push(ClickRegion::tight(row, hamburger_start, cols, ClickAction::ToggleMenu));
    let hamburger_center = (hamburger_start.min(cols.saturating_sub(1)), row);
    regions.push(ClickRegion::slop(row, pane_end, cols, ClickAction::ToggleMenu, hamburger_center));
    regions
}

fn push_content_regions(
    regions: &mut Vec<ClickRegion>,
    row: usize,
    pane_end: usize,
    session_cells: Option<(usize, usize)>,
    pane_action: ClickAction,
    session_action: ClickAction,
) {
    let Some((session_start, session_end)) = clamp_session(session_cells, pane_end) else {
        if pane_end > 0 {
            regions.push(ClickRegion::tight(row, 0, pane_end, pane_action));
        }
        return;
    };
    if session_start > 0 {
        regions.push(ClickRegion::tight(row, 0, session_start, pane_action.clone()));
    }
    regions.push(ClickRegion::tight(row, session_start, session_end, session_action));
    if session_end < pane_end {
        regions.push(ClickRegion::tight(row, session_end, pane_end, pane_action));
    }
}

fn clamp_session(session_cells: Option<(usize, usize)>, pane_end: usize) -> Option<(usize, usize)> {
    let (start, end) = session_cells?;
    let (start, end) = (start.min(pane_end), end.min(pane_end));
    (start < end).then_some((start, end))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::State;

    #[test]
    fn partition_with_slop() {
        let cols = 80;
        let pane_end = 40;
        let hamburger_start = 79;
        let regions = click_regions(
            0,
            cols,
            pane_end,
            hamburger_start,
            ClickAction::ExpandPanes,
            None,
            ClickAction::ExpandSessions,
        );

        assert_eq!(regions.len(), 3);
        assert!(matches!(regions[0].action, ClickAction::ExpandPanes));
        assert_eq!(regions[0].priority, 0);
        assert_eq!(regions[0].col_start, 0);
        assert_eq!(regions[0].col_end, pane_end);
        assert!(matches!(regions[1].action, ClickAction::ToggleMenu));
        assert_eq!(regions[1].priority, 0);
        assert_eq!(regions[1].col_start, hamburger_start);
        assert_eq!(regions[1].col_end, cols);
        assert!(matches!(regions[2].action, ClickAction::ToggleMenu));
        assert_eq!(regions[2].priority, 1);
        assert_eq!(regions[2].col_start, pane_end);
        assert_eq!(regions[2].col_end, cols);

        let mut state = State::default();
        state.frame.click_regions = regions.clone();
        assert_eq!(state.frame.click_to_action(0, 0), Some(ClickAction::ExpandPanes));
        assert_eq!(
            state.frame.click_to_action(0, pane_end + 5),
            Some(ClickAction::ToggleMenu),
        );
        assert_eq!(state.frame.click_to_action(0, hamburger_start), Some(ClickAction::ToggleMenu));
    }

    #[test]
    fn pane_action_collapses_in_selector_mode() {
        let regions = click_regions(
            0,
            80,
            40,
            79,
            ClickAction::CollapseSelector,
            None,
            ClickAction::CollapseSelector,
        );
        assert!(matches!(regions[0].action, ClickAction::CollapseSelector));
        let mut state = State::default();
        state.frame.click_regions = regions;
        assert_eq!(state.frame.click_to_action(0, 0), Some(ClickAction::CollapseSelector));
    }

    #[test]
    fn session_sub_region_dispatches_expand_sessions() {
        let regions = click_regions(
            0,
            80,
            17,
            79,
            ClickAction::ExpandPanes,
            Some((7, 11)),
            ClickAction::ExpandSessions,
        );

        assert_eq!(regions.len(), 5);
        assert_eq!((regions[0].col_start, regions[0].col_end), (0, 7));
        assert!(matches!(regions[0].action, ClickAction::ExpandPanes));
        assert_eq!((regions[1].col_start, regions[1].col_end), (7, 11));
        assert!(matches!(regions[1].action, ClickAction::ExpandSessions));
        assert_eq!((regions[2].col_start, regions[2].col_end), (11, 17));
        assert!(matches!(regions[2].action, ClickAction::ExpandPanes));

        let mut state = State::default();
        state.frame.click_regions = regions;
        assert_eq!(state.frame.click_to_action(0, 3), Some(ClickAction::ExpandPanes));
        assert_eq!(state.frame.click_to_action(0, 9), Some(ClickAction::ExpandSessions));
        assert_eq!(state.frame.click_to_action(0, 11), Some(ClickAction::ExpandPanes));
        assert_eq!(state.frame.click_to_action(0, 14), Some(ClickAction::ExpandPanes));
        assert_eq!(state.frame.click_to_action(0, 30), Some(ClickAction::ToggleMenu));
        assert_eq!(state.frame.click_to_action(0, 79), Some(ClickAction::ToggleMenu));
    }

    #[test]
    fn session_at_left_edge_skips_empty_prefix_region() {
        let regions = click_regions(
            0,
            40,
            11,
            39,
            ClickAction::ExpandPanes,
            Some((0, 4)),
            ClickAction::ExpandSessions,
        );

        assert_eq!(regions.len(), 4);
        assert_eq!((regions[0].col_start, regions[0].col_end), (0, 4));
        assert!(matches!(regions[0].action, ClickAction::ExpandSessions));
        assert_eq!((regions[1].col_start, regions[1].col_end), (4, 11));
        assert!(matches!(regions[1].action, ClickAction::ExpandPanes));
    }
}
