use std::sync::Arc;

use crate::app::icon::BADGE_PALETTE;
use crate::app::model::{MenuState, TrayState};
use zellij_utils::attention::AttentionStatus;
use zellij_utils::input::attention::{BADGE_STATUSES, POPUP_STATUSES};

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(windows)]
mod windows;

#[cfg(target_os = "macos")]
pub use macos::{activate, install_main_menu, set_accessory, MainMenu};
#[cfg(windows)]
pub use windows::work_area;

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MenuAction {
    ShowInbox,
    ShowSessions,
    NewWindow,
    DetachAllWindows,
    DetachEverywhere,
    ClearAll,
    PauseHour,
    PauseUntilTomorrow,
    Resume,
    HideUntilRestart,
    HidePermanently,
    QuitAll,
    Back,
    QuitApp,
    TogglePopup(AttentionStatus),
    ToggleBadge(AttentionStatus),
    BadgeColor(AttentionStatus, Option<[u8; 3]>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrayEvent {
    Activate {
        at: Option<(i32, i32)>,
        token: Option<String>,
    },
    Menu(MenuAction),
}

pub type TrayEvents = Arc<dyn Fn(TrayEvent) + Send + Sync>;

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Anchor {
    Below { x: f64, y: f64 },
    Above { x: f64, y: f64 },
    At { x: f64, y: f64 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MenuEntry {
    Item {
        id: i32,
        label: &'static str,
        action: MenuAction,
        enabled: bool,
        checked: Option<bool>,
    },
    Submenu {
        id: i32,
        label: &'static str,
        items: Vec<MenuEntry>,
    },
    Separator {
        id: i32,
    },
}

pub fn status_label(status: AttentionStatus) -> &'static str {
    match status {
        AttentionStatus::NeedsYou => "Needs you",
        AttentionStatus::Failed => "Failed",
        AttentionStatus::Done => "Done",
        AttentionStatus::Working => "Working",
        AttentionStatus::Quiet => "Gone quiet",
        AttentionStatus::NewOutput => "New output",
        AttentionStatus::Idle => "Idle",
    }
}

fn check(id: i32, label: &'static str, action: MenuAction, checked: bool) -> MenuEntry {
    MenuEntry::Item {
        id,
        label,
        action,
        enabled: true,
        checked: Some(checked),
    }
}

fn notifications_menu(state: &MenuState) -> MenuEntry {
    let popups = POPUP_STATUSES
        .iter()
        .enumerate()
        .map(|(index, status)| {
            check(
                210 + index as i32,
                status_label(*status),
                MenuAction::TogglePopup(*status),
                state.popups.includes(*status),
            )
        })
        .collect();
    let badge = BADGE_STATUSES
        .iter()
        .enumerate()
        .map(|(index, status)| {
            check(
                220 + index as i32,
                status_label(*status),
                MenuAction::ToggleBadge(*status),
                state.badge.statuses.includes(*status),
            )
        })
        .collect();
    let colours = BADGE_STATUSES
        .iter()
        .enumerate()
        .map(|(index, status)| {
            let chosen = state.badge.colors.get(*status);
            let base = 2300 + index as i32 * 10;
            let mut items = vec![check(
                base,
                "Default",
                MenuAction::BadgeColor(*status, None),
                chosen.is_none(),
            )];
            items.extend(
                BADGE_PALETTE
                    .iter()
                    .enumerate()
                    .map(|(offset, (name, colour))| {
                        check(
                            base + 1 + offset as i32,
                            name,
                            MenuAction::BadgeColor(*status, Some(*colour)),
                            chosen == Some(*colour),
                        )
                    }),
            );
            MenuEntry::Submenu {
                id: 230 + index as i32,
                label: status_label(*status),
                items,
            }
        })
        .collect();
    MenuEntry::Submenu {
        id: 20,
        label: "Notifications",
        items: vec![
            MenuEntry::Submenu {
                id: 21,
                label: "Pop-ups",
                items: popups,
            },
            MenuEntry::Submenu {
                id: 22,
                label: "Icon badge",
                items: badge,
            },
            MenuEntry::Submenu {
                id: 23,
                label: "Badge colours",
                items: colours,
            },
        ],
    }
}

pub fn menu(state: &MenuState) -> Vec<MenuEntry> {
    let paused = state.paused;
    let item = |id, label, action, enabled| MenuEntry::Item {
        id,
        label,
        action,
        enabled,
        checked: None,
    };
    vec![
        item(1, "Show inbox", MenuAction::ShowInbox, true),
        item(2, "Show sessions", MenuAction::ShowSessions, true),
        MenuEntry::Separator { id: 3 },
        item(4, "New window", MenuAction::NewWindow, true),
        MenuEntry::Submenu {
            id: 10,
            label: "Detach all",
            items: vec![
                item(111, "Close all windows", MenuAction::DetachAllWindows, true),
                item(
                    112,
                    "Detach every client from every session",
                    MenuAction::DetachEverywhere,
                    true,
                ),
            ],
        },
        item(9, "Clear all notifications", MenuAction::ClearAll, true),
        MenuEntry::Submenu {
            id: 5,
            label: "Pause notifications",
            items: vec![
                item(51, "1 hour", MenuAction::PauseHour, true),
                item(52, "Until tomorrow", MenuAction::PauseUntilTomorrow, true),
                item(53, "Resume", MenuAction::Resume, paused),
            ],
        },
        notifications_menu(state),
        MenuEntry::Submenu {
            id: 6,
            label: "Hide icon",
            items: vec![
                item(61, "Until next launch", MenuAction::HideUntilRestart, true),
                item(62, "Permanently", MenuAction::HidePermanently, true),
            ],
        },
        MenuEntry::Separator { id: 7 },
        item(
            8,
            "Quit Zellij and stop all sessions…",
            MenuAction::QuitAll,
            true,
        ),
    ]
}

pub fn find_action(entries: &[MenuEntry], wanted: i32) -> Option<MenuAction> {
    for entry in entries {
        match entry {
            MenuEntry::Item {
                id,
                action,
                enabled,
                ..
            } if *id == wanted && *enabled => return Some(*action),
            MenuEntry::Submenu { items, .. } => {
                if let Some(action) = find_action(items, wanted) {
                    return Some(action);
                }
            },
            _ => {},
        }
    }
    None
}

pub trait Tray {
    fn update(&mut self, state: &TrayState);
    fn set_visible(&mut self, visible: bool);
    fn anchor(&self) -> Option<Anchor>;
    fn refresh_appearance(&mut self) {}
}

#[cfg(target_os = "linux")]
pub fn open(events: TrayEvents, state: &TrayState) -> Option<Box<dyn Tray>> {
    Some(Box::new(linux::StatusNotifier::open(events, state)))
}

#[cfg(target_os = "macos")]
pub fn open(events: TrayEvents, state: &TrayState) -> Option<Box<dyn Tray>> {
    match macos::StatusItem::open(events, state) {
        Ok(tray) => Some(Box::new(tray)),
        Err(e) => {
            report!("the menu-bar icon could not be created: {:#}", e);
            None
        },
    }
}

#[cfg(windows)]
pub fn open(events: TrayEvents, state: &TrayState) -> Option<Box<dyn Tray>> {
    match windows::NotifyIcon::open(events, state) {
        Ok(tray) => Some(Box::new(tray)),
        Err(e) => {
            report!("the notification-area icon could not be created: {:#}", e);
            None
        },
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
pub fn open(_events: TrayEvents, _state: &TrayState) -> Option<Box<dyn Tray>> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_menu_reads_like_the_plan_and_resume_needs_a_pause() {
        let calm = MenuState::default();
        let paused = MenuState {
            paused: true,
            ..MenuState::default()
        };
        let labels: Vec<&str> = menu(&calm)
            .iter()
            .map(|entry| match entry {
                MenuEntry::Item { label, .. } | MenuEntry::Submenu { label, .. } => *label,
                MenuEntry::Separator { .. } => "─",
            })
            .collect();
        assert_eq!(
            labels,
            vec![
                "Show inbox",
                "Show sessions",
                "─",
                "New window",
                "Detach all",
                "Clear all notifications",
                "Pause notifications",
                "Notifications",
                "Hide icon",
                "─",
                "Quit Zellij and stop all sessions…",
            ]
        );
        assert_eq!(find_action(&menu(&calm), 53), None);
        assert_eq!(find_action(&menu(&paused), 53), Some(MenuAction::Resume));
        assert_eq!(
            find_action(&menu(&calm), 62),
            Some(MenuAction::HidePermanently)
        );
        assert_eq!(find_action(&menu(&calm), 3), None);
        assert_eq!(find_action(&menu(&calm), 99), None);
        assert_eq!(find_action(&menu(&calm), 9), Some(MenuAction::ClearAll));
        assert_eq!(
            find_action(&menu(&calm), 111),
            Some(MenuAction::DetachAllWindows)
        );
        assert_eq!(
            find_action(&menu(&calm), 112),
            Some(MenuAction::DetachEverywhere)
        );
    }

    fn checked(entries: &[MenuEntry], wanted: i32) -> Option<bool> {
        for entry in entries {
            match entry {
                MenuEntry::Item { id, checked, .. } if *id == wanted => return *checked,
                MenuEntry::Submenu { items, .. } => {
                    if let Some(found) = checked(items, wanted) {
                        return Some(found);
                    }
                },
                _ => {},
            }
        }
        None
    }

    #[test]
    fn the_notifications_menu_ticks_the_chosen_types_and_colours() {
        let mut state = MenuState::default();
        state.popups = state.popups.with(AttentionStatus::Done, false);
        state
            .badge
            .colors
            .set(AttentionStatus::Failed, Some(BADGE_PALETTE[4].1));
        let entries = menu(&state);
        assert_eq!(checked(&entries, 210), Some(true));
        assert_eq!(checked(&entries, 212), Some(false), "done pop-ups are off");
        assert_eq!(
            find_action(&entries, 212),
            Some(MenuAction::TogglePopup(AttentionStatus::Done))
        );
        assert_eq!(checked(&entries, 223), Some(true), "working shows a badge");
        assert_eq!(checked(&entries, 224), Some(false), "quiet does not");
        assert_eq!(
            find_action(&entries, 225),
            Some(MenuAction::ToggleBadge(AttentionStatus::NewOutput))
        );
        assert_eq!(checked(&entries, 2310), Some(false));
        assert_eq!(checked(&entries, 2315), Some(true), "failed is purple");
        assert_eq!(checked(&entries, 2300), Some(true), "needs you is default");
        assert_eq!(
            find_action(&entries, 2300),
            Some(MenuAction::BadgeColor(AttentionStatus::NeedsYou, None))
        );
        assert_eq!(
            find_action(&entries, 2351),
            Some(MenuAction::BadgeColor(
                AttentionStatus::NewOutput,
                Some(BADGE_PALETTE[0].1)
            ))
        );
        assert_eq!(checked(&entries, 1), None, "plain items are not ticks");
    }
}
