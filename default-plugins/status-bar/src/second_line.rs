use super::{action_key, to_normal};
use ansi_term::{
    ANSIStrings,
    Color::{Fixed, RGB},
    Style,
};
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
    let shortcut = if linepart.len == 0 {
        full_length_shortcut(true, keys, text, help.style.colors)
    } else {
        full_length_shortcut(false, keys, text, help.style.colors)
    };

    linepart.len += shortcut.len;
    linepart.part = format!("{}{}", linepart.part, shortcut);
    linepart
}

fn full_shortcut_list_nonstandard_mode(
    extra_hint_producing_function: fn(Palette) -> LinePart,
) -> impl FnOnce(&ModeInfo) -> LinePart {
    move |help| {
        let mut lp = LinePart::default();
        let keys_and_hints = get_keys_and_hints(help);

        for (long, _short, keys) in keys_and_hints.into_iter() {
            lp = add_shortcut(help, lp, &long, keys.to_vec());
        }

        let select_pane_shortcut = extra_hint_producing_function(help.style.colors);
        lp.len += select_pane_shortcut.len;
        lp.part = format!("{}{}", lp.part, select_pane_shortcut,);
        lp
    }
}

/// Collect all relevant keybindings and hints to display.
///
/// Creates a vector with tuples containing the following entries:
///
/// - A String to display for this keybinding when there are no size restrictions,
/// - A shortened String (where sensible) to display if the whole second line becomes too long,
/// - A `Vec<Key>` of the keys that map to this keyhint
///
/// This vector is created by iterating over the keybindings for the current [`InputMode`] and
/// storing all Keybindings that match pre-defined patterns of `Action`s. For example, the
/// `InputMode::Pane` input mode determines which keys to display for the "Move focus" hint by
/// searching the keybindings for anything that matches the `Action::MoveFocus(_)` action. Since by
/// default multiple keybindings map to some action patterns (e.g. `Action::MoveFocus(_)` is bound
/// to "hjkl", the arrow keys and "Alt + <hjkl>"), we deduplicate the vector of all keybindings
/// before processing it.
///
/// Therefore we sort it by the [`Key`]s of the current keymap and deduplicate the resulting sorted
/// vector by the `Vec<Action>` action vectors bound to the keys. As such, when multiple keys map
/// to the same sequence of actions, the keys that appear first in the [`Key`] structure will be
/// displayed.
// Please don't let rustfmt play with the formatting. It will stretch out the function to about
// three times the length and all the keybinding vectors we generate become virtually unreadable
// for humans.
#[rustfmt::skip]
fn get_keys_and_hints(mi: &ModeInfo) -> Vec<(String, String, Vec<Key>)> {
    use Action as A;
    use InputMode as IM;
    use actions::Direction as Dir;
    use actions::ResizeDirection as RDir;

    let mut old_keymap = mi.keybinds.clone();
    let s = |string: &str| string.to_string();

    // Sort and deduplicate the keybindings first. We sort after the `Key`s, and deduplicate by
    // their `Action` vectors. An unstable sort is fine here because if the user maps anything to
    // the same key again, anything will happen...
    old_keymap.sort_unstable_by(|(keya, _), (keyb, _)| keya.partial_cmp(keyb).unwrap());

    let mut known_actions: Vec<Vec<Action>> = vec![];
    let mut km = vec![];
    for (key, acvec) in old_keymap.into_iter() {
        if known_actions.contains(&acvec) {
            // This action is known already
            continue;
        } else {
            known_actions.push(acvec.to_vec());
            km.push((key, acvec));
        }
    }

    return if mi.mode == IM::Pane { vec![
        (s("Move focus"), s("Move"), action_key!(km, A::MoveFocus(_))),
        (s("New"), s("New"), action_key!(km, A::NewPane(None), to_normal!())),
        (s("Close"), s("Close"), action_key!(km, A::CloseFocus, to_normal!())),
        (s("Rename"), s("Rename"), action_key!(km, A::SwitchToMode(IM::RenamePane), A::PaneNameInput(_))),
        (s("Split down"), s("Down"), action_key!(km, A::NewPane(Some(Dir::Down)), to_normal!())),
        (s("Split right"), s("Right"), action_key!(km, A::NewPane(Some(Dir::Right)), to_normal!())),
        (s("Fullscreen"), s("Fullscreen"), action_key!(km, A::ToggleFocusFullscreen, to_normal!())),
        (s("Frames"), s("Frames"), action_key!(km, A::TogglePaneFrames, to_normal!())),
        (s("Floating toggle"), s("Floating"), action_key!(km, A::ToggleFloatingPanes, to_normal!())),
        (s("Embed pane"), s("Embed"), action_key!(km, A::TogglePaneEmbedOrFloating, to_normal!())),
        (s("Next"), s("Next"), action_key!(km, A::SwitchFocus)),
    ]} else if mi.mode == IM::Tab { vec![
        (s("Move focus"), s("Move"), action_key!(km, A::GoToPreviousTab).into_iter()
                    .chain(action_key!(km, A::GoToNextTab).into_iter()).collect()),
        (s("New"), s("New"), action_key!(km, A::NewTab(None), to_normal!())),
        (s("Close"), s("Close"), action_key!(km, A::CloseTab, to_normal!())),
        (s("Rename"), s("Rename"), action_key!(km, A::SwitchToMode(IM::RenameTab), A::TabNameInput(_))),
        (s("Sync"), s("Sync"), action_key!(km, A::ToggleActiveSyncTab, to_normal!())),
        (s("Toggle"), s("Toggle"), action_key!(km, A::ToggleTab)),
    ]} else if mi.mode == IM::Resize { vec![
        (s("Resize"), s("Resize"), action_key!(km, A::Resize(RDir::Left)).into_iter()
                    .chain(action_key!(km, A::Resize(RDir::Down)).into_iter())
                    .chain(action_key!(km, A::Resize(RDir::Up)).into_iter())
                    .chain(action_key!(km, A::Resize(RDir::Right)).into_iter())
                    .collect::<Vec<Key>>()),
        (s("Increase/Decrease size"), s("Increase/Decrease"),
            action_key!(km, A::Resize(RDir::Increase)).into_iter()
                    .chain(action_key!(km, A::Resize(RDir::Decrease)).into_iter()).collect()),
    ]} else if mi.mode == IM::Move { vec![
        (s("Move"), s("Move"), action_key!(km, Action::MovePane(Some(_)))),
        (s("Next pane"), s("Next"), action_key!(km, Action::MovePane(None))),
    ]} else if mi.mode == IM::Scroll { vec![
        (s("Scroll"), s("Scroll"), action_key!(km, Action::ScrollDown).into_iter()
                    .chain(action_key!(km, Action::ScrollUp).into_iter()).collect()),
        (s("Scroll page"), s("Scroll"), action_key!(km, Action::PageScrollDown).into_iter()
                    .chain(action_key!(km, Action::PageScrollUp).into_iter()).collect()),
        (s("Scroll half page"), s("Scroll"), action_key!(km, Action::HalfPageScrollDown).into_iter()
                    .chain(action_key!(km, Action::HalfPageScrollUp).into_iter()).collect()),
        (s("Edit scrollback in default editor"), s("Edit"),
            action_key!(km, Action::EditScrollback, to_normal!())),
    ]} else if mi.mode == IM::Scroll { vec![
        (s("Detach"), s("Detach"), action_key!(km, Action::Detach)),
    ]} else { vec![] };
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
        let keys_and_hints = get_keys_and_hints(help);

        for (_, short, keys) in keys_and_hints.into_iter() {
            line_part = add_shortcut(help, line_part, &short, keys.to_vec());
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
