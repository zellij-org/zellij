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

fn frame_lines(styling: &Styling, width: usize) -> [Line; 3] {
    let frames = [
        ("Pane", styling.frame_unselected),
        ("Focused", Some(styling.frame_selected)),
        ("Marked", Some(styling.frame_highlight)),
    ];
    let inner = 13;
    let mut lines = [Line::new(width), Line::new(width), Line::new(width)];
    for (title, declaration) in frames {
        let colour = declaration.map(|declaration| declaration.base);
        let title_part = format!(" {} ", title);
        let rule = inner - title_part.chars().count().min(inner);
        lines[0].push(
            &format!("┌{}{}┐", title_part, "─".repeat(rule)),
            colour,
            None,
            false,
        );
        lines[1].push(&format!("│{}│", " ".repeat(inner)), colour, None, false);
        lines[2].push("└", colour, None, false);
        match declaration {
            Some(declaration) => {
                lines[2].styled(&declaration, &with_emphasis("", " "), false);
            },
            None => {
                lines[2].push(" e0 e1 e2 e3 ", None, None, false);
            },
        }
        lines[2].push("┘", colour, None, false);
        for line in lines.iter_mut() {
            line.push(" ", None, None, false);
        }
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

fn sample_groups(styling: &Styling, width: usize, arrows: bool) -> Vec<Group> {
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
    let mut ribbons = Line::new(width);
    ribbon(
        &mut ribbons,
        &styling.ribbon_unselected,
        surrounding,
        "Tab",
        arrows,
    );
    ribbons.push(" ", None, Some(surrounding), false);
    ribbon(
        &mut ribbons,
        &styling.ribbon_selected,
        surrounding,
        "Active",
        arrows,
    );
    groups.push(Group {
        label: "Ribbons",
        styles: &["ribbon_unselected", "ribbon_selected"],
        lines: vec![ribbons],
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
    let [top, middle, bottom] = frame_lines(styling, width);
    groups.push(Group {
        label: "Frames",
        styles: &["frame_unselected", "frame_selected", "frame_highlight"],
        lines: vec![top, middle, bottom],
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
    let mut users = Line::new(width);
    for colour in [
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
    ] {
        users.push("██", Some(colour), None, false);
        users.push(" ", None, None, false);
    }
    groups.push(Group {
        label: "Players",
        styles: &["multiplayer_user_colors"],
        lines: vec![users],
    });
    groups
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
    let sample_width = width.saturating_sub(LABEL_WIDTH);
    let mut row = 2;
    for (index, group) in sample_groups(&styling, sample_width, arrows)
        .iter()
        .enumerate()
    {
        if index > 0 {
            row += 1;
        }
        let focused = focused_style
            .map(|style| group.styles.contains(&style))
            .unwrap_or(false);
        for (line_index, line) in group.lines.iter().enumerate() {
            if row >= height {
                return;
            }
            if line_index == 0 {
                let label = crate::page::truncate(group.label, LABEL_WIDTH.saturating_sub(1));
                let text = if focused {
                    Text::from(label).color_all(3)
                } else {
                    Text::from(label).dim_all()
                };
                print_text_with_coordinates(text, x, y + row, None, None);
            }
            line.print(x + LABEL_WIDTH, y + row);
            row += 1;
        }
    }
}
