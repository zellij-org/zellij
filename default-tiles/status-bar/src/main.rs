use colored::*;
use std::fmt::{Display, Error, Formatter};
use zellij_tile::*;

// for more of these, copy paste from: https://en.wikipedia.org/wiki/Box-drawing_character
static ARROW_SEPARATOR: &str = " ";
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
    let prefix_text = " Zellij ";
    let part = match &help.mode {
        InputMode::Command => {
            let prefix = prefix_text.bold().white().on_black();
            let separator = ARROW_SEPARATOR.black().on_magenta();
            format!("{}{}", prefix, separator)
        }
        InputMode::Normal => {
            let prefix = prefix_text.bold().white().on_black();
            let separator = ARROW_SEPARATOR.black().on_green();
            format!("{}{}", prefix, separator)
        }
        _ => {
            let prefix = prefix_text.bold().white().on_black();
            let separator = ARROW_SEPARATOR.black().on_magenta();
            format!("{}{}", prefix, separator)
        }
    };
    let len = prefix_text.chars().count() + ARROW_SEPARATOR.chars().count();
    LinePart { part, len }
}

fn key_path(help: &Help) -> LinePart {
    let superkey_text = "<Ctrl-g> ";
    let (part, len) = match &help.mode {
        InputMode::Command => {
            let key_path = superkey_text.bold().on_magenta();
            let first_separator = ARROW_SEPARATOR.magenta().on_black();
            let len = superkey_text.chars().count()
                + ARROW_SEPARATOR.chars().count()
                + ARROW_SEPARATOR.chars().count();
            (format!("{}{}", key_path, first_separator), len)
        }
        InputMode::Resize => {
            let mode_shortcut_text = "r ";
            let superkey = superkey_text.bold().on_magenta();
            let first_superkey_separator = ARROW_SEPARATOR.magenta().on_black();
            let second_superkey_separator = ARROW_SEPARATOR.black().on_magenta();
            let mode_shortcut = mode_shortcut_text.white().bold().on_magenta();
            let mode_shortcut_separator = ARROW_SEPARATOR.magenta().on_black();
            let len = superkey_text.chars().count()
                + ARROW_SEPARATOR.chars().count()
                + ARROW_SEPARATOR.chars().count()
                + mode_shortcut_text.chars().count()
                + ARROW_SEPARATOR.chars().count();
            (
                format!(
                    "{}{}{}{}{}",
                    superkey,
                    first_superkey_separator,
                    second_superkey_separator,
                    mode_shortcut,
                    mode_shortcut_separator
                ),
                len,
            )
        }
        InputMode::Pane => {
            let mode_shortcut_text = "p ";
            let superkey = superkey_text.bold().on_magenta();
            let first_superkey_separator = ARROW_SEPARATOR.magenta().on_black();
            let second_superkey_separator = ARROW_SEPARATOR.black().on_magenta();
            let mode_shortcut = mode_shortcut_text.white().bold().on_magenta();
            let mode_shortcut_separator = ARROW_SEPARATOR.magenta().on_black();
            let len = superkey_text.chars().count()
                + ARROW_SEPARATOR.chars().count()
                + ARROW_SEPARATOR.chars().count()
                + mode_shortcut_text.chars().count()
                + ARROW_SEPARATOR.chars().count();
            (
                format!(
                    "{}{}{}{}{}",
                    superkey,
                    first_superkey_separator,
                    second_superkey_separator,
                    mode_shortcut,
                    mode_shortcut_separator
                ),
                len,
            )
        }
        InputMode::Tab | InputMode::RenameTab => {
            let mode_shortcut_text = "t ";
            let superkey = superkey_text.bold().on_magenta();
            let first_superkey_separator = ARROW_SEPARATOR.magenta().on_black();
            let second_superkey_separator = ARROW_SEPARATOR.black().on_magenta();
            let mode_shortcut = mode_shortcut_text.white().bold().on_magenta();
            let mode_shortcut_separator = ARROW_SEPARATOR.magenta().on_black();
            let len = superkey_text.chars().count()
                + ARROW_SEPARATOR.chars().count()
                + ARROW_SEPARATOR.chars().count()
                + mode_shortcut_text.chars().count()
                + ARROW_SEPARATOR.chars().count();
            (
                format!(
                    "{}{}{}{}{}",
                    superkey,
                    first_superkey_separator,
                    second_superkey_separator,
                    mode_shortcut,
                    mode_shortcut_separator
                ),
                len,
            )
        }
        InputMode::Scroll => {
            let mode_shortcut_text = "s ";
            let superkey = superkey_text.bold().on_magenta();
            let first_superkey_separator = ARROW_SEPARATOR.magenta().on_black();
            let second_superkey_separator = ARROW_SEPARATOR.black().on_magenta();
            let mode_shortcut = mode_shortcut_text.white().bold().on_magenta();
            let mode_shortcut_separator = ARROW_SEPARATOR.magenta().on_black();
            let len = superkey_text.chars().count()
                + ARROW_SEPARATOR.chars().count()
                + ARROW_SEPARATOR.chars().count()
                + mode_shortcut_text.chars().count()
                + ARROW_SEPARATOR.chars().count();
            (
                format!(
                    "{}{}{}{}{}",
                    superkey,
                    first_superkey_separator,
                    second_superkey_separator,
                    mode_shortcut,
                    mode_shortcut_separator
                ),
                len,
            )
        }
        InputMode::Normal | _ => {
            let key_path = superkey_text.on_green();
            let separator = ARROW_SEPARATOR.green().on_black();
            (
                format!("{}{}", key_path, separator),
                superkey_text.chars().count() + ARROW_SEPARATOR.chars().count(),
            )
        }
    };
    LinePart { part, len }
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
            let separator = if i > 0 { " / " } else { "" };
            let shortcut_len = shortcut.chars().count();
            let shortcut = match help.mode {
                InputMode::Normal => shortcut.cyan(),
                _ => shortcut.white().bold(),
            };
            keybinds = format!("{}{}<{}> {}", keybinds, separator, shortcut, description);
            len += shortcut_len + separator.chars().count();
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
    fn load(&mut self) {
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
        println!("{}\u{1b}[40m\u{1b}[0K", status_bar);
    }
}
