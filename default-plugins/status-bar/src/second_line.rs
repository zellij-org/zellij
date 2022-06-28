use super::{action_key, to_normal};
use ansi_term::{
    ANSIStrings,
    Color::{Fixed, RGB},
    Style,
};
use std::collections::VecDeque;
use zellij_tile::prelude::actions::Action;
use zellij_tile::prelude::*;
use zellij_tile_utils::palette_match;

use crate::{
    tip::{data::TIPS, TipFn},
    LinePart, MORE_MSG,
};

#[derive(Clone, Copy)]
enum StatusBarTextColor {
    White,
    Green,
    Orange,
}

#[derive(Clone, Copy)]
enum StatusBarTextBoldness {
    Bold,
    NotBold,
}

fn full_length_shortcut(
    is_first_shortcut: bool,
    key: Vec<Key>,
    action: &str,
    palette: Palette,
) -> LinePart {
    let text_color = palette_match!(match palette.theme_hue {
        ThemeHue::Dark => palette.white,
        ThemeHue::Light => palette.black,
    });
    let key = key
        .iter()
        .map(|key| format!("{}", key))
        .collect::<Vec<String>>()
        .join("");
    if key.is_empty() {
        return LinePart {
            part: "".to_string(),
            len: 0,
        };
    }
    let green_color = palette_match!(palette.green);
    let separator = if is_first_shortcut { " " } else { " / " };
    let separator = Style::new().fg(text_color).paint(separator);
    let shortcut_len = key.chars().count() + 3; // 2 for <>'s around shortcut, 1 for the space
    let shortcut_left_separator = Style::new().fg(text_color).paint("<");
    let shortcut = Style::new().fg(green_color).bold().paint(key);
    let shortcut_right_separator = Style::new().fg(text_color).paint("> ");
    let action_len = action.chars().count();
    let action = Style::new().fg(text_color).bold().paint(action);
    let len = shortcut_len + action_len + separator.chars().count();
    LinePart {
        part: ANSIStrings(&[
            separator,
            shortcut_left_separator,
            shortcut,
            shortcut_right_separator,
            action,
        ])
        .to_string(),
        len,
    }
}

fn first_word_shortcut(
    is_first_shortcut: bool,
    key: &Key,
    _action: &[Action],
    palette: Palette,
) -> LinePart {
    let text_color = palette_match!(match palette.theme_hue {
        ThemeHue::Dark => palette.white,
        ThemeHue::Light => palette.black,
    });
    let letter = format!("{}", key);
    let description = "test".to_string();
    let green_color = palette_match!(palette.green);
    let separator = if is_first_shortcut { " " } else { " / " };
    let separator = Style::new().fg(text_color).paint(separator);
    let shortcut_len = letter.chars().count() + 3; // 2 for <>'s around shortcut, 1 for the space
    let shortcut_left_separator = Style::new().fg(text_color).paint("<");
    let shortcut = Style::new().fg(green_color).bold().paint(letter);
    let shortcut_right_separator = Style::new().fg(text_color).paint("> ");
    let description_first_word = description.split(' ').next().unwrap_or("");
    let description_first_word_length = description_first_word.chars().count();
    let description_first_word = Style::new()
        .fg(text_color)
        .bold()
        .paint(description_first_word);
    let len = shortcut_len + description_first_word_length + separator.chars().count();
    LinePart {
        part: ANSIStrings(&[
            separator,
            shortcut_left_separator,
            shortcut,
            shortcut_right_separator,
            description_first_word,
        ])
        .to_string(),
        len,
    }
}

fn locked_interface_indication(palette: Palette) -> LinePart {
    let locked_text = " -- INTERFACE LOCKED -- ";
    let locked_text_len = locked_text.chars().count();
    let text_color = palette_match!(match palette.theme_hue {
        ThemeHue::Dark => palette.white,
        ThemeHue::Light => palette.black,
    });
    let locked_styled_text = Style::new().fg(text_color).bold().paint(locked_text);
    LinePart {
        part: locked_styled_text.to_string(),
        len: locked_text_len,
    }
}

fn show_extra_hints(
    palette: Palette,
    text_with_style: Vec<(&str, StatusBarTextColor, StatusBarTextBoldness)>,
) -> LinePart {
    use StatusBarTextBoldness::*;
    use StatusBarTextColor::*;
    // get the colors
    let white_color = palette_match!(palette.white);
    let green_color = palette_match!(palette.green);
    let orange_color = palette_match!(palette.orange);
    // calculate length of tipp
    let len = text_with_style
        .iter()
        .fold(0, |len_sum, (text, _, _)| len_sum + text.chars().count());
    // apply the styles defined above
    let styled_text = text_with_style
        .into_iter()
        .map(|(text, color, is_bold)| {
            let color = match color {
                White => white_color,
                Green => green_color,
                Orange => orange_color,
            };
            match is_bold {
                Bold => Style::new().fg(color).bold().paint(text),
                NotBold => Style::new().fg(color).paint(text),
            }
        })
        .collect::<Vec<_>>();
    LinePart {
        part: ANSIStrings(&styled_text[..]).to_string(),
        len,
    }
}

/// Creates hints for usage of Pane Mode
fn confirm_pane_selection(palette: Palette) -> LinePart {
    use StatusBarTextBoldness::*;
    use StatusBarTextColor::*;
    let text_with_style = [
        (" / ", White, NotBold),
        ("<ENTER>", Green, Bold),
        (" Select pane", White, Bold),
    ];
    show_extra_hints(palette, text_with_style.to_vec())
}

/// Creates hints for usage of Rename Mode in Pane Mode
fn select_pane_shortcut(palette: Palette) -> LinePart {
    use StatusBarTextBoldness::*;
    use StatusBarTextColor::*;
    let text_with_style = [
        (" / ", White, NotBold),
        ("Alt", Orange, Bold),
        (" + ", White, NotBold),
        ("<", Green, Bold),
        ("[]", Green, Bold),
        (" or ", White, NotBold),
        ("hjkl", Green, Bold),
        (">", Green, Bold),
        (" Select pane", White, Bold),
    ];
    show_extra_hints(palette, text_with_style.to_vec())
}

fn add_shortcut(help: &ModeInfo, mut linepart: LinePart, text: &str, keys: Vec<Key>) -> LinePart {
    let shortcut = full_length_shortcut(false, keys, text, help.style.colors);
    linepart.len += shortcut.len;
    linepart.part = format!("{}{}", linepart.part, shortcut);
    linepart
}

fn full_shortcut_list_nonstandard_mode(
    extra_hint_producing_function: fn(Palette) -> LinePart,
) -> impl FnOnce(&ModeInfo) -> LinePart {
    move |help| {
        let km: VecDeque<(Key, Vec<Action>)> = help.keybinds.clone().into();
        let mut lp = LinePart::default();

        if help.mode == InputMode::Pane || help.mode == InputMode::Tab {
            // Shared keybindings
            lp = add_shortcut(
                help,
                lp,
                "Move Focus",
                action_key!(km, Action::MoveFocus(_)),
            );
            lp = add_shortcut(help, lp, "New", action_key!(km, Action::NewPane(None)));
            lp = add_shortcut(
                help,
                lp,
                "New",
                action_key!(km, Action::NewTab(_), to_normal!()),
            );
            lp = add_shortcut(
                help,
                lp,
                "Close",
                action_key!(km, Action::CloseFocus, to_normal!()),
            );
            lp = add_shortcut(
                help,
                lp,
                "Rename",
                action_key!(
                    km,
                    Action::SwitchToMode(InputMode::RenamePane),
                    Action::PaneNameInput(_)
                ),
            );
            lp = add_shortcut(
                help,
                lp,
                "Rename",
                action_key!(
                    km,
                    Action::SwitchToMode(InputMode::RenameTab),
                    Action::TabNameInput(_)
                ),
            );
        }

        // Pane keybindings
        lp = add_shortcut(
            help,
            lp,
            "Split Down",
            action_key!(
                km,
                Action::NewPane(Some(actions::Direction::Down)),
                to_normal!()
            ),
        );
        lp = add_shortcut(
            help,
            lp,
            "Split Right",
            action_key!(
                km,
                Action::NewPane(Some(actions::Direction::Right)),
                to_normal!()
            ),
        );
        lp = add_shortcut(
            help,
            lp,
            "Fullscreen",
            action_key!(km, Action::ToggleFocusFullscreen, to_normal!()),
        );
        lp = add_shortcut(
            help,
            lp,
            "Frames",
            action_key!(km, Action::TogglePaneFrames, to_normal!()),
        );
        lp = add_shortcut(
            help,
            lp,
            "Floating Toggle",
            action_key!(km, Action::ToggleFloatingPanes, to_normal!()),
        );
        lp = add_shortcut(
            help,
            lp,
            "Embed Pane",
            action_key!(km, Action::TogglePaneEmbedOrFloating, to_normal!()),
        );
        lp = add_shortcut(help, lp, "Next", action_key!(km, Action::SwitchFocus));

        // Tab keybindings
        lp = add_shortcut(
            help,
            lp,
            "Sync",
            action_key!(km, Action::ToggleActiveSyncTab, to_normal!()),
        );
        lp = add_shortcut(help, lp, "Toggle", action_key!(km, Action::ToggleTab));

        // Resize keybindings
        // By default these are defined in every mode
        // Arrow keys
        if help.mode == InputMode::Resize {
            let arrow_keys = action_key!(km, Action::Resize(actions::ResizeDirection::Left))
                .into_iter()
                .chain(action_key!(km, Action::Resize(actions::ResizeDirection::Down)).into_iter())
                .chain(action_key!(km, Action::Resize(actions::ResizeDirection::Up)).into_iter())
                .chain(action_key!(km, Action::Resize(actions::ResizeDirection::Right)).into_iter())
                .collect();
            lp = add_shortcut(help, lp, "Resize", arrow_keys);

            let pme = action_key!(km, Action::Resize(actions::ResizeDirection::Increase))
                .into_iter()
                .chain(
                    action_key!(km, Action::Resize(actions::ResizeDirection::Decrease)).into_iter(),
                )
                .collect();
            lp = add_shortcut(help, lp, "Increase/Decrease Size", pme);
        }

        // Move keybindings
        lp = add_shortcut(help, lp, "Move", action_key!(km, Action::MovePane(Some(_))));
        lp = add_shortcut(
            help,
            lp,
            "Next Pane",
            action_key!(km, Action::MovePane(None)),
        );

        // Scroll keybindings
        // arrows - Scroll
        // Pg - Scroll Page
        // ud - Scroll Half Page
        lp = add_shortcut(
            help,
            lp,
            "Edit Scrollback in Default Editor",
            action_key!(km, Action::EditScrollback, to_normal!()),
        );

        // Session keybindings
        lp = add_shortcut(help, lp, "Detach", action_key!(km, Action::Detach));

        //for (i, (key, action)) in help.keybinds.iter().enumerate() {
        //    let shortcut = full_length_shortcut(i == 0, key, action, help.style.colors);
        //    lp.len += shortcut.len;
        //    lp.part = format!("{}{}", line_part.part, shortcut,);
        //}
        let select_pane_shortcut = extra_hint_producing_function(help.style.colors);
        lp.len += select_pane_shortcut.len;
        lp.part = format!("{}{}", lp.part, select_pane_shortcut,);
        lp
    }
}

fn full_shortcut_list(help: &ModeInfo, tip: TipFn) -> LinePart {
    match help.mode {
        InputMode::Normal => tip(help.style.colors),
        InputMode::Locked => locked_interface_indication(help.style.colors),
        InputMode::Tmux => full_tmux_mode_indication(help),
        InputMode::RenamePane => full_shortcut_list_nonstandard_mode(select_pane_shortcut)(help),
        InputMode::EnterSearch => full_shortcut_list_nonstandard_mode(select_pane_shortcut)(help),
        _ => full_shortcut_list_nonstandard_mode(confirm_pane_selection)(help),
    }
}

fn shortened_shortcut_list_nonstandard_mode(
    extra_hint_producing_function: fn(Palette) -> LinePart,
) -> impl FnOnce(&ModeInfo) -> LinePart {
    move |help| {
        let mut line_part = LinePart::default();
        for (i, (letter, description)) in help.keybinds.iter().enumerate() {
            let shortcut = first_word_shortcut(i == 0, letter, description, help.style.colors);
            line_part.len += shortcut.len;
            line_part.part = format!("{}{}", line_part.part, shortcut,);
        }
        let select_pane_shortcut = extra_hint_producing_function(help.style.colors);
        line_part.len += select_pane_shortcut.len;
        line_part.part = format!("{}{}", line_part.part, select_pane_shortcut,);
        line_part
    }
}

fn shortened_shortcut_list(help: &ModeInfo, tip: TipFn) -> LinePart {
    match help.mode {
        InputMode::Normal => tip(help.style.colors),
        InputMode::Locked => locked_interface_indication(help.style.colors),
        InputMode::Tmux => short_tmux_mode_indication(help),
        InputMode::RenamePane => {
            shortened_shortcut_list_nonstandard_mode(select_pane_shortcut)(help)
        },
        InputMode::EnterSearch => {
            shortened_shortcut_list_nonstandard_mode(select_pane_shortcut)(help)
        },
        _ => shortened_shortcut_list_nonstandard_mode(confirm_pane_selection)(help),
    }
}

fn best_effort_shortcut_list_nonstandard_mode(
    extra_hint_producing_function: fn(Palette) -> LinePart,
) -> impl FnOnce(&ModeInfo, usize) -> LinePart {
    move |help, max_len| {
        let mut line_part = LinePart::default();
        for (i, (letter, description)) in help.keybinds.iter().enumerate() {
            let shortcut = first_word_shortcut(i == 0, letter, description, help.style.colors);
            if line_part.len + shortcut.len + MORE_MSG.chars().count() > max_len {
                // TODO: better
                line_part.part = format!("{}{}", line_part.part, MORE_MSG);
                line_part.len += MORE_MSG.chars().count();
                break;
            }
            line_part.len += shortcut.len;
            line_part.part = format!("{}{}", line_part.part, shortcut);
        }
        let select_pane_shortcut = extra_hint_producing_function(help.style.colors);
        if line_part.len + select_pane_shortcut.len <= max_len {
            line_part.len += select_pane_shortcut.len;
            line_part.part = format!("{}{}", line_part.part, select_pane_shortcut,);
        }
        line_part
    }
}

fn best_effort_tmux_shortcut_list(help: &ModeInfo, max_len: usize) -> LinePart {
    let mut line_part = tmux_mode_indication(help);
    for (i, (letter, description)) in help.keybinds.iter().enumerate() {
        let shortcut = first_word_shortcut(i == 0, letter, description, help.style.colors);
        if line_part.len + shortcut.len + MORE_MSG.chars().count() > max_len {
            // TODO: better
            line_part.part = format!("{}{}", line_part.part, MORE_MSG);
            line_part.len += MORE_MSG.chars().count();
            break;
        }
        line_part.len += shortcut.len;
        line_part.part = format!("{}{}", line_part.part, shortcut);
    }
    line_part
}

fn best_effort_shortcut_list(help: &ModeInfo, tip: TipFn, max_len: usize) -> LinePart {
    match help.mode {
        InputMode::Normal => {
            let line_part = tip(help.style.colors);
            if line_part.len <= max_len {
                line_part
            } else {
                LinePart::default()
            }
        },
        InputMode::Locked => {
            let line_part = locked_interface_indication(help.style.colors);
            if line_part.len <= max_len {
                line_part
            } else {
                LinePart::default()
            }
        },
        InputMode::Tmux => best_effort_tmux_shortcut_list(help, max_len),
        InputMode::RenamePane => {
            best_effort_shortcut_list_nonstandard_mode(select_pane_shortcut)(help, max_len)
        },
        _ => best_effort_shortcut_list_nonstandard_mode(confirm_pane_selection)(help, max_len),
    }
}

pub fn keybinds(help: &ModeInfo, tip_name: &str, max_width: usize) -> LinePart {
    // It is assumed that there is at least one TIP data in the TIPS HasMap.
    let tip_body = TIPS
        .get(tip_name)
        .unwrap_or_else(|| TIPS.get("quicknav").unwrap());

    let full_shortcut_list = full_shortcut_list(help, tip_body.full);
    if full_shortcut_list.len <= max_width {
        return full_shortcut_list;
    }
    let shortened_shortcut_list = shortened_shortcut_list(help, tip_body.medium);
    if shortened_shortcut_list.len <= max_width {
        return shortened_shortcut_list;
    }
    best_effort_shortcut_list(help, tip_body.short, max_width)
}

pub fn text_copied_hint(palette: &Palette, copy_destination: CopyDestination) -> LinePart {
    let green_color = palette_match!(palette.green);
    let hint = match copy_destination {
        CopyDestination::Command => "Text piped to external command",
        #[cfg(not(target_os = "macos"))]
        CopyDestination::Primary => "Text copied to system primary selection",
        #[cfg(target_os = "macos")] // primary selection does not exist on macos
        CopyDestination::Primary => "Text copied to system clipboard",
        CopyDestination::System => "Text copied to system clipboard",
    };
    LinePart {
        part: Style::new().fg(green_color).bold().paint(hint).to_string(),
        len: hint.len(),
    }
}

pub fn system_clipboard_error(palette: &Palette) -> LinePart {
    let hint = " Error using the system clipboard.";
    let red_color = palette_match!(palette.red);
    LinePart {
        part: Style::new().fg(red_color).bold().paint(hint).to_string(),
        len: hint.len(),
    }
}

pub fn fullscreen_panes_to_hide(palette: &Palette, panes_to_hide: usize) -> LinePart {
    let text_color = palette_match!(match palette.theme_hue {
        ThemeHue::Dark => palette.white,
        ThemeHue::Light => palette.black,
    });
    let green_color = palette_match!(palette.green);
    let orange_color = palette_match!(palette.orange);
    let shortcut_left_separator = Style::new().fg(text_color).bold().paint(" (");
    let shortcut_right_separator = Style::new().fg(text_color).bold().paint("): ");
    let fullscreen = "FULLSCREEN";
    let puls = "+ ";
    let panes = panes_to_hide.to_string();
    let hide = " hidden panes";
    let len = fullscreen.chars().count()
        + puls.chars().count()
        + panes.chars().count()
        + hide.chars().count()
        + 5; // 3 for ():'s around shortcut, 2 for the space
    LinePart {
        part: format!(
            "{}{}{}{}{}{}",
            shortcut_left_separator,
            Style::new().fg(orange_color).bold().paint(fullscreen),
            shortcut_right_separator,
            Style::new().fg(text_color).bold().paint(puls),
            Style::new().fg(green_color).bold().paint(panes),
            Style::new().fg(text_color).bold().paint(hide)
        ),
        len,
    }
}

pub fn floating_panes_are_visible(palette: &Palette) -> LinePart {
    let white_color = match palette.white {
        PaletteColor::Rgb((r, g, b)) => RGB(r, g, b),
        PaletteColor::EightBit(color) => Fixed(color),
    };
    let green_color = match palette.green {
        PaletteColor::Rgb((r, g, b)) => RGB(r, g, b),
        PaletteColor::EightBit(color) => Fixed(color),
    };
    let orange_color = match palette.orange {
        PaletteColor::Rgb((r, g, b)) => RGB(r, g, b),
        PaletteColor::EightBit(color) => Fixed(color),
    };
    let shortcut_left_separator = Style::new().fg(white_color).bold().paint(" (");
    let shortcut_right_separator = Style::new().fg(white_color).bold().paint("): ");
    let floating_panes = "FLOATING PANES VISIBLE";
    let press = "Press ";
    let ctrl = "Ctrl-p ";
    let plus = "+ ";
    let p_left_separator = "<";
    let p = "w";
    let p_right_separator = "> ";
    let to_hide = "to hide.";

    let len = floating_panes.chars().count()
        + press.chars().count()
        + ctrl.chars().count()
        + plus.chars().count()
        + p_left_separator.chars().count()
        + p.chars().count()
        + p_right_separator.chars().count()
        + to_hide.chars().count()
        + 5; // 3 for ():'s around floating_panes, 2 for the space
    LinePart {
        part: format!(
            "{}{}{}{}{}{}{}{}{}{}",
            shortcut_left_separator,
            Style::new().fg(orange_color).bold().paint(floating_panes),
            shortcut_right_separator,
            Style::new().fg(white_color).bold().paint(press),
            Style::new().fg(green_color).bold().paint(ctrl),
            Style::new().fg(white_color).bold().paint(plus),
            Style::new().fg(white_color).bold().paint(p_left_separator),
            Style::new().fg(green_color).bold().paint(p),
            Style::new().fg(white_color).bold().paint(p_right_separator),
            Style::new().fg(white_color).bold().paint(to_hide),
        ),
        len,
    }
}

pub fn tmux_mode_indication(help: &ModeInfo) -> LinePart {
    let white_color = match help.style.colors.white {
        PaletteColor::Rgb((r, g, b)) => RGB(r, g, b),
        PaletteColor::EightBit(color) => Fixed(color),
    };
    let orange_color = match help.style.colors.orange {
        PaletteColor::Rgb((r, g, b)) => RGB(r, g, b),
        PaletteColor::EightBit(color) => Fixed(color),
    };

    let shortcut_left_separator = Style::new().fg(white_color).bold().paint(" (");
    let shortcut_right_separator = Style::new().fg(white_color).bold().paint("): ");
    let tmux_mode_text = "TMUX MODE";
    let tmux_mode_indicator = Style::new().fg(orange_color).bold().paint(tmux_mode_text);
    let line_part = LinePart {
        part: format!(
            "{}{}{}",
            shortcut_left_separator, tmux_mode_indicator, shortcut_right_separator
        ),
        len: tmux_mode_text.chars().count() + 5, // 2 for the separators, 3 for the colon and following space
    };
    line_part
}

pub fn full_tmux_mode_indication(help: &ModeInfo) -> LinePart {
    let white_color = match help.style.colors.white {
        PaletteColor::Rgb((r, g, b)) => RGB(r, g, b),
        PaletteColor::EightBit(color) => Fixed(color),
    };
    let orange_color = match help.style.colors.orange {
        PaletteColor::Rgb((r, g, b)) => RGB(r, g, b),
        PaletteColor::EightBit(color) => Fixed(color),
    };

    let shortcut_left_separator = Style::new().fg(white_color).bold().paint(" (");
    let shortcut_right_separator = Style::new().fg(white_color).bold().paint("): ");
    let tmux_mode_text = "TMUX MODE";
    let tmux_mode_indicator = Style::new().fg(orange_color).bold().paint(tmux_mode_text);
    let line_part = LinePart {
        part: format!(
            "{}{}{}",
            shortcut_left_separator, tmux_mode_indicator, shortcut_right_separator
        ),
        len: tmux_mode_text.chars().count() + 5, // 2 for the separators, 3 for the colon and following space
    };
    line_part
}

pub fn short_tmux_mode_indication(help: &ModeInfo) -> LinePart {
    let white_color = match help.style.colors.white {
        PaletteColor::Rgb((r, g, b)) => RGB(r, g, b),
        PaletteColor::EightBit(color) => Fixed(color),
    };
    let orange_color = match help.style.colors.orange {
        PaletteColor::Rgb((r, g, b)) => RGB(r, g, b),
        PaletteColor::EightBit(color) => Fixed(color),
    };

    let shortcut_left_separator = Style::new().fg(white_color).bold().paint(" (");
    let shortcut_right_separator = Style::new().fg(white_color).bold().paint("): ");
    let tmux_mode_text = "TMUX MODE";
    let tmux_mode_indicator = Style::new().fg(orange_color).bold().paint(tmux_mode_text);
    let mut line_part = LinePart {
        part: format!(
            "{}{}{}",
            shortcut_left_separator, tmux_mode_indicator, shortcut_right_separator
        ),
        len: tmux_mode_text.chars().count() + 5, // 2 for the separators, 3 for the colon and following space
    };

    for (i, (letter, description)) in help.keybinds.iter().enumerate() {
        let shortcut = first_word_shortcut(i == 0, letter, description, help.style.colors);
        line_part.len += shortcut.len;
        line_part.part = format!("{}{}", line_part.part, shortcut);
    }
    line_part
}

pub fn locked_fullscreen_panes_to_hide(palette: &Palette, panes_to_hide: usize) -> LinePart {
    let text_color = palette_match!(match palette.theme_hue {
        ThemeHue::Dark => palette.white,
        ThemeHue::Light => palette.black,
    });
    let green_color = palette_match!(palette.green);
    let orange_color = palette_match!(palette.orange);
    let locked_text = " -- INTERFACE LOCKED -- ";
    let shortcut_left_separator = Style::new().fg(text_color).bold().paint(" (");
    let shortcut_right_separator = Style::new().fg(text_color).bold().paint("): ");
    let fullscreen = "FULLSCREEN";
    let puls = "+ ";
    let panes = panes_to_hide.to_string();
    let hide = " hidden panes";
    let len = locked_text.chars().count()
        + fullscreen.chars().count()
        + puls.chars().count()
        + panes.chars().count()
        + hide.chars().count()
        + 5; // 3 for ():'s around shortcut, 2 for the space
    LinePart {
        part: format!(
            "{}{}{}{}{}{}{}",
            Style::new().fg(text_color).bold().paint(locked_text),
            shortcut_left_separator,
            Style::new().fg(orange_color).bold().paint(fullscreen),
            shortcut_right_separator,
            Style::new().fg(text_color).bold().paint(puls),
            Style::new().fg(green_color).bold().paint(panes),
            Style::new().fg(text_color).bold().paint(hide)
        ),
        len,
    }
}

pub fn locked_floating_panes_are_visible(palette: &Palette) -> LinePart {
    let white_color = match palette.white {
        PaletteColor::Rgb((r, g, b)) => RGB(r, g, b),
        PaletteColor::EightBit(color) => Fixed(color),
    };
    let orange_color = match palette.orange {
        PaletteColor::Rgb((r, g, b)) => RGB(r, g, b),
        PaletteColor::EightBit(color) => Fixed(color),
    };
    let shortcut_left_separator = Style::new().fg(white_color).bold().paint(" (");
    let shortcut_right_separator = Style::new().fg(white_color).bold().paint(")");
    let locked_text = " -- INTERFACE LOCKED -- ";
    let floating_panes = "FLOATING PANES VISIBLE";

    let len = locked_text.chars().count() + floating_panes.chars().count();
    LinePart {
        part: format!(
            "{}{}{}{}",
            Style::new().fg(white_color).bold().paint(locked_text),
            shortcut_left_separator,
            Style::new().fg(orange_color).bold().paint(floating_panes),
            shortcut_right_separator,
        ),
        len,
    }
}
