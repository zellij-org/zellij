use super::text::Text;
use super::Coordinates;
use crate::panes::terminal_character::{AnsiCode, CharacterStyles, RESET_STYLES};
use std::collections::{HashMap, HashSet};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};
use zellij_utils::data::{PaletteColor, Style};

#[derive(Debug, Default, Clone)]
pub struct WidgetState {
    flags: HashSet<String>,
    values: HashMap<String, String>,
}

impl WidgetState {
    pub fn parse(raw: &str) -> Self {
        let mut state = WidgetState::default();
        for token in raw.split(',').filter(|t| !t.is_empty()) {
            match token.split_once('=') {
                Some((key, value)) => {
                    state.values.insert(key.to_owned(), value.to_owned());
                },
                None => {
                    state.flags.insert(token.to_owned());
                },
            }
        }
        state
    }
    pub fn flag(&self, name: &str) -> bool {
        self.flags.contains(name)
    }
    pub fn number(&self, name: &str) -> Option<usize> {
        self.values.get(name).and_then(|v| v.parse::<usize>().ok())
    }
    pub fn count(&self, name: &str) -> usize {
        self.number(name).unwrap_or(0)
    }
}

pub type WidgetRenderer = fn(&WidgetState, &[String], &Style, &Coordinates) -> Vec<u8>;

pub fn decode_text(raw: &str) -> String {
    if raw.is_empty() {
        return String::new();
    }
    let bytes: Vec<u8> = raw
        .split(',')
        .filter_map(|b| b.parse::<u8>().ok())
        .collect();
    String::from_utf8_lossy(&bytes).to_string()
}

pub fn text_width(text: &str) -> usize {
    UnicodeWidthStr::width(text)
}

pub fn truncate(text: &str, max_width: usize) -> String {
    if text_width(text) <= max_width {
        return text.to_owned();
    }
    if max_width == 0 {
        return String::new();
    }
    let mut result = String::new();
    let mut width = 0;
    for character in text.chars() {
        let character_width = character.width().unwrap_or(0);
        if width + character_width > max_width.saturating_sub(1) {
            break;
        }
        width += character_width;
        result.push(character);
    }
    result.push('…');
    result
}

pub fn fit(text: &str, width: usize) -> String {
    let mut fitted = truncate(text, width);
    let fitted_width = text_width(&fitted);
    for _ in fitted_width..width {
        fitted.push(' ');
    }
    fitted
}

pub fn fit_right(text: &str, width: usize) -> String {
    let fitted = truncate(text, width);
    let padding = width.saturating_sub(text_width(&fitted));
    format!("{}{}", " ".repeat(padding), fitted)
}

pub fn center(text: &str, width: usize) -> String {
    let fitted = truncate(text, width);
    let free = width.saturating_sub(text_width(&fitted));
    let left = free / 2;
    format!("{}{}{}", " ".repeat(left), fitted, " ".repeat(free - left))
}

pub fn colored(foreground: PaletteColor, background: Option<PaletteColor>) -> CharacterStyles {
    let styles = RESET_STYLES.foreground(Some(foreground.into()));
    match background {
        Some(background) => styles.background(Some(background.into())),
        None => styles,
    }
}

pub fn bold(styles: CharacterStyles) -> CharacterStyles {
    styles.bold(Some(AnsiCode::On))
}

pub fn dimmed(styles: CharacterStyles) -> CharacterStyles {
    styles.dim(Some(AnsiCode::On))
}

pub fn disabled_look(styles: CharacterStyles) -> CharacterStyles {
    styles.italic(Some(AnsiCode::On)).dim(Some(AnsiCode::On))
}

pub fn reversed(styles: CharacterStyles) -> CharacterStyles {
    styles.reverse(Some(AnsiCode::On))
}

pub fn paint(styles: CharacterStyles, text: &str) -> String {
    if text.is_empty() {
        return String::new();
    }
    format!("{}{}{}", styles, text, RESET_STYLES)
}

pub fn move_to(coordinates: &Coordinates, row: usize, column: usize) -> String {
    format!(
        "\u{1b}[{};{}H",
        coordinates.y + row + 1,
        coordinates.x + column + 1
    )
}

pub fn label_width(state: &WidgetState, label: &str) -> usize {
    state.number("lw").unwrap_or_else(|| {
        if label.is_empty() {
            0
        } else {
            text_width(label) + 1
        }
    })
}

pub fn render_label(
    label: &Text,
    width: usize,
    focused: bool,
    disabled: bool,
    style: &Style,
) -> String {
    if width == 0 {
        return String::new();
    }
    let mut styles = colored(style.colors.text_unselected.base, None);
    if disabled {
        styles = disabled_look(styles);
    } else if focused {
        styles = bold(colored(style.colors.text_unselected.emphasis_0, None));
    }
    paint_text(label, styles, width, !disabled, style)
}

pub fn decode_styled(field: Option<&String>) -> Text {
    field
        .map(|field| super::text::decode_text_field(field))
        .unwrap_or_else(|| Text::plain(""))
}

pub fn paint_text(
    text: &Text,
    base: CharacterStyles,
    width: usize,
    colorable: bool,
    style: &Style,
) -> String {
    let declaration = style.colors.text_unselected;
    let truncated = text_width(&text.text) > width;
    let room = if truncated {
        width.saturating_sub(1)
    } else {
        width
    };
    let mut output = String::new();
    let mut run = String::new();
    let mut run_styles = base;
    let mut used = 0;
    for (index, character) in text.text.chars().enumerate() {
        let character_width = character.width().unwrap_or(0);
        if used + character_width > room {
            break;
        }
        used += character_width;
        let mut character_styles = base;
        if colorable && text.is_styled_at(index) {
            if let Some(color) = text.style_of_index(index, &declaration, &style.colors) {
                character_styles = bold(character_styles.foreground(Some(color.into())));
            }
            if text.is_dimmed_at(index) {
                character_styles = dimmed(character_styles.bold(Some(AnsiCode::Reset)));
            } else if text.is_unbold_at(index) {
                character_styles = character_styles.bold(Some(AnsiCode::Reset));
            }
        }
        if character_styles != run_styles && !run.is_empty() {
            output.push_str(&paint(run_styles, &run));
            run.clear();
        }
        run_styles = character_styles;
        run.push(character);
    }
    output.push_str(&paint(run_styles, &run));
    if truncated && width > 0 {
        output.push_str(&paint(base, "…"));
        used += 1;
    }
    output.push_str(&paint(base, &" ".repeat(width.saturating_sub(used))));
    output
}

pub fn frame_color(style: &Style, focused: bool) -> PaletteColor {
    if focused {
        style.colors.frame_selected.base
    } else {
        style
            .colors
            .frame_unselected
            .map(|d| d.base)
            .unwrap_or(style.colors.list_unselected.base)
    }
}

pub fn gray(style: &Style) -> PaletteColor {
    style.colors.text_selected.background
}

pub fn plain_styles(style: &Style) -> CharacterStyles {
    colored(style.colors.text_unselected.base, None)
}

pub fn selected_row_styles(style: &Style) -> CharacterStyles {
    bold(colored(
        style.colors.text_selected.base,
        Some(style.colors.text_selected.background),
    ))
}

pub fn hovered_row_styles(style: &Style) -> CharacterStyles {
    colored(style.colors.text_unselected.base, Some(gray(style)))
}

pub fn field_bracket_styles(
    style: &Style,
    focused: bool,
    hovered: bool,
    disabled: bool,
    error: bool,
) -> CharacterStyles {
    if disabled {
        disabled_look(plain_styles(style))
    } else if error {
        let styles = colored(style.colors.exit_code_error.base, None);
        if focused {
            bold(styles)
        } else {
            styles
        }
    } else if focused {
        bold(colored(style.colors.frame_selected.base, None))
    } else if hovered {
        bold(plain_styles(style))
    } else {
        plain_styles(style)
    }
}

pub fn accented_field_bracket_styles(
    style: &Style,
    focused: bool,
    hovered: bool,
    disabled: bool,
    error: bool,
    accent: bool,
) -> CharacterStyles {
    if accent && !focused && !disabled && !error {
        bold(colored(style.colors.text_unselected.emphasis_2, None))
    } else {
        field_bracket_styles(style, focused, hovered, disabled, error)
    }
}

pub fn field_styles(
    style: &Style,
    focused: bool,
    hovered: bool,
    disabled: bool,
) -> CharacterStyles {
    if disabled {
        disabled_look(plain_styles(style))
    } else if hovered {
        hovered_row_styles(style)
    } else if focused {
        bold(plain_styles(style))
    } else {
        plain_styles(style)
    }
}

pub fn indicator_label(up: bool, count: usize) -> String {
    format!("{} [+{}]", if up { "↑" } else { "↓" }, count)
}

pub fn indicator_styles(style: &Style, hovered: bool) -> CharacterStyles {
    if hovered {
        bold(colored(
            style.colors.text_unselected.emphasis_1,
            Some(gray(style)),
        ))
    } else {
        colored(style.colors.text_unselected.emphasis_1, None)
    }
}

pub fn indicator_row(
    width: usize,
    up: bool,
    count: usize,
    hovered: bool,
    right_aligned: bool,
    style: &Style,
) -> String {
    if count == 0 {
        return " ".repeat(width);
    }
    let label = indicator_label(up, count);
    let text = if right_aligned {
        fit_right(&format!("{} ", label), width)
    } else {
        fit(&format!(" {}", label), width)
    };
    paint(indicator_styles(style, hovered), &text)
}

pub fn scrollbar_cells(height: usize, total: usize, offset: usize) -> Vec<bool> {
    if height == 0 {
        return vec![];
    }
    if total <= height {
        return vec![true; height];
    }
    let thumb = ((height * height) / total).max(1).min(height);
    let max_offset = total - height;
    let max_thumb_start = height - thumb;
    let thumb_start = (offset.min(max_offset) * max_thumb_start + max_offset / 2) / max_offset;
    (0..height)
        .map(|row| row >= thumb_start && row < thumb_start + thumb)
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonLook {
    Normal,
    Hovered,
    Focused,
    Pressed,
    Disabled,
}

impl ButtonLook {
    pub fn from_state(state: &WidgetState) -> Self {
        if state.flag("d") {
            ButtonLook::Disabled
        } else if state.flag("p") {
            ButtonLook::Pressed
        } else if state.flag("f") {
            ButtonLook::Focused
        } else if state.flag("h") {
            ButtonLook::Hovered
        } else {
            ButtonLook::Normal
        }
    }
}

pub fn button_cells(label: &str, width: usize, look: ButtonLook, style: &Style) -> String {
    button_cells_with_accent(label, width, look, false, style)
}

pub fn button_cells_with_accent(
    label: &str,
    width: usize,
    look: ButtonLook,
    accent_brackets: bool,
    style: &Style,
) -> String {
    button_cells_styled(
        &Text::plain(label),
        width,
        look,
        accent_brackets,
        false,
        style,
    )
}

pub fn button_cells_styled(
    label: &Text,
    width: usize,
    look: ButtonLook,
    accent_brackets: bool,
    left_aligned: bool,
    style: &Style,
) -> String {
    let colors = style.colors;
    let (text_styles, bracket_styles) = match look {
        ButtonLook::Normal => (plain_styles(style), plain_styles(style)),
        ButtonLook::Hovered => (hovered_row_styles(style), bold(hovered_row_styles(style))),
        ButtonLook::Focused => (
            selected_row_styles(style),
            bold(colored(
                colors.frame_selected.base,
                Some(colors.text_selected.background),
            )),
        ),
        ButtonLook::Pressed => {
            let pressed = bold(colored(
                colors.ribbon_unselected.base,
                Some(colors.text_unselected.base),
            ));
            (pressed, pressed)
        },
        ButtonLook::Disabled => (
            disabled_look(plain_styles(style)),
            disabled_look(plain_styles(style)),
        ),
    };
    let bracket_styles = if accent_brackets && look != ButtonLook::Disabled {
        bold(bracket_styles.foreground(Some(colors.text_unselected.emphasis_2.into())))
    } else {
        bracket_styles
    };
    let colorable = !matches!(look, ButtonLook::Pressed | ButtonLook::Disabled);
    if width < 3 {
        return paint_text(label, text_styles, width, colorable, style);
    }
    let inner = width - 2;
    let label_width = text_width(&label.text);
    let (lead, room) = if left_aligned {
        (1.min(inner), inner.saturating_sub(1))
    } else {
        let fitted_width = label_width.min(inner);
        (inner.saturating_sub(fitted_width) / 2, inner)
    };
    let label_cells_width = label_width.min(room);
    let trail = inner.saturating_sub(lead + label_cells_width);
    let label_style = if look == ButtonLook::Focused {
        Style {
            colors: zellij_utils::data::Styling {
                text_unselected: colors.text_selected,
                ..colors
            },
            ..*style
        }
    } else {
        *style
    };
    format!(
        "{}{}{}{}{}",
        paint(bracket_styles, "["),
        paint(text_styles, &" ".repeat(lead)),
        paint_text(
            label,
            text_styles,
            label_cells_width,
            colorable,
            &label_style
        ),
        paint(text_styles, &" ".repeat(trail)),
        paint(bracket_styles, "]")
    )
}

pub fn button_width(label: &str) -> usize {
    text_width(label) + 4
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_left_aligned_button_starts_its_label_after_the_bracket() {
        let style = Style::default();
        let go = Text::plain("Go");
        let cells = button_cells_styled(&go, 10, ButtonLook::Normal, true, true, &style);
        let text = strip_styles(&cells);
        assert_eq!(text, "[ Go     ]");
        let centered = button_cells_styled(&go, 10, ButtonLook::Normal, false, false, &style);
        assert_eq!(strip_styles(&centered), "[   Go   ]");
    }

    fn strip_styles(text: &str) -> String {
        let mut plain = String::new();
        let mut in_escape = false;
        for character in text.chars() {
            if in_escape {
                if character.is_ascii_alphabetic() {
                    in_escape = false;
                }
            } else if character == '\u{1b}' {
                in_escape = true;
            } else {
                plain.push(character);
            }
        }
        plain
    }

    fn styled(text: &str, level: usize, range: std::ops::Range<usize>) -> Text {
        let mut indices = vec![vec![]; level + 1];
        indices[level] = range.collect();
        Text {
            indices,
            ..Text::plain(text)
        }
    }

    #[test]
    fn a_label_paints_its_text_styles_over_the_label_style() {
        let style = Style::default();
        let label = styled("Delete main?", 3, 7..11);
        let painted = render_label(&label, 14, false, false, &style);
        assert_eq!(strip_styles(&painted), "Delete main?  ");
        let emphasis = bold(
            colored(style.colors.text_unselected.base, None)
                .foreground(Some(style.colors.text_unselected.emphasis_3.into())),
        );
        assert!(painted.contains(&paint(emphasis, "main")));
        let disabled = render_label(&label, 14, false, true, &style);
        assert!(!disabled.contains(&paint(emphasis, "main")));
        assert_eq!(
            strip_styles(&render_label(&label, 8, false, false, &style)),
            "Delete …"
        );
    }

    #[test]
    fn a_button_label_keeps_its_text_styles() {
        let style = Style::default();
        let label = styled("Go on", 0, 3..5);
        let cells = button_cells_styled(&label, 9, ButtonLook::Normal, false, false, &style);
        assert_eq!(strip_styles(&cells), "[ Go on ]");
        let emphasis = bold(
            plain_styles(&style).foreground(Some(style.colors.text_unselected.emphasis_0.into())),
        );
        assert!(cells.contains(&paint(emphasis, "on")));
    }

    #[test]
    fn more_items_indicators_use_emphasis_1() {
        let style = Style::default();
        let emphasis_1 = style.colors.text_unselected.emphasis_1;
        assert_eq!(indicator_styles(&style, false), colored(emphasis_1, None));
        assert_eq!(
            indicator_styles(&style, true),
            bold(colored(emphasis_1, Some(gray(&style))))
        );
    }
}
