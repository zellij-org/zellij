use std::fmt::{Display, Error, Formatter};
use std::time::Instant;

use zellij_utils::{
    data::{PaletteColor, Styling},
    errors::prelude::*,
};

#[derive(Debug, Clone, Default)]
pub struct LoadingIndication {
    pub ended: bool,
    error: Option<String>,
    animation_offset: usize,
    plugin_name: String,
    terminal_emulator_colors: Option<Styling>,
    override_previous_error: bool,
    started_at: Option<Instant>,
}

impl LoadingIndication {
    pub fn new(plugin_name: String) -> Self {
        let started_at = Some(Instant::now());
        LoadingIndication {
            plugin_name,
            animation_offset: 0,
            started_at,
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
                style!(terminal_emulator_colors.exit_code_success.emphasis_0).bold()
            },
            None => ansi_term::Style::new(),
        };
        let red = match self.terminal_emulator_colors {
            Some(terminal_emulator_colors) => {
                style!(terminal_emulator_colors.exit_code_error.base).bold()
            },
            None => ansi_term::Style::new(),
        };
        let plugin_name = &self.plugin_name;
        let add_dots = |stringified: &mut String| {
            for _ in 0..self.animation_offset {
                stringified.push('.');
            }
            stringified.push(' ');
        };
        let mut stringified = String::new();

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
        } else {
            let loading_text = "Loading";
            stringified.push_str(&format!("{} {}", loading_text, cyan.paint(plugin_name)));
            add_dots(&mut stringified);
        }
        if self
            .started_at
            .map(|s| s.elapsed() > std::time::Duration::from_millis(400))
            .unwrap_or(true)
            || self.error.is_some()
        {
            write!(f, "{}", stringified)
        } else {
            Ok(())
        }
    }
}
