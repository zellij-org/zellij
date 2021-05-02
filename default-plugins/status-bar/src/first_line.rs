use crate::{
    mode_info::pick_key_from_keybinds,
    styled_text::{CtrlKeyMode, Prefix, StyledText},
    LinePart,
};
use zellij_tile::prelude::*;
#[derive(Debug)]
struct KeyShortcut {
    prefix: Prefix,
    mode: CtrlKeyMode,
    action: KeyAction,
    key: Option<Key>,
}

impl KeyShortcut {
    pub fn new(prefix: Prefix, mode: CtrlKeyMode, action: KeyAction, key: Option<Key>) -> Self {
        KeyShortcut {
            prefix,
            mode,
            action,
            key,
        }
    }
}

#[derive(Clone, Debug)]
enum KeyAction {
    Lock,
    Pane,
    Tab,
    Resize,
    Scroll,
    Quit,
}

impl PartialEq<InputMode> for KeyAction {
    fn eq(&self, other: &InputMode) -> bool {
        matches!(
            (other, self),
            (InputMode::Locked, KeyAction::Lock)
                | (InputMode::Pane, KeyAction::Pane)
                | (InputMode::Tab, KeyAction::Tab)
                | (InputMode::RenameTab, KeyAction::Tab)
                | (InputMode::Resize, KeyAction::Resize)
                | (InputMode::Scroll, KeyAction::Scroll)
        )
    }
}

impl std::fmt::Display for KeyAction {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                KeyAction::Lock => "LOCK",
                KeyAction::Pane => "PANE",
                KeyAction::Tab => "TAB",
                KeyAction::Resize => "RESIZE",
                KeyAction::Scroll => "SCROLL",
                KeyAction::Quit => "QUIT",
            }
        )
    }
}
impl KeyShortcut {
    fn letter_shortcut_key(&self) -> String {
        match (self.key, self.prefix) {
            (Some(Key::Ctrl(c)), Prefix::Ctrl) | (Some(Key::Alt(c)), Prefix::Alt) => c.to_string(),
            (Some(Key::Char(c)), _) => c.to_string(),
            (Some(key), _) => key.to_string(),
            (None, _) => String::from(""),
        }
    }

    pub fn full_text(&self) -> StyledText {
        if self.key.is_none() {
            // if there's no keybind for this mode
            // it should not be displayed on status-bar
            StyledText::new()
        } else {
            StyledText::new()
                .style(self.mode.clone())
                .push_prefix()
                .push_text(" ")
                .push_left_sep()
                .push_shortcut(&self.letter_shortcut_key())
                .push_right_sep()
                .push_text(" ")
                .push_text(&self.action.to_string())
                .push_text(" ")
                .push_suffix()
                .done()
        }
    }
    pub fn shortened_text(&self) -> StyledText {
        if self.key.is_none() {
            StyledText::new()
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
                            Some((a, b)) => StyledText::new()
                                .style(self.mode.clone())
                                .push_prefix()
                                .push_text(" ")
                                .push_text(&capitalize_str(a))
                                .push_left_sep()
                                .push_shortcut(&c.to_string())
                                .push_right_sep()
                                .push_text(&b.to_lowercase())
                                .push_text(" ")
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

    pub fn single_letter(&self) -> StyledText {
        if self.key.is_none() {
            StyledText::new()
        } else {
            StyledText::new()
                .style(self.mode.clone())
                .push_prefix()
                .push_text(" ")
                .push_shortcut(&self.letter_shortcut_key())
                .push_text(" ")
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

fn key_indicators(prefix: Prefix, max_len: usize, keys: &[KeyShortcut]) -> LinePart {
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
    const MODES: &[(KeyAction, Action)] = &[
        (KeyAction::Lock, Action::SwitchToMode(InputMode::Locked)),
        (KeyAction::Pane, Action::SwitchToMode(InputMode::Pane)),
        (KeyAction::Tab, Action::SwitchToMode(InputMode::Tab)),
        (KeyAction::Resize, Action::SwitchToMode(InputMode::Resize)),
        (KeyAction::Scroll, Action::SwitchToMode(InputMode::Scroll)),
        (KeyAction::Quit, Action::Quit),
    ];
    let prefix = superkey(help);
    let mut v = Vec::new();
    for (m, a) in MODES.iter() {
        let shortcut = if m == &help.mode {
            KeyShortcut::new(
                prefix,
                CtrlKeyMode::Selected,
                m.clone(),
                pick_key_from_keybinds(Action::SwitchToMode(InputMode::Normal), &help.keybinds),
            )
        } else {
            KeyShortcut::new(
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
