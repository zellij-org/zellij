use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use zellij_tile::prelude::actions::Action;
use zellij_tile::prelude::desktop_integration::{
    opt_out_kdl, window_welcome_points, Platform, REMOTE_WARNING, WINDOW_WELCOME_EMPHASIS,
};
use zellij_tile::prelude::*;

use crate::active_component::ActiveComponent;
use crate::keybindings::{Feature, KeybindingState};
use crate::pages::{ComponentLine, Page, PageKind, TextOrCustomRender};

pub const DESKTOP_TIP_INDEX: usize = 12;
pub const LAST_ONBOARDING_TIP_INDEX: usize = 16;
pub const HOME_MANAGER_NOTE: &str =
    "If your config file comes from home-manager, set this through programs.zellij.settings.";

#[derive(Debug, Clone, Default)]
pub struct DesktopFacts {
    pub launcher_installable: bool,
    pub in_desktop_window: bool,
    pub platform: Platform,
    pub session_name: Option<String>,
}

pub fn line(text: &str) -> ComponentLine {
    ComponentLine::new(vec![ActiveComponent::new(TextOrCustomRender::Text(
        Text::from(text),
    ))])
}

pub fn styled(text: Text) -> ComponentLine {
    ComponentLine::new(vec![ActiveComponent::new(TextOrCustomRender::Text(text))])
}

fn key_line(key: &str, what: &str) -> ComponentLine {
    styled(Text::from(format!("{} {}", key, what)).color_substring(3, key))
}

pub fn mode_keys(base_mode: InputMode, mode_letter: char, key: &str) -> String {
    match base_mode {
        InputMode::Locked => format!("Ctrl g + {} + {}", mode_letter, key),
        _ => format!("Ctrl {} + {}", mode_letter, key),
    }
}

fn session_mode_keys(base_mode: InputMode, key: &str) -> String {
    mode_keys(base_mode, 'o', key)
}

fn pane_mode_keys(base_mode: InputMode, key: &str) -> String {
    mode_keys(base_mode, 'p', key)
}

fn esc_help() -> Text {
    Text::from("Help: <ESC> - Go back").color_range(1, 6..=10)
}

fn platform_shortcuts(platform: Platform) -> &'static str {
    match platform {
        Platform::Windows => {
            "Right-click its taskbar icon for Show Inbox and Show Sessions."
        },
        Platform::MacOs => "Its Dock menu will offer Show Inbox and Show Sessions.",
        _ => "Right-click its launcher for Show Inbox and Show Sessions.",
    }
}

fn new_window_keys(platform: Platform) -> &'static str {
    match platform {
        Platform::MacOs => "Cmd+N",
        _ => "Ctrl+Shift+N",
    }
}

impl Page {
    pub fn new_desktop_app(facts: Rc<RefCell<DesktopFacts>>) -> Page {
        let facts = facts.borrow().clone();
        let mut keys = vec![];
        if !facts.in_desktop_window {
            keys.push(key_line("<w>", "open this session in a window"));
        }
        if facts.launcher_installable {
            keys.push(key_line("<i>", "add Zellij to your app menu"));
        }
        keys.push(key_line("<s>", "Setup Zellij: launcher, default terminal and more"));
        let mut help = String::from("Help:");
        if !facts.in_desktop_window {
            help.push_str(" <w> - Open in a window,");
        }
        if facts.launcher_installable {
            help.push_str(" <i> - Add to app menu,");
        }
        help.push_str(" <s> - Setup Zellij, <ESC> - Go back");
        Page::new()
            .with_kind(PageKind::DesktopApp)
            .with_title(Text::from("Zellij as your terminal").color_range(0, ..))
            .with_paragraph(vec![
                line("Zellij can now run as its own app: a window with your tabs, panes and"),
                line("sessions, a tray icon, and desktop notifications. Closing a window keeps"),
                line("its session running."),
            ])
            .with_paragraph(vec![
                styled(
                    Text::from(format!(
                        "Start it with `zellij window`, from the app menu, or with {} in a window.",
                        new_window_keys(facts.platform)
                    ))
                    .color_substring(3, "zellij window")
                    .color_substring(3, new_window_keys(facts.platform)),
                ),
                line(platform_shortcuts(facts.platform)),
                line("It can also become your default terminal app; Setup Zellij offers this."),
            ])
            .with_paragraph(keys)
            .with_paragraph(vec![styled(Text::from(REMOTE_WARNING).color_range(1, ..))])
            .with_help(Box::new(move |_hovering_over_link, _menu_item_is_selected| {
                Text::from(help.clone())
                    .color_substring(1, "<w>")
                    .color_substring(1, "<i>")
                    .color_substring(1, "<s>")
                    .color_substring(1, "<ESC>")
            }))
    }

    pub fn new_inbox(
        keybinding_state: Rc<RefCell<KeybindingState>>,
        main_screen_builder: Rc<dyn Fn() -> Page>,
    ) -> Page {
        let base_mode = keybinding_state.borrow().base_mode();
        let mut page = Page::new()
            .with_kind(PageKind::Inbox)
            .with_title(Text::from("Inbox").color_range(0, ..))
            .with_paragraph(vec![
                line("Panes now show what they need: approval, a failure, or a finished"),
                line("command, as symbols on pane frames and tabs. The indicator in the tab"),
                line("bar (or compact bar) sums up every session."),
            ])
            .with_paragraph(vec![
                styled(
                    Text::from("<F9> opens the inbox (or click the indicator, or w in Session mode):")
                        .color_substring(3, "<F9>"),
                ),
                line("preview output, reply into a pane, go to it, or mark items seen."),
                styled(
                    Text::from(format!(
                        "{} jumps to the next item that needs you, in any session.",
                        session_mode_keys(base_mode, "n")
                    ))
                    .color_substring(3, &session_mode_keys(base_mode, "n")),
                ),
            ])
            .with_paragraph(vec![
                styled(
                    Text::from(format!(
                        "{} / {} / {} mute, ignore the output of, or watch a pane.",
                        pane_mode_keys(base_mode, "u"),
                        pane_mode_keys(base_mode, "g"),
                        pane_mode_keys(base_mode, "v")
                    ))
                    .color_substring(3, &pane_mode_keys(base_mode, "u"))
                    .color_substring(3, &pane_mode_keys(base_mode, "g"))
                    .color_substring(3, &pane_mode_keys(base_mode, "v")),
                ),
                styled(
                    Text::from(format!(
                        "{} returns to the previous session; {} / {} step through recent ones.",
                        session_mode_keys(base_mode, "Tab"),
                        session_mode_keys(base_mode, "Left"),
                        session_mode_keys(base_mode, "Right")
                    ))
                    .color_substring(3, &session_mode_keys(base_mode, "Tab"))
                    .color_substring(3, &session_mode_keys(base_mode, "Left"))
                    .color_substring(3, &session_mode_keys(base_mode, "Right")),
                ),
                styled(
                    Text::from("zellij run --notify -- <command> tells you when it finishes.")
                        .color_substring(3, "zellij run --notify"),
                ),
            ]);
        for feature in [Feature::Inbox, Feature::SessionHistory] {
            if let Some(note) =
                crate::pages::missing_binds_note(&keybinding_state, &main_screen_builder, feature)
            {
                page = page.with_paragraph(note);
                break;
            }
        }
        page.with_help(Box::new(|_hovering_over_link, _menu_item_is_selected| {
            esc_help()
        }))
    }

    pub fn new_window_welcome(missing_services: &[String], only_missing: bool) -> Page {
        let title = if only_missing {
            "Something is missing on this desktop"
        } else {
            "Welcome to the Zellij window"
        };
        let mut lines = vec![];
        for text in window_welcome_points(only_missing) {
            lines.extend(bulleted(&text, 76));
        }
        for text in missing_services {
            for wrapped in wrap(text, 76) {
                lines.push(line(&wrapped));
            }
        }
        Page::new()
            .main_screen()
            .with_title(Text::from(title).color_range(2, ..))
            .with_paragraph(lines)
            .with_help(Box::new(|_hovering_over_link, _menu_item_is_selected| {
                Text::from("Help: <ESC> - Dismiss").color_range(1, 6..=10)
            }))
    }

    pub fn new_repair(missing: &str, current: &str) -> Page {
        let mut lines = vec![];
        for wrapped in wrap(
            &format!(
                "Your Zellij launcher points to {}, which no longer exists. Point it at this copy ({})?",
                missing, current
            ),
            76,
        ) {
            lines.push(line(&wrapped));
        }
        Page::new()
            .main_screen()
            .with_title(Text::from("Your Zellij launcher is broken").color_range(2, ..))
            .with_paragraph(lines)
            .with_paragraph(vec![styled(
                Text::from("<y> yes · <n> no · <v> never ask again")
                    .color_substring(3, "<y>")
                    .color_substring(3, "<n>")
                    .color_substring(3, "<v>"),
            )])
            .with_help(Box::new(|_hovering_over_link, _menu_item_is_selected| {
                Text::from("Help: <y> - Fix it, <n> - Not now, <v> - Never, <ESC> - Not now")
                    .color_substring(1, "<y>")
                    .color_substring(1, "<n>")
                    .color_substring(1, "<v>")
                    .color_substring(1, "<ESC>")
            }))
    }

    pub fn new_message(title: &str, message_lines: &[String]) -> Page {
        let mut lines = vec![];
        for text in message_lines {
            for wrapped in wrap(text, 76) {
                lines.push(line(&wrapped));
            }
        }
        Page::new()
            .with_title(Text::from(title).color_range(2, ..))
            .with_paragraph(lines)
            .with_help(Box::new(|_hovering_over_link, _menu_item_is_selected| esc_help()))
    }

    pub fn tip_desktop_app() -> Page {
        tip_page(
            DESKTOP_TIP_INDEX,
            vec![
                styled(
                    Text::from("Zellij can run as its own app: zellij window")
                        .color_substring(3, "zellij window"),
                ),
                line("It has a tray icon and desktop notifications, and closing a window keeps"),
                line("its session running. Its launcher's right-click menu has Show Inbox and"),
                line("Show Sessions, and Zellij can become your default terminal (Setup Zellij)."),
                styled(Text::from(REMOTE_WARNING).dim_all()),
            ],
        )
    }

    pub fn tip_inbox_keys(base_mode: Rc<RefCell<InputMode>>) -> Page {
        let base_mode = *base_mode.borrow();
        tip_page(
            DESKTOP_TIP_INDEX + 1,
            vec![
                styled(
                    Text::from("Press <F9> to open the inbox: everything that needs you, in every")
                        .color_substring(3, "<F9>"),
                ),
                line("session. Preview a pane's output, reply to it, or go straight there."),
                styled(
                    Text::from(format!(
                        "{} jumps to the next item.",
                        session_mode_keys(base_mode, "n")
                    ))
                    .color_substring(3, &session_mode_keys(base_mode, "n")),
                ),
            ],
        )
    }

    pub fn tip_pane_controls(base_mode: Rc<RefCell<InputMode>>) -> Page {
        let base_mode = *base_mode.borrow();
        let mute = pane_mode_keys(base_mode, "u");
        let ignore = pane_mode_keys(base_mode, "g");
        let watch = pane_mode_keys(base_mode, "v");
        tip_page(
            DESKTOP_TIP_INDEX + 2,
            vec![
                line("Too many notifications from one pane?"),
                styled(
                    Text::from(format!("{} mutes it, {} ignores its output,", mute, ignore))
                        .color_substring(3, &mute)
                        .color_substring(3, &ignore),
                ),
                styled(
                    Text::from(format!(
                        "and {} watches it: you hear about every pause in its output.",
                        watch
                    ))
                    .color_substring(3, &watch),
                ),
            ],
        )
    }

    pub fn tip_run_notify() -> Page {
        tip_page(
            DESKTOP_TIP_INDEX + 3,
            vec![
                styled(
                    Text::from("zellij run --notify -- cargo build")
                        .color_substring(3, "zellij run --notify"),
                ),
                line("runs a command in a new pane and tells you when it finishes, even when you"),
                line("are looking at another tab or another session."),
            ],
        )
    }

    pub fn tip_session_history(base_mode: Rc<RefCell<InputMode>>) -> Page {
        let base_mode = *base_mode.borrow();
        let back = session_mode_keys(base_mode, "Tab");
        let left = session_mode_keys(base_mode, "Left");
        let right = session_mode_keys(base_mode, "Right");
        let next = session_mode_keys(base_mode, "n");
        tip_page(
            DESKTOP_TIP_INDEX + 4,
            vec![
                styled(
                    Text::from(format!("{} returns to the previous session.", back))
                        .color_substring(3, &back),
                ),
                styled(
                    Text::from(format!("{} / {} step through recent sessions.", left, right))
                        .color_substring(3, &left)
                        .color_substring(3, &right),
                ),
                styled(
                    Text::from(format!("{} jumps to the next one that needs you.", next))
                        .color_substring(3, &next),
                ),
            ],
        )
    }
}

fn tip_page(index: usize, lines: Vec<ComponentLine>) -> Page {
    Page::new()
        .main_screen()
        .with_title(Text::from(format!("Zellij Tip #{}", index + 1)).color_range(0, ..))
        .with_paragraph(lines)
        .with_help(Box::new(|_hovering_over_link, _menu_item_is_selected| {
            Text::from(
                "Help: <ESC> - Dismiss, <↓↑> - Browse tips, <Ctrl c> - Don't show tips on startup",
            )
            .color_range(1, 6..=10)
            .color_range(1, 23..=26)
            .color_range(1, 43..=50)
        }))
}

pub fn opt_out_lines() -> Vec<String> {
    opt_out_kdl().lines().map(str::to_owned).collect()
}

pub fn opt_out_done_lines() -> Vec<String> {
    vec![
        "Done. The status symbols on panes and tabs, the session indicator, the tray icon and"
            .to_owned(),
        "desktop notifications are off.".to_owned(),
        "Turn any of them back on in Settings → Attention.".to_owned(),
    ]
}

pub fn opt_out_failed_lines(path: Option<String>) -> Vec<String> {
    let mut lines = vec![format!(
        "{} could not be written. Add these lines to it yourself:",
        path.unwrap_or_else(|| "Your config file".to_owned())
    )];
    lines.extend(opt_out_lines().into_iter().map(|l| format!("    {}", l)));
    lines.push(HOME_MANAGER_NOTE.to_owned());
    lines
}

pub fn open_setup() {
    let configuration = BTreeMap::from([("rerun_setup".to_owned(), "true".to_owned())]);
    open_plugin_pane_floating(
        "zellij:configuration",
        configuration,
        None,
        BTreeMap::new(),
    );
}

pub fn kdl_for_binds(binds: &[(InputMode, KeyWithModifier, Vec<Action>)]) -> Vec<String> {
    let mut lines = vec!["keybinds {".to_owned()];
    for (mode, key, actions) in binds {
        let actions: Vec<String> = actions.iter().map(|a| format!("{};", a)).collect();
        lines.push(format!(
            "    {} {{ bind \"{}\" {{ {} }} }}",
            format!("{:?}", mode).to_lowercase(),
            key,
            actions.join(" ")
        ));
    }
    lines.push("}".to_owned());
    lines
}

fn emphasis_ranges(text: &str) -> Vec<(usize, usize, usize)> {
    WINDOW_WELCOME_EMPHASIS
        .iter()
        .filter_map(|(phrase, level)| {
            text.find(phrase).map(|byte_start| {
                let start = text[..byte_start].chars().count();
                (start, start + phrase.chars().count(), *level)
            })
        })
        .collect()
}

pub fn bulleted(text: &str, width: usize) -> Vec<ComponentLine> {
    let ranges = emphasis_ranges(text);
    let mut lines = vec![];
    let mut position = 0;
    for (index, wrapped) in wrap(text, width.saturating_sub(2)).into_iter().enumerate() {
        let length = wrapped.chars().count();
        let prefix = if index == 0 { "- " } else { "  " };
        let mut styled_line = Text::from(format!("{}{}", prefix, wrapped));
        for (start, end, level) in &ranges {
            let from = (*start).max(position);
            let to = (*end).min(position + length);
            if from < to {
                styled_line =
                    styled_line.color_range(*level, from - position + 2..to - position + 2);
            }
        }
        lines.push(styled(styled_line));
        position += length + 1;
    }
    lines
}

pub fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut lines = vec![];
    let mut current = String::new();
    for word in text.split_whitespace() {
        let candidate = if current.is_empty() {
            word.to_owned()
        } else {
            format!("{} {}", current, word)
        };
        if candidate.chars().count() > width && !current.is_empty() {
            lines.push(std::mem::take(&mut current));
            current = word.to_owned();
        } else {
            current = candidate;
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    lines
}

#[cfg(test)]
mod window_welcome_tests {
    use super::*;

    #[test]
    fn the_window_welcome_points_have_no_semicolons() {
        for point in window_welcome_points(false) {
            assert!(!point.contains(';'), "{}", point);
        }
    }

    #[test]
    fn every_emphasized_phrase_is_in_a_point() {
        let points = window_welcome_points(false).join(" ");
        for (phrase, _) in WINDOW_WELCOME_EMPHASIS {
            assert!(points.contains(phrase), "{}", phrase);
        }
    }

    #[test]
    fn a_wrapped_point_has_one_bullet_and_an_indented_continuation() {
        let text = window_welcome_points(false)[0].clone();
        let ranges = emphasis_ranges(&text);
        assert_eq!(ranges.len(), 1);
        let wrapped = wrap(&text, 30);
        assert!(wrapped.len() > 1);
        assert_eq!(bulleted(&text, 32).len(), wrapped.len());
    }
}
