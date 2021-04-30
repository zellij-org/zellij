use ansi_term::{ANSIGenericString, ANSIStrings, Style};
use zellij_tile::prelude::*;

use crate::{
    colors::{BLACK, BRIGHT_GRAY, GRAY, GREEN, RED, WHITE},
    mode_info::pick_key_from_keybinds,
};
use crate::{LinePart, ARROW_SEPARATOR};

#[derive(Debug, Copy, Clone, PartialEq)]
enum Prefix {
    Ctrl,
    Alt,
    None,
}

impl Prefix {
    fn text(&self) -> ShortcutText {
        match self {
            Prefix::Ctrl => ShortcutText::new().push_superkey("Ctrl + ").done(),
            Prefix::Alt => ShortcutText::new().push_superkey("Alt + ").done(),
            Prefix::None => ShortcutText::new(),
        }
    }
}

#[derive(Debug)]
struct CtrlKeyShortcut {
    prefix: Prefix,
    mode: CtrlKeyMode,
    action: CtrlKeyAction,
    key: Option<Key>,
}

impl CtrlKeyShortcut {
    pub fn new(prefix: Prefix, mode: CtrlKeyMode, action: CtrlKeyAction, key: Option<Key>) -> Self {
        CtrlKeyShortcut {
            prefix,
            mode,
            action,
            key,
        }
    }
}

#[derive(Clone, Debug)]
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
        matches!(
            (other, self),
            (InputMode::Locked, CtrlKeyAction::Lock)
                | (InputMode::Pane, CtrlKeyAction::Pane)
                | (InputMode::Tab, CtrlKeyAction::Tab)
                | (InputMode::RenameTab, CtrlKeyAction::Tab)
                | (InputMode::Resize, CtrlKeyAction::Resize)
                | (InputMode::Scroll, CtrlKeyAction::Scroll)
        )
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

#[derive(Clone, Debug)]
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
    fn push_superkey(mut self, s: &str) -> Self {
        self.parts
            .push(ShortCutTextElement::SuperKey(s.to_string()));
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
    SuperKey(String),
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
            }
        )
    }
}

impl CtrlKeyShortcut {
    fn letter_shortcut_key(&self) -> String {
        match (self.key, self.prefix) {
            (Some(Key::Ctrl(c)), Prefix::Ctrl) | (Some(Key::Alt(c)), Prefix::Alt) => c.to_string(),
            (Some(Key::Char(c)), _) => c.to_string(),
            (Some(key), _) => key.to_string(),
            (None, _) => String::from(""),
        }
    }

    pub fn full_text(&self) -> ShortcutText {
        if self.key.is_none() {
            // if there's no keybind for this mode
            // it should not be displayed on status-bar
            ShortcutText::new()
        } else {
            ShortcutText::new()
                .style(self.mode.clone())
                .push_prefix()
                .push_text(" ")
                .push_left_sep()
                .push_shortcut(&self.letter_shortcut_key())
                .push_right_sep()
                .push_text(" ")
                .push_text(&self.action.to_string())
                .push_suffix()
                .done()
        }
    }
    pub fn shortened_text(&self) -> ShortcutText {
        if self.key.is_none() {
            ShortcutText::new()
        } else {
            match self.key {
                Some(key) => match key {
                    // shortened text only available when 
                    // * shortcut key is an available character
                    // * Ctrl/Alt is already extracted as a prefix
                    // * action string contain current shortcut key
                    // otherwise, it should just act like a full text
                    Key::Alt(c) | Key::Char(c) | Key::Ctrl(c) if self.prefix != Prefix::None => {
                        match self.action.to_string().split_once(c.to_ascii_uppercase()) {
                            Some((a, b)) => ShortcutText::new()
                                .style(self.mode.clone())
                                .push_prefix()
                                .push_text(&capitalize_str(a))
                                .push_left_sep()
                                .push_shortcut(&c.to_string())
                                .push_right_sep()
                                .push_text(&b.to_lowercase())
                                .push_suffix()
                                .done(),
                            None => self.full_text(),
                        }
                    }
                    _ => self.full_text(),
                },
                None => self.full_text(),
            }
        }
    }

    pub fn single_letter(&self) -> ShortcutText {
        if self.key.is_none() {
            ShortcutText::new()
        } else {
            ShortcutText::new()
                .style(self.mode.clone())
                .push_prefix()
                .push_shortcut(&self.letter_shortcut_key())
                .push_suffix()
                .done()
        }
    }
}

fn capitalize_str(s: &str) -> String {
    if s.len() < 2 {
        s.to_uppercase()
    } else {
        let (l, r) = s.split_at(1);
        l.to_uppercase() + &r.to_lowercase()
    }
}

fn key_indicators(prefix: Prefix, max_len: usize, keys: &[CtrlKeyShortcut]) -> LinePart {
    //TODO use .fold instead of for .. in.
    let prefix = prefix.text().to_styled_text();
    let mut line_part = LinePart::default();
    line_part.part = format!("{}{}", line_part.part, prefix.part);
    line_part.len += prefix.len;
    for ctrl_key in keys {
        let key = ctrl_key.full_text().to_styled_text();
        line_part.part = format!("{}{}", line_part.part, key.part);
        line_part.len += key.len;
    }
    if line_part.len < max_len {
        return line_part;
    }
    line_part = LinePart::default();
    line_part.part = format!("{}{}", line_part.part, prefix.part);
    line_part.len += prefix.len;
    for ctrl_key in keys {
        let key = ctrl_key.shortened_text().to_styled_text();
        line_part.part = format!("{}{}", line_part.part, key.part);
        line_part.len += key.len;
    }
    if line_part.len < max_len {
        return line_part;
    }
    line_part = LinePart::default();
    line_part.part = format!("{}{}", line_part.part, prefix.part);
    line_part.len += prefix.len;
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

// if all key starts with Ctrl (or Alt) we should print a lead prefix
// otherwise the key should be printed as-is
// e.g.
// keys: Ctrl-p Ctrl-t Ctrl-q, print: Ctrl - p/t/q
// Keys: Ctrl-p Alt-t Alt-q, print: Ctrl-p/Alt-t/Alt-q
fn superkey(help: &ModeInfo) -> Prefix {
    const MODES_LIST: &[Action] = &[
        Action::SwitchToMode(InputMode::Locked),
        Action::SwitchToMode(InputMode::Pane),
        Action::SwitchToMode(InputMode::Tab),
        Action::SwitchToMode(InputMode::Resize),
        Action::SwitchToMode(InputMode::Scroll),
        Action::SwitchToMode(InputMode::Normal),
        Action::Quit,
    ];
    let mode_keys: Vec<Key> = MODES_LIST
        .iter()
        .filter_map(|action| pick_key_from_keybinds(action.clone(), &help.keybinds))
        .collect();
    if mode_keys.iter().all(|c| matches!(c, Key::Ctrl(_))) {
        Prefix::Ctrl
    } else if mode_keys.iter().all(|c| matches!(c, Key::Alt(_))) {
        Prefix::Alt
    } else {
        Prefix::None
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
    let prefix = superkey(help);
    let mut v = Vec::new();
    for (m, a) in MODES.iter() {
        let shortcut = if m == &help.mode {
            CtrlKeyShortcut::new(
                prefix,
                CtrlKeyMode::Selected,
                m.clone(),
                pick_key_from_keybinds(Action::SwitchToMode(InputMode::Normal), &help.keybinds),
            )
        } else {
            CtrlKeyShortcut::new(
                prefix,
                CtrlKeyMode::Unselected,
                m.clone(),
                pick_key_from_keybinds(a.clone(), &help.keybinds),
            )
        };
        v.push(shortcut);
    }
    key_indicators(prefix, max_len, &v)
}
