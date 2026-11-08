//! Per-logical-line summaries for scrollback.
//!
//! A logical line is one canonical row followed by zero or more continuation
//! rows.  Measuring a line means greedily wrapping the concatenation of its
//! stored cells at a fixed width, never splitting a character.  This mirrors
//! `Grid`'s `WrapState`/`Row::rewrap_rows` rule, but keeps only the per-line
//! summary so scrollback bookkeeping does not rescan every row.
//!
//! `LineIndex` owns metadata only; the rows remain in `Grid::lines_above`.

use super::grid::Row;
use std::collections::VecDeque;

#[cfg(test)]
thread_local! {
    static ROW_VISITS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    static CELL_VISITS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    static BOUNDARY_DISCOVERIES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    static LINE_SCAN_VISITS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
pub(crate) fn row_visits() -> usize {
    ROW_VISITS.with(std::cell::Cell::get)
}

#[cfg(test)]
pub(crate) fn reset_row_visits() {
    ROW_VISITS.with(|value| value.set(0));
}

#[cfg(test)]
pub(crate) fn cell_visits() -> usize {
    CELL_VISITS.with(std::cell::Cell::get)
}

#[cfg(test)]
pub(crate) fn reset_cell_visits() {
    CELL_VISITS.with(|value| value.set(0));
}

#[cfg(test)]
pub(crate) fn boundary_discoveries() -> usize {
    BOUNDARY_DISCOVERIES.with(std::cell::Cell::get)
}

#[cfg(test)]
pub(crate) fn reset_boundary_discoveries() {
    BOUNDARY_DISCOVERIES.with(|value| value.set(0));
}

#[cfg(test)]
pub(crate) fn line_scan_visits() -> usize {
    LINE_SCAN_VISITS.with(std::cell::Cell::get)
}

#[cfg(test)]
pub(crate) fn reset_line_scan_visits() {
    LINE_SCAN_VISITS.with(|value| value.set(0));
}

#[cfg(test)]
fn note_row_visit() {
    ROW_VISITS.with(|value| value.set(value.get() + 1));
}

#[cfg(test)]
fn note_cell_visits(count: usize) {
    CELL_VISITS.with(|value| value.set(value.get() + count));
}

#[cfg(test)]
fn note_boundary_discovery() {
    BOUNDARY_DISCOVERIES.with(|value| value.set(value.get() + 1));
}

#[cfg(test)]
fn note_line_scan_visit() {
    LINE_SCAN_VISITS.with(|value| value.set(value.get() + 1));
}

/// The wrapping state at the start of a stored row, or after a line's last
/// row.  `completed_rows` counts display rows already finished; `current`
/// counts cells used on the display row that is still open.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct RowBoundary {
    pub completed_rows: u32,
    pub current: u32,
}

impl RowBoundary {
    fn push_character(&mut self, character_width: usize, width: usize) {
        if width == 0 {
            return;
        }
        let width = width.min(u32::MAX as usize) as u64;
        let character_width = character_width.min(u32::MAX as usize) as u64;
        let current = self.current as u64;
        if current + character_width > width {
            if current > 0 {
                self.completed_rows = self.completed_rows.saturating_add(1);
            }
            self.current = 0;
        }
        self.current = (self.current as u64 + character_width).min(u32::MAX as u64) as u32;
    }

    /// Equivalent to `cells` calls of `push_character(1, width)`, using the
    /// same fast path as `WrapState::push_single_width_cells`.
    fn push_single_width_cells(&mut self, cells: usize, width: usize) {
        if cells == 0 || width == 0 {
            return;
        }
        let width = width.min(u32::MAX as usize) as u64;
        let mut current = self.current as u64;
        if current > width {
            self.completed_rows = self.completed_rows.saturating_add(1);
            current = 0;
        }
        let total = current.saturating_add(cells as u64);
        if total <= width {
            self.current = total as u32;
        } else {
            let completed = (total - 1) / width;
            self.completed_rows = self
                .completed_rows
                .saturating_add(completed.min(u32::MAX as u64) as u32);
            self.current = (total - completed * width) as u32;
        }
    }

    fn height(self) -> usize {
        (self.completed_rows as usize)
            .saturating_add(usize::from(self.current > 0))
            .max(1)
    }
}

/// Exact change, or a request to measure an affected line first.  Structural
/// operations still update grouping when they return `NeedsMeasure`; removal
/// deltas are negative and include the appended row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LineDelta {
    Exact(isize),
    NeedsMeasure,
}

impl LineDelta {
    #[cfg(test)]
    pub(crate) fn exact(self) -> Option<isize> {
        match self {
            LineDelta::Exact(delta) => Some(delta),
            LineDelta::NeedsMeasure => None,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct MeasureBudget {
    pub rows: usize,
    pub cells: usize,
}

impl MeasureBudget {
    pub(crate) const fn new(rows: usize, cells: usize) -> Self {
        Self { rows, cells }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MeasureProgress {
    Pending,
    Done,
}

#[derive(Clone, Debug)]
struct MeasureCursor {
    line_index: usize,
    /// Global index of this line's first stored row.
    row_start: usize,
    /// Offset within the line.
    row_index: usize,
    /// Offset within the stored row: columns first, then compressed blanks.
    cell_index: usize,
    state: RowBoundary,
    boundaries: Vec<RowBoundary>,
    cell_prefix: VecDeque<usize>,
    uniform_state: UniformState,
    /// Uniform-width merge state at the start of each entered row.
    uniform_prefix: Vec<UniformState>,
    width: usize,
}

#[derive(Clone, Copy, Debug, Default)]
struct UniformState {
    width: Option<usize>,
    saw_content: bool,
}

impl UniformState {
    fn observe(&mut self, cells: usize, width: Option<usize>) {
        if cells == 0 {
            return;
        }
        if self.saw_content {
            self.width = merge_uniform_width(self.width, width);
        } else {
            self.width = width;
            self.saw_content = true;
        }
    }

    fn resolved(self) -> Option<usize> {
        if self.saw_content {
            self.width
        } else {
            Some(1)
        }
    }
}

impl MeasureCursor {
    fn new(line_index: usize, row_start: usize, width: usize) -> Self {
        let mut cell_prefix = VecDeque::new();
        cell_prefix.push_back(0);
        Self {
            line_index,
            row_start,
            row_index: 0,
            cell_index: 0,
            state: RowBoundary::default(),
            boundaries: Vec::new(),
            cell_prefix,
            uniform_state: UniformState::default(),
            uniform_prefix: Vec::new(),
            width,
        }
    }

    fn reset_for_line(&mut self, line_index: usize, row_start: usize) {
        self.line_index = line_index;
        self.row_start = row_start;
        self.row_index = 0;
        self.cell_index = 0;
        self.state = RowBoundary::default();
        self.boundaries.clear();
        self.cell_prefix.clear();
        self.cell_prefix.push_back(0);
        self.uniform_state = UniformState::default();
        self.uniform_prefix.clear();
    }
}

struct LineMeasurement {
    boundaries: Vec<RowBoundary>,
    end: RowBoundary,
    height: usize,
    cell_prefix: VecDeque<usize>,
    uniform_character_width: Option<usize>,
}

#[derive(Clone, Debug)]
struct Line {
    row_count: usize,
    measured_width: Option<usize>,
    boundaries: Vec<RowBoundary>,
    end: RowBoundary,
    height: usize,
    /// Cumulative stored cells from the original line start.  `cell_prefix`
    /// is empty for an unmeasured line.  `cell_prefix_base` advances when a
    /// front eviction keeps a suffix of the line.
    cell_prefix: VecDeque<usize>,
    cell_prefix_base: usize,
    /// Single-width lines derive every boundary from `cell_prefix`, so a
    /// mid-line front eviction does not have to rewrite the whole line.
    uniform_character_width: Option<usize>,
}

impl Line {
    fn unmeasured(row_count: usize) -> Self {
        Self {
            row_count,
            measured_width: None,
            boundaries: Vec::new(),
            end: RowBoundary::default(),
            height: 0,
            cell_prefix: VecDeque::new(),
            cell_prefix_base: 0,
            uniform_character_width: None,
        }
    }

    fn from_measurement(row_count: usize, width: usize, measurement: LineMeasurement) -> Self {
        Self {
            row_count,
            measured_width: Some(width),
            boundaries: if measurement.uniform_character_width.is_some() {
                Vec::new()
            } else {
                measurement.boundaries
            },
            end: measurement.end,
            height: measurement.height,
            cell_prefix: measurement.cell_prefix,
            cell_prefix_base: 0,
            uniform_character_width: measurement.uniform_character_width,
        }
    }

    fn suffix_cells(&self, relative_row: usize) -> usize {
        self.cell_prefix[self.cell_prefix_base + relative_row]
            .saturating_sub(self.cell_prefix[self.cell_prefix_base])
    }

    #[cfg(test)]
    fn boundary_at(&self, relative_row: usize, width: usize) -> RowBoundary {
        if let Some(character_width) = self.uniform_character_width {
            uniform_boundary(self.suffix_cells(relative_row), character_width, width)
        } else {
            self.boundaries[relative_row]
        }
    }

    fn height_after(&self, end: RowBoundary) -> usize {
        end.height()
    }

    /// Drop a measured summary. The line keeps its rows and grouping, but the
    /// next read must remeasure it at the current width. Used when a cut moves
    /// the logical-line origin or when a uniform line gains a different glyph
    /// width: both changes make the stored boundaries path-dependent.
    fn invalidate(&mut self) {
        self.measured_width = None;
        self.boundaries = Vec::new();
        self.end = RowBoundary::default();
        self.height = 0;
        self.cell_prefix = VecDeque::new();
        self.cell_prefix_base = 0;
        self.uniform_character_width = None;
    }

    /// Do not keep a large backing allocation after the line shrank by an
    /// order of magnitude. The check is cheap and the shrink is amortized by
    /// the eviction that made the capacity oversized.
    fn release_oversized_backing(&mut self) {
        fn oversized(len: usize, capacity: usize) -> bool {
            capacity > len.saturating_mul(2).saturating_add(64)
        }
        if oversized(self.cell_prefix.len(), self.cell_prefix.capacity()) {
            self.cell_prefix.shrink_to_fit();
        }
        if oversized(self.boundaries.len(), self.boundaries.capacity()) {
            self.boundaries.shrink_to_fit();
        }
    }

    fn compact_cell_prefix(&mut self) {
        const COMPACT_AT: usize = 1024;
        if self.cell_prefix_base >= COMPACT_AT {
            for _ in 0..self.cell_prefix_base {
                self.cell_prefix.pop_front();
            }
            self.cell_prefix_base = 0;
            self.release_oversized_backing();
        }
    }

    #[cfg(test)]
    fn check_prefix(&self, context: &str) {
        if self.measured_width.is_some() && self.uniform_character_width.is_some() {
            assert!(
                self.cell_prefix_base + self.row_count < self.cell_prefix.len(),
                "{context}: base={} rows={} len={}",
                self.cell_prefix_base,
                self.row_count,
                self.cell_prefix.len()
            );
        }
    }
}

enum PartialLine {
    SingleWidth {
        cut: usize,
        height: usize,
        end: RowBoundary,
    },
    Invalidated,
}

fn trailing_blanks(row: &Row) -> usize {
    row.stored_cell_count().saturating_sub(row.columns.len())
}

fn row_uniform_character_width(row: &Row) -> Option<usize> {
    let mut width = None;
    for character in &row.columns {
        let character_width = character.width();
        if character_width == 0 {
            return None;
        }
        match width {
            Some(existing) if existing != character_width => return None,
            None => width = Some(character_width),
            _ => {},
        }
    }
    let blanks = trailing_blanks(row);
    if blanks > 0 && width.is_some_and(|width| width != 1) {
        return None;
    }
    width.or_else(|| (blanks > 0).then_some(1))
}

fn uniform_boundary(cells: usize, character_width: usize, width: usize) -> RowBoundary {
    if width == 0 {
        return RowBoundary::default();
    }
    if character_width == 0 {
        return RowBoundary::default();
    }
    let per_row = (width / character_width).max(1);
    let completed = cells.saturating_sub(1) / per_row;
    let current_glyphs = cells.saturating_sub(completed * per_row);
    RowBoundary {
        completed_rows: completed.min(u32::MAX as usize) as u32,
        current: (current_glyphs * character_width).min(u32::MAX as usize) as u32,
    }
}

fn uniform_height(cells: usize, character_width: usize, width: usize) -> usize {
    if width == 0 {
        return 1;
    }
    if character_width == 0 {
        return 1;
    }
    let per_row = (width / character_width).max(1);
    cells.div_ceil(per_row).max(1)
}

fn merge_uniform_width(current: Option<usize>, row: Option<usize>) -> Option<usize> {
    match (current, row) {
        (Some(left), Some(right)) if left == right => Some(left),
        _ => None,
    }
}

fn measured_uniform_width_rows(row_widths: &[(usize, Option<usize>)]) -> Option<usize> {
    let mut uniform = None;
    let mut saw_content = false;
    for &(cells, width) in row_widths {
        if cells == 0 {
            continue;
        }
        if !saw_content {
            uniform = width;
            saw_content = true;
        } else {
            uniform = merge_uniform_width(uniform, width);
        }
        if uniform.is_none() {
            break;
        }
    }
    if saw_content {
        uniform
    } else {
        Some(1)
    }
}

fn measure_row(state: &mut RowBoundary, row: &Row, width: usize) {
    #[cfg(test)]
    note_cell_visits(row.stored_cell_count());
    for character in &row.columns {
        state.push_character(character.width(), width);
    }
    state.push_single_width_cells(trailing_blanks(row), width);
}

fn measure_rows<'a>(rows: impl IntoIterator<Item = &'a Row>, width: usize) -> LineMeasurement {
    let mut state = RowBoundary::default();
    let mut boundaries = Vec::new();
    let mut cell_prefix = VecDeque::new();
    cell_prefix.push_back(0usize);
    let mut row_uniform_widths = Vec::new();
    for row in rows {
        #[cfg(test)]
        {
            note_row_visit();
            note_boundary_discovery();
        }
        boundaries.push(state);
        measure_row(&mut state, row, width);
        let total = cell_prefix.back().copied().unwrap_or(0);
        cell_prefix.push_back(total.saturating_add(row.stored_cell_count()));
        row_uniform_widths.push((row.stored_cell_count(), row_uniform_character_width(row)));
    }
    let uniform_character_width = measured_uniform_width_rows(&row_uniform_widths);
    LineMeasurement {
        boundaries,
        end: state,
        height: state.height(),
        cell_prefix,
        uniform_character_width,
    }
}

/// Per-line summaries over an externally owned scrollback deque.
#[derive(Clone, Debug, Default)]
pub(crate) struct LineIndex {
    lines: VecDeque<Line>,
    width: usize,
    measured_lines: usize,
    measured_display_rows: usize,
    /// Last fully measured total.  Kept across a width change until the
    /// resumable measurement completes, so readers see a complete value.
    published_display_rows: usize,
    measure: Option<MeasureCursor>,
    /// Every line before this index is measured. `measure_hint_row_start` is
    /// the number of stored rows before it. This keeps summary repair O(1)
    /// instead of rescanning every measured line to find the first gap.
    measure_hint_line: usize,
    measure_hint_row_start: usize,
}

impl LineIndex {
    pub(crate) fn new(width: usize) -> Self {
        Self {
            width,
            ..Self::default()
        }
    }

    pub(crate) fn clear(&mut self) {
        self.lines.clear();
        self.measured_lines = 0;
        self.measured_display_rows = 0;
        self.published_display_rows = 0;
        self.measure = None;
        self.measure_hint_line = 0;
        self.measure_hint_row_start = 0;
    }

    /// Rebuild logical-line grouping without measuring cells.  Used when a
    /// resize path rewrites the stored rows and invalidates old boundaries.
    pub(crate) fn regroup(&mut self, rows: &VecDeque<Row>, width: usize) {
        self.width = width;
        self.lines.clear();
        self.measure = None;
        for row in rows {
            #[cfg(test)]
            {
                note_row_visit();
                note_boundary_discovery();
            }
            if self.lines.is_empty() || row.is_canonical {
                self.lines.push_back(Line::unmeasured(1));
            } else if let Some(line) = self.lines.back_mut() {
                line.row_count += 1;
            }
        }
        self.measured_lines = 0;
        self.measured_display_rows = 0;
        self.measure_hint_line = 0;
        self.measure_hint_row_start = 0;
        if rows.is_empty() {
            self.published_display_rows = 0;
        }
    }

    pub(crate) fn line_count(&self) -> usize {
        self.lines.len()
    }

    #[cfg(test)]
    pub(crate) fn line_row_count(&self, line_index: usize) -> Option<usize> {
        self.lines.get(line_index).map(|line| line.row_count)
    }

    #[cfg(test)]
    pub(crate) fn line_height(&self, line_index: usize) -> Option<usize> {
        self.lines
            .get(line_index)
            .and_then(|line| (line.measured_width == Some(self.width)).then_some(line.height))
    }

    pub(crate) fn is_fully_measured(&self) -> bool {
        self.measured_lines == self.lines.len()
    }

    /// Whether the oldest line still needs its summary rebuilt. While that is
    /// true, front eviction would reset the measurement origin.
    pub(crate) fn front_line_unmeasured(&self) -> bool {
        self.lines
            .front()
            .is_some_and(|line| line.measured_width != Some(self.width))
    }

    pub(crate) fn front_line_row_count(&self) -> Option<usize> {
        self.lines.front().map(|line| line.row_count)
    }

    pub(crate) fn front_line_is_measured(&self) -> bool {
        self.lines
            .front()
            .is_some_and(|line| line.measured_width == Some(self.width))
    }

    pub(crate) fn measurement_active(&self) -> bool {
        self.measure.is_some()
    }

    /// Whether removing one row from the front keeps every summary exact.
    /// Uniform lines rebase cheaply and a one-row line disappears whole; a
    /// partial mixed-width cut has to remeasure from its new origin.
    pub(crate) fn front_row_cut_is_exact(&self) -> bool {
        self.lines.front().map_or(true, |line| {
            line.measured_width == Some(self.width)
                && (line.uniform_character_width.is_some() || line.row_count == 1)
        })
    }

    /// Drop one line's summary after its stored rows changed in a way that
    /// makes the old boundaries path-dependent (for example a row split).
    pub(crate) fn invalidate_line(&mut self, line_index: usize) {
        let hint_shift = (line_index < self.measure_hint_line).then(|| {
            self.lines
                .range(line_index..self.measure_hint_line)
                .map(|line| line.row_count)
                .sum::<usize>()
        });
        let Some(line) = self.lines.get_mut(line_index) else {
            return;
        };
        if line.measured_width == Some(self.width) {
            self.measured_display_rows = self.measured_display_rows.saturating_sub(line.height);
            self.measured_lines = self.measured_lines.saturating_sub(1);
        }
        line.invalidate();
        if let Some(cursor) = self.measure.as_mut() {
            if cursor.line_index == line_index {
                let row_start = cursor.row_start;
                cursor.reset_for_line(line_index, row_start);
            }
        }
        if let Some(shift) = hint_shift {
            self.measure_hint_line = line_index;
            self.measure_hint_row_start = self.measure_hint_row_start.saturating_sub(shift);
        }
    }

    /// Keep an in-flight measurement job aligned with the surviving line
    /// deque after rows left the front. A job on a later line keeps its
    /// progress; a job on the partially cut first line restarts from that
    /// line's new row zero.
    fn rebase_measure_after_front_removal(
        &mut self,
        popped_lines: usize,
        removed_rows: usize,
        partial_front_line: bool,
    ) {
        let Some(mut cursor) = self.measure.take() else {
            return;
        };
        if cursor.line_index < popped_lines {
            return;
        }
        cursor.line_index -= popped_lines;
        cursor.row_start = cursor.row_start.saturating_sub(removed_rows);
        if partial_front_line && cursor.line_index == 0 {
            cursor.reset_for_line(0, 0);
        }
        self.measure = Some(cursor);
    }

    /// Keep an in-flight measurement job valid after rows left the back.
    /// Full lines removed after the cursor do not move it. If the cursor's
    /// own line lost rows it had already accounted for, retreat to the last
    /// surviving row boundary. A cursor part-way through a row carries that
    /// row's cells in `state` and its staged metadata, so equality with the
    /// surviving row count is only safe when the cursor has not consumed any
    /// cells from the removed row.
    fn clamp_measure_after_back_removal(&mut self) {
        let Some(cursor) = self.measure.as_mut() else {
            return;
        };
        let Some(line) = self.lines.get(cursor.line_index) else {
            self.measure = None;
            return;
        };
        let surviving_rows = line.row_count;
        let removed_consumed_row = cursor.row_index > surviving_rows
            || (cursor.row_index == surviving_rows && cursor.cell_index > 0);
        if removed_consumed_row {
            let line_index = cursor.line_index;
            let row_start = cursor.row_start;
            if surviving_rows == 0 {
                cursor.reset_for_line(line_index, row_start);
            } else {
                // `boundaries[row]` is the state at the start of that row, so
                // the first removed row's boundary is the end of the retained
                // content. Keep the completed prefix instead of restarting.
                let boundary = cursor.boundaries[surviving_rows];
                let uniform_state = cursor.uniform_prefix[surviving_rows];
                cursor.boundaries.truncate(surviving_rows);
                cursor.cell_prefix.truncate(surviving_rows + 1);
                cursor.uniform_prefix.truncate(surviving_rows);
                cursor.state = boundary;
                cursor.uniform_state = uniform_state;
                cursor.row_index = surviving_rows;
                cursor.cell_index = 0;
            }
        }
    }

    pub(crate) fn published_display_rows(&self) -> usize {
        self.published_display_rows
    }

    /// Exact only while every line has been measured at the current width.
    #[cfg(test)]
    pub(crate) fn total_display_rows(&self) -> LineDelta {
        if self.is_fully_measured() {
            LineDelta::Exact(self.measured_display_rows as isize)
        } else {
            LineDelta::NeedsMeasure
        }
    }

    fn publish_if_fully_measured(&mut self) {
        if self.is_fully_measured() {
            self.published_display_rows = self.measured_display_rows;
        }
    }

    fn apply_measured_delta(&mut self, delta: isize, measured_line_delta: isize) {
        self.measured_display_rows = (self.measured_display_rows as isize + delta).max(0) as usize;
        self.measured_lines = (self.measured_lines as isize + measured_line_delta).max(0) as usize;
    }

    /// A width change invalidates every line summary; already measured lines
    /// are never silently reinterpreted at the new width.  The published
    /// total stays complete until the resumable measurement finishes.
    #[cfg(test)]
    pub(crate) fn set_width(&mut self, width: usize) {
        if self.width == width {
            return;
        }
        self.width = width;
        for line in &mut self.lines {
            let row_count = line.row_count;
            *line = Line::unmeasured(row_count);
        }
        self.measured_lines = 0;
        self.measured_display_rows = 0;
        self.measure = None;
        self.measure_hint_line = 0;
        self.measure_hint_row_start = 0;
        if self.lines.is_empty() {
            self.published_display_rows = 0;
        }
    }

    /// Append a stored row.  A canonical row starts a new line; any other row
    /// extends the last line.  A measured last line is extended from its end
    /// boundary without rescanning earlier rows.
    pub(crate) fn append_row(&mut self, rows: &mut VecDeque<Row>, row: Row) -> LineDelta {
        if self.lines.is_empty() || row.is_canonical {
            let measurement = measure_rows(std::iter::once(&row), self.width);
            let height = measurement.height;
            self.lines
                .push_back(Line::from_measurement(1, self.width, measurement));
            rows.push_back(row);
            self.measured_lines += 1;
            self.measured_display_rows += height;
            self.publish_if_fully_measured();
            return LineDelta::Exact(height as isize);
        }

        let width = self.width;
        let hint_at_end = self.measure_hint_line == self.lines.len();
        let last_line_index = self.lines.len() - 1;
        let line = self.lines.back_mut().expect("checked non-empty");
        if line.measured_width == Some(width) {
            let old_height = line.height;
            let row_uniform_width = row_uniform_character_width(&row);
            #[cfg(test)]
            note_row_visit();
            if line.uniform_character_width.is_some()
                && (row.stored_cell_count() == 0
                    || row_uniform_width == line.uniform_character_width)
            {
                #[cfg(test)]
                note_cell_visits(row.stored_cell_count());
                let character_width = line.uniform_character_width.expect("checked");
                let total = line.cell_prefix.back().copied().unwrap_or(0);
                line.cell_prefix
                    .push_back(total.saturating_add(row.stored_cell_count()));
                line.row_count += 1;
                let cells = line.suffix_cells(line.row_count);
                line.height = uniform_height(cells, character_width, width);
                line.end = uniform_boundary(cells, character_width, width);
                let delta = line.height as isize - old_height as isize;
                rows.push_back(row);
                self.measured_display_rows =
                    (self.measured_display_rows as isize + delta).max(0) as usize;
                if hint_at_end {
                    self.measure_hint_row_start = self.measure_hint_row_start.saturating_add(1);
                }
                self.publish_if_fully_measured();
                LineDelta::Exact(delta)
            } else if line.uniform_character_width.is_some() {
                line.row_count += 1;
                rows.push_back(row);
                self.measured_display_rows =
                    (self.measured_display_rows as isize - old_height as isize).max(0) as usize;
                self.measured_lines -= 1;
                if hint_at_end {
                    self.measure_hint_line = last_line_index;
                    self.measure_hint_row_start = self
                        .measure_hint_row_start
                        .saturating_add(1)
                        .saturating_sub(line.row_count);
                }
                line.invalidate();
                LineDelta::NeedsMeasure
            } else {
                #[cfg(test)]
                note_cell_visits(row.stored_cell_count());
                let boundary = line.end;
                line.boundaries.push(boundary);
                measure_row(&mut line.end, &row, width);
                let total = line.cell_prefix.back().copied().unwrap_or(0);
                line.cell_prefix
                    .push_back(total.saturating_add(row.stored_cell_count()));
                line.row_count += 1;
                line.height = line.height_after(line.end);
                let delta = line.height as isize - old_height as isize;
                rows.push_back(row);
                self.measured_display_rows =
                    (self.measured_display_rows as isize + delta).max(0) as usize;
                if hint_at_end {
                    self.measure_hint_row_start = self.measure_hint_row_start.saturating_add(1);
                }
                self.publish_if_fully_measured();
                LineDelta::Exact(delta)
            }
        } else {
            line.row_count += 1;
            rows.push_back(row);
            LineDelta::NeedsMeasure
        }
    }

    /// Update metadata after `count` rows were removed from the front.  A
    /// single-width partial survivor is cheaply rebased; any other partial
    /// survivor is invalidated so the resumable measure job recomputes it
    /// from column zero of the surviving rows.
    pub(crate) fn note_front_rows_removed(
        &mut self,
        _rows_after: &VecDeque<Row>,
        count: usize,
    ) -> LineDelta {
        if count == 0 {
            return LineDelta::Exact(0);
        }
        let mut remaining = count;
        let mut full_lines = 0;
        let mut measured_delta = 0isize;
        let mut measured_line_delta = 0isize;
        let mut exact = true;
        let mut partial = None;

        for (index, line) in self.lines.iter().enumerate() {
            if remaining == 0 {
                break;
            }
            if remaining >= line.row_count {
                if line.measured_width == Some(self.width) {
                    measured_delta -= line.height as isize;
                    measured_line_delta -= 1;
                } else {
                    exact = false;
                }
                remaining -= line.row_count;
                full_lines = index + 1;
            } else {
                let cut = remaining;
                let surviving_rows = line.row_count - cut;
                if line.measured_width == Some(self.width) {
                    if let Some(character_width) = line.uniform_character_width {
                        let cells = line.cell_prefix[line.cell_prefix_base + line.row_count]
                            .saturating_sub(line.cell_prefix[line.cell_prefix_base + cut]);
                        let height = uniform_height(cells, character_width, self.width);
                        measured_delta += height as isize - line.height as isize;
                        partial = Some((
                            surviving_rows,
                            PartialLine::SingleWidth {
                                cut,
                                height,
                                end: uniform_boundary(cells, character_width, self.width),
                            },
                        ));
                    } else {
                        // A mixed cut cannot be derived from the old origin:
                        // a prefix of the first surviving row may merge into
                        // the previous row and cascade through wide glyphs.
                        // Rebuilding the survivors eagerly would reintroduce
                        // the full-line cliff, so publish `NeedsMeasure` and
                        // let the resumable job recompute under its budget.
                        measured_delta -= line.height as isize;
                        measured_line_delta -= 1;
                        exact = false;
                        partial = Some((surviving_rows, PartialLine::Invalidated));
                    }
                } else {
                    exact = false;
                    partial = Some((surviving_rows, PartialLine::Invalidated));
                }
                break;
            }
        }

        for _ in 0..full_lines {
            self.lines.pop_front().expect("planned full line");
        }
        let partial_front_line = partial.is_some();
        if let Some((surviving_rows, partial)) = partial {
            let line = self.lines.front_mut().expect("surviving partial line");
            line.row_count = surviving_rows;
            match partial {
                PartialLine::SingleWidth { cut, height, end } => {
                    line.measured_width = Some(self.width);
                    line.cell_prefix_base += cut;
                    line.compact_cell_prefix();
                    line.height = height;
                    line.end = end;
                    line.boundaries.clear();
                },
                PartialLine::Invalidated => line.invalidate(),
            }
        }
        let front_unmeasured = partial_front_line
            && self
                .lines
                .front()
                .is_some_and(|line| line.measured_width != Some(self.width));
        if front_unmeasured || self.measure_hint_line < full_lines {
            self.measure_hint_line = 0;
            self.measure_hint_row_start = 0;
        } else {
            self.measure_hint_line -= full_lines;
            self.measure_hint_row_start = self.measure_hint_row_start.saturating_sub(count);
        }
        self.rebase_measure_after_front_removal(full_lines, count, partial_front_line);
        self.apply_measured_delta(measured_delta, measured_line_delta);
        self.publish_if_fully_measured();
        if exact {
            LineDelta::Exact(measured_delta)
        } else {
            LineDelta::NeedsMeasure
        }
    }

    #[cfg(test)]
    pub(crate) fn remove_front(&mut self, rows: &mut VecDeque<Row>, count: usize) -> LineDelta {
        let count = count.min(rows.len());
        if count == 0 {
            return LineDelta::Exact(0);
        }
        rows.drain(..count);
        self.note_front_rows_removed(rows, count)
    }

    /// Update metadata after `count` rows were removed from the back.  The
    /// rows passed here are the surviving deque.
    pub(crate) fn note_back_rows_removed(
        &mut self,
        _rows_after: &VecDeque<Row>,
        count: usize,
    ) -> LineDelta {
        if count == 0 {
            return LineDelta::Exact(0);
        }
        let mut remaining = count;
        let mut full_lines = 0;
        let mut measured_delta = 0isize;
        let mut measured_line_delta = 0isize;
        let mut exact = true;
        let mut partial = None;
        let hint_was_at_end = self.measure_hint_line == self.lines.len();

        for line in self.lines.iter().rev() {
            if remaining == 0 {
                break;
            }
            if remaining >= line.row_count {
                if line.measured_width == Some(self.width) {
                    measured_delta -= line.height as isize;
                    measured_line_delta -= 1;
                } else {
                    exact = false;
                }
                remaining -= line.row_count;
                full_lines += 1;
            } else {
                let surviving_rows = line.row_count - remaining;
                let measured_height = if line.measured_width == Some(self.width) {
                    if let Some(character_width) = line.uniform_character_width {
                        let cells = line.cell_prefix[line.cell_prefix_base + surviving_rows]
                            .saturating_sub(line.cell_prefix[line.cell_prefix_base]);
                        uniform_height(cells, character_width, self.width)
                    } else {
                        line.boundaries[surviving_rows].height()
                    }
                } else {
                    exact = false;
                    0
                };
                let partial_height = if line.measured_width == Some(self.width) {
                    measured_delta += measured_height as isize - line.height as isize;
                    Some(measured_height)
                } else {
                    None
                };
                partial = Some((surviving_rows, partial_height));
                break;
            }
        }

        for _ in 0..full_lines {
            self.lines.pop_back().expect("planned full line");
        }
        if let Some((surviving_rows, partial_height)) = partial {
            let line = self.lines.back_mut().expect("surviving partial line");
            line.row_count = surviving_rows;
            if let Some(height) = partial_height {
                if let Some(character_width) = line.uniform_character_width {
                    let cells = line.suffix_cells(surviving_rows);
                    line.end = uniform_boundary(cells, character_width, self.width);
                    line.cell_prefix
                        .truncate(line.cell_prefix_base + surviving_rows + 1);
                } else {
                    line.end = line.boundaries[surviving_rows];
                    line.boundaries.truncate(surviving_rows);
                    line.cell_prefix
                        .truncate(line.cell_prefix_base + surviving_rows + 1);
                }
                line.height = height;
                line.release_oversized_backing();
            } else {
                line.invalidate();
            }
        }
        if hint_was_at_end {
            self.measure_hint_line = self.lines.len();
            self.measure_hint_row_start = self.measure_hint_row_start.saturating_sub(count);
        } else if self.measure_hint_line > self.lines.len() {
            self.measure_hint_line = 0;
            self.measure_hint_row_start = 0;
        }
        self.clamp_measure_after_back_removal();
        self.apply_measured_delta(measured_delta, measured_line_delta);
        self.publish_if_fully_measured();
        if exact {
            LineDelta::Exact(measured_delta)
        } else {
            LineDelta::NeedsMeasure
        }
    }

    pub(crate) fn take_back(
        &mut self,
        rows: &mut VecDeque<Row>,
        count: usize,
    ) -> (Vec<Row>, LineDelta) {
        let count = count.min(rows.len());
        if count == 0 {
            return (Vec::new(), LineDelta::Exact(0));
        }
        let tail = rows.split_off(rows.len() - count);
        let delta = self.note_back_rows_removed(rows, count);
        (tail.into_iter().collect(), delta)
    }

    /// Prepend rows (viewport -> scrollback).  A non-canonical existing first
    /// row merges with the last prepended logical line; otherwise the groups
    /// stay separate.
    #[cfg(test)]
    pub(crate) fn prepend_rows(
        &mut self,
        rows: &mut VecDeque<Row>,
        new_rows: Vec<Row>,
    ) -> LineDelta {
        if new_rows.is_empty() {
            return LineDelta::Exact(0);
        }
        self.measure = None;
        self.measure_hint_line = 0;
        self.measure_hint_row_start = 0;

        let mut group_counts = Vec::new();
        for row in &new_rows {
            if group_counts.is_empty() || row.is_canonical {
                group_counts.push(1);
            } else {
                *group_counts.last_mut().expect("non-empty") += 1;
            }
        }

        let mut offset = 0;
        let mut measurements = Vec::with_capacity(group_counts.len());
        for &group_count in &group_counts {
            measurements.push(measure_rows(
                new_rows[offset..offset + group_count].iter(),
                self.width,
            ));
            offset += group_count;
        }

        let merge = rows.front().map(|row| !row.is_canonical).unwrap_or(false);
        let old_first_rows = self.lines.front().map(|line| line.row_count).unwrap_or(0);
        let old_first_height = if merge {
            self.ensure_line_measured(rows, 0)
        } else {
            0
        };

        let last_group_count = group_counts.last().copied().unwrap_or(0);
        let last_group_start = new_rows.len().saturating_sub(last_group_count);
        let merged = if merge {
            Some(measure_rows(
                new_rows[last_group_start..]
                    .iter()
                    .chain(rows.iter().take(old_first_rows)),
                self.width,
            ))
        } else {
            None
        };

        let separate_groups = if merge {
            group_counts.len().saturating_sub(1)
        } else {
            group_counts.len()
        };
        let mut separate_lines = Vec::with_capacity(separate_groups);
        let mut separate_height = 0usize;
        for (group_count, measurement) in group_counts
            .iter()
            .copied()
            .zip(measurements.into_iter())
            .take(separate_groups)
        {
            separate_height += measurement.height;
            separate_lines.push(Line::from_measurement(group_count, self.width, measurement));
        }

        let mut delta = separate_height as isize;
        if let Some(merged) = merged {
            let combined_height = merged.height;
            let combined_rows = last_group_count + old_first_rows;
            let line = self.lines.front_mut().expect("existing first line");
            delta += combined_height as isize - old_first_height as isize;
            line.row_count = combined_rows;
            line.measured_width = Some(self.width);
            line.boundaries = if merged.uniform_character_width.is_some() {
                Vec::new()
            } else {
                merged.boundaries
            };
            line.end = merged.end;
            line.height = combined_height;
            line.cell_prefix = merged.cell_prefix;
            line.cell_prefix_base = 0;
            line.uniform_character_width = merged.uniform_character_width;
        }

        self.measured_lines += separate_lines.len();
        self.measured_display_rows = (self.measured_display_rows as isize + delta).max(0) as usize;
        for line in separate_lines.into_iter().rev() {
            self.lines.push_front(line);
        }
        for row in new_rows.into_iter().rev() {
            rows.push_front(row);
        }
        self.publish_if_fully_measured();
        LineDelta::Exact(delta)
    }

    #[cfg(test)]
    fn ensure_line_measured(&mut self, rows: &VecDeque<Row>, line_index: usize) -> usize {
        if self
            .lines
            .get(line_index)
            .is_some_and(|line| line.measured_width == Some(self.width))
        {
            return self.lines[line_index].height;
        }
        self.measure_line(rows, line_index).unwrap_or(0)
    }

    /// Measure one line synchronously.  Test-only: production summaries are
    /// rebuilt by [`Self::measure_next`] under the caller's work budget.
    #[cfg(test)]
    pub(crate) fn measure_line(
        &mut self,
        rows: &VecDeque<Row>,
        line_index: usize,
    ) -> Option<usize> {
        if line_index >= self.lines.len() {
            return None;
        }
        self.measure = None;
        let row_start = self
            .lines
            .iter()
            .take(line_index)
            .map(|line| line.row_count)
            .sum();
        let row_count = self.lines[line_index].row_count;
        let measurement = measure_rows(rows.iter().skip(row_start).take(row_count), self.width);
        let height = measurement.height;
        let line = &mut self.lines[line_index];
        if line.measured_width == Some(self.width) {
            self.measured_display_rows -= line.height;
            self.measured_lines -= 1;
        }
        line.measured_width = Some(self.width);
        line.boundaries = if measurement.uniform_character_width.is_some() {
            Vec::new()
        } else {
            measurement.boundaries
        };
        line.end = measurement.end;
        line.height = height;
        line.cell_prefix = measurement.cell_prefix;
        line.cell_prefix_base = 0;
        line.uniform_character_width = measurement.uniform_character_width;
        self.measured_lines += 1;
        self.measured_display_rows += height;
        self.publish_if_fully_measured();
        Some(height)
    }

    fn first_unmeasured(&mut self) -> Option<(usize, usize)> {
        let mut line_index = self.measure_hint_line.min(self.lines.len());
        let mut row_start = self.measure_hint_row_start;
        while line_index < self.lines.len() {
            #[cfg(test)]
            note_line_scan_visit();
            let line = &self.lines[line_index];
            if line.measured_width != Some(self.width) {
                self.measure_hint_line = line_index;
                self.measure_hint_row_start = row_start;
                return Some((line_index, row_start));
            }
            row_start = row_start.saturating_add(line.row_count);
            line_index += 1;
        }
        self.measure_hint_line = line_index;
        self.measure_hint_row_start = row_start;
        None
    }

    /// Measure at most one logical line per call, in chunks bounded by
    /// `budget`.  A zero budget still advances by one row/cell so callers
    /// cannot stall progress.
    pub(crate) fn measure_next(
        &mut self,
        rows: &VecDeque<Row>,
        budget: MeasureBudget,
    ) -> MeasureProgress {
        let mut cursor = match self.measure.take() {
            Some(cursor) if cursor.width == self.width => cursor,
            _ => {
                let Some((line_index, row_start)) = self.first_unmeasured() else {
                    return MeasureProgress::Done;
                };
                MeasureCursor::new(line_index, row_start, self.width)
            },
        };

        let mut rows_remaining = budget.rows.max(1);
        let mut cells_remaining = budget.cells.max(1);
        loop {
            if cursor.line_index >= self.lines.len() {
                if self.is_fully_measured() {
                    self.measure_hint_line = self.lines.len();
                    self.measure_hint_row_start =
                        self.lines.iter().map(|line| line.row_count).sum();
                }
                self.measure = None;
                return MeasureProgress::Done;
            }
            let line_row_count = self.lines[cursor.line_index].row_count;
            if cursor.row_index >= line_row_count {
                let height = cursor.state.height();
                let row_count = line_row_count;
                let line = &mut self.lines[cursor.line_index];
                if line.measured_width == Some(self.width) {
                    self.measured_display_rows -= line.height;
                    self.measured_lines -= 1;
                }
                let uniform_character_width = cursor.uniform_state.resolved();
                line.measured_width = Some(self.width);
                line.boundaries = if uniform_character_width.is_some() {
                    Vec::new()
                } else {
                    std::mem::take(&mut cursor.boundaries)
                };
                line.end = cursor.state;
                line.height = height;
                line.cell_prefix = std::mem::take(&mut cursor.cell_prefix);
                line.cell_prefix_base = 0;
                line.uniform_character_width = uniform_character_width;
                self.measured_lines += 1;
                self.measured_display_rows += height;

                cursor.row_start += row_count;
                let mut next_line = cursor.line_index + 1;
                while next_line < self.lines.len()
                    && self.lines[next_line].measured_width == Some(self.width)
                {
                    cursor.row_start += self.lines[next_line].row_count;
                    next_line += 1;
                }
                if next_line >= self.lines.len() {
                    if self.is_fully_measured() {
                        self.measure_hint_line = self.lines.len();
                        self.measure_hint_row_start = cursor.row_start;
                    }
                    self.measure = None;
                    self.publish_if_fully_measured();
                    return MeasureProgress::Done;
                }
                cursor.reset_for_line(next_line, cursor.row_start);
                continue;
            }

            if self.width == 0 {
                if rows_remaining == 0 {
                    self.measure = Some(cursor);
                    return MeasureProgress::Pending;
                }
                let rows_left = line_row_count - cursor.row_index;
                let processed = rows_left.min(rows_remaining);
                for row_index in cursor.row_index..cursor.row_index + processed {
                    #[cfg(test)]
                    {
                        note_row_visit();
                        note_boundary_discovery();
                    }
                    let row = &rows[cursor.row_start + row_index];
                    let total = cursor.cell_prefix.back().copied().unwrap_or(0);
                    cursor
                        .cell_prefix
                        .push_back(total.saturating_add(row.stored_cell_count()));
                    cursor.uniform_prefix.push(cursor.uniform_state);
                    cursor
                        .uniform_state
                        .observe(row.stored_cell_count(), row_uniform_character_width(row));
                }
                cursor
                    .boundaries
                    .extend(std::iter::repeat_n(RowBoundary::default(), processed));
                cursor.row_index += processed;
                rows_remaining -= processed;
                continue;
            }

            if cursor.cell_index == 0 {
                if rows_remaining == 0 || cells_remaining == 0 {
                    self.measure = Some(cursor);
                    return MeasureProgress::Pending;
                }
                #[cfg(test)]
                {
                    note_row_visit();
                    note_boundary_discovery();
                }
                cursor.boundaries.push(cursor.state);
                rows_remaining -= 1;
            }
            let row = &rows[cursor.row_start + cursor.row_index];
            let columns = row.columns.len();
            let total_cells = row.stored_cell_count();
            if cursor.cell_index == 0 {
                let total = cursor.cell_prefix.back().copied().unwrap_or(0);
                cursor
                    .cell_prefix
                    .push_back(total.saturating_add(total_cells));
                cursor.uniform_prefix.push(cursor.uniform_state);
                cursor
                    .uniform_state
                    .observe(total_cells, row_uniform_character_width(row));
            }
            if cursor.cell_index >= total_cells {
                cursor.row_index += 1;
                cursor.cell_index = 0;
                continue;
            }
            if cells_remaining == 0 {
                self.measure = Some(cursor);
                return MeasureProgress::Pending;
            }
            let character_width = if cursor.cell_index < columns {
                row.columns[cursor.cell_index].width()
            } else {
                1
            };
            cursor.state.push_character(character_width, self.width);
            #[cfg(test)]
            note_cell_visits(1);
            cursor.cell_index += 1;
            cells_remaining -= 1;
        }
    }

    pub(crate) fn measure_all(&mut self, rows: &VecDeque<Row>) -> usize {
        while self.measure_next(rows, MeasureBudget::new(usize::MAX, usize::MAX))
            == MeasureProgress::Pending
        {}
        self.measured_display_rows
    }
}

#[cfg(test)]
#[path = "./unit/line_index_tests.rs"]
mod line_index_tests;
