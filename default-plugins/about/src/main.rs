mod active_component;
mod keybindings;
mod onboarding;
mod pages;
mod tips;
use zellij_tile::prelude::*;

use pages::Page;
use rand::prelude::*;
use rand::rng;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;
use tips::MAX_TIP_INDEX;

use crate::active_component::ActiveComponent;
use crate::keybindings::{apply_missing_binds, ApplyStatus, Feature, KeybindingState};
use crate::onboarding::{DesktopFacts, DESKTOP_TIP_INDEX, HOME_MANAGER_NOTE};
use crate::pages::{ComponentLine, PageKind, TextOrCustomRender};
use zellij_tile::prelude::desktop_integration::{
    opt_out_kdl, opt_out_record, Item, RepairAnswer, Request, Response,
};

const UI_ROWS: usize = 28;
const UI_COLUMNS: usize = 90;

#[derive(Debug, Clone, PartialEq, Eq)]
enum Special {
    None,
    WindowWelcome,
    Repair,
}

#[derive(Debug)]
struct App {
    active_page: Page,
    link_executable: Rc<RefCell<String>>,
    zellij_version: Rc<RefCell<String>>,
    base_mode: Rc<RefCell<InputMode>>,
    keybinding_state: Rc<RefCell<KeybindingState>>,
    tab_rows: usize,
    tab_columns: usize,
    own_plugin_id: Option<u32>,
    is_release_notes: bool,
    is_startup_tip: bool,
    tip_index: usize,
    waiting_for_config_to_be_written: bool,
    error: Option<String>,
    facts: Rc<RefCell<DesktopFacts>>,
    special: Special,
    waiting_for_opt_out: bool,
}

impl Default for App {
    fn default() -> Self {
        let link_executable = Rc::new(RefCell::new("".to_owned()));
        let zellij_version = Rc::new(RefCell::new("".to_owned()));
        let base_mode = Rc::new(RefCell::new(Default::default()));
        let keybinding_state = Rc::new(RefCell::new(Default::default()));
        let facts = Rc::new(RefCell::new(DesktopFacts::default()));
        App {
            active_page: Page::new_main_screen(
                link_executable.clone(),
                "".to_owned(),
                base_mode.clone(),
                false,
                keybinding_state.clone(),
                facts.clone(),
            ),
            link_executable,
            zellij_version,
            base_mode,
            keybinding_state,
            tab_rows: 0,
            tab_columns: 0,
            own_plugin_id: None,
            is_release_notes: false,
            is_startup_tip: false,
            tip_index: 0,
            waiting_for_config_to_be_written: false,
            error: None,
            facts,
            special: Special::None,
            waiting_for_opt_out: false,
        }
    }
}

register_plugin!(App);

impl ZellijPlugin for App {
    fn load(&mut self, configuration: BTreeMap<String, String>) {
        self.is_release_notes = configuration
            .get("is_release_notes")
            .map(|v| v == "true")
            .unwrap_or(false);
        self.is_startup_tip = configuration
            .get("is_startup_tip")
            .map(|v| v == "true")
            .unwrap_or(false);
        let desktop_tip = configuration
            .get("desktop_tip")
            .map(|v| v == "true")
            .unwrap_or(false);
        self.facts.borrow_mut().in_desktop_terminal = configuration
            .get("in_desktop_terminal")
            .map(|v| v == "true")
            .unwrap_or(false);
        subscribe(&[
            EventType::Key,
            EventType::Mouse,
            EventType::ModeUpdate,
            EventType::RunCommandResult,
            EventType::TabUpdate,
            EventType::FailedToWriteConfigToDisk,
            EventType::ConfigWasWrittenToDisk,
            EventType::DesktopIntegration,
        ]);
        let own_plugin_id = get_plugin_ids().plugin_id;
        self.own_plugin_id = Some(own_plugin_id);
        *self.zellij_version.borrow_mut() = get_zellij_version();
        self.query_link_executable();
        let page = configuration.get("page").cloned().unwrap_or_default();
        self.active_page = if page == "window_welcome" {
            self.special = Special::WindowWelcome;
            let missing: Vec<String> = configuration
                .get("missing_services")
                .map(|text| {
                    text.lines()
                        .filter(|l| !l.trim().is_empty())
                        .map(str::to_owned)
                        .collect()
                })
                .unwrap_or_default();
            let only_missing = configuration
                .get("only_missing_services")
                .map(|v| v == "true")
                .unwrap_or(false);
            Page::new_window_welcome(&missing, only_missing)
        } else if page == "repair" {
            self.special = Special::Repair;
            Page::new_repair(
                configuration
                    .get("missing")
                    .map(String::as_str)
                    .unwrap_or(""),
                configuration
                    .get("current")
                    .map(String::as_str)
                    .unwrap_or(""),
            )
        } else if self.is_startup_tip {
            self.tip_index = if desktop_tip {
                DESKTOP_TIP_INDEX
            } else {
                let mut rng = rng();
                rng.random_range(0..=MAX_TIP_INDEX)
            };
            Page::new_tip_screen(
                self.link_executable.clone(),
                self.base_mode.clone(),
                self.tip_index,
            )
        } else {
            request_desktop_integration(&Request::Status);
            self.main_screen()
        };
        self.change_own_title();
    }
    fn update(&mut self, event: Event) -> bool {
        let mut should_render = false;
        match event {
            Event::DesktopIntegration(payload) => {
                should_render = self.desktop_integration(payload);
            },
            Event::FailedToWriteConfigToDisk(file_path) if self.waiting_for_opt_out => {
                self.waiting_for_opt_out = false;
                request_desktop_integration(&Request::ClearOptOut);
                self.active_page =
                    Page::new_message("Not saved", &onboarding::opt_out_failed_lines(file_path));
                should_render = true;
            },
            Event::ConfigWasWrittenToDisk if self.waiting_for_opt_out => {
                self.waiting_for_opt_out = false;
                self.active_page =
                    Page::new_message("Opted out", &onboarding::opt_out_done_lines());
                should_render = true;
            },
            Event::FailedToWriteConfigToDisk(file_path) => {
                if self.applying_keybindings() {
                    let mut lines = vec![format!(
                        "The keybindings work in this session, but {} could not be written.",
                        file_path
                            .clone()
                            .unwrap_or_else(|| "the config file".to_owned())
                    )];
                    lines.push("Add these lines to it yourself:".to_owned());
                    lines.extend(
                        onboarding::kdl_for_binds(
                            &self.keybinding_state.borrow().binds_for_config(),
                        )
                        .into_iter()
                        .map(|l| format!("    {}", l)),
                    );
                    lines.push(HOME_MANAGER_NOTE.to_owned());
                    self.keybinding_state
                        .borrow_mut()
                        .set_status(ApplyStatus::Failed(file_path));
                    self.active_page = Page::new_message("Keybindings not saved", &lines);
                    should_render = true;
                } else if self.waiting_for_config_to_be_written {
                    let error = match file_path {
                        Some(file_path) => {
                            format!("Failed to write config to disk at: {}", file_path)
                        },
                        None => format!("Failed to write config to disk."),
                    };
                    eprintln!("{}", error);
                    self.error = Some(error);
                    should_render = true;
                }
            },
            Event::ConfigWasWrittenToDisk => {
                if self.applying_keybindings() {
                    self.keybinding_state
                        .borrow_mut()
                        .set_status(ApplyStatus::Applied);
                    self.refresh_active_page();
                    should_render = true;
                } else if self.waiting_for_config_to_be_written {
                    close_self();
                }
            },
            Event::TabUpdate(tab_info) => {
                self.center_own_pane(tab_info);
            },
            Event::Mouse(mouse_event) => {
                should_render = self.handle_mouse_event(mouse_event);
            },
            Event::ModeUpdate(mode_info) => {
                self.facts.borrow_mut().session_name = mode_info.session_name.clone();
                if let Some(base_mode) = mode_info.base_mode {
                    should_render = self.update_base_mode(base_mode);
                }
                let base_mode_changed = self
                    .keybinding_state
                    .borrow_mut()
                    .set_base_mode(mode_info.base_mode.unwrap_or(InputMode::Normal));
                let missing_keybindings_changed = self
                    .keybinding_state
                    .borrow_mut()
                    .update_from_keybinds(&mode_info.keybinds);
                if base_mode_changed || missing_keybindings_changed {
                    self.refresh_active_page();
                    should_render = true;
                }
            },
            Event::RunCommandResult(exit_code, _stdout, _stderr, context) => {
                let is_xdg_open = context.get("xdg_open_cli").is_some();
                let is_open = context.get("open_cli").is_some();
                if is_xdg_open {
                    if exit_code == Some(0) {
                        self.update_link_executable("xdg-open".to_owned());
                    }
                } else if is_open {
                    if exit_code == Some(0) {
                        self.update_link_executable("open".to_owned());
                    }
                }
            },
            Event::Key(key) => {
                if let Some(_error) = self.error.take() {
                    // dismiss error on any key
                    should_render = true;
                } else {
                    should_render = self.handle_key(key);
                }
            },
            _ => {},
        }
        should_render
    }
    fn render(&mut self, rows: usize, cols: usize) {
        if let Some(error) = &self.error {
            self.render_error(rows, cols, error.to_owned())
        } else {
            self.active_page.render(rows, cols, &self.error);
        }
    }
}

impl App {
    fn main_screen(&self) -> Page {
        Page::new_main_screen(
            self.link_executable.clone(),
            self.zellij_version.borrow().clone(),
            self.base_mode.clone(),
            self.is_release_notes,
            self.keybinding_state.clone(),
            self.facts.clone(),
        )
    }
    fn main_screen_builder(&self) -> Rc<dyn Fn() -> Page> {
        let link_executable = self.link_executable.clone();
        let zellij_version = self.zellij_version.clone();
        let base_mode = self.base_mode.clone();
        let keybinding_state = self.keybinding_state.clone();
        let is_release_notes = self.is_release_notes;
        let facts = self.facts.clone();
        Rc::new(move || {
            Page::new_main_screen(
                link_executable.clone(),
                zellij_version.borrow().clone(),
                base_mode.clone(),
                is_release_notes,
                keybinding_state.clone(),
                facts.clone(),
            )
        })
    }
    fn desktop_integration(&mut self, payload: String) -> bool {
        match Response::from_json(&payload) {
            Response::Status(status) => {
                let launcher = status.item(Item::Launcher);
                let mut facts = self.facts.borrow_mut();
                facts.launcher_installable = launcher
                    .map(|l| l.available && l.changeable && !l.installed)
                    .unwrap_or(false);
                facts.platform = status.environment.platform;
                drop(facts);
                self.refresh_active_page();
                true
            },
            Response::Applied(outcomes) => {
                let lines: Vec<String> = outcomes
                    .iter()
                    .flat_map(|outcome| {
                        let mark = if outcome.succeeded { "✓" } else { "✗" };
                        std::iter::once(format!("{} {}", mark, outcome.item.label()))
                            .chain(outcome.message.lines().map(|l| format!("  {}", l)))
                    })
                    .collect();
                self.facts.borrow_mut().launcher_installable = false;
                let page = Page::new_message("Done", &lines);
                self.active_page = if self.special == Special::Repair {
                    self.special = Special::None;
                    page.main_screen()
                } else {
                    page
                };
                true
            },
            Response::Done(_) => false,
            Response::Error(error) => {
                self.active_page = Page::new_message("Something went wrong", &[error]);
                true
            },
        }
    }
    fn opt_out(&mut self) {
        self.waiting_for_opt_out = true;
        request_desktop_integration(&Request::RecordOptOut(opt_out_record()));
        reconfigure(opt_out_kdl(), true);
    }
    fn update_keybindings_screen(&self) -> Page {
        Page::new_update_keybindings(self.keybinding_state.clone(), self.main_screen_builder())
    }
    fn applying_keybindings(&self) -> bool {
        self.keybinding_state.borrow().status() == &ApplyStatus::Applying
    }
    fn awaiting_keybinding_confirmation(&self) -> bool {
        let is_confirmation_page = match self.active_page.kind {
            PageKind::UpdateKeybindings => true,
            PageKind::NestedSessions => self
                .keybinding_state
                .borrow()
                .has_missing_binds_for(Feature::NestedSessions),
            PageKind::PaneFocus => self
                .keybinding_state
                .borrow()
                .has_missing_binds_for(Feature::PaneFocus),
            PageKind::ScrollByCommand => self
                .keybinding_state
                .borrow()
                .has_missing_binds_for(Feature::ScrollByCommand),
            PageKind::Inbox => {
                let state = self.keybinding_state.borrow();
                state.has_missing_binds_for(Feature::Inbox)
                    || state.has_missing_binds_for(Feature::SessionHistory)
            },
            _ => false,
        };
        is_confirmation_page && self.keybinding_state.borrow().status() == &ApplyStatus::NotApplied
    }
    fn confirm_keybindings(&mut self) {
        let needs_conflict_confirmation = self.active_page.kind != PageKind::UpdateKeybindings
            && self.keybinding_state.borrow().has_conflicts();
        if needs_conflict_confirmation {
            self.active_page = self.update_keybindings_screen();
        } else {
            apply_missing_binds(&self.keybinding_state);
            self.refresh_active_page();
        }
    }
    fn refresh_active_page(&mut self) {
        match self.active_page.kind {
            PageKind::MainScreen => {
                self.active_page = self.main_screen();
            },
            PageKind::NestedSessions => {
                self.active_page = Page::new_nested_sessions(
                    self.keybinding_state.clone(),
                    self.main_screen_builder(),
                );
            },
            PageKind::PaneFocus => {
                self.active_page =
                    Page::new_pane_focus(self.keybinding_state.clone(), self.main_screen_builder());
            },
            PageKind::ScrollByCommand => {
                self.active_page = Page::new_scroll_by_command(
                    self.keybinding_state.clone(),
                    self.main_screen_builder(),
                );
            },
            PageKind::UpdateKeybindings => {
                self.active_page = self.update_keybindings_screen();
            },
            PageKind::DesktopApp => {
                self.active_page = Page::new_desktop_app(self.facts.clone());
            },
            PageKind::Inbox => {
                self.active_page =
                    Page::new_inbox(self.keybinding_state.clone(), self.main_screen_builder());
            },
            PageKind::Other => {},
        }
    }
    pub fn change_own_title(&mut self) {
        if let Some(own_plugin_id) = self.own_plugin_id {
            if self.special == Special::WindowWelcome {
                rename_plugin_pane(own_plugin_id, "Welcome");
            } else if self.special == Special::Repair {
                rename_plugin_pane(own_plugin_id, "Zellij launcher");
            } else if self.is_release_notes {
                rename_plugin_pane(
                    own_plugin_id,
                    format!("Release Notes {}", self.zellij_version.borrow()),
                );
            } else {
                rename_plugin_pane(own_plugin_id, "About Zellij");
            }
        }
    }
    pub fn query_link_executable(&self) {
        let mut xdg_open_context = BTreeMap::new();
        xdg_open_context.insert("xdg_open_cli".to_owned(), String::new());
        run_command(&["xdg-open", "--help"], xdg_open_context);
        let mut open_context = BTreeMap::new();
        open_context.insert("open_cli".to_owned(), String::new());
        run_command(&["open", "--help"], open_context);
    }
    pub fn update_link_executable(&mut self, new_link_executable: String) {
        *self.link_executable.borrow_mut() = new_link_executable;
    }
    pub fn update_base_mode(&mut self, new_base_mode: InputMode) -> bool {
        let mut should_render = false;
        if *self.base_mode.borrow() != new_base_mode {
            should_render = true;
        }
        *self.base_mode.borrow_mut() = new_base_mode;
        should_render
    }
    pub fn handle_mouse_event(&mut self, mouse_event: Mouse) -> bool {
        let mut should_render = false;
        match mouse_event {
            Mouse::LeftClick(line, column) => {
                if let Some(new_page) = self
                    .active_page
                    .handle_mouse_left_click(column, line as usize)
                {
                    self.active_page = new_page;
                    should_render = true;
                }
            },
            Mouse::Hover(line, column) => {
                should_render = self.active_page.handle_mouse_hover(column, line as usize);
            },
            _ => {},
        }
        should_render
    }
    fn handle_special_key(&mut self, key: &KeyWithModifier) -> Option<bool> {
        if !key.has_no_modifiers() {
            return None;
        }
        let kind = self.active_page.kind;
        match (key.bare_key, &self.special) {
            (BareKey::Char('y'), Special::Repair) => {
                request_desktop_integration(&Request::Repair(RepairAnswer::Yes));
                return Some(true);
            },
            (BareKey::Char('n'), Special::Repair) | (BareKey::Esc, Special::Repair) => {
                request_desktop_integration(&Request::Repair(RepairAnswer::No));
                close_self();
                return Some(true);
            },
            (BareKey::Char('v'), Special::Repair) => {
                request_desktop_integration(&Request::Repair(RepairAnswer::Never));
                close_self();
                return Some(true);
            },
            _ => {},
        }
        if self.is_startup_tip || self.special != Special::None {
            return None;
        }
        match key.bare_key {
            BareKey::Char('o') if kind == PageKind::MainScreen => {
                self.opt_out();
                Some(true)
            },
            BareKey::Char('s') if kind == PageKind::MainScreen || kind == PageKind::DesktopApp => {
                onboarding::open_setup();
                Some(true)
            },
            BareKey::Char('w') if kind == PageKind::DesktopApp => {
                let facts = self.facts.borrow().clone();
                if !facts.in_desktop_terminal {
                    if let Some(session_name) = facts.session_name {
                        open_session_terminal(&session_name, false);
                    }
                }
                Some(true)
            },
            BareKey::Char('i') if kind == PageKind::DesktopApp => {
                if self.facts.borrow().launcher_installable {
                    request_desktop_integration(&Request::Apply {
                        install: vec![Item::Launcher],
                        remove: vec![],
                        overwrite_binary: false,
                    });
                }
                Some(true)
            },
            _ => None,
        }
    }
    pub fn handle_key(&mut self, key: KeyWithModifier) -> bool {
        if let Some(should_render) = self.handle_special_key(&key) {
            return should_render;
        }
        let mut should_render = false;
        if key.bare_key == BareKey::Up && key.has_no_modifiers() && self.is_startup_tip {
            self.previous_tip();
            should_render = true;
        } else if key.bare_key == BareKey::Down && key.has_no_modifiers() && self.is_startup_tip {
            self.next_tip();
            should_render = true;
        } else if key.bare_key == BareKey::Enter && key.has_no_modifiers() {
            if let Some(new_page) = self.active_page.handle_selection() {
                self.active_page = new_page;
                should_render = true;
            }
        } else if key.bare_key == BareKey::Char('c')
            && key.has_modifiers(&[KeyModifier::Ctrl])
            && self.is_startup_tip
        {
            self.waiting_for_config_to_be_written = true;
            let save_configuration = true;
            reconfigure("show_startup_tips false".to_owned(), save_configuration);
        } else if key.bare_key == BareKey::Char('u')
            && key.has_no_modifiers()
            && !self.is_startup_tip
            && self.keybinding_state.borrow().has_missing_binds()
        {
            self.active_page = self.update_keybindings_screen();
            should_render = true;
        } else if key.bare_key == BareKey::Char('y')
            && key.has_no_modifiers()
            && self.awaiting_keybinding_confirmation()
        {
            self.confirm_keybindings();
            should_render = true;
        } else if key.bare_key == BareKey::Char('n')
            && key.has_no_modifiers()
            && self.awaiting_keybinding_confirmation()
        {
            self.active_page = self.main_screen();
            should_render = true;
        } else if key.bare_key == BareKey::Esc && key.has_no_modifiers() {
            if self.active_page.is_main_screen {
                close_self();
            } else {
                self.active_page = self.main_screen();
                should_render = true;
            }
        } else if key.bare_key == BareKey::Char('?')
            && !self.is_release_notes
            && !self.is_startup_tip
        {
            self.is_startup_tip = true;
            self.active_page = Page::new_tip_screen(
                self.link_executable.clone(),
                self.base_mode.clone(),
                self.tip_index,
            );
            should_render = true;
        } else {
            should_render = self.active_page.handle_key(key);
        }
        should_render
    }
    fn render_error(&self, rows: usize, cols: usize, error: String) {
        let mut error_page = Page::new()
            .main_screen()
            .with_title(Text::from(format!("{}", error)).color_range(3, ..))
            .with_paragraph(vec![
                ComponentLine::new(vec![ActiveComponent::new(TextOrCustomRender::Text(
                    Text::from("Unable to permanently dismiss tips."),
                ))]),
                ComponentLine::new(vec![ActiveComponent::new(TextOrCustomRender::Text(
                    Text::from("You can do so manually by adding the following to your config:"),
                ))]),
            ])
            .with_paragraph(vec![ComponentLine::new(vec![ActiveComponent::new(
                TextOrCustomRender::Text(Text::from("show_startup_tips false").color_range(0, ..)),
            )])])
            .with_paragraph(vec![ComponentLine::new(vec![ActiveComponent::new(
                TextOrCustomRender::Text(Text::from(HOME_MANAGER_NOTE)),
            )])])
            .with_help(Box::new(|_hovering_over_link, _menu_item_is_selected| {
                Text::from("<ESC> - dismiss").color_range(1, ..=4)
            }));
        error_page.render(rows, cols, &None)
    }
    fn center_own_pane(&mut self, tab_info: Vec<TabInfo>) {
        // we only take the size of the first tab because at the time of writing this is
        // identical to all tabs, but this might not always be the case...
        if let Some(first_tab) = tab_info.get(0) {
            let prev_tab_columns = self.tab_columns;
            let prev_tab_rows = self.tab_rows;
            self.tab_columns = first_tab.display_area_columns;
            self.tab_rows = first_tab.display_area_rows;
            if self.tab_columns != prev_tab_columns || self.tab_rows != prev_tab_rows {
                let desired_x_coords = self.tab_columns.saturating_sub(UI_COLUMNS) / 2;
                let desired_y_coords = self.tab_rows.saturating_sub(UI_ROWS) / 2;
                change_floating_panes_coordinates(vec![(
                    PaneId::Plugin(self.own_plugin_id.unwrap()),
                    FloatingPaneCoordinates::new(
                        Some(desired_x_coords.to_string()),
                        Some(desired_y_coords.to_string()),
                        Some(UI_COLUMNS.to_string()),
                        Some(UI_ROWS.to_string()),
                        None,
                        Some(false),
                    )
                    .unwrap(),
                )]);
            }
        }
    }
    fn previous_tip(&mut self) {
        if self.tip_index == 0 {
            self.tip_index = MAX_TIP_INDEX;
        } else {
            self.tip_index = self.tip_index.saturating_sub(1);
        }
        self.active_page = Page::new_tip_screen(
            self.link_executable.clone(),
            self.base_mode.clone(),
            self.tip_index,
        );
    }
    fn next_tip(&mut self) {
        if self.tip_index == MAX_TIP_INDEX {
            self.tip_index = 0;
        } else {
            self.tip_index += 1;
        }
        self.active_page = Page::new_tip_screen(
            self.link_executable.clone(),
            self.base_mode.clone(),
            self.tip_index,
        );
    }
}
