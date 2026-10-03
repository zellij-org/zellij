mod action_picker;
mod blocks_screen;
mod keybindings_screen;
mod keys_screen;
mod list_editor;
mod page;
mod settings;
mod settings_screen;
mod theme_preview;
mod themes_screen;
mod ui_components;

use zellij_tile::prelude::*;

use keys_screen::KeysScreen;
use settings_screen::SettingsScreen;
use ui_components::set_close_directly;

use std::collections::BTreeMap;

const SETUP_WIZARD_WIDTH: usize = 72;
const SETUP_WIZARD_HEIGHT: usize = 18;

enum Screen {
    SetupWizard(KeysScreen),
    Settings(SettingsScreen),
}

impl Default for Screen {
    fn default() -> Self {
        Screen::SetupWizard(KeysScreen::new(true))
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
                EventType::Mouse,
                EventType::Timer,
                EventType::FailedToWriteConfigToDisk,
                EventType::AvailableKeybindPresets,
            ]);
            set_close_directly(true);
            let mut keys_screen = KeysScreen::new(true);
            keys_screen.set_snapshot(&read_config());
            self.screen = Screen::SetupWizard(keys_screen);
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
                EventType::AvailableKeybindPresets,
                EventType::ConfigFileChangedSinceRead,
                EventType::BeforeClose,
                EventType::Visible,
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
            Screen::SetupWizard(keys_screen) => match event {
                Event::AvailableKeybindPresets(presets, errors) => {
                    keys_screen.set_presets(presets, errors);
                    true
                },
                Event::Key(key) => {
                    if self.notification.is_some() {
                        self.notification = None;
                        return true;
                    }
                    let should_render = keys_screen.handle_key(key);
                    if keys_screen.take_needs_refresh() {
                        keys_screen.set_snapshot(&read_config());
                    }
                    should_render
                },
                Event::Mouse(mouse) => {
                    let should_render = keys_screen.handle_mouse(mouse);
                    let refreshed = keys_screen.take_needs_refresh();
                    if refreshed {
                        keys_screen.set_snapshot(&read_config());
                    }
                    should_render || refreshed
                },
                Event::Timer(_) => keys_screen.handle_timer(),
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
                Event::AvailableKeybindPresets(presets, errors) => {
                    settings_screen.update_keybind_presets(presets, errors);
                    true
                },
                Event::ConfigFileChangedSinceRead => {
                    settings_screen.config_file_changed_since_read();
                    true
                },
                Event::BeforeClose => {
                    settings_screen.before_close();
                    false
                },
                Event::Visible(false) => {
                    settings_screen.hidden();
                    true
                },
                _ => false,
            },
        }
    }
    fn render(&mut self, rows: usize, cols: usize) {
        page::clear_overlays();
        match &mut self.screen {
            Screen::SetupWizard(keys_screen) => {
                if self.notification.is_some() {
                    keys_screen.set_notice(self.notification.clone());
                }
                let width = SETUP_WIZARD_WIDTH.min(cols);
                let height = SETUP_WIZARD_HEIGHT.min(rows);
                let x = cols.saturating_sub(width) / 2;
                let y = rows.saturating_sub(height) / 2;
                keys_screen.render(x, y, width, height);
                keys_screen.render_overlays(rows, cols);
            },
            Screen::Settings(settings_screen) => settings_screen.render(rows, cols),
        }
    }
}
