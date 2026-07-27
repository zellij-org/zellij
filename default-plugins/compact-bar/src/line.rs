use ansi_term::ANSIStrings;
use unicode_width::UnicodeWidthStr;

use crate::{LinePart, TabRenderData, ARROW_SEPARATOR};
use zellij_tile::prelude::*;
use zellij_tile_utils::style;

pub struct TabLineOutput {
    pub parts: Vec<LinePart>,
    pub breadcrumb_range: Option<(usize, usize)>,
}

pub fn tab_line(
    mode_info: &ModeInfo,
    tab_data: TabRenderData,
    cols: usize,
    toggle_tooltip_key: Option<String>,
    tooltip_is_active: bool,
) -> TabLineOutput {
    let dimmed = mode_info.session_ascended == Some(true) || mode_info.session_dimmed == Some(true);
    let breadcrumb_ancestry = if mode_info.host_fullscreen == Some(true) {
        mode_info.session_ancestry.clone()
    } else {
        Vec::new()
    };
    let nested_hint = NestedSessionHint::from_mode_info(mode_info);
    let config = TabLineConfig {
        session_name: mode_info.session_name.to_owned(),
        hide_session_name: mode_info.style.hide_session_name,
        mode: mode_info.mode,
        active_swap_layout_name: tab_data.active_swap_layout_name,
        is_swap_layout_dirty: tab_data.is_swap_layout_dirty,
        toggle_tooltip_key,
        tooltip_is_active,
        dimmed,
        breadcrumb_ancestry,
        nested_hint,
    };

    let builder = TabLineBuilder::new(config, mode_info.style.colors, mode_info.capabilities, cols);
    builder.build(tab_data.tabs, tab_data.active_tab_index)
}

#[derive(Debug, Clone)]
pub enum NestedSessionHint {
    None,
    Ascend(Vec<KeyWithModifier>),
    Descend(Vec<KeyWithModifier>),
}

impl NestedSessionHint {
    fn from_mode_info(mode_info: &ModeInfo) -> Self {
        if mode_info.session_dimmed == Some(true) {
            NestedSessionHint::Ascend(mode_info.nested_ascend_keys.clone())
        } else if mode_info.session_ascended == Some(true) {
            NestedSessionHint::Descend(mode_info.nested_descend_keys.clone())
        } else {
            NestedSessionHint::None
        }
    }

    fn is_active(&self) -> bool {
        !matches!(self, NestedSessionHint::None)
    }
}

#[derive(Debug, Clone)]
pub struct TabLineConfig {
    pub session_name: Option<String>,
    pub hide_session_name: bool,
    pub mode: InputMode,
    pub active_swap_layout_name: Option<String>,
    pub is_swap_layout_dirty: bool,
    pub toggle_tooltip_key: Option<String>,
    pub tooltip_is_active: bool,
    pub dimmed: bool,
    pub breadcrumb_ancestry: Vec<String>,
    pub nested_hint: NestedSessionHint,
}

fn calculate_total_length(parts: &[LinePart]) -> usize {
    parts.iter().map(|p| p.len).sum()
}

fn dim_style(bg: PaletteColor) -> ansi_term::Style {
    ansi_term::Style::new()
        .on(match bg {
            PaletteColor::Rgb((r, g, b)) => ansi_term::Color::RGB(r, g, b),
            PaletteColor::EightBit(c) => ansi_term::Color::Fixed(c),
        })
        .dimmed()
}

struct TabLinePopulator {
    cols: usize,
    palette: Styling,
    capabilities: PluginCapabilities,
    dimmed: bool,
}

impl TabLinePopulator {
    fn new(cols: usize, palette: Styling, capabilities: PluginCapabilities, dimmed: bool) -> Self {
        Self {
            cols,
            palette,
            capabilities,
            dimmed,
        }
    }

    fn populate_tabs(
        &self,
        tabs_before_active: &mut Vec<LinePart>,
        tabs_after_active: &mut Vec<LinePart>,
        tabs_to_render: &mut Vec<LinePart>,
    ) {
        let mut middle_size = calculate_total_length(tabs_to_render);
        let mut total_left = 0;
        let mut total_right = 0;

        loop {
            let left_count = tabs_before_active.len();
            let right_count = tabs_after_active.len();

            let collapsed_indicators =
                self.create_collapsed_indicators(left_count, right_count, tabs_to_render.len());

            let total_size =
                collapsed_indicators.left.len + middle_size + collapsed_indicators.right.len;

            if total_size > self.cols {
                break;
            }

            let tab_sizes = TabSizes {
                left: tabs_before_active.last().map_or(usize::MAX, |tab| tab.len),
                right: tabs_after_active.get(0).map_or(usize::MAX, |tab| tab.len),
            };

            let fit_analysis = self.analyze_tab_fit(
                &tab_sizes,
                total_size,
                left_count,
                right_count,
                &collapsed_indicators,
            );

            match self.decide_next_action(&fit_analysis, total_left, total_right) {
                TabAction::AddLeft => {
                    if let Some(tab) = tabs_before_active.pop() {
                        middle_size += tab.len;
                        total_left += tab.len;
                        tabs_to_render.insert(0, tab);
                    }
                },
                TabAction::AddRight => {
                    if !tabs_after_active.is_empty() {
                        let tab = tabs_after_active.remove(0);
                        middle_size += tab.len;
                        total_right += tab.len;
                        tabs_to_render.push(tab);
                    }
                },
                TabAction::Finish => {
                    tabs_to_render.insert(0, collapsed_indicators.left);
                    tabs_to_render.push(collapsed_indicators.right);
                    break;
                },
            }
        }
    }

    fn create_collapsed_indicators(
        &self,
        left_count: usize,
        right_count: usize,
        rendered_count: usize,
    ) -> CollapsedIndicators {
        let left_more_tab_index = left_count.saturating_sub(1);
        let right_more_tab_index = left_count + rendered_count;

        CollapsedIndicators {
            left: self.create_left_indicator(left_count, left_more_tab_index),
            right: self.create_right_indicator(right_count, right_more_tab_index),
        }
    }

    fn analyze_tab_fit(
        &self,
        tab_sizes: &TabSizes,
        total_size: usize,
        left_count: usize,
        right_count: usize,
        collapsed_indicators: &CollapsedIndicators,
    ) -> TabFitAnalysis {
        let size_by_adding_left =
            tab_sizes
                .left
                .saturating_add(total_size)
                .saturating_sub(if left_count == 1 {
                    collapsed_indicators.left.len
                } else {
                    0
                });

        let size_by_adding_right =
            tab_sizes
                .right
                .saturating_add(total_size)
                .saturating_sub(if right_count == 1 {
                    collapsed_indicators.right.len
                } else {
                    0
                });

        TabFitAnalysis {
            left_fits: size_by_adding_left <= self.cols,
            right_fits: size_by_adding_right <= self.cols,
        }
    }

    fn decide_next_action(
        &self,
        fit_analysis: &TabFitAnalysis,
        total_left: usize,
        total_right: usize,
    ) -> TabAction {
        if (total_left <= total_right || !fit_analysis.right_fits) && fit_analysis.left_fits {
            TabAction::AddLeft
        } else if fit_analysis.right_fits {
            TabAction::AddRight
        } else {
            TabAction::Finish
        }
    }

    fn create_left_indicator(&self, tab_count: usize, tab_index: usize) -> LinePart {
        if tab_count == 0 {
            return LinePart::default();
        }

        let more_text = self.format_count_text(tab_count, "← +{}", " ← +many ");
        self.create_styled_indicator(more_text, tab_index)
    }

    fn create_right_indicator(&self, tab_count: usize, tab_index: usize) -> LinePart {
        if tab_count == 0 {
            return LinePart::default();
        }

        let more_text = self.format_count_text(tab_count, "+{} →", " +many → ");
        self.create_styled_indicator(more_text, tab_index)
    }

    fn format_count_text(&self, count: usize, format_str: &str, fallback: &str) -> String {
        if count < 10000 {
            format!(" {} ", format_str.replace("{}", &count.to_string()))
        } else {
            fallback.to_string()
        }
    }

    fn create_styled_indicator(&self, text: String, tab_index: usize) -> LinePart {
        let separator = tab_separator(self.capabilities);
        let text_len = text.width() + 2 * separator.width();

        let colors = IndicatorColors {
            text: self.palette.ribbon_unselected.base,
            separator: self.palette.text_unselected.background,
            background: self.palette.text_selected.emphasis_0,
        };

        let text_style = if self.dimmed {
            style!(colors.text, colors.background).italic()
        } else {
            style!(colors.text, colors.background).bold()
        };

        let styled_parts = [
            style!(colors.separator, colors.background).paint(separator),
            text_style.paint(text),
            style!(colors.background, colors.separator).paint(separator),
        ];

        LinePart {
            part: ANSIStrings(&styled_parts).to_string(),
            len: text_len,
            tab_index: Some(tab_index),
        }
    }
}

#[derive(Debug)]
struct CollapsedIndicators {
    left: LinePart,
    right: LinePart,
}

#[derive(Debug)]
struct TabSizes {
    left: usize,
    right: usize,
}

#[derive(Debug)]
struct TabFitAnalysis {
    left_fits: bool,
    right_fits: bool,
}

#[derive(Debug)]
struct IndicatorColors {
    text: PaletteColor,
    separator: PaletteColor,
    background: PaletteColor,
}

#[derive(Debug)]
enum TabAction {
    AddLeft,
    AddRight,
    Finish,
}

struct TabLinePrefixBuilder<'a> {
    palette: Styling,
    cols: usize,
    dimmed: bool,
    breadcrumb_ancestry: &'a [String],
}

struct TabLinePrefix {
    parts: Vec<LinePart>,
    breadcrumb_range: Option<(usize, usize)>,
}

impl<'a> TabLinePrefixBuilder<'a> {
    fn new(palette: Styling, cols: usize, dimmed: bool, breadcrumb_ancestry: &'a [String]) -> Self {
        Self {
            palette,
            cols,
            dimmed,
            breadcrumb_ancestry,
        }
    }

    fn build(&self, session_name: Option<&str>, mode: InputMode) -> TabLinePrefix {
        let mut parts = vec![self.create_zellij_part()];
        let mut used_len = parts.get(0).map_or(0, |p| p.len);
        let mut breadcrumb_range = None;

        if let Some(name) = session_name {
            if !self.breadcrumb_ancestry.is_empty() {
                if let Some((mut breadcrumb_parts, range, breadcrumb_len)) =
                    self.create_breadcrumb_parts(name, used_len)
                {
                    used_len += breadcrumb_len;
                    breadcrumb_range = Some(range);
                    parts.append(&mut breadcrumb_parts);
                }
            } else if let Some(name_part) = self.create_session_name_part(name, used_len) {
                used_len += name_part.len;
                parts.push(name_part);
            }
        }

        if let Some(mode_part) = self.create_mode_part(mode, used_len) {
            parts.push(mode_part);
        }

        TabLinePrefix {
            parts,
            breadcrumb_range,
        }
    }

    fn create_breadcrumb_parts(
        &self,
        name: &str,
        used_len: usize,
    ) -> Option<(Vec<LinePart>, (usize, usize), usize)> {
        let bg_color = self.palette.text_unselected.background;
        let text_color = self.palette.text_unselected.base;
        let ancestor_text = format!("({} ▸ ", self.breadcrumb_ancestry.join(" ▸ "));
        let ancestor_len = ancestor_text.width();
        let name_text = name.to_string();
        let name_len = name_text.width();
        let closing_text = ") ".to_string();
        let closing_len = closing_text.width();
        let total_len = ancestor_len + name_len + closing_len;

        if self.cols.saturating_sub(used_len) < total_len {
            return None;
        }

        let ancestor_style = if self.dimmed {
            dim_style(bg_color)
        } else {
            style!(text_color, bg_color).italic()
        };
        let name_style = if self.dimmed {
            dim_style(bg_color)
        } else {
            style!(self.palette.text_unselected.emphasis_0, bg_color).bold()
        };
        let closing_style = if self.dimmed {
            dim_style(bg_color)
        } else {
            style!(text_color, bg_color).bold()
        };

        let ancestor_start = used_len;
        let ancestor_end = ancestor_start + ancestor_len;
        let parts = vec![
            LinePart {
                part: ancestor_style.paint(ancestor_text).to_string(),
                len: ancestor_len,
                tab_index: None,
            },
            LinePart {
                part: name_style.paint(name_text).to_string(),
                len: name_len,
                tab_index: None,
            },
            LinePart {
                part: closing_style.paint(closing_text).to_string(),
                len: closing_len,
                tab_index: None,
            },
        ];

        Some((parts, (ancestor_start, ancestor_end), total_len))
    }

    fn create_zellij_part(&self) -> LinePart {
        let prefix_text = " Zellij ";
        let colors = self.get_text_colors();
        let text_style = if self.dimmed {
            style!(colors.text, colors.background).italic()
        } else {
            style!(colors.text, colors.background).bold()
        };

        LinePart {
            part: text_style.paint(prefix_text).to_string(),
            len: prefix_text.chars().count(),
            tab_index: None,
        }
    }

    fn create_session_name_part(&self, name: &str, used_len: usize) -> Option<LinePart> {
        let name_part = format!("({})", name);
        let name_part_len = name_part.width();

        if self.cols.saturating_sub(used_len) >= name_part_len {
            let colors = self.get_text_colors();
            let text_style = if self.dimmed {
                style!(colors.text, colors.background).italic()
            } else {
                style!(colors.text, colors.background).bold()
            };
            Some(LinePart {
                part: text_style.paint(name_part).to_string(),
                len: name_part_len,
                tab_index: None,
            })
        } else {
            None
        }
    }

    fn create_mode_part(&self, mode: InputMode, used_len: usize) -> Option<LinePart> {
        let mode_text = format!(" {} ", format!("{:?}", mode).to_uppercase());
        let mode_len = mode_text.width();

        if self.cols.saturating_sub(used_len) >= mode_len {
            let colors = self.get_text_colors();
            let style = if self.dimmed {
                style!(colors.text, colors.background).italic()
            } else {
                match mode {
                    InputMode::Locked => {
                        style!(self.palette.text_unselected.emphasis_3, colors.background)
                    },
                    InputMode::Normal => {
                        style!(self.palette.text_unselected.emphasis_2, colors.background)
                    },
                    _ => style!(self.palette.text_unselected.emphasis_0, colors.background),
                }
                .bold()
            };

            Some(LinePart {
                part: style.paint(mode_text).to_string(),
                len: mode_len,
                tab_index: None,
            })
        } else {
            None
        }
    }

    fn get_text_colors(&self) -> IndicatorColors {
        IndicatorColors {
            text: self.palette.text_unselected.base,
            background: self.palette.text_unselected.background,
            separator: self.palette.text_unselected.background,
        }
    }
}

struct RightSideElementsBuilder {
    palette: Styling,
    capabilities: PluginCapabilities,
    dimmed: bool,
}

impl RightSideElementsBuilder {
    fn new(palette: Styling, capabilities: PluginCapabilities, dimmed: bool) -> Self {
        Self {
            palette,
            capabilities,
            dimmed,
        }
    }

    fn build(&self, config: &TabLineConfig, available_space: usize) -> Vec<LinePart> {
        if config.nested_hint.is_active() {
            if let Some(hint) = self.create_nested_hint(&config.nested_hint, available_space) {
                return vec![hint];
            }
            return Vec::new();
        }

        let mut elements = Vec::new();

        if let Some(ref tooltip_key) = config.toggle_tooltip_key {
            elements.push(self.create_tooltip_indicator(tooltip_key, config.tooltip_is_active));
        }

        if let Some(swap_status) = self.create_swap_layout_status(config, available_space) {
            elements.push(swap_status);
        }

        elements
    }

    fn create_nested_hint(&self, hint: &NestedSessionHint, max_len: usize) -> Option<LinePart> {
        let (prefix, keys) = match hint {
            NestedSessionHint::Ascend(keys) => ("Ascend: ", keys),
            NestedSessionHint::Descend(keys) => ("Descend: ", keys),
            NestedSessionHint::None => return None,
        };

        let bg = self.palette.text_unselected.background;
        let key_color = self.palette.text_unselected.emphasis_0;
        let sep_color = self.palette.text_unselected.base;
        let keys_text: Vec<String> = if keys.is_empty() {
            vec!["<unbound>".to_string()]
        } else {
            keys.iter().map(|key| format!("<{}>", key)).collect()
        };

        let prefix_len = prefix.width() + 1;
        let keys_display_len: usize = keys_text.iter().map(|k| k.width()).sum::<usize>()
            + keys_text.len().saturating_sub(1) * 3;

        if !keys_text.is_empty() && prefix_len + keys_display_len <= max_len {
            let prefix_style = dim_style(bg).italic();
            let mut part = prefix_style.paint(format!(" {}", prefix)).to_string();
            for (i, key) in keys_text.iter().enumerate() {
                if i > 0 {
                    part.push_str(&style!(sep_color, bg).paint(" + ").to_string());
                }
                if keys.is_empty() {
                    part.push_str(&dim_style(bg).italic().paint(key).to_string());
                } else {
                    part.push_str(&style!(key_color, bg).bold().paint(key).to_string());
                }
            }
            return Some(LinePart {
                part,
                len: prefix_len + keys_display_len,
                tab_index: None,
            });
        }

        let message = match hint {
            NestedSessionHint::Ascend(_) => " nested session",
            NestedSessionHint::Descend(_) => " host session",
            NestedSessionHint::None => return None,
        };
        let message_len = message.width();
        if message_len <= max_len {
            let style = dim_style(bg).italic();
            return Some(LinePart {
                part: style.paint(message).to_string(),
                len: message_len,
                tab_index: None,
            });
        }

        None
    }

    fn create_tooltip_indicator(&self, toggle_key: &str, is_active: bool) -> LinePart {
        let key_text = toggle_key;
        let key = if self.dimmed {
            Text::new(key_text).disabled().opaque()
        } else {
            Text::new(key_text).color_all(3).opaque()
        };
        let ribbon_text = "Tooltip";
        let mut ribbon = Text::new(ribbon_text);

        if self.dimmed {
            ribbon = ribbon.disabled();
        } else if is_active {
            ribbon = ribbon.selected();
        }

        LinePart {
            part: format!("{} {}", serialize_text(&key), serialize_ribbon(&ribbon)),
            len: key_text.chars().count() + ribbon_text.chars().count() + 6,
            tab_index: None,
        }
    }

    fn create_swap_layout_status(
        &self,
        config: &TabLineConfig,
        max_len: usize,
    ) -> Option<LinePart> {
        let swap_layout_name = config.active_swap_layout_name.as_ref()?;

        let mut layout_name = format!(" {} ", swap_layout_name);
        layout_name.make_ascii_uppercase();
        let layout_name_len = layout_name.len() + 3;

        let colors = SwapLayoutColors {
            bg: self.palette.text_unselected.background,
            fg: self.palette.ribbon_unselected.background,
            green: self.palette.ribbon_selected.background,
        };

        let separator = tab_separator(self.capabilities);
        let styled_parts = self.create_swap_layout_styled_parts(
            &layout_name,
            config.mode,
            config.is_swap_layout_dirty,
            &colors,
            separator,
        );

        let indicator = format!("{}{}{}", styled_parts.0, styled_parts.1, styled_parts.2);
        let (part, full_len) = (indicator.clone(), layout_name_len);
        let short_len = layout_name_len + 1;

        if full_len <= max_len {
            Some(LinePart {
                part,
                len: full_len,
                tab_index: None,
            })
        } else if short_len <= max_len && config.mode != InputMode::Locked {
            Some(LinePart {
                part: indicator,
                len: short_len,
                tab_index: None,
            })
        } else {
            None
        }
    }

    fn create_swap_layout_styled_parts(
        &self,
        layout_name: &str,
        mode: InputMode,
        is_dirty: bool,
        colors: &SwapLayoutColors,
        separator: &str,
    ) -> (String, String, String) {
        if self.dimmed {
            let text_style = style!(colors.bg, colors.fg).italic();
            return (
                style!(colors.bg, colors.fg).paint(separator).to_string(),
                text_style.paint(layout_name).to_string(),
                style!(colors.fg, colors.bg).paint(separator).to_string(),
            );
        }
        match mode {
            InputMode::Locked => (
                style!(colors.bg, colors.fg).paint(separator).to_string(),
                style!(colors.bg, colors.fg)
                    .italic()
                    .paint(layout_name)
                    .to_string(),
                style!(colors.fg, colors.bg).paint(separator).to_string(),
            ),
            _ if is_dirty => (
                style!(colors.bg, colors.fg).paint(separator).to_string(),
                style!(colors.bg, colors.fg)
                    .bold()
                    .paint(layout_name)
                    .to_string(),
                style!(colors.fg, colors.bg).paint(separator).to_string(),
            ),
            _ => (
                style!(colors.bg, colors.green).paint(separator).to_string(),
                style!(colors.bg, colors.green)
                    .bold()
                    .paint(layout_name)
                    .to_string(),
                style!(colors.green, colors.bg).paint(separator).to_string(),
            ),
        }
    }
}

#[derive(Debug)]
struct SwapLayoutColors {
    bg: PaletteColor,
    fg: PaletteColor,
    green: PaletteColor,
}

pub struct TabLineBuilder {
    config: TabLineConfig,
    palette: Styling,
    capabilities: PluginCapabilities,
    cols: usize,
}

impl TabLineBuilder {
    pub fn new(
        config: TabLineConfig,
        palette: Styling,
        capabilities: PluginCapabilities,
        cols: usize,
    ) -> Self {
        Self {
            config,
            palette,
            capabilities,
            cols,
        }
    }

    pub fn build(self, all_tabs: Vec<LinePart>, active_tab_index: usize) -> TabLineOutput {
        let (tabs_before_active, active_tab, tabs_after_active) =
            self.split_tabs(all_tabs, active_tab_index);

        let session_name = if self.config.hide_session_name {
            None
        } else {
            self.config.session_name.as_deref()
        };
        let breadcrumb_ancestry: &[String] = if self.config.hide_session_name {
            &[]
        } else {
            &self.config.breadcrumb_ancestry
        };
        let prefix_builder = TabLinePrefixBuilder::new(
            self.palette,
            self.cols,
            self.config.dimmed,
            breadcrumb_ancestry,
        );

        let prefix_result = prefix_builder.build(session_name, self.config.mode);
        let mut prefix = prefix_result.parts;
        let breadcrumb_range = prefix_result.breadcrumb_range;
        let prefix_len = calculate_total_length(&prefix);

        if prefix_len + active_tab.len > self.cols {
            return TabLineOutput {
                parts: prefix,
                breadcrumb_range,
            };
        }

        let mut tabs_to_render = vec![active_tab];
        let populator = TabLinePopulator::new(
            self.cols.saturating_sub(prefix_len),
            self.palette,
            self.capabilities,
            self.config.dimmed,
        );

        let mut tabs_before = tabs_before_active;
        let mut tabs_after = tabs_after_active;
        populator.populate_tabs(&mut tabs_before, &mut tabs_after, &mut tabs_to_render);

        prefix.append(&mut tabs_to_render);

        self.add_right_side_elements(&mut prefix);
        TabLineOutput {
            parts: prefix,
            breadcrumb_range,
        }
    }

    fn split_tabs(
        &self,
        mut all_tabs: Vec<LinePart>,
        active_tab_index: usize,
    ) -> (Vec<LinePart>, LinePart, Vec<LinePart>) {
        let mut tabs_after_active = all_tabs.split_off(active_tab_index);
        let mut tabs_before_active = all_tabs;

        let active_tab = if !tabs_after_active.is_empty() {
            tabs_after_active.remove(0)
        } else {
            tabs_before_active.pop().unwrap_or_default()
        };

        (tabs_before_active, active_tab, tabs_after_active)
    }

    fn add_right_side_elements(&self, prefix: &mut Vec<LinePart>) {
        let current_len = calculate_total_length(prefix);

        if current_len < self.cols {
            let right_builder =
                RightSideElementsBuilder::new(self.palette, self.capabilities, self.config.dimmed);
            let available_space = self.cols.saturating_sub(current_len);
            let mut right_elements = right_builder.build(&self.config, available_space);

            let right_len = calculate_total_length(&right_elements);

            if current_len + right_len <= self.cols {
                let remaining_space = self
                    .cols
                    .saturating_sub(current_len)
                    .saturating_sub(right_len);

                if remaining_space > 0 {
                    prefix.push(self.create_spacer(remaining_space));
                }

                prefix.append(&mut right_elements);
            }
        }
    }

    fn create_spacer(&self, space: usize) -> LinePart {
        let bg = self.palette.text_unselected.background;
        let buffer = (0..space)
            .map(|_| style!(bg, bg).paint(" ").to_string())
            .collect::<String>();

        LinePart {
            part: buffer,
            len: space,
            tab_index: None,
        }
    }
}

pub fn tab_separator(capabilities: PluginCapabilities) -> &'static str {
    if !capabilities.arrow_fonts {
        ARROW_SEPARATOR
    } else {
        ""
    }
}
