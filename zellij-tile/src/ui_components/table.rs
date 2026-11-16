use super::Text;

/// render a table with arbitrary data
#[derive(Debug, Default, Clone)]
pub struct Table {
    contents: Vec<Vec<Text>>,
}

impl Table {
    pub fn new() -> Self {
        Table { contents: vec![] }
    }
    pub fn add_row(mut self, row: Vec<impl ToString>) -> Self {
        self.contents
            .push(row.iter().map(|c| Text::from(c.to_string())).collect());
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
        "\u{1b}Pztable;{}/{}/{}/{};{}",
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
        "\u{1b}Pztable;{}/{}/{}/{};{}",
        x,
        y,
        width,
        height,
        table.serialize()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_table() -> Table {
        Table::new().add_row(vec!["a", "b"]).add_row(vec!["c", "d"])
    }

    fn end_marker_count(serialized: &str) -> usize {
        serialized.matches("\u{1b}\\").count()
    }

    #[test]
    fn serialized_table_has_a_single_end_marker() {
        let serialized = serialize_table(&sample_table());
        assert_eq!(end_marker_count(&serialized), 1);
        assert!(serialized.starts_with("\u{1b}Pztable;2;2;"));
        assert!(serialized.ends_with("\u{1b}\\"));
    }

    #[test]
    fn serialized_table_with_coordinates_has_a_single_end_marker() {
        let serialized = serialize_table_with_coordinates(&sample_table(), 1, 2, Some(10), None);
        assert_eq!(end_marker_count(&serialized), 1);
        assert_eq!(
            serialized,
            format!("\u{1b}Pztable;1/2/10/;{}", sample_table().serialize())
        );
        assert!(serialized.ends_with("\u{1b}\\"));
    }
}
