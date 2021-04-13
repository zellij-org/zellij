//! Some general utility functions.

use std::{iter, str::from_utf8};

use strip_ansi_escapes::strip;

fn ansi_len(s: &str) -> usize {
    from_utf8(&strip(s.as_bytes()).unwrap())
        .unwrap()
        .chars()
        .count()
}

pub fn adjust_to_size(s: &str, rows: usize, columns: usize) -> String {
    s.lines()
        .map(|l| {
            let actual_len = ansi_len(l);
            if actual_len > columns {
                let mut line = String::from(l);
                line.truncate(columns);
                return line;
            } else {
                return [l, &str::repeat(" ", columns - ansi_len(l))].concat();
            }
        })
        .chain(iter::repeat(str::repeat(" ", columns)))
        .take(rows)
        .collect::<Vec<_>>()
        .join("\n\r")
}
