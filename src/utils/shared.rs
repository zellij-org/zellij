use std::iter;

use strip_ansi_escapes::strip;

// FIXME: Should this be an extension trait? Or here at all?
pub fn ansi_len(s: &str) -> usize {
    strip(s.as_bytes()).unwrap().len()
}

pub fn pad_to_size(s: &str, rows: usize, columns: usize) -> String {
    s.lines()
        .map(|l| [l, &str::repeat(" ", dbg!(columns) - dbg!(ansi_len(l)))].concat())
        .chain(iter::repeat(str::repeat(" ", columns)))
        .take(rows)
        .collect::<Vec<_>>()
        .join("\n\r")
}
