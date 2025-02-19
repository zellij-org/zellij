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

struct State {
    notification: Option<String>,
    is_setup_wizard: bool,
    ui_size: usize,
    current_screen: Screen,
    latest_mode_info: Option<ModeInfo>,
    colors: Styling,
}

impl Default for State {
    fn default() -> Self {
        State {
            notification: None,
            is_setup_wizard: false,
            ui_size: UI_SIZE,
            current_screen: Screen::default(),
            latest_mode_info: None,
            colors: Palette::default().into(),
        }
    }
}

register_plugin!(State);

impl ZellijPlugin for State {
    fn load(&mut self, configuration: BTreeMap<String, String>) {
        self.is_setup_wizard = configuration
            .get("is_setup_wizard")
            .map(|v| v == "true")
            .unwrap_or(false);
        subscribe(&[
            EventType::Key,
            EventType::FailedToWriteConfigToDisk,
            EventType::ModeUpdate,
        ]);
        let own_plugin_id = get_plugin_ids().plugin_id;
        if self.is_setup_wizard {
            self.ui_size = 18;
            self.current_screen = Screen::new_reset_keybindings_screen(Some(0));
            rename_plugin_pane(own_plugin_id, "First Run Setup Wizard (Step 1/1)");
            resize_focused_pane(Resize::Increase);
            resize_focused_pane(Resize::Increase);
            resize_focused_pane(Resize::Increase);
        } else {
            rename_plugin_pane(own_plugin_id, "Configuration");
        }
    }
    fn update(&mut self, event: Event) -> bool {
        let mut should_render = false;
        match event {
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
                    should_render = match &mut self.current_screen {
                        Screen::RebindLeaders(rebind_leaders_screen) => {
                            rebind_leaders_screen.handle_key(key)
                        },
                        Screen::Presets(presets_screen) => {
                            if self.is_setup_wizard {
                                presets_screen.handle_setup_wizard_key(key)
                            } else {
                                presets_screen.handle_presets_key(key)
                            }
                        },
                    };
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
        let notification = self.notification.clone();
        if self.is_in_main_screen() {
            top_tab_menu(cols, &self.current_screen, &self.colors);
        }
        match &mut self.current_screen {
            Screen::RebindLeaders(rebind_leaders_screen) => {
                rebind_leaders_screen.render(rows, cols, self.ui_size, &notification);
            },
            Screen::Presets(presets_screen) => {
                if self.is_setup_wizard {
                    presets_screen.render_setup_wizard_screen(
                        rows,
                        cols,
                        self.ui_size,
                        &notification,
                    )
                } else {
                    presets_screen.render_reset_keybindings_screen(
                        rows,
                        cols,
                        self.ui_size,
                        &notification,
                    )
                }
            },
        };
    }
}

impl State {
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
