use colored::*;
use ansi_term::{Style, ANSIStrings};
use ansi_term::Colour::{Fixed, Black, White};
use std::fmt::{Display, Error, Formatter};
use zellij_tile::*;

// for more of these, copy paste from: https://en.wikipedia.org/wiki/Box-drawing_character
static ARROW_SEPARATOR: &str = "";
static MORE_MSG: &str = " ... ";

#[derive(Default)]
struct State {}

register_tile!(State);

struct LinePart {
    part: String,
    len: usize,
}

impl Display for LinePart {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        write!(f, "{}", self.part)
    }
}

fn prefix(help: &Help) -> LinePart {
    let prefix_text = " Ctrl + ";
    let prefix = Style::new().fg(White).on(Fixed(238)).bold().paint(prefix_text);
    LinePart {
        part: format!("{}", prefix),
        len: prefix_text.chars().count(),
    }
}

fn key_path(help: &Help) -> LinePart {
    let (part, len) = match &help.mode {
        InputMode::Locked => {
            let lock_key = selected_mode_shortcut('g', "LOCK");
            let pane_shortcut = disabled_mode_shortcut(" <p> PANE ");
            let resize_shortcut = disabled_mode_shortcut(" <r> RESIZE ");
            let tab_shortcut = disabled_mode_shortcut(" <t> TAB ");
            let scroll_shortcut = disabled_mode_shortcut(" <s> SCROLL ");
            let quit_shortcut = disabled_mode_shortcut(" <q> QUIT ");
            (
                format!("{}{}{}{}{}{}", lock_key, pane_shortcut, resize_shortcut, tab_shortcut, scroll_shortcut, quit_shortcut),
                lock_key.len + pane_shortcut.len + resize_shortcut.len + tab_shortcut.len + scroll_shortcut.len + quit_shortcut.len
            )
        }
        InputMode::Resize => {
            let lock_key = unselected_mode_shortcut('g', "BACK");
            let pane_shortcut = unselected_mode_shortcut('p', "PANE");
            let resize_shortcut = selected_mode_shortcut('r', "RESIZE");
            let tab_shortcut = unselected_mode_shortcut('t', "TAB");
            let scroll_shortcut = unselected_mode_shortcut('s', "SCROLL");
            let quit_shortcut = unselected_mode_shortcut('q', "QUIT");
            (
                format!("{}{}{}{}{}{}", lock_key, pane_shortcut, resize_shortcut, tab_shortcut, scroll_shortcut, quit_shortcut),
                lock_key.len + pane_shortcut.len + resize_shortcut.len + tab_shortcut.len + scroll_shortcut.len + quit_shortcut.len
            )
        }
        InputMode::Pane => {
            let lock_key = unselected_mode_shortcut('g', "BACK");
            let pane_shortcut = selected_mode_shortcut('p', "PANE");
            let resize_shortcut = unselected_mode_shortcut('r', "RESIZE");
            let tab_shortcut = unselected_mode_shortcut('t', "TAB");
            let scroll_shortcut = unselected_mode_shortcut('s', "SCROLL");
            let quit_shortcut = unselected_mode_shortcut('q', "QUIT");
            (
                format!("{}{}{}{}{}{}", lock_key, pane_shortcut, resize_shortcut, tab_shortcut, scroll_shortcut, quit_shortcut),
                lock_key.len + pane_shortcut.len + resize_shortcut.len + tab_shortcut.len + scroll_shortcut.len + quit_shortcut.len
            )
        }
        InputMode::Tab => {
            let lock_key = unselected_mode_shortcut('g', "BACK");
            let pane_shortcut = unselected_mode_shortcut('p', "PANE");
            let resize_shortcut = unselected_mode_shortcut('r', "RESIZE");
            let tab_shortcut = selected_mode_shortcut('t', "TAB");
            let scroll_shortcut = unselected_mode_shortcut('s', "SCROLL");
            let quit_shortcut = unselected_mode_shortcut('q', "QUIT");
            (
                format!("{}{}{}{}{}{}", lock_key, pane_shortcut, resize_shortcut, tab_shortcut, scroll_shortcut, quit_shortcut),
                lock_key.len + pane_shortcut.len + resize_shortcut.len + tab_shortcut.len + scroll_shortcut.len + quit_shortcut.len
            )
        }
        InputMode::Scroll => {
            let lock_key = unselected_mode_shortcut('g', "BACK");
            let pane_shortcut = unselected_mode_shortcut('p', "PANE");
            let resize_shortcut = unselected_mode_shortcut('r', "RESIZE");
            let tab_shortcut = unselected_mode_shortcut('t', "TAB");
            let scroll_shortcut = selected_mode_shortcut('s', "SCROLL");
            let quit_shortcut = unselected_mode_shortcut('q', "QUIT");
            (
                format!("{}{}{}{}{}{}", lock_key, pane_shortcut, resize_shortcut, tab_shortcut, scroll_shortcut, quit_shortcut),
                lock_key.len + pane_shortcut.len + resize_shortcut.len + tab_shortcut.len + scroll_shortcut.len + quit_shortcut.len
            )
        }
        InputMode::Normal | _ => {
            let lock_key = unselected_mode_shortcut('g', "LOCK");
            let pane_shortcut = unselected_mode_shortcut('p', "PANE");
            let resize_shortcut = unselected_mode_shortcut('r', "RESIZE");
            let tab_shortcut = unselected_mode_shortcut('t', "TAB");
            let scroll_shortcut = unselected_mode_shortcut('s', "SCROLL");
            let quit_shortcut = unselected_mode_shortcut('q', "QUIT");
            (
                format!("{}{}{}{}{}{}", lock_key, pane_shortcut, resize_shortcut, tab_shortcut, scroll_shortcut, quit_shortcut),
                lock_key.len + pane_shortcut.len + resize_shortcut.len + tab_shortcut.len + scroll_shortcut.len + quit_shortcut.len
            )
        }
    };
    LinePart { part, len }
}

fn unselected_mode_shortcut(letter: char, text: &str) -> LinePart {
    let prefix_separator = Style::new().fg(Fixed(238)).on(Fixed(69)).paint(ARROW_SEPARATOR);
    let char_left_separator = Style::new().bold().fg(Fixed(16)).on(Fixed(69)).bold().paint(format!(" <"));
    let char_shortcut = Style::new().bold().fg(Fixed(88)).on(Fixed(69)).bold().paint(format!("{}", letter));
    let char_right_separator = Style::new().bold().fg(Fixed(16)).on(Fixed(69)).bold().paint(format!("> "));
    let styled_text = Style::new().fg(Fixed(16)).on(Fixed(69)).bold().paint(format!("{} ", text));
    let suffix_separator = Style::new().fg(Fixed(69)).on(Fixed(238)).paint(ARROW_SEPARATOR);
    LinePart {
        part: format!("{}", ANSIStrings(&[prefix_separator, char_left_separator, char_shortcut, char_right_separator, styled_text, suffix_separator])),
        // TODO: fix length here, it doesn't account for the letter and <>s
        len: text.chars().count() + 2, // 2 for the arrows
    }
}

fn selected_mode_shortcut(letter: char, text: &str) -> LinePart {
    let prefix_separator = Style::new().fg(Fixed(238)).on(Fixed(183)).paint(ARROW_SEPARATOR);
    let char_left_separator = Style::new().bold().fg(Fixed(16)).on(Fixed(183)).bold().paint(format!(" <"));
    let char_shortcut = Style::new().bold().fg(Fixed(88)).on(Fixed(183)).bold().paint(format!("{}", letter));
    let char_right_separator = Style::new().bold().fg(Fixed(16)).on(Fixed(183)).bold().paint(format!("> "));
    let styled_text = Style::new().fg(Fixed(16)).on(Fixed(183)).bold().paint(format!("{} ", text));
    let suffix_separator = Style::new().fg(Fixed(183)).on(Fixed(238)).paint(ARROW_SEPARATOR);
    LinePart {
        // part: format!("{}{}{}{}", prefix_separator, char_shortcut, styled_text, suffix_separator),
        part: format!("{}", ANSIStrings(&[prefix_separator, char_left_separator, char_shortcut, char_right_separator, styled_text, suffix_separator])),
        len: text.chars().count() + 2, // 2 for the arrows
    }
}

fn disabled_mode_shortcut(text: &str) -> LinePart {
    let prefix_separator = Style::new().fg(Fixed(238)).on(Fixed(245)).paint(ARROW_SEPARATOR);
    let styled_text = Style::new().fg(Fixed(255)).on(Fixed(245)).dimmed().italic().paint(text);
    let suffix_separator = Style::new().fg(Fixed(245)).on(Fixed(238)).paint(ARROW_SEPARATOR);
    LinePart {
        part: format!("{}{}{}", prefix_separator, styled_text, suffix_separator),
        len: text.chars().count() + 2, // 2 for the arrows
    }
}

fn keybinds(help: &Help, max_width: usize) -> LinePart {
    let mut keybinds = String::new();
    let mut len = 0;
    let full_keybinds_len =
        help.keybinds
            .iter()
            .enumerate()
            .fold(0, |acc, (i, (shortcut, description))| {
                let shortcut_length = shortcut.chars().count() + 3; // 2 for <>'s around shortcut, 1 for the space
                let description_length = description.chars().count() + 2;
                let (_separator, separator_len) = if i > 0 { (" / ", 3) } else { ("", 0) };
                acc + shortcut_length + description_length + separator_len
            });
    if full_keybinds_len < max_width {
        for (i, (shortcut, description)) in help.keybinds.iter().enumerate() {
            let separator = if i > 0 { " / " } else { " " };
            let separator = Style::new().on(Fixed(238)).fg(Fixed(183)).paint(separator);
            let shortcut_len = shortcut.chars().count();
            let shortcut_left_separator = Style::new().on(Fixed(238)).fg(Fixed(183)).paint("<");
            let shortcut = Style::new().on(Fixed(238)).fg(Fixed(77)).bold().paint(shortcut);
            let shortcut_right_separator = Style::new().on(Fixed(238)).fg(Fixed(183)).paint("> ");
            let description = Style::new().on(Fixed(238)).fg(Fixed(183)).bold().paint(description);
            len += shortcut_len + separator.chars().count();
            keybinds = format!("{}{}", keybinds, ANSIStrings(&[separator, shortcut_left_separator, shortcut, shortcut_right_separator, description]));
        }
    } else {
        for (i, (shortcut, description)) in help.keybinds.iter().enumerate() {
            let description_first_word = description.split(' ').next().unwrap_or("");
            let current_length = keybinds.chars().count();
            let shortcut_length = shortcut.chars().count() + 3; // 2 for <>'s around shortcut, 1 for the space
            let description_first_word_length = description_first_word.chars().count();
            let (separator, separator_len) = if i > 0 { (" / ", 3) } else { ("", 0) };
            let shortcut = match help.mode {
                InputMode::Normal => shortcut.cyan(),
                _ => shortcut.white().bold(),
            };
            if current_length
                + shortcut_length
                + description_first_word_length
                + separator_len
                + MORE_MSG.chars().count()
                < max_width
            {
                keybinds = format!(
                    "{}{}<{}> {}",
                    keybinds, separator, shortcut, description_first_word
                );
                len += shortcut_length + description_first_word_length + separator_len;
            } else if current_length + shortcut_length + separator_len + MORE_MSG.chars().count()
                < max_width
            {
                keybinds = format!("{}{}<{}>", keybinds, separator, shortcut);
                len += shortcut_length + separator_len;
            } else {
                keybinds = format!("{}{}", keybinds, MORE_MSG);
                len += MORE_MSG.chars().count();
                break;
            }
        }
    }
    LinePart {
        part: keybinds,
        len,
    }
}

impl ZellijTile for State {
    fn init(&mut self) {
        set_selectable(false);
        set_invisible_borders(true);
        set_max_height(1);
    }

    fn draw(&mut self, _rows: usize, cols: usize) {
        let help = get_help();
        let line_prefix = prefix(&help);
        let key_path = key_path(&help);
        let line_len_before_keybinds = line_prefix.len + key_path.len;
        let status_bar = if line_len_before_keybinds + MORE_MSG.chars().count() < cols {
            let keybinds = keybinds(&help, cols - line_len_before_keybinds);
            let keybinds = keybinds.part.cyan().on_black();
            format!("{}{}{}", line_prefix, key_path, keybinds)
        } else if line_len_before_keybinds < cols {
            format!("{}{}", line_prefix, key_path)
        } else if line_prefix.len < cols {
            format!("{}", line_prefix)
        } else {
            // sorry, too small :(
            format!("")
        };
        // 40m is black background, 0K is so that it fills the rest of the line,
        // I could not find a way to do this with colored and did not want to have to
        // manually fill the line with spaces to achieve the same
        println!("{}\u{1b}[48;5;238m\u{1b}[0K", status_bar);
    }
}
