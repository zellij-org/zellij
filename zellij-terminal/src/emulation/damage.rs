#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Damage {
    Everything,
    Rows(Vec<usize>),
}

impl Damage {
    #[cfg(test)]
    pub fn of(rows: impl IntoIterator<Item = usize>) -> Self {
        Damage::Rows(rows.into_iter().collect())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub line: usize,
    pub left: usize,
    pub right: usize,
}

impl Span {
    pub fn new(line: usize, left: usize, right: usize) -> Self {
        Self { line, left, right }
    }
}
