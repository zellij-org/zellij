mod action_picker;
mod bindings_list;
mod blocks_screen;
mod contrast;
mod keybindings_screen;
mod keys_screen;
mod list_editor;
mod mousebindings_screen;
mod page;
mod settings;
mod settings_screen;
mod setup_screen;
mod theme_preview;
mod theme_sample;
mod themes_screen;
mod ui_components;

use zellij_tile::prelude::*;

use settings_screen::SettingsScreen;
use setup_screen::SetupScreen;
use ui_components::set_close_directly;

use std::collections::BTreeMap;

enum Screen {
    Loading,
    SetupWizard(Box<SetupScreen>),
    Settings(SettingsScreen),
}

impl Default for Screen {
    fn default() -> Self {
        Screen::Loading
    }
}

#[derive(Default)]
struct State {
    screen: Screen,
}

const WIZARD_EVENTS: [EventType; 8] = [
    EventType::Key,
    EventType::Mouse,
    EventType::Timer,
    EventType::FailedToWriteConfigToDisk,
    EventType::ConfigWasWrittenToDisk,
    EventType::AvailableKeybindPresets,
    EventType::DesktopIntegration,
    EventType::PromptResult,
];

register_plugin!(State);

impl ZellijPlugin for State {
    fn load(&mut self, configuration: BTreeMap<String, String>) {
        let is_setup_wizard = configuration
            .get("is_setup_wizard")
            .map(|v| v == "true")
            .unwrap_or(false);
        let rerun_setup = configuration
            .get("rerun_setup")
            .map(|v| v == "true")
            .unwrap_or(false);
        let own_plugin_id = get_plugin_ids().plugin_id;
        if is_setup_wizard || rerun_setup {
            subscribe(&WIZARD_EVENTS);
            set_close_directly(true);
            self.screen = Screen::SetupWizard(Box::new(SetupScreen::new(
                is_setup_wizard,
                own_plugin_id,
            )));
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
                EventType::PromptResult,
                EventType::DesktopIntegration,
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
        let should_render = match &mut self.screen {
            Screen::Loading => false,
            Screen::SetupWizard(setup) => match event {
                Event::AvailableKeybindPresets(presets, errors) => {
                    setup.keys_mut().set_presets(presets, errors);
                    true
                },
                Event::DesktopIntegration(payload) => setup.desktop_integration(payload),
                Event::PromptResult(request_id, result) => setup.prompt_result(request_id, result),
                Event::Key(key) => setup.handle_key(key),
                Event::Mouse(mouse) => setup.handle_mouse(mouse),
                Event::Timer(_) => setup.handle_timer(),
                Event::ConfigWasWrittenToDisk => setup.config_written(),
                Event::FailedToWriteConfigToDisk(config_file_path) => {
                    setup.config_write_failed(config_file_path)
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
                Event::PromptResult(request_id, result) => {
                    settings_screen.prompt_result(request_id, result)
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
        };
        if let Screen::Settings(settings_screen) = &mut self.screen {
            if settings_screen.take_setup_request() {
                set_close_directly(true);
                let own_plugin_id = get_plugin_ids().plugin_id;
                self.screen = Screen::SetupWizard(Box::new(SetupScreen::new(
                    false,
                    own_plugin_id,
                )));
                return true;
            }
        }
        should_render
    }
    fn render(&mut self, rows: usize, cols: usize) {
        page::clear_overlays();
        match &mut self.screen {
            Screen::Loading => {},
            Screen::SetupWizard(setup) => setup.render(rows, cols),
            Screen::Settings(settings_screen) => settings_screen.render(rows, cols),
        }
    }
}
