use std::fmt::{Display, Error, Formatter};

use zellij_utils::{
    data::{PaletteColor, Styling},
    errors::prelude::*,
};

#[derive(Debug, Clone)]
pub enum LoadingStatus {
    InProgress,
    Success,
    NotFound,
}

#[derive(Debug, Clone, Default)]
pub struct LoadingIndication {
    pub ended: bool,
    loading_from_memory: Option<LoadingStatus>,
    loading_from_hd_cache: Option<LoadingStatus>,
    compiling: Option<LoadingStatus>,
    starting_plugin: Option<LoadingStatus>,
    writing_plugin_to_cache: Option<LoadingStatus>,
    cloning_plugin_for_other_clients: Option<LoadingStatus>,
    error: Option<String>,
    animation_offset: usize,
    plugin_name: String,
    terminal_emulator_colors: Option<Styling>,
    override_previous_error: bool,
}

impl LoadingIndication {
    pub fn new(plugin_name: String) -> Self {
        LoadingIndication {
            plugin_name,
            animation_offset: 0,
            ..Default::default()
        }
    }
    pub fn set_name(&mut self, plugin_name: String) {
        self.plugin_name = plugin_name;
    }
    pub fn with_colors(mut self, terminal_emulator_colors: Styling) -> Self {
        self.terminal_emulator_colors = Some(terminal_emulator_colors);
        self
    }
    pub fn merge(&mut self, other: LoadingIndication) {
        let current_animation_offset = self.animation_offset;
        let current_terminal_emulator_colors = self.terminal_emulator_colors.take();
        let mut current_error = self.error.take();
        let override_previous_error = other.override_previous_error;
        drop(std::mem::replace(self, other));
        self.animation_offset = current_animation_offset;
        self.terminal_emulator_colors = current_terminal_emulator_colors;
        if let Some(current_error) = current_error.take() {
            // we do this so that only the first error (usually the root cause) will be shown
            // when plugins support scrolling, we might want to do an append here
            if !override_previous_error {
                self.error = Some(current_error);
            }
        }
    }
    pub fn indicate_loading_plugin_from_memory(&mut self) {
        self.loading_from_memory = Some(LoadingStatus::InProgress);
    }
    pub fn indicate_loading_plugin_from_memory_success(&mut self) {
        self.loading_from_memory = Some(LoadingStatus::Success);
    }
    pub fn indicate_loading_plugin_from_memory_notfound(&mut self) {
        self.loading_from_memory = Some(LoadingStatus::NotFound);
    }
    pub fn indicate_loading_plugin_from_hd_cache(&mut self) {
        self.loading_from_hd_cache = Some(LoadingStatus::InProgress);
    }
    pub fn indicate_loading_plugin_from_hd_cache_success(&mut self) {
        self.loading_from_hd_cache = Some(LoadingStatus::Success);
    }
    pub fn indicate_loading_plugin_from_hd_cache_notfound(&mut self) {
        self.loading_from_hd_cache = Some(LoadingStatus::NotFound);
    }
    pub fn indicate_compiling_plugin(&mut self) {
        self.compiling = Some(LoadingStatus::InProgress);
    }
    pub fn indicate_compiling_plugin_success(&mut self) {
        self.compiling = Some(LoadingStatus::Success);
    }
    pub fn indicate_starting_plugin(&mut self) {
        self.starting_plugin = Some(LoadingStatus::InProgress);
    }
    pub fn indicate_starting_plugin_success(&mut self) {
        self.starting_plugin = Some(LoadingStatus::Success);
    }
    pub fn indicate_writing_plugin_to_cache(&mut self) {
        self.writing_plugin_to_cache = Some(LoadingStatus::InProgress);
    }
    pub fn indicate_writing_plugin_to_cache_success(&mut self) {
        self.writing_plugin_to_cache = Some(LoadingStatus::Success);
    }
    pub fn indicate_cloning_plugin_for_other_clients(&mut self) {
        self.cloning_plugin_for_other_clients = Some(LoadingStatus::InProgress);
    }
    pub fn indicate_cloning_plugin_for_other_clients_success(&mut self) {
        self.cloning_plugin_for_other_clients = Some(LoadingStatus::Success);
    }
    pub fn end(&mut self) {
        self.ended = true;
    }
    pub fn progress_animation_offset(&mut self) {
        if self.animation_offset == 3 {
            self.animation_offset = 0;
        } else {
            self.animation_offset += 1;
        }
    }
    pub fn indicate_loading_error(&mut self, error_text: String) {
        self.error = Some(error_text);
    }
    pub fn is_error(&self) -> bool {
        self.error.is_some()
    }
    pub fn override_previous_error(&mut self) {
        self.override_previous_error = true;
    }
    fn started_loading(&self) -> bool {
        self.loading_from_memory.is_some()
            || self.loading_from_hd_cache.is_some()
            || self.compiling.is_some()
            || self.starting_plugin.is_some()
            || self.writing_plugin_to_cache.is_some()
            || self.cloning_plugin_for_other_clients.is_some()
    }
}

macro_rules! style {
    ($fg:expr) => {
        ansi_term::Style::new().fg(match $fg {
            PaletteColor::Rgb((r, g, b)) => ansi_term::Color::RGB(r, g, b),
            PaletteColor::EightBit(color) => ansi_term::Color::Fixed(color),
        })
    };
}

impl Display for LoadingIndication {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        let cyan = match self.terminal_emulator_colors {
            Some(terminal_emulator_colors) => {
                style!(terminal_emulator_colors.exit_code_success[1]).bold()
            },
            None => ansi_term::Style::new(),
        };
        let green = match self.terminal_emulator_colors {
            Some(terminal_emulator_colors) => {
                style!(terminal_emulator_colors.exit_code_success[0]).bold()
            },
            None => ansi_term::Style::new(),
        };
        let yellow = match self.terminal_emulator_colors {
            Some(terminal_emulator_colors) => {
                style!(terminal_emulator_colors.exit_code_error[1]).bold()
            },
            None => ansi_term::Style::new(),
        };
        let red = match self.terminal_emulator_colors {
            Some(terminal_emulator_colors) => {
                style!(terminal_emulator_colors.exit_code_error[0]).bold()
            },
            None => ansi_term::Style::new(),
        };
        let bold = ansi_term::Style::new().bold().italic();
        let plugin_name = &self.plugin_name;
        let success = green.paint("SUCCESS");
        let failure = red.paint("FAILED");
        let not_found = yellow.paint("NOT FOUND");
        let add_dots = |stringified: &mut String| {
            for _ in 0..self.animation_offset {
                stringified.push('.');
            }
            stringified.push(' ');
        };
        let mut stringified = String::new();
        let loading_text = "Loading";
        let loading_from_memory_text = "Attempting to load from memory";
        let loading_from_hd_cache_text = "Attempting to load from cache";
        let compiling_text = "Compiling WASM";
        let starting_plugin_text = "Starting";
        let writing_plugin_to_cache_text = "Writing to cache";
        let cloning_plugin_for_other_clients_text = "Cloning for other clients";
        if self.started_loading() {
            stringified.push_str(&format!("{} {}...", loading_text, cyan.paint(plugin_name)));
        } else {
            stringified.push_str(&format!(
                "{} {}",
                bold.paint(loading_text),
                cyan.italic().paint(plugin_name)
            ));
            add_dots(&mut stringified);
        }
        match self.loading_from_memory {
            Some(LoadingStatus::InProgress) => {
                stringified.push_str(&format!("\n\r{}", bold.paint(loading_from_memory_text)));
                add_dots(&mut stringified);
            },
            Some(LoadingStatus::Success) => {
                stringified.push_str(&format!("\n\r{loading_from_memory_text}... {success}"));
            },
            Some(LoadingStatus::NotFound) => {
                stringified.push_str(&format!("\n\r{loading_from_memory_text}... {not_found}"));
            },
            None => {},
        }
        match self.loading_from_hd_cache {
            Some(LoadingStatus::InProgress) => {
                stringified.push_str(&format!("\n\r{}", bold.paint(loading_from_hd_cache_text)));
                add_dots(&mut stringified);
            },
            Some(LoadingStatus::Success) => {
                stringified.push_str(&format!("\n\r{loading_from_hd_cache_text}... {success}"));
            },
            Some(LoadingStatus::NotFound) => {
                stringified.push_str(&format!("\n\r{loading_from_hd_cache_text}... {not_found}"));
            },
            None => {},
        }
        match self.compiling {
            Some(LoadingStatus::InProgress) => {
                stringified.push_str(&format!("\n\r{}", bold.paint(compiling_text)));
                add_dots(&mut stringified);
            },
            Some(LoadingStatus::Success) => {
                stringified.push_str(&format!("\n\r{compiling_text}... {success}"));
            },
            Some(LoadingStatus::NotFound) => {
                stringified.push_str(&format!("\n\r{compiling_text}... {failure}"));
            },
            None => {},
        }
        match self.starting_plugin {
            Some(LoadingStatus::InProgress) => {
                stringified.push_str(&format!("\n\r{}", bold.paint(starting_plugin_text)));
                add_dots(&mut stringified);
            },
            Some(LoadingStatus::Success) => {
                stringified.push_str(&format!("\n\r{starting_plugin_text}... {success}"));
            },
            Some(LoadingStatus::NotFound) => {
                stringified.push_str(&format!("\n\r{starting_plugin_text}... {failure}"));
            },
            None => {},
        }
        match self.writing_plugin_to_cache {
            Some(LoadingStatus::InProgress) => {
                stringified.push_str(&format!("\n\r{}", bold.paint(writing_plugin_to_cache_text)));
                add_dots(&mut stringified);
            },
            Some(LoadingStatus::Success) => {
                stringified.push_str(&format!("\n\r{writing_plugin_to_cache_text}... {success}"));
            },
            Some(LoadingStatus::NotFound) => {
                stringified.push_str(&format!("\n\r{writing_plugin_to_cache_text}... {failure}"));
            },
            None => {},
        }
        match self.cloning_plugin_for_other_clients {
            Some(LoadingStatus::InProgress) => {
                stringified.push_str(&format!(
                    "\n\r{}",
                    bold.paint(cloning_plugin_for_other_clients_text)
                ));
                add_dots(&mut stringified);
            },
            Some(LoadingStatus::Success) => {
                stringified.push_str(&format!(
                    "\n\r{cloning_plugin_for_other_clients_text}... {success}"
                ));
            },
            Some(LoadingStatus::NotFound) => {
                stringified.push_str(&format!(
                    "\n\r{cloning_plugin_for_other_clients_text}... {failure}"
                ));
            },
            None => {},
        }
        if let Some(error_text) = &self.error {
            stringified.push_str(&format!(
                "\n\r{} {}",
                red.bold().paint("ERROR: "),
                error_text.replace('\n', "\n\r")
            ));
            // we add this additional line explicitly to make it easier to realize when something
            // is wrong in very small plugins (eg. the tab-bar and status-bar)
            stringified.push_str(&format!(
                "\n\r{}",
                red.bold()
                    .paint("ERROR IN PLUGIN - check logs for more info")
            ));
        }
        write!(f, "{}", stringified)
    }
}
