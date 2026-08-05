//! Treats a pane's canvas as a text file and describes each change to it as a
//! unified (git-style) diff.
//!
//! A pane viewport arrives from the server as `Vec<String>` — one entry per
//! screen row. We keep the previous version of that "file" per pane and, on
//! every update, emit the diff between the two versions:
//!
//! ```text
//! @@ -3,1 +3,1 @@
//! -sudr
//! +sudo
//! ```

use serde::{Deserialize, Serialize};
use similar::{ChangeTag, TextDiff};

/// Number of unchanged context lines kept around each hunk, matching git's default.
pub const CONTEXT_RADIUS: usize = 3;

/// A single hunk of a canvas diff, in the shape a git user expects.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DiffHunk {
    /// 1-based first line of the hunk in the old version.
    pub old_start: usize,
    pub old_lines: usize,
    /// 1-based first line of the hunk in the new version.
    pub new_start: usize,
    pub new_lines: usize,
    /// The hunk body: each entry prefixed with ' ', '-' or '+'.
    pub lines: Vec<String>,
}

/// The change between two consecutive versions of one pane's canvas.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CanvasDiff {
    /// Version this diff moves the canvas *from*.
    pub from_version: u64,
    /// Version this diff moves the canvas *to*.
    pub to_version: u64,
    pub added: usize,
    pub removed: usize,
    pub hunks: Vec<DiffHunk>,
    /// The whole diff rendered as unified-diff text, ready to print.
    pub unified: String,
}

impl CanvasDiff {
    pub fn is_empty(&self) -> bool {
        self.hunks.is_empty()
    }
}

/// Normalise a screen row before diffing.
///
/// The terminal pads rows out to the pane width, and the cursor moving around
/// rewrites rows without changing anything visible. Trimming the trailing
/// padding keeps those non-events out of the history.
fn normalize(line: &str) -> String {
    line.trim_end().to_string()
}

pub fn normalize_canvas(lines: &[String]) -> Vec<String> {
    lines.iter().map(|l| normalize(l)).collect()
}

/// Diff two canvas versions. Returns `None` when nothing visible changed.
pub fn diff_canvas(
    old: &[String],
    new: &[String],
    from_version: u64,
    to_version: u64,
    label: &str,
) -> Option<CanvasDiff> {
    if old == new {
        return None;
    }

    // `TextDiff::from_slices` compares the rows as opaque units, which is what
    // we want: a screen row is the unit of change, like a line in a file.
    let old_rows: Vec<&str> = old.iter().map(|s| s.as_str()).collect();
    let new_rows: Vec<&str> = new.iter().map(|s| s.as_str()).collect();
    let diff = TextDiff::from_slices(&old_rows, &new_rows);

    let mut hunks: Vec<DiffHunk> = Vec::new();
    let mut added = 0usize;
    let mut removed = 0usize;

    for group in diff.grouped_ops(CONTEXT_RADIUS).iter() {
        let mut lines: Vec<String> = Vec::new();
        let mut old_start = usize::MAX;
        let mut new_start = usize::MAX;
        let mut old_lines = 0usize;
        let mut new_lines = 0usize;

        for op in group {
            for change in diff.iter_changes(op) {
                let idx_old = change.old_index();
                let idx_new = change.new_index();
                if let Some(i) = idx_old {
                    old_start = old_start.min(i);
                }
                if let Some(i) = idx_new {
                    new_start = new_start.min(i);
                }
                let value = change.value();
                match change.tag() {
                    ChangeTag::Equal => {
                        old_lines += 1;
                        new_lines += 1;
                        lines.push(format!(" {}", value));
                    },
                    ChangeTag::Delete => {
                        old_lines += 1;
                        removed += 1;
                        lines.push(format!("-{}", value));
                    },
                    ChangeTag::Insert => {
                        new_lines += 1;
                        added += 1;
                        lines.push(format!("+{}", value));
                    },
                }
            }
        }

        if lines.is_empty() {
            continue;
        }

        hunks.push(DiffHunk {
            // Unified diff line numbers are 1-based. An empty side is
            // conventionally reported as starting at 0.
            old_start: if old_lines == 0 {
                0
            } else {
                old_start.saturating_add(1)
            },
            old_lines,
            new_start: if new_lines == 0 {
                0
            } else {
                new_start.saturating_add(1)
            },
            new_lines,
            lines,
        });
    }

    if hunks.is_empty() {
        return None;
    }

    let unified = render_unified(&hunks, label, from_version, to_version);

    Some(CanvasDiff {
        from_version,
        to_version,
        added,
        removed,
        hunks,
        unified,
    })
}

fn render_unified(hunks: &[DiffHunk], label: &str, from_version: u64, to_version: u64) -> String {
    let mut out = String::new();
    out.push_str(&format!("--- {}@{}\n", label, from_version));
    out.push_str(&format!("+++ {}@{}\n", label, to_version));
    for hunk in hunks {
        out.push_str(&format!(
            "@@ -{},{} +{},{} @@\n",
            hunk.old_start, hunk.old_lines, hunk.new_start, hunk.new_lines
        ));
        for line in &hunk.lines {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn canvas(lines: &[&str]) -> Vec<String> {
        lines.iter().map(|l| l.to_string()).collect()
    }

    #[test]
    fn identical_canvases_produce_no_diff() {
        let a = canvas(&["$ ls", "foo bar", ""]);
        assert!(diff_canvas(&a, &a, 0, 1, "pane/terminal_1").is_none());
    }

    #[test]
    fn single_line_edit_reads_like_git() {
        // The motivating example from the goal: a typo corrected in place.
        let old = canvas(&["$ ", "line two", "sudr", "line four"]);
        let new = canvas(&["$ ", "line two", "sudo", "line four"]);
        let diff = diff_canvas(&old, &new, 4, 5, "pane/terminal_1").expect("expected a diff");

        assert_eq!(diff.added, 1);
        assert_eq!(diff.removed, 1);
        assert!(
            diff.unified.contains("-sudr\n+sudo\n"),
            "unified diff should show the replacement, got:\n{}",
            diff.unified
        );
        assert!(diff.unified.contains("--- pane/terminal_1@4"));
        assert!(diff.unified.contains("+++ pane/terminal_1@5"));
    }

    #[test]
    fn hunk_line_numbers_are_one_based() {
        let old = canvas(&["a", "b", "c"]);
        let new = canvas(&["a", "B", "c"]);
        let diff = diff_canvas(&old, &new, 0, 1, "p").unwrap();
        assert_eq!(diff.hunks.len(), 1);
        let hunk = &diff.hunks[0];
        // The hunk starts at line 1 because of the context line above it.
        assert_eq!(hunk.old_start, 1);
        assert_eq!(hunk.new_start, 1);
        assert_eq!(hunk.old_lines, 3);
        assert_eq!(hunk.new_lines, 3);
        assert!(diff.unified.contains("@@ -1,3 +1,3 @@"));
    }

    #[test]
    fn appended_output_is_an_addition_only_hunk() {
        let old = canvas(&["$ ls"]);
        let new = canvas(&["$ ls", "Cargo.toml", "src"]);
        let diff = diff_canvas(&old, &new, 0, 1, "p").unwrap();
        assert_eq!(diff.added, 2);
        assert_eq!(diff.removed, 0);
        assert!(diff.unified.contains("+Cargo.toml"));
        assert!(diff.unified.contains("+src"));
    }

    #[test]
    fn scrolling_shows_as_removal_at_top_and_addition_at_bottom() {
        let old = canvas(&["one", "two", "three"]);
        let new = canvas(&["two", "three", "four"]);
        let diff = diff_canvas(&old, &new, 1, 2, "p").unwrap();
        assert_eq!(diff.removed, 1);
        assert_eq!(diff.added, 1);
        assert!(diff.unified.contains("-one"));
        assert!(diff.unified.contains("+four"));
    }

    #[test]
    fn trailing_padding_is_not_a_change() {
        // The renderer pads rows to the pane width; that must not register.
        let old = normalize_canvas(&canvas(&["$ echo hi", "hi"]));
        let new = normalize_canvas(&canvas(&["$ echo hi    ", "hi        "]));
        assert_eq!(old, new);
        assert!(diff_canvas(&old, &new, 0, 1, "p").is_none());
    }

    #[test]
    fn distant_changes_produce_separate_hunks() {
        let mut old_lines: Vec<String> = (0..40).map(|i| format!("line {}", i)).collect();
        let mut new_lines = old_lines.clone();
        new_lines[2] = "changed near top".to_string();
        new_lines[35] = "changed near bottom".to_string();
        old_lines.truncate(40);
        new_lines.truncate(40);

        let diff = diff_canvas(&old_lines, &new_lines, 7, 8, "p").unwrap();
        assert_eq!(
            diff.hunks.len(),
            2,
            "changes 30 lines apart should not be merged into one hunk"
        );
        assert_eq!(diff.added, 2);
        assert_eq!(diff.removed, 2);
    }

    #[test]
    fn clearing_the_screen_removes_every_line() {
        let old = canvas(&["a", "b", "c"]);
        let new: Vec<String> = vec![];
        let diff = diff_canvas(&old, &new, 3, 4, "p").unwrap();
        assert_eq!(diff.removed, 3);
        assert_eq!(diff.added, 0);
        assert_eq!(diff.hunks[0].new_start, 0, "empty side starts at 0");
    }
}
