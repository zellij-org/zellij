use std::collections::BTreeMap;
use zellij_tile::graphemes::display_width;
use zellij_tile::prelude::*;

pub const PANEL_SIZE_PIPE: &str = "session_panel_size";
const DROPDOWN_CHROME_WIDTH: usize = 6;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PanelSize {
    #[default]
    Closed,
    Card,
    Manager,
}

impl PanelSize {
    pub fn from_pipe_payload(payload: &str) -> PanelSize {
        match payload {
            "card" => PanelSize::Card,
            "manager" => PanelSize::Manager,
            _ => PanelSize::Closed,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct SessionIndicator {
    pub global: Option<GlobalAttention>,
    pub enabled: bool,
    pub panel_size: PanelSize,
    pub manager_plugin_id: Option<u32>,
    pub hovered: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndicatorColor {
    Level(usize),
    Error,
    Success,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndicatorText {
    pub text: String,
    pub emphasis: Option<(IndicatorColor, std::ops::Range<usize>)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClickAction {
    Toggle,
}

fn plain(text: String) -> IndicatorText {
    IndicatorText {
        text,
        emphasis: None,
    }
}

fn status_color(status: AttentionStatus) -> IndicatorColor {
    match status {
        AttentionStatus::NeedsYou => IndicatorColor::Level(0),
        AttentionStatus::Failed => IndicatorColor::Error,
        AttentionStatus::Done => IndicatorColor::Success,
        _ => IndicatorColor::Level(1),
    }
}

fn status_words(status: AttentionStatus) -> &'static str {
    match status {
        AttentionStatus::NeedsYou => "need you",
        AttentionStatus::Failed => "failed",
        AttentionStatus::Done => "done",
        AttentionStatus::Working => "working",
        _ => "",
    }
}

pub fn watch_results(global: &GlobalAttention) -> usize {
    global
        .items
        .iter()
        .filter(|item| item.item.attention.from_watch && !item.is_muted())
        .count()
}

fn watch_variants(count: usize, running: usize, simplified_ui: bool) -> Vec<IndicatorText> {
    let symbol = if simplified_ui { "@" } else { "◉" };
    let head = format!("{} {}", symbol, count);
    let head_len = head.chars().count();
    let words = if count == 1 {
        "watch alert"
    } else {
        "watch alerts"
    };
    let colored = |text: String, end: usize| IndicatorText {
        text,
        emphasis: Some((IndicatorColor::Level(1), 0..end)),
    };
    vec![
        colored(
            format!("{} {} · {} running", head, words, running),
            head_len + 1 + words.chars().count(),
        ),
        colored(
            format!("{} {}", head, words),
            head_len + 1 + words.chars().count(),
        ),
        colored(format!("{} · {}", head, running), head_len),
        colored(head.clone(), head_len),
        colored(symbol.to_owned(), symbol.chars().count()),
    ]
}

pub fn indicator_variants(
    top: Option<(AttentionStatus, usize)>,
    watch_results: usize,
    running: usize,
    simplified_ui: bool,
) -> Vec<IndicatorText> {
    if watch_results > 0 {
        return watch_variants(watch_results, running, simplified_ui);
    }
    match top {
        None => vec![
            plain(format!("{} running", running)),
            plain(format!("{}", running)),
        ],
        Some((status, count)) => {
            let symbol = status.marker(simplified_ui);
            let symbol_len = symbol.chars().count();
            let head = format!("{} {}", symbol, count);
            let head_len = head.chars().count();
            let colored = |text: String, end: usize| IndicatorText {
                text,
                emphasis: Some((status_color(status), 0..end)),
            };
            vec![
                colored(
                    format!("{} {} · {} running", head, status_words(status), running),
                    head_len + 1 + status_words(status).chars().count(),
                ),
                colored(format!("{} · {}", head, running), head_len),
                colored(head.clone(), head_len),
                colored(symbol.to_owned(), symbol_len),
            ]
        },
    }
}

impl SessionIndicator {
    pub fn update(&mut self, event: &Event) -> bool {
        match event {
            Event::GlobalAttentionUpdate(global) => {
                let changed = self.global.as_ref() != Some(global);
                self.global = Some(global.clone());
                changed && self.enabled
            },
            Event::SessionCountsUpdate(_) => {
                let changed = !self.enabled;
                self.enabled = true;
                changed
            },
            _ => false,
        }
    }

    pub fn handle_pipe(&mut self, pipe_message: &PipeMessage) -> bool {
        if pipe_message.name != PANEL_SIZE_PIPE {
            return false;
        }
        let size = PanelSize::from_pipe_payload(pipe_message.payload.as_deref().unwrap_or(""));
        let plugin_id = pipe_message
            .args
            .get("plugin_id")
            .and_then(|id| id.parse::<u32>().ok());
        let closed_by_another_manager = size == PanelSize::Closed
            && plugin_id.is_some()
            && self.manager_plugin_id.is_some()
            && plugin_id != self.manager_plugin_id;
        if closed_by_another_manager {
            return false;
        }
        let changed = self.panel_size != size || self.manager_plugin_id != plugin_id;
        self.panel_size = size;
        self.manager_plugin_id = if size == PanelSize::Closed {
            None
        } else {
            plugin_id
        };
        changed
    }

    pub fn is_visible(&self) -> bool {
        self.enabled && self.global.is_some()
    }

    #[cfg(test)]
    pub fn variants(&self) -> Vec<IndicatorText> {
        self.variants_for(false)
    }

    pub fn variants_for(&self, simplified_ui: bool) -> Vec<IndicatorText> {
        if !self.enabled {
            return vec![];
        }
        match self.global.as_ref() {
            Some(global) => indicator_variants(
                global.top(),
                watch_results(global),
                global.running(),
                simplified_ui,
            ),
            None => vec![],
        }
    }

    pub fn arrow(&self, simplified_ui: bool) -> &'static str {
        match (self.panel_size, simplified_ui) {
            (PanelSize::Closed, false) => "▾",
            (_, false) => "▴",
            (PanelSize::Closed, true) => "v",
            (_, true) => "^",
        }
    }

    pub fn fitting_variant(&self, max_width: usize, simplified_ui: bool) -> Option<IndicatorText> {
        self.variants_for(simplified_ui)
            .into_iter()
            .find(|variant| display_width(&variant.text) + DROPDOWN_CHROME_WIDTH <= max_width)
    }

    pub fn width_of(text: &IndicatorText) -> usize {
        display_width(&text.text) + DROPDOWN_CHROME_WIDTH
    }

    pub fn render(&self, text: &IndicatorText, x: usize, y: usize, simplified_ui: bool) -> String {
        let mut value = Text::from(text.text.clone());
        if let Some((color, range)) = &text.emphasis {
            value = match color {
                IndicatorColor::Level(level) => value.color_range(*level, range.clone()),
                IndicatorColor::Error => value.error_color_range(range.clone()),
                IndicatorColor::Success => value.success_color_range(range.clone()),
            };
        }
        let mut dropdown = Dropdown::new("", vec![text.text.clone()])
            .accent_brackets()
            .arrow(self.arrow(simplified_ui))
            .styled_value(value)
            .hover_emphasis()
            .bold_value();
        if self.hovered {
            dropdown = dropdown.hovered();
        }
        dropdown.serialize(x, y, Self::width_of(text))
    }

    #[cfg(test)]
    pub fn plain_render(&self, text: &IndicatorText, simplified_ui: bool) -> String {
        format!("[ {} {} ]", text.text, self.arrow(simplified_ui))
    }

    pub fn click_action(&self) -> Option<ClickAction> {
        if self.panel_size != PanelSize::Closed || self.is_visible() {
            Some(ClickAction::Toggle)
        } else {
            None
        }
    }

    pub fn clicked(&self, _line: usize, _column: usize) {
        if let Some(ClickAction::Toggle) = self.click_action() {
            run_action(
                zellij_tile::prelude::actions::Action::ToggleSessionCard,
                BTreeMap::new(),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(variants: Vec<IndicatorText>) -> Vec<String> {
        variants.into_iter().map(|v| v.text).collect()
    }

    fn indicator(top: Option<(AttentionStatus, usize)>, running: usize) -> SessionIndicator {
        let mut sessions = vec![];
        for index in 0..running {
            let mut attention = SessionAttention::default();
            if index == 0 {
                if let Some((status, count)) = top {
                    for id in 0..count {
                        let pane = PaneAttention {
                            status,
                            ..Default::default()
                        };
                        attention.counts.add(&pane);
                        attention.items.push(AttentionItem {
                            pane_id: PaneId::Terminal(id as u32),
                            tab_id: 0,
                            tab_name: String::new(),
                            pane_title: String::new(),
                            attention: pane,
                        });
                    }
                }
            }
            sessions.push((format!("s{}", index), attention));
        }
        SessionIndicator {
            global: Some(GlobalAttention::build("s0", sessions)),
            enabled: true,
            ..Default::default()
        }
    }

    #[test]
    fn calm_shows_only_the_running_count_including_this_session() {
        assert_eq!(texts(indicator(None, 5).variants()), vec!["5 running", "5"]);
    }

    #[test]
    fn attention_shortens_step_by_step() {
        assert_eq!(
            texts(indicator(Some((AttentionStatus::NeedsYou, 2)), 5).variants()),
            vec!["● 2 need you · 5 running", "● 2 · 5", "● 2", "●"]
        );
        assert_eq!(
            texts(indicator(Some((AttentionStatus::Failed, 1)), 3).variants_for(true)),
            vec!["x 1 failed · 3 running", "x 1 · 3", "x 1", "x"]
        );
    }

    #[test]
    fn only_the_top_status_is_counted_and_coloured() {
        let variants = indicator(Some((AttentionStatus::Done, 3)), 2).variants();
        assert_eq!(
            variants[0].emphasis,
            Some((IndicatorColor::Success, 0.."✓ 3 done".chars().count()))
        );
    }

    #[test]
    fn a_finished_watch_is_announced_before_anything_else() {
        let mut indicator = indicator(Some((AttentionStatus::Failed, 1)), 3);
        let global = indicator.global.as_mut().unwrap();
        let mut watched = global.items[0].clone();
        watched.item.pane_id = PaneId::Terminal(9);
        watched.item.attention = PaneAttention {
            status: AttentionStatus::NeedsYou,
            from_watch: true,
            ..Default::default()
        };
        global.items.push(watched);
        assert_eq!(watch_results(global), 1);
        let variants = indicator.variants_for(false);
        assert_eq!(variants[0].text, "◉ 1 watch alert · 3 running");
        assert_eq!(
            variants[0].emphasis,
            Some((
                IndicatorColor::Level(1),
                0.."◉ 1 watch alert".chars().count()
            ))
        );
        assert_eq!(
            indicator.variants_for(true)[0].text,
            "@ 1 watch alert · 3 running"
        );
    }

    #[test]
    fn the_simplified_ui_uses_letters() {
        let indicator = indicator(Some((AttentionStatus::NeedsYou, 2)), 5);
        let variant = indicator.fitting_variant(40, true).unwrap();
        assert_eq!(
            indicator.plain_render(&variant, true),
            "[ ! 2 need you · 5 running v ]"
        );
    }

    #[test]
    fn shortening_steps_by_width() {
        let indicator = indicator(Some((AttentionStatus::NeedsYou, 2)), 5);
        let rendered: Vec<String> = [40, 20, 11, 8, 6]
            .iter()
            .map(|width| {
                indicator
                    .fitting_variant(*width, false)
                    .map(|v| indicator.plain_render(&v, false))
                    .unwrap_or_else(|| "<hidden>".to_owned())
            })
            .collect();
        assert_eq!(
            rendered,
            vec![
                "[ ● 2 need you · 5 running ▾ ]",
                "[ ● 2 · 5 ▾ ]",
                "[ ● 2 ▾ ]",
                "[ ● ▾ ]",
                "<hidden>",
            ]
        );
    }

    #[test]
    fn nothing_is_shown_before_attention_arrives_or_while_turned_off() {
        let empty = SessionIndicator::default();
        assert!(empty.variants().is_empty());
        assert_eq!(empty.click_action(), None);
        let mut turned_off = indicator(None, 2);
        turned_off.enabled = false;
        assert!(turned_off.variants().is_empty());
        assert_eq!(turned_off.click_action(), None);
        let mut turned_on = SessionIndicator::default();
        assert!(turned_on.update(&Event::SessionCountsUpdate(SessionCounts::default())));
        assert!(turned_on.variants().is_empty());
        assert!(turned_on.update(&Event::GlobalAttentionUpdate(GlobalAttention::default())));
        assert_eq!(texts(turned_on.variants()), vec!["0 running", "0"]);
    }

    #[test]
    fn clicks_toggle_through_the_server_whatever_this_bar_believes() {
        let mut indicator = indicator(None, 1);
        assert_eq!(indicator.click_action(), Some(ClickAction::Toggle));
        indicator.panel_size = PanelSize::Card;
        assert_eq!(indicator.click_action(), Some(ClickAction::Toggle));
        indicator.panel_size = PanelSize::Manager;
        assert_eq!(indicator.click_action(), Some(ClickAction::Toggle));
    }

    #[test]
    fn panel_size_pipe_is_tracked() {
        let mut indicator = indicator(None, 1);
        let mut args = BTreeMap::new();
        args.insert("plugin_id".to_owned(), "7".to_owned());
        let message = PipeMessage {
            source: PipeSource::Plugin(7),
            name: PANEL_SIZE_PIPE.to_owned(),
            payload: Some("manager".to_owned()),
            args,
            is_private: false,
        };
        assert!(indicator.handle_pipe(&message));
        assert_eq!(indicator.panel_size, PanelSize::Manager);
        assert_eq!(indicator.manager_plugin_id, Some(7));
        let closed = PipeMessage {
            payload: Some("closed".to_owned()),
            ..message
        };
        assert!(indicator.handle_pipe(&closed));
        assert_eq!(indicator.panel_size, PanelSize::Closed);
        assert_eq!(indicator.manager_plugin_id, None);
    }

    #[test]
    fn a_late_close_from_an_old_manager_does_not_hide_the_new_one() {
        let mut indicator = indicator(None, 1);
        let message = |plugin_id: &str, payload: &str| {
            let mut args = BTreeMap::new();
            args.insert("plugin_id".to_owned(), plugin_id.to_owned());
            PipeMessage {
                source: PipeSource::Plugin(plugin_id.parse().unwrap()),
                name: PANEL_SIZE_PIPE.to_owned(),
                payload: Some(payload.to_owned()),
                args,
                is_private: false,
            }
        };
        indicator.handle_pipe(&message("8", "manager"));
        assert!(!indicator.handle_pipe(&message("7", "closed")));
        assert_eq!(indicator.panel_size, PanelSize::Manager);
        assert!(indicator.handle_pipe(&message("8", "closed")));
        assert_eq!(indicator.panel_size, PanelSize::Closed);
    }
}
