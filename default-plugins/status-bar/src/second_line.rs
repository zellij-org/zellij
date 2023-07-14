use ansi_term::{
    unstyled_len, ANSIString, ANSIStrings,
    Color::{Fixed, RGB},
    Style,
};
use zellij_tile::prelude::actions::Action;
use zellij_tile::prelude::*;
use zellij_tile_utils::palette_match;

use crate::{
    action_key, action_key_group, style_key_with_modifier,
    tip::{data::TIPS, TipFn},
    LinePart, MORE_MSG, TO_NORMAL,
};

fn full_length_shortcut(
    is_first_shortcut: bool,
    key: Vec<Key>,
    action: &str,
    palette: Palette,
) -> LinePart {
    if key.is_empty() {
        return LinePart::default();
    }

    let text_color = palette_match!(match palette.theme_hue {
        ThemeHue::Dark => palette.white,
        ThemeHue::Light => palette.black,
    });

    let separator = if is_first_shortcut { " " } else { " / " };
    let mut bits: Vec<ANSIString> = vec![Style::new().fg(text_color).paint(separator)];
    bits.extend(style_key_with_modifier(&key, &palette, None));
    bits.push(
        Style::new()
            .fg(text_color)
            .bold()
            .paint(format!(" {}", action)),
    );
    let part = ANSIStrings(&bits);

    LinePart {
        part: part.to_string(),
        len: unstyled_len(&part),
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

fn add_shortcut(help: &ModeInfo, linepart: &LinePart, text: &str, keys: Vec<Key>) -> LinePart {
    let shortcut = if linepart.len == 0 {
        full_length_shortcut(true, keys, text, help.style.colors)
    } else {
        full_length_shortcut(false, keys, text, help.style.colors)
    };

    let mut new_linepart = LinePart::default();
    new_linepart.len += linepart.len + shortcut.len;
    new_linepart.part = format!("{}{}", linepart.part, shortcut);
    new_linepart
}

fn full_shortcut_list_nonstandard_mode(help: &ModeInfo) -> LinePart {
    let mut line_part = LinePart::default();
    let keys_and_hints = get_keys_and_hints(help);

    for (long, _short, keys) in keys_and_hints.into_iter() {
        line_part = add_shortcut(help, &line_part, &long, keys.to_vec());
    }
    line_part
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
    use Direction as Dir;
    use actions::SearchDirection as SDir;
    use actions::SearchOption as SOpt;

    let mut old_keymap = mi.get_mode_keybinds();
    let s = |string: &str| string.to_string();

    // Find a keybinding to get back to "Normal" input mode. In this case we prefer '\n' over other
    // choices. Do it here before we dedupe the keymap below!
    let to_normal_keys = action_key(&old_keymap, &[TO_NORMAL]);
    let to_normal_key = if to_normal_keys.contains(&Key::Char('\n')) {
        vec![Key::Char('\n')]
    } else {
        // Yield `vec![key]` if `to_normal_keys` has at least one key, or an empty vec otherwise.
        to_normal_keys.into_iter().take(1).collect()
    };

    // Sort and deduplicate the keybindings first. We sort after the `Key`s, and deduplicate by
    // their `Action` vectors. An unstable sort is fine here because if the user maps anything to
    // the same key again, anything will happen...
    old_keymap.sort_unstable_by(|(keya, _), (keyb, _)| keya.partial_cmp(keyb).unwrap());

    let mut known_actions: Vec<Vec<Action>> = vec![];
    let mut km = vec![];
    for (key, acvec) in old_keymap {
        if known_actions.contains(&acvec) {
            // This action is known already
            continue;
        } else {
            known_actions.push(acvec.to_vec());
            km.push((key, acvec));
        }
    }

    if mi.mode == IM::Pane { vec![
        (s("Move focus"), s("Move"),
            action_key_group(&km, &[&[A::MoveFocus(Dir::Left)], &[A::MoveFocus(Dir::Down)],
                &[A::MoveFocus(Dir::Up)], &[A::MoveFocus(Dir::Right)]])),
        (s("New"), s("New"), action_key(&km, &[A::NewPane(None, None), TO_NORMAL])),
        (s("Close"), s("Close"), action_key(&km, &[A::CloseFocus, TO_NORMAL])),
        (s("Rename"), s("Rename"),
            action_key(&km, &[A::SwitchToMode(IM::RenamePane), A::PaneNameInput(vec![0])])),
        (s("Split down"), s("Down"), action_key(&km, &[A::NewPane(Some(Dir::Down), None), TO_NORMAL])),
        (s("Split right"), s("Right"), action_key(&km, &[A::NewPane(Some(Dir::Right), None), TO_NORMAL])),
        (s("Fullscreen"), s("Fullscreen"), action_key(&km, &[A::ToggleFocusFullscreen, TO_NORMAL])),
        (s("Frames"), s("Frames"), action_key(&km, &[A::TogglePaneFrames, TO_NORMAL])),
        (s("Floating toggle"), s("Floating"),
            action_key(&km, &[A::ToggleFloatingPanes, TO_NORMAL])),
        (s("Embed pane"), s("Embed"), action_key(&km, &[A::TogglePaneEmbedOrFloating, TO_NORMAL])),
        (s("Next"), s("Next"), action_key(&km, &[A::SwitchFocus])),
        (s("Select pane"), s("Select"), to_normal_key),
    ]} else if mi.mode == IM::Tab {
        // With the default bindings, "Move focus" for tabs is tricky: It binds all the arrow keys
        // to moving tabs focus (left/up go left, right/down go right). Since we sort the keys
        // above and then dedpulicate based on the actions, we will end up with LeftArrow for
        // "left" and DownArrow for "right". What we really expect is to see LeftArrow and
        // RightArrow.
        // FIXME: So for lack of a better idea we just check this case manually here.
        let old_keymap = mi.get_mode_keybinds();
        let focus_keys_full: Vec<Key> = action_key_group(&old_keymap,
            &[&[A::GoToPreviousTab], &[A::GoToNextTab]]);
        let focus_keys = if focus_keys_full.contains(&Key::Left)
            && focus_keys_full.contains(&Key::Right) {
            vec![Key::Left, Key::Right]
        } else {
            action_key_group(&km, &[&[A::GoToPreviousTab], &[A::GoToNextTab]])
        };

        vec![
        (s("Move focus"), s("Move"), focus_keys),
        (s("New"), s("New"), action_key(&km, &[A::NewTab(None, vec![], None, None, None), TO_NORMAL])),
        (s("Close"), s("Close"), action_key(&km, &[A::CloseTab, TO_NORMAL])),
        (s("Rename"), s("Rename"),
            action_key(&km, &[A::SwitchToMode(IM::RenameTab), A::TabNameInput(vec![0])])),
        (s("Sync"), s("Sync"), action_key(&km, &[A::ToggleActiveSyncTab, TO_NORMAL])),
        (s("Toggle"), s("Toggle"), action_key(&km, &[A::ToggleTab])),
        (s("Select pane"), s("Select"), to_normal_key),
    ]} else if mi.mode == IM::Resize { vec![
        (s("Increase to"), s("Increase"), action_key_group(&km, &[
            &[A::Resize(Resize::Increase, Some(Dir::Left))],
            &[A::Resize(Resize::Increase, Some(Dir::Down))],
            &[A::Resize(Resize::Increase, Some(Dir::Up))],
            &[A::Resize(Resize::Increase, Some(Dir::Right))]
            ])),
        (s("Decrease from"), s("Decrease"), action_key_group(&km, &[
            &[A::Resize(Resize::Decrease, Some(Dir::Left))],
            &[A::Resize(Resize::Decrease, Some(Dir::Down))],
            &[A::Resize(Resize::Decrease, Some(Dir::Up))],
            &[A::Resize(Resize::Decrease, Some(Dir::Right))]
            ])),
        (s("Increase/Decrease size"), s("Increase/Decrease"),
            action_key_group(&km, &[
                &[A::Resize(Resize::Increase, None)],
                &[A::Resize(Resize::Decrease, None)]
            ])),
        (s("Select pane"), s("Select"), to_normal_key),
    ]} else if mi.mode == IM::Move { vec![
        (s("Move"), s("Move"), action_key_group(&km, &[
            &[Action::MovePane(Some(Dir::Left))], &[Action::MovePane(Some(Dir::Down))],
            &[Action::MovePane(Some(Dir::Up))], &[Action::MovePane(Some(Dir::Right))]])),
        (s("Next pane"), s("Next"), action_key(&km, &[Action::MovePane(None)])),
        (s("Previous pane"), s("Previous"), action_key(&km, &[Action::MovePaneBackwards])),
    ]} else if mi.mode == IM::Scroll { vec![
        (s("Scroll"), s("Scroll"),
            action_key_group(&km, &[&[Action::ScrollDown], &[Action::ScrollUp]])),
        (s("Scroll page"), s("Scroll"),
            action_key_group(&km, &[&[Action::PageScrollDown], &[Action::PageScrollUp]])),
        (s("Scroll half page"), s("Scroll"),
            action_key_group(&km, &[&[Action::HalfPageScrollDown], &[Action::HalfPageScrollUp]])),
        (s("Edit scrollback in default editor"), s("Edit"),
            action_key(&km, &[Action::EditScrollback, TO_NORMAL])),
        (s("Enter search term"), s("Search"),
            action_key(&km, &[A::SwitchToMode(IM::EnterSearch), A::SearchInput(vec![0])])),
        (s("Select pane"), s("Select"), to_normal_key),
    ]} else if mi.mode == IM::EnterSearch { vec![
        (s("When done"), s("Done"), action_key(&km, &[A::SwitchToMode(IM::Search)])),
        (s("Cancel"), s("Cancel"),
            action_key(&km, &[A::SearchInput(vec![27]), A::SwitchToMode(IM::Scroll)])),
    ]} else if mi.mode == IM::Search { vec![
        (s("Scroll"), s("Scroll"),
            action_key_group(&km, &[&[Action::ScrollDown], &[Action::ScrollUp]])),
        (s("Scroll page"), s("Scroll"),
            action_key_group(&km, &[&[Action::PageScrollDown], &[Action::PageScrollUp]])),
        (s("Scroll half page"), s("Scroll"),
            action_key_group(&km, &[&[Action::HalfPageScrollDown], &[Action::HalfPageScrollUp]])),
        (s("Enter term"), s("Search"),
            action_key(&km, &[A::SwitchToMode(IM::EnterSearch), A::SearchInput(vec![0])])),
        (s("Search down"), s("Down"), action_key(&km, &[A::Search(SDir::Down)])),
        (s("Search up"), s("Up"), action_key(&km, &[A::Search(SDir::Up)])),
        (s("Case sensitive"), s("Case"),
            action_key(&km, &[A::SearchToggleOption(SOpt::CaseSensitivity)])),
        (s("Wrap"), s("Wrap"),
            action_key(&km, &[A::SearchToggleOption(SOpt::Wrap)])),
        (s("Whole words"), s("Whole"),
            action_key(&km, &[A::SearchToggleOption(SOpt::WholeWord)])),
    ]} else if mi.mode == IM::Session { vec![
        (s("Detach"), s("Detach"), action_key(&km, &[Action::Detach])),
        (s("Select pane"), s("Select"), to_normal_key),
    ]} else if mi.mode == IM::Tmux { vec![
        (s("Move focus"), s("Move"), action_key_group(&km, &[
            &[A::MoveFocus(Dir::Left)], &[A::MoveFocus(Dir::Down)],
            &[A::MoveFocus(Dir::Up)], &[A::MoveFocus(Dir::Right)]])),
        (s("Split down"), s("Down"), action_key(&km, &[A::NewPane(Some(Dir::Down), None), TO_NORMAL])),
        (s("Split right"), s("Right"), action_key(&km, &[A::NewPane(Some(Dir::Right), None), TO_NORMAL])),
        (s("Fullscreen"), s("Fullscreen"), action_key(&km, &[A::ToggleFocusFullscreen, TO_NORMAL])),
        (s("New tab"), s("New"), action_key(&km, &[A::NewTab(None, vec![], None, None, None), TO_NORMAL])),
        (s("Rename tab"), s("Rename"),
            action_key(&km, &[A::SwitchToMode(IM::RenameTab), A::TabNameInput(vec![0])])),
        (s("Previous Tab"), s("Previous"), action_key(&km, &[A::GoToPreviousTab, TO_NORMAL])),
        (s("Next Tab"), s("Next"), action_key(&km, &[A::GoToNextTab, TO_NORMAL])),
        (s("Select pane"), s("Select"), to_normal_key),
    ]} else if matches!(mi.mode, IM::RenamePane | IM::RenameTab) { vec![
        (s("When done"), s("Done"), to_normal_key),
        (s("Select pane"), s("Select"), action_key_group(&km, &[
            &[A::MoveFocus(Dir::Left)], &[A::MoveFocus(Dir::Down)],
            &[A::MoveFocus(Dir::Up)], &[A::MoveFocus(Dir::Right)]])),
    ]} else { vec![] }
}

fn full_shortcut_list(help: &ModeInfo, tip: TipFn) -> LinePart {
    match help.mode {
        InputMode::Normal => tip(help),
        InputMode::Locked => locked_interface_indication(help.style.colors),
        _ => full_shortcut_list_nonstandard_mode(help),
    }
}

fn shortened_shortcut_list_nonstandard_mode(help: &ModeInfo) -> LinePart {
    let mut line_part = LinePart::default();
    let keys_and_hints = get_keys_and_hints(help);

    for (_, short, keys) in keys_and_hints.into_iter() {
        line_part = add_shortcut(help, &line_part, &short, keys.to_vec());
    }
    line_part
}

fn shortened_shortcut_list(help: &ModeInfo, tip: TipFn) -> LinePart {
    match help.mode {
        InputMode::Normal => tip(help),
        InputMode::Locked => locked_interface_indication(help.style.colors),
        _ => shortened_shortcut_list_nonstandard_mode(help),
    }
}

fn best_effort_shortcut_list_nonstandard_mode(help: &ModeInfo, max_len: usize) -> LinePart {
    let mut line_part = LinePart::default();
    let keys_and_hints = get_keys_and_hints(help);

    for (_, short, keys) in keys_and_hints.into_iter() {
        let new_line_part = add_shortcut(help, &line_part, &short, keys.to_vec());
        if new_line_part.len + MORE_MSG.chars().count() > max_len {
            line_part.part = format!("{}{}", line_part.part, MORE_MSG);
            line_part.len += MORE_MSG.chars().count();
            break;
        }
        line_part = new_line_part;
    }
    line_part
}

fn best_effort_shortcut_list(help: &ModeInfo, tip: TipFn, max_len: usize) -> LinePart {
    match help.mode {
        InputMode::Normal => {
            let line_part = tip(help);
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
        _ => best_effort_shortcut_list_nonstandard_mode(help, max_len),
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

pub fn floating_panes_are_visible(mode_info: &ModeInfo) -> LinePart {
    let palette = mode_info.style.colors;
    let km = &mode_info.get_mode_keybinds();
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
    let pane_mode = format!(
        "{}",
        action_key(km, &[Action::SwitchToMode(InputMode::Pane)])
            .first()
            .unwrap_or(&Key::Char('?'))
    );
    let plus = ", ";
    let p_left_separator = "<";
    let p = format!(
        "{}",
        action_key(
            &mode_info.get_keybinds_for_mode(InputMode::Pane),
            &[Action::ToggleFloatingPanes, TO_NORMAL]
        )
        .first()
        .unwrap_or(&Key::Char('?'))
    );
    let p_right_separator = "> ";
    let to_hide = "to hide.";

    let len = floating_panes.chars().count()
        + press.chars().count()
        + pane_mode.chars().count()
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
            Style::new().fg(green_color).bold().paint(pane_mode),
            Style::new().fg(white_color).bold().paint(plus),
            Style::new().fg(white_color).bold().paint(p_left_separator),
            Style::new().fg(green_color).bold().paint(p),
            Style::new().fg(white_color).bold().paint(p_right_separator),
            Style::new().fg(white_color).bold().paint(to_hide),
        ),
        len,
    }
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

#[cfg(test)]
/// Unit tests.
///
/// Note that we cheat a little here, because the number of things one may want to test is endless,
/// and creating a Mockup of [`ModeInfo`] by hand for all these testcases is nothing less than
/// torture. Hence, we test the most atomic unit thoroughly ([`full_length_shortcut`] and then test
/// the public API ([`keybinds`]) to ensure correct operation.
mod tests {
    use super::*;

    // Strip style information from `LinePart` and return a raw String instead
    fn unstyle(line_part: LinePart) -> String {
        let string = line_part.to_string();

        let re = regex::Regex::new(r"\x1b\[[0-9;]*m").unwrap();
        let string = re.replace_all(&string, "".to_string());

        string.to_string()
    }

    fn get_palette() -> Palette {
        Palette::default()
    }

    #[test]
    fn full_length_shortcut_with_key() {
        let keyvec = vec![Key::Char('a')];
        let palette = get_palette();

        let ret = full_length_shortcut(false, keyvec, "Foobar", palette);
        let ret = unstyle(ret);

        assert_eq!(ret, " / <a> Foobar");
    }

    #[test]
    fn full_length_shortcut_with_key_first_element() {
        let keyvec = vec![Key::Char('a')];
        let palette = get_palette();

        let ret = full_length_shortcut(true, keyvec, "Foobar", palette);
        let ret = unstyle(ret);

        assert_eq!(ret, " <a> Foobar");
    }

    #[test]
    // When there is no binding, we print no shortcut either
    fn full_length_shortcut_without_key() {
        let keyvec = vec![];
        let palette = get_palette();

        let ret = full_length_shortcut(false, keyvec, "Foobar", palette);
        let ret = unstyle(ret);

        assert_eq!(ret, "");
    }

    #[test]
    fn full_length_shortcut_with_key_unprintable_1() {
        let keyvec = vec![Key::Char('\n')];
        let palette = get_palette();

        let ret = full_length_shortcut(false, keyvec, "Foobar", palette);
        let ret = unstyle(ret);

        assert_eq!(ret, " / <ENTER> Foobar");
    }

    #[test]
    fn full_length_shortcut_with_key_unprintable_2() {
        let keyvec = vec![Key::Backspace];
        let palette = get_palette();

        let ret = full_length_shortcut(false, keyvec, "Foobar", palette);
        let ret = unstyle(ret);

        assert_eq!(ret, " / <BACKSPACE> Foobar");
    }

    #[test]
    fn full_length_shortcut_with_ctrl_key() {
        let keyvec = vec![Key::Ctrl('a')];
        let palette = get_palette();

        let ret = full_length_shortcut(false, keyvec, "Foobar", palette);
        let ret = unstyle(ret);

        assert_eq!(ret, " / Ctrl + <a> Foobar");
    }

    #[test]
    fn full_length_shortcut_with_alt_key() {
        let keyvec = vec![Key::Alt(CharOrArrow::Char('a'))];
        let palette = get_palette();

        let ret = full_length_shortcut(false, keyvec, "Foobar", palette);
        let ret = unstyle(ret);

        assert_eq!(ret, " / Alt + <a> Foobar");
    }

    #[test]
    fn full_length_shortcut_with_homogenous_key_group() {
        let keyvec = vec![Key::Char('a'), Key::Char('b'), Key::Char('c')];
        let palette = get_palette();

        let ret = full_length_shortcut(false, keyvec, "Foobar", palette);
        let ret = unstyle(ret);

        assert_eq!(ret, " / <a|b|c> Foobar");
    }

    #[test]
    fn full_length_shortcut_with_heterogenous_key_group() {
        let keyvec = vec![Key::Char('a'), Key::Ctrl('b'), Key::Char('\n')];
        let palette = get_palette();

        let ret = full_length_shortcut(false, keyvec, "Foobar", palette);
        let ret = unstyle(ret);

        assert_eq!(ret, " / <a|Ctrl+b|ENTER> Foobar");
    }

    #[test]
    fn full_length_shortcut_with_key_group_shared_ctrl_modifier() {
        let keyvec = vec![Key::Ctrl('a'), Key::Ctrl('b'), Key::Ctrl('c')];
        let palette = get_palette();

        let ret = full_length_shortcut(false, keyvec, "Foobar", palette);
        let ret = unstyle(ret);

        assert_eq!(ret, " / Ctrl + <a|b|c> Foobar");
    }
    //pub fn keybinds(help: &ModeInfo, tip_name: &str, max_width: usize) -> LinePart {

    #[test]
    // Note how it leaves out elements that don't exist!
    fn keybinds_wide() {
        let mode_info = ModeInfo {
            mode: InputMode::Pane,
            keybinds: vec![(
                InputMode::Pane,
                vec![
                    (Key::Left, vec![Action::MoveFocus(Direction::Left)]),
                    (Key::Down, vec![Action::MoveFocus(Direction::Down)]),
                    (Key::Up, vec![Action::MoveFocus(Direction::Up)]),
                    (Key::Right, vec![Action::MoveFocus(Direction::Right)]),
                    (Key::Char('n'), vec![Action::NewPane(None, None), TO_NORMAL]),
                    (Key::Char('x'), vec![Action::CloseFocus, TO_NORMAL]),
                    (
                        Key::Char('f'),
                        vec![Action::ToggleFocusFullscreen, TO_NORMAL],
                    ),
                ],
            )],
            ..ModeInfo::default()
        };

        let ret = keybinds(&mode_info, "quicknav", 500);
        let ret = unstyle(ret);

        assert_eq!(
            ret,
            " <←↓↑→> Move focus / <n> New / <x> Close / <f> Fullscreen"
        );
    }

    #[test]
    // Note how "Move focus" becomes "Move"
    fn keybinds_tight_width() {
        let mode_info = ModeInfo {
            mode: InputMode::Pane,
            keybinds: vec![(
                InputMode::Pane,
                vec![
                    (Key::Left, vec![Action::MoveFocus(Direction::Left)]),
                    (Key::Down, vec![Action::MoveFocus(Direction::Down)]),
                    (Key::Up, vec![Action::MoveFocus(Direction::Up)]),
                    (Key::Right, vec![Action::MoveFocus(Direction::Right)]),
                    (Key::Char('n'), vec![Action::NewPane(None, None), TO_NORMAL]),
                    (Key::Char('x'), vec![Action::CloseFocus, TO_NORMAL]),
                    (
                        Key::Char('f'),
                        vec![Action::ToggleFocusFullscreen, TO_NORMAL],
                    ),
                ],
            )],
            ..ModeInfo::default()
        };

        let ret = keybinds(&mode_info, "quicknav", 35);
        let ret = unstyle(ret);

        assert_eq!(ret, " <←↓↑→> Move / <n> New ... ");
    }

    #[test]
    fn keybinds_wide_weird_keys() {
        let mode_info = ModeInfo {
            mode: InputMode::Pane,
            keybinds: vec![(
                InputMode::Pane,
                vec![
                    (Key::Ctrl('a'), vec![Action::MoveFocus(Direction::Left)]),
                    (Key::Ctrl('\n'), vec![Action::MoveFocus(Direction::Down)]),
                    (Key::Ctrl('1'), vec![Action::MoveFocus(Direction::Up)]),
                    (Key::Ctrl(' '), vec![Action::MoveFocus(Direction::Right)]),
                    (Key::Backspace, vec![Action::NewPane(None, None), TO_NORMAL]),
                    (Key::Esc, vec![Action::CloseFocus, TO_NORMAL]),
                    (Key::End, vec![Action::ToggleFocusFullscreen, TO_NORMAL]),
                ],
            )],
            ..ModeInfo::default()
        };

        let ret = keybinds(&mode_info, "quicknav", 500);
        let ret = unstyle(ret);

        assert_eq!(ret, " Ctrl + <a|ENTER|1|SPACE> Move focus / <BACKSPACE> New / <ESC> Close / <END> Fullscreen");
    }
}
