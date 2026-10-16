use std::sync::Arc;

use crate::app::model::TrayState;

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
    ClearAll,
    PauseHour,
    PauseUntilTomorrow,
    Resume,
    HideUntilRestart,
    HidePermanently,
    QuitAll,
    Back,
    QuitApp,
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

pub fn menu(paused: bool) -> Vec<MenuEntry> {
    let item = |id, label, action, enabled| MenuEntry::Item {
        id,
        label,
        action,
        enabled,
    };
    vec![
        item(1, "Show inbox", MenuAction::ShowInbox, true),
        item(2, "Show sessions", MenuAction::ShowSessions, true),
        MenuEntry::Separator { id: 3 },
        item(4, "New window", MenuAction::NewWindow, true),
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
        let labels: Vec<&str> = menu(false)
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
                "Clear all notifications",
                "Pause notifications",
                "Hide icon",
                "─",
                "Quit Zellij and stop all sessions…",
            ]
        );
        assert_eq!(find_action(&menu(false), 53), None);
        assert_eq!(find_action(&menu(true), 53), Some(MenuAction::Resume));
        assert_eq!(
            find_action(&menu(false), 62),
            Some(MenuAction::HidePermanently)
        );
        assert_eq!(find_action(&menu(false), 3), None);
        assert_eq!(find_action(&menu(false), 99), None);
        assert_eq!(find_action(&menu(false), 9), Some(MenuAction::ClearAll));
    }
}
