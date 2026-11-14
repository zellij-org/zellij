//! winit doesn't implement `applicationShouldHandleReopen:hasVisibleWindows:`, so clicking the
//! running app in the Dock or Finder after closing its last window did nothing. We add it.

use std::sync::OnceLock;

use objc2::ffi::class_addMethod;
use objc2::runtime::{AnyClass, AnyObject, Bool, Imp, Sel};
use objc2::{sel, MainThreadMarker};
use objc2_app_kit::NSApplication;

use super::{MenuAction, TrayEvent, TrayEvents};

static EVENTS: OnceLock<TrayEvents> = OnceLock::new();

/// AppKit brings minimized windows back by itself, so only open one if there are none.
fn reopen_opens_a_window(has_visible_windows: bool, any_minimized: bool) -> bool {
    !has_visible_windows && !any_minimized
}

/// Call on the main thread, after winit has set up its app delegate.
pub fn install_reopen_handler(events: TrayEvents) {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let app = NSApplication::sharedApplication(mtm);
    let Some(delegate) = app.delegate() else {
        log::warn!("the app has no delegate, so opening it again will not open a window");
        return;
    };
    if EVENTS.set(events).is_err() {
        return;
    }
    let delegate: &AnyObject = delegate.as_ref();
    let class: &AnyClass = delegate.class();
    let selector = sel!(applicationShouldHandleReopen:hasVisibleWindows:);
    if class.instance_method(selector).is_some() {
        log::warn!("the app delegate already handles reopening, so it is left as it is");
        return;
    }
    type Handler = extern "C-unwind" fn(&AnyObject, Sel, &AnyObject, Bool) -> Bool;
    let handler: Handler = should_handle_reopen;
    // SAFETY: the function matches the method's Objective-C signature, which the type
    // encoding states: returns BOOL, takes self, _cmd, an NSApplication and a BOOL.
    let added = unsafe {
        class_addMethod(
            class as *const AnyClass as *mut AnyClass,
            selector,
            std::mem::transmute::<Handler, Imp>(handler),
            c"B@:@B".as_ptr(),
        )
    };
    if !added.as_bool() {
        log::warn!("the reopen handler could not be added to the app delegate");
    }
}

extern "C-unwind" fn should_handle_reopen(
    _this: &AnyObject,
    _cmd: Sel,
    _app: &AnyObject,
    has_visible_windows: Bool,
) -> Bool {
    let any_minimized = MainThreadMarker::new()
        .map(|mtm| {
            NSApplication::sharedApplication(mtm)
                .windows()
                .iter()
                .any(|window| window.isMiniaturized())
        })
        .unwrap_or(false);
    if reopen_opens_a_window(has_visible_windows.as_bool(), any_minimized) {
        if let Some(events) = EVENTS.get() {
            events(TrayEvent::Menu(MenuAction::NewWindow));
        }
    }
    Bool::YES
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reopen_opens_a_window_only_when_none_is_visible_or_minimized() {
        assert!(reopen_opens_a_window(false, false));
        assert!(
            !reopen_opens_a_window(false, true),
            "AppKit brings minimized windows back"
        );
        assert!(!reopen_opens_a_window(true, false));
        assert!(!reopen_opens_a_window(true, true));
    }
}
