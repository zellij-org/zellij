// use colored::*;
use ansi_term::{ANSIStrings, Style};
use zellij_tile::prelude::*;

use crate::mode_info::get_mode_info;
use crate::{
    colors::{GREEN, ORANGE, WHITE},
    mode_info::pick_key_from_keybinds,
    styled_text::{Prefix, StyledText},
};
use crate::{LinePart, MORE_MSG};

fn full_length_shortcut(is_first_shortcut: bool, letter: &str, description: &str) -> LinePart {
    let separator = if is_first_shortcut { " " } else { " / " };
    let separator = Style::new().fg(WHITE).paint(separator);
    let shortcut_len = letter.chars().count() + 3; // 2 for <>'s around shortcut, 1 for the space
    let shortcut_left_separator = Style::new().fg(WHITE).paint("<");
    let shortcut = Style::new().fg(GREEN).bold().paint(letter);
    let shortcut_right_separator = Style::new().fg(WHITE).paint("> ");
    let description_len = description.chars().count();
    let description = Style::new().fg(WHITE).bold().paint(description);
    let len = shortcut_len + description_len + separator.chars().count();
    LinePart {
        part: format!(
            "{}",
            ANSIStrings(&[
                separator,
                shortcut_left_separator,
                shortcut,
                shortcut_right_separator,
                description
            ])
        ),
        len,
    }
}

fn first_word_shortcut(is_first_shortcut: bool, letter: &str, description: &str) -> LinePart {
    let separator = if is_first_shortcut { " " } else { " / " };
    let separator = Style::new().fg(WHITE).paint(separator);
    let shortcut_len = letter.chars().count() + 3; // 2 for <>'s around shortcut, 1 for the space
    let shortcut_left_separator = Style::new().fg(WHITE).paint("<");
    let shortcut = Style::new().fg(GREEN).bold().paint(letter);
    let shortcut_right_separator = Style::new().fg(WHITE).paint("> ");
    let description_first_word = description.split(' ').next().unwrap_or("");
    let description_first_word_length = description_first_word.chars().count();
    let description_first_word = Style::new().fg(WHITE).bold().paint(description_first_word);
    let len = shortcut_len + description_first_word_length + separator.chars().count();
    LinePart {
        part: format!(
            "{}",
            ANSIStrings(&[
                separator,
                shortcut_left_separator,
                shortcut,
                shortcut_right_separator,
                description_first_word,
            ])
        ),
        len,
    }
}

// Represent a key group, e.g. Move Focus (Alt + hljk)
// a quick nav may contains multiple keygroupes
struct KeyGroup {
    prefix: Prefix,
    keys: Vec<Key>,
}

impl KeyGroup {
    fn new() -> Self {
        KeyGroup {
            prefix: Prefix::None,
            keys: Vec::new(),
        }
    }

    fn push_key(mut self, k: Key) -> Self {
        self.keys.push(k);
        self
    }

    fn done(mut self) -> Self {
        if self.keys.iter().all(|c| matches!(c, Key::Ctrl(_))) {
            self.prefix = Prefix::Ctrl;
        } else if self.keys.iter().all(|c| matches!(c, Key::Alt(_))) {
            self.prefix = Prefix::Alt;
        } else {
            self.prefix = Prefix::None;
        }
        self
    }

    fn fill_prefix(&self, mut st: StyledText) -> StyledText {
        match self.prefix {
            Prefix::Alt => {
                st = st.push_nav_prefix("Alt + ");
            }
            Prefix::Ctrl => {
                st = st.push_nav_prefix("Ctrl + ");
            }
            Prefix::None => {}
        };
        st
    }

    fn fill_keys(&self, mut st: StyledText) -> StyledText {
        for (i, k) in self.keys.iter().enumerate() {
            if i > 0 {
                st = st.push_nav_text("/");
            }
            st = st.push_nav_key(&letter_shortcut_key(&self.prefix, k));
        }
        st
    }

    fn fill_style_text(&self, mut st: StyledText, description: &str) -> StyledText {
        if !self.keys.is_empty() {
            st = self.fill_prefix(st);
            st = self.fill_keys(st);
            st.push_nav_text(description)
        } else {
            st
        }
    }
}

fn letter_shortcut_key(prefix: &Prefix, key: &Key) -> String {
    match (key, prefix) {
        (Key::Ctrl(c), Prefix::Ctrl) | (Key::Alt(c), Prefix::Alt) => c.to_string(),
        (Key::Char(c), _) => c.to_string(),
        (key, _) => key.to_string(),
    }
}

struct QuickNavbar {
    open_pane: KeyGroup,
    move_focus: KeyGroup,
    switch_focus: KeyGroup,
}

impl QuickNavbar {
    fn from_keybinds(help: &ModeInfo) -> Self {
        const MOVE_FOCUS: &[Action] = &[
            Action::MoveFocus(Direction::Left),
            Action::MoveFocus(Direction::Up),
            Action::MoveFocus(Direction::Down),
            Action::MoveFocus(Direction::Right),
        ];
        const SWITCH_FOCUS: &[Action] = &[Action::FocusPreviousPane, Action::FocusNextPane];
        let open_pane = match pick_key_from_keybinds(Action::NewPane(None), &help.keybinds) {
            Some(k) => KeyGroup::new().push_key(k).done(),
            None => KeyGroup::new(),
        };
        let mut move_focus = KeyGroup::new();
        for k in MOVE_FOCUS
            .iter()
            .filter_map(|action| pick_key_from_keybinds(action.clone(), &help.keybinds))
        {
            move_focus = move_focus.push_key(k);
        }
        move_focus = move_focus.done();
        let mut switch_focus = KeyGroup::new();
        for k in SWITCH_FOCUS
            .iter()
            .filter_map(|action| pick_key_from_keybinds(action.clone(), &help.keybinds))
        {
            switch_focus = switch_focus.push_key(k);
        }
        switch_focus = switch_focus.done();
        QuickNavbar {
            open_pane,
            move_focus,
            switch_focus,
        }
    }

    fn full_text(&self) -> StyledText {
        let mut st = StyledText::new().push_nav_text(" Tip: ");
        st = self.open_pane.fill_style_text(st, " => open new pane. ");
        match (self.move_focus.prefix, self.switch_focus.prefix) {
            (Prefix::Alt, Prefix::Alt) | (Prefix::Ctrl, Prefix::Ctrl) => {
                if !self.switch_focus.keys.is_empty() && !self.move_focus.keys.is_empty() {
                    st = self.switch_focus.fill_prefix(st);
                    if !self.switch_focus.keys.is_empty() {
                        st = self.switch_focus.fill_keys(st);
                        st = st.push_nav_text(" or ");
                    }
                    if !self.move_focus.keys.is_empty() {
                        st = self.move_focus.fill_keys(st);
                    }
                    st = st.push_nav_text(" => navigate between panes.");
                }
            }
            _ => {
                // Switch focus and move focus have different prefix
                // e.g. Ctrl + [/] or Alt + h/j/l/k => ...
                // notice both switch_focus and move_focus may be empty
                st = self.switch_focus.fill_style_text(
                    st,
                    if self.move_focus.keys.is_empty() {
                        " => navigate between panes."
                    } else {
                        " or "
                    },
                );
                st = self
                    .move_focus
                    .fill_style_text(st, " => navigate between panes.");
            }
        }
        st.done()
    }

    fn medium_text(&self) -> StyledText {
        let mut st = StyledText::new().push_nav_text(" Tip: ");
        st = self.open_pane.fill_style_text(st, " => new pane. ");
        match (self.move_focus.prefix, self.switch_focus.prefix) {
            (Prefix::Alt, Prefix::Alt) | (Prefix::Ctrl, Prefix::Ctrl) => {
                if !self.switch_focus.keys.is_empty() && !self.move_focus.keys.is_empty() {
                    st = self.switch_focus.fill_prefix(st);
                    if !self.switch_focus.keys.is_empty() {
                        st = self.switch_focus.fill_keys(st);
                        st = st.push_nav_text(" or ");
                    }
                    if !self.move_focus.keys.is_empty() {
                        st = self.move_focus.fill_keys(st);
                    }
                    st = st.push_nav_text(" => navigate.");
                }
            }
            _ => {
                st = self.switch_focus.fill_style_text(
                    st,
                    if self.move_focus.keys.is_empty() {
                        " => navigate."
                    } else {
                        " or "
                    },
                );
                st = self.move_focus.fill_style_text(st, " => navigate.");
            }
        }
        st.done()
    }

    fn short_text(&self) -> StyledText {
        let mut st = StyledText::new().push_nav_text(" QuickNav: ");
        st = self.open_pane.fill_style_text(st, ", ");
        match (self.move_focus.prefix, self.switch_focus.prefix) {
            (Prefix::Alt, Prefix::Alt) | (Prefix::Ctrl, Prefix::Ctrl) => {
                if !self.switch_focus.keys.is_empty() && !self.move_focus.keys.is_empty() {
                    st = self.switch_focus.fill_prefix(st);
                    if !self.switch_focus.keys.is_empty() {
                        st = self.switch_focus.fill_keys(st);
                        if !self.move_focus.keys.is_empty() {
                            st = st.push_nav_text("/");
                        }
                    }
                    if !self.move_focus.keys.is_empty() {
                        st = self.move_focus.fill_keys(st);
                    }
                    st = st.push_nav_text(".");
                }
            }
            _ => {
                st = self.switch_focus.fill_style_text(st, ", ");
                st = self.move_focus.fill_style_text(st, ".");
            }
        }
        st.done()
    }
}

fn locked_interface_indication() -> LinePart {
    let locked_text = " -- INTERFACE LOCKED -- ";
    let locked_text_len = locked_text.chars().count();
    let locked_styled_text = Style::new().fg(WHITE).bold().paint(locked_text);
    LinePart {
        part: format!("{}", locked_styled_text),
        len: locked_text_len,
    }
}

fn select_pane_shortcut(is_first_shortcut: bool) -> LinePart {
    let shortcut = "ENTER";
    let description = "Select pane";
    let separator = if is_first_shortcut { " " } else { " / " };
    let separator = Style::new().fg(WHITE).paint(separator);
    let shortcut_len = shortcut.chars().count() + 3; // 2 for <>'s around shortcut, 1 for the space
    let shortcut_left_separator = Style::new().fg(WHITE).paint("<");
    let shortcut = Style::new().fg(ORANGE).bold().paint(shortcut);
    let shortcut_right_separator = Style::new().fg(WHITE).paint("> ");
    let description_len = description.chars().count();
    let description = Style::new().fg(WHITE).bold().paint(description);
    let len = shortcut_len + description_len + separator.chars().count();
    LinePart {
        part: format!(
            "{}",
            ANSIStrings(&[
                separator,
                shortcut_left_separator,
                shortcut,
                shortcut_right_separator,
                description
            ])
        ),
        len,
    }
}

fn full_shortcut_list(help: &ModeInfo, keybinds: &[(String, String)]) -> LinePart {
    match help.mode {
        InputMode::Normal => QuickNavbar::from_keybinds(help)
            .full_text()
            .to_styled_text(),
        InputMode::Locked => locked_interface_indication(),
        _ => {
            let mut line_part = LinePart::default();
            for (i, (letter, description)) in keybinds.iter().enumerate() {
                let shortcut = full_length_shortcut(i == 0, &letter, &description);
                line_part.len += shortcut.len;
                line_part.part = format!("{}{}", line_part.part, shortcut,);
            }
            let select_pane_shortcut = select_pane_shortcut(help.keybinds.is_empty());
            line_part.len += select_pane_shortcut.len;
            line_part.part = format!("{}{}", line_part.part, select_pane_shortcut,);
            line_part
        }
    }
}

fn shortened_shortcut_list(help: &ModeInfo, keybinds: &[(String, String)]) -> LinePart {
    match help.mode {
        InputMode::Normal => QuickNavbar::from_keybinds(help)
            .medium_text()
            .to_styled_text(),
        InputMode::Locked => locked_interface_indication(),
        _ => {
            let mut line_part = LinePart::default();
            for (i, (letter, description)) in keybinds.iter().enumerate() {
                let shortcut = first_word_shortcut(i == 0, &letter, &description);
                line_part.len += shortcut.len;
                line_part.part = format!("{}{}", line_part.part, shortcut,);
            }
            let select_pane_shortcut = select_pane_shortcut(help.keybinds.is_empty());
            line_part.len += select_pane_shortcut.len;
            line_part.part = format!("{}{}", line_part.part, select_pane_shortcut,);
            line_part
        }
    }
}

fn best_effort_shortcut_list(
    help: &ModeInfo,
    max_len: usize,
    keybinds: &[(String, String)],
) -> LinePart {
    match help.mode {
        InputMode::Normal => {
            let line_part = QuickNavbar::from_keybinds(help)
                .short_text()
                .to_styled_text();
            if line_part.len <= max_len {
                line_part
            } else {
                LinePart::default()
            }
        }
        InputMode::Locked => {
            let line_part = locked_interface_indication();
            if line_part.len <= max_len {
                line_part
            } else {
                LinePart::default()
            }
        }
        _ => {
            let mut line_part = LinePart::default();
            for (i, (letter, description)) in keybinds.iter().enumerate() {
                let shortcut = first_word_shortcut(i == 0, &letter, &description);
                if line_part.len + shortcut.len + MORE_MSG.chars().count() > max_len {
                    line_part.part = format!("{}{}", line_part.part, MORE_MSG);
                    line_part.len += MORE_MSG.chars().count();
                    break;
                }
                line_part.len += shortcut.len;
                line_part.part = format!("{}{}", line_part.part, shortcut);
            }
            let select_pane_shortcut = select_pane_shortcut(help.keybinds.is_empty());
            if line_part.len + select_pane_shortcut.len <= max_len {
                line_part.len += select_pane_shortcut.len;
                line_part.part = format!("{}{}", line_part.part, select_pane_shortcut,);
            }
            line_part
        }
    }
}

pub fn keybinds(help: &ModeInfo, max_width: usize) -> LinePart {
    let keybinds = get_mode_info(help.mode, &help.keybinds.clone().into_iter().collect());
    let full_shortcut_list = full_shortcut_list(help, &keybinds);
    if full_shortcut_list.len <= max_width {
        return full_shortcut_list;
    }
    let shortened_shortcut_list = shortened_shortcut_list(help, &keybinds);
    if shortened_shortcut_list.len <= max_width {
        return shortened_shortcut_list;
    }
    best_effort_shortcut_list(help, max_width, &keybinds)
}
