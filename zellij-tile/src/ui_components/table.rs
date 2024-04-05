use super::Text;

/// render a table with arbitrary data
#[derive(Debug, Clone)]
pub struct Table {
    contents: Vec<Vec<Text>>,
}

impl Table {
    pub fn new() -> Self {
        Table { contents: vec![] }
    }
    pub fn add_row(mut self, row: Vec<impl ToString>) -> Self {
        self.contents
            .push(row.iter().map(|c| Text::new(c.to_string())).collect());
        self
    }
    pub fn add_styled_row(mut self, row: Vec<Text>) -> Self {
        self.contents.push(row);
        self
    }
    pub fn serialize(&self) -> String {
        let columns = self
            .contents
            .get(0)
            .map(|first_row| first_row.len())
            .unwrap_or(0);
        let rows = self.contents.len();
        let contents = self
            .contents
            .iter()
            .flatten()
            .map(|t| t.serialize())
            .collect::<Vec<_>>()
            .join(";");
        format!("{};{};{}\u{1b}\\", columns, rows, contents)
    }
}

pub fn print_table(table: Table) {
    print!("\u{1b}Pztable;{}", table.serialize())
}

pub fn print_table_with_coordinates(
    table: Table,
    x: usize,
    y: usize,
    width: Option<usize>,
    height: Option<usize>,
) {
    let width = width.map(|w| w.to_string()).unwrap_or_default();
    let height = height.map(|h| h.to_string()).unwrap_or_default();
    print!(
        "\u{1b}Pztable;{}/{}/{}/{};{}\u{1b}\\",
        x,
        y,
        width,
        height,
        table.serialize()
    )
}

pub fn serialize_table(table: &Table) -> String {
    format!("\u{1b}Pztable;{}", table.serialize())
}

pub fn serialize_table_with_coordinates(
    table: &Table,
    x: usize,
    y: usize,
    width: Option<usize>,
    height: Option<usize>,
) -> String {
    let width = width.map(|w| w.to_string()).unwrap_or_default();
    let height = height.map(|h| h.to_string()).unwrap_or_default();
    format!(
        "\u{1b}Pztable;{}/{}/{}/{};{}\u{1b}\\",
        x,
        y,
        width,
        height,
        table.serialize()
    )
}
