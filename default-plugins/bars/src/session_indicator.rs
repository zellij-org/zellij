use std::collections::BTreeMap;
use unicode_width::UnicodeWidthStr;
use zellij_tile::prelude::*;

pub const SESSION_MANAGER_ALIAS: &str = "session-manager";
pub const PANEL_SIZE_PIPE: &str = "session_panel_size";
pub const CARD_WIDTH: usize = 72;
pub const CARD_HEIGHT: usize = 13;
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
    pub counts: SessionCounts,
    pub counts_received: bool,
    pub folder_sessions: Option<FolderSessions>,
    pub panel_size: PanelSize,
    pub manager_plugin_id: Option<u32>,
    pub hovered: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndicatorText {
    pub text: String,
    pub emphasis: Option<(usize, std::ops::Range<usize>)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClickAction {
    Close,
    OpenCard,
}

fn plain(text: String) -> IndicatorText {
    IndicatorText {
        text,
        emphasis: None,
    }
}

fn running_text(running: u32, full_matches: usize, suffix: &str) -> IndicatorText {
    let count = running.to_string();
    if full_matches == 0 {
        return plain(format!("{}{}", count, suffix));
    }
    let full = full_matches.to_string();
    let start = count.chars().count() + 1;
    let end = start + full.chars().count();
    IndicatorText {
        text: format!("{}/{}{}", count, full, suffix),
        emphasis: Some((2, start..end)),
    }
}

impl SessionIndicator {
    pub fn update(&mut self, event: &Event) -> bool {
        match event {
            Event::SessionCountsUpdate(counts) => {
                let changed = &self.counts != counts || !self.counts_received;
                self.counts = *counts;
                self.counts_received = true;
                changed
            },
            Event::FolderSessionsUpdate(folder_sessions) => {
                let new_value = if folder_sessions.total() == 0 {
                    None
                } else {
                    Some(folder_sessions.clone())
                };
                let changed = self.folder_sessions != new_value;
                self.folder_sessions = new_value;
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
        self.counts_received
    }

    pub fn variants(&self) -> Vec<IndicatorText> {
        if !self.counts_received {
            return vec![];
        }
        let running = self.counts.running;
        if running == 0 {
            return vec![
                plain("No Other Sessions".to_owned()),
                plain("No Sessions".to_owned()),
                plain("0".to_owned()),
            ];
        }
        let full_matches = self
            .folder_sessions
            .as_ref()
            .map(|f| f.full_match_running.len())
            .unwrap_or(0);
        let long = if running == 1 {
            " running session"
        } else {
            " running sessions"
        };
        vec![
            running_text(running, full_matches, long),
            running_text(running, full_matches, " running"),
            running_text(running, full_matches, ""),
        ]
    }

    pub fn arrow(&self, simplified_ui: bool) -> &'static str {
        match (self.panel_size, simplified_ui) {
            (PanelSize::Closed, false) => "▾",
            (_, false) => "▴",
            (PanelSize::Closed, true) => "v",
            (_, true) => "^",
        }
    }

    pub fn fitting_variant(&self, max_width: usize) -> Option<IndicatorText> {
        self.variants()
            .into_iter()
            .find(|variant| variant.text.width() + DROPDOWN_CHROME_WIDTH <= max_width)
    }

    pub fn width_of(text: &IndicatorText) -> usize {
        text.text.width() + DROPDOWN_CHROME_WIDTH
    }

    pub fn render(&self, text: &IndicatorText, x: usize, y: usize, simplified_ui: bool) -> String {
        let mut value = Text::from(text.text.clone());
        if let Some((level, range)) = &text.emphasis {
            value = value.color_range(*level, range.clone());
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
        if self.panel_size != PanelSize::Closed {
            return Some(ClickAction::Close);
        }
        if !self.is_visible() {
            return None;
        }
        Some(ClickAction::OpenCard)
    }

    pub fn clicked(&self, line: usize, column: usize) {
        match self.click_action() {
            Some(ClickAction::Close) => {
                if let Some(plugin_id) = self.manager_plugin_id {
                    pipe_message_to_plugin(
                        MessageToPlugin::new("close").with_destination_plugin_id(plugin_id),
                    );
                }
            },
            Some(ClickAction::OpenCard) => {
                let mut configuration = BTreeMap::new();
                configuration.insert("role".to_owned(), "card".to_owned());
                open_plugin_popup_with_options(
                    SESSION_MANAGER_ALIAS,
                    configuration,
                    line,
                    column,
                    CARD_WIDTH,
                    CARD_HEIGHT,
                    PopupOptions {
                        focused: false,
                        corner: Some(PopupCorner::TopRight),
                    },
                );
            },
            None => {},
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn counts(running: u32, resumable_matching: u32) -> SessionIndicator {
        SessionIndicator {
            counts: SessionCounts {
                running,
                resumable_matching,
            },
            counts_received: true,
            ..Default::default()
        }
    }

    fn texts(indicator: &SessionIndicator) -> Vec<String> {
        indicator.variants().into_iter().map(|v| v.text).collect()
    }

    fn with_full_matches(mut indicator: SessionIndicator, names: &[&str]) -> SessionIndicator {
        indicator.folder_sessions = Some(FolderSessions {
            folder: "/p/".to_owned(),
            running: names.iter().map(|n| n.to_string()).collect(),
            resumable: vec![],
            full_match_running: names.iter().map(|n| n.to_string()).collect(),
        });
        indicator
    }

    #[test]
    fn running_sessions_are_counted() {
        assert_eq!(
            texts(&counts(2, 1)),
            vec!["2 running sessions", "2 running", "2"]
        );
        assert_eq!(
            texts(&counts(1, 0)),
            vec!["1 running session", "1 running", "1"]
        );
    }

    #[test]
    fn without_other_running_sessions_the_indicator_says_so_and_still_opens_the_card() {
        for indicator in [counts(0, 0), counts(0, 2)] {
            assert_eq!(
                texts(&indicator),
                vec!["No Other Sessions", "No Sessions", "0"]
            );
            assert_eq!(indicator.click_action(), Some(ClickAction::OpenCard));
        }
    }

    #[test]
    fn nothing_is_shown_before_the_counts_arrive() {
        let indicator = SessionIndicator::default();
        assert!(texts(&indicator).is_empty());
        assert_eq!(indicator.click_action(), None);
        let mut indicator = SessionIndicator::default();
        assert!(indicator.update(&Event::SessionCountsUpdate(SessionCounts::default())));
        assert_eq!(texts(&indicator)[0], "No Other Sessions");
    }

    #[test]
    fn full_matches_follow_the_count_after_a_slash() {
        let indicator = with_full_matches(counts(38, 0), &["a", "b", "c", "d", "e", "f", "g"]);
        assert_eq!(
            texts(&indicator),
            vec!["38/7 running sessions", "38/7 running", "38/7"]
        );
        assert_eq!(
            indicator.variants()[0].emphasis,
            Some((2, 3..4)),
            "the full match count is shown in emphasis_2"
        );
    }

    #[test]
    fn shortening_steps() {
        let indicator = counts(2, 1);
        let rendered: Vec<String> = [40, 25, 16, 8, 6]
            .iter()
            .map(|width| {
                indicator
                    .fitting_variant(*width)
                    .map(|v| indicator.plain_render(&v, false))
                    .unwrap_or_else(|| "<hidden>".to_owned())
            })
            .collect();
        assert_eq!(
            rendered,
            vec![
                "[ 2 running sessions ▾ ]",
                "[ 2 running sessions ▾ ]",
                "[ 2 running ▾ ]",
                "[ 2 ▾ ]",
                "<hidden>",
            ]
        );
    }

    #[test]
    fn the_arrow_shows_whether_the_card_is_open() {
        let mut indicator = counts(1, 0);
        let mut rendered = vec![];
        for (size, simplified) in [
            (PanelSize::Closed, false),
            (PanelSize::Card, false),
            (PanelSize::Manager, false),
            (PanelSize::Closed, true),
            (PanelSize::Card, true),
        ] {
            indicator.panel_size = size;
            let variant = indicator.variants().remove(0);
            rendered.push(indicator.plain_render(&variant, simplified));
        }
        assert_eq!(
            rendered,
            vec![
                "[ 1 running session ▾ ]",
                "[ 1 running session ▴ ]",
                "[ 1 running session ▴ ]",
                "[ 1 running session v ]",
                "[ 1 running session ^ ]",
            ]
        );
    }

    #[test]
    fn clicks_open_the_card_or_close_whatever_is_open() {
        let mut indicator = counts(1, 0);
        assert_eq!(indicator.click_action(), Some(ClickAction::OpenCard));
        indicator.panel_size = PanelSize::Card;
        assert_eq!(indicator.click_action(), Some(ClickAction::Close));
        indicator.panel_size = PanelSize::Manager;
        assert_eq!(indicator.click_action(), Some(ClickAction::Close));
        assert_eq!(SessionIndicator::default().click_action(), None);
    }

    #[test]
    fn panel_size_pipe_is_tracked() {
        let mut indicator = counts(1, 0);
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
}
