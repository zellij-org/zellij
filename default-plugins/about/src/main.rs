mod presets;
mod presets_screen;
mod rebind_leaders_screen;
mod ui_components;

use zellij_tile::prelude::*;

use presets_screen::PresetsScreen;
use rebind_leaders_screen::RebindLeadersScreen;
use ui_components::top_tab_menu;

use std::collections::BTreeMap;

pub static UI_SIZE: usize = 15;
pub static WIDTH_BREAKPOINTS: (usize, usize) = (62, 35);
pub static POSSIBLE_MODIFIERS: [KeyModifier; 4] = [
    KeyModifier::Ctrl,
    KeyModifier::Alt,
    KeyModifier::Super,
    KeyModifier::Shift,
];

#[derive(Debug)]
enum Screen {
    RebindLeaders(RebindLeadersScreen),
    Presets(PresetsScreen),
}

impl Screen {
    pub fn reset_state(&mut self, is_setup_wizard: bool) {
        if is_setup_wizard {
            Screen::new_reset_keybindings_screen(Some(0));
        } else {
            match self {
                Screen::RebindLeaders(r) => {
                    let notification = r.drain_notification();
                    *r = Default::default();
                    r.set_notification(notification);
                },
                Screen::Presets(r) => {
                    let notification = r.drain_notification();
                    *r = Default::default();
                    r.set_notification(notification);
                },
            }
        }
    }
    pub fn update_mode_info(&mut self, latest_mode_info: ModeInfo) {
        match self {
            Screen::RebindLeaders(r) => r.update_mode_info(latest_mode_info),
            Screen::Presets(r) => r.update_mode_info(latest_mode_info),
        }
    }
}

impl Default for Screen {
    fn default() -> Self {
        Screen::RebindLeaders(Default::default())
    }
}

impl Screen {
    pub fn new_reset_keybindings_screen(selected_index: Option<usize>) -> Self {
        Screen::Presets(PresetsScreen::new(selected_index))
    }
}

struct LinkCoordinates {
    x: usize,
    y: usize,
    width: usize,
    destination_url: String,
}

impl LinkCoordinates {
    pub fn new(x: usize, y: usize, width: usize, destination_url: &str) -> Self {
        LinkCoordinates {
            x,
            y,
            width,
            destination_url: destination_url.to_owned()
        }
    }
    pub fn contains(&self, line: isize, col: usize) -> bool {
        line == self.y as isize && col >= self.x && col < self.x + self.width
    }
}

struct State {
    version: String,
    link_coordinates: Vec<LinkCoordinates>,
    hover_coordinates: Option<(isize, usize)>, // line/col
    selected_item_index: Option<usize>,
    notification: Option<String>,
    is_setup_wizard: bool,
    is_release_notes: bool,
    ui_size: usize,
    current_screen: Screen,
    latest_mode_info: Option<ModeInfo>,
    colors: Palette,
}

impl Default for State {
    fn default() -> Self {
        State {
            version: String::from("0.42.0"), // TODO: from Zellij
            link_coordinates: vec![],
            hover_coordinates: None,
            selected_item_index: None,
            notification: None,
            is_setup_wizard: false,
            is_release_notes: false,
            ui_size: UI_SIZE,
            current_screen: Screen::default(),
            latest_mode_info: None,
            colors: Palette::default(),
        }
    }
}

register_plugin!(State);

impl ZellijPlugin for State {
    fn load(&mut self, configuration: BTreeMap<String, String>) {
        self.is_release_notes = configuration
            .get("is_release_notes")
            .map(|v| v == "true")
            .unwrap_or(false);
        subscribe(&[
            EventType::Key,
            EventType::Mouse,
            EventType::FailedToWriteConfigToDisk,
            EventType::ModeUpdate,
        ]);
        let own_plugin_id = get_plugin_ids().plugin_id;
        if self.is_release_notes {
            rename_plugin_pane(own_plugin_id, format!("Release Notes {}", self.version));
        } else {
            rename_plugin_pane(own_plugin_id, "About Zellij");
        }
    }
    fn update(&mut self, event: Event) -> bool {
        let mut should_render = false;
        match event {
            Event::Mouse(mouse_event) => {
                match mouse_event {
                    Mouse::LeftClick(line, col) => {
                        for link_coordinates in &self.link_coordinates {
                            if link_coordinates.contains(line, col) {
                                eprintln!("can has click on link!");
                                run_command(
                                    &["xdg-open", &link_coordinates.destination_url],
                                     Default::default()
                                );
                                break;
                            }
                        }
                    }
                    Mouse::Hover(line, col) => {
                        let mut contained_in_link = false;
                        for link_coordinates in &self.link_coordinates {
                            if link_coordinates.contains(line, col) {
                                self.hover_coordinates = Some((line, col));
                                should_render = true;
                                contained_in_link = true;
                                break;
                            }
                        }
                        if !contained_in_link {
                            if self.hover_coordinates.is_some() {
                                // so that we clear the hover indication
                                should_render = true;
                            }
                            self.hover_coordinates = None;
                        }
                    }
                    _ => {}
                }
                eprintln!("mouse_event: {:?}", mouse_event);
            }
            Event::ModeUpdate(mode_info) => {
                self.colors = mode_info.style.colors;
                if self.latest_mode_info.as_ref().and_then(|l| l.base_mode) != mode_info.base_mode {
                    // reset ui state
                    self.current_screen.reset_state(self.is_setup_wizard);
                }
                self.latest_mode_info = Some(mode_info.clone());
                self.current_screen.update_mode_info(mode_info.clone());
                should_render = true;
            },
            Event::Key(key) => {
                if self.notification.is_some() {
                    self.notification = None;
                    should_render = true;
                } else if key.bare_key == BareKey::Tab
                    && key.has_no_modifiers()
                    && !self.is_setup_wizard
                {
                    self.switch_screen();
                    should_render = true;
                } else {
                    if key.bare_key == BareKey::Down && key.has_no_modifiers() {
                        self.move_selection_down();
                        should_render = true;
                    } else if key.bare_key == BareKey::Up && key.has_no_modifiers() {
                        self.move_selection_up();
                        should_render = true;
                    }
//                     should_render = match &mut self.current_screen {
//                         Screen::RebindLeaders(rebind_leaders_screen) => {
//                             rebind_leaders_screen.handle_key(key)
//                         },
//                         Screen::Presets(presets_screen) => {
//                             if self.is_setup_wizard {
//                                 presets_screen.handle_setup_wizard_key(key)
//                             } else {
//                                 presets_screen.handle_presets_key(key)
//                             }
//                         },
//                     };
                }
            },
            Event::FailedToWriteConfigToDisk(config_file_path) => {
                match config_file_path {
                    Some(failed_path) => {
                        self.notification = Some(format!(
                            "Failed to write configuration file: {}",
                            failed_path
                        ));
                    },
                    None => {
                        self.notification = Some(format!("Failed to write configuration file."));
                    },
                }
                should_render = true;
            },
            _ => (),
        };
        should_render
    }
    fn render(&mut self, rows: usize, cols: usize) {
        // TODO: CONTINUE HERE - move this to the render function of a MainScreen, then implement
        // the other screens
        self.link_coordinates.clear();
        let ui_width = 74; // length of please support line
        let ui_height = 12;
        let base_x = cols.saturating_sub(ui_width) / 2;
        let base_y = rows.saturating_sub(ui_height) / 2;

        let title_text = format!("Hi there, welcome to Zellij {}!", self.version);
        let title = Text::new(title_text).color_range(2, 21..=27 + self.version.chars().count());
        print_text_with_coordinates(title, base_x, base_y, None, None);

        let whats_new_text = format!("What's new?");
        let whats_new = Text::new(whats_new_text);
        print_text_with_coordinates(whats_new, base_x, base_y + 2, None, None);

        let stacked_resize_text = format!("1. Stacked resize");
        let mut stacked_resize = Text::new(stacked_resize_text).color_range(0, 3..);
        if self.selected_item_index == Some(0) {
            stacked_resize = stacked_resize.selected();
        }
        print_text_with_coordinates(stacked_resize, base_x, base_y + 3, Some(ui_width), None);

        let pinned_floating_panes_text = format!("2. Pinned floating panes");
        let mut pinned_floating_panes = Text::new(pinned_floating_panes_text).color_range(0, 3..);
        if self.selected_item_index == Some(1) {
            pinned_floating_panes = pinned_floating_panes.selected();
        }
        print_text_with_coordinates(pinned_floating_panes, base_x, base_y + 4, Some(ui_width), None);

        let new_theme_def_text = format!("3. New theme definition spec");
        let mut new_theme_def = Text::new(new_theme_def_text).color_range(0, 3..);
        if self.selected_item_index == Some(2) {
            new_theme_def = new_theme_def.selected();
        }
        print_text_with_coordinates(new_theme_def, base_x, base_y + 5, Some(ui_width), None);

        let new_plugin_apis_text = format!("4. New plugin APIs");
        let mut new_plugin_apis = Text::new(new_plugin_apis_text).color_range(0, 3..);
        if self.selected_item_index == Some(3) {
            new_plugin_apis = new_plugin_apis.selected();
        }
        print_text_with_coordinates(new_plugin_apis, base_x, base_y + 6, Some(ui_width), None);

        let mouse_anyevent_text = format!("5. Mouse AnyEvent Handling");
        let mut mouse_anyevent = Text::new(mouse_anyevent_text).color_range(0, 3..);
        if self.selected_item_index == Some(4) {
            mouse_anyevent = mouse_anyevent.selected();
        }
        print_text_with_coordinates(mouse_anyevent, base_x, base_y + 7, Some(ui_width), None);

        self.render_changelog_link(base_x, base_y + 9);
        self.render_sponsor_link(base_x, base_y + 11);
        self.render_help(rows);


//         let notification = self.notification.clone();
//         if self.is_in_main_screen() {
//             top_tab_menu(cols, &self.current_screen, &self.colors);
//         }
//         match &mut self.current_screen {
//             Screen::RebindLeaders(rebind_leaders_screen) => {
//                 rebind_leaders_screen.render(rows, cols, self.ui_size, &notification);
//             },
//             Screen::Presets(presets_screen) => {
//                 if self.is_setup_wizard {
//                     presets_screen.render_setup_wizard_screen(
//                         rows,
//                         cols,
//                         self.ui_size,
//                         &notification,
//                     )
//                 } else {
//                     presets_screen.render_reset_keybindings_screen(
//                         rows,
//                         cols,
//                         self.ui_size,
//                         &notification,
//                     )
//                 }
//             },
//         };
    }
}

impl State {
    fn render_sponsor_link(&mut self, x: usize, y: usize) {
        let link_coordinates = LinkCoordinates::new(x + 40, y, 34, "https://github.com/sponsors/imsnif");
        if let Some((line, col)) = self.hover_coordinates.as_ref() {
            if link_coordinates.contains(*line, *col) {
                let support_text = format!("Please support the Zellij developer <3:");
                self.link_coordinates.push(link_coordinates);
                let support = Text::new(support_text).color_range(3, 0..=38);
                print_text_with_coordinates(support, x, y, None, None);
                print!("\u{1b}[{};{}H\u{1b}[m\u{1b}[1;4mhttps://github.com/sponsors/imsnif", y + 1, x + 41);
                return;
            }
        }
        let support_text = format!("Please support the Zellij developer <3: https://github.com/sponsors/imsnif");
        self.link_coordinates.push(link_coordinates);
        let support = Text::new(support_text).color_range(3, 0..=38);
        print_text_with_coordinates(support, x, y, None, None);
    }
    fn render_changelog_link(&mut self, x: usize, y: usize) {
        let link_coordinates = LinkCoordinates::new(x + 16, y, 51 + self.version.chars().count(), &format!("https://github.com/zellij-org/zellij/releases/tag/v{}", self.version));
        if let Some((line, col)) = self.hover_coordinates.as_ref() {
            if link_coordinates.contains(*line, *col) {
                self.link_coordinates.push(link_coordinates);
                let full_changelog_text = format!("Full Changelog:");
                let full_changelog = Text::new(full_changelog_text);
                print_text_with_coordinates(full_changelog, x, y, None, None);
                print!("\u{1b}[{};{}H\u{1b}[m\u{1b}[1;4mhttps://github.com/zellij-org/zellij/releases/tag/v{}", y + 1, x + 17, self.version);
                return;
            }
        }
        self.link_coordinates.push(link_coordinates);
        let full_changelog_text = format!("Full Changelog: https://github.com/zellij-org/zellij/releases/tag/v{}", self.version);
        let full_changelog = Text::new(full_changelog_text);
        print_text_with_coordinates(full_changelog, x, y, None, None);
    }
    fn render_help(&self, rows: usize) {
        if self.hover_coordinates.is_some() {
            let help_text = format!("Help: Click or Shift-Click to open in browser");
            let help = Text::new(help_text)
                .color_range(3, 6..=10)
                .color_range(3, 15..=25);
            print_text_with_coordinates(help, 0, rows, None, None);
        } else if self.selected_item_index.is_some() {
            let help_text = format!("Help: <↓↑> - Navigate, <ENTER> - Learn More, <ESC> - Dismiss");
            let help = Text::new(help_text)
                .color_range(1, 6..=9)
                .color_range(1, 23..=29)
                .color_range(1, 45..=49);
            print_text_with_coordinates(help, 0, rows, None, None);
        } else {
            let help_text = format!("Help: <↓↑> - Navigate, <ESC> - Dismiss");
            let help = Text::new(help_text)
                .color_range(1, 6..=9)
                .color_range(1, 23..=27);
            print_text_with_coordinates(help, 0, rows, None, None);
        }
    }
    fn move_selection_down(&mut self) {
        if self.selected_item_index.is_none() {
            self.selected_item_index = Some(0);
        } else if let Some(selected_item_index) = self.selected_item_index.take() {
            if selected_item_index == 4 {
                self.selected_item_index = None;
            } else {
                self.selected_item_index = Some(selected_item_index + 1);
            }
        }
    }
    fn move_selection_up(&mut self) {
        if self.selected_item_index.is_none() {
            self.selected_item_index = Some(4);
        } else if let Some(selected_item_index) = self.selected_item_index.take() {
            if selected_item_index == 0 {
                self.selected_item_index = None;
            } else {
                self.selected_item_index = Some(selected_item_index.saturating_sub(1));
            }
        }
    }
    fn is_in_main_screen(&self) -> bool {
        match &self.current_screen {
            Screen::RebindLeaders(_) => true,
            Screen::Presets(presets_screen) => {
                if self.is_setup_wizard || presets_screen.rebinding_leaders() {
                    false
                } else {
                    true
                }
            },
        }
    }
    fn switch_screen(&mut self) {
        match &self.current_screen {
            Screen::RebindLeaders(_) => {
                self.current_screen = Screen::Presets(Default::default());
            },
            Screen::Presets(_) => {
                self.current_screen = Screen::RebindLeaders(
                    RebindLeadersScreen::default().with_mode_info(self.latest_mode_info.clone()),
                );
            },
        }
        if let Some(mode_info) = &self.latest_mode_info {
            self.current_screen.update_mode_info(mode_info.clone());
        }
    }
}
