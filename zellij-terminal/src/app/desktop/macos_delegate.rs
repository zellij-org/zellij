//! Adds AppKit callbacks that winit's app delegate doesn't implement.

use std::ffi::CStr;

use objc2::ffi::class_addMethod;
use objc2::runtime::{AnyClass, AnyObject, Imp, Sel};
use objc2::MainThreadMarker;
use objc2_app_kit::NSApplication;

/// # Safety
///
/// `method` must be an `extern "C-unwind"` function matching the type encoding `types`.
pub unsafe fn add_app_delegate_method(selector: Sel, method: Imp, types: &CStr) {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let app = NSApplication::sharedApplication(mtm);
    let Some(delegate) = app.delegate() else {
        log::warn!(
            "the app has no delegate, so {} is not added",
            selector.name().to_string_lossy()
        );
        return;
    };
    let delegate: &AnyObject = delegate.as_ref();
    let class: &AnyClass = delegate.class();
    if class.instance_method(selector).is_some() {
        log::warn!(
            "the app delegate already implements {}, so it is left as it is",
            selector.name().to_string_lossy()
        );
        return;
    }
    // SAFETY: the caller guarantees that `method` matches `types`.
    let added = unsafe {
        class_addMethod(
            class as *const AnyClass as *mut AnyClass,
            selector,
            method,
            types.as_ptr(),
        )
    };
    if !added.as_bool() {
        log::warn!(
            "{} could not be added to the app delegate",
            selector.name().to_string_lossy()
        );
    }
}
