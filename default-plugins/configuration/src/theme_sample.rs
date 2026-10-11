use zellij_tile::prelude::*;
use zellij_utils::input::config_blocks::styling_from_colours;

const LABEL_WIDTH: usize = 12;
pub const SAMPLE_MIN_WIDTH: usize = LABEL_WIDTH + 40;

fn fg_code(colour: PaletteColor) -> String {
    match colour {
        PaletteColor::Rgb((r, g, b)) => format!("\u{1b}[38;2;{};{};{}m", r, g, b),
        PaletteColor::EightBit(index) => format!("\u{1b}[38;5;{}m", index),
    }
}

fn bg_code(colour: PaletteColor) -> String {
    match colour {
        PaletteColor::Rgb((r, g, b)) => format!("\u{1b}[48;2;{};{};{}m", r, g, b),
        PaletteColor::EightBit(index) => format!("\u{1b}[48;5;{}m", index),
    }
}

#[derive(Clone, Copy)]
enum Part {
    Base,
    Emphasis(usize),
}

struct Line {
    text: String,
    width: usize,
    max: usize,
}

impl Line {
    fn new(max: usize) -> Self {
        Line {
            text: String::new(),
            width: 0,
            max,
        }
    }
    fn push(
        &mut self,
        text: &str,
        fg: Option<PaletteColor>,
        bg: Option<PaletteColor>,
        bold: bool,
    ) -> &mut Self {
        let room = self.max.saturating_sub(self.width);
        let shown: String = text.chars().take(room).collect();
        if shown.is_empty() {
            return self;
        }
        self.width += shown.chars().count();
        self.text.push_str("\u{1b}[0m");
        if bold {
            self.text.push_str("\u{1b}[1m");
        }
        if let Some(fg) = fg {
            self.text.push_str(&fg_code(fg));
        }
        if let Some(bg) = bg {
            self.text.push_str(&bg_code(bg));
        }
        self.text.push_str(&shown);
        self
    }
    fn styled(
        &mut self,
        declaration: &StyleDeclaration,
        parts: &[(&str, Part)],
        with_background: bool,
    ) -> &mut Self {
        let bg = if with_background {
            Some(declaration.background)
        } else {
            None
        };
        for (text, part) in parts {
            let (fg, bold) = match part {
                Part::Base => (declaration.base, false),
                Part::Emphasis(0) => (declaration.emphasis_0, true),
                Part::Emphasis(1) => (declaration.emphasis_1, true),
                Part::Emphasis(2) => (declaration.emphasis_2, true),
                Part::Emphasis(_) => (declaration.emphasis_3, true),
            };
            self.push(text, Some(fg), bg, bold);
        }
        self
    }
    fn print(&self, x: usize, y: usize) {
        print!("\u{1b}[{};{}H{}\u{1b}[0m", y + 1, x + 1, self.text);
    }
}

const EMPHASIS: [(&str, Part); 8] = [
    (" ", Part::Base),
    ("e0", Part::Emphasis(0)),
    (" ", Part::Base),
    ("e1", Part::Emphasis(1)),
    (" ", Part::Base),
    ("e2", Part::Emphasis(2)),
    (" ", Part::Base),
    ("e3", Part::Emphasis(3)),
];

fn with_emphasis<'a>(before: &'a str, after: &'a str) -> Vec<(&'a str, Part)> {
    let mut parts = vec![(before, Part::Base)];
    parts.extend(EMPHASIS.iter().copied());
    parts.push((after, Part::Base));
    parts
}

const ARROW: &str = "\u{e0b0}";

fn ribbon(
    line: &mut Line,
    declaration: &StyleDeclaration,
    surrounding: PaletteColor,
    label: &str,
    arrows: bool,
) {
    if arrows {
        line.push(
            ARROW,
            Some(surrounding),
            Some(declaration.background),
            false,
        );
    }
    let label = format!(" {}", label);
    line.styled(declaration, &with_emphasis(&label, " "), true);
    if arrows {
        line.push(
            ARROW,
            Some(declaration.background),
            Some(surrounding),
            false,
        );
    }
}

const FRAMES_PER_ROW: usize = 3;

fn frame_lines(styling: &Styling, width: usize, stacked: bool) -> Vec<Line> {
    let frames = [
        ("Pane", styling.frame_unselected),
        ("Focused", Some(styling.frame_selected)),
        ("Marked", Some(styling.frame_highlight)),
    ];
    let inner = 13;
    let per_row = if stacked { 1 } else { FRAMES_PER_ROW };
    let mut lines: Vec<Line> = vec![];
    for (index, (title, declaration)) in frames.into_iter().enumerate() {
        if index % per_row == 0 {
            lines.extend([Line::new(width), Line::new(width), Line::new(width)]);
        } else {
            let start = lines.len() - 3;
            for line in lines[start..].iter_mut() {
                line.push(" ", None, None, false);
            }
        }
        let start = lines.len() - 3;
        let block = &mut lines[start..];
        let colour = declaration.map(|declaration| declaration.base);
        let title_part = format!(" {} ", title);
        let rule = inner - title_part.chars().count().min(inner);
        block[0].push(
            &format!("┌{}{}┐", title_part, "─".repeat(rule)),
            colour,
            None,
            false,
        );
        block[1].push(&format!("│{}│", " ".repeat(inner)), colour, None, false);
        block[2].push("└", colour, None, false);
        match declaration {
            Some(declaration) => {
                block[2].styled(&declaration, &with_emphasis("", " "), false);
            },
            None => {
                block[2].push(" e0 e1 e2 e3 ", None, None, false);
            },
        }
        block[2].push("┘", colour, None, false);
    }
    lines
}

struct Group {
    label: &'static str,
    styles: &'static [&'static str],
    lines: Vec<Line>,
}

fn styled_line(
    width: usize,
    declaration: &StyleDeclaration,
    before: &str,
    after: &str,
    background: bool,
) -> Line {
    let mut line = Line::new(width);
    line.styled(declaration, &with_emphasis(before, after), background);
    line
}

const RIBBONS: usize = 2;
const FRAMES: usize = 5;
const PLAYERS: usize = 7;
const GROUP_COUNT: usize = 8;
const STACKABLE: [usize; 3] = [RIBBONS, FRAMES, PLAYERS];

fn sample_groups(
    styling: &Styling,
    width: usize,
    arrows: bool,
    stacked: &[bool; GROUP_COUNT],
) -> Vec<Group> {
    let mut groups = vec![];
    groups.push(Group {
        label: "Text",
        styles: &["text_unselected"],
        lines: vec![styled_line(
            width,
            &styling.text_unselected,
            "Text",
            " ",
            true,
        )],
    });
    groups.push(Group {
        label: "Selected",
        styles: &["text_selected"],
        lines: vec![styled_line(
            width,
            &styling.text_selected,
            "Selected text",
            " ",
            true,
        )],
    });
    let surrounding = styling.text_unselected.background;
    let mut ribbon_lines = vec![Line::new(width)];
    ribbon(
        &mut ribbon_lines[0],
        &styling.ribbon_unselected,
        surrounding,
        "Tab",
        arrows,
    );
    if stacked[RIBBONS] {
        ribbon_lines.push(Line::new(width));
    } else {
        ribbon_lines[0].push(" ", None, Some(surrounding), false);
    }
    ribbon(
        ribbon_lines.last_mut().unwrap(),
        &styling.ribbon_selected,
        surrounding,
        "Active",
        arrows,
    );
    groups.push(Group {
        label: "Ribbons",
        styles: &["ribbon_unselected", "ribbon_selected"],
        lines: ribbon_lines,
    });
    groups.push(Group {
        label: "Table",
        styles: &[
            "table_title",
            "table_cell_unselected",
            "table_cell_selected",
        ],
        lines: vec![
            styled_line(width, &styling.table_title, "NAME     ", " ", false),
            styled_line(
                width,
                &styling.table_cell_unselected,
                "notes.txt",
                " ",
                true,
            ),
            styled_line(width, &styling.table_cell_selected, "todo.md  ", " ", true),
        ],
    });
    groups.push(Group {
        label: "List",
        styles: &["list_unselected", "list_selected"],
        lines: vec![
            styled_line(width, &styling.list_unselected, "• First ", " ", true),
            styled_line(width, &styling.list_selected, "• Second", " ", true),
        ],
    });
    groups.push(Group {
        label: "Frames",
        styles: &["frame_unselected", "frame_selected", "frame_highlight"],
        lines: frame_lines(styling, width, stacked[FRAMES]),
    });
    groups.push(Group {
        label: "Exit codes",
        styles: &["exit_code_success", "exit_code_error"],
        lines: vec![
            styled_line(
                width,
                &styling.exit_code_success,
                "EXIT CODE: 0",
                " ",
                false,
            ),
            styled_line(width, &styling.exit_code_error, "EXIT CODE: 1", " ", false),
        ],
    });
    let players = styling.multiplayer_user_colors;
    let colours = [
        players.player_1,
        players.player_2,
        players.player_3,
        players.player_4,
        players.player_5,
        players.player_6,
        players.player_7,
        players.player_8,
        players.player_9,
        players.player_10,
    ];
    let per_line = if stacked[PLAYERS] {
        colours.len() / 2
    } else {
        colours.len()
    };
    let mut users = vec![];
    for chunk in colours.chunks(per_line) {
        if !users.is_empty() {
            users.push(Line::new(width));
        }
        let mut line = Line::new(width);
        for (index, colour) in chunk.iter().enumerate() {
            if index > 0 {
                line.push(" ", None, None, false);
            }
            line.push("██", Some(*colour), None, false);
        }
        users.push(line);
    }
    groups.push(Group {
        label: "Players",
        styles: &["multiplayer_user_colors"],
        lines: users,
    });
    groups
}

const COLUMN_GAP: usize = 3;
const HEADER_ROWS: usize = 2;
const NATURAL_WIDTH: usize = 1000;
const MAX_COLUMNS: usize = 3;
const TIERS: [(usize, usize); 6] = [(1, 1), (2, 1), (1, 0), (2, 0), (3, 1), (3, 0)];

#[derive(Debug, Clone, Copy)]
struct Size {
    width: usize,
    height: usize,
}

#[derive(Debug, Clone)]
struct Dims {
    label_width: usize,
    sizes: [Size; 2],
}

fn group_dims(styling: &Styling, arrows: bool) -> Vec<Dims> {
    let size_of = |group: &Group| Size {
        width: group.lines.iter().map(|line| line.width).max().unwrap_or(0),
        height: group.lines.len(),
    };
    let side_by_side = sample_groups(styling, NATURAL_WIDTH, arrows, &[false; GROUP_COUNT]);
    let stacked = sample_groups(styling, NATURAL_WIDTH, arrows, &[true; GROUP_COUNT]);
    side_by_side
        .iter()
        .zip(stacked.iter())
        .map(|(wide, tall)| Dims {
            label_width: wide.label.chars().count() + 2,
            sizes: [size_of(wide), size_of(tall)],
        })
        .collect()
}

#[derive(Debug, Clone)]
struct Column {
    groups: Vec<usize>,
    label_width: usize,
    line_width: usize,
}

#[derive(Debug, Clone)]
struct Layout {
    columns: Vec<Column>,
    gap: usize,
    stacked: [bool; GROUP_COUNT],
}

impl Layout {
    fn total_width(&self) -> usize {
        self.columns
            .iter()
            .map(|column| column.label_width + column.line_width)
            .sum::<usize>()
            + COLUMN_GAP * self.columns.len().saturating_sub(1)
    }
    fn column_height(&self, dims: &[Dims], column: &Column) -> usize {
        column
            .groups
            .iter()
            .map(|index| dims[*index].sizes[self.stacked[*index] as usize].height)
            .sum::<usize>()
            + self.gap * column.groups.len().saturating_sub(1)
    }
    fn tallest_column(&self, dims: &[Dims]) -> usize {
        self.columns
            .iter()
            .map(|column| self.column_height(dims, column))
            .max()
            .unwrap_or(0)
    }
}

fn stacking_choices() -> Vec<[bool; GROUP_COUNT]> {
    let mut choices: Vec<[bool; GROUP_COUNT]> = (0..1usize << STACKABLE.len())
        .map(|mask| {
            let mut stacked = [false; GROUP_COUNT];
            for (bit, index) in STACKABLE.iter().enumerate() {
                stacked[*index] = mask & (1 << bit) != 0;
            }
            stacked
        })
        .collect();
    choices.sort_by_key(|stacked| stacked.iter().filter(|s| **s).count());
    choices
}

fn build_layout(
    dims: &[Dims],
    assignment: &[usize],
    column_count: usize,
    gap: usize,
    stacked: [bool; GROUP_COUNT],
) -> Layout {
    let columns = (0..column_count)
        .map(|column_index| {
            let groups: Vec<usize> = assignment
                .iter()
                .enumerate()
                .filter(|(_, column)| **column == column_index)
                .map(|(group, _)| group)
                .collect();
            let label_width = groups
                .iter()
                .map(|index| dims[*index].label_width)
                .max()
                .unwrap_or(0);
            let line_width = groups
                .iter()
                .map(|index| dims[*index].sizes[stacked[*index] as usize].width)
                .max()
                .unwrap_or(0);
            Column {
                groups,
                label_width,
                line_width,
            }
        })
        .collect();
    Layout {
        columns,
        gap,
        stacked,
    }
}

fn search(
    dims: &[Dims],
    assignment: &mut Vec<usize>,
    heights: &mut Vec<usize>,
    column_count: usize,
    gap: usize,
    stacked: [bool; GROUP_COUNT],
    limits: Size,
    best: &mut Option<Layout>,
) {
    let next = assignment.len();
    if next == dims.len() {
        if heights.len() != column_count {
            return;
        }
        let layout = build_layout(dims, assignment, column_count, gap, stacked);
        if layout.total_width() > limits.width {
            return;
        }
        let better = match best {
            None => true,
            Some(current) => {
                (layout.tallest_column(dims), layout.total_width())
                    < (current.tallest_column(dims), current.total_width())
            },
        };
        if better {
            *best = Some(layout);
        }
        return;
    }
    let height = dims[next].sizes[stacked[next] as usize].height;
    let open_columns = (heights.len() + 1).min(column_count);
    for column in 0..open_columns {
        let added = if column < heights.len() {
            heights[column] + gap + height
        } else {
            height
        };
        if added > limits.height {
            continue;
        }
        let is_new = column == heights.len();
        if is_new {
            heights.push(added);
        } else {
            heights[column] = added;
        }
        assignment.push(column);
        search(
            dims,
            assignment,
            heights,
            column_count,
            gap,
            stacked,
            limits,
            best,
        );
        assignment.pop();
        if is_new {
            heights.pop();
        } else {
            heights[column] -= gap + height;
        }
    }
}

fn arrange(dims: &[Dims], width: usize, rows: usize) -> Layout {
    let limits = Size {
        width,
        height: rows,
    };
    for (column_count, gap) in TIERS {
        if column_count > MAX_COLUMNS {
            continue;
        }
        for stacked in stacking_choices() {
            let mut best = None;
            search(
                dims,
                &mut vec![],
                &mut vec![],
                column_count,
                gap,
                stacked,
                limits,
                &mut best,
            );
            if let Some(layout) = best {
                return layout;
            }
        }
    }
    let mut fallback = build_layout(dims, &vec![0; dims.len()], 1, 1, [false; GROUP_COUNT]);
    for column in fallback.columns.iter_mut() {
        column.label_width = column.label_width.max(LABEL_WIDTH);
    }
    fallback
}

pub fn render_sample(
    colours: &[String],
    focused_style: Option<&str>,
    arrows: bool,
    x: usize,
    y: usize,
    width: usize,
    height: usize,
) {
    print_text_with_coordinates(
        Text::from(crate::page::truncate(
            "Preview (e0-e3 show the emphasis colors)",
            width,
        ))
        .color_range(2, ..7)
        .dim_range(7..),
        x,
        y,
        None,
        None,
    );
    let styling = styling_from_colours(colours);
    let rows = height.saturating_sub(HEADER_ROWS);
    let dims = group_dims(&styling, arrows);
    let layout = arrange(&dims, width, rows);
    let column_count = layout.columns.len();
    let mut column_x = x;
    for (column_index, column) in layout.columns.iter().enumerate() {
        let used = column_x.saturating_sub(x);
        let line_width = if column_index + 1 == column_count {
            width.saturating_sub(used + column.label_width)
        } else {
            column.line_width
        };
        let groups = sample_groups(&styling, line_width, arrows, &layout.stacked);
        let mut row = 0;
        for (position, group_index) in column.groups.iter().enumerate() {
            let group = &groups[*group_index];
            if position > 0 {
                row += layout.gap;
            }
            let focused = focused_style
                .map(|style| group.styles.contains(&style))
                .unwrap_or(false);
            for (line_index, line) in group.lines.iter().enumerate() {
                if row >= rows {
                    break;
                }
                let screen_row = y + HEADER_ROWS + row;
                if line_index == 0 {
                    let label =
                        crate::page::truncate(group.label, column.label_width.saturating_sub(1));
                    let text = if focused {
                        Text::from(label).color_all(3)
                    } else {
                        Text::from(label).dim_all()
                    };
                    print_text_with_coordinates(text, column_x, screen_row, None, None);
                }
                line.print(column_x + column.label_width, screen_row);
                row += 1;
            }
        }
        column_x += column.label_width + column.line_width + COLUMN_GAP;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dims() -> Vec<Dims> {
        let colours = vec![String::new(); 128];
        group_dims(&styling_from_colours(&colours), true)
    }

    fn assert_fits(layout: &Layout, dims: &[Dims], width: usize, rows: usize) {
        assert!(layout.total_width() <= width);
        assert!(layout.tallest_column(dims) <= rows);
        let mut shown: Vec<usize> = layout
            .columns
            .iter()
            .flat_map(|column| column.groups.clone())
            .collect();
        shown.sort();
        assert_eq!(shown, (0..GROUP_COUNT).collect::<Vec<_>>());
        for column in &layout.columns {
            assert!(column.groups.windows(2).all(|pair| pair[0] < pair[1]));
        }
    }

    fn single_column_height(dims: &[Dims]) -> usize {
        dims.iter().map(|d| d.sizes[0].height).sum::<usize>() + GROUP_COUNT - 1
    }

    #[test]
    fn everything_fits_in_one_column_when_there_is_room() {
        let dims = dims();
        let rows = single_column_height(&dims);
        let layout = arrange(&dims, 200, rows);
        assert_eq!(layout.columns.len(), 1);
        assert_eq!(layout.gap, 1);
        assert!(!layout.stacked.iter().any(|s| *s));
        assert_fits(&layout, &dims, 200, rows);
    }

    #[test]
    fn a_short_area_gets_two_columns_with_the_gaps_kept() {
        let dims = dims();
        let rows = single_column_height(&dims) - 8;
        let layout = arrange(&dims, 200, rows);
        assert_eq!(layout.columns.len(), 2);
        assert_eq!(layout.gap, 1);
        assert!(!layout.stacked.iter().any(|s| *s));
        assert_fits(&layout, &dims, 200, rows);
    }

    #[test]
    fn a_narrow_short_area_stacks_wide_samples_to_stay_in_two_columns() {
        let dims = dims();
        let layout = arrange(&dims, 75, 16);
        assert_eq!(layout.columns.len(), 2);
        assert_eq!(layout.gap, 1);
        assert!(layout.stacked.iter().any(|s| *s));
        assert_fits(&layout, &dims, 75, 16);
    }

    #[test]
    fn an_area_too_small_for_any_arrangement_falls_back_to_one_column() {
        let dims = dims();
        let layout = arrange(&dims, 30, 5);
        assert_eq!(layout.columns.len(), 1);
        assert_eq!(layout.gap, 1);
    }
}
