use std::collections::HashSet;

use crate::panes::PaneId;

/// The tiled panes that are left out of the layout and not rendered.
///
/// A pane can be hidden for two separate reasons, and each is tracked on its own so that one
/// cannot undo the other:
/// - it is beneath a fullscreen pane, which lasts until fullscreen is unset
/// - it is covered, eg. a plugin that collapsed itself to give its space back to its
///   neighbors, which lasts until it is uncovered, across entering and leaving fullscreen
#[derive(Debug, Default)]
pub struct PanesToHide {
    fullscreen_pane: Option<PaneId>,
    beneath_fullscreen: HashSet<PaneId>,
    covered: HashSet<PaneId>,
}

impl PanesToHide {
    pub fn contains(&self, pane_id: &PaneId) -> bool {
        self.beneath_fullscreen.contains(pane_id)
            || (self.covered.contains(pane_id) && self.fullscreen_pane != Some(*pane_id))
    }
    pub fn len(&self) -> usize {
        let covered_and_not_beneath_fullscreen = self
            .covered
            .iter()
            .filter(|pane_id| {
                !self.beneath_fullscreen.contains(pane_id)
                    && self.fullscreen_pane != Some(**pane_id)
            })
            .count();
        self.beneath_fullscreen.len() + covered_and_not_beneath_fullscreen
    }
    /// The fullscreen pane itself is never hidden, even if it is also covered.
    pub fn set_fullscreen(&mut self, fullscreen_pane: PaneId, beneath: HashSet<PaneId>) {
        self.fullscreen_pane = Some(fullscreen_pane);
        self.beneath_fullscreen = beneath;
    }
    /// Returns the panes that were beneath the fullscreen pane, so they can be rendered again.
    /// Covered panes stay hidden.
    pub fn unset_fullscreen(&mut self) -> HashSet<PaneId> {
        self.fullscreen_pane = None;
        std::mem::take(&mut self.beneath_fullscreen)
    }
    /// Returns whether this changed anything.
    pub fn set_covered(&mut self, pane_id: PaneId) -> bool {
        self.covered.insert(pane_id)
    }
    /// Returns whether this changed anything.
    pub fn unset_covered(&mut self, pane_id: &PaneId) -> bool {
        self.covered.remove(pane_id)
    }
    pub fn is_covered(&self, pane_id: &PaneId) -> bool {
        self.covered.contains(pane_id)
    }
    /// Uncover every covered pane and return them, to be covered again with `restore_covered`.
    pub fn take_covered(&mut self) -> HashSet<PaneId> {
        std::mem::take(&mut self.covered)
    }
    pub fn restore_covered(&mut self, covered: HashSet<PaneId>) {
        self.covered.extend(covered);
    }
    /// Drop every trace of a pane that is leaving, so that a pane id that is handed out again
    /// does not arrive already hidden.
    pub fn forget(&mut self, pane_id: &PaneId) {
        self.beneath_fullscreen.remove(pane_id);
        self.covered.remove(pane_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn covered_pane_stays_hidden_after_fullscreen_is_unset() {
        let mut panes_to_hide = PanesToHide::default();
        panes_to_hide.set_covered(PaneId::Plugin(1));
        panes_to_hide.set_fullscreen(
            PaneId::Terminal(1),
            HashSet::from([PaneId::Terminal(2), PaneId::Plugin(1)]),
        );
        assert_eq!(panes_to_hide.len(), 2);
        let beneath = panes_to_hide.unset_fullscreen();
        assert_eq!(beneath.len(), 2);
        assert!(panes_to_hide.contains(&PaneId::Plugin(1)));
        assert!(!panes_to_hide.contains(&PaneId::Terminal(2)));
        assert_eq!(panes_to_hide.len(), 1);
    }

    #[test]
    fn fullscreen_pane_is_not_hidden_even_if_covered() {
        let mut panes_to_hide = PanesToHide::default();
        panes_to_hide.set_covered(PaneId::Terminal(1));
        panes_to_hide.set_fullscreen(PaneId::Terminal(1), HashSet::from([PaneId::Terminal(2)]));
        assert!(!panes_to_hide.contains(&PaneId::Terminal(1)));
        assert_eq!(panes_to_hide.len(), 1);
        panes_to_hide.unset_fullscreen();
        assert!(panes_to_hide.contains(&PaneId::Terminal(1)));
    }

    #[test]
    fn forgotten_pane_is_no_longer_hidden() {
        let mut panes_to_hide = PanesToHide::default();
        panes_to_hide.set_covered(PaneId::Plugin(1));
        panes_to_hide.set_fullscreen(PaneId::Terminal(1), HashSet::from([PaneId::Terminal(2)]));
        panes_to_hide.forget(&PaneId::Plugin(1));
        panes_to_hide.forget(&PaneId::Terminal(2));
        assert_eq!(panes_to_hide.len(), 0);
        assert!(!panes_to_hide.is_covered(&PaneId::Plugin(1)));
    }
}
