use ansi_term::{ANSIGenericString, ANSIStrings, Style};
use zellij_tile::prelude::*;

use crate::{
    colors::{BLACK, BRIGHT_GRAY, GRAY, GREEN, RED, WHITE},
    mode_info::pick_key_from_keybinds,
};
use crate::{LinePart, ARROW_SEPARATOR};

struct CtrlKeyShortcut {
    mode: CtrlKeyMode,
    action: CtrlKeyAction,
    key: Option<Key>,
}

impl CtrlKeyShortcut {
    pub fn new(mode: CtrlKeyMode, action: CtrlKeyAction, key: Option<Key>) -> Self {
        CtrlKeyShortcut { mode, action, key }
    }
}

#[derive(Clone)]
enum CtrlKeyAction {
    Lock,
    Pane,
    Tab,
    Resize,
    Scroll,
    Quit,
}

impl PartialEq<InputMode> for CtrlKeyAction {
    fn eq(&self, other: &InputMode) -> bool {
        matches!((other, self),
            (InputMode::Locked, CtrlKeyAction::Lock)
            | (InputMode::Pane, CtrlKeyAction::Pane)
            | (InputMode::Tab, CtrlKeyAction::Tab)
            | (InputMode::RenameTab, CtrlKeyAction::Tab)
            | (InputMode::Resize, CtrlKeyAction::Resize)
            | (InputMode::Scroll, CtrlKeyAction::Scroll))
    }
}

impl std::fmt::Display for CtrlKeyAction {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                CtrlKeyAction::Lock => "LOCK",
                CtrlKeyAction::Pane => "PANE",
                CtrlKeyAction::Tab => "TAB",
                CtrlKeyAction::Resize => "RESIZE",
                CtrlKeyAction::Scroll => "SCROLL",
                CtrlKeyAction::Quit => "QUIT",
            }
        )
    }
}

#[derive(Clone)]
enum CtrlKeyMode {
    Unselected,
    Selected,
    Disabled,
}

struct ShortcutText {
    parts: Vec<ShortCutTextElement>,
    style: CtrlKeyMode,
}

impl ShortcutText {
    fn new() -> Self {
        ShortcutText {
            parts: Vec::new(),
            style: CtrlKeyMode::Disabled,
        }
    }
    fn style(mut self, style: CtrlKeyMode) -> Self {
        self.style = style;
        self
    }
    fn push_prefix(mut self) -> Self {
        self.parts.push(ShortCutTextElement::Prefix);
        self
    }
    fn push_suffix(mut self) -> Self {
        self.parts.push(ShortCutTextElement::Suffix);
        self
    }
    fn push_left_sep(mut self) -> Self {
        self.parts.push(ShortCutTextElement::LeftSeparator);
        self
    }
    fn push_right_sep(mut self) -> Self {
        self.parts.push(ShortCutTextElement::RightSeparator);
        self
    }
    fn push_shortcut(mut self, s: &str) -> Self {
        self.parts
            .push(ShortCutTextElement::Shortcut(s.to_string()));
        self
    }
    fn push_text(mut self, s: &str) -> Self {
        self.parts.push(ShortCutTextElement::Text(s.to_string()));
        self
    }
    fn done(self) -> Self {
        self
    }

    fn to_styled_text(&self) -> LinePart {
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
                ShortCutTextElement::LeftSeparator => " <",
                ShortCutTextElement::RightSeparator => ">",
                ShortCutTextElement::Shortcut(s) => s,
                ShortCutTextElement::Text(s) => s,
            }
        )
    }
}

impl CtrlKeyShortcut {
    fn shortcut_key(&self) -> String {
        match self.key {
            Some(k) => k.to_string(),
            None => String::from(""),
        }
    }

    fn letter_shortcut_key(&self) -> String {
        match self.key {
            Some(Key::Ctrl(c)) | Some(Key::Alt(c)) | Some(Key::Char(c)) => c.to_string(),
            Some(key) => key.to_string(),
            None => String::from(""),
        }
    }

    pub fn full_text(&self) -> ShortcutText {
        ShortcutText::new()
            .style(self.mode.clone())
            .push_prefix()
            .push_left_sep()
            .push_shortcut(&self.letter_shortcut_key())
            .push_right_sep()
            .push_text(&self.action.to_string())
            .push_suffix()
            .done()
    }
    pub fn shortened_text(&self) -> ShortcutText {
        match self.key {
            Some(key) => match key {
                Key::Alt(c) | Key::Char(c) | Key::Ctrl(c) => {
                    match self
                        .action
                        .to_string()
                        .split_once(c.to_ascii_uppercase())
                    {
                        Some((a, b)) => ShortcutText::new()
                            .style(self.mode.clone())
                            .push_prefix()
                            .push_text(a)
                            .push_left_sep()
                            .push_shortcut(&c.to_string())
                            .push_right_sep()
                            .push_text(b)
                            .push_suffix()
                            .done(),
                        None => ShortcutText::new()
                            .style(self.mode.clone())
                            .push_prefix()
                            .push_left_sep()
                            .push_shortcut(&c.to_string())
                            .push_right_sep()
                            .push_text(&self.action.to_string())
                            .done(),
                    }
                }
                Key::Null => ShortcutText::new()
                    .style(self.mode.clone())
                    .push_prefix()
                    .push_text(&self.action.to_string())
                    .push_suffix()
                    .done(),
                _ => ShortcutText::new()
                    .style(self.mode.clone())
                    .push_prefix()
                    .push_left_sep()
                    .push_shortcut(&self.shortcut_key())
                    .push_right_sep()
                    .push_text(&self.action.to_string())
                    .done(),
            },
            None => ShortcutText::new()
                .style(self.mode.clone())
                .push_prefix()
                .push_text(&self.action.to_string())
                .push_suffix()
                .done(),
        }
    }

    pub fn single_letter(&self) -> ShortcutText {
        ShortcutText::new()
            .style(self.mode.clone())
            .push_prefix()
            .push_shortcut(&self.letter_shortcut_key())
            .push_suffix()
            .done()
    }
}

fn key_indicators(max_len: usize, keys: &[CtrlKeyShortcut]) -> LinePart {
    let mut line_part = LinePart::default();
    for ctrl_key in keys {
        let key = ctrl_key.full_text().to_styled_text();
        line_part.part = format!("{}{}", line_part.part, key.part);
        line_part.len += key.len;
    }
    if line_part.len < max_len {
        return line_part;
    }
    line_part = LinePart::default();
    for ctrl_key in keys {
        let key = ctrl_key.shortened_text().to_styled_text();
        line_part.part = format!("{}{}", line_part.part, key.part);
        line_part.len += key.len;
    }
    if line_part.len < max_len {
        return line_part;
    }
    line_part = LinePart::default();
    for ctrl_key in keys {
        let key = ctrl_key.single_letter().to_styled_text();
        line_part.part = format!("{}{}", line_part.part, key.part);
        line_part.len += key.len;
    }
    if line_part.len < max_len {
        return line_part;
    }
    line_part = LinePart::default();
    line_part
}

pub fn superkey() -> LinePart {
    let prefix_text = " Ctrl + ";
    let prefix = Style::new().fg(WHITE).on(GRAY).bold().paint(prefix_text);
    LinePart {
        part: prefix.to_string(),
        len: prefix_text.chars().count(),
    }
}

pub fn ctrl_keys(help: &ModeInfo, max_len: usize) -> LinePart {
    const MODES: &[(CtrlKeyAction, Action)] = &[
        (CtrlKeyAction::Lock, Action::SwitchToMode(InputMode::Locked)),
        (CtrlKeyAction::Pane, Action::SwitchToMode(InputMode::Pane)),
        (CtrlKeyAction::Tab, Action::SwitchToMode(InputMode::Tab)),
        (
            CtrlKeyAction::Resize,
            Action::SwitchToMode(InputMode::Resize),
        ),
        (
            CtrlKeyAction::Scroll,
            Action::SwitchToMode(InputMode::Scroll),
        ),
        (CtrlKeyAction::Quit, Action::Quit),
    ];
    let mut v = Vec::new();
    for (m, a) in MODES.iter() {
        let shortcut = if m == &help.mode {
            CtrlKeyShortcut::new(
                CtrlKeyMode::Selected,
                m.clone(),
                pick_key_from_keybinds(a.clone(), &help.keybinds),
            )
        } else {
            CtrlKeyShortcut::new(
                CtrlKeyMode::Unselected,
                m.clone(),
                pick_key_from_keybinds(a.clone(), &help.keybinds),
            )
        };
        v.push(shortcut);
    }
    key_indicators(max_len, &v)
}