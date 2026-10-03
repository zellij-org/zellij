use zellij_tile::prelude::*;
use zellij_utils::input::config_blocks::{
    copy_theme_kdl, parse_colour, styling_colours, theme_slots, themes_kdl,
};

use crate::blocks_screen::check_name;
use crate::page::{
    is_click, is_plain, is_shift_tab, markers, print_dim, print_heading, truncate, typed,
    ButtonRow, ColumnLayout, Effect, Page, PageResponse, RowLook, RowScroll,
};

const SLOT_LABEL_WIDTH: usize = 34;
const VALUE_WIDTH: usize = 14;

pub fn source_text(source: ThemeSource) -> &'static str {
    match source {
        ThemeSource::BuiltIn => "built in",
        ThemeSource::ConfigFile => "config file",
        ThemeSource::ThemeFolder => "theme folder",
    }
}

fn theme_order(a: &ThemeEntry, b: &ThemeEntry) -> std::cmp::Ordering {
    (a.source != ThemeSource::ConfigFile, &a.name)
        .cmp(&(b.source != ThemeSource::ConfigFile, &b.name))
}

pub fn restore_effect(original_unsaved: bool, original_config_themes: &[ThemeEntry]) -> Effect {
    if original_unsaved {
        Effect::ReplaceBlocks(themes_kdl(original_config_themes))
    } else {
        Effect::Revert(SettingKey::Themes)
    }
}

fn swatch(colour: &str, x: usize, y: usize) {
    let escape = match parse_colour(colour) {
        Some(PaletteColor::Rgb((r, g, b))) => format!("\u{1b}[38;2;{};{};{}m", r, g, b),
        Some(PaletteColor::EightBit(index)) => format!("\u{1b}[38;5;{}m", index),
        None => return,
    };
    print!("\u{1b}[{};{}H\u{1b}[0m{}███\u{1b}[0m", y + 1, x + 1, escape);
}

#[derive(Debug, Clone, PartialEq)]
enum NamePurpose {
    New,
    Copy(String),
    Rename(String),
}

struct NameForm {
    purpose: NamePurpose,
    input: TextInput,
    error: Option<String>,
}

struct ColourForm {
    name: String,
    inputs: FocusGroup<usize>,
    buttons: ButtonRow,
    scroll: RowScroll,
    live: bool,
    previewed: bool,
    original_unsaved: bool,
    original_config_themes: Vec<ThemeEntry>,
    error: Option<String>,
}

pub struct ThemesScreen {
    themes: Vec<ThemeEntry>,
    saved: Vec<ThemeEntry>,
    active: String,
    themes_unsaved: bool,
    selected: usize,
    colour_form: Option<ColourForm>,
    name_form: Option<NameForm>,
    dialog: Option<(ConfirmDialog, String)>,
    effects: Vec<Effect>,
    notice: Option<String>,
    focused: bool,
    scroll: RowScroll,
    buttons: ButtonRow,
    form_buttons: ButtonRow,
}

impl Default for ThemesScreen {
    fn default() -> Self {
        ThemesScreen {
            themes: vec![],
            saved: vec![],
            active: String::new(),
            themes_unsaved: false,
            selected: 0,
            colour_form: None,
            name_form: None,
            dialog: None,
            effects: vec![],
            notice: None,
            focused: false,
            scroll: RowScroll::default(),
            buttons: ButtonRow::new(&["+ New theme", "Colours", "Copy", "Rename", "Delete"]),
            form_buttons: ButtonRow::new(&["Apply", "Cancel"]),
        }
    }
}

impl ThemesScreen {
    pub fn set_themes(
        &mut self,
        themes: Vec<ThemeEntry>,
        saved: Vec<ThemeEntry>,
        active: String,
        themes_unsaved: bool,
    ) {
        self.themes = themes;
        self.themes.sort_by(theme_order);
        self.saved = saved;
        self.active = active;
        self.themes_unsaved = themes_unsaved;
        self.selected = self.selected.min(self.themes.len().saturating_sub(1));
    }
    fn config_themes(&self) -> Vec<ThemeEntry> {
        self.themes
            .iter()
            .filter(|theme| theme.source == ThemeSource::ConfigFile)
            .cloned()
            .collect()
    }
    fn selected_theme(&self) -> Option<ThemeEntry> {
        self.themes.get(self.selected).cloned()
    }
    fn is_unsaved(&self, theme: &ThemeEntry) -> bool {
        theme.source == ThemeSource::ConfigFile
            && self.saved.iter().find(|saved| saved.name == theme.name) != Some(theme)
    }
    fn apply_config_themes(&mut self, config_themes: Vec<ThemeEntry>) {
        let matches_saved = config_themes.len() == self.saved.len()
            && config_themes
                .iter()
                .all(|theme| self.saved.iter().any(|saved| saved == theme));
        if matches_saved {
            self.effects.push(Effect::Revert(SettingKey::Themes));
        } else {
            self.effects
                .push(Effect::ReplaceBlocks(themes_kdl(&config_themes)));
        }
        self.themes
            .retain(|theme| theme.source != ThemeSource::ConfigFile);
        for theme in config_themes {
            self.themes.retain(|other| other.name != theme.name);
            self.themes.push(theme);
        }
        self.themes.sort_by(theme_order);
    }
    pub fn open_colours(&mut self) {
        let Some(theme) = self.selected_theme() else {
            return;
        };
        if theme.source != ThemeSource::ConfigFile {
            self.notice = Some(format!(
                "{} comes from the {} and is read-only here; c makes an editable copy",
                theme.name,
                source_text(theme.source)
            ));
            return;
        }
        let mut inputs = FocusGroup::new().wrap(false);
        for (index, (style, component)) in theme_slots().iter().enumerate() {
            let value = theme.colours.get(index).cloned().unwrap_or_default();
            inputs.add(
                index,
                TextInput::new(value)
                    .label(format!("{}.{}", style, component))
                    .label_width(SLOT_LABEL_WIDTH)
                    .placeholder("#rrggbb, r g b or 0-255"),
            );
        }
        inputs.focus_first();
        self.colour_form = Some(ColourForm {
            live: theme.name == self.active,
            name: theme.name,
            inputs,
            buttons: ButtonRow::new(&["Apply", "Cancel"]),
            scroll: RowScroll::default(),
            previewed: false,
            original_unsaved: self.themes_unsaved,
            original_config_themes: self.config_themes(),
            error: None,
        });
    }
    fn edited_config_themes(&self, form: &ColourForm) -> Result<Vec<ThemeEntry>, String> {
        let mut colours = vec![];
        for (index, (style, component)) in theme_slots().iter().enumerate() {
            let text = form
                .inputs
                .text_input(&index)
                .map(|input| input.get_text().trim().to_owned())
                .unwrap_or_default();
            if !text.is_empty() && parse_colour(&text).is_none() {
                return Err(format!(
                    "{}.{}: use #rgb, #rrggbb, \"r g b\" or a number from 0 to 255",
                    style, component
                ));
            }
            let normalized = parse_colour(&text)
                .map(|colour| zellij_utils::input::config_blocks::colour_text(&colour))
                .unwrap_or_default();
            colours.push(normalized);
        }
        Ok(form
            .original_config_themes
            .iter()
            .map(|theme| {
                if theme.name == form.name {
                    ThemeEntry {
                        colours: colours.clone(),
                        ..theme.clone()
                    }
                } else {
                    theme.clone()
                }
            })
            .collect())
    }
    fn preview(&mut self) {
        let Some(form) = self.colour_form.as_ref() else {
            return;
        };
        if !form.live {
            return;
        }
        match self.edited_config_themes(form) {
            Ok(themes) => {
                self.effects
                    .push(Effect::ReplaceBlocks(themes_kdl(&themes)));
                if let Some(form) = self.colour_form.as_mut() {
                    form.previewed = true;
                    form.error = None;
                }
            },
            Err(error) => {
                if let Some(form) = self.colour_form.as_mut() {
                    form.error = Some(error);
                }
            },
        }
    }
    fn apply_colours(&mut self) {
        let Some(form) = self.colour_form.as_ref() else {
            return;
        };
        match self.edited_config_themes(form) {
            Ok(themes) => {
                let name = form.name.clone();
                self.colour_form = None;
                self.apply_config_themes(themes);
                self.notice = Some(format!("Colours of {} applied", name));
            },
            Err(error) => {
                if let Some(form) = self.colour_form.as_mut() {
                    form.error = Some(error);
                }
            },
        }
    }
    fn cancel_colours(&mut self) {
        if let Some(form) = self.colour_form.take() {
            if form.previewed {
                self.effects.push(restore_effect(
                    form.original_unsaved,
                    &form.original_config_themes,
                ));
            }
        }
    }
    fn open_name_form(&mut self, purpose: NamePurpose) {
        let initial = match &purpose {
            NamePurpose::New => String::new(),
            NamePurpose::Copy(from) => format!("{}-custom", from),
            NamePurpose::Rename(from) => from.clone(),
        };
        self.name_form = Some(NameForm {
            purpose,
            input: TextInput::new(initial)
                .label("Name")
                .label_width(6)
                .focused(),
            error: None,
        });
    }
    fn apply_name(&mut self) {
        let Some(form) = self.name_form.as_ref() else {
            return;
        };
        let name = match check_name(form.input.get_text(), "theme name") {
            Ok(name) => name,
            Err(error) => {
                if let Some(form) = self.name_form.as_mut() {
                    form.error = Some(error);
                }
                return;
            },
        };
        let renaming_same = matches!(&form.purpose, NamePurpose::Rename(from) if *from == name);
        if !renaming_same && self.themes.iter().any(|theme| theme.name == name) {
            if let Some(form) = self.name_form.as_mut() {
                form.error = Some(format!("A theme called {} already exists", name));
            }
            return;
        }
        let purpose = form.purpose.clone();
        self.name_form = None;
        let mut config_themes = self.config_themes();
        match purpose {
            NamePurpose::New => {
                let colours = self
                    .themes
                    .iter()
                    .find(|theme| theme.name == "default")
                    .map(|theme| theme.colours.clone())
                    .unwrap_or_else(|| styling_colours(&DEFAULT_STYLES));
                config_themes.push(ThemeEntry {
                    name: name.clone(),
                    source: ThemeSource::ConfigFile,
                    colours,
                });
            },
            NamePurpose::Copy(from) => {
                let Some(source) = self.themes.iter().find(|theme| theme.name == from) else {
                    return;
                };
                if source.source != ThemeSource::ConfigFile {
                    self.effects
                        .push(Effect::ReplaceBlocks(copy_theme_kdl(&from, &name)));
                    self.notice = Some(format!("{} is an editable copy of {}", name, from));
                    return;
                }
                config_themes.push(ThemeEntry {
                    name: name.clone(),
                    source: ThemeSource::ConfigFile,
                    colours: source.colours.clone(),
                });
            },
            NamePurpose::Rename(from) => {
                for theme in config_themes.iter_mut() {
                    if theme.name == from {
                        theme.name = name.clone();
                    }
                }
                if from == self.active {
                    self.notice = Some(format!(
                        "{} was the active theme; choose {} under Appearance to keep using it",
                        from, name
                    ));
                }
            },
        }
        self.apply_config_themes(config_themes);
        if let Some(index) = self.themes.iter().position(|theme| theme.name == name) {
            self.selected = index;
        }
    }
    fn request_delete(&mut self) {
        let Some(theme) = self.selected_theme() else {
            return;
        };
        if theme.source != ThemeSource::ConfigFile {
            self.notice = Some(format!(
                "{} comes from the {} and cannot be deleted here",
                theme.name,
                source_text(theme.source)
            ));
            return;
        }
        self.dialog = Some((
            ConfirmDialog::new(
                "Delete theme?",
                format!("Delete the theme {} from the config file?", theme.name),
            )
            .buttons(vec!["Delete", "Cancel"])
            .width(56)
            .opened(),
            theme.name,
        ));
    }
    fn revert_selected(&mut self) {
        let Some(theme) = self.selected_theme() else {
            return;
        };
        if !self.is_unsaved(&theme) {
            self.notice = Some("Nothing to revert".to_owned());
            return;
        }
        let mut config_themes: Vec<ThemeEntry> = self
            .config_themes()
            .into_iter()
            .filter(|other| other.name != theme.name)
            .collect();
        if let Some(saved) = self.saved.iter().find(|saved| saved.name == theme.name) {
            config_themes.push(saved.clone());
        }
        self.apply_config_themes(config_themes);
        self.notice = Some(format!("Reverted {}", theme.name));
    }
    fn handle_colour_key(&mut self, key: &KeyWithModifier) {
        let Some(form) = self.colour_form.as_mut() else {
            return;
        };
        if is_plain(key, BareKey::Esc) {
            self.cancel_colours();
            return;
        }
        form.scroll.follow();
        if is_plain(key, BareKey::Down) {
            form.inputs.focus_next();
            return;
        }
        if is_plain(key, BareKey::Up) {
            form.inputs.focus_prev();
            return;
        }
        match form.inputs.handle_key(key) {
            FocusEvent::Element {
                response: UiResponse::Submitted(_),
                ..
            } => self.apply_colours(),
            FocusEvent::Element {
                response: UiResponse::Changed(_),
                ..
            } => self.preview(),
            FocusEvent::Element {
                response: UiResponse::Cancelled,
                ..
            } => self.cancel_colours(),
            _ => {},
        }
    }
}

impl ThemesScreen {
    fn handle_colour_mouse(&mut self, mouse: Mouse) {
        let Some(form) = self.colour_form.as_mut() else {
            return;
        };
        if form.scroll.handle_wheel(&mouse).is_some() {
            return;
        }
        match form.buttons.handle_mouse(mouse) {
            Some(0) => {
                self.apply_colours();
                return;
            },
            Some(_) => {
                self.cancel_colours();
                return;
            },
            None => {},
        }
        form.inputs.handle_mouse(mouse);
    }
    fn handle_name_mouse(&mut self, mouse: Mouse) {
        let Some(form) = self.name_form.as_mut() else {
            return;
        };
        form.input.handle_mouse(mouse);
        match self.form_buttons.handle_mouse(mouse) {
            Some(0) => self.apply_name(),
            Some(_) => self.name_form = None,
            None => {},
        }
    }
    fn button_pressed(&mut self, button: usize) {
        match button {
            0 => self.open_name_form(NamePurpose::New),
            1 => self.open_colours(),
            2 => {
                if let Some(theme) = self.selected_theme() {
                    self.open_name_form(NamePurpose::Copy(theme.name));
                }
            },
            3 => self.request_rename(),
            _ => self.request_delete(),
        }
    }
    fn delete_theme(&mut self, name: &str) {
        let config_themes: Vec<ThemeEntry> = self
            .config_themes()
            .into_iter()
            .filter(|theme| theme.name != name)
            .collect();
        self.apply_config_themes(config_themes);
        self.notice = Some(format!("Deleted {}", name));
    }
    fn request_rename(&mut self) {
        match self.selected_theme() {
            Some(theme) if theme.source == ThemeSource::ConfigFile => {
                self.open_name_form(NamePurpose::Rename(theme.name))
            },
            Some(theme) => {
                self.notice = Some(format!(
                    "{} comes from the {} and cannot be renamed here",
                    theme.name,
                    source_text(theme.source)
                ))
            },
            None => {},
        }
    }
    fn handle_list_mouse(&mut self, mouse: Mouse) -> PageResponse {
        if let Some(button) = self.buttons.handle_mouse(mouse) {
            self.button_pressed(button);
            return PageResponse::Handled;
        }
        if self.scroll.hover(&mouse).is_some() {
            return PageResponse::Handled;
        }
        if let Some(changed) = self.scroll.handle_wheel(&mouse) {
            return if changed {
                PageResponse::Handled
            } else {
                PageResponse::NotHandled
            };
        }
        let Some((line, column)) = is_click(&mouse) else {
            return PageResponse::NotHandled;
        };
        match self.scroll.row_at(line, column) {
            Some(row) if self.focused && row == self.selected => {
                self.open_colours();
                PageResponse::Handled
            },
            Some(row) => {
                self.selected = row;
                PageResponse::Handled
            },
            None => PageResponse::NotHandled,
        }
    }
}

impl Page for ThemesScreen {
    fn set_snapshot(&mut self, snapshot: &ConfigSnapshot) {
        let active = snapshot
            .setting(SettingKey::Theme)
            .and_then(|setting| setting.current_value.clone())
            .unwrap_or_else(|| "default".to_owned());
        let unsaved = snapshot
            .setting(SettingKey::Themes)
            .map(|setting| setting.is_unsaved())
            .unwrap_or(false);
        self.set_themes(
            snapshot.blocks.themes.clone(),
            snapshot.saved_blocks.themes.clone(),
            active,
            unsaved,
        );
    }
    fn handle_key(&mut self, key: &KeyWithModifier) -> PageResponse {
        if let Some((dialog, name)) = self.dialog.as_mut() {
            let response = dialog.handle_key(key);
            let name = name.clone();
            if !dialog.is_open() {
                self.dialog = None;
                if let UiResponse::Submitted(UiValue::Choice { index: 0, .. }) = response {
                    self.delete_theme(&name);
                }
            }
            return PageResponse::Handled;
        }
        if self.colour_form.is_some() {
            self.handle_colour_key(key);
            return PageResponse::Handled;
        }
        if let Some(form) = self.name_form.as_mut() {
            match form.input.handle_key(key) {
                UiResponse::Submitted(_) => self.apply_name(),
                UiResponse::Cancelled => self.name_form = None,
                _ => {},
            }
            return PageResponse::Handled;
        }
        self.scroll.follow();
        if is_plain(key, BareKey::Down) {
            if self.selected + 1 < self.themes.len() {
                self.selected += 1;
            }
        } else if is_plain(key, BareKey::Up) {
            self.selected = self.selected.saturating_sub(1);
        } else if is_plain(key, BareKey::PageDown) {
            self.selected = (self.selected + 10).min(self.themes.len().saturating_sub(1));
        } else if is_plain(key, BareKey::PageUp) {
            self.selected = self.selected.saturating_sub(10);
        } else if is_plain(key, BareKey::Enter) {
            self.open_colours();
        } else if typed(key, 'a') {
            self.open_name_form(NamePurpose::New);
        } else if typed(key, 'c') {
            if let Some(theme) = self.selected_theme() {
                self.open_name_form(NamePurpose::Copy(theme.name));
            }
        } else if typed(key, 'n') {
            self.request_rename();
        } else if typed(key, 'd') {
            self.request_delete();
        } else if typed(key, 'r') {
            self.revert_selected();
        } else if is_plain(key, BareKey::Esc) {
            return PageResponse::Close;
        } else if is_plain(key, BareKey::Tab) || is_plain(key, BareKey::Left) || is_shift_tab(key) {
            return PageResponse::LeaveToMenu;
        } else {
            return PageResponse::NotHandled;
        }
        PageResponse::Handled
    }
    fn render(&mut self, x: usize, y: usize, width: usize, height: usize) {
        if let Some(form) = self.name_form.as_mut() {
            let title = match &form.purpose {
                NamePurpose::New => "New theme".to_owned(),
                NamePurpose::Copy(from) => format!("Editable copy of {}", from),
                NamePurpose::Rename(from) => format!("Rename {}", from),
            };
            print_heading(&title, x, y, width);
            self.scroll.clear();
            self.buttons.clear();
            form.input.set_show_cursor(true);
            form.input.render(x, y + 2, width.min(50));
            self.form_buttons.render(x, y + 4, width);
            if let Some(error) = &form.error {
                print_text_with_coordinates(
                    Text::new(truncate(error, width)).error_color_all(),
                    x,
                    y + 6,
                    None,
                    None,
                );
            }
            return;
        }
        self.form_buttons.clear();
        if let Some(form) = self.colour_form.as_mut() {
            let title = if form.live {
                format!("Colours of {} (active: changes preview live)", form.name)
            } else {
                format!("Colours of {}", form.name)
            };
            print_heading(&title, x, y, width);
            self.scroll.clear();
            self.buttons.clear();
            form.inputs.clear_areas();
            let rows = height.saturating_sub(5).max(1);
            let focused = form.inputs.focused_key().copied().unwrap_or(0);
            let slot_count = theme_slots().len();
            let visible = form
                .scroll
                .layout(x, y + 2, width, rows, slot_count, Some(focused));
            for index in visible {
                let Some(row) = form.scroll.screen_row(index) else {
                    continue;
                };
                let is_focused = index == focused;
                if let Some(input) = form.inputs.text_input_mut(&index) {
                    input.set_show_cursor(is_focused);
                    let value = input.get_text().to_owned();
                    input.render(x, row, SLOT_LABEL_WIDTH + VALUE_WIDTH);
                    swatch(&value, x + SLOT_LABEL_WIDTH + VALUE_WIDTH + 1, row);
                }
            }
            form.buttons.render(x, y + 3 + rows, width);
            if let Some(error) = &form.error {
                print_text_with_coordinates(
                    Text::new(truncate(error, width)).error_color_all(),
                    x,
                    y + 4 + rows,
                    None,
                    None,
                );
            }
            return;
        }
        print_dim(
            "Themes from the theme folder and built-in themes are read-only; c edits a copy",
            x,
            y,
            width,
        );
        let has_theme = self.selected_theme().is_some();
        for button in 1..5 {
            self.buttons.set_disabled(button, !has_theme);
        }
        self.buttons.render(x, y + 1, width);
        let list_height = height.saturating_sub(3).max(1);
        let visible = self.scroll.layout(
            x,
            y + 3,
            width,
            list_height,
            self.themes.len(),
            Some(self.selected),
        );
        let rows: Vec<(Vec<String>, String)> = self
            .themes
            .iter()
            .map(|theme| {
                let source = if theme.name == self.active {
                    format!("{} (active)", source_text(theme.source))
                } else {
                    source_text(theme.source).to_owned()
                };
                let marker = markers(
                    self.is_unsaved(theme),
                    theme.source != ThemeSource::ConfigFile,
                    false,
                );
                (vec![theme.name.clone(), source], marker)
            })
            .collect();
        let layout = ColumnLayout::new(
            rows.iter()
                .map(|(columns, marker)| (columns.as_slice(), marker.as_str())),
            width,
        );
        for index in visible {
            let Some(screen_y) = self.scroll.screen_row(index) else {
                continue;
            };
            let (columns, marker) = &rows[index];
            let look = RowLook::new(
                self.focused && index == self.selected,
                self.scroll.is_hovered(index),
            );
            layout.print(columns, marker, x, screen_y, self.scroll.row_width(), look);
        }
    }
    fn handle_mouse(&mut self, mouse: Mouse) -> PageResponse {
        if let Some((dialog, name)) = self.dialog.as_mut() {
            let response = dialog.handle_mouse(mouse);
            let name = name.clone();
            if !dialog.is_open() {
                self.dialog = None;
                if let UiResponse::Submitted(UiValue::Choice { index: 0, .. }) = response {
                    self.delete_theme(&name);
                }
            }
            return PageResponse::Handled;
        }
        if self.colour_form.is_some() {
            self.handle_colour_mouse(mouse);
            return PageResponse::Handled;
        }
        if self.name_form.is_some() {
            self.handle_name_mouse(mouse);
            return PageResponse::Handled;
        }
        self.handle_list_mouse(mouse)
    }
    fn handle_timer(&mut self) -> bool {
        let form_changed = self
            .colour_form
            .as_mut()
            .map(|form| form.buttons.handle_timer())
            .unwrap_or(false);
        self.buttons.handle_timer() | self.form_buttons.handle_timer() | form_changed
    }
    fn render_overlays(&mut self, rows: usize, cols: usize) {
        if let Some((dialog, _)) = self.dialog.as_mut() {
            dialog.render_centered(rows, cols);
        }
    }
    fn captures_keys(&self) -> bool {
        self.colour_form.is_some() || self.name_form.is_some() || self.dialog.is_some()
    }
    fn hints(&self) -> Vec<(&'static str, &'static str)> {
        if self.colour_form.is_some() {
            return vec![
                ("<↓↑>", "colour"),
                ("<Enter>", "apply"),
                ("<Esc>", "cancel"),
            ];
        }
        if self.name_form.is_some() || self.dialog.is_some() {
            return vec![("<Enter>", "confirm"), ("<Esc>", "cancel")];
        }
        vec![
            ("<Enter>", "colours"),
            ("<a>", "add"),
            ("<c>", "copy"),
            ("<n>", "rename"),
            ("<d>", "delete"),
            ("<r>", "revert"),
            ("<←>", "categories"),
        ]
    }
    fn take_effects(&mut self) -> Vec<Effect> {
        std::mem::take(&mut self.effects)
    }
    fn take_notice(&mut self) -> Option<String> {
        self.notice.take()
    }
    fn leave(&mut self) {
        self.cancel_colours();
        self.name_form = None;
        self.dialog = None;
    }
    fn set_focused(&mut self, focused: bool) {
        self.focused = focused;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn theme(name: &str, source: ThemeSource, first: &str) -> ThemeEntry {
        let mut colours = styling_colours(&DEFAULT_STYLES);
        colours[0] = first.to_owned();
        ThemeEntry {
            name: name.to_owned(),
            source,
            colours,
        }
    }

    fn screen() -> ThemesScreen {
        let mine = theme("mine", ThemeSource::ConfigFile, "#102030");
        let mut screen = ThemesScreen::default();
        screen.set_themes(
            vec![theme("dracula", ThemeSource::BuiltIn, "1"), mine.clone()],
            vec![mine],
            "mine".to_owned(),
            false,
        );
        screen
    }

    fn type_text(screen: &mut ThemesScreen, text: &str) {
        for character in text.chars() {
            screen.handle_key(&KeyWithModifier::new(BareKey::Char(character)));
        }
    }

    #[test]
    fn a_read_only_theme_is_copied_before_editing() {
        let mut screen = screen();
        assert_eq!(screen.themes[0].name, "mine");
        screen.handle_key(&KeyWithModifier::new(BareKey::Down));
        screen.handle_key(&KeyWithModifier::new(BareKey::Enter));
        assert!(screen.colour_form.is_none());
        assert!(screen.take_notice().unwrap().contains("read-only"));
        screen.handle_key(&KeyWithModifier::new(BareKey::Char('c')));
        screen.handle_key(&KeyWithModifier::new(BareKey::Enter));
        let effects = screen.take_effects();
        match &effects[..] {
            [Effect::ReplaceBlocks(kdl)] => {
                assert_eq!(kdl, &copy_theme_kdl("dracula", "dracula-custom"));
            },
            other => panic!("unexpected {:?}", other),
        }
    }

    #[test]
    fn editing_the_active_theme_previews_and_cancelling_restores() {
        let mut screen = screen();
        screen.handle_key(&KeyWithModifier::new(BareKey::Enter));
        assert!(screen.colour_form.is_some());
        screen.handle_key(&KeyWithModifier::new(BareKey::End));
        screen.handle_key(&KeyWithModifier::new(BareKey::Backspace));
        type_text(&mut screen, "1");
        let effects = screen.take_effects();
        assert!(!effects.is_empty());
        assert!(
            matches!(effects.last(), Some(Effect::ReplaceBlocks(kdl)) if kdl.contains("16 32 49"))
        );
        screen.handle_key(&KeyWithModifier::new(BareKey::Esc));
        assert_eq!(
            screen.take_effects(),
            vec![Effect::Revert(SettingKey::Themes)]
        );
    }

    #[test]
    fn restoring_an_unsaved_original_sets_it_again() {
        let original = vec![theme("mine", ThemeSource::ConfigFile, "#000000")];
        assert_eq!(
            restore_effect(true, &original),
            Effect::ReplaceBlocks(themes_kdl(&original))
        );
        assert_eq!(
            restore_effect(false, &original),
            Effect::Revert(SettingKey::Themes)
        );
    }

    #[test]
    fn an_invalid_colour_is_not_applied() {
        let mut screen = screen();
        screen.select_for_test(0);
        screen.handle_key(&KeyWithModifier::new(BareKey::Enter));
        type_text(&mut screen, "zz");
        screen.take_effects();
        screen.handle_key(&KeyWithModifier::new(BareKey::Enter));
        assert!(screen.colour_form.is_some());
        assert!(screen.take_effects().is_empty());
    }

    impl ThemesScreen {
        fn select_for_test(&mut self, index: usize) {
            self.selected = index;
        }
    }
}
