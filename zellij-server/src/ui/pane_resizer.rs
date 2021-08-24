use crate::{os_input_output::ServerOsApi, panes::PaneId, tab::Pane};
use cassowary::{
    strength::{REQUIRED, STRONG},
    Solver, Variable,
    WeightedRelation::*,
};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    ops::Not,
};
use zellij_utils::pane_size::{Constraint, Dimension, PaneGeom};

pub struct PaneResizer<'a> {
    panes: BTreeMap<&'a PaneId, &'a mut Box<dyn Pane>>,
    vars: BTreeMap<PaneId, (Variable, Variable)>,
    solver: Solver,
    os_api: &'a mut Box<dyn ServerOsApi>,
}

#[derive(Debug, Clone, Copy)]
pub enum Direction {
    Horizontal,
    Vertical,
}

impl Not for Direction {
    type Output = Self;

    fn not(self) -> Self::Output {
        match self {
            Direction::Horizontal => Direction::Vertical,
            Direction::Vertical => Direction::Horizontal,
        }
    }
}

// FIXME: Just hold a mutable Pane reference instead of the PaneId, fixed, pos, and size?
// Do this after panes are no longer trait-objects!
#[derive(Debug, Clone, Copy)]
struct Span {
    pid: PaneId,
    direction: Direction,
    pos: usize,
    size: Dimension,
    // FIXME: The solver shouldn't need to touch positions!
    pos_var: Variable,
    size_var: Variable,
}

// TODO: currently there are some functions here duplicated with Tab
// all resizing functions should move here

impl<'a> PaneResizer<'a> {
    // FIXME: Maybe find a way to just construct this once and use
    // solver.reset() before each call to resize. I'll likely fight the
    // borrow-checker and need to use something like Rc<RefCell> to have
    // multiple owners that can mutate it?
    pub fn new(
        panes: impl Iterator<Item = (&'a PaneId, &'a mut Box<dyn Pane>)>,
        os_api: &'a mut Box<dyn ServerOsApi>,
    ) -> Self {
        let panes: BTreeMap<_, _> = panes.collect();
        let mut vars = BTreeMap::new();
        for &&k in panes.keys() {
            vars.insert(k, (Variable::new(), Variable::new()));
        }
        log::info!("{}", "\n".repeat(5));
        for (pid, pane) in &panes {
            log::info!(
                "\n{:?}:\n\t{:?}\n\t{:?}",
                pid,
                pane.position_and_size(),
                pane.position_and_size_override()
            );
        }
        log::info!("{}", "\n".repeat(5));
        PaneResizer {
            panes,
            vars,
            solver: Solver::new(),
            os_api,
        }
    }

    // FIXME: Is this even a resize function even more? Should I call it
    // something like `(re)layout`?
    pub fn resize(&mut self, direction: Direction, size: usize) -> Option<usize> {
        // FIXME: This function shouldn't need a mutable reference? Honestly, it should just be a
        // function that creates the PaneResizer on-the-fly. Caller should just need
        // PaneResizer::resize(...)
        self.solver.reset();
        self.layout_direction(direction, size)?;
        // FIXME: Dumb return type, we just need a boolean to indicate success
        // or failure?
        Some(size)
    }

    fn layout_direction(&mut self, direction: Direction, new_size: usize) -> Option<()> {
        let spans = self.solve_direction(direction, new_size)?;
        for span in &spans {
            log::info!("ID: {:?}; Size: {}", span.pid, span.size.as_usize());
        }
        self.apply_spans(&spans);
        // FIXME: This is beyond stupid. I need to break this code up so this useless return isn't
        // needed... Maybe up in `resize`: solve -> discretize_spans -> apply_spans
        Some(())
    }

    fn solve_direction(&mut self, direction: Direction, space: usize) -> Option<Vec<Span>> {
        let mut grid = Vec::new();
        for boundary in self.grid_boundaries(direction) {
            log::info!("Boundary: {:?}", boundary);
            grid.push(self.spans_in_boundary(direction, boundary));
        }
        let dbg_grid: Vec<Vec<PaneId>> = grid
            .iter()
            .map(|r| r.iter().map(|s| s.pid).collect())
            .collect();
        log::info!("Grid: {:#?}\nSpace: {}", dbg_grid, space);

        let constraints: Vec<_> = grid
            .iter()
            .flat_map(|s| constrain_spans(space, s))
            .collect();

        // FIXME: This line needs to be restored before merging!
        //self.solver.add_constraints(&constraints).ok()?;
        self.solver.add_constraints(&constraints).unwrap();
        // FIXME: This chunk needs to be broken up into smaller functions!
        let mut rounded_sizes = HashMap::new();
        // FIXME: This should loop over something flattened, not be a nested loop
        for spans in &mut grid {
            for span in spans.iter_mut() {
                let size = self.solver.get_value(span.size_var);
                rounded_sizes.insert(span.size_var, size as isize);
            }
        }
        let mut finalised = Vec::new();
        for spans in &mut grid {
            let rounded_size: isize = spans.iter().map(|s| rounded_sizes[&s.size_var]).sum();
            let mut error = space as isize - rounded_size;
            let mut flex_spans: Vec<&mut Span> = spans
                .iter_mut()
                .filter(|s| !s.size.is_fixed() && !finalised.contains(&s.pid))
                .collect();
            // FIXME: Reverse the order when shrinking panes (to shrink the largest)
            flex_spans.sort_by_key(|s| rounded_sizes[&s.size_var]);
            if error < 0 {
                flex_spans.reverse();
            }
            log::info!("Finalised: {:?}", &finalised);
            for span in flex_spans {
                log::info!("Error: {}", error);
                // FIXME: If this causes errors, `i % flex_spans.len()`
                *rounded_sizes.get_mut(&span.size_var).unwrap() += error.signum();
                error -= error.signum();
            }
            finalised.extend(spans.iter().map(|s| s.pid));
        }
        for spans in &mut grid {
            let mut offset = 0;
            for span in spans.iter_mut() {
                span.pos = offset;
                let sz = rounded_sizes[&span.size_var];
                if sz < 1 {
                    return None;
                }
                span.size.set_inner(sz as usize);
                offset += span.size.as_usize();
            }
            if offset != space {
                log::error!("\n\n\nThe spans don't add up properly!\n\n\n");
                log::error!("Naughty: {:#?}", spans);
                panic!();
            }
        }
        Some(grid.into_iter().flatten().collect())
    }

    // FIXME: Functions like this should have unit tests!
    fn grid_boundaries(&self, direction: Direction) -> Vec<(usize, usize)> {
        // Select the spans running *perpendicular* to the direction of resize
        let spans: Vec<Span> = self
            .panes
            .values()
            .map(|p| self.get_span(!direction, p.as_ref()))
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

    fn spans_in_boundary(&self, direction: Direction, boundary: (usize, usize)) -> Vec<Span> {
        let bwn = |v, (s, e)| s <= v && v < e;
        let mut spans: Vec<_> = self
            .panes
            .values()
            .filter(|p| {
                let s = self.get_span(!direction, p.as_ref());
                let span_bounds = (s.pos, s.pos + s.size.as_usize());
                // FIXME: This needs some cleaning up! These conditions are ridiculous!
                bwn(span_bounds.0, boundary)
                    || (bwn(boundary.0, span_bounds)
                        && (bwn(boundary.1, span_bounds) || boundary.1 == span_bounds.1))
            })
            .map(|p| self.get_span(direction, p.as_ref()))
            .collect();
        spans.sort_unstable_by_key(|s| s.pos);
        spans
    }

    fn get_span(&self, direction: Direction, pane: &dyn Pane) -> Span {
        let pas = pane.current_geom();
        let (pos_var, size_var) = self.vars[&pane.pid()];
        match direction {
            Direction::Horizontal => Span {
                pid: pane.pid(),
                direction,
                pos: pas.x,
                size: pas.cols,
                pos_var,
                size_var,
            },
            Direction::Vertical => Span {
                pid: pane.pid(),
                direction,
                pos: pas.y,
                size: pas.rows,
                pos_var,
                size_var,
            },
        }
    }

    fn apply_spans(&mut self, spans: &[Span]) {
        for span in spans {
            log::info!("Applying span: {:#?}", span);
            let pane = self.panes.get_mut(&span.pid).unwrap();
            let new_geom = match span.direction {
                Direction::Horizontal => PaneGeom {
                    x: span.pos,
                    cols: span.size,
                    ..pane.current_geom()
                },
                Direction::Vertical => PaneGeom {
                    y: span.pos,
                    rows: span.size,
                    ..pane.current_geom()
                },
            };
            if pane.position_and_size_override().is_some() {
                // FIXME: This should really be called "set_geom_override"
                pane.override_size_and_position(new_geom);
            } else {
                // FIXME: This naming is really inconsistent... No "override", no "change", just
                // make it "set" at some point. This also takes a reference while the "override"
                // one takes ownership? Crazy inconsistent here.
                pane.change_pos_and_size(&new_geom);
            }
            if let PaneId::Terminal(pid) = pane.pid() {
                log::info!("Starting to set {:?} terminal size", pid);
                self.os_api.set_terminal_size_using_fd(
                    pid,
                    pane.get_content_columns() as u16,
                    pane.get_content_rows() as u16,
                );
                log::info!("Finished setting terminal size!");
            }
        }
    }
}

fn constrain_spans(space: usize, spans: &[Span]) -> HashSet<cassowary::Constraint> {
    let mut constraints = HashSet::new();

    // The first span needs to start at 0
    constraints.insert(spans[0].pos_var | EQ(REQUIRED) | 0.0);

    // Calculating "flexible" space (space not consumed by fixed-size spans)
    let new_flex_space = spans.iter().fold(space, |a, s| {
        if let Constraint::Fixed(sz) = s.size.constraint {
            a.saturating_sub(sz)
        } else {
            a
        }
    });

    // Keep spans stuck together
    for pair in spans.windows(2) {
        let (ls, rs) = (pair[0], pair[1]);
        constraints.insert((ls.pos_var + ls.size_var) | EQ(REQUIRED) | rs.pos_var);
    }

    // Try to maintain ratios and lock non-flexible sizes
    for span in spans {
        match span.size.constraint {
            Constraint::Fixed(s) => constraints.insert(span.size_var | EQ(REQUIRED) | s as f64),
            Constraint::Percent(p) => constraints
                .insert((span.size_var / new_flex_space as f64) | EQ(STRONG) | (p / 100.0)),
        };
    }

    // The last pane needs to end at the end of the space
    let last = spans.last().unwrap();
    constraints.insert((last.pos_var + last.size_var) | EQ(REQUIRED) | space as f64);

    constraints
}
