use super::*;
use crate::panes::terminal_character::TerminalCharacter;

fn character_row(text: &str, canonical: bool) -> Row {
    let mut row = Row::from_columns(text.chars().map(TerminalCharacter::new).collect());
    if canonical {
        row = row.canonical();
    }
    row
}

fn character_row_with_blanks(text: &str, blanks: usize, canonical: bool) -> Row {
    let mut columns: Vec<TerminalCharacter> = text.chars().map(TerminalCharacter::new).collect();
    columns.extend(std::iter::repeat_n(TerminalCharacter::new(' '), blanks));
    let mut row = Row::from_columns(columns);
    row.compress_trailing_blanks();
    if canonical {
        row = row.canonical();
    }
    row
}

fn empty_row(canonical: bool) -> Row {
    let row = Row::new();
    if canonical {
        row.canonical()
    } else {
        row
    }
}

struct Fixture {
    rows: VecDeque<Row>,
    index: LineIndex,
}

impl Fixture {
    fn new(rows: Vec<Row>, width: usize) -> Self {
        let rows: VecDeque<Row> = rows.into();
        let mut index = LineIndex::new(width);
        index.regroup(&rows, width);
        index.measure_all(&rows);
        Self { rows, index }
    }

    fn append(&mut self, row: Row) -> LineDelta {
        self.index.append_row(&mut self.rows, row)
    }

    fn evict_front(&mut self, count: usize) -> LineDelta {
        self.index.remove_front(&mut self.rows, count)
    }

    fn take_back(&mut self, count: usize) -> (Vec<Row>, LineDelta) {
        self.index.take_back(&mut self.rows, count)
    }

    fn prepend(&mut self, rows: Vec<Row>) -> LineDelta {
        self.index.prepend_rows(&mut self.rows, rows)
    }

    fn set_width(&mut self, width: usize) {
        self.index.set_width(width);
    }

    fn measure_next(&mut self, rows: usize, cells: usize) -> MeasureProgress {
        self.index
            .measure_next(&self.rows, MeasureBudget::new(rows, cells))
    }

    fn measure_all(&mut self) {
        self.index.measure_all(&self.rows);
    }

    fn line_counts(&self) -> Vec<usize> {
        self.index.lines.iter().map(|line| line.row_count).collect()
    }
}

fn reference_wrap_height<'a>(rows: impl IntoIterator<Item = &'a Row>, width: usize) -> usize {
    if width == 0 {
        return 1;
    }
    let mut completed = 0usize;
    let mut current = 0usize;
    let mut push = |character_width: usize| {
        if current + character_width > width {
            if current > 0 {
                completed += 1;
            }
            current = 0;
        }
        current += character_width;
    };
    for row in rows {
        for character in &row.columns {
            push(character.width());
        }
        for _ in 0..(row.stored_cell_count() - row.columns.len()) {
            push(1);
        }
    }
    (completed + usize::from(current > 0)).max(1)
}

fn reference_line_counts(rows: &VecDeque<Row>) -> Vec<usize> {
    let mut counts = Vec::new();
    for (index, row) in rows.iter().enumerate() {
        if index == 0 || row.is_canonical {
            counts.push(1);
        } else {
            *counts.last_mut().expect("non-empty") += 1;
        }
    }
    counts
}

fn reference_heights(rows: &VecDeque<Row>, width: usize) -> Vec<usize> {
    let mut heights = Vec::new();
    let mut start = 0;
    for (index, _row) in rows.iter().enumerate() {
        let continues = rows
            .get(index + 1)
            .map(|next| !next.is_canonical)
            .unwrap_or(false);
        if !continues {
            heights.push(reference_wrap_height(rows.range(start..=index), width));
            start = index + 1;
        }
    }
    heights
}

fn reference_total(rows: &VecDeque<Row>, width: usize) -> usize {
    reference_heights(rows, width).into_iter().sum()
}

fn row_signature(row: &Row) -> (bool, Vec<usize>, usize) {
    (
        row.is_canonical,
        row.columns
            .iter()
            .map(|character| character.width())
            .collect(),
        row.stored_cell_count() - row.columns.len(),
    )
}

fn assert_exact_total(fixture: &Fixture, width: usize) {
    assert_eq!(
        fixture.index.total_display_rows(),
        LineDelta::Exact(reference_total(&fixture.rows, width) as isize),
        "exact total must match the reference model"
    );
    assert_eq!(
        fixture.line_counts(),
        reference_line_counts(&fixture.rows),
        "logical-line grouping must match the reference model"
    );
    assert_eq!(
        (0..fixture.index.line_count())
            .map(|line| fixture.index.line_height(line).unwrap())
            .collect::<Vec<_>>(),
        reference_heights(&fixture.rows, width)
    );
}

#[test]
fn groups_canonical_and_continuation_rows() {
    let fixture = Fixture::new(
        vec![
            character_row("a", true),
            character_row("b", false),
            character_row("c", true),
        ],
        80,
    );
    assert_eq!(fixture.index.line_count(), 2);
    assert_eq!(fixture.line_counts(), vec![2, 1]);
    assert_exact_total(&fixture, 80);
}

#[test]
fn wide_characters_are_never_split() {
    let fixture = Fixture::new(vec![character_row("a界b", true)], 3);
    assert_eq!(fixture.index.total_display_rows(), LineDelta::Exact(2));
    assert_exact_total(&fixture, 3);

    let fixture = Fixture::new(vec![character_row("界界", true)], 2);
    assert_eq!(fixture.index.total_display_rows(), LineDelta::Exact(2));
    assert_exact_total(&fixture, 2);

    let fixture = Fixture::new(vec![character_row("界", true)], 1);
    assert_exact_total(&fixture, 1);
}

#[test]
fn width_change_invalidates_summaries_and_resumable_measurement_is_exact() {
    let mut fixture = Fixture::new(
        vec![
            character_row("abcdefghij", true),
            character_row("klmnop", false),
            character_row("qrstuvwxyz", true),
        ],
        80,
    );
    fixture.set_width(4);
    assert_eq!(fixture.index.total_display_rows(), LineDelta::NeedsMeasure);
    let mut progress = MeasureProgress::Pending;
    while progress == MeasureProgress::Pending {
        progress = fixture.measure_next(1, 1);
    }
    assert_eq!(progress, MeasureProgress::Done);
    assert_exact_total(&fixture, 4);
}

#[test]
fn zero_width_treats_each_line_as_one_display_row() {
    let mut fixture = Fixture::new(
        vec![
            character_row("abc", true),
            character_row("de", false),
            character_row("f", true),
        ],
        0,
    );
    assert_eq!(fixture.index.total_display_rows(), LineDelta::Exact(2));

    fixture.set_width(5);
    fixture.set_width(0);
    assert_eq!(fixture.index.total_display_rows(), LineDelta::NeedsMeasure);
    let mut progress = MeasureProgress::Pending;
    while progress == MeasureProgress::Pending {
        progress = fixture.measure_next(1, 1);
    }
    assert_eq!(progress, MeasureProgress::Done);
    assert_eq!(fixture.index.total_display_rows(), LineDelta::Exact(2));
    assert_eq!(fixture.index.line_height(0), Some(1));
    assert_eq!(fixture.index.line_height(1), Some(1));
}

#[test]
fn append_updates_measured_line_incrementally() {
    let width = 5;
    let mut fixture = Fixture::new(
        vec![character_row("abc", true), character_row("def", false)],
        width,
    );
    let old_total = fixture.index.total_display_rows().exact().unwrap();
    let delta = fixture.append(character_row("ghijk", false));
    assert_eq!(delta, LineDelta::Exact(1));
    assert_eq!(
        fixture.index.total_display_rows().exact(),
        Some(old_total + 1)
    );
    assert_exact_total(&fixture, width);
    assert_eq!(fixture.index.lines.back().unwrap().row_count, 3);
}

#[test]
fn append_to_unmeasured_last_line_is_incorporated_when_measured() {
    let width = 4;
    let mut fixture = Fixture::new(
        vec![character_row("abc", true), character_row("defgh", true)],
        10,
    );
    fixture.set_width(width);
    while fixture.index.line_height(0).is_none() {
        assert_eq!(fixture.measure_next(1, 1), MeasureProgress::Pending);
    }
    assert_eq!(fixture.index.line_height(0), Some(1));
    assert_eq!(fixture.index.line_height(1), None);

    fixture.append(character_row("ij", false));
    assert_eq!(fixture.append(empty_row(false)), LineDelta::NeedsMeasure);
    fixture.measure_all();
    assert_exact_total(&fixture, width);
}

#[test]
fn append_during_an_active_measurement_is_incorporated() {
    let width = 4;
    let mut fixture = Fixture::new(vec![character_row("abcdefgh", true)], 100);
    fixture.set_width(width);
    assert_eq!(fixture.measure_next(1, 2), MeasureProgress::Pending);
    assert_eq!(
        fixture.append(character_row("ijkl", false)),
        LineDelta::NeedsMeasure
    );

    let mut progress = MeasureProgress::Pending;
    while progress == MeasureProgress::Pending {
        progress = fixture.measure_next(1, 1);
    }
    assert_eq!(progress, MeasureProgress::Done);
    assert_eq!(fixture.index.total_display_rows(), LineDelta::Exact(3));
    assert_eq!(fixture.index.lines.front().unwrap().row_count, 2);
    assert_exact_total(&fixture, width);
}

#[test]
fn structural_removal_while_unmeasured_updates_grouping() {
    let width = 3;
    let mut fixture = Fixture::new(
        vec![
            character_row("abcde", true),
            character_row("fghij", false),
            character_row("klm", true),
        ],
        5,
    );
    fixture.set_width(width);
    assert_eq!(fixture.evict_front(2), LineDelta::NeedsMeasure);
    assert_eq!(fixture.line_counts(), reference_line_counts(&fixture.rows));
    fixture.measure_all();
    assert_exact_total(&fixture, width);
}

#[test]
fn partial_front_eviction_mid_open_row_rebases_to_a_start_boundary() {
    let width = 5;
    let mut fixture = Fixture::new(
        vec![character_row("abc", true), character_row("xyz", false)],
        width,
    );
    assert_eq!(fixture.index.total_display_rows(), LineDelta::Exact(2));
    assert_eq!(fixture.evict_front(1), LineDelta::Exact(-1));
    assert_eq!(fixture.index.total_display_rows(), LineDelta::Exact(1));
    assert_eq!(
        fixture.index.lines.front().unwrap().boundary_at(0, width),
        RowBoundary::default()
    );
    assert_exact_total(&fixture, width);

    let mut fixture = Fixture::new(
        vec![
            character_row("abcde", true),
            character_row("fghij", false),
            character_row("klm", false),
        ],
        width,
    );
    let before = reference_total(&fixture.rows, width) as isize;
    let removed = fixture.evict_front(2).exact().unwrap();
    assert_eq!(
        fixture.index.total_display_rows().exact().unwrap(),
        before + removed
    );
    assert_eq!(fixture.evict_front(0), LineDelta::Exact(0));
    assert_exact_total(&fixture, width);
}

#[test]
fn partial_front_eviction_after_completed_rows_rebases_the_open_row() {
    let width = 5;
    let mut fixture = Fixture::new(
        vec![
            character_row("abcde", true),
            character_row("fghij", false),
            character_row("klmno", false),
        ],
        width,
    );
    assert_eq!(fixture.index.total_display_rows(), LineDelta::Exact(3));
    assert_eq!(
        fixture.index.lines.front().unwrap().boundary_at(2, width),
        RowBoundary {
            completed_rows: 1,
            current: 5
        }
    );
    assert_eq!(fixture.evict_front(2), LineDelta::Exact(-2));
    assert_eq!(fixture.index.total_display_rows(), LineDelta::Exact(1));
    assert_exact_total(&fixture, width);
}

#[test]
fn whole_line_front_eviction_removes_the_line_height() {
    let width = 5;
    let mut fixture = Fixture::new(
        vec![
            character_row("abcde", true),
            character_row("fghij", false),
            character_row("z", true),
        ],
        width,
    );
    assert_eq!(fixture.index.total_display_rows(), LineDelta::Exact(3));
    assert_eq!(fixture.evict_front(2), LineDelta::Exact(-2));
    assert_eq!(fixture.index.line_count(), 1);
    assert_eq!(fixture.index.total_display_rows(), LineDelta::Exact(1));
    assert_exact_total(&fixture, width);
}

#[test]
fn back_transfer_restores_the_last_line_boundary() {
    let width = 5;
    let mut fixture = Fixture::new(
        vec![character_row("abcde", true), character_row("f", false)],
        width,
    );
    assert_eq!(fixture.index.total_display_rows(), LineDelta::Exact(2));
    let before = fixture.rows.iter().map(row_signature).collect::<Vec<_>>();
    let (taken, delta) = fixture.take_back(1);
    assert_eq!(delta, LineDelta::Exact(-1));
    assert_eq!(taken.len(), 1);
    assert_eq!(row_signature(&taken[0]), before[before.len() - 1]);
    assert_eq!(fixture.index.total_display_rows(), LineDelta::Exact(1));
    assert_eq!(fixture.index.lines.back().unwrap().row_count, 1);
    assert_exact_total(&fixture, width);
}

#[test]
fn back_transfer_across_a_canonical_boundary_drops_the_whole_line() {
    let width = 10;
    let mut fixture = Fixture::new(
        vec![character_row("abcde", true), character_row("fghij", true)],
        width,
    );
    assert_eq!(fixture.take_back(1).1, LineDelta::Exact(-1));
    assert_eq!(fixture.index.line_count(), 1);
    assert_eq!(fixture.index.total_display_rows(), LineDelta::Exact(1));
    assert_exact_total(&fixture, width);
}

#[test]
fn prepend_merges_with_a_continuation_first_line() {
    let width = 10;
    let mut fixture = Fixture::new(vec![character_row("b", false)], width);
    assert_eq!(
        fixture.prepend(vec![character_row("a", true)]),
        LineDelta::Exact(0)
    );
    assert_eq!(fixture.index.line_count(), 1);
    assert_eq!(fixture.index.line_row_count(0), Some(2));
    assert_eq!(
        fixture.index.lines.front().unwrap().boundary_at(0, width),
        RowBoundary::default()
    );
    assert_eq!(
        fixture.index.lines.front().unwrap().boundary_at(1, width),
        RowBoundary {
            completed_rows: 0,
            current: 1
        }
    );
    assert_exact_total(&fixture, width);

    let mut fixture = Fixture::new(vec![character_row("b", true)], width);
    assert_eq!(
        fixture.prepend(vec![character_row("a", false)]),
        LineDelta::Exact(1)
    );
    assert_eq!(fixture.index.line_count(), 2);
    assert_eq!(fixture.line_counts(), vec![1, 1]);
    assert_exact_total(&fixture, width);
}

#[test]
fn prepend_multiple_groups_merges_only_the_last_group() {
    let width = 5;
    let mut fixture = Fixture::new(vec![character_row("b", false)], width);
    assert_eq!(
        fixture.prepend(vec![character_row("a", false), character_row("cc", true)]),
        LineDelta::Exact(1)
    );
    assert_eq!(fixture.line_counts(), vec![1, 2]);
    assert_exact_total(&fixture, width);
}

#[test]
fn prepend_into_an_empty_index_creates_lines() {
    let width = 5;
    let mut fixture = Fixture::new(Vec::new(), width);
    assert_eq!(
        fixture.prepend(vec![character_row("abc", true), character_row("de", false),]),
        LineDelta::Exact(1)
    );
    assert_eq!(fixture.index.line_count(), 1);
    assert_eq!(fixture.index.line_row_count(0), Some(2));
    assert_exact_total(&fixture, width);
}

#[test]
fn empty_continuation_rows_do_not_change_height_but_can_be_transferred() {
    let width = 5;
    let mut fixture = Fixture::new(
        vec![
            character_row("abc", true),
            empty_row(false),
            empty_row(false),
        ],
        width,
    );
    assert_eq!(fixture.index.total_display_rows(), LineDelta::Exact(1));
    let (taken, delta) = fixture.take_back(2);
    assert_eq!(delta, LineDelta::Exact(0));
    assert_eq!(taken.len(), 2);
    assert_eq!(fixture.index.line_row_count(0), Some(1));
    assert_exact_total(&fixture, width);
}

#[test]
fn transferring_every_row_empties_the_index() {
    let width = 5;
    let mut fixture = Fixture::new(
        vec![character_row("abc", true), character_row("def", false)],
        width,
    );
    let (taken, delta) = fixture.take_back(2);
    assert_eq!(taken.len(), 2);
    assert_eq!(delta, LineDelta::Exact(-2));
    assert_eq!(fixture.rows.len(), 0);
    assert_eq!(fixture.index.line_count(), 0);
    assert_eq!(fixture.index.total_display_rows(), LineDelta::Exact(0));
}

#[test]
fn trailing_blanks_use_their_stored_width() {
    let width = 7;
    let fixture = Fixture::new(vec![character_row_with_blanks("ab", 8, true)], width);
    assert_exact_total(&fixture, width);
    assert_eq!(fixture.index.total_display_rows(), LineDelta::Exact(2));
}

struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0
    }

    fn below(&mut self, limit: usize) -> usize {
        (self.next() % limit as u64) as usize
    }
}

fn random_row(rng: &mut Lcg) -> Row {
    let canonical = rng.below(3) == 0;
    let length = rng.below(9);
    let mut columns = Vec::new();
    for _ in 0..length {
        let character = match rng.below(6) {
            0 => '界',
            1 => 'a',
            2 => ' ',
            3 => 'é',
            4 => '1',
            _ => 'x',
        };
        columns.push(TerminalCharacter::new(character));
    }
    let blanks = if length == 0 {
        rng.below(3)
    } else {
        rng.below(2)
    };
    columns.extend(std::iter::repeat_n(TerminalCharacter::new(' '), blanks));
    let mut row = Row::from_columns(columns);
    row.compress_trailing_blanks();
    if canonical {
        row = row.canonical();
    }
    row
}

#[test]
fn randomized_operations_match_the_reference_model() {
    let mut rng = Lcg(0x5eed_1234_9876_abcd);
    let mut width = 1 + rng.below(10);
    let mut fixture = Fixture::new(Vec::new(), width);

    for step in 0..10_000 {
        let operation = rng.below(10);
        match operation {
            0..=3 => {
                let row = random_row(&mut rng);
                let _ = fixture.append(row);
            },
            4 => {
                width = 1 + rng.below(10);
                fixture.set_width(width);
            },
            5 => {
                let _ = fixture.measure_next(rng.below(5) + 1, rng.below(8) + 1);
            },
            6 => {
                fixture.measure_all();
                let count = rng.below(fixture.rows.len() + 1);
                let before = fixture.rows.iter().map(row_signature).collect::<Vec<_>>();
                let _ = fixture.evict_front(count);
                assert_eq!(
                    fixture.rows.iter().map(row_signature).collect::<Vec<_>>(),
                    before[count..]
                );
            },
            7 => {
                fixture.measure_all();
                let count = rng.below(fixture.rows.len() + 1);
                let before = fixture.rows.iter().map(row_signature).collect::<Vec<_>>();
                let (taken, _) = fixture.take_back(count);
                assert_eq!(
                    taken.iter().map(row_signature).collect::<Vec<_>>(),
                    before[before.len() - count..]
                );
                assert_eq!(
                    fixture.rows.iter().map(row_signature).collect::<Vec<_>>(),
                    before[..before.len() - count]
                );
            },
            8 => {
                let prepend_count = rng.below(4);
                let added = (0..prepend_count)
                    .map(|_| random_row(&mut rng))
                    .collect::<Vec<_>>();
                let before = fixture.rows.iter().map(row_signature).collect::<Vec<_>>();
                let _ = fixture.prepend(added.clone());
                let actual = fixture.rows.iter().map(row_signature).collect::<Vec<_>>();
                assert_eq!(
                    actual[..added.len()],
                    added.iter().map(row_signature).collect::<Vec<_>>()[..]
                );
                assert_eq!(actual[added.len()..], before[..]);
            },
            _ => {
                fixture.measure_all();
            },
        }

        for line in &fixture.index.lines {
            line.check_prefix(&format!("step {step} op {operation}"));
        }
        if fixture.index.is_fully_measured() {
            assert_exact_total(&fixture, width);
        } else {
            assert_eq!(fixture.index.total_display_rows(), LineDelta::NeedsMeasure);
        }
        let _ = step;
    }
}
