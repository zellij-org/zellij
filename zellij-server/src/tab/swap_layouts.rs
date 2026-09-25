use crate::panes::{FloatingPanes, TiledPanes};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;
use zellij_utils::{
    input::layout::{
        FloatingPaneLayout, LayoutConstraint, SwapFloatingLayout, SwapTiledLayout, TiledPaneLayout,
    },
    pane_size::{PaneGeom, Size},
};

#[derive(Clone, Debug, Default)]
pub struct SwapLayouts {
    swap_tiled_layouts: Vec<SwapTiledLayout>,
    swap_floating_layouts: Vec<SwapFloatingLayout>,
    current_floating_layout_position: usize,
    current_tiled_layout_position: usize,
    is_floating_damaged: bool,
    is_tiled_damaged: bool,
    display_area: Rc<RefCell<Size>>, // includes all panes (including eg. the status bar and tab bar in the default layout)
}

impl SwapLayouts {
    pub fn new(
        swap_layouts: (Vec<SwapTiledLayout>, Vec<SwapFloatingLayout>),
        display_area: Rc<RefCell<Size>>,
    ) -> Self {
        let display_area = display_area.clone();
        SwapLayouts {
            swap_tiled_layouts: swap_layouts.0,
            swap_floating_layouts: swap_layouts.1,
            is_floating_damaged: false,
            is_tiled_damaged: false,
            display_area,
            ..Default::default()
        }
    }
    pub fn set_base_layout(&mut self, layout: (TiledPaneLayout, Vec<FloatingPaneLayout>)) {
        let mut base_swap_tiled_layout = BTreeMap::new();
        let mut base_swap_floating_layout = BTreeMap::new();
        let tiled_panes_count = layout.0.pane_count();
        let floating_panes_count = layout.1.len();
        // we set ExactPanes to the current panes in the layout, because the base layout is not
        // intended to be progressive - i.e. to have additional panes added to it
        // we also don't want it to be applied for less than the expected amount of panes, because
        // then unintended things can happen
        // we still want to keep it around in case we'd like to swap layouts without adding panes
        base_swap_tiled_layout.insert(LayoutConstraint::ExactPanes(tiled_panes_count), layout.0);
        base_swap_floating_layout
            .insert(LayoutConstraint::ExactPanes(floating_panes_count), layout.1);
        self.swap_tiled_layouts
            .insert(0, (base_swap_tiled_layout, Some("BASE".into())));
        self.swap_floating_layouts
            .insert(0, (base_swap_floating_layout, Some("BASE".into())));
        self.current_tiled_layout_position = 0;
        self.current_floating_layout_position = 0;
    }
    pub fn set_swap_tiled_layouts(&mut self, new_swap_tiled_layouts: Vec<SwapTiledLayout>) {
        self.swap_tiled_layouts = new_swap_tiled_layouts;
        self.current_tiled_layout_position = 0;
    }
    pub fn set_swap_floating_layouts(
        &mut self,
        new_swap_floating_layouts: Vec<SwapFloatingLayout>,
    ) {
        self.swap_floating_layouts = new_swap_floating_layouts;
        self.current_floating_layout_position = 0;
    }
    pub fn set_is_floating_damaged(&mut self) {
        self.is_floating_damaged = true;
    }
    pub fn set_is_tiled_damaged(&mut self) {
        self.is_tiled_damaged = true;
    }
    pub fn reset_floating_damage(&mut self) {
        self.is_floating_damaged = false;
    }
    pub fn is_floating_damaged(&self) -> bool {
        self.is_floating_damaged
    }
    pub fn is_tiled_damaged(&self) -> bool {
        self.is_tiled_damaged
    }
    pub fn tiled_layout_info(&self) -> (Option<String>, bool) {
        // (swap_layout_name, is_swap_layout_dirty)
        match self
            .swap_tiled_layouts
            .iter()
            .nth(self.current_tiled_layout_position)
        {
            Some(current_tiled_layout) => (
                current_tiled_layout.1.clone().or_else(|| {
                    Some(format!(
                        "Layout #{}",
                        self.current_tiled_layout_position + 1
                    ))
                }),
                self.is_tiled_damaged,
            ),
            None => (None, self.is_tiled_damaged),
        }
    }
    pub fn floating_layout_info(&self) -> (Option<String>, bool) {
        // (swap_layout_name, is_swap_layout_dirty)
        match self
            .swap_floating_layouts
            .iter()
            .nth(self.current_floating_layout_position)
        {
            Some(current_floating_layout) => (
                current_floating_layout.1.clone().or_else(|| {
                    Some(format!(
                        "Layout #{}",
                        self.current_floating_layout_position + 1
                    ))
                }),
                self.is_floating_damaged,
            ),
            None => (None, self.is_floating_damaged),
        }
    }
    pub fn swap_floating_panes(
        &mut self,
        floating_panes: &FloatingPanes,
        search_backwards: bool,
    ) -> Option<Vec<FloatingPaneLayout>> {
        if self.swap_floating_layouts.is_empty() {
            return None;
        }
        let pane_count = floating_panes.visible_panes_count();
        let initial_position = self.current_floating_layout_position;

        macro_rules! progress_layout {
            () => {{
                if search_backwards {
                    if self.current_floating_layout_position == 0 {
                        self.current_floating_layout_position =
                            self.swap_floating_layouts.len().saturating_sub(1);
                    } else {
                        self.current_floating_layout_position -= 1;
                    }
                } else {
                    self.current_floating_layout_position += 1;
                }
            }};
        }

        if !self.is_floating_damaged
            && self
                .swap_floating_layouts
                .iter()
                .nth(self.current_floating_layout_position)
                .is_some()
        {
            progress_layout!();
        }
        self.is_floating_damaged = false;
        loop {
            match self
                .swap_floating_layouts
                .iter()
                .nth(self.current_floating_layout_position)
            {
                Some(swap_layout) => {
                    if let Some(layout) = self.floating_layout_candidate(swap_layout, pane_count) {
                        return Some(layout);
                    }
                    progress_layout!();
                },
                None => {
                    self.current_floating_layout_position = 0;
                },
            };
            if self.current_floating_layout_position == initial_position {
                break;
            }
        }
        None
    }
    fn state_fits_pane_count_constraint(constraint: &LayoutConstraint, pane_count: usize) -> bool {
        match constraint {
            LayoutConstraint::MaxPanes(max_panes) => pane_count <= *max_panes,
            LayoutConstraint::MinPanes(min_panes) => pane_count >= *min_panes,
            LayoutConstraint::ExactPanes(exact_panes) => pane_count == *exact_panes,
            LayoutConstraint::NoConstraint => true,
        }
    }
    fn tiled_layout_candidate(
        &self,
        swap_layout: &SwapTiledLayout,
        pane_count: usize,
    ) -> Option<TiledPaneLayout> {
        for (constraint, layout) in swap_layout.0.iter() {
            if Self::state_fits_pane_count_constraint(constraint, pane_count) {
                let focus_layout_if_not_focused = true;
                let display_area = self.display_area.borrow();
                // TODO: reuse the assets from position_panes_in_space here?
                let display_area = PaneGeom::from(&*display_area);
                if layout
                    .position_panes_in_space(
                        &display_area,
                        Some(pane_count),
                        false,
                        focus_layout_if_not_focused,
                    )
                    .is_ok()
                {
                    return Some(layout.clone());
                }
            };
        }
        None
    }
    fn floating_layout_candidate(
        &self,
        swap_layout: &SwapFloatingLayout,
        pane_count: usize,
    ) -> Option<Vec<FloatingPaneLayout>> {
        for (constraint, layout) in swap_layout.0.iter() {
            if Self::state_fits_pane_count_constraint(constraint, pane_count) {
                return Some(layout.clone());
            };
        }
        None
    }
    pub fn tiled_layout_candidate_by_name(
        &self,
        layout_name: &str,
        pane_count: usize,
    ) -> Option<(usize, TiledPaneLayout)> {
        self.swap_tiled_layouts
            .iter()
            .enumerate()
            .filter(|(_, layout)| layout.1.as_deref() == Some(layout_name))
            .find_map(|(position, layout)| {
                self.tiled_layout_candidate(layout, pane_count)
                    .map(|layout| (position, layout))
            })
    }
    pub fn floating_layout_candidate_by_name(
        &self,
        layout_name: &str,
        pane_count: usize,
    ) -> Option<(usize, Vec<FloatingPaneLayout>)> {
        self.swap_floating_layouts
            .iter()
            .enumerate()
            .filter(|(_, layout)| layout.1.as_deref() == Some(layout_name))
            .find_map(|(position, layout)| {
                self.floating_layout_candidate(layout, pane_count)
                    .map(|layout| (position, layout))
            })
    }
    pub fn set_current_tiled_layout_position(&mut self, position: usize) {
        self.current_tiled_layout_position = position;
        self.is_tiled_damaged = false;
    }
    pub fn set_current_floating_layout_position(&mut self, position: usize) {
        self.current_floating_layout_position = position;
        self.is_floating_damaged = false;
    }
    pub fn swap_tiled_panes(
        &mut self,
        tiled_panes: &TiledPanes,
        search_backwards: bool,
    ) -> Option<TiledPaneLayout> {
        if self.swap_tiled_layouts.is_empty() {
            return None;
        }
        let pane_count = tiled_panes.visible_panes_count();

        macro_rules! progress_layout {
            () => {{
                if search_backwards {
                    if self.current_tiled_layout_position == 0 {
                        self.current_tiled_layout_position =
                            self.swap_tiled_layouts.len().saturating_sub(1);
                    } else {
                        self.current_tiled_layout_position -= 1;
                    }
                } else {
                    self.current_tiled_layout_position += 1;
                }
            }};
        }

        let initial_position = self.current_tiled_layout_position;
        if !self.is_tiled_damaged
            && self
                .swap_tiled_layouts
                .iter()
                .nth(self.current_tiled_layout_position)
                .is_some()
        {
            progress_layout!();
        }
        self.is_tiled_damaged = false;
        loop {
            match self
                .swap_tiled_layouts
                .iter()
                .nth(self.current_tiled_layout_position)
            {
                Some(swap_layout) => {
                    if let Some(layout) = self.tiled_layout_candidate(swap_layout, pane_count) {
                        return Some(layout);
                    }
                    progress_layout!();
                },
                None => {
                    self.current_tiled_layout_position = 0;
                },
            };
            if self.current_tiled_layout_position == initial_position {
                break;
            }
        }
        None
    }
}
