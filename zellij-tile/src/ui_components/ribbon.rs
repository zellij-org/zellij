use super::Text;

pub fn print_ribbon(text: Text) {
    print!("\u{1b}Pzribbon;{}\u{1b}\\", text.serialize());
}

pub fn print_ribbon_with_coordinates(
    text: Text,
    x: usize,
    y: usize,
    width: Option<usize>,
    height: Option<usize>,
) {
    let width = width.map(|w| w.to_string()).unwrap_or_default();
    let height = height.map(|h| h.to_string()).unwrap_or_default();
    print!(
        "\u{1b}Pzribbon;{}/{}/{}/{};{}\u{1b}\\",
        x,
        y,
        width,
        height,
        text.serialize()
    );
}
