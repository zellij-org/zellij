mod presets;
mod presets_screen;
mod rebind_leaders_screen;
mod settings;
mod settings_screen;
mod ui_components;

use zellij_tile::prelude::*;

use presets_screen::PresetsScreen;
use settings_screen::SettingsScreen;
use ui_components::set_close_directly;

use std::collections::BTreeMap;

pub static UI_SIZE: usize = 15;
pub static WIDTH_BREAKPOINTS: (usize, usize) = (62, 35);
pub static POSSIBLE_MODIFIERS: [KeyModifier; 4] = [
    KeyModifier::Ctrl,
    KeyModifier::Alt,
    KeyModifier::Super,
    KeyModifier::Shift,
];

const SETUP_WIZARD_UI_SIZE: usize = 18;

enum Screen {
    SetupWizard(PresetsScreen),
    Settings(SettingsScreen),
}

impl Default for Screen {
    fn default() -> Self {
        Screen::SetupWizard(PresetsScreen::new(Some(0)))
    }
}

#[derive(Default)]
struct State {
    notification: Option<String>,
    screen: Screen,
}

register_plugin!(State);

impl ZellijPlugin for State {
    fn load(&mut self, configuration: BTreeMap<String, String>) {
        let is_setup_wizard = configuration
            .get("is_setup_wizard")
            .map(|v| v == "true")
            .unwrap_or(false);
        let own_plugin_id = get_plugin_ids().plugin_id;
        if is_setup_wizard {
            subscribe(&[
                EventType::Key,
                EventType::FailedToWriteConfigToDisk,
                EventType::ModeUpdate,
            ]);
            set_close_directly(true);
            self.screen = Screen::SetupWizard(PresetsScreen::new(Some(0)));
            rename_plugin_pane(own_plugin_id, "First Run Setup Wizard (Step 1/1)");
            resize_focused_pane(Resize::Increase);
            resize_focused_pane(Resize::Increase);
            resize_focused_pane(Resize::Increase);
        } else {
            subscribe(&[
                EventType::Key,
                EventType::Mouse,
                EventType::Timer,
                EventType::FailedToWriteConfigToDisk,
                EventType::ConfigWasWrittenToDisk,
                EventType::ConfigChangesDropped,
                EventType::ModeUpdate,
            ]);
            set_close_directly(false);
            self.screen = Screen::Settings(SettingsScreen::new());
            rename_plugin_pane(own_plugin_id, "Configuration");
            if let Some(coordinates) = FloatingPaneCoordinates::new(
                Some("10%".to_owned()),
                Some("10%".to_owned()),
                Some("80%".to_owned()),
                Some("80%".to_owned()),
                None,
                None,
            ) {
                change_floating_panes_coordinates(vec![(
                    PaneId::Plugin(own_plugin_id),
                    coordinates,
                )]);
            }
        }
    }
    fn update(&mut self, event: Event) -> bool {
        match &mut self.screen {
            Screen::SetupWizard(presets_screen) => match event {
                Event::ModeUpdate(mode_info) => {
                    presets_screen.update_mode_info(mode_info);
                    true
                },
                Event::Key(key) => {
                    if self.notification.is_some() {
                        self.notification = None;
                        true
                    } else {
                        presets_screen.handle_setup_wizard_key(key)
                    }
                },
                Event::FailedToWriteConfigToDisk(config_file_path) => {
                    self.notification = Some(match config_file_path {
                        Some(failed_path) => {
                            format!("Failed to write configuration file: {}", failed_path)
                        },
                        None => "Failed to write configuration file.".to_owned(),
                    });
                    true
                },
                _ => false,
            },
            Screen::Settings(settings_screen) => match event {
                Event::ModeUpdate(mode_info) => {
                    settings_screen.update_mode_info(mode_info);
                    true
                },
                Event::Key(key) => settings_screen.handle_key(key),
                Event::Mouse(mouse) => settings_screen.handle_mouse(mouse),
                Event::Timer(_) => settings_screen.handle_timer(),
                Event::ConfigWasWrittenToDisk => {
                    settings_screen.config_written();
                    true
                },
                Event::FailedToWriteConfigToDisk(config_file_path) => {
                    settings_screen.config_write_failed(config_file_path);
                    true
                },
                Event::ConfigChangesDropped(dropped) => {
                    settings_screen.changes_dropped(dropped);
                    true
                },
                _ => false,
            },
        }
    }
    fn render(&mut self, rows: usize, cols: usize) {
        match &mut self.screen {
            Screen::SetupWizard(presets_screen) => presets_screen.render_setup_wizard_screen(
                rows,
                cols,
                SETUP_WIZARD_UI_SIZE,
                self.notification.clone(),
            ),
            Screen::Settings(settings_screen) => settings_screen.render(rows, cols),
        }
    }
}
