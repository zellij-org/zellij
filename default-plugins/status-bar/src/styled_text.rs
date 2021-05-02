use ansi_term::{ANSIGenericString, ANSIStrings, Style};

use crate::colors::{BLACK, BRIGHT_GRAY, GRAY, GREEN, ORANGE, RED, WHITE};
use crate::{LinePart, ARROW_SEPARATOR};

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum Prefix {
    Ctrl,
    Alt,
    None,
}

impl Prefix {
    pub fn text(&self) -> StyledText {
        match self {
            Prefix::Ctrl => StyledText::new().push_superkey(" Ctrl + ").done(),
            Prefix::Alt => StyledText::new().push_superkey(" Alt + ").done(),
            Prefix::None => StyledText::new(),
        }
    }
}

#[derive(Clone, Debug)]
pub enum CtrlKeyMode {
    Unselected,
    Selected,
    Disabled,
}

pub struct StyledText {
    parts: Vec<ShortCutTextElement>,
    style: CtrlKeyMode,
}

impl StyledText {
    pub fn new() -> Self {
        StyledText {
            parts: Vec::new(),
            style: CtrlKeyMode::Disabled,
        }
    }
    pub fn style(mut self, style: CtrlKeyMode) -> Self {
        self.style = style;
        self
    }
    pub fn push_prefix(mut self) -> Self {
        self.parts.push(ShortCutTextElement::Prefix);
        self
    }
    pub fn push_suffix(mut self) -> Self {
        self.parts.push(ShortCutTextElement::Suffix);
        self
    }
    pub fn push_left_sep(mut self) -> Self {
        self.parts.push(ShortCutTextElement::LeftSeparator);
        self
    }
    pub fn push_right_sep(mut self) -> Self {
        self.parts.push(ShortCutTextElement::RightSeparator);
        self
    }
    pub fn push_shortcut(mut self, s: &str) -> Self {
        self.parts
            .push(ShortCutTextElement::Shortcut(s.to_string()));
        self
    }
    pub fn push_text(mut self, s: &str) -> Self {
        self.parts.push(ShortCutTextElement::Text(s.to_string()));
        self
    }
    pub fn push_superkey(mut self, s: &str) -> Self {
        self.parts
            .push(ShortCutTextElement::SuperKey(s.to_string()));
        self
    }
    pub fn push_nav_text(mut self, s: &str) -> Self {
        self.parts.push(ShortCutTextElement::NavText(s.to_string()));
        self
    }
    pub fn push_nav_prefix(mut self, s: &str) -> Self {
        self.parts
            .push(ShortCutTextElement::NavPrefix(s.to_string()));
        self
    }
    pub fn push_nav_key(mut self, s: &str) -> Self {
        self.parts.push(ShortCutTextElement::NavKey(s.to_string()));
        self
    }
    pub fn done(self) -> Self {
        self
    }

    pub fn to_styled_text(&self) -> LinePart {
        LinePart {
            part: ANSIStrings(
                &self
                    .parts
                    .iter()
                    .map(|x| x.to_styled_text(self.style.clone()))
                    .collect::<Vec<_>>(),
            )
            .to_string(),
            len: self
                .parts
                .iter()
                .map(|x| x.to_string().chars().count())
                .sum(),
        }
    }
}

enum ShortCutTextElement {
    Prefix,
    Suffix,
    LeftSeparator,
    RightSeparator,
    Shortcut(String),
    Text(String),
    SuperKey(String),
    NavText(String),
    NavKey(String),
    NavPrefix(String),
}

impl ShortCutTextElement {
    fn to_styled_text(&self, style: CtrlKeyMode) -> ANSIGenericString<str> {
        match (style, self) {
            (CtrlKeyMode::Unselected, ShortCutTextElement::Prefix) => {
                Style::new().fg(GRAY).on(BRIGHT_GRAY)
            }
            (CtrlKeyMode::Unselected, ShortCutTextElement::Suffix) => {
                Style::new().fg(BRIGHT_GRAY).on(GRAY)
            }
            (CtrlKeyMode::Unselected, ShortCutTextElement::Shortcut(_)) => {
                Style::new().bold().fg(RED).on(BRIGHT_GRAY).bold()
            }
            (CtrlKeyMode::Unselected, _) => Style::new().bold().fg(BLACK).on(BRIGHT_GRAY).bold(),
            (CtrlKeyMode::Selected, ShortCutTextElement::Prefix) => Style::new().fg(GRAY).on(GREEN),
            (CtrlKeyMode::Selected, ShortCutTextElement::Suffix) => Style::new().fg(GREEN).on(GRAY),
            (CtrlKeyMode::Selected, ShortCutTextElement::Shortcut(_)) => {
                Style::new().bold().fg(RED).on(GREEN).bold()
            }
            (CtrlKeyMode::Selected, _) => Style::new().fg(BLACK).on(GREEN).bold(),
            (CtrlKeyMode::Disabled, ShortCutTextElement::Prefix) => {
                Style::new().fg(GRAY).on(BRIGHT_GRAY)
            }
            (CtrlKeyMode::Disabled, ShortCutTextElement::Suffix) => {
                Style::new().fg(BRIGHT_GRAY).on(GRAY)
            }
            (_, ShortCutTextElement::SuperKey(_)) => Style::new().fg(WHITE).on(GRAY).bold(),
            (CtrlKeyMode::Disabled, ShortCutTextElement::NavText(_)) => Style::new().bold(),
            (CtrlKeyMode::Disabled, ShortCutTextElement::NavKey(_)) => {
                Style::new().fg(GREEN).bold()
            }
            (CtrlKeyMode::Disabled, ShortCutTextElement::NavPrefix(_)) => {
                Style::new().fg(ORANGE).bold()
            }
            (CtrlKeyMode::Disabled, _) => Style::new().fg(GRAY).on(BRIGHT_GRAY).dimmed(),
        }
        .paint(self.to_string())
    }
}

impl std::fmt::Display for ShortCutTextElement {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                ShortCutTextElement::Prefix | ShortCutTextElement::Suffix => ARROW_SEPARATOR,
                ShortCutTextElement::LeftSeparator => "<",
                ShortCutTextElement::RightSeparator => ">",
                ShortCutTextElement::Shortcut(s) => s,
                ShortCutTextElement::Text(s) => s,
                ShortCutTextElement::SuperKey(s) => s,
                ShortCutTextElement::NavKey(s) => s,
                ShortCutTextElement::NavPrefix(s) => s,
                ShortCutTextElement::NavText(s) => s,
            }
        )
    }
}
