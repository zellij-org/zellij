use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use zellij_tile::prelude::actions::Action;
use zellij_tile::prelude::desktop_integration::{
    window_welcome_points, REMOTE_WARNING, WINDOW_WELCOME_EMPHASIS,
};
use zellij_tile::prelude::*;

use crate::active_component::{ActiveComponent, ClickAction};
use crate::keybindings::Feature;
use crate::pages::{
    feature_page_help, link, missing_binds_note, ComponentLine, Page, PageKind, Shared,
    TextOrCustomRender,
};

pub const DESKTOP_TIP_INDEX: usize = 12;
pub const LAST_ONBOARDING_TIP_INDEX: usize = 16;
pub const INTEGRATIONS_URL: &str = "https://zellij.dev/documentation/inbox-integrations.html";
pub const HOME_MANAGER_NOTE: &str =
    "If your config file comes from home-manager, set this through programs.zellij.settings.";

#[derive(Debug, Clone, Default)]
pub struct DesktopFacts {
    pub launcher_installable: bool,
    pub in_desktop_terminal: bool,
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

fn command_line(command: &str) -> ComponentLine {
    styled(Text::from(format!("$ {}", command)).color_substring(3, command))
}

fn text_piece(text: &str) -> ActiveComponent {
    ActiveComponent::new(TextOrCustomRender::Text(Text::from(text)))
}

fn settings_link_line(prefix: &str, targets: &[(&'static str, &'static str)]) -> ComponentLine {
    let mut components = vec![text_piece(prefix)];
    for (index, (label, page)) in targets.iter().enumerate() {
        if index > 0 {
            components.push(text_piece(" · "));
        }
        let page = *page;
        components.push(link(
            format!("Settings → {}", label),
            ClickAction::new_run(move || open_settings(Some(page))),
        ));
    }
    ComponentLine::new(components)
}

fn try_it_out_line(keys: Option<String>, fallback: &str, action: ClickAction) -> ComponentLine {
    ComponentLine::new(vec![
        text_piece("Try it out: "),
        link(keys.unwrap_or_else(|| fallback.to_owned()), action),
    ])
}

fn terminal_page_action(shared: &Shared) -> ClickAction {
    let shared = shared.clone();
    ClickAction::new_change_page(move || Page::new_desktop_app(&shared))
}

fn open_inbox_action(shared: &Shared) -> ClickAction {
    let keybinding_state = shared.keybinding_state.clone();
    ClickAction::new_run(move || keybinding_state.borrow().open_inbox())
}

fn open_session_manager_action(shared: &Shared) -> ClickAction {
    let keybinding_state = shared.keybinding_state.clone();
    ClickAction::new_run(move || keybinding_state.borrow().open_session_manager())
}

impl Page {
    pub fn new_desktop_app(shared: &Shared) -> Page {
        let facts = shared.facts.borrow().clone();
        let mut keys = vec![];
        if !facts.in_desktop_terminal {
            keys.push(key_line("<w>", "open this session in a window"));
        }
        if facts.launcher_installable {
            keys.push(key_line("<i>", "add Zellij to your app menu"));
        }
        keys.push(key_line(
            "<s>",
            "Setup Zellij: tray icon, default terminal and more",
        ));
        let mut help = String::from("Help:");
        if !facts.in_desktop_terminal {
            help.push_str(" <w> - Open in a window,");
        }
        if facts.launcher_installable {
            help.push_str(" <i> - Add to app menu,");
        }
        help.push_str(" <s> - Setup Zellij, <ESC> - Go back");
        Page::new()
            .with_kind(PageKind::DesktopApp)
            .with_title(Text::from("Zellij Can Now be Your Terminal").color_range(0, ..))
            .with_paragraph(vec![
                line("Zellij can now also be your terminal, giving you a native multiplexer"),
                line("experience inside a desktop application. Zellij can still run inside"),
                line("another terminal, and sessions can be shared between the two and remotely."),
            ])
            .with_paragraph(vec![line("Try it out:"), command_line("zellij terminal")])
            .with_paragraph(vec![
                line("Install it on your desktop and tray:"),
                command_line("zellij setup --install-desktop-entry"),
            ])
            .with_paragraph(keys)
            .with_paragraph(vec![settings_link_line(
                "Configure it: ",
                &[("Zellij terminal", "zellij_terminal")],
            )])
            .with_paragraph(vec![styled(Text::from(REMOTE_WARNING).color_range(1, ..))])
            .with_help(Box::new(
                move |_hovering_over_link, _menu_item_is_selected| {
                    Text::from(help.clone())
                        .color_substring(1, "<w>")
                        .color_substring(1, "<i>")
                        .color_substring(1, "<s>")
                        .color_substring(1, "<ESC>")
                },
            ))
    }

    pub fn new_inbox(shared: &Shared) -> Page {
        let terminal = link("terminal", terminal_page_action(shared));
        let inbox_keys = shared.keybinding_state.borrow().inbox_keys();
        let mut page = Page::new()
            .with_kind(PageKind::Inbox)
            .with_title(Text::from("Inbox").color_range(0, ..))
            .with_paragraph(vec![
                line("Zellij now includes a built-in attention system managed inside a new Inbox."),
                line("Programs and shells can request your attention, and you can follow running"),
                line("programs to find out when they are done or have errored."),
            ])
            .with_paragraph(vec![
                ComponentLine::new(vec![
                    text_piece("The inbox integrates natively with the desktop "),
                    terminal,
                    text_piece(" and system tray."),
                ]),
                ComponentLine::new(vec![
                    text_piece("Many programs will work out of the box. For others, see "),
                    link(
                        "our integrations",
                        ClickAction::new_open_link(
                            INTEGRATIONS_URL.to_owned(),
                            shared.link_executable.clone(),
                        ),
                    ),
                    text_piece("."),
                ]),
            ])
            .with_paragraph(vec![try_it_out_line(
                inbox_keys,
                "open the inbox",
                open_inbox_action(shared),
            )]);
        for feature in [Feature::Inbox, Feature::SessionHistory] {
            if let Some(note) = missing_binds_note(shared, feature) {
                page = page.with_paragraph(note);
                break;
            }
        }
        page.with_paragraph(vec![settings_link_line(
            "Don't want this? ",
            &[("Attention", "attention")],
        )])
        .with_help(Box::new(feature_page_help))
    }

    pub fn new_session_previews(shared: &Shared) -> Page {
        let session_manager_keys = shared.keybinding_state.borrow().session_manager_keys();
        Page::new()
            .with_kind(PageKind::SessionPreviews)
            .with_title(Text::from("Session Previews").color_range(0, ..))
            .with_paragraph(vec![
                ComponentLine::new(vec![
                    text_piece("The "),
                    link("session manager", open_session_manager_action(shared)),
                    text_piece(" now includes session previews, allowing you to"),
                ]),
                line("both view all sessions running on the machine and type into them for quick"),
                line("commands without switching. This also integrates with the"),
                ComponentLine::new(vec![
                    link("desktop client and tray icon", terminal_page_action(shared)),
                    text_piece(", allowing you to browse sessions at a glance"),
                ]),
                line("without opening a terminal."),
            ])
            .with_paragraph(vec![try_it_out_line(
                session_manager_keys,
                "open the session manager",
                open_session_manager_action(shared),
            )])
            .with_help(Box::new(feature_page_help))
    }

    pub fn new_mouse_control(_shared: &Shared) -> Page {
        Page::new()
            .with_title(
                Text::from("Right-click Menu and Improved Mouse Control").color_range(0, ..),
            )
            .with_paragraph(vec![
                line("Zellij now includes a configurable right-click menu, allowing quick actions"),
                line(
                    "with a mouse click. The menu is contextual on terminal panes, tabs and stack",
                ),
                line("titles."),
            ])
            .with_paragraph(vec![styled(
                Text::from(
                    "It's now also possible to create a floating pane by Ctrl+dragging the mouse.",
                )
                .color_substring(3, "Ctrl+dragging"),
            )])
            .with_paragraph(vec![settings_link_line(
                "Don't want this? ",
                &[
                    ("Right-click menu", "right_click_menu"),
                    ("Mouse bindings", "mouse_bindings"),
                ],
            )])
            .with_help(Box::new(feature_page_help))
    }

    pub fn new_settings_screen(_shared: &Shared) -> Page {
        Page::new()
            .with_title(Text::from("Settings Screen").color_range(0, ..))
            .with_paragraph(vec![
                line("Zellij now includes a settings screen to change its behavior and appearance"),
                line("in a straightforward way, without hunting through configuration files."),
                line("Configuration files still work as before, with the settings screen acting"),
                line("as their user interface."),
            ])
            .with_paragraph(vec![ComponentLine::new(vec![
                text_piece("Open it: "),
                link("Settings", ClickAction::new_run(|| open_settings(None))),
            ])])
            .with_help(Box::new(feature_page_help))
    }

    pub fn new_window_welcome(missing_services: &[String], only_missing: bool) -> Page {
        let title = if only_missing {
            "Something is missing on this desktop"
        } else {
            "Welcome to the Zellij terminal"
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
            .with_help(Box::new(|_hovering_over_link, _menu_item_is_selected| {
                esc_help()
            }))
    }

    pub fn tip_desktop_app() -> Page {
        tip_page(
            DESKTOP_TIP_INDEX,
            vec![
                styled(
                    Text::from("Zellij can run as its own app: zellij terminal")
                        .color_substring(3, "zellij terminal"),
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
                    Text::from(format!(
                        "Press {} to open the inbox: everything that needs you, in every",
                        session_mode_keys(base_mode, "i")
                    ))
                    .color_substring(3, &session_mode_keys(base_mode, "i")),
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
                    Text::from(format!(
                        "{} / {} step through recent sessions.",
                        left, right
                    ))
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

pub fn open_setup() {
    let configuration = BTreeMap::from([("rerun_setup".to_owned(), "true".to_owned())]);
    open_plugin_pane_floating("zellij:configuration", configuration, None, BTreeMap::new());
}

pub fn open_settings(page: Option<&str>) {
    let mut configuration = BTreeMap::new();
    if let Some(page) = page {
        configuration.insert("page".to_owned(), page.to_owned());
    }
    open_plugin_pane_floating("zellij:configuration", configuration, None, BTreeMap::new());
}

pub fn kdl_for_binds(binds: &[(InputMode, KeyWithModifier, Vec<Action>)]) -> Vec<String> {
    let mut lines = vec!["keybinds {".to_owned()];
    for (mode, key, actions) in binds {
        let actions: Vec<String> = actions
            .iter()
            .map(|action| {
                let written = action
                    .to_kdl()
                    .map(|node| node.to_string().trim().to_owned())
                    .unwrap_or_else(|| action.to_string());
                format!("{};", written)
            })
            .collect();
        lines.push(format!(
            "    {} {{ bind \"{}\" {{ {} }} }}",
            format!("{:?}", mode).to_lowercase(),
            key.to_kdl(),
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
