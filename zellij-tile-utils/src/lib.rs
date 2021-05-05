#[macro_export]
macro_rules! rgb {
    ($a:expr) => {
        ansi_term::Color::Rgb($a.0, $a.1, $a.2)
    };
}

#[macro_export]
macro_rules! style {
    ($fg:expr, $bg:expr) => {
        ansi_term::Style::new()
            .fg(match $fg {
                PaletteColor::Rgb((r, g, b)) => ansi_term::Color::RGB(r, g, b),
                PaletteColor::EightBit(color) => ansi_term::Color::Fixed(color),
            })
            .on(match $bg {
                PaletteColor::Rgb((r, g, b)) => ansi_term::Color::RGB(r, g, b),
                PaletteColor::EightBit(color) => ansi_term::Color::Fixed(color),
            })
    };
}
