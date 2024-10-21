mod ui_components;
mod rebind_leaders_screen;
mod presets_screen;
mod presets;

use zellij_tile::prelude::*;

use ui_components::top_tab_menu;
use rebind_leaders_screen::RebindLeadersScreen;
use presets_screen::PresetsScreen;

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
                Screen::RebindLeaders(r) => *r = Default::default(),
                Screen::Presets(r) => *r = Default::default(),
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
    main_leader: Option<KeyWithModifier>,
    latest_mode_info: Option<ModeInfo>,
}

impl Default for State {
    fn default() -> Self {
        State {
            notification: None,
            is_setup_wizard: false,
            ui_size: UI_SIZE,
            current_screen: Screen::default(),
            main_leader: None,
            latest_mode_info: None,
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
        subscribe(&[EventType::Key, EventType::FailedToWriteConfigToDisk, EventType::ModeUpdate]);
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
                if self.latest_mode_info.as_ref().and_then(|l| l.base_mode) != mode_info.base_mode {
                    // reset ui state
                    self.current_screen.reset_state(self.is_setup_wizard);
                }
                self.latest_mode_info = Some(mode_info.clone());
                self.current_screen.update_mode_info(mode_info.clone());
                if let Some(InputMode::Locked) = mode_info.base_mode {
                    let prev_leader = self.main_leader.take();
                    self.set_main_leader();
                    if prev_leader != self.main_leader {
                        should_render = true;
                    }
                }
            }
            Event::Key(key) => {
                if self.notification.is_some() {
                    self.notification = None;
                    should_render = true;
                } else if key.bare_key == BareKey::Tab && key.has_no_modifiers() && !self.is_setup_wizard {
                    self.switch_screen();
                    should_render = true;
                } else {
                    should_render = match &mut self.current_screen {
                        Screen::RebindLeaders(rebind_leaders_screen) => rebind_leaders_screen.handle_key(key),
                        Screen::Presets(presets_screen) => if self.is_setup_wizard {
                            presets_screen.handle_setup_wizard_key(key)
                        } else {
                            presets_screen.handle_presets_key(key)
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
            top_tab_menu(cols, &self.current_screen);
        }
        match &mut self.current_screen {
            Screen::RebindLeaders(rebind_leaders_screen) => {
                rebind_leaders_screen.render(rows, cols, self.ui_size, &notification);
            }
            Screen::Presets(presets_screen) => if self.is_setup_wizard {
                presets_screen
                    .render_setup_wizard_screen(rows, cols, self.ui_size, &notification)
            } else {
                presets_screen
                    .render_reset_keybindings_screen(rows, cols, self.ui_size, &notification)
            },
        };
    }
}

impl State {
    fn is_in_main_screen(&self) -> bool {
        match &self.current_screen {
            Screen::RebindLeaders(_) => true,
            Screen::Presets(presets_screen) => if self.is_setup_wizard || presets_screen.rebinding_leaders() {
                false
            } else {
                true
            },
        }
    }
    fn set_main_leader(&mut self) {
        self.main_leader = self.latest_mode_info.as_ref().and_then(|mode_info| {
            mode_info
            .keybinds
            .iter()
            .find_map(|m| {
                if m.0 == InputMode::Locked {
                    Some(m.1.clone())
                } else {
                    None
                }
            })
            .and_then(|k| k.into_iter().find_map(|(k, a)| {
                if a == &[actions::Action::SwitchToMode(InputMode::Normal)] {
                    Some(k)
                } else {
                    None
                }
            }))
        });
    }
    fn switch_screen(&mut self) {
        match &self.current_screen {
            Screen::RebindLeaders(_) => {
                self.current_screen = Screen::Presets(Default::default());
            },
            Screen::Presets(_) => {
                self.current_screen = Screen::RebindLeaders(Default::default());
            }
        }
        if let Some(mode_info) = &self.latest_mode_info {
            self.current_screen.update_mode_info(mode_info.clone());
        }
    }
}
