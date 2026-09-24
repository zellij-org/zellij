use zellij_server::panes::terminal_character::{
    AnsiCode, AnsiStyledUnderline, CharacterStyles, NamedColor, RESET_STYLES,
};

pub const NAMED_COLORS: [NamedColor; 16] = [
    NamedColor::Black,
    NamedColor::Red,
    NamedColor::Green,
    NamedColor::Yellow,
    NamedColor::Blue,
    NamedColor::Magenta,
    NamedColor::Cyan,
    NamedColor::White,
    NamedColor::BrightBlack,
    NamedColor::BrightRed,
    NamedColor::BrightGreen,
    NamedColor::BrightYellow,
    NamedColor::BrightBlue,
    NamedColor::BrightMagenta,
    NamedColor::BrightCyan,
    NamedColor::BrightWhite,
];

pub fn goto(row: usize, col: usize) -> String {
    format!("\u{1b}[{};{}H\u{1b}[m", row + 1, col + 1)
}

pub fn sgr(styles: &CharacterStyles) -> String {
    format!("{}", styles.enable_styled_underlines(true))
}

fn base() -> CharacterStyles {
    RESET_STYLES.enable_styled_underlines(true)
}

pub fn attribute_styles() -> Vec<(String, CharacterStyles)> {
    let mut styles = vec![
        ("bold".to_owned(), base().bold(Some(AnsiCode::On))),
        ("dim".to_owned(), base().dim(Some(AnsiCode::On))),
        ("italic".to_owned(), base().italic(Some(AnsiCode::On))),
        ("inverse".to_owned(), base().reverse(Some(AnsiCode::On))),
        ("hidden".to_owned(), base().hidden(Some(AnsiCode::On))),
        ("strike".to_owned(), base().strike(Some(AnsiCode::On))),
        (
            "blink-slow".to_owned(),
            base().blink_slow(Some(AnsiCode::On)),
        ),
        (
            "blink-fast".to_owned(),
            base().blink_fast(Some(AnsiCode::On)),
        ),
        (
            "underline".to_owned(),
            base().underline(Some(AnsiCode::Underline(None))),
        ),
    ];

    for (name, variant) in [
        ("double-underline", AnsiStyledUnderline::Double),
        ("undercurl", AnsiStyledUnderline::Undercurl),
        ("dotted-underline", AnsiStyledUnderline::Underdotted),
        ("dashed-underline", AnsiStyledUnderline::Underdashed),
    ] {
        styles.push((
            name.to_owned(),
            base().underline(Some(AnsiCode::Underline(Some(variant)))),
        ));
    }

    styles.push((
        "every-attribute".to_owned(),
        base()
            .bold(Some(AnsiCode::On))
            .dim(Some(AnsiCode::On))
            .italic(Some(AnsiCode::On))
            .reverse(Some(AnsiCode::On))
            .hidden(Some(AnsiCode::On))
            .strike(Some(AnsiCode::On))
            .blink_slow(Some(AnsiCode::On))
            .blink_fast(Some(AnsiCode::On))
            .underline(Some(AnsiCode::Underline(Some(
                AnsiStyledUnderline::Undercurl,
            )))),
    ));

    styles
}

pub fn color_styles() -> Vec<(String, CharacterStyles)> {
    let mut styles = Vec::new();

    for named in NAMED_COLORS {
        styles.push((
            format!("fg-{:?}", named),
            base().foreground(Some(AnsiCode::NamedColor(named))),
        ));
        styles.push((
            format!("bg-{:?}", named),
            base().background(Some(AnsiCode::NamedColor(named))),
        ));
    }

    for index in [0u8, 1, 7, 8, 15, 16, 100, 208, 254, 255] {
        styles.push((
            format!("fg-idx{}", index),
            base().foreground(Some(AnsiCode::ColorIndex(index))),
        ));
        styles.push((
            format!("bg-idx{}", index),
            base().background(Some(AnsiCode::ColorIndex(index))),
        ));
    }

    for rgb in [(0, 0, 0), (1, 2, 3), (255, 255, 255), (128, 0, 64)] {
        styles.push((
            format!("fg-rgb{:?}", rgb),
            base().foreground(Some(AnsiCode::RgbCode(rgb))),
        ));
        styles.push((
            format!("bg-rgb{:?}", rgb),
            base().background(Some(AnsiCode::RgbCode(rgb))),
        ));
    }

    for (name, code) in [
        ("named", AnsiCode::NamedColor(NamedColor::BrightMagenta)),
        ("indexed", AnsiCode::ColorIndex(208)),
        ("rgb", AnsiCode::RgbCode((9, 8, 7))),
    ] {
        styles.push((
            format!("underline-color-{}", name),
            base()
                .underline(Some(AnsiCode::Underline(Some(
                    AnsiStyledUnderline::Undercurl,
                ))))
                .underline_color(Some(code)),
        ));
    }

    styles
}

pub fn pair_styles() -> Vec<(String, CharacterStyles)> {
    let mut styles = Vec::new();
    let attributes = attribute_styles();
    for (left_name, left) in &attributes {
        for (right_name, right) in &attributes {
            if left_name >= right_name {
                continue;
            }
            styles.push((format!("{}+{}", left_name, right_name), merge(left, right)));
        }
    }
    styles
}

fn merge(left: &CharacterStyles, right: &CharacterStyles) -> CharacterStyles {
    let mut merged = *left;
    for (target, source) in [
        (&mut merged.bold, right.bold),
        (&mut merged.dim, right.dim),
        (&mut merged.italic, right.italic),
        (&mut merged.reverse, right.reverse),
        (&mut merged.hidden, right.hidden),
        (&mut merged.strike, right.strike),
        (&mut merged.slow_blink, right.slow_blink),
        (&mut merged.fast_blink, right.fast_blink),
        (&mut merged.underline, right.underline),
        (&mut merged.foreground, right.foreground),
        (&mut merged.background, right.background),
        (&mut merged.underline_color, right.underline_color),
    ] {
        if !matches!(source, Some(AnsiCode::Reset)) {
            *target = source;
        }
    }
    merged
}

pub fn styled_stream(styles: &[(String, CharacterStyles)], cols: usize) -> String {
    let mut stream = String::new();
    for (row, (_, style)) in styles.iter().enumerate() {
        stream.push_str(&goto(row, 0));
        stream.push_str(&sgr(style));
        let label = format!("{:width$}", "x", width = cols.min(4));
        stream.push_str(&label);
    }
    stream
}

pub fn wire_streams(rows: usize, cols: usize) -> Vec<(String, String)> {
    let last_row = rows - 1;
    let last_col = cols - 1;
    vec![
        (
            "wide-char-then-narrow-over-its-tail".to_owned(),
            format!("{}\u{4f60}{}x", goto(0, 0), goto(0, 1)),
        ),
        (
            "wide-char-then-narrow-over-its-head".to_owned(),
            format!("{}\u{4f60}{}x", goto(0, 0), goto(0, 0)),
        ),
        (
            "goto-past-the-last-row-and-column".to_owned(),
            format!(
                "{}over{}edge",
                goto(rows + 4, cols + 4),
                goto(last_row, last_col)
            ),
        ),
        (
            "erase-in-display-between-chunks".to_owned(),
            format!("{}before\u{1b}[m\u{1b}[2J{}after", goto(0, 0), goto(1, 0)),
        ),
        (
            "save-and-restore-around-a-chunk".to_owned(),
            format!("{}anchor\u{1b}[s{}moved\u{1b}[u", goto(0, 0), goto(2, 2)),
        ),
        (
            "an-unclosed-hyperlink-runs-to-the-row-end".to_owned(),
            format!(
                "{}\u{1b}]8;id=7;https://example.com/unclosed\u{1b}\\link",
                goto(0, 0)
            ),
        ),
        (
            "an-empty-sgr-resets-every-attribute".to_owned(),
            format!("{}\u{1b}[1;4;31mstyled\u{1b}[mplain", goto(0, 0)),
        ),
        (
            "out-of-range-sgr-parameters-are-ignored".to_owned(),
            format!("{}\u{1b}[999m\u{1b}[65535mtext", goto(0, 0)),
        ),
        (
            "a-color-parameter-of-five-is-not-a-blink".to_owned(),
            format!("{}\u{1b}[38;5;5;48;5;6mtext", goto(0, 0)),
        ),
        (
            "cursor-visibility-toggles".to_owned(),
            format!("\u{1b}[?25l{}hidden\u{1b}[?25h", goto(0, 0)),
        ),
        (
            "control-bytes-embedded-in-a-chunk".to_owned(),
            format!("{}a\u{7}b\u{0}c", goto(0, 0)),
        ),
        (
            "every-cell-of-every-row-is-written".to_owned(),
            (0..rows)
                .map(|row| format!("{}{}", goto(row, 0), "\u{2588}".repeat(cols)))
                .collect::<Vec<_>>()
                .join(""),
        ),
    ]
}

pub fn application_streams(cols: usize) -> Vec<(String, String)> {
    vec![
        (
            "a-wide-character-that-does-not-fit-the-last-column".to_owned(),
            format!("{}\u{4f60}", " ".repeat(cols - 1)),
        ),
        (
            "a-wide-character-overwritten-by-a-narrow-one".to_owned(),
            "\u{4f60}\u{1b}[1;2Hx".to_owned(),
        ),
        (
            "text-wrapping-past-the-last-column".to_owned(),
            "w".repeat(cols + 12),
        ),
        (
            "a-scrolling-region-scrolled-by-line-feeds".to_owned(),
            format!(
                "\u{1b}[3;10r\u{1b}[3;1H{}\u{1b}[r",
                (0..20)
                    .map(|line| format!("line {}\r\n", line))
                    .collect::<String>()
            ),
        ),
        (
            "a-tab-stop-run".to_owned(),
            "a\tb\tc\td\u{1b}[1;1H\u{1b}[3g".to_owned(),
        ),
        (
            "the-dec-alignment-pattern".to_owned(),
            "\u{1b}#8".to_owned(),
        ),
        (
            "dec-special-graphics-line-drawing".to_owned(),
            "\u{1b}(0lqqqk\u{1b}(B plain".to_owned(),
        ),
        (
            "an-alternate-screen-round-trip".to_owned(),
            "primary\u{1b}[?1049hsecondary\u{1b}[?1049l".to_owned(),
        ),
        (
            "insert-and-delete-characters".to_owned(),
            "abcdef\u{1b}[1;3H\u{1b}[2@\u{1b}[1;1H\u{1b}[1P".to_owned(),
        ),
        ("a-repeated-character".to_owned(), "z\u{1b}[40b".to_owned()),
    ]
}

pub fn application_attribute_streams() -> Vec<(String, String)> {
    let mut streams = vec![
        ("bold".to_owned(), "\u{1b}[1m".to_owned()),
        ("dim".to_owned(), "\u{1b}[2m".to_owned()),
        ("italic".to_owned(), "\u{1b}[3m".to_owned()),
        ("underline".to_owned(), "\u{1b}[4m".to_owned()),
        ("blink-slow".to_owned(), "\u{1b}[5m".to_owned()),
        ("blink-fast".to_owned(), "\u{1b}[6m".to_owned()),
        ("inverse".to_owned(), "\u{1b}[7m".to_owned()),
        ("hidden".to_owned(), "\u{1b}[8m".to_owned()),
        ("strike".to_owned(), "\u{1b}[9m".to_owned()),
        ("double-underline".to_owned(), "\u{1b}[4:2m".to_owned()),
        ("undercurl".to_owned(), "\u{1b}[4:3m".to_owned()),
        ("dotted-underline".to_owned(), "\u{1b}[4:4m".to_owned()),
        ("dashed-underline".to_owned(), "\u{1b}[4:5m".to_owned()),
        ("fg-named".to_owned(), "\u{1b}[31m".to_owned()),
        ("fg-bright-named".to_owned(), "\u{1b}[96m".to_owned()),
        ("fg-indexed".to_owned(), "\u{1b}[38;5;208m".to_owned()),
        ("fg-rgb".to_owned(), "\u{1b}[38;2;1;2;3m".to_owned()),
        ("bg-named".to_owned(), "\u{1b}[41m".to_owned()),
        ("bg-bright-named".to_owned(), "\u{1b}[106m".to_owned()),
        ("bg-indexed".to_owned(), "\u{1b}[48;5;99m".to_owned()),
        ("bg-rgb".to_owned(), "\u{1b}[48;2;9;8;7m".to_owned()),
        (
            "underline-color-indexed".to_owned(),
            "\u{1b}[4:3m\u{1b}[58:5:208m".to_owned(),
        ),
        (
            "underline-color-rgb".to_owned(),
            "\u{1b}[4:3m\u{1b}[58:2::1:2:3m".to_owned(),
        ),
    ];
    for (_, stream) in streams.iter_mut() {
        stream.push_str("sample\u{1b}[m");
    }
    streams.push((
        "hyperlink".to_owned(),
        "\u{1b}]8;id=1;https://example.com\u{1b}\\linked\u{1b}]8;;\u{1b}\\".to_owned(),
    ));
    streams.push(("wide".to_owned(), "\u{4f60}\u{597d}x".to_owned()));
    streams
}

pub fn every_stream(rows: usize, cols: usize) -> Vec<(String, String)> {
    let mut streams = wire_streams(rows, cols);
    for (name, styles) in attribute_styles()
        .into_iter()
        .chain(color_styles())
        .chain(pair_styles())
    {
        streams.push((name.clone(), styled_stream(&[(name, styles)], cols)));
    }
    streams
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::projection::compare;
    use crate::reference::Reference;
    use crate::vte_terminal::VteTerminal;

    const ROWS: usize = 40;
    const COLS: usize = 120;

    fn assert_agrees(name: &str, rows: usize, cols: usize, stream: &str) {
        let mut reference = Reference::new(rows, cols);
        reference.advance(stream.as_bytes());
        let mut state = VteTerminal::new(rows, cols);
        state.apply(stream);
        let divergences = compare(&reference.project(), &crate::projection::of_window(&state));
        assert!(
            divergences.is_empty(),
            "{}: {}",
            name,
            divergences
                .iter()
                .map(|divergence| divergence.describe())
                .collect::<Vec<_>>()
                .join("\n")
        );
    }

    #[test]
    fn every_single_attribute_agrees() {
        let styles = attribute_styles();
        assert_agrees("attributes", ROWS, COLS, &styled_stream(&styles, COLS));
    }

    #[test]
    fn every_color_form_agrees() {
        for chunk in color_styles().chunks(ROWS) {
            assert_agrees("colors", ROWS, COLS, &styled_stream(chunk, COLS));
        }
    }

    #[test]
    fn every_attribute_pair_agrees() {
        for chunk in pair_styles().chunks(ROWS) {
            assert_agrees("pairs", ROWS, COLS, &styled_stream(chunk, COLS));
        }
    }

    #[test]
    fn every_wire_stream_agrees() {
        for (name, stream) in wire_streams(ROWS, COLS) {
            assert_agrees(&name, ROWS, COLS, &stream);
        }
    }

    #[test]
    fn every_application_stream_agrees_once_the_server_has_interpreted_it() {
        for (name, stream) in application_streams(COLS) {
            let frames = crate::differential::frames_of(stream.as_bytes(), ROWS, COLS, 4096);
            let joined = frames.concat();
            assert_agrees(&name, ROWS, COLS, &joined);
        }
    }

    #[test]
    fn a_wide_character_at_the_last_column_never_reaches_the_wire() {
        let stream = format!("{}\u{4f60}", " ".repeat(COLS - 1));
        let frames = crate::differential::frames_of(stream.as_bytes(), ROWS, COLS, 4096);
        let joined = frames.concat();
        assert!(
            !joined.contains(&format!("\u{1b}[1;{}H", COLS)),
            "the server positioned a chunk at the last column: {:?}",
            joined
        );

        let direct = format!("{}\u{4f60}", goto(0, COLS - 1));
        let mut reference = Reference::new(ROWS, COLS);
        reference.advance(direct.as_bytes());
        let mut state = VteTerminal::new(ROWS, COLS);
        state.apply(&direct);
        let divergences = compare(&reference.project(), &crate::projection::of_window(&state));
        assert_eq!(divergences.len(), 1);
        assert_eq!(divergences[0].what, format!("cell 0,{}", COLS - 1));
        assert!(divergences[0].actual.contains("LeadingSpacer"));
    }

    #[test]
    fn a_stream_truncated_mid_sequence_and_resumed_agrees() {
        let stream = format!("{}\u{1b}[1;31mred", goto(0, 0));
        for split in 1..stream.len() {
            if !stream.is_char_boundary(split) {
                continue;
            }
            let mut reference = Reference::new(4, 20);
            reference.advance(stream.as_bytes());
            let mut state = VteTerminal::new(4, 20);
            state.apply(&stream[..split]);
            state.apply(&stream[split..]);
            let divergences = compare(&reference.project(), &crate::projection::of_window(&state));
            assert!(
                divergences.is_empty(),
                "split at {}: {}",
                split,
                divergences
                    .iter()
                    .map(|divergence| divergence.describe())
                    .collect::<Vec<_>>()
                    .join("\n")
            );
        }
    }

    #[test]
    fn fast_blink_alone_survives_the_servers_own_serialization() {
        let fast = sgr(&base().blink_fast(Some(AnsiCode::On)));
        let at = fast.find("\u{1b}[6m").expect("the fast blink code");
        assert!(!fast[at..].contains("\u{1b}[25m"), "{:?}", fast);
        assert_eq!(fast.matches("\u{1b}[25m").count(), 1, "{:?}", fast);

        let slow = sgr(&base().blink_slow(Some(AnsiCode::On)));
        let at = slow.find("\u{1b}[5m").expect("the slow blink code");
        assert!(!slow[at..].contains("\u{1b}[25m"), "{:?}", slow);
        assert_eq!(slow.matches("\u{1b}[25m").count(), 1, "{:?}", slow);

        let stream = format!("{}{}x", goto(0, 0), fast);
        let mut reference = Reference::new(4, 8);
        reference.advance(stream.as_bytes());
        assert!(
            reference.project().cells[0][0].attrs.contains("blink-fast"),
            "a fast-blink-only cell reaches the terminal still blinking"
        );
        assert_eq!(reference.project().cells[0][0].attrs.len(), 1);

        let both = sgr(&base()
            .blink_fast(Some(AnsiCode::On))
            .blink_slow(Some(AnsiCode::On)));
        let stream = format!("{}{}x", goto(0, 0), both);
        let mut reference = Reference::new(4, 8);
        reference.advance(stream.as_bytes());
        assert_eq!(reference.project().cells[0][0].attrs.len(), 2);
    }

    #[test]
    fn the_generated_sgr_is_the_servers_own_serialization() {
        let bold = base().bold(Some(AnsiCode::On));
        let emitted = sgr(&bold);
        assert!(emitted.contains("\u{1b}[1m"), "{:?}", emitted);
        assert_eq!(sgr(&base()), "\u{1b}[m");
    }
}
