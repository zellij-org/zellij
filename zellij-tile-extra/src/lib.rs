#[macro_export]
macro_rules! rgb {
    ($a:expr) => {
        ansi_term::Color::RGB($a.0, $a.1, $a.2)
    };
}

#[macro_export]
macro_rules! style {
    ($a:expr, $b:expr) => {
        ansi_term::Style::new()
            .fg(ansi_term::Color::RGB($a.0, $a.1, $a.2))
            .on(ansi_term::Color::RGB($b.0, $b.1, $b.2))
    };
}
