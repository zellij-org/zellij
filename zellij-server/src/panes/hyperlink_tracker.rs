use crate::panes::grid::Row;
use crate::panes::link_handler::LinkHandler;
use crate::panes::terminal_character::{LinkAnchor, RcCharacterStyles};
use std::collections::VecDeque;

const URL_SCHEMES: [&str; 4] = ["http://", "https://", "ftp://", "mailto:"];

#[derive(Clone, Copy)]
struct Cell {
    character: char,
    y: isize,
    index: usize,
}

fn row_ref<'a>(
    lines_above: &'a VecDeque<Row>,
    viewport: &'a VecDeque<Row>,
    y: isize,
) -> Option<&'a Row> {
    if y < 0 {
        let index = lines_above.len() as isize + y;
        return usize::try_from(index)
            .ok()
            .and_then(|index| lines_above.get(index));
    }
    viewport.get(y as usize)
}

fn row_mut<'a>(
    lines_above: &'a mut VecDeque<Row>,
    viewport: &'a mut VecDeque<Row>,
    y: isize,
) -> Option<&'a mut Row> {
    if y < 0 {
        let index = lines_above.len() as isize + y;
        return usize::try_from(index)
            .ok()
            .and_then(|index| lines_above.get_mut(index));
    }
    viewport.get_mut(y as usize)
}

fn line_start(lines_above: &VecDeque<Row>, viewport: &VecDeque<Row>, y: isize) -> isize {
    let mut start = y;
    let top = -(lines_above.len() as isize);
    while start > top {
        match row_ref(lines_above, viewport, start) {
            Some(row) if !row.is_canonical => start -= 1,
            _ => break,
        }
    }
    start
}

fn line_rows(lines_above: &VecDeque<Row>, viewport: &VecDeque<Row>, start: isize) -> isize {
    let mut end = start + 1;
    while let Some(row) = row_ref(lines_above, viewport, end) {
        if row.is_canonical {
            break;
        }
        end += 1;
    }
    end
}

pub fn is_url_terminator(character: char) -> bool {
    matches!(
        character,
        ' ' | '\n' | '\r' | '\t' | '"' | '\'' | '<' | '>' | '(' | ')' | '[' | ']' | '{' | '}' | '⏎'
    )
}

pub fn trim_trailing_punctuation(url: &str) -> &str {
    url.trim_end_matches(['.', ',', ';', '!', '?', '\n', '\r', '⏎'])
}

pub fn is_valid_url(url: &str) -> bool {
    if url.len() < 8 {
        return false;
    }
    if let Some(after_protocol) = url
        .strip_prefix("http://")
        .or_else(|| url.strip_prefix("https://"))
    {
        return !after_protocol.is_empty() && after_protocol.contains('.');
    }
    if let Some(after_protocol) = url.strip_prefix("ftp://") {
        return !after_protocol.is_empty();
    }
    if let Some(after_colon) = url.strip_prefix("mailto:") {
        return after_colon.contains('@');
    }
    false
}

fn scheme_at(cells: &[Cell], at: usize) -> bool {
    URL_SCHEMES.iter().any(|scheme| {
        scheme.len() <= cells.len() - at
            && scheme
                .chars()
                .zip(&cells[at..])
                .all(|(expected, cell)| cell.character == expected)
    })
}

pub fn link_urls_in_lines(
    lines_above: &mut VecDeque<Row>,
    viewport: &mut VecDeque<Row>,
    lines: &[isize],
    cursor: Option<(usize, usize)>,
    link_handler: &mut LinkHandler,
) {
    let mut starts: Vec<isize> = lines
        .iter()
        .filter(|y| **y < viewport.len() as isize && row_ref(lines_above, viewport, **y).is_some())
        .map(|y| line_start(lines_above, viewport, *y))
        .collect();
    starts.sort_unstable();
    starts.dedup();
    let mut cells: Vec<Cell> = Vec::new();
    for start in starts {
        let end = line_rows(lines_above, viewport, start);
        for y in start..end {
            if let Some(row) = row_mut(lines_above, viewport, y) {
                row.link_scan_pending = false;
            }
        }
        let has_colon = (start..end).any(|y| {
            row_ref(lines_above, viewport, y)
                .map(|row| row.columns.iter().any(|cell| cell.character == ':'))
                .unwrap_or(false)
        });
        if !has_colon {
            continue;
        }
        cells.clear();
        for y in start..end {
            if let Some(row) = row_ref(lines_above, viewport, y) {
                cells.extend(row.columns.iter().enumerate().map(|(index, cell)| Cell {
                    character: cell.character,
                    y,
                    index,
                }));
            }
        }
        let cursor_cell = cursor
            .filter(|(_, cursor_y)| (start..end).contains(&(*cursor_y as isize)))
            .map(|(cursor_x, cursor_y)| {
                cells
                    .iter()
                    .position(|cell| cell.y == cursor_y as isize && cell.index >= cursor_x)
                    .unwrap_or(cells.len())
            });
        let deferred = link_urls_in_line(lines_above, viewport, &cells, cursor_cell, link_handler);
        if deferred {
            if let Some(row) = row_mut(lines_above, viewport, end - 1) {
                row.link_scan_pending = true;
            }
        }
    }
}

fn link_urls_in_line(
    lines_above: &mut VecDeque<Row>,
    viewport: &mut VecDeque<Row>,
    cells: &[Cell],
    cursor_cell: Option<usize>,
    link_handler: &mut LinkHandler,
) -> bool {
    let mut deferred = false;
    let mut at = 0;
    while at < cells.len() {
        if !matches!(cells[at].character, 'h' | 'f' | 'm') || !scheme_at(cells, at) {
            at += 1;
            continue;
        }
        let end = cells[at..]
            .iter()
            .position(|cell| is_url_terminator(cell.character))
            .map_or(cells.len(), |offset| at + offset);
        if cursor_cell.is_some_and(|cursor| (at..=end).contains(&cursor)) {
            deferred = true;
            at = end + 1;
            continue;
        }
        let text: String = cells[at..end].iter().map(|cell| cell.character).collect();
        let url = trim_trailing_punctuation(&text);
        let kept = url.chars().count();
        if is_valid_url(url) {
            let previous = start_anchor_at(lines_above, viewport, cells[at]);
            if !linked_exactly(lines_above, viewport, &cells[at..], kept, previous) {
                let anchor = link_handler.new_link_from_url(url.to_owned());
                mark_link(lines_above, viewport, &cells[at..at + kept], anchor);
                clear_stale_link(lines_above, viewport, &cells[at + kept..], previous);
                mark_end(lines_above, viewport, cells, at + kept);
            }
        }
        at = end + 1;
    }
    deferred
}

fn start_anchor_at(
    lines_above: &VecDeque<Row>,
    viewport: &VecDeque<Row>,
    cell: Cell,
) -> Option<LinkAnchor> {
    row_ref(lines_above, viewport, cell.y)
        .and_then(|row| row.columns.get(cell.index))
        .and_then(|character| character.styles.link_anchor)
        .filter(|anchor| matches!(anchor, LinkAnchor::Start(_)))
}

fn linked_exactly(
    lines_above: &VecDeque<Row>,
    viewport: &VecDeque<Row>,
    cells: &[Cell],
    kept: usize,
    previous: Option<LinkAnchor>,
) -> bool {
    let Some(previous) = previous else {
        return false;
    };
    let anchor_of = |cell: &Cell| {
        row_ref(lines_above, viewport, cell.y)
            .and_then(|row| row.columns.get(cell.index))
            .and_then(|character| character.styles.link_anchor)
    };
    cells[..kept]
        .iter()
        .all(|cell| anchor_of(cell) == Some(previous))
        && cells
            .get(kept)
            .map_or(true, |cell| anchor_of(cell) != Some(previous))
}

fn clear_stale_link(
    lines_above: &mut VecDeque<Row>,
    viewport: &mut VecDeque<Row>,
    cells: &[Cell],
    previous: Option<LinkAnchor>,
) {
    let Some(previous) = previous else {
        return;
    };
    for cell in cells {
        let Some(character) =
            row_mut(lines_above, viewport, cell.y).and_then(|row| row.columns.get_mut(cell.index))
        else {
            return;
        };
        let anchor = character.styles.link_anchor;
        if anchor == Some(previous) || anchor == Some(LinkAnchor::End) {
            character.styles.update(|styles| styles.link_anchor = None);
        }
        if anchor != Some(previous) {
            return;
        }
    }
}

fn mark_link(
    lines_above: &mut VecDeque<Row>,
    viewport: &mut VecDeque<Row>,
    cells: &[Cell],
    anchor: LinkAnchor,
) {
    let mut last_update: Option<(RcCharacterStyles, RcCharacterStyles)> = None;
    for cell in cells {
        let Some(character) =
            row_mut(lines_above, viewport, cell.y).and_then(|row| row.columns.get_mut(cell.index))
        else {
            continue;
        };
        match &last_update {
            Some((before, after)) if character.styles.ptr_eq(before) => {
                character.styles = after.clone();
            },
            _ => {
                let before = character.styles.clone();
                character
                    .styles
                    .update(|styles| styles.link_anchor = Some(anchor));
                last_update = Some((before, character.styles.clone()));
            },
        }
    }
}

fn mark_end(
    lines_above: &mut VecDeque<Row>,
    viewport: &mut VecDeque<Row>,
    cells: &[Cell],
    after: usize,
) {
    let (y, index) = match cells.get(after) {
        Some(cell) => (cell.y, cell.index),
        None => match cells.last() {
            Some(last) => (last.y, last.index + 1),
            None => return,
        },
    };
    let Some(row) = row_mut(lines_above, viewport, y) else {
        return;
    };
    if index >= row.columns.len() {
        row.expand_trailing_blanks();
    }
    if let Some(character) = row.columns.get_mut(index) {
        if character.styles.link_anchor.is_none() {
            character
                .styles
                .update(|styles| styles.link_anchor = Some(LinkAnchor::End));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::panes::terminal_character::{TerminalCharacter, EMPTY_TERMINAL_CHARACTER};

    fn row_with(text: &str, width: usize, canonical: bool) -> Row {
        let mut columns: Vec<TerminalCharacter> =
            text.chars().map(TerminalCharacter::new).collect();
        columns.resize(width.max(columns.len()), TerminalCharacter::new(' '));
        let row = Row::from_columns(columns);
        if canonical {
            row.canonical()
        } else {
            row
        }
    }

    fn screen(lines: &[(&str, bool)], width: usize) -> VecDeque<Row> {
        lines
            .iter()
            .map(|(text, canonical)| row_with(text, width, *canonical))
            .collect()
    }

    fn anchors(row: &Row) -> Vec<Option<LinkAnchor>> {
        row.columns
            .iter()
            .map(|character| character.styles.link_anchor)
            .collect()
    }

    fn is_start(anchor: &Option<LinkAnchor>) -> bool {
        matches!(anchor, Some(LinkAnchor::Start(_)))
    }

    fn link_all(
        lines_above: &mut VecDeque<Row>,
        viewport: &mut VecDeque<Row>,
        cursor: Option<(usize, usize)>,
        link_handler: &mut LinkHandler,
    ) {
        let changed: Vec<isize> = (0..viewport.len() as isize).collect();
        link_urls_in_lines(lines_above, viewport, &changed, cursor, link_handler);
    }

    fn uris(link_handler: &LinkHandler) -> Vec<String> {
        let mut uris: Vec<String> = link_handler
            .links()
            .values()
            .map(|link| link.uri.clone())
            .collect();
        uris.sort();
        uris
    }

    #[test]
    fn every_supported_scheme_is_linked() {
        for url in [
            "http://example.com",
            "https://example.com/path?q=1",
            "ftp://files.example.com",
            "mailto:user@example.com",
        ] {
            let mut viewport = screen(&[(&format!("see {} now", url), true), ("", true)], 60);
            let mut handler = LinkHandler::new();
            link_all(&mut VecDeque::new(), &mut viewport, None, &mut handler);
            assert_eq!(uris(&handler), vec![url.to_owned()], "{}", url);
            let marked = anchors(&viewport[0]);
            assert!(marked[4..4 + url.len()].iter().all(is_start), "{}", url);
            assert_eq!(marked[4 + url.len()], Some(LinkAnchor::End), "{}", url);
            assert!(marked[..4].iter().all(|a| a.is_none()), "{}", url);
        }
    }

    #[test]
    fn invalid_urls_are_not_linked() {
        for text in ["http:// x", "http://nodot y", "mailto:nobody z", "ftp:// q"] {
            let mut viewport = screen(&[(text, true)], 40);
            let mut handler = LinkHandler::new();
            link_all(&mut VecDeque::new(), &mut viewport, None, &mut handler);
            assert!(uris(&handler).is_empty(), "{}", text);
        }
    }

    #[test]
    fn trailing_punctuation_is_left_out_of_the_link() {
        let mut viewport = screen(&[("go to https://example.com/a.", true)], 40);
        let mut handler = LinkHandler::new();
        link_all(&mut VecDeque::new(), &mut viewport, None, &mut handler);
        assert_eq!(uris(&handler), vec!["https://example.com/a".to_owned()]);
        let marked = anchors(&viewport[0]);
        assert!(is_start(&marked[26]));
        assert_eq!(marked[27], Some(LinkAnchor::End));
    }

    #[test]
    fn each_terminator_ends_a_url() {
        for terminator in [
            ' ', '\t', '"', '\'', '<', '>', '(', ')', '[', ']', '{', '}', '⏎',
        ] {
            let text = format!("http://example.com{}rest", terminator);
            let mut viewport = screen(&[(&text, true)], 40);
            let mut handler = LinkHandler::new();
            link_all(&mut VecDeque::new(), &mut viewport, None, &mut handler);
            assert_eq!(
                uris(&handler),
                vec!["http://example.com".to_owned()],
                "{:?}",
                terminator
            );
        }
    }

    #[test]
    fn a_url_wrapped_over_two_rows_is_one_link() {
        let mut viewport = screen(
            &[
                ("https://example.com/", true),
                ("path x", false),
                ("", true),
            ],
            20,
        );
        let mut handler = LinkHandler::new();
        link_all(&mut VecDeque::new(), &mut viewport, None, &mut handler);
        assert_eq!(uris(&handler), vec!["https://example.com/path".to_owned()]);
        assert!(anchors(&viewport[0]).iter().all(is_start));
        let second = anchors(&viewport[1]);
        assert!(second[..4].iter().all(is_start));
        assert_eq!(second[4], Some(LinkAnchor::End));
        assert!(second[5..].iter().all(|a| a.is_none()));
    }

    #[test]
    fn a_url_ending_a_line_does_not_continue_onto_the_next_line() {
        let url = "https://www.fastmail.com";
        let mut viewport = screen(&[(url, true), ("2", true)], 40);
        let mut handler = LinkHandler::new();
        link_all(&mut VecDeque::new(), &mut viewport, None, &mut handler);
        assert_eq!(uris(&handler), vec![url.to_owned()]);
        assert!(anchors(&viewport[1]).iter().all(|a| a.is_none()));
    }

    #[test]
    fn a_url_that_starts_in_scrollback_and_wraps_into_the_screen_is_one_link() {
        let mut lines_above = screen(&[("https://example.", true)], 16);
        let mut viewport = screen(&[("com/rest done", false), ("", true)], 16);
        let mut handler = LinkHandler::new();
        link_urls_in_lines(&mut lines_above, &mut viewport, &[0], None, &mut handler);
        assert_eq!(uris(&handler), vec!["https://example.com/rest".to_owned()]);
        assert!(anchors(&lines_above[0]).iter().all(is_start));
        assert!(anchors(&viewport[0])[..8].iter().all(is_start));
    }

    #[test]
    fn a_url_the_cursor_may_still_extend_is_left_for_later() {
        let mut viewport = screen(&[("https://example.com", true)], 19);
        let mut handler = LinkHandler::new();
        link_all(
            &mut VecDeque::new(),
            &mut viewport,
            Some((19, 0)),
            &mut handler,
        );
        assert!(uris(&handler).is_empty());
        viewport[0] = row_with("https://example.com/more ", 30, true);
        link_all(
            &mut VecDeque::new(),
            &mut viewport,
            Some((25, 0)),
            &mut handler,
        );
        assert_eq!(uris(&handler), vec!["https://example.com/more".to_owned()]);
    }

    #[test]
    fn a_url_the_cursor_sits_right_after_is_left_for_later() {
        let mut viewport = screen(&[("> http://www.e", true)], 40);
        let mut handler = LinkHandler::new();
        link_all(
            &mut VecDeque::new(),
            &mut viewport,
            Some((14, 0)),
            &mut handler,
        );
        assert!(uris(&handler).is_empty());
        assert!(viewport[0].link_scan_pending);
        link_all(
            &mut VecDeque::new(),
            &mut viewport,
            Some((0, 1)),
            &mut handler,
        );
        assert_eq!(uris(&handler), vec!["http://www.e".to_owned()]);
        assert!(!viewport[0].link_scan_pending);
    }

    #[test]
    fn a_linked_url_that_grew_is_relinked_whole() {
        let mut viewport = screen(&[("http://www.e", true)], 40);
        let mut handler = LinkHandler::new();
        link_all(&mut VecDeque::new(), &mut viewport, None, &mut handler);
        let first = anchors(&viewport[0])[0];
        for (index, character) in "xample.com".chars().enumerate() {
            viewport[0].columns[12 + index].character = character;
        }
        viewport[0].columns[12].styles = viewport[0].columns[0].styles.clone();
        link_all(&mut VecDeque::new(), &mut viewport, None, &mut handler);
        let marked = anchors(&viewport[0]);
        assert_ne!(marked[0], first);
        assert!(marked[..22].iter().all(|anchor| *anchor == marked[0]));
        assert_eq!(marked[22], Some(LinkAnchor::End));
        assert!(handler
            .links()
            .values()
            .any(|link| link.uri == "http://www.example.com"));
    }

    #[test]
    fn a_linked_url_that_shrank_drops_its_stale_tail() {
        let mut viewport = screen(&[("http://www.example.com", true)], 40);
        let mut handler = LinkHandler::new();
        link_all(&mut VecDeque::new(), &mut viewport, None, &mut handler);
        viewport[0].columns[15].character = ' ';
        link_all(&mut VecDeque::new(), &mut viewport, None, &mut handler);
        let marked = anchors(&viewport[0]);
        assert!(marked[..15]
            .iter()
            .all(|anchor| *anchor == marked[0] && is_start(anchor)));
        assert_eq!(marked[15], Some(LinkAnchor::End));
        assert!(marked[16..].iter().all(|anchor| anchor.is_none()));
    }

    #[test]
    fn scanning_the_same_line_twice_creates_one_link() {
        let mut viewport = screen(&[("x https://example.com y", true)], 40);
        let mut handler = LinkHandler::new();
        link_all(&mut VecDeque::new(), &mut viewport, None, &mut handler);
        link_all(&mut VecDeque::new(), &mut viewport, None, &mut handler);
        assert_eq!(handler.links().len(), 1);
    }

    #[test]
    fn several_urls_on_one_line_get_their_own_links() {
        let mut viewport = screen(&[("http://first.com and https://second.com", true)], 60);
        let mut handler = LinkHandler::new();
        link_all(&mut VecDeque::new(), &mut viewport, None, &mut handler);
        assert_eq!(
            uris(&handler),
            vec![
                "http://first.com".to_owned(),
                "https://second.com".to_owned()
            ]
        );
        let marked = anchors(&viewport[0]);
        assert_ne!(marked[0], marked[21]);
    }

    #[test]
    fn the_end_marker_does_not_clobber_a_link_that_follows() {
        let url = "https://example.com";
        let mut viewport = screen(&[(&format!("{} ", url), true)], 40);
        let mut handler = LinkHandler::new();
        let neighbour = handler
            .dispatch_osc8(&[b"8", b"", b"https://example.com/neighbour"])
            .expect("an osc 8 anchor");
        viewport[0].columns[url.len()]
            .styles
            .update(|styles| styles.link_anchor = Some(neighbour));
        link_all(&mut VecDeque::new(), &mut viewport, None, &mut handler);
        assert_eq!(anchors(&viewport[0])[url.len()], Some(neighbour));
    }

    #[test]
    fn a_url_before_compressed_scrollback_blanks_gets_its_end_marker() {
        let url = "http://example.com";
        let mut columns: Vec<TerminalCharacter> = url.chars().map(TerminalCharacter::new).collect();
        columns.resize(80, EMPTY_TERMINAL_CHARACTER);
        let mut stored = Row::from_columns(columns).canonical();
        stored.compress_trailing_blanks();
        assert_eq!(stored.columns.len(), url.len());
        let mut lines_above = VecDeque::from(vec![stored]);
        let mut viewport = screen(&[("", true)], 80);
        let mut handler = LinkHandler::new();
        let start = line_start(&lines_above, &viewport, -1);
        let end = line_rows(&lines_above, &viewport, start);
        let cells: Vec<Cell> = (start..end)
            .flat_map(|y| {
                let row = row_ref(&lines_above, &viewport, y).unwrap();
                row.columns
                    .iter()
                    .enumerate()
                    .map(move |(index, c)| Cell {
                        character: c.character,
                        y,
                        index,
                    })
                    .collect::<Vec<_>>()
            })
            .collect();
        link_urls_in_line(&mut lines_above, &mut viewport, &cells, None, &mut handler);
        let expanded = lines_above[0].with_trailing_blanks();
        assert!(expanded.columns[..url.len()]
            .iter()
            .all(|c| is_start(&c.styles.link_anchor)));
        assert_eq!(
            expanded.columns[url.len()].styles.link_anchor,
            Some(LinkAnchor::End)
        );
    }

    #[test]
    fn helper_rules() {
        assert_eq!(trim_trailing_punctuation("http://a.com..."), "http://a.com");
        assert_eq!(trim_trailing_punctuation("http://a.com"), "http://a.com");
        assert!(is_valid_url("https://sub.domain.example.com/path"));
        assert!(!is_valid_url("https://"));
        assert!(!is_valid_url("short"));
    }
}
