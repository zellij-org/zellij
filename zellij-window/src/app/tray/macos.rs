use std::cell::{Cell, RefCell};

use anyhow::{anyhow, Result};
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObject, NSObjectProtocol, Sel};
use objc2::{
    define_class, msg_send, sel, AllocAnyThread, DefinedClass, MainThreadMarker, MainThreadOnly,
};
use objc2_app_kit::{
    NSAppearanceCustomization, NSAppearanceNameAqua, NSAppearanceNameDarkAqua, NSApplication,
    NSEventMask, NSEventType, NSImage, NSMenu, NSMenuItem, NSScreen, NSStatusBar, NSStatusItem,
    NSVariableStatusItemLength,
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

fn main_menu_item(
    mtm: MainThreadMarker,
    handler: &Handler,
    title: &str,
    tag: isize,
    key: &str,
) -> Retained<NSMenuItem> {
    let item = unsafe {
        NSMenuItem::initWithTitle_action_keyEquivalent(
            NSMenuItem::alloc(mtm),
            &NSString::from_str(title),
            Some(sel!(menuItem:)),
            &NSString::from_str(key),
        )
    };
    unsafe { item.setTarget(Some(handler)) };
    item.setTag(tag);
    item
}

fn titled(
    mtm: MainThreadMarker,
    title: &str,
    items: Vec<Retained<NSMenuItem>>,
) -> Retained<NSMenuItem> {
    let menu = NSMenu::initWithTitle(NSMenu::alloc(mtm), &NSString::from_str(title));
    for item in items {
        menu.addItem(&item);
    }
    let holder = NSMenuItem::new(mtm);
    holder.setSubmenu(Some(&menu));
    holder
}

pub struct MainMenu {
    _handler: Retained<Handler>,
}

pub fn install_main_menu(events: TrayEvents) -> Option<MainMenu> {
    let mtm = MainThreadMarker::new()?;
    let handler = Handler::new(mtm, events);
    let bar = NSMenu::new(mtm);
    bar.addItem(&titled(
        mtm,
        "Zellij",
        vec![main_menu_item(mtm, &handler, "Quit tray", 103, "q")],
    ));
    bar.addItem(&titled(
        mtm,
        "File",
        vec![main_menu_item(mtm, &handler, "New Window", 101, "n")],
    ));
    bar.addItem(&titled(
        mtm,
        "Window",
        vec![main_menu_item(mtm, &handler, "Back", 102, "")],
    ));
    NSApplication::sharedApplication(mtm).setMainMenu(Some(&bar));
    Some(MainMenu { _handler: handler })
}
