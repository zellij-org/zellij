use super::stacked_panes::StackedPanes;
use crate::{panes::PaneId, tab::Pane};
use cassowary::{
    strength::{REQUIRED, STRONG},
    Expression, Solver, Variable,
    WeightedRelation::EQ,
};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use zellij_utils::{
    errors::prelude::*,
    input::layout::SplitDirection,
    pane_size::{Constraint, Dimension, PaneGeom},
};

pub struct PaneResizer<'a> {
    panes: Rc<RefCell<HashMap<PaneId, &'a mut Box<dyn Pane>>>>,
    vars: HashMap<PaneId, Variable>,
    solver: Solver,
}

// FIXME: Just hold a mutable Pane reference instead of the PaneId, fixed, pos, and size?
// Do this after panes are no longer trait-objects!
#[derive(Debug, Clone, Copy)]
struct Span {
    pid: PaneId,
    direction: SplitDirection,
    pos: usize,
    size: Dimension,
    size_var: Variable,
}

type Grid = Vec<Vec<Span>>;

impl<'a> PaneResizer<'a> {
    pub fn new(panes: Rc<RefCell<HashMap<PaneId, &'a mut Box<dyn Pane>>>>) -> Self {
        let mut vars = HashMap::new();
        for &pane_id in panes.borrow().keys() {
            vars.insert(pane_id, Variable::new());
        }
        PaneResizer {
            panes,
            vars,
            solver: Solver::new(),
        }
    }

    pub fn layout(&mut self, direction: SplitDirection, space: usize) -> Result<()> {
        self.solver.reset();
        let grid = self
            .solve(direction, space)
            .map_err(|err| anyhow!("{}", err))?;
        let spans = self
            .discretize_spans(grid, space)
            .map_err(|err| anyhow!("{}", err))?;

        if self.is_layout_valid(&spans) {
            self.apply_spans(spans)?;
        }
        Ok(())
    }

    fn solve(&mut self, direction: SplitDirection, space: usize) -> Result<Grid, String> {
        let grid: Grid = self
            .grid_boundaries(direction)
            .into_iter()
            .map(|b| self.spans_in_boundary(direction, b))
            .collect();

        let constraints: HashSet<_> = grid
            .iter()
            .flat_map(|s| constrain_spans(space, s))
            .collect();

        self.solver
            .add_constraints(&constraints)
            .map_err(|e| format!("{:?}", e))?;

        Ok(grid)
    }

    fn discretize_spans(&mut self, mut grid: Grid, space: usize) -> Result<Vec<Span>, String> {
        let mut rounded_sizes: HashMap<_, _> = grid
            .iter()
            .flatten()
            .map(|s| {
                (
                    s.size_var,
                    stable_round(self.solver.get_value(s.size_var)) as isize,
                )
            })
            .collect();

        // Round f64 pane sizes to usize without gaps or overlap
        let mut finalised = Vec::new();
        for spans in &mut grid {
            let rounded_size: isize = spans.iter().map(|s| rounded_sizes[&s.size_var]).sum();
            let mut error = space as isize - rounded_size;
            let mut flex_spans: Vec<_> = spans
                .iter_mut()
                .filter(|s| !s.size.is_fixed() && !finalised.contains(&s.pid))
                .collect();
            flex_spans.sort_by_key(|s| rounded_sizes[&s.size_var]);
            if error < 0 {
                flex_spans.reverse();
            }
            for span in flex_spans {
                rounded_sizes
                    .entry(span.size_var)
                    .and_modify(|s| *s += error.signum());
                error -= error.signum();
            }
            finalised.extend(spans.iter().map(|s| s.pid));
        }

        // Update span positions based on their rounded sizes
        for spans in &mut grid {
            let mut offset = 0;
            for span in spans {
                span.pos = offset;
                let sz = rounded_sizes[&span.size_var];
                if sz < 1 {
                    return Err("Ran out of room for spans".into());
                }
                span.size.set_inner(sz as usize);
                offset += span.size.as_usize();
            }
        }

        Ok(grid.into_iter().flatten().collect())
    }

    // HACK: This whole function is a bit of a hack — it's here to stop us from breaking the layout if we've been given
    // a bad state to start with. If this function returns false, nothing is resized.
    fn is_layout_valid(&self, spans: &[Span]) -> bool {
        // If pane stacks are too tall to fit on the screen, abandon ship before the status bar gets caught up in
        // any erroneous resizing...
        for span in spans {
            let pane_is_stacked = self
                .panes
                .borrow()
                .get(&span.pid)
                .unwrap()
                .current_geom()
                .is_stacked();
            if pane_is_stacked && span.direction == SplitDirection::Vertical {
                let min_stack_height = StackedPanes::new(self.panes.clone())
                    .min_stack_height(&span.pid)
                    .unwrap();
                if span.size.as_usize() < min_stack_height {
                    return false;
                }
            }
        }
        true
    }

    fn apply_spans(&mut self, spans: Vec<Span>) -> Result<()> {
        let err_context = || format!("Failed to apply spans");
        let mut geoms_changed = false;
        for span in spans {
            let pane_is_stacked = self
                .panes
                .borrow()
                .get(&span.pid)
                .unwrap()
                .current_geom()
                .is_stacked();
            if pane_is_stacked {
                let current_geom = StackedPanes::new(self.panes.clone())
                    .position_and_size_of_stack(&span.pid)
                    .unwrap();
                let new_geom = match span.direction {
                    SplitDirection::Horizontal => PaneGeom {
                        x: span.pos,
                        cols: span.size,
                        ..current_geom
                    },
                    SplitDirection::Vertical => PaneGeom {
                        y: span.pos,
                        rows: span.size,
                        ..current_geom
                    },
                };
                StackedPanes::new(self.panes.clone()).resize_panes_in_stack(&span.pid, new_geom)?;
                if new_geom.rows.as_usize() != current_geom.rows.as_usize()
                    || new_geom.cols.as_usize() != current_geom.cols.as_usize()
                {
                    geoms_changed = true;
                }
            } else {
                let mut panes = self.panes.borrow_mut();
                let pane = panes.get_mut(&span.pid).unwrap();
                let current_geom = pane.position_and_size();
                let new_geom = match span.direction {
                    SplitDirection::Horizontal => PaneGeom {
                        x: span.pos,
                        cols: span.size,
                        ..pane.current_geom()
                    },
                    SplitDirection::Vertical => PaneGeom {
                        y: span.pos,
                        rows: span.size,
                        ..pane.current_geom()
                    },
                };
                if new_geom.rows.as_usize() != current_geom.rows.as_usize()
                    || new_geom.cols.as_usize() != current_geom.cols.as_usize()
                {
                    geoms_changed = true;
                }
                if pane.geom_override().is_some() {
                    pane.set_geom_override(new_geom);
                } else {
                    pane.set_geom(new_geom);
                }
            }
        }
        if geoms_changed {
            Ok(())
        } else {
            // probably a rounding issue - this might be considered an error depending on who
            // called us - if it's an explicit resize operation, it's clearly an error (the user
            // wanted to resize and doesn't care about percentage rounding), if it's resizing the
            // terminal window as a whole, it might not be
            Err(ZellijError::PaneSizeUnchanged).with_context(err_context)
        }
    }

    // FIXME: Functions like this should have unit tests!
    fn grid_boundaries(&self, direction: SplitDirection) -> Vec<(usize, usize)> {
        // Select the spans running *perpendicular* to the direction of resize
        let spans: Vec<Span> = self
            .panes
            .borrow()
            .values()
            .filter_map(|p| self.get_span(!direction, p.as_ref()))
            .collect();

        let mut last_edge = 0;
        let mut bounds = Vec::new();
        let mut edges: Vec<usize> = spans.iter().map(|s| s.pos + s.size.as_usize()).collect();
        edges.sort_unstable();
        edges.dedup();
        for next in edges {
            let next_edge = next;
            bounds.push((last_edge, next_edge));
            last_edge = next_edge;
        }
        bounds
    }

    fn spans_in_boundary(&self, direction: SplitDirection, boundary: (usize, usize)) -> Vec<Span> {
        let bwn = |v, (s, e)| s <= v && v < e;
        let mut spans: Vec<_> = self
            .panes
            .borrow()
            .values()
            .filter(|p| match self.get_span(!direction, p.as_ref()) {
                Some(s) => {
                    let span_bounds = (s.pos, s.pos + s.size.as_usize());
                    bwn(span_bounds.0, boundary)
                        || (bwn(boundary.0, span_bounds)
                            && (bwn(boundary.1, span_bounds) || boundary.1 == span_bounds.1))
                },
                None => false,
            })
            .filter_map(|p| self.get_span(direction, p.as_ref()))
            .collect();
        spans.sort_unstable_by_key(|s| s.pos);
        spans
    }

    fn get_span(&self, direction: SplitDirection, pane: &dyn Pane) -> Option<Span> {
        let position_and_size = {
            let pas = pane.current_geom();
            if pas.is_stacked() && pas.rows.is_percent() {
                // this is the main pane of the stack
                StackedPanes::new(self.panes.clone()).position_and_size_of_stack(&pane.pid())
            } else if pas.is_stacked() {
                // this is a one-liner stacked pane and should be handled as the same rect with
                // the rest of the stack, represented by the main pane in the if branch above
                None
            } else {
                // non-stacked pane, treat normally
                Some(pas)
            }
        }?;
        let size_var = *self.vars.get(&pane.pid()).unwrap();
        match direction {
            SplitDirection::Horizontal => Some(Span {
                pid: pane.pid(),
                direction,
                pos: position_and_size.x,
                size: position_and_size.cols,
                size_var,
            }),
            SplitDirection::Vertical => Some(Span {
                pid: pane.pid(),
                direction,
                pos: position_and_size.y,
                size: position_and_size.rows,
                size_var,
            }),
        }
    }
}

fn constrain_spans(space: usize, spans: &[Span]) -> HashSet<cassowary::Constraint> {
    let mut constraints = HashSet::new();

    // Calculating "flexible" space (space not consumed by fixed-size spans)
    let new_flex_space = spans.iter().fold(space, |a, s| {
        if let Constraint::Fixed(sz) = s.size.constraint {
            a.saturating_sub(sz)
        } else {
            a
        }
    });

    // Spans must use all of the available space
    let full_size = spans
        .iter()
        .fold(Expression::from_constant(0.0), |acc, s| acc + s.size_var);
    constraints.insert(full_size.clone() | EQ(REQUIRED) | space as f64);

    // Try to maintain ratios and lock non-flexible sizes
    for span in spans {
        match span.size.constraint {
            Constraint::Fixed(s) => constraints.insert(span.size_var | EQ(REQUIRED) | s as f64),
            Constraint::Percent(p) => constraints
                .insert((span.size_var / new_flex_space as f64) | EQ(STRONG) | (p / 100.0)),
        };
    }

    constraints
}

fn stable_round(x: f64) -> f64 {
    ((x * 100.0).round() / 100.0).round()
}
