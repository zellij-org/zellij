use std::collections::HashMap;

use ansi_term::{
    unstyled_len, ANSIString, ANSIStrings,
    Color::{Fixed, RGB},
    Style,
};
use lazy_static::lazy_static;

use crate::{palette_match, strings, tip::TipFnMap, LinePart};
use zellij_tile::prelude::*;

lazy_static! {
    pub static ref TIPS_DATA: HashMap<&'static str, TipFnMap> = HashMap::from([(
        "quicknav",
        TipFnMap {
            short: quicknav_short,
            medium: quicknav_medium,
            full: quicknav_full,
        }
    ),]);
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

/**
 * To test, need to wasmtime and cargo-wasi.
 */
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_function_from_static_is_ok() {
        let default_palette = Palette::default();
        let quicknav_map = TIPS_DATA.get(&"quicknav").unwrap();
        let quicknav_full_func = quicknav_map.full;
        let quicknav_full_line = quicknav_full_func(default_palette);

        assert_eq!(quicknav_full_line.len, 122);
    }
}
