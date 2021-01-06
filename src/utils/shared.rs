use std::{iter, str::from_utf8};

use strip_ansi_escapes::strip;

// FIXME: Should this be an extension trait? Or here at all?
pub fn ansi_len(s: &str) -> usize {
    from_utf8(&strip(s.as_bytes()).unwrap()).unwrap().chars().count()
}

pub fn pad_to_size(s: &str, rows: usize, columns: usize) -> String {
    s.lines()
        .map(|l| [l, &str::repeat(" ", columns - ansi_len(l))].concat())
        .chain(iter::repeat(str::repeat(" ", columns)))
        .take(rows)
        .collect::<Vec<_>>()
        .join("\n\r")
}
