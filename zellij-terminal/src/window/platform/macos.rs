use winit::raw_window_handle::HasWindowHandle;
use winit::window::{Window, WindowAttributes};
use zellij_utils::input::terminal::OptionAsAlt;

pub(in crate::window) fn set_see_through(window: &Window, see_through: bool) {
    use objc2_app_kit::NSColor;
    window.set_transparent(see_through);
    if !see_through {
        return;
    }
    if let Some(ns_window) = ns_window(window) {
        let almost_clear = NSColor::clearColor().colorWithAlphaComponent(0.001);
        ns_window.setBackgroundColor(Some(&almost_clear));
    }
}

pub(in crate::window) fn ns_window(
    window: &Window,
) -> Option<objc2::rc::Retained<objc2_app_kit::NSWindow>> {
    use winit::raw_window_handle::RawWindowHandle;
    let Ok(RawWindowHandle::AppKit(handle)) = window.window_handle().map(|handle| handle.as_raw())
    else {
        return None;
    };
    // SAFETY: winit hands out the handle only on the main thread, and the handle of a live
    // window points at its content view, which the window keeps alive.
    let view = unsafe { handle.ns_view.cast::<objc2_app_kit::NSView>().as_ref() };
    view.window()
}

pub(in crate::window) fn native_option_as_alt(
    side: OptionAsAlt,
) -> winit::platform::macos::OptionAsAlt {
    use winit::platform::macos::OptionAsAlt as Native;
    match side {
        OptionAsAlt::None => Native::None,
        OptionAsAlt::Left => Native::OnlyLeft,
        OptionAsAlt::Right => Native::OnlyRight,
        OptionAsAlt::Both => Native::Both,
    }
}

pub(in crate::window) fn with_option_as_alt(
    attributes: WindowAttributes,
    side: OptionAsAlt,
) -> WindowAttributes {
    use winit::platform::macos::WindowAttributesExtMacOS;
    attributes.with_option_as_alt(native_option_as_alt(side))
}

pub(in crate::window) fn set_option_as_alt(window: &Window, side: OptionAsAlt) {
    use winit::platform::macos::WindowExtMacOS;
    window.set_option_as_alt(native_option_as_alt(side));
}

pub(in crate::window) fn set_accessory(accessory: bool) {
    crate::app::desktop::tray::set_accessory(accessory);
}

pub(in crate::window) fn activate_app() {
    crate::app::desktop::tray::activate();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_option_as_alt_side_reaches_the_windowing_system() {
        use winit::platform::macos::OptionAsAlt as Native;
        for (side, native) in [
            (OptionAsAlt::None, Native::None),
            (OptionAsAlt::Left, Native::OnlyLeft),
            (OptionAsAlt::Right, Native::OnlyRight),
            (OptionAsAlt::Both, Native::Both),
        ] {
            assert_eq!(native_option_as_alt(side), native, "{:?}", side);
        }
    }
}
