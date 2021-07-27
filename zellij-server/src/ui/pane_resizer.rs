#![allow(dead_code)]
use crate::{os_input_output::ServerOsApi, panes::PaneId, tab::Pane};
use cassowary::{
    strength::{REQUIRED, STRONG},
    Constraint, Solver, Variable,
    WeightedRelation::*,
};
use std::{
    collections::{BTreeMap, HashSet},
    ops::Not,
};
use zellij_utils::pane_size::PositionAndSize;

const GAP_SIZE: usize = 1; // Panes are separated by this number of rows / columns

pub struct PaneResizer<'a> {
    panes: &'a mut BTreeMap<PaneId, Box<dyn Pane>>,
    vars: BTreeMap<PaneId, (Variable, Variable)>,
    solver: Solver,
    os_api: &'a mut Box<dyn ServerOsApi>,
}

#[derive(Debug, Clone, Copy)]
enum Direction {
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

#[derive(Debug, Clone, Copy)]
struct Span {
    pid: PaneId,
    direction: Direction,
    fixed: bool,
    pos: usize,
    size: usize,
    pos_var: Variable,
    size_var: Variable,
}

// TODO: currently there are some functions here duplicated with Tab
// all resizing functions should move here

// FIXME:
// 1. Rounding causes a loss of ratios, I need to store an internal f64 for
//    each pane as well as the displayed usize and add custom rounding logic.
// 2. Vertical resizing doesn't seem to respect the space consumed by the tab
//    and status bars?
// 3. A 2x2 layout and simultaneous vertical + horizontal resizing sometimes
//    leads to unsolvable constraints? Maybe related to 2 (and possibly 1).
//    I should sanity-check the `spans_in_boundary()` here!

impl<'a> PaneResizer<'a> {
    pub fn new(
        panes: &'a mut BTreeMap<PaneId, Box<dyn Pane>>,
        os_api: &'a mut Box<dyn ServerOsApi>,
    ) -> Self {
        let mut vars = BTreeMap::new();
        for &k in panes.keys() {
            vars.insert(k, (Variable::new(), Variable::new()));
        }
        PaneResizer {
            panes,
            vars,
            solver: Solver::new(),
            os_api,
        }
    }

    pub fn resize(
        &mut self,
        current_size: PositionAndSize,
        new_size: PositionAndSize,
    ) -> Option<(isize, isize)> {
        let col_delta = new_size.cols as isize - current_size.cols as isize;
        let row_delta = new_size.rows as isize - current_size.rows as isize;
        if col_delta != 0 {
            let spans = self.solve_direction(Direction::Horizontal, new_size.cols)?;
            self.collapse_spans(&spans);
        }
        self.solver.reset();
        if row_delta != 0 {
            let spans = self.solve_direction(Direction::Vertical, new_size.rows)?;
            self.collapse_spans(&spans);
        }
        Some((col_delta, row_delta))
    }

    fn solve_direction(&mut self, direction: Direction, space: usize) -> Option<Vec<Span>> {
        let mut grid = Vec::new();
        for boundary in self.grid_boundaries(direction) {
            grid.push(self.spans_in_boundary(direction, boundary));
        }

        let constraints: Vec<_> = grid
            .iter()
            .flat_map(|s| constrain_spans(space, s))
            .collect();

        // FIXME: This line needs to be restored before merging!
        //self.solver.add_constraints(&constraints).ok()?;
        self.solver.add_constraints(&constraints).unwrap();
        Some(grid.into_iter().flatten().collect())
    }

    fn grid_boundaries(&self, direction: Direction) -> Vec<(usize, usize)> {
        // Select the spans running *perpendicular* to the direction of resize
        let spans: Vec<Span> = self
            .panes
            .values()
            .map(|p| self.get_span(!direction, p.as_ref()))
            .collect();

        let mut last_edge = 0;
        let mut bounds = Vec::new();
        loop {
            let mut spans_on_edge: Vec<&Span> =
                spans.iter().filter(|p| p.pos == last_edge).collect();
            spans_on_edge.sort_unstable_by_key(|s| s.size);
            if let Some(next) = spans_on_edge.first() {
                let next_edge = last_edge + next.size;
                bounds.push((last_edge, next_edge));
                last_edge = next_edge + GAP_SIZE;
            } else {
                break;
            }
        }
        bounds
    }

    fn spans_in_boundary(&self, direction: Direction, boundary: (usize, usize)) -> Vec<Span> {
        let (start, end) = boundary;
        let bwn = |v| start <= v && v < end;
        let mut spans: Vec<_> = self
            .panes
            .values()
            .filter(|p| {
                let s = self.get_span(!direction, p.as_ref());
                bwn(s.pos) || bwn(s.pos + s.size)
            })
            .map(|p| self.get_span(direction, p.as_ref()))
            .collect();
        spans.sort_unstable_by_key(|s| s.pos);
        spans
    }

    fn get_span(&self, direction: Direction, pane: &dyn Pane) -> Span {
        let pas = pane.position_and_size();
        let (pos_var, size_var) = self.vars[&pane.pid()];
        match direction {
            Direction::Horizontal => Span {
                pid: pane.pid(),
                direction,
                fixed: pas.cols_fixed,
                pos: pas.x,
                size: pas.cols,
                pos_var,
                size_var,
            },
            Direction::Vertical => Span {
                pid: pane.pid(),
                direction,
                fixed: pas.rows_fixed,
                pos: pas.y,
                size: pas.rows,
                pos_var,
                size_var,
            },
        }
    }

    fn collapse_spans(&mut self, spans: &[Span]) {
        for span in spans {
            let solver = &self.solver; // Hand-holding the borrow-checker
            let pane = self.panes.get_mut(&span.pid).unwrap();
            let fetch_usize = |v| solver.get_value(v).round() as usize;
            match span.direction {
                Direction::Horizontal => pane.change_pos_and_size(&PositionAndSize {
                    x: fetch_usize(span.pos_var),
                    cols: fetch_usize(span.size_var),
                    ..pane.position_and_size()
                }),
                Direction::Vertical => pane.change_pos_and_size(&PositionAndSize {
                    y: fetch_usize(span.pos_var),
                    rows: fetch_usize(span.size_var),
                    ..pane.position_and_size()
                }),
            }
            if let PaneId::Terminal(pid) = pane.pid() {
                self.os_api
                    .set_terminal_size_using_fd(pid, pane.cols() as u16, pane.rows() as u16);
            }
        }
    }
}

fn constrain_spans(space: usize, spans: &[Span]) -> HashSet<Constraint> {
    let mut constraints = HashSet::new();

    // The first span needs to start at 0
    constraints.insert(spans[0].pos_var | EQ(REQUIRED) | 0.0);

    // Calculating "flexible" space (space not consumed by fixed-size spans)
    let gap_space = GAP_SIZE * (spans.len() - 1);
    let old_flex_space = spans
        .iter()
        .fold(0, |a, s| if !s.fixed { a + s.size } else { a });
    let new_flex_space = spans.iter().fold(
        space - gap_space,
        |a, s| if s.fixed { a - s.size } else { a },
    );

    // Keep spans stuck together
    for pair in spans.windows(2) {
        let (ls, rs) = (pair[0], pair[1]);
        constraints
            .insert((ls.pos_var + ls.size_var + GAP_SIZE as f64) | EQ(REQUIRED) | rs.pos_var);
    }

    // Try to maintain ratios and lock non-flexible sizes
    for span in spans {
        if span.fixed {
            constraints.insert(span.size_var | EQ(REQUIRED) | span.size as f64);
        } else {
            let ratio = span.size as f64 / old_flex_space as f64;
            constraints.insert((span.size_var / new_flex_space as f64) | EQ(STRONG) | ratio);
        }
    }

    // The last pane needs to end at the end of the space
    let last = spans.last().unwrap();
    constraints.insert((last.pos_var + last.size_var) | EQ(REQUIRED) | space as f64);

    constraints
}
