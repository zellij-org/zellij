//! Bounded, resumable work primitives shared by pane jobs.
//!
//! A job may only publish an exact result after [`JobProgress::Done`]. A
//! [`JobProgress::Pending`] step has spent at most one [`WorkBudget`] and
//! must be resumable. [`JobProgress::Invalid`] means the content the job
//! was started against changed underneath it; no partial result may be
//! published.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum JobProgress {
    Done,
    Pending,
    Invalid,
}

/// Independent limits for the three kinds of work a pane job can do.
///
/// Rows and cells bound traversal of stored content; transitions bound
/// wrap/table state updates. All three are charged even by closed-form
/// fast paths, so no path can bypass the bound.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct WorkBudget {
    rows: usize,
    cells: usize,
    transitions: usize,
}

impl WorkBudget {
    pub(crate) const fn new(rows: usize, cells: usize, transitions: usize) -> Self {
        Self {
            rows,
            cells,
            transitions,
        }
    }

    pub(crate) fn remaining_cells(&self) -> usize {
        self.cells
    }

    pub(crate) fn remaining_transitions(&self) -> usize {
        self.transitions
    }

    pub(crate) fn charge_row(&mut self) -> bool {
        self.charge_rows(1)
    }

    pub(crate) fn charge_rows(&mut self, rows: usize) -> bool {
        match self.rows.checked_sub(rows) {
            Some(remaining) => {
                self.rows = remaining;
                true
            },
            None => {
                self.rows = 0;
                false
            },
        }
    }

    pub(crate) fn charge_cell(&mut self) -> bool {
        self.charge_cells(1)
    }

    pub(crate) fn charge_cells(&mut self, cells: usize) -> bool {
        match self.cells.checked_sub(cells) {
            Some(remaining) => {
                self.cells = remaining;
                true
            },
            None => {
                self.cells = 0;
                false
            },
        }
    }

    pub(crate) fn charge_transition(&mut self) -> bool {
        self.charge_transitions(1)
    }

    pub(crate) fn charge_transitions(&mut self, transitions: usize) -> bool {
        match self.transitions.checked_sub(transitions) {
            Some(remaining) => {
                self.transitions = remaining;
                true
            },
            None => {
                self.transitions = 0;
                false
            },
        }
    }
}

/// A resumable job over a context. `epoch` is the content epoch the job
/// was started against; a step that observes a different epoch must
/// return [`JobProgress::Invalid`].
pub(crate) trait ResumableJob<Ctx> {
    fn step(&mut self, ctx: &mut Ctx, epoch: u64, budget: &mut WorkBudget) -> JobProgress;
}
