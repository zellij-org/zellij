use super::Text;
use std::borrow::Borrow;

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

pub fn serialize_ribbon(text: &Text) -> String {
    format!("\u{1b}Pzribbon;{}\u{1b}\\", text.serialize())
}

pub fn serialize_ribbon_with_coordinates(
    text: &Text,
    x: usize,
    y: usize,
    width: Option<usize>,
    height: Option<usize>,
) -> String {
    let width = width.map(|w| w.to_string()).unwrap_or_default();
    let height = height.map(|h| h.to_string()).unwrap_or_default();

    format!(
        "\u{1b}Pzribbon;{}/{}/{}/{};{}\u{1b}\\",
        x,
        y,
        width,
        height,
        text.serialize()
    )
}

pub fn serialize_ribbon_line<I>(ribbons: I) -> String
where
    I: IntoIterator,
    I::Item: Borrow<Text>,
{
    ribbons
        .into_iter()
        .map(|r| serialize_ribbon(r.borrow()))
        .collect()
}

pub fn serialize_ribbon_line_with_coordinates<I>(
    ribbons: I,
    x: usize,
    y: usize,
    width: Option<usize>,
    height: Option<usize>,
) -> String
where
    I: IntoIterator,
    I::Item: Borrow<Text>,
{
    let mut ribbons = ribbons.into_iter();
    let Some(first) = ribbons.next() else {
        return String::new();
    };

    let mut result = serialize_ribbon_with_coordinates(first.borrow(), x, y, width, height);
    result.push_str(&serialize_ribbon_line(ribbons));
    result
}
