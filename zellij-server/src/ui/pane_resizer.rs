use crate::{panes::PaneId, tab::Pane};
use cassowary::{
    strength::{REQUIRED, STRONG},
    Expression, Solver, Variable,
    WeightedRelation::EQ,
};
use std::collections::{HashMap, HashSet};
use zellij_utils::{
    input::layout::Direction,
    pane_size::{Constraint, Dimension, PaneGeom},
};

pub struct PaneResizer<'a> {
    panes: HashMap<&'a PaneId, &'a mut Box<dyn Pane>>,
    vars: HashMap<PaneId, Variable>,
    solver: Solver,
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

type Grid = Vec<Vec<Span>>;

impl<'a> PaneResizer<'a> {
    pub fn new(panes: impl IntoIterator<Item = (&'a PaneId, &'a mut Box<dyn Pane>)>) -> Self {
        let panes: HashMap<_, _> = panes.into_iter().collect();
        let mut vars = HashMap::new();
        for &&k in panes.keys() {
            vars.insert(k, Variable::new());
        }
        PaneResizer {
            panes,
            vars,
            solver: Solver::new(),
        }
    }

    pub fn layout(&mut self, direction: Direction, space: usize) -> Result<(), String> {
        self.solver.reset();
        let grid = self.solve(direction, space)?;
        let spans = self.discretize_spans(grid, space)?;
        self.apply_spans(spans);
        Ok(())
    }

    fn solve(&mut self, direction: Direction, space: usize) -> Result<Grid, String> {
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

    fn apply_spans(&mut self, spans: Vec<Span>) {
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
        }
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

fn stable_round(x: f64) -> f64 {
    ((x * 100.0).round() / 100.0).round()
}
