use std::cell::{Cell, RefCell};
use std::ffi::CStr;

use anyhow::{anyhow, Result};
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObject, NSObjectProtocol, Sel};
use objc2::{
    define_class, msg_send, sel, AllocAnyThread, DefinedClass, MainThreadMarker, MainThreadOnly,
};
use objc2_app_kit::{
    NSAppearanceCustomization, NSAppearanceNameAqua, NSAppearanceNameDarkAqua, NSApplication,
    NSEventMask, NSEventModifierFlags, NSEventType, NSImage, NSMenu, NSMenuItem, NSScreen,
    NSStatusBar, NSStatusItem, NSVariableStatusItemLength, NSWindow,
};
use objc2_foundation::{NSArray, NSData, NSSize, NSString};

use super::{find_action, menu, Anchor, MenuAction, MenuEntry, Tray, TrayEvent, TrayEvents};
use crate::app::icon::{render, DARK_INK, LIGHT_INK};
use crate::app::model::{TrayLook, TrayState};

const POINTS: f64 = 18.0;
const PIXELS: u32 = 36;

pub struct HandlerIvars {
    events: TrayEvents,
    item: RefCell<Option<Retained<NSStatusItem>>>,
    paused: Cell<bool>,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "ZellijStatusItemHandler"]
    #[ivars = HandlerIvars]
    struct Handler;

    unsafe impl NSObjectProtocol for Handler {}

    impl Handler {
        #[unsafe(method(clicked:))]
        fn clicked(&self, _sender: Option<&AnyObject>) {
            let mtm = self.mtm();
            let right = NSApplication::sharedApplication(mtm)
                .currentEvent()
                .map(|event| event.r#type() == NSEventType::RightMouseUp)
                .unwrap_or(false);
            if right {
                self.show_menu(mtm);
            } else {
                (self.ivars().events)(TrayEvent::Activate {
                    at: None,
                    token: None,
                });
            }
        }

        #[unsafe(method(menuItem:))]
        fn menu_item(&self, sender: Option<&AnyObject>) {
            let Some(sender) = sender else {
                return;
            };
            let tag: isize = unsafe { msg_send![sender, tag] };
            let action = main_menu_action(tag as i32)
                .or_else(|| find_action(&menu(self.ivars().paused.get()), tag as i32));
            if let Some(action) = action {
                (self.ivars().events)(TrayEvent::Menu(action));
            }
        }
    }
);

impl Handler {
    fn new(mtm: MainThreadMarker, events: TrayEvents) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(HandlerIvars {
            events,
            item: RefCell::new(None),
            paused: Cell::new(false),
        });
        unsafe { msg_send![super(this), init] }
    }

    fn show_menu(&self, mtm: MainThreadMarker) {
        let Some(item) = self.ivars().item.borrow().clone() else {
            return;
        };
        let menu = build_menu(mtm, &menu(self.ivars().paused.get()), self);
        item.setMenu(Some(&menu));
        if let Some(button) = item.button(mtm) {
            unsafe { button.performClick(None) };
        }
        item.setMenu(None);
    }
}

fn build_menu(mtm: MainThreadMarker, entries: &[MenuEntry], handler: &Handler) -> Retained<NSMenu> {
    let menu = NSMenu::new(mtm);
    menu.setAutoenablesItems(false);
    for entry in entries {
        match entry {
            MenuEntry::Separator { .. } => menu.addItem(&NSMenuItem::separatorItem(mtm)),
            MenuEntry::Item {
                id, label, enabled, ..
            } => {
                let item = unsafe {
                    NSMenuItem::initWithTitle_action_keyEquivalent(
                        NSMenuItem::alloc(mtm),
                        &NSString::from_str(label),
                        Some(sel!(menuItem:)),
                        &NSString::from_str(""),
                    )
                };
                unsafe { item.setTarget(Some(handler)) };
                item.setTag(*id as isize);
                item.setEnabled(*enabled);
                menu.addItem(&item);
            },
            MenuEntry::Submenu { label, items, .. } => {
                let item = unsafe {
                    NSMenuItem::initWithTitle_action_keyEquivalent(
                        NSMenuItem::alloc(mtm),
                        &NSString::from_str(label),
                        None::<Sel>,
                        &NSString::from_str(""),
                    )
                };
                let submenu = build_menu(mtm, items, handler);
                item.setSubmenu(Some(&submenu));
                menu.addItem(&item);
            },
        }
    }
    menu
}

pub struct StatusItem {
    mtm: MainThreadMarker,
    item: Retained<NSStatusItem>,
    handler: Retained<Handler>,
    look: Option<(TrayLook, bool)>,
    state: TrayState,
}

impl StatusItem {
    pub fn open(events: TrayEvents, state: &TrayState) -> Result<StatusItem> {
        let mtm = MainThreadMarker::new().ok_or_else(|| anyhow!("not on the main thread"))?;
        let item = NSStatusBar::systemStatusBar().statusItemWithLength(NSVariableStatusItemLength);
        let handler = Handler::new(mtm, events);
        *handler.ivars().item.borrow_mut() = Some(item.clone());
        let button = item
            .button(mtm)
            .ok_or_else(|| anyhow!("the status item has no button"))?;
        unsafe {
            button.setTarget(Some(&handler));
            button.setAction(Some(sel!(clicked:)));
        }
        button.sendActionOn(NSEventMask::LeftMouseUp | NSEventMask::RightMouseUp);
        let mut tray = StatusItem {
            mtm,
            item,
            handler,
            look: None,
            state: state.clone(),
        };
        tray.update(state);
        Ok(tray)
    }

    fn dark(&self) -> bool {
        let Some(button) = self.item.button(self.mtm) else {
            return false;
        };
        let names =
            unsafe { NSArray::from_slice(&[NSAppearanceNameAqua, NSAppearanceNameDarkAqua]) };
        button
            .effectiveAppearance()
            .bestMatchFromAppearancesWithNames(&names)
            .map(|name| name.to_string().contains("Dark"))
            .unwrap_or(false)
    }

    fn draw(&mut self, force: bool) {
        let dark = self.dark();
        let key = (self.state.look.clone(), dark);
        if !force && self.look.as_ref() == Some(&key) {
            return;
        }
        let template = self.state.look.pill.is_none();
        let ink = if template || !dark {
            DARK_INK
        } else {
            LIGHT_INK
        };
        let image = render(PIXELS, &self.state.look, ink);
        let encoded = crate::image_io::encode(&crate::image_io::Image {
            width: image.width,
            height: image.height,
            pixels: image.pixels,
        });
        let Ok(encoded) = encoded else {
            return;
        };
        let data = NSData::with_bytes(&encoded);
        let Some(picture) = NSImage::initWithData(NSImage::alloc(), &data) else {
            return;
        };
        picture.setSize(NSSize::new(POINTS, POINTS));
        picture.setTemplate(template);
        if let Some(button) = self.item.button(self.mtm) {
            button.setImage(Some(&picture));
            button.setToolTip(Some(&NSString::from_str(&self.state.tooltip)));
        }
        self.look = Some(key);
    }
}

impl Tray for StatusItem {
    fn update(&mut self, state: &TrayState) {
        self.state = state.clone();
        self.handler.ivars().paused.set(state.paused);
        self.draw(false);
        if let Some(button) = self.item.button(self.mtm) {
            button.setToolTip(Some(&NSString::from_str(&state.tooltip)));
        }
    }

    fn set_visible(&mut self, visible: bool) {
        self.item.setVisible(visible);
    }

    fn anchor(&self) -> Option<Anchor> {
        let frame = self.item.button(self.mtm)?.window()?.frame();
        let screens = NSScreen::screens(self.mtm);
        let primary = screens.firstObject()?.frame();
        let top = primary.size.height - frame.origin.y;
        Some(Anchor::Below {
            x: frame.origin.x,
            y: top,
        })
    }

    fn refresh_appearance(&mut self) {
        self.draw(false);
    }
}

impl Drop for StatusItem {
    fn drop(&mut self) {
        NSStatusBar::systemStatusBar().removeStatusItem(&self.item);
    }
}

pub fn set_accessory(accessory: bool) {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let policy = if accessory {
        objc2_app_kit::NSApplicationActivationPolicy::Accessory
    } else {
        objc2_app_kit::NSApplicationActivationPolicy::Regular
    };
    NSApplication::sharedApplication(mtm).setActivationPolicy(policy);
}

pub fn activate() {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    #[allow(deprecated)]
    NSApplication::sharedApplication(mtm).activateIgnoringOtherApps(true);
}

fn main_menu_action(tag: i32) -> Option<MenuAction> {
    match tag {
        101 => Some(MenuAction::NewWindow),
        102 => Some(MenuAction::Back),
        103 => Some(MenuAction::QuitApp),
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum MainAction {
    Ours(isize),
    // Sent to no target, so AppKit hands it along the responder chain as for any app.
    Standard(&'static CStr),
}

#[derive(Clone, Copy)]
struct MainItem {
    title: &'static str,
    key: &'static str,
    with_option: bool,
    action: MainAction,
}

const fn ours(title: &'static str, tag: isize, key: &'static str) -> Option<MainItem> {
    Some(MainItem {
        title,
        key,
        with_option: false,
        action: MainAction::Ours(tag),
    })
}

const fn standard(
    title: &'static str,
    selector: &'static CStr,
    key: &'static str,
) -> Option<MainItem> {
    Some(MainItem {
        title,
        key,
        with_option: false,
        action: MainAction::Standard(selector),
    })
}

// The menu macOS lists the open windows in.
const WINDOW_MENU: &str = "Window";

const MAIN_MENU: [(&str, &[Option<MainItem>]); 3] = [
    (
        "Zellij",
        &[
            standard("About Zellij", c"orderFrontStandardAboutPanel:", ""),
            None,
            standard("Hide Zellij", c"hide:", "h"),
            Some(MainItem {
                title: "Hide Others",
                key: "h",
                with_option: true,
                action: MainAction::Standard(c"hideOtherApplications:"),
            }),
            standard("Show All", c"unhideAllApplications:", ""),
            None,
            ours("Quit Zellij", 103, "q"),
        ],
    ),
    (
        "File",
        &[
            ours("New Window", 101, "n"),
            standard("Close Window", c"performClose:", "w"),
        ],
    ),
    (
        WINDOW_MENU,
        &[
            standard("Minimize", c"performMiniaturize:", "m"),
            standard("Zoom", c"performZoom:", ""),
            None,
            ours("Back", 102, ""),
            None,
            standard("Bring All to Front", c"arrangeInFront:", ""),
        ],
    ),
];

fn main_menu_item(
    mtm: MainThreadMarker,
    handler: &Handler,
    item: &MainItem,
) -> Retained<NSMenuItem> {
    let selector = match item.action {
        MainAction::Ours(_) => sel!(menuItem:),
        MainAction::Standard(name) => Sel::register(name),
    };
    let menu_item = unsafe {
        NSMenuItem::initWithTitle_action_keyEquivalent(
            NSMenuItem::alloc(mtm),
            &NSString::from_str(item.title),
            Some(selector),
            &NSString::from_str(item.key),
        )
    };
    if item.with_option {
        menu_item.setKeyEquivalentModifierMask(
            NSEventModifierFlags::Command | NSEventModifierFlags::Option,
        );
    }
    if let MainAction::Ours(tag) = item.action {
        unsafe { menu_item.setTarget(Some(handler)) };
        menu_item.setTag(tag);
    }
    menu_item
}

fn titled(
    mtm: MainThreadMarker,
    handler: &Handler,
    title: &str,
    items: &[Option<MainItem>],
) -> (Retained<NSMenuItem>, Retained<NSMenu>) {
    let menu = NSMenu::initWithTitle(NSMenu::alloc(mtm), &NSString::from_str(title));
    for item in items {
        match item {
            Some(item) => menu.addItem(&main_menu_item(mtm, handler, item)),
            None => menu.addItem(&NSMenuItem::separatorItem(mtm)),
        }
    }
    let holder = NSMenuItem::new(mtm);
    holder.setSubmenu(Some(&menu));
    (holder, menu)
}

pub struct MainMenu {
    _handler: Retained<Handler>,
}

pub fn install_main_menu(events: TrayEvents) -> Option<MainMenu> {
    let mtm = MainThreadMarker::new()?;
    let handler = Handler::new(mtm, events);
    let bar = NSMenu::new(mtm);
    let app = NSApplication::sharedApplication(mtm);
    for (title, items) in MAIN_MENU {
        let (holder, menu) = titled(mtm, &handler, title, items);
        bar.addItem(&holder);
        if title == WINDOW_MENU {
            app.setWindowsMenu(Some(&menu));
        }
    }
    // With a Window menu macOS offers to merge windows into tabs of one window, which
    // each zellij window, drawn and sized on its own, is not made for.
    NSWindow::setAllowsAutomaticWindowTabbing(false, mtm);
    app.setMainMenu(Some(&bar));
    Some(MainMenu { _handler: handler })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_main_menu_has_the_standard_macos_items() {
        let layout: Vec<(&str, Vec<Option<(&str, &str, bool)>>)> = MAIN_MENU
            .iter()
            .map(|(menu, items)| {
                (
                    *menu,
                    items
                        .iter()
                        .map(|item| item.map(|item| (item.title, item.key, item.with_option)))
                        .collect(),
                )
            })
            .collect();
        assert_eq!(
            layout,
            vec![
                (
                    "Zellij",
                    vec![
                        Some(("About Zellij", "", false)),
                        None,
                        Some(("Hide Zellij", "h", false)),
                        Some(("Hide Others", "h", true)),
                        Some(("Show All", "", false)),
                        None,
                        Some(("Quit Zellij", "q", false)),
                    ]
                ),
                (
                    "File",
                    vec![
                        Some(("New Window", "n", false)),
                        Some(("Close Window", "w", false)),
                    ]
                ),
                (
                    "Window",
                    vec![
                        Some(("Minimize", "m", false)),
                        Some(("Zoom", "", false)),
                        None,
                        Some(("Back", "", false)),
                        None,
                        Some(("Bring All to Front", "", false)),
                    ]
                ),
            ]
        );
        let action = |title: &str| {
            MAIN_MENU
                .iter()
                .flat_map(|(_, items)| items.iter().flatten())
                .find(|item| item.title == title)
                .map(|item| item.action)
        };
        assert_eq!(action("Quit Zellij"), Some(MainAction::Ours(103)));
        assert_eq!(action("New Window"), Some(MainAction::Ours(101)));
        assert_eq!(action("Back"), Some(MainAction::Ours(102)));
        assert_eq!(
            action("Close Window"),
            Some(MainAction::Standard(c"performClose:")),
            "Cmd+W goes the way of the close button"
        );
        assert_eq!(
            action("Minimize"),
            Some(MainAction::Standard(c"performMiniaturize:"))
        );
        assert_eq!(action("Hide Zellij"), Some(MainAction::Standard(c"hide:")));
    }
}
