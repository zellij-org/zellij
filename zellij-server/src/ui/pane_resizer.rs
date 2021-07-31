use crate::{os_input_output::ServerOsApi, panes::PaneId, tab::Pane};
use cassowary::{
    strength::{REQUIRED, STRONG},
    Solver, Variable,
    WeightedRelation::*,
};
use std::{
    collections::{BTreeMap, HashSet},
    ops::Not,
};
use zellij_utils::pane_size::{Constraint, Dimension, PaneGeom, Size};

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

    pub fn resize(&mut self, new_size: Size) -> Option<(usize, usize)> {
        // FIXME: Please don't forget that using this type for window sizes is dumb!
        // `new_size.cols` should be a plain usize!!!
        // let spans = self.solve_direction(Direction::Horizontal, new_size.cols.as_usize())?;
        // self.apply_spans(&spans);
        log::info!("Time for a horizontal resize!\nNew Size: {:?}", new_size);
        self.layout_direction(Direction::Horizontal, new_size.cols);
        self.solver.reset();
        log::info!("Time for a vertical resize!");
        self.layout_direction(Direction::Vertical, new_size.rows);
        //let spans = self.solve_direction(Direction::Vertical, new_size.rows.as_usize())?;
        //self.apply_spans(&spans);
        Some((new_size.cols, new_size.rows))
    }

    fn layout_direction(&mut self, direction: Direction, new_size: usize) -> Option<()> {
        let spans = self.solve_direction(direction, new_size)?;
        self.apply_spans(&spans);
        // FIXME: This is beyond stupid. I need to break this code up so this useless return isn't
        // needed... Maybe up in `resize`: solve -> discretize_spans -> apply_spans
        Some(())
    }

    fn solve_direction(&mut self, direction: Direction, space: usize) -> Option<Vec<Span>> {
        let mut grid = Vec::new();
        for boundary in self.grid_boundaries(direction) {
            log::info!("Boundary: {:?}",  boundary);
            grid.push(self.spans_in_boundary(direction, boundary));
        }
        let dbg_grid: Vec<Vec<PaneId>> = grid.iter().map(|r| r.iter().map(|s| s.pid).collect()).collect();
        log::info!("Grid: {:#?}\nSpace: {}", dbg_grid, space);
        let constraints: Vec<_> = grid
            .iter()
            .flat_map(|s| constrain_spans(space, s))
            .collect();

        // FIXME: This line needs to be restored before merging!
        //self.solver.add_constraints(&constraints).ok()?;
        self.solver.add_constraints(&constraints).unwrap();
        for spans in &mut grid {
            let mut rounded_size = 0;
            for span in spans.iter_mut() {
                let size = self.solver.get_value(span.size_var);
                span.size.set_inner(size as usize);
                log::info!("Span {:?}; Size: {}", span.pid, span.size.as_usize());
                rounded_size += span.size.as_usize() + GAP_SIZE;
                log::info!("Rounded Size: {}", rounded_size);
            }
            rounded_size -= GAP_SIZE;
            log::info!("Space: {}; Rounded: {}", space, rounded_size);
            let error = space - rounded_size;
            let mut flex_spans: Vec<&mut Span> =
                spans.iter_mut().filter(|s| !s.size.is_fixed()).collect();
            flex_spans.sort_unstable_by_key(|s| s.size.as_usize());
            for i in 0..error {
                // FIXME: If this causes errors, `i % flex_spans.len()`
                // FIXME: Think about implementing `AddAssign`
                let sz = flex_spans[i].size.as_usize() + 1;
                flex_spans[i].size.set_inner(sz);
            }
            let mut offset = 0;
            for span in spans.iter_mut() {
                span.pos = offset;
                offset += span.size.as_usize() + GAP_SIZE;
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
        let mut edges: Vec<usize> = spans.iter().map(|s| s.pos + s.size.as_usize() + 1).collect();
        edges.sort_unstable();
        edges.dedup();
        log::info!("Edges: {:?}", &edges);
        for next in edges {
            let next_edge = next;
            bounds.push((last_edge, next_edge));
            last_edge = next_edge;
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
                bwn(s.pos) || bwn(s.pos + s.size.as_usize())
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
            let pane = self.panes.get_mut(&span.pid).unwrap();
            match span.direction {
                Direction::Horizontal => pane.change_pos_and_size(&PaneGeom {
                    x: span.pos,
                    cols: span.size,
                    ..pane.position_and_size()
                }),
                Direction::Vertical => pane.change_pos_and_size(&PaneGeom {
                    y: span.pos,
                    rows: span.size,
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

fn constrain_spans(space: usize, spans: &[Span]) -> HashSet<cassowary::Constraint> {
    let mut constraints = HashSet::new();

    // The first span needs to start at 0
    constraints.insert(spans[0].pos_var | EQ(REQUIRED) | 0.0);

    // Calculating "flexible" space (space not consumed by fixed-size spans)
    let gap_space = GAP_SIZE * (spans.len() - 1);
    let new_flex_space = spans.iter().fold(space - gap_space, |a, s| {
        if let Constraint::Fixed(sz) = s.size.constraint {
            a - sz
        } else {
            a
        }
    });
    log::info!("Space: {}; New Flex: {}", space, new_flex_space);

    // Keep spans stuck together
    for pair in spans.windows(2) {
        let (ls, rs) = (pair[0], pair[1]);
        constraints
            .insert((ls.pos_var + ls.size_var + GAP_SIZE as f64) | EQ(REQUIRED) | rs.pos_var);
    }

    // Try to maintain ratios and lock non-flexible sizes
    for span in spans {
        match span.size.constraint {
            Constraint::Fixed(s) => {
                constraints.insert(span.size_var | EQ(REQUIRED) | s as f64)
            }
            Constraint::Percent(p) => constraints
                .insert((span.size_var / new_flex_space as f64) | EQ(STRONG) | (p / 100.0)),
        };
    }

    // The last pane needs to end at the end of the space
    let last = spans.last().unwrap();
    constraints.insert((last.pos_var + last.size_var) | EQ(REQUIRED) | space as f64);

    constraints
}
