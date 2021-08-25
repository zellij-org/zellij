use crate::{os_input_output::ServerOsApi, panes::PaneId, tab::Pane};
use cassowary::{
    strength::{REQUIRED, STRONG},
    Expression, Solver, Variable,
    WeightedRelation::EQ,
};
use std::{
    collections::{HashMap, HashSet},
    ops::Not,
};
use zellij_utils::pane_size::{Constraint, Dimension, PaneGeom};

pub struct PaneResizer<'a> {
    panes: HashMap<&'a PaneId, &'a mut Box<dyn Pane>>,
    os_api: &'a mut Box<dyn ServerOsApi>,
    vars: HashMap<PaneId, Variable>,
    solver: Solver,
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
    size_var: Variable,
}

impl<'a> PaneResizer<'a> {
    pub fn new(
        panes: impl Iterator<Item = (&'a PaneId, &'a mut Box<dyn Pane>)>,
        os_api: &'a mut Box<dyn ServerOsApi>,
    ) -> Self {
        let panes: HashMap<_, _> = panes.collect();
        let mut vars = HashMap::new();
        for &&k in panes.keys() {
            vars.insert(k, Variable::new());
        }
        PaneResizer {
            panes,
            os_api,
            vars,
            solver: Solver::new(),
        }
    }

    // FIXME: Is this even a resize function even more? Should I call it
    // something like `(re)layout`?
    pub fn layout(&mut self, direction: Direction, size: usize) -> Result<(), String> {
        self.solver.reset();
        self.layout_direction(direction, size)?;
        Ok(())
    }

    fn layout_direction(&mut self, direction: Direction, new_size: usize) -> Result<(), String> {
        let spans = self.solve_direction(direction, new_size)?;
        self.apply_spans(&spans);
        // FIXME: This is beyond stupid. I need to break this code up so this useless return isn't
        // needed... Maybe up in `resize`: solve -> discretize_spans -> apply_spans
        Ok(())
    }

    fn solve_direction(&mut self, direction: Direction, space: usize) -> Result<Vec<Span>, String> {
        let mut grid = Vec::new();
        for boundary in self.grid_boundaries(direction) {
            grid.push(self.spans_in_boundary(direction, boundary));
        }

        let constraints: Vec<_> = grid
            .iter()
            .flat_map(|s| constrain_spans(space, s))
            .collect();

        self.solver
            .add_constraints(&constraints)
            .map_err(|e| format!("{:?}", e))?;
        // FIXME: This chunk needs to be broken up into smaller functions!
        let mut rounded_sizes = HashMap::new();
        for span in grid.iter_mut().flatten() {
            let size = self.solver.get_value(span.size_var);
            rounded_sizes.insert(span.size_var, size as isize);
        }
        let mut finalised = Vec::new();
        for spans in &mut grid {
            let rounded_size: isize = spans.iter().map(|s| rounded_sizes[&s.size_var]).sum();
            let mut error = space as isize - rounded_size;
            let mut flex_spans: Vec<&mut Span> = spans
                .iter_mut()
                .filter(|s| !s.size.is_fixed() && !finalised.contains(&s.pid))
                .collect();
            flex_spans.sort_by_key(|s| rounded_sizes[&s.size_var]);
            if error < 0 {
                flex_spans.reverse();
            }
            for span in flex_spans {
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
                    return Err("Ran out of room for spans".into());
                }
                span.size.set_inner(sz as usize);
                offset += span.size.as_usize();
            }
        }
        Ok(grid.into_iter().flatten().collect())
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
        let size_var = self.vars[&pane.pid()];
        match direction {
            Direction::Horizontal => Span {
                pid: pane.pid(),
                direction,
                pos: pas.x,
                size: pas.cols,
                size_var,
            },
            Direction::Vertical => Span {
                pid: pane.pid(),
                direction,
                pos: pas.y,
                size: pas.rows,
                size_var,
            },
        }
    }

    fn apply_spans(&mut self, spans: &[Span]) {
        for span in spans {
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
            if pane.geom_override().is_some() {
                pane.get_geom_override(new_geom);
            } else {
                pane.set_geom(new_geom);
            }
            if let PaneId::Terminal(pid) = pane.pid() {
                self.os_api.set_terminal_size_using_fd(
                    pid,
                    pane.get_content_columns() as u16,
                    pane.get_content_rows() as u16,
                );
            }
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
    constraints.insert(full_size | EQ(REQUIRED) | space as f64);

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
