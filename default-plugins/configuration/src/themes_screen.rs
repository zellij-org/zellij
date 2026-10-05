use std::collections::BTreeMap;

use zellij_tile::prelude::*;
use zellij_utils::input::config_blocks::{
    colour_text, parse_colour, styling_colours, theme_slots, themes_kdl,
};

use crate::blocks_screen::check_name;
use crate::contrast::{
    slot_contrasts, suggest, SlotContrast, GOOD_CONTRAST, LOW_CONTRAST,
};
use crate::theme_sample::{render_sample, SAMPLE_MIN_WIDTH};
use crate::ui_components::print_link;
use crate::page::{
    changed_by, is_click, is_plain, is_shift_tab, markers, note_dropdown, outside_overlays,
    print_dim, print_heading, truncate, typed, under_overlay, ButtonRow, ColumnLayout, ColumnStyle, Effect, Page,
    PageResponse, RowLook, RowScroll, BESIDE_SHORT_FIELD, DIM, SHORT_FIELD_WIDTH,
    SHORT_LABEL_WIDTH,
};

const SLOT_LABEL_WIDTH: usize = 34;
const VALUE_WIDTH: usize = 14;
const RATIO_WIDTH: usize = 8;
const LIST_WIDTH: usize = SLOT_LABEL_WIDTH + VALUE_WIDTH + 5 + RATIO_WIDTH;
const COLOUR_HELP: [&str; 2] = [
    "Accepted formats: \"#rrggbb\" (#6fde21), \"rrggbb\" (6fde21), \"r g b\" (111 222 33) or a number (236).",
    "A number from 0 to 255 picks a color from the terminal's 256-color palette.",
];
const COLOUR_ERROR: &str =
    "use #rrggbb, rrggbb, r g b (three numbers from 0 to 255) or a palette number from 0 to 255";
const TITLE: &str = "Themes";
const ALL_SOURCES: &str = "All";
const NEW_THEME: &str = "+ New theme";
const ACTIVE: &str = "active";
const MATCH_COLOR: usize = 1;
const COPY_NAME_FIELD: &str = "name";
const PATH_PREFIX: &str = "Defined in ";
const SOURCE_ORDER: [ThemeSource; 3] = [
    ThemeSource::ConfigFile,
    ThemeSource::ThemeFolder,
    ThemeSource::BuiltIn,
];

pub fn source_text(source: ThemeSource) -> &'static str {
    match source {
        ThemeSource::BuiltIn => "built in",
        ThemeSource::ConfigFile => "config file",
        ThemeSource::ThemeFolder => "theme folder",
    }
}

fn source_heading(source: ThemeSource) -> &'static str {
    match source {
        ThemeSource::BuiltIn => "Built in",
        ThemeSource::ConfigFile => "Config file",
        ThemeSource::ThemeFolder => "Theme folder",
    }
}

fn source_rank(source: ThemeSource) -> usize {
    SOURCE_ORDER
        .iter()
        .position(|other| *other == source)
        .unwrap_or(SOURCE_ORDER.len())
}

fn theme_order(a: &ThemeEntry, b: &ThemeEntry) -> std::cmp::Ordering {
    (source_rank(a.source), &a.name).cmp(&(source_rank(b.source), &b.name))
}

pub fn restore_effect(original_unsaved: bool, original_config_themes: &[ThemeEntry]) -> Effect {
    if original_unsaved {
        Effect::ReplaceBlocks(themes_kdl(original_config_themes))
    } else {
        Effect::Revert(SettingKey::Themes)
    }
}

fn styled_advice(text: String, width: usize, error: Option<usize>, accent: Option<(usize, usize)>) -> Text {
    let text = truncate(&text, width);
    let length = text.chars().count();
    let mut styled = Text::new(&text);
    if let Some(end) = error {
        styled = styled.error_color_range(..end.min(length));
    }
    if let Some((start, end)) = accent {
        if start < length {
            styled = styled.color_range(3, start..end.min(length));
        }
    }
    styled
}

fn contrast_advice(
    slots: &[(&'static str, &'static str)],
    contrasts: &[Option<SlotContrast>],
    focused: usize,
    width: usize,
) -> Option<Text> {
    let low: Vec<usize> = contrasts
        .iter()
        .enumerate()
        .filter(|(_, found)| {
            found
                .as_ref()
                .map(|found| found.ratio < LOW_CONTRAST)
                .unwrap_or(false)
        })
        .map(|(index, _)| index)
        .collect();
    let all_hint = match low.len() {
        0 => String::new(),
        1 => " · Ctrl e fixes the 1 color below 3:1".to_owned(),
        count => format!(" · Ctrl e fixes all {} colors below 3:1", count),
    };
    if let Some(Some(found)) = contrasts.get(focused) {
        if found.ratio < GOOD_CONTRAST {
            let better = suggest(found.foreground, found.background, GOOD_CONTRAST);
            let better_text = colour_text(&better);
            let better_ratio = crate::contrast::contrast(better, found.background);
            let ratio = format!("{:.1}:1", found.ratio);
            let prefix = format!(
                "{} against {} (4.5:1 reads well). Suggested: ",
                ratio, found.background_slot
            );
            let start = prefix.chars().count();
            let text = format!(
                "{}{} ({:.1}:1), Ctrl f uses it{}",
                prefix, better_text, better_ratio, all_hint
            );
            let error = if found.ratio < LOW_CONTRAST {
                Some(ratio.chars().count())
            } else {
                None
            };
            return Some(styled_advice(
                text,
                width,
                error,
                Some((start, start + better_text.chars().count())),
            ));
        }
    }
    if let Some((style, component)) = slots.get(focused) {
        if *component == "background" {
            let slot = format!("{}.{}", style, component);
            let affected = low
                .iter()
                .filter(|index| {
                    contrasts[**index]
                        .as_ref()
                        .map(|found| found.background_slot == slot)
                        .unwrap_or(false)
                })
                .count();
            if affected > 0 {
                let text = format!(
                    "{} color{} on this background {} below 3:1{}",
                    affected,
                    if affected == 1 { "" } else { "s" },
                    if affected == 1 { "is" } else { "are" },
                    all_hint
                );
                return Some(styled_advice(text, width, None, None));
            }
        }
    }
    if low.is_empty() {
        return None;
    }
    let text = format!(
        "{} color{} below 3:1 contrast{}",
        low.len(),
        if low.len() == 1 { " is" } else { "s are" },
        all_hint
    );
    Some(styled_advice(text, width, None, None))
}

fn swatch(colour: &str, x: usize, y: usize) {
    let escape = match parse_colour(colour) {
        Some(PaletteColor::Rgb((r, g, b))) => format!("\u{1b}[38;2;{};{};{}m", r, g, b),
        Some(PaletteColor::EightBit(index)) => format!("\u{1b}[38;5;{}m", index),
        None => return,
    };
    print!("\u{1b}[{};{}H\u{1b}[0m{}███\u{1b}[0m", y + 1, x + 1, escape);
}

struct ThemeMatch {
    name: Vec<usize>,
    score: (usize, usize),
}

fn match_theme(query: &str, theme: &ThemeEntry) -> Option<ThemeMatch> {
    let indices = fuzzy_match_indices(query, &theme.name)?;
    let first = indices.first().copied().unwrap_or(0);
    let last = indices.last().copied().unwrap_or(0);
    Some(ThemeMatch {
        name: indices,
        score: (last - first, first),
    })
}

fn theme_columns(theme: &ThemeEntry, active: &str) -> Vec<String> {
    let active = if theme.name == active {
        ACTIVE.to_owned()
    } else {
        String::new()
    };
    vec![
        theme.name.clone(),
        source_text(theme.source).to_owned(),
        active,
    ]
}

fn theme_styles(columns: &[String], found: Option<&ThemeMatch>) -> Vec<ColumnStyle> {
    let mut styles = vec![];
    let length = columns.first().map(|name| name.chars().count()).unwrap_or(0);
    for index in 0..length {
        let matched = found.map(|m| m.name.contains(&index)).unwrap_or(false);
        let color = if matched { MATCH_COLOR } else { 3 };
        styles.push((0, color, index..index + 1));
    }
    if let Some(source) = columns.get(1) {
        styles.push((1, DIM, 0..source.chars().count()));
    }
    if let Some(active) = columns.get(2) {
        if !active.is_empty() {
            styles.push((2, 2, 0..active.chars().count()));
        }
    }
    styles
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Focus {
    Search,
    Filter,
    List,
}

#[derive(Debug, Clone, PartialEq)]
enum NamePurpose {
    New,
    Rename(String),
}

struct NameForm {
    purpose: NamePurpose,
    input: TextInput,
    error: Option<String>,
}

struct ColourForm {
    name: String,
    file: Option<String>,
    base_colours: Vec<String>,
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
    pending_delete: Option<(u64, String)>,
    pending_copy: Option<(u64, String)>,
    pending_file_delete: Option<(u64, String, bool)>,
    follow: Option<String>,
    open_followed: bool,
    arrow_fonts: bool,
    effects: Vec<Effect>,
    notice: Option<String>,
    focused: bool,
    focus: Focus,
    search: TextInput,
    filter: Dropdown,
    scroll: RowScroll,
    new_button: Button,
    config_file_path: Option<String>,
    link_color: Option<PaletteColor>,
    link_area: Option<Rect>,
    link_hovered: bool,
    link_target: Option<String>,
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
            pending_delete: None,
            pending_copy: None,
            pending_file_delete: None,
            follow: None,
            open_followed: false,
            arrow_fonts: true,
            effects: vec![],
            notice: None,
            focused: false,
            focus: Focus::List,
            search: TextInput::empty()
                .placeholder("/ to search themes")
                .search_mode(),
            filter: Dropdown::new(
                Text::new("Source").color_all(0),
                vec![ALL_SOURCES.to_owned()],
            )
            .label_width(SHORT_LABEL_WIDTH)
            .accent_brackets(),
            scroll: RowScroll::default(),
            new_button: Button::new(NEW_THEME).accent_brackets(),
            config_file_path: None,
            link_color: None,
            link_area: None,
            link_hovered: false,
            link_target: None,
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
        self.sync_filter();
        self.selected = self.selected.min(self.visible().len().saturating_sub(1));
        self.merge_file_change();
        if let Some(name) = self.follow.take() {
            if self.themes.iter().any(|theme| theme.name == name) {
                self.select_named(&name);
                if std::mem::take(&mut self.open_followed) {
                    self.open_colours();
                }
            } else {
                self.follow = Some(name);
            }
        }
    }
    fn query(&self) -> String {
        self.search.get_text().trim().to_lowercase()
    }
    fn searching(&self) -> bool {
        !self.query().is_empty()
    }
    fn source_filter(&self) -> Option<ThemeSource> {
        let label = self.filter.selected_value()?;
        SOURCE_ORDER
            .iter()
            .copied()
            .find(|source| source_heading(*source) == label)
    }
    fn sync_filter(&mut self) {
        if self.filter.is_open() {
            return;
        }
        let options: Vec<String> = std::iter::once(ALL_SOURCES)
            .chain(
                SOURCE_ORDER
                    .iter()
                    .copied()
                    .filter(|source| self.themes.iter().any(|theme| theme.source == *source))
                    .map(source_heading),
            )
            .map(|label| label.to_owned())
            .collect();
        if self.filter.options() != options.as_slice() {
            let current = self.filter.selected_value().map(|label| label.to_owned());
            let index = current
                .and_then(|current| options.iter().position(|option| *option == current))
                .unwrap_or(0);
            self.filter.set_options(options);
            self.filter.set_selected(index);
        }
    }
    fn visible(&self) -> Vec<ThemeEntry> {
        let query = self.query();
        if !query.is_empty() {
            let mut found: Vec<(ThemeEntry, (usize, usize))> = self
                .themes
                .iter()
                .filter_map(|theme| match_theme(&query, theme).map(|m| (theme.clone(), m.score)))
                .collect();
            found.sort_by_key(|(_, score)| *score);
            return found.into_iter().map(|(theme, _)| theme).collect();
        }
        let filter = self.source_filter();
        self.themes
            .iter()
            .filter(|theme| filter.map(|filter| filter == theme.source).unwrap_or(true))
            .cloned()
            .collect()
    }
    fn select_first(&mut self) {
        self.selected = 0;
    }
    fn select_named(&mut self, name: &str) {
        if !self.visible().iter().any(|theme| theme.name == name) {
            self.search.clear();
            self.filter.set_selected(0);
        }
        if let Some(index) = self.visible().iter().position(|theme| theme.name == name) {
            self.selected = index;
            self.focus = Focus::List;
            self.scroll.follow();
        }
    }
    fn config_themes(&self) -> Vec<ThemeEntry> {
        self.themes
            .iter()
            .filter(|theme| theme.source == ThemeSource::ConfigFile)
            .cloned()
            .collect()
    }
    fn selected_theme(&self) -> Option<ThemeEntry> {
        self.visible().get(self.selected).cloned()
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
        self.sync_filter();
        self.selected = self.selected.min(self.visible().len().saturating_sub(1));
    }
    pub fn open_colours(&mut self) {
        let Some(theme) = self.selected_theme() else {
            return;
        };
        let file = match theme.source {
            ThemeSource::ConfigFile => None,
            ThemeSource::ThemeFolder if theme.file_path.is_some() => theme.file_path.clone(),
            _ => {
                let name = format!("{}-custom", theme.name);
                self.ask_copy(theme.name, name, None);
                return;
            },
        };
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
            live: theme.name == self.active && file.is_none(),
            name: theme.name,
            file,
            base_colours: theme.colours.clone(),
            inputs,
            buttons: ButtonRow::new(&["Apply", "Fix all low contrast", "Cancel"]),
            scroll: RowScroll::default(),
            previewed: false,
            original_unsaved: self.themes_unsaved,
            original_config_themes: self.config_themes(),
            error: None,
        });
    }
    fn ask_copy(&mut self, from: String, name: String, error: Option<String>) {
        let source = self
            .themes
            .iter()
            .find(|theme| theme.name == from)
            .map(|theme| source_text(theme.source))
            .unwrap_or("built in");
        let question = format!(
            "{} is {} and cannot be changed. Make an editable copy of it as a new file in the theme folder?",
            from,
            match source {
                "built in" => "built in".to_owned(),
                other => format!("from the {}", other),
            }
        );
        let name_end = from.chars().count();
        let mut message = Text::new(&question).color_range(3, 0..name_end);
        if let Some(error) = error {
            let start = question.chars().count() + 1;
            let text = format!("{} {}", question, error);
            let end = text.chars().count();
            message = Text::new(text)
                .color_range(3, 0..name_end)
                .error_color_range(start..end);
        }
        let request_id = prompt(
            PromptRequest::form(vec![FormField::input(COPY_NAME_FIELD, "Name")
                .required()
                .default_value(name)])
            .title("Make an editable copy")
            .message(message)
            .buttons("Make a copy", "Cancel"),
        );
        self.pending_copy = Some((request_id, from));
    }
    fn answer_copy(&mut self, from: String, result: &PromptResult) {
        let PromptResult::Answered(PromptValue::Form(values)) = result else {
            return;
        };
        let name = match values.get(COPY_NAME_FIELD) {
            Some(PromptValue::Text(name)) => name.clone(),
            _ => String::new(),
        };
        if let Err(error) = self.copy_to_file(&from, &name) {
            self.ask_copy(from, name, Some(error));
        }
    }
    fn edited_config_themes(&self, form: &ColourForm) -> Result<Vec<ThemeEntry>, String> {
        let colours = Self::edited_colours(form)?;
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
    fn typed_colours(form: &ColourForm) -> Vec<String> {
        (0..theme_slots().len())
            .map(|index| {
                form.inputs
                    .text_input(&index)
                    .map(|input| input.get_text().trim().to_owned())
                    .unwrap_or_default()
            })
            .collect()
    }
    fn edited_colours(form: &ColourForm) -> Result<Vec<String>, String> {
        let mut colours = vec![];
        for (index, (style, component)) in theme_slots().iter().enumerate() {
            let text = form
                .inputs
                .text_input(&index)
                .map(|input| input.get_text().trim().to_owned())
                .unwrap_or_default();
            if !text.is_empty() && parse_colour(&text).is_none() {
                return Err(format!("{}.{}: {}", style, component, COLOUR_ERROR));
            }
            let normalized = parse_colour(&text)
                .map(|colour| zellij_utils::input::config_blocks::colour_text(&colour))
                .unwrap_or_default();
            colours.push(normalized);
        }
        Ok(colours)
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
            Err(_) => {},
        }
    }
    fn fix_contrast(&mut self, focused_only: bool) {
        let Some(form) = self.colour_form.as_mut() else {
            return;
        };
        let focused = form.inputs.focused_key().copied().unwrap_or(0);
        let contrasts = slot_contrasts(&Self::typed_colours(form));
        let mut fixed = 0;
        for (index, found) in contrasts.iter().enumerate() {
            let Some(found) = found else {
                continue;
            };
            let wanted = if focused_only {
                index == focused && found.ratio < GOOD_CONTRAST
            } else {
                found.ratio < LOW_CONTRAST
            };
            if !wanted {
                continue;
            }
            let better = suggest(found.foreground, found.background, GOOD_CONTRAST);
            if let Some(input) = form.inputs.text_input_mut(&index) {
                input.set_text(colour_text(&better));
                fixed += 1;
            }
        }
        self.notice = Some(match (focused_only, fixed) {
            (true, 0) => "This color already has enough contrast".to_owned(),
            (true, _) => "Raised the contrast of this color".to_owned(),
            (false, 0) => "No color has low contrast".to_owned(),
            (false, 1) => "Raised the contrast of 1 color".to_owned(),
            (false, count) => format!("Raised the contrast of {} colors", count),
        });
        if fixed > 0 {
            self.preview();
        }
    }
    fn apply_colours(&mut self) {
        let Some(form) = self.colour_form.as_ref() else {
            return;
        };
        if form.file.is_some() {
            let name = form.name.clone();
            let written = Self::edited_colours(form)
                .and_then(|colours| write_theme_file(&name, None, &colours));
            match written {
                Ok(path) => {
                    self.colour_form = None;
                    self.effects.push(Effect::Refresh);
                    self.notice = Some(format!("Colors of {} saved to {}", name, path));
                },
                Err(error) => {
                    if let Some(form) = self.colour_form.as_mut() {
                        form.error = Some(error);
                    }
                },
            }
            return;
        }
        match self.edited_config_themes(form) {
            Ok(themes) => {
                let name = form.name.clone();
                self.colour_form = None;
                self.apply_config_themes(themes);
                self.notice = Some(format!("Colors of {} applied", name));
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
        let purpose = form.purpose.clone();
        let text = form.input.get_text().to_owned();
        if let Err(error) = self.commit_name(purpose, &text) {
            if let Some(form) = self.name_form.as_mut() {
                form.error = Some(error);
            }
        }
    }
    fn copy_to_file(&mut self, from: &str, text: &str) -> Result<(), String> {
        let name = check_name(text, "theme name")?;
        if self.themes.iter().any(|theme| theme.name == name) {
            return Err(format!("A theme called {} already exists", name));
        }
        let path = write_theme_file(&name, Some(from), &[])?;
        self.effects.push(Effect::Refresh);
        self.notice = Some(format!(
            "{} is an editable copy of {} in {}",
            name, from, path
        ));
        self.follow = Some(name);
        self.open_followed = true;
        Ok(())
    }
    fn commit_name(&mut self, purpose: NamePurpose, text: &str) -> Result<(), String> {
        let name = check_name(text, "theme name")?;
        let renaming_same = matches!(&purpose, NamePurpose::Rename(from) if *from == name);
        if !renaming_same && self.themes.iter().any(|theme| theme.name == name) {
            return Err(format!("A theme called {} already exists", name));
        }
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
                    file_path: None,
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
        self.select_named(&name);
        Ok(())
    }
    fn request_delete(&mut self) {
        let Some(theme) = self.selected_theme() else {
            return;
        };
        if theme.source == ThemeSource::ThemeFolder {
            self.ask_delete_file_theme(theme);
            return;
        }
        if theme.source != ThemeSource::ConfigFile {
            self.notice = Some(format!(
                "{} comes from the {} and cannot be deleted here",
                theme.name,
                source_text(theme.source)
            ));
            return;
        }
        if !crate::page::confirm_removals() {
            self.delete_theme(&theme.name);
            return;
        }
        let name_start = "Delete the theme ".chars().count();
        let name_end = name_start + theme.name.chars().count();
        let request_id = prompt(crate::page::removal_prompt(
            Text::new(format!("Delete the theme {}?", theme.name))
                .color_range(3, name_start..name_end),
        ));
        self.pending_delete = Some((request_id, theme.name));
    }
    fn ask_delete_file_theme(&mut self, theme: ThemeEntry) {
        let Some(path) = theme.file_path.clone() else {
            self.notice = Some(format!(
                "The file that defines {} could not be found",
                theme.name
            ));
            return;
        };
        let others = self
            .themes
            .iter()
            .filter(|other| other.name != theme.name && other.file_path.as_ref() == Some(&path))
            .count();
        let question = if others == 0 {
            format!(
                "Delete the theme {}? Its file {} will be deleted.",
                theme.name, path
            )
        } else {
            format!(
                "Delete the theme {} from {}? The other {} in that file stay{}.",
                theme.name,
                path,
                if others == 1 { "theme" } else { "themes" },
                if others == 1 { "s" } else { "" }
            )
        };
        let name_start = "Delete the theme ".chars().count();
        let name_end = name_start + theme.name.chars().count();
        let request_id = prompt(
            PromptRequest::confirm(Text::new(question).color_range(3, name_start..name_end))
                .title("Delete theme")
                .yes("Delete")
                .no("Cancel"),
        );
        self.pending_file_delete = Some((request_id, theme.name, others == 0));
    }
    fn answer_file_delete(&mut self, name: String, deletes_file: bool, result: &PromptResult) {
        if *result != PromptResult::Confirmed(true) {
            return;
        }
        match delete_theme_file(&name) {
            Ok(path) => {
                self.effects.push(Effect::Refresh);
                self.notice = Some(if deletes_file {
                    format!("Deleted {} and its file {}", name, path)
                } else {
                    format!("Deleted {} from {}", name, path)
                });
            },
            Err(error) => self.notice = Some(error),
        }
    }
    fn merge_file_change(&mut self) {
        let Some(form) = self.colour_form.as_mut() else {
            return;
        };
        if form.file.is_none() {
            return;
        }
        match self.themes.iter().find(|theme| theme.name == form.name) {
            Some(theme) if theme.colours != form.base_colours => {
                for (index, colour) in theme.colours.iter().enumerate() {
                    let base = form.base_colours.get(index).cloned().unwrap_or_default();
                    if let Some(input) = form.inputs.text_input_mut(&index) {
                        if input.get_text().trim() == base {
                            input.set_text(colour.clone());
                        }
                    }
                }
                form.base_colours = theme.colours.clone();
                form.file = theme.file_path.clone().or(form.file.take());
                self.notice = Some(format!(
                    "The file of {} changed; its new colors are shown",
                    form.name
                ));
            },
            Some(_) => {},
            None => {
                form.error = Some(format!(
                    "{} is no longer defined in its file; Apply will fail",
                    form.name
                ));
            },
        }
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
        if key.is_key_with_ctrl_modifier(BareKey::Char('a')) {
            self.apply_colours();
            return;
        }
        if key.is_key_with_ctrl_modifier(BareKey::Char('f')) {
            self.fix_contrast(true);
            return;
        }
        if key.is_key_with_ctrl_modifier(BareKey::Char('e')) {
            self.fix_contrast(false);
            return;
        }
        if is_plain(key, BareKey::Down) || is_plain(key, BareKey::Enter) {
            form.inputs.focus_next();
            return;
        }
        if is_plain(key, BareKey::Up) {
            form.inputs.focus_prev();
            return;
        }
        match form.inputs.handle_key(key) {
            FocusEvent::Element {
                response: UiResponse::Changed(_),
                ..
            } => {
                if let Some(form) = self.colour_form.as_mut() {
                    form.error = None;
                }
                self.preview();
            },
            FocusEvent::Element {
                response: UiResponse::Cancelled,
                ..
            } => self.cancel_colours(),
            _ => {},
        }
    }
    fn handle_list_key(&mut self, key: &KeyWithModifier) -> PageResponse {
        self.scroll.follow();
        let count = self.visible().len();
        if is_plain(key, BareKey::Down) {
            if self.selected + 1 < count {
                self.selected += 1;
            }
        } else if is_plain(key, BareKey::Up) {
            if self.selected == 0 {
                self.focus = if self.searching() {
                    Focus::Search
                } else {
                    Focus::Filter
                };
            } else {
                self.selected -= 1;
            }
        } else if is_plain(key, BareKey::PageDown) {
            self.selected = (self.selected + 10).min(count.saturating_sub(1));
        } else if is_plain(key, BareKey::PageUp) {
            self.selected = self.selected.saturating_sub(10);
        } else if typed(key, '/') {
            self.focus = Focus::Search;
        } else if is_plain(key, BareKey::Enter) {
            self.open_colours();
        } else if typed(key, 'a') {
            self.open_name_form(NamePurpose::New);
        } else if typed(key, 'n') {
            self.request_rename();
        } else if is_plain(key, BareKey::Delete) {
            self.request_delete();
        } else if typed(key, 'r') {
            self.revert_selected();
        } else if is_plain(key, BareKey::Esc) {
            if self.searching() {
                self.search.clear();
                self.select_first();
                self.scroll.reset();
            } else {
                return PageResponse::Close;
            }
        } else if is_plain(key, BareKey::Tab) || is_plain(key, BareKey::Left) || is_shift_tab(key) {
            return PageResponse::LeaveToMenu;
        } else {
            return PageResponse::NotHandled;
        }
        PageResponse::Handled
    }
    fn handle_filter_key(&mut self, key: &KeyWithModifier) -> PageResponse {
        if self.filter.is_open() {
            if let UiResponse::Changed(_) = self.filter.handle_key(key) {
                self.select_first();
                self.scroll.reset();
            }
            return PageResponse::Handled;
        }
        if is_plain(key, BareKey::Down) || is_plain(key, BareKey::Tab) {
            self.focus = Focus::List;
            self.scroll.follow();
            return PageResponse::Handled;
        }
        if is_plain(key, BareKey::Up) || is_shift_tab(key) || typed(key, '/') {
            self.focus = Focus::Search;
            return PageResponse::Handled;
        }
        if is_plain(key, BareKey::Left) {
            return PageResponse::LeaveToMenu;
        }
        if is_plain(key, BareKey::Esc) {
            return PageResponse::Close;
        }
        if typed(key, 'a') {
            self.open_name_form(NamePurpose::New);
            return PageResponse::Handled;
        }
        match self.filter.handle_key(key) {
            UiResponse::Changed(_) => {
                self.select_first();
                self.scroll.reset();
                PageResponse::Handled
            },
            UiResponse::NotHandled => PageResponse::NotHandled,
            _ => PageResponse::Handled,
        }
    }
    fn handle_search_key(&mut self, key: &KeyWithModifier) -> PageResponse {
        if is_plain(key, BareKey::Enter) || is_plain(key, BareKey::Down) || is_plain(key, BareKey::Tab)
        {
            if self.searching() {
                self.focus = Focus::List;
                self.select_first();
                self.scroll.follow();
            } else {
                self.focus = Focus::Filter;
            }
            return PageResponse::Handled;
        }
        if is_plain(key, BareKey::Up) || is_shift_tab(key) {
            return PageResponse::LeaveUp;
        }
        if is_plain(key, BareKey::Esc) {
            if self.searching() {
                self.search.clear();
                self.select_first();
                self.scroll.reset();
                return PageResponse::Handled;
            }
            return PageResponse::Close;
        }
        match self.search.handle_key(key) {
            UiResponse::Changed(_) => {
                self.select_first();
                self.scroll.reset();
                PageResponse::Handled
            },
            UiResponse::NotHandled => PageResponse::NotHandled,
            _ => PageResponse::Handled,
        }
    }
}

impl ThemesScreen {
    fn handle_colour_mouse(&mut self, mouse: Mouse) -> bool {
        let Some(form) = self.colour_form.as_mut() else {
            return false;
        };
        if form.scroll.handle_wheel(&mouse).is_some() {
            return true;
        }
        let (activated, buttons_changed) = form.buttons.handle_mouse_with_hover(mouse);
        match activated {
            Some(0) => {
                self.apply_colours();
                return true;
            },
            Some(1) => {
                self.fix_contrast(false);
                return true;
            },
            Some(_) => {
                self.cancel_colours();
                return true;
            },
            None => {},
        }
        let inputs_changed = form.inputs.handle_mouse(mouse).is_handled();
        buttons_changed || inputs_changed || !matches!(mouse, Mouse::Hover(..))
    }
    fn handle_name_mouse(&mut self, mouse: Mouse) -> bool {
        let Some(form) = self.name_form.as_mut() else {
            return false;
        };
        let input_changed = form.input.handle_mouse(mouse).is_handled();
        let (activated, buttons_changed) = self.form_buttons.handle_mouse_with_hover(mouse);
        match activated {
            Some(0) => self.apply_name(),
            Some(_) => self.name_form = None,
            None => {},
        }
        activated.is_some()
            || input_changed
            || buttons_changed
            || !matches!(mouse, Mouse::Hover(..))
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
        let is_hover = matches!(mouse, Mouse::Hover(..));
        let mut hover_changed = false;
        if self.filter.is_open() || matches!(mouse, Mouse::LeftClick(..) | Mouse::Hover(..)) {
            let (response, filter_changed) =
                changed_by(&mut self.filter, |filter| filter.handle_mouse(mouse));
            hover_changed |= filter_changed;
            if let UiResponse::Changed(_) = response {
                self.select_first();
                self.scroll.reset();
            }
            if response.is_handled() && !is_hover {
                self.focus = Focus::Filter;
                return PageResponse::Handled;
            }
            if self.filter.is_open() {
                return if !is_hover || filter_changed {
                    PageResponse::Handled
                } else {
                    PageResponse::NotHandled
                };
            }
        }
        if let Some((line, column)) = is_click(&mouse) {
            if self.link_at(line, column) {
                self.open_config_file();
                return PageResponse::Handled;
            }
        }
        if let Mouse::Hover(line, column) = mouse {
            let hovered = !under_overlay(line, column) && self.link_at(line, column);
            hover_changed |= hovered != self.link_hovered;
            self.link_hovered = hovered;
        }
        let was_hovered = self.new_button.is_hovered();
        if matches!(
            self.new_button.handle_mouse(outside_overlays(mouse)),
            UiResponse::Activated
        ) {
            self.open_name_form(NamePurpose::New);
            return PageResponse::Handled;
        }
        hover_changed |= was_hovered != self.new_button.is_hovered();
        let rows_hover = self.scroll.hover(&mouse);
        let search_response = self.search.handle_mouse(mouse);
        if is_hover && search_response.is_handled() {
            hover_changed = true;
        }
        if let Some(rows_changed) = rows_hover {
            return if rows_changed || hover_changed {
                PageResponse::Handled
            } else {
                PageResponse::NotHandled
            };
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
        if self.search.hit_test(line, column) {
            self.focus = Focus::Search;
            return PageResponse::Handled;
        }
        match self.scroll.row_at(line, column) {
            Some(row) if self.focused && self.focus == Focus::List && row == self.selected => {
                self.open_colours();
                PageResponse::Handled
            },
            Some(row) => {
                self.selected = row;
                self.focus = Focus::List;
                PageResponse::Handled
            },
            None => PageResponse::NotHandled,
        }
    }
    fn link_at(&self, line: isize, column: usize) -> bool {
        self.link_area
            .map(|area| area.contains(line, column))
            .unwrap_or(false)
    }
    fn open_config_file(&mut self) {
        if let Some(path) = self.link_target.clone() {
            open_file_floating(FileToOpen::new(path), None, BTreeMap::new());
        }
    }
    fn render_path(&mut self, x: usize, y: usize, width: usize) {
        self.link_area = None;
        self.link_target = None;
        let target = match self.selected_theme() {
            Some(theme) if theme.source == ThemeSource::ConfigFile => {
                Some(self.config_file_path.clone())
            },
            Some(theme) if theme.source == ThemeSource::ThemeFolder => Some(theme.file_path),
            _ => None,
        };
        let Some(target) = target else {
            self.link_hovered = false;
            return;
        };
        let prefix_width = PATH_PREFIX.chars().count();
        match target {
            Some(path) if width > prefix_width => {
                self.link_target = Some(path.clone());
                print_dim(PATH_PREFIX, x, y, width);
                let link = truncate(&path, width - prefix_width);
                let link_x = x + prefix_width;
                self.link_area = Some(Rect::new(link_x, y, link.chars().count(), 1));
                print_link(&link, link_x, y, self.link_color, self.link_hovered);
            },
            Some(_) => {},
            None => print_dim("Defined in a file that could not be found", x, y, width),
        }
    }
    fn render_list(&mut self, x: usize, y: usize, width: usize, height: usize) {
        print_text_with_coordinates(
            Text::new(truncate(TITLE, width)).color_all(2),
            x,
            y,
            None,
            None,
        );
        let search_x = TITLE.chars().count() + 2;
        let search_end = BESIDE_SHORT_FIELD + self.new_button.natural_width();
        let search_width = search_end.min(width).saturating_sub(search_x);
        let search_focused = self.focused && self.focus == Focus::Search;
        self.search.set_focused(search_focused);
        self.search.set_show_cursor(search_focused);
        self.search.render(x + search_x, y, search_width);
        let searching = self.searching();
        let query = self.query();
        self.filter
            .set_focused(self.focused && self.focus == Focus::Filter);
        self.sync_filter();
        let list_offset = if searching {
            self.filter.clear_area();
            self.new_button.clear_area();
            2
        } else {
            self.filter.render(x, y + 2, SHORT_FIELD_WIDTH.min(width));
            if BESIDE_SHORT_FIELD + self.new_button.natural_width() <= width {
                self.new_button.render(x + BESIDE_SHORT_FIELD, y + 2);
            } else {
                self.new_button.clear_area();
            }
            4
        };
        let entries = self.visible();
        let list_y = y + list_offset;
        let list_height = height.saturating_sub(list_offset + 2).max(1);
        self.render_path(x, list_y + list_height + 1, width);
        let cursor_shown = self.focused && self.focus == Focus::List;
        let rows: Vec<(Vec<String>, String, Vec<ColumnStyle>)> = entries
            .iter()
            .map(|theme| {
                let columns = theme_columns(theme, &self.active);
                let found = if searching {
                    match_theme(&query, theme)
                } else {
                    None
                };
                let styles = theme_styles(&columns, found.as_ref());
                (columns, markers(self.is_unsaved(theme), false, false), styles)
            })
            .collect();
        let natural = ColumnLayout::new(
            rows.iter()
                .map(|(columns, marker, _)| (columns.as_slice(), marker.as_str())),
            usize::MAX,
        )
        .with_indent(0);
        let content_width = natural.natural_width("● unsaved".chars().count());
        let list_width = RowScroll::fitted_width(rows.len(), list_height, content_width, width);
        let visible = self.scroll.layout(
            x,
            list_y,
            list_width,
            list_height,
            rows.len(),
            Some(self.selected),
        );
        if entries.is_empty() {
            let empty = if searching {
                "No matching themes"
            } else {
                "No themes · a: add a theme"
            };
            print_dim(empty, x, list_y, width);
            return;
        }
        let row_width = self.scroll.row_width();
        let layout = ColumnLayout::new(
            rows.iter()
                .map(|(columns, marker, _)| (columns.as_slice(), marker.as_str())),
            row_width,
        )
        .with_indent(0);
        for index in visible {
            let Some(screen_y) = self.scroll.screen_row(index) else {
                continue;
            };
            let (columns, marker, styles) = &rows[index];
            let look = RowLook::new(
                cursor_shown && index == self.selected,
                self.scroll.is_hovered(index),
            );
            layout.print_styled(columns, marker, x, screen_y, row_width, look, styles);
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
        self.config_file_path = snapshot.config_file_path.clone();
        self.set_themes(
            snapshot.blocks.themes.clone(),
            snapshot.saved_blocks.themes.clone(),
            active,
            unsaved,
        );
    }
    fn set_mode_info(&mut self, mode_info: &ModeInfo) {
        self.link_color = Some(mode_info.style.colors.text_unselected.emphasis_2);
        self.arrow_fonts = !mode_info.capabilities.arrow_fonts;
    }
    fn handle_key(&mut self, key: &KeyWithModifier) -> PageResponse {
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
        match self.focus {
            Focus::Search => self.handle_search_key(key),
            Focus::Filter => self.handle_filter_key(key),
            Focus::List => self.handle_list_key(key),
        }
    }
    fn render(&mut self, x: usize, y: usize, width: usize, height: usize) {
        if let Some(form) = self.name_form.as_mut() {
            let title = match &form.purpose {
                NamePurpose::New => "New theme".to_owned(),
                NamePurpose::Rename(from) => format!("Rename {}", from),
            };
            print_heading(&title, x, y, width);
            self.scroll.clear();
            self.search.clear_area();
            self.filter.clear_area();
            self.new_button.clear_area();
            self.link_area = None;
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
                format!("Colors of {} (active: changes preview live)", form.name)
            } else if form.file.is_some() {
                format!("Colors of {} (Apply saves them to its file)", form.name)
            } else {
                format!("Colors of {}", form.name)
            };
            print_heading(&title, x, y, width);
            for (line, help) in COLOUR_HELP.iter().enumerate() {
                print_dim(help, x, y + 1 + line, width);
            }
            self.scroll.clear();
            self.search.clear_area();
            self.filter.clear_area();
            self.new_button.clear_area();
            self.link_area = None;
            form.inputs.clear_areas();
            let list_y = y + 2 + COLOUR_HELP.len();
            let rows = height.saturating_sub(list_y - y + 5).max(1);
            let contrasts = slot_contrasts(&Self::typed_colours(form));
            let focused = form.inputs.focused_key().copied().unwrap_or(0);
            let slots = theme_slots();
            let list_width = RowScroll::fitted_width(slots.len(), rows, LIST_WIDTH, width);
            let visible = form
                .scroll
                .layout(x, list_y, list_width, rows, slots.len(), Some(focused));
            let sample_x = x + list_width + 3;
            let sample_fits = width >= list_width + 3 + SAMPLE_MIN_WIDTH;
            if sample_fits {
                let colours = Self::typed_colours(form);
                let focused_style = slots.get(focused).map(|(style, _)| *style);
                render_sample(
                    &colours,
                    focused_style,
                    self.arrow_fonts,
                    sample_x,
                    list_y,
                    (x + width).saturating_sub(sample_x),
                    rows,
                );
            }
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
                    let ratio_x = x + SLOT_LABEL_WIDTH + VALUE_WIDTH + 5;
                    let invalid = !value.trim().is_empty() && parse_colour(&value).is_none();
                    let ratio = if invalid {
                        Some(Text::new("invalid").error_color_all())
                    } else {
                        contrasts.get(index).cloned().flatten().map(|found| {
                            let label = format!("{:.1}:1", found.ratio);
                            if found.ratio < LOW_CONTRAST {
                                Text::new(label).error_color_all()
                            } else if found.ratio < GOOD_CONTRAST {
                                Text::new(label).color_all(3)
                            } else {
                                Text::new(label).dim_all()
                            }
                        })
                    };
                    if let Some(ratio) = ratio {
                        print_text_with_coordinates(ratio, ratio_x, row, None, None);
                    }
                }
            }
            let advice_y = list_y + rows + 1;
            if let Some(advice) = contrast_advice(&slots, &contrasts, focused, width) {
                print_text_with_coordinates(
                    advice,
                    x,
                    advice_y,
                    None,
                    None,
                );
            }
            form.buttons.render(x, advice_y + 2, width);
            if let Some(error) = &form.error {
                print_text_with_coordinates(
                    Text::new(truncate(error, width)).error_color_all(),
                    x,
                    advice_y + 3,
                    None,
                    None,
                );
            } else if !sample_fits {
                print_dim(
                    "Widen the pane to see a preview of these colors",
                    x,
                    advice_y + 3,
                    width,
                );
            }
            return;
        }
        self.render_list(x, y, width, height);
    }
    fn render_overlays(&mut self, rows: usize, cols: usize) {
        if self.colour_form.is_none() && self.name_form.is_none() && self.filter.is_open() {
            self.filter.render_overlay(rows, cols);
            note_dropdown(&self.filter);
        }
    }
    fn handle_mouse(&mut self, mouse: Mouse) -> PageResponse {
        let changed = if self.colour_form.is_some() {
            self.handle_colour_mouse(mouse)
        } else if self.name_form.is_some() {
            self.handle_name_mouse(mouse)
        } else {
            return self.handle_list_mouse(mouse);
        };
        if changed {
            PageResponse::Handled
        } else {
            PageResponse::NotHandled
        }
    }
    fn handle_timer(&mut self) -> bool {
        let form_changed = self
            .colour_form
            .as_mut()
            .map(|form| form.buttons.handle_timer())
            .unwrap_or(false);
        self.new_button.handle_timer() | self.form_buttons.handle_timer() | form_changed
    }
    fn full_screen(&self) -> bool {
        self.colour_form.is_some()
    }
    fn header_hints(&self) -> Option<Vec<(&'static str, &'static str)>> {
        self.colour_form
            .as_ref()
            .map(|_| vec![("<Ctrl a>", "apply colors"), ("<Esc>", "back to configuration")])
    }
    fn header_file(&self) -> Option<String> {
        self.colour_form.as_ref().and_then(|form| form.file.clone())
    }
    fn window_title(&self) -> Option<String> {
        self.colour_form
            .as_ref()
            .map(|form| format!("Edit {}", form.name))
    }
    fn captures_keys(&self) -> bool {
        self.colour_form.is_some()
            || self.name_form.is_some()
            || self.filter.is_open()
            || self.focus == Focus::Search
    }
    fn hints(&self) -> Vec<(&'static str, &'static str)> {
        if self.colour_form.is_some() {
            return vec![
                ("<Esc>", "back to configuration"),
                ("<Enter/↓↑>", "next / previous color"),
                ("<Ctrl a>", "apply"),
                ("<Ctrl f>", "fix contrast"),
                ("<Ctrl e>", "fix all low contrast"),
                ("<#rrggbb | rrggbb | r g b | 0-255>", "color formats"),
            ];
        }
        if self.name_form.is_some() {
            return vec![("<Enter>", "confirm"), ("<Esc>", "cancel")];
        }
        match self.focus {
            Focus::Search => vec![
                ("<type>", "search themes"),
                ("<↓>", "results"),
                ("<Esc>", "clear"),
            ],
            Focus::Filter => vec![
                ("<Space>", "choose source"),
                ("<↓>", "themes"),
                ("<Esc>", "close"),
            ],
            Focus::List => vec![
                ("<a>", "add"),
                ("<Enter>", "colors"),
                ("<n>", "rename"),
                ("<Del>", "delete"),
                ("<r>", "revert"),
                ("</>", "search"),
            ],
        }
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
        self.pending_delete = None;
        self.pending_copy = None;
        self.pending_file_delete = None;
    }
    fn set_focused(&mut self, focused: bool) {
        self.focused = focused;
    }
    fn prompt_result(&mut self, request_id: u64, result: &PromptResult) -> bool {
        match self.pending_copy.take() {
            Some((pending_id, from)) if pending_id == request_id => {
                self.answer_copy(from, result);
                return true;
            },
            other => self.pending_copy = other,
        }
        match self.pending_file_delete.take() {
            Some((pending_id, name, deletes_file)) if pending_id == request_id => {
                self.answer_file_delete(name, deletes_file, result);
                return true;
            },
            other => self.pending_file_delete = other,
        }
        match self.pending_delete.take() {
            Some((pending_id, name)) if pending_id == request_id => {
                if crate::page::removal_confirmed(result) {
                    self.delete_theme(&name);
                }
                true
            },
            other => {
                self.pending_delete = other;
                false
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap as Map;

    fn theme(name: &str, source: ThemeSource, first: &str) -> ThemeEntry {
        let mut colours = styling_colours(&DEFAULT_STYLES);
        colours[0] = first.to_owned();
        ThemeEntry {
            name: name.to_owned(),
            source,
            colours,
            file_path: None,
        }
    }

    fn folder_theme(name: &str, path: &str) -> ThemeEntry {
        ThemeEntry {
            file_path: Some(path.to_owned()),
            ..theme(name, ThemeSource::ThemeFolder, "#405060")
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

    fn screen_with_folder(themes: Vec<ThemeEntry>) -> ThemesScreen {
        let mut screen = ThemesScreen::default();
        screen.set_themes(themes, vec![], "default".to_owned(), false);
        screen
    }

    fn key(bare_key: BareKey) -> KeyWithModifier {
        KeyWithModifier::new(bare_key)
    }

    fn ctrl(character: char) -> KeyWithModifier {
        KeyWithModifier::new(BareKey::Char(character)).with_ctrl_modifier()
    }

    fn type_text(screen: &mut ThemesScreen, text: &str) {
        for character in text.chars() {
            screen.handle_key(&key(BareKey::Char(character)));
        }
    }

    fn slot_index(style: &str, component: &str) -> usize {
        theme_slots()
            .iter()
            .position(|slot| *slot == (style, component))
            .unwrap()
    }

    fn input_text(screen: &ThemesScreen, index: usize) -> String {
        screen
            .colour_form
            .as_ref()
            .and_then(|form| form.inputs.text_input(&index))
            .map(|input| input.get_text().to_owned())
            .unwrap()
    }

    fn focused_slot(screen: &ThemesScreen) -> usize {
        screen
            .colour_form
            .as_ref()
            .and_then(|form| form.inputs.focused_key().copied())
            .unwrap()
    }

    fn name_answer(name: &str) -> PromptResult {
        let mut values = Map::new();
        values.insert(COPY_NAME_FIELD.to_owned(), PromptValue::Text(name.to_owned()));
        PromptResult::Answered(PromptValue::Form(values))
    }

    #[test]
    fn the_list_shows_where_each_theme_comes_from() {
        let screen = screen();
        assert_eq!(screen.themes[0].name, "mine");
        assert_eq!(
            theme_columns(&screen.themes[0], &screen.active),
            vec!["mine", "config file", "active"]
        );
        assert_eq!(
            theme_columns(&screen.themes[1], &screen.active),
            vec!["dracula", "built in", ""]
        );
    }

    #[test]
    fn searching_and_filtering_narrow_the_list() {
        let mut screen = screen();
        screen.handle_key(&key(BareKey::Char('/')));
        type_text(&mut screen, "drc");
        let names: Vec<String> = screen.visible().into_iter().map(|t| t.name).collect();
        assert_eq!(names, vec!["dracula"]);
        screen.handle_key(&key(BareKey::Esc));
        assert_eq!(screen.visible().len(), 2);
        let built_in = screen
            .filter
            .options()
            .iter()
            .position(|option| option == "Built in")
            .unwrap();
        screen.filter.set_selected(built_in);
        let names: Vec<String> = screen.visible().into_iter().map(|t| t.name).collect();
        assert_eq!(names, vec!["dracula"]);
    }

    #[test]
    fn enter_on_a_built_in_theme_asks_to_make_a_copy() {
        let mut screen = screen();
        screen.handle_key(&key(BareKey::Down));
        screen.handle_key(&key(BareKey::Enter));
        assert!(screen.colour_form.is_none());
        assert_eq!(
            screen.pending_copy.as_ref().map(|(_, from)| from.as_str()),
            Some("dracula")
        );
    }

    #[test]
    fn a_copy_with_a_taken_or_empty_name_asks_again() {
        let mut screen = screen();
        screen.handle_key(&key(BareKey::Down));
        screen.handle_key(&key(BareKey::Enter));
        let (first_request, _) = screen.pending_copy.clone().unwrap();
        assert!(screen.prompt_result(first_request, &name_answer("mine")));
        let (second_request, from) = screen.pending_copy.clone().unwrap();
        assert_ne!(first_request, second_request);
        assert_eq!(from, "dracula");
        assert!(screen.prompt_result(second_request, &name_answer("  ")));
        assert!(screen.pending_copy.is_some());
        assert!(screen.take_effects().is_empty());
    }

    #[test]
    fn cancelling_the_copy_popup_does_nothing() {
        let mut screen = screen();
        screen.handle_key(&key(BareKey::Down));
        screen.handle_key(&key(BareKey::Enter));
        let (request, _) = screen.pending_copy.clone().unwrap();
        assert!(screen.prompt_result(request, &PromptResult::Cancelled));
        assert!(screen.pending_copy.is_none());
        assert!(screen.take_effects().is_empty());
    }

    #[test]
    fn editing_the_active_theme_previews_and_cancelling_restores() {
        let mut screen = screen();
        screen.handle_key(&key(BareKey::Enter));
        assert!(screen.colour_form.is_some());
        screen.handle_key(&key(BareKey::End));
        screen.handle_key(&key(BareKey::Backspace));
        type_text(&mut screen, "1");
        let effects = screen.take_effects();
        assert!(!effects.is_empty());
        assert!(
            matches!(effects.last(), Some(Effect::ReplaceBlocks(kdl)) if kdl.contains("16 32 49"))
        );
        screen.handle_key(&key(BareKey::Esc));
        assert!(screen.colour_form.is_none());
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
    fn enter_moves_to_the_next_color_and_ctrl_a_applies() {
        let mut screen = screen();
        screen.handle_key(&key(BareKey::Enter));
        assert_eq!(focused_slot(&screen), 0);
        screen.handle_key(&key(BareKey::Enter));
        assert_eq!(focused_slot(&screen), 1);
        assert!(screen.colour_form.is_some());
        screen.handle_key(&key(BareKey::Up));
        screen.handle_key(&key(BareKey::End));
        screen.handle_key(&key(BareKey::Backspace));
        type_text(&mut screen, "1");
        screen.take_effects();
        screen.handle_key(&ctrl('a'));
        assert!(screen.colour_form.is_none());
        assert!(matches!(
            screen.take_effects().last(),
            Some(Effect::ReplaceBlocks(kdl)) if kdl.contains("16 32 49")
        ));
    }

    #[test]
    fn an_invalid_color_is_reported_only_when_applying() {
        let mut screen = screen();
        screen.handle_key(&key(BareKey::Enter));
        type_text(&mut screen, "zz");
        assert!(screen.colour_form.as_ref().unwrap().error.is_none());
        screen.take_effects();
        screen.handle_key(&ctrl('a'));
        let form = screen.colour_form.as_ref().unwrap();
        assert!(form.error.as_ref().unwrap().contains("#rrggbb"));
        assert!(screen.take_effects().is_empty());
    }

    #[test]
    fn a_theme_folder_theme_opens_full_screen_with_its_own_file() {
        let mut screen = screen_with_folder(vec![folder_theme("ocean", "/themes/ocean.kdl")]);
        screen.handle_key(&key(BareKey::Enter));
        let form = screen.colour_form.as_ref().unwrap();
        assert!(!form.live);
        assert_eq!(form.file.as_deref(), Some("/themes/ocean.kdl"));
        assert!(screen.full_screen());
        assert_eq!(screen.window_title().as_deref(), Some("Edit ocean"));
        assert_eq!(screen.header_file().as_deref(), Some("/themes/ocean.kdl"));
        assert!(screen.header_hints().is_some());
        assert!(screen.captures_keys());
    }

    #[test]
    fn a_changed_theme_file_updates_the_colors_that_were_not_edited() {
        let mut screen = screen_with_folder(vec![folder_theme("ocean", "/themes/ocean.kdl")]);
        screen.handle_key(&key(BareKey::Enter));
        screen.handle_key(&key(BareKey::Down));
        screen.handle_key(&key(BareKey::End));
        for _ in 0..10 {
            screen.handle_key(&key(BareKey::Backspace));
        }
        type_text(&mut screen, "#111111");
        let mut changed = folder_theme("ocean", "/themes/ocean.kdl");
        changed.colours[0] = "#abcdef".to_owned();
        changed.colours[1] = "#fedcba".to_owned();
        screen.set_themes(vec![changed], vec![], "default".to_owned(), false);
        assert_eq!(input_text(&screen, 0), "#abcdef");
        assert_eq!(input_text(&screen, 1), "#111111");
        assert!(screen.take_notice().unwrap().contains("changed"));
        screen.set_themes(vec![], vec![], "default".to_owned(), false);
        assert!(screen.colour_form.as_ref().unwrap().error.is_some());
    }

    #[test]
    fn deleting_a_theme_folder_theme_asks_first() {
        let mut screen = screen_with_folder(vec![folder_theme("ocean", "/themes/ocean.kdl")]);
        screen.handle_key(&key(BareKey::Delete));
        let (request, name, deletes_file) = screen.pending_file_delete.clone().unwrap();
        assert_eq!(name, "ocean");
        assert!(deletes_file);
        assert!(screen.prompt_result(request, &PromptResult::Confirmed(false)));
        assert!(screen.pending_file_delete.is_none());
        assert!(screen.take_effects().is_empty());
        let mut screen = screen_with_folder(vec![
            folder_theme("ocean", "/themes/both.kdl"),
            folder_theme("sea", "/themes/both.kdl"),
        ]);
        screen.handle_key(&key(BareKey::Delete));
        let (_, _, deletes_file) = screen.pending_file_delete.clone().unwrap();
        assert!(!deletes_file);
    }

    #[test]
    fn a_built_in_theme_cannot_be_deleted() {
        let mut screen = screen();
        screen.handle_key(&key(BareKey::Down));
        screen.handle_key(&key(BareKey::Delete));
        assert!(screen.pending_file_delete.is_none());
        assert!(screen.take_notice().unwrap().contains("cannot be deleted"));
    }

    fn low_contrast_screen() -> ThemesScreen {
        let mut low = theme("low", ThemeSource::ConfigFile, "#000000");
        low.colours[slot_index("text_unselected", "background")] = "#000000".to_owned();
        low.colours[slot_index("text_unselected", "emphasis_0")] = "#050505".to_owned();
        let mut screen = ThemesScreen::default();
        screen.set_themes(vec![low.clone()], vec![low], "other".to_owned(), false);
        screen.handle_key(&key(BareKey::Enter));
        screen
    }

    #[test]
    fn ctrl_f_raises_the_contrast_of_the_focused_color() {
        let mut screen = low_contrast_screen();
        screen.handle_key(&ctrl('f'));
        let fixed = parse_colour(&input_text(&screen, 0)).unwrap();
        assert!(crate::contrast::contrast(fixed, PaletteColor::Rgb((0, 0, 0))) >= GOOD_CONTRAST);
        assert_eq!(
            input_text(&screen, slot_index("text_unselected", "emphasis_0")),
            "#050505"
        );
    }

    #[test]
    fn ctrl_e_raises_every_low_contrast_color() {
        let mut screen = low_contrast_screen();
        screen.handle_key(&ctrl('e'));
        let form = screen.colour_form.as_ref().unwrap();
        let contrasts = slot_contrasts(&ThemesScreen::typed_colours(form));
        assert!(contrasts
            .iter()
            .flatten()
            .all(|found| found.ratio >= LOW_CONTRAST));
        assert!(screen.take_notice().unwrap().starts_with("Raised the contrast"));
    }

    #[test]
    fn the_contrast_advice_names_the_background_and_a_suggestion() {
        let screen = low_contrast_screen();
        let form = screen.colour_form.as_ref().unwrap();
        let contrasts = slot_contrasts(&ThemesScreen::typed_colours(form));
        let advice = contrast_advice(&theme_slots(), &contrasts, 0, 200).unwrap();
        let text = format!("{:?}", advice);
        assert!(text.contains("text_unselected.background"));
        assert!(text.contains("Ctrl f"));
        assert!(text.contains("Ctrl e"));
    }
}
