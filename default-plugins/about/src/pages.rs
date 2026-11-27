use zellij_tile::prelude::*;

use std::cell::RefCell;
use std::rc::Rc;

use crate::active_component::{ActiveComponent, ClickAction};
use crate::keybindings::{apply_missing_binds, ApplyStatus, Feature, KeybindingState};
use crate::onboarding::DesktopFacts;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PageKind {
    MainScreen,
    UpdateKeybindings,
    DesktopApp,
    Inbox,
    SessionPreviews,
    Other,
}

#[derive(Debug)]
pub struct Page {
    title: Option<Text>,
    components_to_render: Vec<RenderedComponent>,
    has_hover: bool,
    hovering_over_link: bool,
    menu_item_is_selected: bool,
    pub is_main_screen: bool,
    pub kind: PageKind,
}

#[derive(Clone)]
pub struct Shared {
    pub link_executable: Rc<RefCell<String>>,
    pub keybinding_state: Rc<RefCell<KeybindingState>>,
    pub facts: Rc<RefCell<DesktopFacts>>,
    pub main_screen: Rc<dyn Fn() -> Page>,
}

impl Shared {
    pub fn main_screen(&self) -> Page {
        (self.main_screen)()
    }
}

impl Page {
    pub fn new_main_screen(
        link_executable: Rc<RefCell<String>>,
        zellij_version: String,
        base_mode: Rc<RefCell<InputMode>>,
        is_release_notes: bool,
        keybinding_state: Rc<RefCell<KeybindingState>>,
        facts: Rc<RefCell<DesktopFacts>>,
    ) -> Self {
        let has_missing_binds = keybinding_state.borrow().has_missing_binds();
        let main_screen_builder: Rc<dyn Fn() -> Page> = Rc::new({
            let link_executable = link_executable.clone();
            let zellij_version = zellij_version.clone();
            let base_mode = base_mode.clone();
            let keybinding_state = keybinding_state.clone();
            let facts = facts.clone();
            move || {
                Page::new_main_screen(
                    link_executable.clone(),
                    zellij_version.clone(),
                    base_mode.clone(),
                    is_release_notes,
                    keybinding_state.clone(),
                    facts.clone(),
                )
            }
        });
        let shared = Shared {
            link_executable: link_executable.clone(),
            keybinding_state: keybinding_state.clone(),
            facts: facts.clone(),
            main_screen: main_screen_builder,
        };
        let mut page = Page::new()
            .main_screen()
            .with_kind(PageKind::MainScreen)
            .with_title(main_screen_title(zellij_version.clone(), is_release_notes))
            .with_bulletin_list(BulletinList::new(whats_new_title()).with_items(vec![
                main_menu_entry(
                    "Zellij Can Now be Your Terminal",
                    &shared,
                    Page::new_desktop_app,
                ),
                main_menu_entry("Inbox", &shared, Page::new_inbox),
                main_menu_entry("Session Previews", &shared, Page::new_session_previews),
                main_menu_entry(
                    "Right-click Menu and Improved Mouse Control",
                    &shared,
                    Page::new_mouse_control,
                ),
                main_menu_entry("Settings Screen", &shared, Page::new_settings_screen),
            ]));
        page = page.with_paragraph(vec![ComponentLine::new(vec![
            ActiveComponent::new(TextOrCustomRender::Text(
                Text::from("Use Zellij as your terminal app").color_range(2, ..),
            ))
            .with_hover(TextOrCustomRender::Text(
                Text::from("Use Zellij as your terminal app")
                    .color_range(2, ..)
                    .selected(),
            ))
            .with_left_click_action(ClickAction::new_change_page({
                let shared = shared.clone();
                move || Page::new_desktop_app(&shared)
            }))
            .keyboard_selectable(),
            ActiveComponent::new(TextOrCustomRender::Text(
                Text::from(" · <s> Setup Zellij").color_substring(3, "<s>"),
            )),
        ])]);
        page.with_paragraph(vec![ComponentLine::new(vec![
            ActiveComponent::new(TextOrCustomRender::Text(Text::from("Full Changelog: "))),
            ActiveComponent::new(TextOrCustomRender::Text(changelog_link_unselected(
                zellij_version.clone(),
            )))
            .with_hover(TextOrCustomRender::CustomRender(
                Box::new(changelog_link_selected(zellij_version.clone())),
                Box::new(changelog_link_selected_len(zellij_version.clone())),
            ))
            .with_left_click_action(ClickAction::new_open_link(
                format!(
                    "https://github.com/zellij-org/zellij/releases/tag/v{}",
                    zellij_version.clone()
                ),
                link_executable.clone(),
            )),
        ])])
        .with_paragraph(vec![ComponentLine::new(vec![
            ActiveComponent::new(TextOrCustomRender::Text(support_the_developer_text())),
            ActiveComponent::new(TextOrCustomRender::Text(sponsors_link_text_unselected()))
                .with_hover(TextOrCustomRender::CustomRender(
                    Box::new(sponsors_link_text_selected),
                    Box::new(sponsors_link_text_selected_len),
                ))
                .with_left_click_action(ClickAction::new_open_link(
                    "https://github.com/sponsors/imsnif".to_owned(),
                    link_executable.clone(),
                )),
        ])])
        .with_help(Box::new(
            move |hovering_over_link, menu_item_is_selected| {
                main_screen_help_text(hovering_over_link, menu_item_is_selected, has_missing_binds)
            },
        ))
    }
    pub fn new_update_keybindings(shared: &Shared) -> Page {
        let keybinding_state = &shared.keybinding_state;
        let keybind_items: Vec<ActiveComponent> = grouped_binds_to_add(keybinding_state)
            .into_iter()
            .map(|(mode_name, binds)| {
                let keys: Vec<String> = binds
                    .iter()
                    .map(|(key_text, description)| format!("{} {}", key_text, description))
                    .collect();
                let mut text = Text::from(format!("{} mode + {}", mode_name, keys.join(", ")))
                    .color_substring(3, &mode_name);
                for (key_text, _description) in &binds {
                    text = text.color_substring(3, key_text);
                }
                ActiveComponent::new(TextOrCustomRender::Text(text))
            })
            .collect();
        let conflicting_binds: Vec<ComponentLine> = keybinding_state
            .borrow()
            .conflicting_binds()
            .iter()
            .map(|(bind, currently_bound_to)| {
                ComponentLine::new(vec![ActiveComponent::new(TextOrCustomRender::Text(
                    Text::from(format!(
                        "{} is currently bound to {} and will be overwritten",
                        bind.key_text(),
                        currently_bound_to
                    ))
                    .color_substring(3, &bind.key_text())
                    .color_substring(0, currently_bound_to),
                ))])
            })
            .collect();
        let status = keybinding_state.borrow().status().clone();
        let mut page = Page::new()
            .with_kind(PageKind::UpdateKeybindings)
            .with_title(Text::from("Update Keybindings").color_range(0, ..))
            .with_paragraph(feature_sentence(shared))
            .with_bulletin_list(
                BulletinList::new(Text::from("New keybindings:").color_range(2, ..))
                    .with_items(keybind_items),
            );
        if !conflicting_binds.is_empty() {
            page = page.with_paragraph(conflicting_binds);
        }
        match status {
            ApplyStatus::NotApplied => page
                .with_paragraph(vec![ComponentLine::new(vec![
                    ActiveComponent::new(TextOrCustomRender::Text(Text::from(
                        "Add these keybindings to your configuration file? (",
                    ))),
                    ActiveComponent::new(TextOrCustomRender::Text(confirm_text()))
                        .with_hover(TextOrCustomRender::CustomRender(
                            Box::new(confirm_key_selected),
                            Box::new(single_character_len),
                        ))
                        .with_left_click_action(ClickAction::new_change_page({
                            let shared = shared.clone();
                            move || {
                                apply_missing_binds(&shared.keybinding_state);
                                Page::new_update_keybindings(&shared)
                            }
                        })),
                    ActiveComponent::new(TextOrCustomRender::Text(Text::from("/"))),
                    ActiveComponent::new(TextOrCustomRender::Text(cancel_text()))
                        .with_hover(TextOrCustomRender::CustomRender(
                            Box::new(cancel_key_selected),
                            Box::new(single_character_len),
                        ))
                        .with_left_click_action(ClickAction::new_change_page({
                            let shared = shared.clone();
                            move || shared.main_screen()
                        })),
                    ActiveComponent::new(TextOrCustomRender::Text(Text::from(")"))),
                ])])
                .with_help(Box::new(|_hovering_over_link, _menu_item_is_selected| {
                    Text::from("Help: <y> - Add Keybindings, <n> - Cancel, <ESC> - Go back")
                        .color_substring(1, "<y>")
                        .color_substring(1, "<n>")
                        .color_substring(1, "<ESC>")
                })),
            status => page
                .with_paragraph(vec![ComponentLine::new(vec![ActiveComponent::new(
                    TextOrCustomRender::Text(keybinding_status_text(&status)),
                )])])
                .with_help(Box::new(|_hovering_over_link, _menu_item_is_selected| {
                    esc_to_go_back_help()
                })),
        }
    }
}

fn main_menu_entry(
    name: &'static str,
    shared: &Shared,
    build_page: fn(&Shared) -> Page,
) -> ActiveComponent {
    let shared = shared.clone();
    ActiveComponent::new(TextOrCustomRender::Text(main_menu_item(name)))
        .with_hover(TextOrCustomRender::Text(main_menu_item(name).selected()))
        .with_left_click_action(ClickAction::new_change_page(move || build_page(&shared)))
}

fn keybinding_status_text(status: &ApplyStatus) -> Text {
    match status {
        ApplyStatus::Applying => Text::from("Adding keybindings...").color_range(2, ..),
        ApplyStatus::Applied => {
            Text::from("Keybindings added and saved to your config file.").color_range(2, ..)
        },
        ApplyStatus::Failed(Some(config_file)) => Text::from(format!(
            "Keybindings added to this session, but {} could not be written.",
            config_file
        ))
        .color_range(3, ..),
        ApplyStatus::Failed(None) => Text::from(
            "Keybindings added to this session, but the config file could not be written.",
        )
        .color_range(3, ..),
        ApplyStatus::NotApplied => Text::from(""),
    }
}

pub(crate) fn missing_binds_note(shared: &Shared, feature: Feature) -> Option<Vec<ComponentLine>> {
    let keybinding_state = &shared.keybinding_state;
    let status = keybinding_state.borrow().status().clone();
    let has_missing_binds = keybinding_state.borrow().has_missing_binds_for(feature);
    let has_conflicts = keybinding_state.borrow().has_conflicts();
    match status {
        ApplyStatus::NotApplied if has_missing_binds => Some(vec![ComponentLine::new(vec![
            ActiveComponent::new(TextOrCustomRender::Text(
                Text::from("Note: these keybindings are not in your config file. Add them? (")
                    .color_substring(2, "Note:"),
            )),
            ActiveComponent::new(TextOrCustomRender::Text(confirm_text()))
                .with_hover(TextOrCustomRender::CustomRender(
                    Box::new(confirm_key_selected),
                    Box::new(single_character_len),
                ))
                .with_left_click_action(ClickAction::new_change_page({
                    let shared = shared.clone();
                    move || {
                        if has_conflicts {
                            Page::new_update_keybindings(&shared)
                        } else {
                            apply_missing_binds(&shared.keybinding_state);
                            feature_page(feature, &shared)
                        }
                    }
                })),
            ActiveComponent::new(TextOrCustomRender::Text(Text::from("/"))),
            ActiveComponent::new(TextOrCustomRender::Text(cancel_text()))
                .with_hover(TextOrCustomRender::CustomRender(
                    Box::new(cancel_key_selected),
                    Box::new(single_character_len),
                ))
                .with_left_click_action(ClickAction::new_change_page({
                    let shared = shared.clone();
                    move || shared.main_screen()
                })),
            ActiveComponent::new(TextOrCustomRender::Text(Text::from(")"))),
        ])]),
        ApplyStatus::NotApplied => None,
        status => Some(vec![ComponentLine::new(vec![ActiveComponent::new(
            TextOrCustomRender::Text(keybinding_status_text(&status)),
        )])]),
    }
}

fn feature_has_page(feature: Feature) -> bool {
    matches!(feature, Feature::Inbox | Feature::SessionHistory)
}

fn feature_page(feature: Feature, shared: &Shared) -> Page {
    if feature_has_page(feature) {
        Page::new_inbox(shared)
    } else {
        shared.main_screen()
    }
}

fn confirm_text() -> Text {
    Text::from("y").color_range(3, ..)
}

fn cancel_text() -> Text {
    Text::from("n").color_range(3, ..)
}

fn confirm_key_selected(x: usize, y: usize) -> usize {
    print!("\u{1b}[{};{}H\u{1b}[m\u{1b}[1;4my", y + 1, x + 1);
    1
}

fn cancel_key_selected(x: usize, y: usize) -> usize {
    print!("\u{1b}[{};{}H\u{1b}[m\u{1b}[1;4mn", y + 1, x + 1);
    1
}

fn single_character_len() -> usize {
    1
}

pub fn link(label: impl Into<String>, action: ClickAction) -> ActiveComponent {
    let label: String = label.into();
    let hover_label = label.clone();
    let width = zellij_tile::graphemes::display_width(&label);
    ActiveComponent::new(TextOrCustomRender::Text(
        Text::from(label.as_str()).color_range(3, ..),
    ))
    .with_hover(TextOrCustomRender::CustomRender(
        Box::new(move |x, y| {
            print!(
                "\u{1b}[{};{}H\u{1b}[m\u{1b}[1;4m{}",
                y + 1,
                x + 1,
                hover_label
            );
            width
        }),
        Box::new(move || width),
    ))
    .with_left_click_action(action)
    .keyboard_selectable()
}

fn grouped_binds_to_add(
    keybinding_state: &Rc<RefCell<KeybindingState>>,
) -> Vec<(String, Vec<(String, &'static str)>)> {
    let mut groups: Vec<(Feature, String, Vec<(String, &'static str)>)> = vec![];
    for bind in keybinding_state.borrow().binds_to_add() {
        let mode_name = bind.mode_name();
        let entry = (bind.key_text(), bind.description);
        match groups.last_mut() {
            Some((feature, group_mode_name, binds))
                if *feature == bind.feature && *group_mode_name == mode_name =>
            {
                binds.push(entry);
            },
            _ => groups.push((bind.feature, mode_name, vec![entry])),
        }
    }
    groups
        .into_iter()
        .map(|(_feature, mode_name, binds)| (mode_name, binds))
        .collect()
}

const FEATURE_SENTENCE_WIDTH: usize = 78;

fn feature_sentence(shared: &Shared) -> Vec<ComponentLine> {
    let features = shared.keybinding_state.borrow().features_to_add();
    let mut pieces: Vec<(String, Option<Feature>)> = vec![(
        "This version includes new keybindings for ".to_owned(),
        None,
    )];
    for (feature_index, feature) in features.iter().enumerate() {
        let is_last = feature_index == features.len().saturating_sub(1);
        if feature_index > 0 && is_last {
            pieces.push(("and ".to_owned(), None));
        }
        pieces.push((feature.label().to_owned(), Some(*feature)));
        let punctuation = if is_last {
            "."
        } else if feature_index + 2 == features.len() {
            " "
        } else {
            ", "
        };
        pieces.push((punctuation.to_owned(), None));
    }
    let mut lines: Vec<Vec<ActiveComponent>> = vec![vec![]];
    let mut line_width = 0;
    for (piece, feature) in pieces {
        let words: Vec<String> = match feature {
            Some(_) => vec![piece],
            None => piece
                .split_inclusive(' ')
                .map(|word| word.to_owned())
                .collect(),
        };
        for word in words {
            let width = zellij_tile::graphemes::display_width(word.trim_end());
            if line_width > 0 && line_width + width > FEATURE_SENTENCE_WIDTH {
                lines.push(vec![]);
                line_width = 0;
            }
            line_width += zellij_tile::graphemes::display_width(&word);
            let component = match feature {
                Some(feature) if feature_has_page(feature) => {
                    let shared = shared.clone();
                    link(
                        word,
                        ClickAction::new_change_page(move || feature_page(feature, &shared)),
                    )
                },
                Some(_) => ActiveComponent::new(TextOrCustomRender::Text(
                    Text::from(word).color_range(3, ..),
                )),
                None => ActiveComponent::new(TextOrCustomRender::Text(Text::from(word))),
            };
            if let Some(line) = lines.last_mut() {
                line.push(component);
            }
        }
    }
    lines.into_iter().map(ComponentLine::new).collect()
}

impl Page {
    pub fn new() -> Self {
        Page {
            title: None,
            components_to_render: vec![],
            has_hover: false,
            hovering_over_link: false,
            menu_item_is_selected: false,
            is_main_screen: false,
            kind: PageKind::Other,
        }
    }
    pub fn main_screen(mut self) -> Self {
        self.is_main_screen = true;
        self
    }
    pub fn with_kind(mut self, kind: PageKind) -> Self {
        self.kind = kind;
        self
    }
    pub fn with_title(mut self, title: Text) -> Self {
        self.title = Some(title);
        self
    }
    pub fn with_bulletin_list(mut self, bulletin_list: BulletinList) -> Self {
        self.components_to_render
            .push(RenderedComponent::BulletinList(bulletin_list));
        self
    }
    pub fn with_paragraph(mut self, paragraph: Vec<ComponentLine>) -> Self {
        self.components_to_render
            .push(RenderedComponent::Paragraph(paragraph));
        self
    }
    pub fn with_help(mut self, help_text_fn: Box<dyn Fn(bool, bool) -> Text>) -> Self {
        self.components_to_render
            .push(RenderedComponent::HelpText(help_text_fn));
        self
    }
    pub fn handle_key(&mut self, key: KeyWithModifier) -> bool {
        let mut should_render = false;
        if key.bare_key == BareKey::Down && key.has_no_modifiers() {
            self.move_selection_down();
            should_render = true;
        } else if key.bare_key == BareKey::Up && key.has_no_modifiers() {
            self.move_selection_up();
            should_render = true;
        }
        should_render
    }
    pub fn handle_mouse_left_click(&mut self, x: usize, y: usize) -> Option<Page> {
        for rendered_component in &mut self.components_to_render {
            match rendered_component {
                RenderedComponent::BulletinList(bulletin_list) => {
                    let page_to_render = bulletin_list.handle_left_click_at_position(x, y);
                    if page_to_render.is_some() {
                        return page_to_render;
                    }
                },
                RenderedComponent::Paragraph(paragraph) => {
                    for component_line in paragraph {
                        let page_to_render = component_line.handle_left_click_at_position(x, y);
                        if page_to_render.is_some() {
                            return page_to_render;
                        }
                    }
                },
                _ => {},
            }
        }
        None
    }
    pub fn handle_selection(&mut self) -> Option<Page> {
        for component in self.selectable_components() {
            if component.is_active {
                let page_to_render = component.handle_selection();
                if page_to_render.is_some() {
                    return page_to_render;
                }
            }
        }
        None
    }
    pub fn handle_mouse_hover(&mut self, x: usize, y: usize) -> bool {
        let hover_cleared = self.clear_hover(); // TODO: do the right thing if the same component was hovered from
                                                // previous motion
        for rendered_component in &mut self.components_to_render {
            match rendered_component {
                RenderedComponent::BulletinList(bulletin_list) => {
                    let should_render = bulletin_list.handle_hover_at_position(x, y);
                    if should_render {
                        self.has_hover = true;
                        self.menu_item_is_selected = true;
                        return should_render;
                    }
                },
                RenderedComponent::Paragraph(paragraph) => {
                    for component_line in paragraph {
                        if let Some(hovered_component_is_selectable) =
                            component_line.handle_hover_at_position(x, y)
                        {
                            self.has_hover = true;
                            if hovered_component_is_selectable {
                                self.menu_item_is_selected = true;
                            } else {
                                self.hovering_over_link = true;
                            }
                            return true;
                        }
                    }
                },
                _ => {},
            }
        }
        hover_cleared
    }
    fn move_selection_up(&mut self) {
        match self.position_of_active_selectable() {
            Some(position_of_active_selectable) if position_of_active_selectable > 0 => {
                self.clear_active_selectables();
                self.set_active_selectable(position_of_active_selectable.saturating_sub(1));
            },
            Some(0) => {
                self.clear_active_selectables();
            },
            _ => {
                self.clear_active_selectables();
                self.set_last_active_selectable();
            },
        }
    }
    fn move_selection_down(&mut self) {
        match self.position_of_active_selectable() {
            Some(position_of_active_selectable) => {
                self.clear_active_selectables();
                self.set_active_selectable(position_of_active_selectable + 1);
            },
            None => {
                self.set_active_selectable(0);
            },
        }
    }
    fn selectable_components(&mut self) -> Vec<&mut ActiveComponent> {
        let mut selectable_components = vec![];
        for rendered_component in &mut self.components_to_render {
            match rendered_component {
                RenderedComponent::BulletinList(bulletin_list) => {
                    selectable_components.append(&mut bulletin_list.items_mut());
                },
                RenderedComponent::Paragraph(paragraph) => {
                    for component_line in paragraph {
                        selectable_components
                            .append(&mut component_line.keyboard_selectable_components_mut());
                    }
                },
                _ => {},
            }
        }
        selectable_components
    }
    fn position_of_active_selectable(&mut self) -> Option<usize> {
        self.selectable_components()
            .iter()
            .position(|component| component.is_active)
    }
    fn clear_active_selectables(&mut self) {
        for component in self.selectable_components() {
            component.is_active = false;
        }
    }
    fn set_active_selectable(&mut self, position: usize) {
        if let Some(component) = self.selectable_components().get_mut(position) {
            component.is_active = true;
        }
    }
    fn set_last_active_selectable(&mut self) {
        if let Some(component) = self.selectable_components().last_mut() {
            component.is_active = true;
        }
    }
    fn clear_hover(&mut self) -> bool {
        let had_hover = self.has_hover;
        self.menu_item_is_selected = false;
        self.hovering_over_link = false;
        for rendered_component in &mut self.components_to_render {
            match rendered_component {
                RenderedComponent::BulletinList(bulletin_list) => {
                    bulletin_list.clear_hover();
                },
                RenderedComponent::Paragraph(paragraph) => {
                    for active_component in paragraph {
                        active_component.clear_hover();
                    }
                },
                _ => {},
            }
        }
        self.has_hover = false;
        had_hover
    }
    pub fn ui_column_count(&mut self) -> usize {
        let mut column_count = 0;
        for rendered_component in &self.components_to_render {
            match rendered_component {
                RenderedComponent::BulletinList(bulletin_list) => {
                    column_count = std::cmp::max(column_count, bulletin_list.column_count());
                },
                RenderedComponent::Paragraph(paragraph) => {
                    for active_component in paragraph {
                        column_count = std::cmp::max(column_count, active_component.column_count());
                    }
                },
                RenderedComponent::HelpText(_text) => {}, // we ignore help text in column
                                                          // calculation because it's always left
                                                          // justified
            }
        }
        column_count
    }
    pub fn ui_row_count(&mut self) -> usize {
        let mut row_count = 0;
        if self.title.is_some() {
            row_count += 1;
        }
        for rendered_component in &self.components_to_render {
            match rendered_component {
                RenderedComponent::BulletinList(bulletin_list) => {
                    row_count += bulletin_list.len();
                },
                RenderedComponent::Paragraph(paragraph) => {
                    row_count += paragraph.len();
                },
                RenderedComponent::HelpText(_text) => {}, // we ignore help text as it is outside
                                                          // the UI container
            }
        }
        row_count += self.components_to_render.len();
        row_count
    }
    pub fn render(&mut self, rows: usize, columns: usize, error: &Option<String>) {
        let base_x = columns.saturating_sub(self.ui_column_count()) / 2;
        let base_y = rows.saturating_sub(self.ui_row_count()) / 2;
        let mut current_y = base_y;
        if let Some(title) = &self.title {
            print_text_with_coordinates(
                title.clone(),
                base_x,
                current_y,
                Some(columns),
                Some(rows),
            );
            current_y += 2;
        }
        for rendered_component in &mut self.components_to_render {
            let is_help = match rendered_component {
                RenderedComponent::HelpText(_) => true,
                _ => false,
            };
            if is_help {
                if let Some(error) = error {
                    render_error(error, rows);
                    continue;
                }
            }
            let y = if is_help { rows } else { current_y };
            let columns = if is_help {
                columns
            } else {
                columns.saturating_sub(base_x * 2)
            };
            let rendered_rows = rendered_component.render(
                base_x,
                y,
                rows,
                columns,
                self.hovering_over_link,
                self.menu_item_is_selected,
            );
            current_y += rendered_rows + 1; // 1 for the line space between components
        }
    }
}

fn render_error(error: &str, y: usize) {
    print_text_with_coordinates(
        Text::from(format!("ERROR: {}", error)).color_range(3, ..),
        0,
        y,
        None,
        None,
    );
}

fn changelog_link_unselected(version: String) -> Text {
    let full_changelog_text = format!(
        "https://github.com/zellij-org/zellij/releases/tag/v{}",
        version
    );
    Text::from(full_changelog_text)
}

fn changelog_link_selected(version: String) -> Box<dyn Fn(usize, usize) -> usize> {
    Box::new(move |x, y| {
        print!(
            "\u{1b}[{};{}H\u{1b}[m\u{1b}[1;4mhttps://github.com/zellij-org/zellij/releases/tag/v{}",
            y + 1,
            x + 1,
            version
        );
        51 + zellij_tile::graphemes::display_width(&version)
    })
}

fn changelog_link_selected_len(version: String) -> Box<dyn Fn() -> usize> {
    Box::new(move || 51 + zellij_tile::graphemes::display_width(&version))
}

fn sponsors_link_text_unselected() -> Text {
    Text::from("https://github.com/sponsors/imsnif")
}

fn sponsors_link_text_selected(x: usize, y: usize) -> usize {
    print!(
        "\u{1b}[{};{}H\u{1b}[m\u{1b}[1;4mhttps://github.com/sponsors/imsnif",
        y + 1,
        x + 1
    );
    34
}

fn sponsors_link_text_selected_len() -> usize {
    34
}

// Text components
fn whats_new_title() -> Text {
    Text::from("What's new?")
}

fn main_screen_title(version: String, is_release_notes: bool) -> Text {
    if is_release_notes {
        let title_text = format!("Hi there, welcome to Zellij {}!", &version);
        Text::from(title_text).color_range(2, 21..=27 + version.chars().count())
    } else {
        let title_text = format!("Zellij {}", &version);
        Text::from(title_text).color_range(2, ..)
    }
}

fn main_screen_help_text(
    hovering_over_link: bool,
    menu_item_is_selected: bool,
    has_missing_binds: bool,
) -> Text {
    if hovering_over_link {
        return link_hover_help();
    }
    let mut help_text = String::from("Help: <↓↑> - Navigate");
    if menu_item_is_selected {
        help_text.push_str(", <ENTER> - Learn More");
    }
    if has_missing_binds {
        help_text.push_str(", <u> - Update Keybindings");
    }
    help_text.push_str(", <s> - Setup, <ESC> - Dismiss");
    if !menu_item_is_selected {
        help_text.push_str(", <?> - Usage Tips");
    }
    color_help_keys(help_text)
}

fn color_help_keys(help_text: String) -> Text {
    Text::from(help_text)
        .color_substring(1, "<↓↑>")
        .color_substring(1, "<ENTER>")
        .color_substring(2, "Update Keybindings")
        .color_substring(1, "<u>")
        .color_substring(1, "<s>")
        .color_substring(1, "<ESC>")
        .color_substring(1, "<?>")
}

fn link_hover_help() -> Text {
    Text::from("Help: Click or Shift-Click to open in browser")
        .color_range(3, 6..=10)
        .color_range(3, 15..=25)
}

pub fn feature_page_help(_hovering_over_link: bool, _menu_item_is_selected: bool) -> Text {
    Text::from("Help: <↓↑> - Navigate, <ENTER> - Open, <ESC> - Go back")
        .color_substring(1, "<↓↑>")
        .color_substring(1, "<ENTER>")
        .color_substring(1, "<ESC>")
}

fn esc_to_go_back_help() -> Text {
    let help_text = format!("Help: <ESC> - Go back");
    Text::from(help_text).color_range(1, 6..=10)
}

fn main_menu_item(item_name: &str) -> Text {
    Text::from(item_name).color_range(0, ..)
}

fn support_the_developer_text() -> Text {
    let support_text = format!("Please support the Zellij developer <3: ");
    Text::from(support_text).color_range(3, ..)
}

pub enum TextOrCustomRender {
    Text(Text),
    CustomRender(
        Box<dyn Fn(usize, usize) -> usize>, // (rows, columns) -> text_len (render function)
        Box<dyn Fn() -> usize>,             // length of rendered component
    ),
}

impl TextOrCustomRender {
    pub fn len(&self) -> usize {
        match self {
            TextOrCustomRender::Text(text) => text.len(),
            TextOrCustomRender::CustomRender(_render_fn, len_fn) => len_fn(),
        }
    }
    pub fn render(&mut self, x: usize, y: usize, rows: usize, columns: usize) -> usize {
        match self {
            TextOrCustomRender::Text(text) => {
                print_text_with_coordinates(text.clone(), x, y, Some(columns), Some(rows));
                text.len()
            },
            TextOrCustomRender::CustomRender(render_fn, _len_fn) => render_fn(x, y),
        }
    }
}

impl std::fmt::Debug for TextOrCustomRender {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TextOrCustomRender::Text(text) => write!(f, "Text {{ {:?} }}", text),
            TextOrCustomRender::CustomRender(..) => write!(f, "CustomRender"),
        }
    }
}

enum RenderedComponent {
    HelpText(Box<dyn Fn(bool, bool) -> Text>),
    BulletinList(BulletinList),
    Paragraph(Vec<ComponentLine>),
}

impl std::fmt::Debug for RenderedComponent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RenderedComponent::HelpText(_) => write!(f, "HelpText"),
            RenderedComponent::BulletinList(bulletinlist) => write!(f, "{:?}", bulletinlist),
            RenderedComponent::Paragraph(component_list) => write!(f, "{:?}", component_list),
        }
    }
}

impl RenderedComponent {
    pub fn render(
        &mut self,
        x: usize,
        y: usize,
        rows: usize,
        columns: usize,
        hovering_over_link: bool,
        menu_item_is_selected: bool,
    ) -> usize {
        let mut rendered_rows = 0;
        match self {
            RenderedComponent::HelpText(text) => {
                rendered_rows += 1;
                print_text_with_coordinates(
                    text(hovering_over_link, menu_item_is_selected),
                    0,
                    y,
                    Some(columns),
                    Some(rows),
                );
            },
            RenderedComponent::BulletinList(bulletin_list) => {
                rendered_rows += bulletin_list.len();
                bulletin_list.render(x, y, rows, columns);
            },
            RenderedComponent::Paragraph(paragraph) => {
                let mut paragraph_rendered_rows = 0;
                for component_line in paragraph {
                    component_line.render(
                        x,
                        y + paragraph_rendered_rows,
                        rows.saturating_sub(paragraph_rendered_rows),
                        columns,
                    );
                    rendered_rows += 1;
                    paragraph_rendered_rows += 1;
                }
            },
        }
        rendered_rows
    }
}

#[derive(Debug)]
pub struct BulletinList {
    title: Text,
    items: Vec<ActiveComponent>,
}

impl BulletinList {
    pub fn new(title: Text) -> Self {
        BulletinList {
            title,
            items: vec![],
        }
    }
    pub fn with_items(mut self, items: Vec<ActiveComponent>) -> Self {
        self.items = items;
        self
    }
    pub fn len(&self) -> usize {
        self.items.len() + 1 // 1 for the title
    }
    pub fn column_count(&self) -> usize {
        let mut column_count = 0;
        for (item_index, item) in self.items.iter().enumerate() {
            let item_bulletin_len = format!("{}. ", item_index + 1).chars().count();
            column_count = std::cmp::max(column_count, item.column_count() + item_bulletin_len);
        }
        column_count
    }
    pub fn handle_left_click_at_position(&mut self, x: usize, y: usize) -> Option<Page> {
        for component in &mut self.items {
            let page_to_render = component.handle_left_click_at_position(x, y);
            if page_to_render.is_some() {
                return page_to_render;
            }
        }
        None
    }
    pub fn items_mut(&mut self) -> Vec<&mut ActiveComponent> {
        self.items.iter_mut().collect()
    }
    pub fn handle_hover_at_position(&mut self, x: usize, y: usize) -> bool {
        for component in &mut self.items {
            let should_render = component.handle_hover_at_position(x, y);
            if should_render {
                return should_render;
            }
        }
        false
    }
    pub fn clear_hover(&mut self) {
        for component in &mut self.items {
            component.clear_hover();
        }
    }
    pub fn render(&mut self, x: usize, y: usize, rows: usize, columns: usize) {
        print_text_with_coordinates(self.title.clone(), x, y, Some(columns), Some(rows));
        let mut item_bulletin = 1;
        let mut running_y = y + 1;
        for item in &mut self.items {
            let mut item_bulletin_text = Text::from(format!("{}. ", item_bulletin));
            if item.is_active {
                item_bulletin_text = item_bulletin_text.selected();
            }
            let item_bulletin_text_len = item_bulletin_text.len();
            print_text_with_coordinates(
                item_bulletin_text,
                x,
                running_y,
                Some(item_bulletin_text_len),
                Some(rows),
            );
            let extend_hit_area_to_end_of_line = true;
            item.render(
                x + item_bulletin_text_len,
                running_y,
                rows,
                columns.saturating_sub(item_bulletin_text_len),
                extend_hit_area_to_end_of_line,
            );
            running_y += 1;
            item_bulletin += 1;
        }
    }
}

#[derive(Debug)]
pub struct ComponentLine {
    components: Vec<ActiveComponent>,
}

impl ComponentLine {
    pub fn handle_left_click_at_position(&mut self, x: usize, y: usize) -> Option<Page> {
        for active_component in &mut self.components {
            let page_to_render = active_component.handle_left_click_at_position(x, y);
            if page_to_render.is_some() {
                return page_to_render;
            }
        }
        None
    }
    pub fn handle_hover_at_position(&mut self, x: usize, y: usize) -> Option<bool> {
        for active_component in &mut self.components {
            if active_component.handle_hover_at_position(x, y) {
                return Some(active_component.is_keyboard_selectable());
            }
        }
        None
    }
    pub fn keyboard_selectable_components_mut(&mut self) -> Vec<&mut ActiveComponent> {
        self.components
            .iter_mut()
            .filter(|component| component.is_keyboard_selectable())
            .collect()
    }
    pub fn clear_hover(&mut self) {
        for active_component in &mut self.components {
            active_component.clear_hover();
        }
    }
    pub fn column_count(&self) -> usize {
        let mut column_count = 0;
        for active_component in &self.components {
            column_count += active_component.column_count()
        }
        column_count
    }
    pub fn render(&mut self, x: usize, y: usize, rows: usize, columns: usize) {
        let mut current_x = x;
        let mut columns_left = columns;
        let last_component_index = self.components.len().saturating_sub(1);
        for (component_index, component) in self.components.iter_mut().enumerate() {
            let extend_hit_area_to_end_of_line = component_index == last_component_index;
            let component_len = component.render(
                current_x,
                y,
                rows,
                columns_left,
                extend_hit_area_to_end_of_line,
            );
            current_x += component_len;
            columns_left = columns_left.saturating_sub(component_len);
        }
    }
}

impl ComponentLine {
    pub fn new(components: Vec<ActiveComponent>) -> Self {
        ComponentLine { components }
    }
}
