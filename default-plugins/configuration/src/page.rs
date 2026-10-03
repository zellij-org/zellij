use std::cell::{Cell, RefCell};

use zellij_tile::prelude::*;

thread_local! {
    static OVERLAYS: RefCell<Vec<Rect>> = RefCell::new(vec![]);
    static CONFIRM_REMOVALS: Cell<bool> = Cell::new(true);
}

pub const SHORT_LABEL_WIDTH: usize = 8;
pub const SHORT_FIELD_WIDTH: usize = 34;
pub const BESIDE_SHORT_FIELD: usize = SHORT_FIELD_WIDTH + 2;
pub const DIM: usize = usize::MAX;

pub const DELETE_BUTTONS: [&str; 3] = ["Delete", "Delete, don't ask again", "Cancel"];

pub fn confirm_removals() -> bool {
    CONFIRM_REMOVALS.with(|confirm| confirm.get())
}

pub fn removal_confirmed(response: &UiResponse) -> bool {
    match response {
        UiResponse::Submitted(UiValue::Choice { index: 0, .. }) => true,
        UiResponse::Submitted(UiValue::Choice { index: 1, .. }) => {
            CONFIRM_REMOVALS.with(|confirm| confirm.set(false));
            true
        },
        _ => false,
    }
}

pub fn clear_overlays() {
    OVERLAYS.with(|overlays| overlays.borrow_mut().clear());
}

pub fn note_dropdown(dropdown: &Dropdown) {
    if dropdown.is_open() {
        if let Some(area) = dropdown.list_area() {
            OVERLAYS.with(|overlays| overlays.borrow_mut().push(area));
        }
    }
}

pub fn note_overlay(area: Rect) {
    OVERLAYS.with(|overlays| overlays.borrow_mut().push(area));
}

pub fn render_frame(title: &str, x: usize, y: usize, width: usize, height: usize) {
    let frame = ConfirmDialog::new(title, "")
        .buttons(Vec::<String>::new())
        .width(width);
    let (_, base_height) = frame.size_for(width);
    let mut frame = frame
        .footer_rows(height.saturating_sub(base_height))
        .opened();
    frame.render(x, y, width, height);
}

pub fn note_group<K: Clone + PartialEq>(group: &FocusGroup<K>) {
    for key in group.keys() {
        if let Some(dropdown) = group.dropdown(&key) {
            note_dropdown(dropdown);
        }
    }
}

pub fn under_overlay(line: isize, column: usize) -> bool {
    OVERLAYS.with(|overlays| {
        overlays
            .borrow()
            .iter()
            .any(|area| area.contains(line, column))
    })
}

pub fn outside_overlays(mouse: Mouse) -> Mouse {
    match mouse {
        Mouse::Hover(line, column) if under_overlay(line, column) => Mouse::Hover(-1, 0),
        other => other,
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    Reconfigure(String),
    ReplaceBlocks(String),
    Revert(SettingKey),
    ResetKeys(Vec<(InputMode, KeyWithModifier)>),
    SwitchMode(InputMode),
}

pub fn run_effects(effects: Vec<Effect>) -> bool {
    let mut changed_config = false;
    for effect in effects {
        match effect {
            Effect::Reconfigure(kdl) => {
                reconfigure(kdl, false);
                changed_config = true;
            },
            Effect::ReplaceBlocks(kdl) => {
                replace_config_blocks(kdl);
                changed_config = true;
            },
            Effect::Revert(key) => {
                revert_config(Some(key));
                changed_config = true;
            },
            Effect::ResetKeys(keys) => {
                reset_keys(keys, false);
                changed_config = true;
            },
            Effect::SwitchMode(mode) => switch_to_input_mode(&mode),
        }
    }
    changed_config
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageResponse {
    Handled,
    NotHandled,
    LeaveToMenu,
    LeaveUp,
    Close,
}

pub trait Page {
    fn set_snapshot(&mut self, snapshot: &ConfigSnapshot);
    fn set_mode_info(&mut self, _mode_info: &ModeInfo) {}
    fn handle_key(&mut self, key: &KeyWithModifier) -> PageResponse;
    fn handle_mouse(&mut self, mouse: Mouse) -> PageResponse;
    fn handle_timer(&mut self) -> bool {
        false
    }
    fn render(&mut self, x: usize, y: usize, width: usize, height: usize);
    fn render_overlays(&mut self, _rows: usize, _cols: usize) {}
    fn captures_keys(&self) -> bool;
    fn hints(&self) -> Vec<(&'static str, &'static str)>;
    fn take_effects(&mut self) -> Vec<Effect>;
    fn take_notice(&mut self) -> Option<String>;
    fn leave(&mut self) {}
    fn set_focused(&mut self, _focused: bool) {}
}

pub fn is_plain(key: &KeyWithModifier, bare_key: BareKey) -> bool {
    key.bare_key == bare_key && key.has_no_modifiers()
}

pub fn is_move_key(key: &KeyWithModifier, bare_key: BareKey) -> bool {
    key.bare_key == bare_key
        && (key.has_modifiers(&[KeyModifier::Alt]) || key.has_modifiers(&[KeyModifier::Shift]))
}

pub fn is_shift_tab(key: &KeyWithModifier) -> bool {
    key.bare_key == BareKey::Tab && key.has_modifiers(&[KeyModifier::Shift])
}

pub fn typed(key: &KeyWithModifier, character: char) -> bool {
    key.bare_key == BareKey::Char(character)
        && (key.has_no_modifiers() || key.has_modifiers(&[KeyModifier::Shift]))
}

pub fn truncate(text: &str, width: usize) -> String {
    if text.chars().count() <= width {
        text.to_owned()
    } else if width == 0 {
        String::new()
    } else {
        let mut truncated: String = text.chars().take(width.saturating_sub(1)).collect();
        truncated.push('…');
        truncated
    }
}

pub fn pad(text: &str, width: usize) -> String {
    let text = truncate(text, width);
    let length = text.chars().count();
    format!("{}{}", text, " ".repeat(width.saturating_sub(length)))
}

pub fn short_action_text(text: &str) -> String {
    let mut short = String::new();
    let mut previous_was_space = false;
    for character in text.chars() {
        let character = if character == '\n' { ' ' } else { character };
        if character == ' ' {
            if previous_was_space {
                continue;
            }
            previous_was_space = true;
        } else {
            previous_was_space = false;
        }
        short.push(character);
    }
    short.trim().trim_end_matches(';').trim().to_owned()
}

pub fn mode_label(value: &str) -> Option<&'static str> {
    const LABELS: [&str; 13] = [
        "Normal",
        "Locked",
        "Pane",
        "Tab",
        "Resize",
        "Move",
        "Scroll",
        "Search",
        "EnterSearch",
        "RenameTab",
        "RenamePane",
        "Session",
        "Tmux",
    ];
    LABELS
        .iter()
        .copied()
        .find(|label| label.eq_ignore_ascii_case(value))
}

pub fn action_display_text(text: &str) -> String {
    let short = short_action_text(text);
    let Some((name, rest)) = short.split_once(' ') else {
        return short;
    };
    if !name.contains("Mode") {
        return short;
    }
    let rest = rest
        .split(' ')
        .map(|word| {
            let inner = word.trim_matches('"');
            match mode_label(inner) {
                Some(label) if word.starts_with('"') && word.ends_with('"') && word.len() > 1 => {
                    format!("\"{}\"", label)
                },
                _ => word.to_owned(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ");
    format!("{} {}", name, rest)
}

pub fn action_argument_range(text: &str) -> Option<std::ops::Range<usize>> {
    let space = text.find(' ')?;
    let start = text[..space].chars().count() + 1;
    Some(start..text.chars().count())
}

pub fn actions_summary(actions: &[String]) -> String {
    actions
        .iter()
        .map(|action| action_display_text(action))
        .collect::<Vec<_>>()
        .join("; ")
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RowLook {
    pub selected: bool,
    pub hovered: bool,
}

impl RowLook {
    pub fn new(selected: bool, hovered: bool) -> Self {
        RowLook { selected, hovered }
    }
    pub fn apply(&self, text: Text) -> Text {
        if self.selected {
            text.selected()
        } else if self.hovered {
            text.selected().unbold_all()
        } else {
            text
        }
    }
}

pub type ColumnStyle = (usize, usize, std::ops::Range<usize>);

pub const ROW_INDENT: usize = 2;
pub const COLUMN_GAP: usize = 2;
const MIN_COLUMN_WIDTH: usize = 8;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ColumnLayout {
    pub widths: Vec<usize>,
    pub marker_width: usize,
    pub marker_column: usize,
    pub indent: usize,
}

impl ColumnLayout {
    pub fn new<'a>(
        rows: impl IntoIterator<Item = (&'a [String], &'a str)>,
        available: usize,
    ) -> Self {
        let mut widths: Vec<usize> = vec![];
        let mut marker_width = 0;
        for (columns, marker) in rows {
            if widths.len() < columns.len() {
                widths.resize(columns.len(), 0);
            }
            for (index, column) in columns.iter().enumerate() {
                widths[index] = widths[index].max(column.chars().count());
            }
            marker_width = marker_width.max(marker.chars().count());
        }
        let shown = widths.iter().filter(|width| **width > 0).count();
        let fixed = ROW_INDENT
            + COLUMN_GAP * shown.saturating_sub(1)
            + if marker_width > 0 {
                COLUMN_GAP + marker_width
            } else {
                0
            };
        let mut total = widths.iter().sum::<usize>() + fixed;
        for index in (0..widths.len()).rev() {
            if total <= available {
                break;
            }
            let reducible = widths[index].saturating_sub(MIN_COLUMN_WIDTH);
            let cut = reducible.min(total - available);
            widths[index] -= cut;
            total -= cut;
        }
        ColumnLayout {
            widths,
            marker_width,
            marker_column: 0,
            indent: ROW_INDENT,
        }
    }
    pub fn fit(mut self, available: usize, order: &[usize]) -> Self {
        let shown = self.widths.iter().filter(|width| **width > 0).count();
        let fixed = self.indent
            + COLUMN_GAP * shown.saturating_sub(1)
            + if self.marker_width > 0 {
                COLUMN_GAP + self.marker_width
            } else {
                0
            };
        let mut total = self.widths.iter().sum::<usize>() + fixed;
        for index in order {
            if total <= available {
                break;
            }
            let Some(width) = self.widths.get_mut(*index) else {
                continue;
            };
            let reducible = width.saturating_sub(MIN_COLUMN_WIDTH);
            let cut = reducible.min(total - available);
            *width -= cut;
            total -= cut;
        }
        self
    }
    pub fn with_indent(mut self, indent: usize) -> Self {
        self.indent = indent;
        self
    }
    pub fn text_width(&self) -> usize {
        let shown: Vec<usize> = self.widths.iter().copied().filter(|w| *w > 0).collect();
        self.indent + shown.iter().sum::<usize>() + COLUMN_GAP * shown.len().saturating_sub(1)
    }
    pub fn align_markers(layouts: &mut [ColumnLayout]) {
        let column = layouts
            .iter()
            .map(|layout| layout.text_width())
            .max()
            .unwrap_or(0);
        let marker_width = layouts
            .iter()
            .map(|layout| layout.marker_width)
            .max()
            .unwrap_or(0);
        for layout in layouts.iter_mut() {
            layout.marker_column = column;
            layout.marker_width = marker_width;
        }
    }
    pub fn natural_width(&self, min_marker_width: usize) -> usize {
        let marker_width = self.marker_width.max(min_marker_width);
        let text_end = self.text_width().max(self.marker_column);
        if marker_width > 0 {
            text_end + COLUMN_GAP + marker_width
        } else {
            text_end
        }
    }
    fn column_start(&self, column: usize) -> usize {
        let mut start = self.indent;
        for (index, width) in self.widths.iter().enumerate() {
            if *width == 0 {
                continue;
            }
            if index == column {
                return start;
            }
            start += width + COLUMN_GAP;
        }
        start
    }
    pub fn line(&self, columns: &[String], marker: &str) -> (String, usize) {
        let mut line = " ".repeat(self.indent);
        let mut first = true;
        for (index, width) in self.widths.iter().enumerate() {
            if *width == 0 {
                continue;
            }
            if !first {
                line.push_str(&" ".repeat(COLUMN_GAP));
            }
            first = false;
            let column = columns.get(index).map(|c| c.as_str()).unwrap_or("");
            line.push_str(&pad(column, *width));
        }
        let text_end = line.chars().count();
        if self.marker_column > text_end {
            line.push_str(&" ".repeat(self.marker_column - text_end));
        }
        let marker_start = if self.marker_width > 0 {
            line.push_str(&" ".repeat(COLUMN_GAP));
            let start = line.chars().count();
            line.push_str(&pad(marker, self.marker_width));
            start
        } else {
            line.chars().count()
        };
        (line, marker_start)
    }
    pub fn print(
        &self,
        columns: &[String],
        marker: &str,
        x: usize,
        y: usize,
        width: usize,
        look: RowLook,
    ) {
        self.print_styled(columns, marker, x, y, width, look, &[]);
    }
    pub fn print_styled(
        &self,
        columns: &[String],
        marker: &str,
        x: usize,
        y: usize,
        width: usize,
        look: RowLook,
        styles: &[ColumnStyle],
    ) {
        let (line, marker_start) = self.line(columns, marker);
        let line = pad(&line, width);
        let mut rendered = Text::new(&line);
        if marker_start < width {
            let marker_end = |label: &str| (marker_start + label.chars().count()).min(width);
            if marker.starts_with('●') {
                rendered = rendered.color_range(1, marker_start..marker_end("● unsaved"));
            } else if marker.starts_with('○') {
                rendered = rendered.dim_range(marker_start..marker_end("○ default"));
            }
        }
        for (column, color, range) in styles {
            let dim = *color == DIM;
            let column_width = self.widths.get(*column).copied().unwrap_or(0);
            if column_width == 0 {
                continue;
            }
            let start = self.column_start(*column);
            let from = (start + range.start.min(column_width)).min(width);
            let to = (start + range.end.min(column_width)).min(width);
            if from < to && dim {
                rendered = rendered.dim_range(from..to);
            } else if from < to {
                rendered = rendered.color_range(*color, from..to);
            }
        }
        print_text_with_coordinates(look.apply(rendered), x, y, None, None);
    }
}

pub fn print_heading(text: &str, x: usize, y: usize, width: usize) {
    print_text_with_coordinates(
        Text::new(truncate(text, width)).color_all(2),
        x,
        y,
        None,
        None,
    );
}

pub fn print_dim(text: &str, x: usize, y: usize, width: usize) {
    print_text_with_coordinates(Text::new(truncate(text, width)).dim_all(), x, y, None, None);
}

pub fn markers(unsaved: bool, is_default: bool, restart_only: bool) -> String {
    let mut marker = String::new();
    if unsaved {
        marker.push_str("● unsaved");
    } else if is_default {
        marker.push_str("○ default");
    }
    if restart_only {
        if !marker.is_empty() {
            marker.push(' ');
        }
        marker.push_str("⟳ restart");
    }
    marker
}

#[derive(Debug, Default, Clone)]
pub struct RowScroll {
    view: ScrollView,
    pub hovered: Option<usize>,
    follow: bool,
    area: Option<(usize, usize, usize, usize)>,
    on_top: bool,
}

impl RowScroll {
    pub fn on_top() -> Self {
        RowScroll {
            on_top: true,
            ..RowScroll::default()
        }
    }
    pub fn follow(&mut self) {
        self.follow = true;
    }
    pub fn reset(&mut self) {
        self.view.set_offset(0);
    }
    pub fn layout(
        &mut self,
        x: usize,
        y: usize,
        width: usize,
        height: usize,
        total: usize,
        selected: Option<usize>,
    ) -> std::ops::Range<usize> {
        let height = height.max(1);
        self.area = Some((x, y, width, height));
        self.view.set_total_rows(total);
        let range = self.view.layout(x, y, width, height);
        if self.follow {
            if let Some(selected) = selected {
                self.view.ensure_visible(selected);
            }
            self.follow = false;
        }
        self.view.render_indicators();
        let _ = range;
        self.view.visible_range()
    }
    pub fn fitted_width(
        total: usize,
        height: usize,
        content_width: usize,
        available: usize,
    ) -> usize {
        let height = height.max(1);
        let width = if total > height && height >= 2 {
            content_width + 7 + total.to_string().len() + 1
        } else {
            content_width
        };
        width.min(available)
    }
    pub fn row_width(&self) -> usize {
        self.area
            .map(|(_, _, width, _)| {
                let gutter = self.view.gutter_width();
                if gutter > 0 {
                    width.saturating_sub(gutter + 1)
                } else {
                    width
                }
            })
            .unwrap_or(0)
    }
    pub fn clear(&mut self) {
        self.area = None;
        self.hovered = None;
        self.view.clear_area();
    }
    pub fn is_hovered(&self, row: usize) -> bool {
        self.hovered == Some(row)
    }
    pub fn hover(&mut self, mouse: &Mouse) -> Option<bool> {
        match mouse {
            Mouse::Hover(line, column) => {
                let indicator_changed = self.view.handle_mouse(*mouse) == UiResponse::Consumed;
                let hovered = self.row_at(*line, *column);
                let changed = hovered != self.hovered || indicator_changed;
                self.hovered = hovered;
                Some(changed)
            },
            _ => None,
        }
    }
    pub fn screen_row(&self, row: usize) -> Option<usize> {
        self.area?;
        self.view.screen_row(row)
    }
    pub fn contains(&self, line: isize, column: usize) -> bool {
        match self.area {
            Some((x, y, _, height)) => {
                line >= y as isize
                    && (line as usize) < y + height
                    && column >= x
                    && column < x + self.row_width()
            },
            None => false,
        }
    }
    pub fn row_at(&self, line: isize, column: usize) -> Option<usize> {
        if !self.contains(line, column) || (!self.on_top && under_overlay(line, column)) {
            return None;
        }
        self.view.row_at(line)
    }
    pub fn scroll_by(&mut self, delta: isize) -> bool {
        self.view.scroll_by(delta)
    }
    pub fn is_dragging(&self) -> bool {
        self.view.is_dragging()
    }
    pub fn handle_wheel(&mut self, mouse: &Mouse) -> Option<bool> {
        match mouse {
            Mouse::ScrollUp(lines) => Some(self.scroll_by(-((*lines).max(1) as isize))),
            Mouse::ScrollDown(lines) => Some(self.scroll_by((*lines).max(1) as isize)),
            Mouse::LeftClick(..) | Mouse::Hold(..) | Mouse::Release(..) => {
                match self.view.handle_mouse(*mouse) {
                    UiResponse::NotHandled => None,
                    UiResponse::Changed(_) => Some(true),
                    _ => Some(false),
                }
            },
            _ => None,
        }
    }
}

pub struct ButtonRow {
    buttons: Vec<Button>,
}

impl ButtonRow {
    pub fn new(labels: &[&str]) -> Self {
        ButtonRow {
            buttons: labels.iter().map(|label| Button::new(*label)).collect(),
        }
    }
    pub fn set_disabled(&mut self, index: usize, disabled: bool) {
        if let Some(button) = self.buttons.get_mut(index) {
            button.set_disabled(disabled);
        }
    }
    pub fn render(&mut self, x: usize, y: usize, width: usize) {
        let mut column = x;
        for button in self.buttons.iter_mut() {
            let button_width = button.natural_width();
            if column + button_width <= x + width {
                button.render(column, y);
                column += button_width + 1;
            } else {
                button.clear_area();
            }
        }
    }
    pub fn clear(&mut self) {
        for button in self.buttons.iter_mut() {
            button.clear_area();
        }
    }
    pub fn handle_mouse(&mut self, mouse: Mouse) -> Option<usize> {
        let mouse = outside_overlays(mouse);
        let mut activated = None;
        for (index, button) in self.buttons.iter_mut().enumerate() {
            if let UiResponse::Activated = button.handle_mouse(mouse) {
                activated = Some(index);
            }
        }
        activated
    }
    pub fn handle_timer(&mut self) -> bool {
        let mut changed = false;
        for button in self.buttons.iter_mut() {
            changed |= button.handle_timer();
        }
        changed
    }
}

pub fn is_click(mouse: &Mouse) -> Option<(isize, usize)> {
    match mouse {
        Mouse::LeftClick(line, column) => Some((*line, *column)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(columns: &[&str]) -> Vec<String> {
        columns.iter().map(|c| c.to_string()).collect()
    }

    #[test]
    fn columns_take_the_width_of_their_longest_entry() {
        let rows = vec![
            (
                strings(&["New pane", "Alt n", "NewPane"]),
                "○ default".to_owned(),
            ),
            (
                strings(&["Fullscreen", "", "ToggleFocusFullscreen"]),
                "● unsaved".to_owned(),
            ),
            (strings(&["Hi", "", "NewTab"]), String::new()),
        ];
        let layout = ColumnLayout::new(rows.iter().map(|(c, m)| (c.as_slice(), m.as_str())), 200);
        assert_eq!(layout.widths, vec![10, 5, 21]);
        assert_eq!(layout.marker_width, 9);
        let (first, marker_start) = layout.line(&rows[0].0, &rows[0].1);
        let (second, second_marker_start) = layout.line(&rows[1].0, &rows[1].1);
        assert_eq!(marker_start, second_marker_start);
        assert_eq!(
            first,
            "  New pane    Alt n  NewPane                ○ default"
        );
        assert_eq!(
            second,
            "  Fullscreen         ToggleFocusFullscreen  ● unsaved"
        );
    }

    #[test]
    fn a_narrow_list_shrinks_the_last_columns_first() {
        let rows = vec![(
            strings(&["name", "a very long value that does not fit"]),
            String::new(),
        )];
        let layout = ColumnLayout::new(rows.iter().map(|(c, m)| (c.as_slice(), m.as_str())), 30);
        assert_eq!(layout.widths, vec![4, 22]);
        assert_eq!(layout.line(&rows[0].0, "").0.chars().count(), 30);
    }

    #[test]
    fn an_empty_column_takes_no_space() {
        let rows = vec![(strings(&["A", ""]), String::new())];
        let layout = ColumnLayout::new(rows.iter().map(|(c, m)| (c.as_slice(), m.as_str())), 80);
        assert_eq!(layout.line(&rows[0].0, "").0, "  A");
    }

    #[test]
    fn markers_of_different_lists_can_share_one_column() {
        let first = vec![(strings(&["a", "1"]), "○ default".to_owned())];
        let second = vec![(strings(&["longer name", "2"]), "● unsaved".to_owned())];
        let mut layouts = vec![
            ColumnLayout::new(first.iter().map(|(c, m)| (c.as_slice(), m.as_str())), 80),
            ColumnLayout::new(second.iter().map(|(c, m)| (c.as_slice(), m.as_str())), 80),
        ];
        ColumnLayout::align_markers(&mut layouts);
        assert_eq!(
            layouts[0].line(&first[0].0, &first[0].1).1,
            layouts[1].line(&second[0].0, &second[0].1).1
        );
    }

    #[test]
    fn mode_names_are_capitalized_only_for_display() {
        assert_eq!(
            action_display_text("SwitchToMode \"tmux\";"),
            "SwitchToMode \"Tmux\""
        );
        assert_eq!(
            action_display_text("SwitchToMode \"entersearch\""),
            "SwitchToMode \"EnterSearch\""
        );
        assert_eq!(action_display_text("NewTab \"tmux\""), "NewTab \"tmux\"");
        assert_eq!(mode_label("renamepane"), Some("RenamePane"));
        assert_eq!(mode_label("nothing"), None);
    }

    #[test]
    fn a_fitted_layout_shrinks_the_columns_in_the_given_order() {
        let rows = vec![(
            strings(&["Label of the item", "SomeVeryLongActionName", "Ctrl p"]),
            String::new(),
        )];
        let layout = ColumnLayout::new(
            rows.iter()
                .map(|(columns, marker)| (columns.as_slice(), marker.as_str())),
            usize::MAX,
        )
        .with_indent(0)
        .fit(40, &[1, 0, 2]);
        assert_eq!(layout.widths[2], 6);
        assert_eq!(layout.widths[0], 17);
        assert!(layout.text_width() <= 40);
    }

    #[test]
    fn items_move_with_alt_or_shift_arrows() {
        let down = KeyWithModifier::new(BareKey::Down);
        assert!(is_move_key(&down.clone().with_shift_modifier(), BareKey::Down));
        assert!(is_move_key(&down.clone().with_alt_modifier(), BareKey::Down));
        assert!(!is_move_key(&down, BareKey::Down));
    }
}
