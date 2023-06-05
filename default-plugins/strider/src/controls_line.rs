use crate::search::SearchType;
use crate::ui::{GRAY_DARK, GRAY_LIGHT, WHITE, BLACK, RED, arrow, dot, styled_text, color_line_to_end};

#[derive(Default)]
pub struct ControlsLine {
    controls: Vec<Control>,
    scanning_indication: Option<Vec<&'static str>>,
    animation_offset: u8,
}

impl ControlsLine {
    pub fn new(controls: Vec<Control>, scanning_indication: Option<Vec<&'static str>>) -> Self {
        ControlsLine {
            controls,
            scanning_indication,
            ..Default::default()
        }
    }
    pub fn with_animation_offset(mut self, animation_offset: u8) -> Self {
        self.animation_offset = animation_offset;
        self
    }
    pub fn render(&self, max_width: usize) -> String {
        let loading_animation = LoadingAnimation::new(&self.scanning_indication, self.animation_offset);
        let full_length = loading_animation.full_len() + self.controls.iter().map(|c| c.full_len()).sum::<usize>();
        let mid_length = loading_animation.mid_len() + self.controls.iter().map(|c| c.mid_len()).sum::<usize>();
        let short_length = loading_animation.short_len() + self.controls.iter().map(|c| c.short_len()).sum::<usize>();
        if max_width >= full_length {
            let mut to_render = String::new();
            for control in &self.controls {
                to_render.push_str(&control.render_full_length());
            }
            to_render.push_str(&self.render_padding(max_width.saturating_sub(full_length)));
            to_render.push_str(&loading_animation.render_full_length());
            to_render
        } else if max_width >= mid_length {
            let mut to_render = String::new();
            for control in &self.controls {
                to_render.push_str(&control.render_mid_length());
            }
            to_render.push_str(&self.render_padding(max_width.saturating_sub(mid_length)));
            to_render.push_str(&loading_animation.render_mid_length());
            to_render
        } else if max_width >= short_length {
            let mut to_render = String::new();
            for control in &self.controls {
                to_render.push_str(&control.render_short_length());
            }
            to_render.push_str(&self.render_padding(max_width.saturating_sub(short_length)));
            to_render.push_str(&loading_animation.render_short_length());
            to_render
        } else {
            format!("")
        }
    }
    fn render_padding(&self, padding: usize) -> String {
        format!("\u{1b}[{}C", padding)
    }
}

pub struct Control {
    key: &'static str,
    options: Vec<&'static str>,
    option_index: (usize, usize), // eg. 1 out of 2 (1, 2)
    keycode_background_color: u8,
    keycode_foreground_color: u8,
    control_text_background_color: u8,
    control_text_foreground_color: u8,
    active_dot_color: u8,
}

impl Default for Control {
    fn default() -> Self {
        Control {
            key: "",
            options: vec![],
            option_index: (0, 0),
            keycode_background_color: GRAY_LIGHT,
            keycode_foreground_color: WHITE,
            control_text_background_color: GRAY_DARK,
            control_text_foreground_color: BLACK,
            active_dot_color: RED
        }
    }
}

impl Control {
    pub fn new(key: &'static str, options: Vec<&'static str>, option_index: (usize, usize)) -> Self {
        Control { key, options, option_index, ..Default::default() }
    }
    pub fn new_floating_control(key: &'static str, should_open_floating: bool) -> Self {
        if should_open_floating {
            Control::new(
                key,
                vec!["OPEN FLOATING", "FLOATING", "F"],
                (2, 2)
            )
        } else {
            Control::new(
                key,
                vec!["OPEN TILED", "TILED", "T"],
                (1, 2)
            )
        }
    }
    pub fn new_filter_control(key: &'static str, search_filter: &SearchType) -> Self {
        match search_filter {
            SearchType::NamesAndContents => {
                Control::new(
                    key,
                    vec!["FILE NAMES AND CONTENTS", "NAMES + CONTENTS", "N+C"],
                    (1, 3)
                )
            }
            SearchType::Names => {
                Control::new(
                    key,
                    vec!["FILE NAMES", "NAMES", "N"],
                    (2, 3)
                )
            }
            SearchType::Contents => {
                Control::new(
                    key,
                    vec!["FILE CONTENTS", "CONTENTS", "C"],
                    (3, 3)
                )
            }
        }
    }
    pub fn short_len(&self) -> usize {
        let short_text = self.options.get(2).or_else(|| self.options.get(1)).or_else(|| self.options.get(0)).unwrap_or(&"");
        short_text.chars().count() + self.key.chars().count() + self.option_index.1 + 7 // 7 for all the spaces and decorations
    }
    pub fn mid_len(&self) -> usize {
        let mid_text = self.options.get(1).or_else(|| self.options.get(0)).unwrap_or(&"");
        mid_text.chars().count() + self.key.chars().count() + self.option_index.1 + 7 // 7 for all the spaces and decorations
    }
    pub fn full_len(&self) -> usize {
        let full_text = self.options.get(0).unwrap_or(&"");
        full_text.chars().count() + self.key.chars().count() + self.option_index.1 + 7 // 7 for all the spaces and decorations
    }
    pub fn render_short_length(&self) -> String {
        let short_text = self.options.get(2).or_else(|| self.options.get(1)).or_else(|| self.options.get(0)).unwrap_or(&"");
        self.render(short_text)
    }
    pub fn render_mid_length(&self) -> String {
        let mid_text = self.options.get(1).or_else(|| self.options.get(0)).unwrap_or(&"");
        self.render(mid_text)
    }
    pub fn render_full_length(&self) -> String {
        let full_text = self.options.get(0).unwrap_or(&"");
        self.render(full_text)
    }
    fn render(&self, text: &str) -> String {
        format!(
            "{}{} {}{} {}{}",
            self.render_keycode(&format!(" {} ", self.key)),
            arrow(self.keycode_background_color, self.keycode_foreground_color),
            self.render_selection_dots(),
            self.render_control_text(text),
            arrow(self.keycode_foreground_color, self.keycode_background_color),
            color_line_to_end(self.keycode_background_color),
        )
    }
    fn render_keycode(&self, text: &str) -> String {
        styled_text(self.keycode_foreground_color, self.keycode_background_color, text)
    }
    fn render_control_text(&self, text: &str) -> String {
        styled_text(self.control_text_foreground_color, self.control_text_background_color, text)
    }
    fn render_selection_dots(&self) -> String {
        let mut selection_dots = String::from(" ");
        for i in 1..=self.option_index.1 {
            if i == self.option_index.0 {
                selection_dots.push_str(&dot(self.active_dot_color, self.control_text_background_color));
            } else {
                selection_dots.push_str(&dot(self.control_text_foreground_color, self.control_text_background_color));
            }
        }
        selection_dots.push_str(" ");
        selection_dots
    }
}

struct LoadingAnimation {
    scanning_indication: Option<Vec<&'static str>>,
    animation_offset: u8,

}
impl LoadingAnimation {
    pub fn new(scanning_indication: &Option<Vec<&'static str>>, animation_offset: u8) -> Self {
        LoadingAnimation {
            scanning_indication: scanning_indication.clone(),
            animation_offset
        }
    }
    pub fn full_len(&self) -> usize {
        self.scanning_indication.as_ref()
            .and_then(|scanning_indication| scanning_indication.get(0))
            .map(|s| s.chars().count() + 3) // 3 for animation dots
            .unwrap_or(0)
    }
    pub fn mid_len(&self) -> usize {
        self.scanning_indication.as_ref()
            .and_then(|scanning_indication| scanning_indication.get(1)
                .or_else(|| scanning_indication.get(0))
            )
            .map(|s| s.chars().count() + 3) // 3 for animation dots
            .unwrap_or(0)
    }
    pub fn short_len(&self) -> usize {
        self.scanning_indication.as_ref()
            .and_then(|scanning_indication| scanning_indication.get(2)
                .or_else(|| scanning_indication.get(1))
                .or_else(|| scanning_indication.get(0))
            )
            .map(|s| s.chars().count() + 3) // 3 for animation dots
            .unwrap_or(0)
    }
    pub fn render_full_length(&self) -> String {
        self.scanning_indication.as_ref()
            .and_then(|scanning_indication| scanning_indication.get(0))
            .map(|s| s.to_string() + &self.animation_dots())
            .unwrap_or_else(String::new)
    }
    pub fn render_mid_length(&self) -> String {
        self.scanning_indication.as_ref()
            .and_then(|scanning_indication| scanning_indication.get(1)
                .or_else(|| scanning_indication.get(0))
            )
            .map(|s| s.to_string() + &self.animation_dots())
            .unwrap_or_else(String::new)
    }
    pub fn render_short_length(&self) -> String {
        self.scanning_indication.as_ref()
            .and_then(|scanning_indication| scanning_indication.get(2)
                .or_else(|| scanning_indication.get(1))
                .or_else(|| scanning_indication.get(0))
            )
            .map(|s| s.to_string() + &self.animation_dots())
            .unwrap_or_else(String::new)
    }
    fn animation_dots(&self) -> String {
        let mut to_render = String::from("");
        let dot_count = self.animation_offset % 4;
        for _ in 0..dot_count {
            to_render.push('.');
        }
        to_render
    }
}
