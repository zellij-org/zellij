use std::collections::HashMap;

use ansi_term::{
    unstyled_len, ANSIString, ANSIStrings,
    Color::{Fixed, RGB},
    Style,
};
use lazy_static::lazy_static;

use crate::{tip::TipBody, LinePart};
use zellij_tile::prelude::*;
use zellij_tile_utils::palette_match;

macro_rules! strings {
    ($ANSIStrings:expr) => {{
        let strings: &[ANSIString<'static>] = $ANSIStrings;

        let ansi_strings = ANSIStrings(strings);

        LinePart {
            part: format!("{}", ansi_strings),
            len: unstyled_len(&ansi_strings),
        }
    }};
}

lazy_static! {
    pub static ref TIPS: HashMap<&'static str, TipBody> = HashMap::from([
        (
            "quicknav",
            TipBody {
                short: quicknav_short,
                medium: quicknav_medium,
                full: quicknav_full,
            }
        ),
        // This tip will have deleted before merge.
        (
            "test",
            TipBody {
                short: test_tip,
                medium: test_tip,
                full: test_tip,
            }
        )
    ]);
}

fn quicknav_full(palette: Palette) -> LinePart {
    let green_color = palette_match!(palette.green);
    let orange_color = palette_match!(palette.orange);

    strings!(&[
        Style::new().paint(" Tip: "),
        Style::new().fg(orange_color).bold().paint("Alt"),
        Style::new().paint(" + "),
        Style::new().fg(green_color).bold().paint("<n>"),
        Style::new().paint(" => open new pane. "),
        Style::new().fg(orange_color).bold().paint("Alt"),
        Style::new().paint(" + "),
        Style::new().fg(green_color).bold().paint("<[]"),
        Style::new().paint(" or "),
        Style::new().fg(green_color).bold().paint("hjkl>"),
        Style::new().paint(" => navigate between panes. "),
        Style::new().fg(orange_color).bold().paint("Alt"),
        Style::new().paint(" + "),
        Style::new().fg(green_color).bold().paint("<+->"),
        Style::new().paint(" => increase/decrease pane size."),
    ])
}

fn quicknav_medium(palette: Palette) -> LinePart {
    let green_color = palette_match!(palette.green);
    let orange_color = palette_match!(palette.orange);

    strings!(&[
        Style::new().paint(" Tip: "),
        Style::new().fg(orange_color).bold().paint("Alt"),
        Style::new().paint(" + "),
        Style::new().fg(green_color).bold().paint("<n>"),
        Style::new().paint(" => new pane. "),
        Style::new().fg(orange_color).bold().paint("Alt"),
        Style::new().paint(" + "),
        Style::new().fg(green_color).bold().paint("<[]"),
        Style::new().paint(" or "),
        Style::new().fg(green_color).bold().paint("hjkl>"),
        Style::new().paint(" => navigate. "),
        Style::new().fg(orange_color).bold().paint("Alt"),
        Style::new().paint(" + "),
        Style::new().fg(green_color).bold().paint("<+->"),
        Style::new().paint(" => resize pane."),
    ])
}

fn quicknav_short(palette: Palette) -> LinePart {
    let green_color = palette_match!(palette.green);
    let orange_color = palette_match!(palette.orange);

    strings!(&[
        Style::new().paint(" QuickNav: "),
        Style::new().fg(orange_color).bold().paint("Alt"),
        Style::new().paint(" + "),
        Style::new().fg(green_color).bold().paint("n"),
        Style::new().paint("/"),
        Style::new().fg(green_color).bold().paint("[]"),
        Style::new().paint("/"),
        Style::new().fg(green_color).bold().paint("hjkl"),
        Style::new().paint("/"),
        Style::new().fg(green_color).bold().paint("+-"),
    ])
}

fn test_tip(_: Palette) -> LinePart {
    strings!(&[Style::new().paint(" This is Test Tip :)")])
}
