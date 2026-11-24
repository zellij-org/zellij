//! Opens a new window for each script or folder opened with the app (Open With, a Dock drop,
//! `open -a`).

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;

use objc2::runtime::{AnyObject, Imp, Sel};
use objc2::sel;
use objc2_foundation::{NSArray, NSURL};
use zellij_utils::app_control::AppRequest;

use crate::app::desktop::macos_delegate::add_app_delegate_method;
use crate::launch::opened_request;
use crate::window::AppPoster;

struct Opener {
    poster: AppPoster,
    shell: Option<String>,
}

static OPENER: OnceLock<Opener> = OnceLock::new();
static OPENED: AtomicBool = AtomicBool::new(false);

/// Call before the event loop runs: on a cold launch, AppKit delivers the files before
/// `applicationDidFinishLaunching:`, where the desktop decides on its default window.
pub(crate) fn install_open_handler(poster: AppPoster, default_shell: Option<&Path>) {
    let shell = default_shell
        .map(|shell| shell.to_string_lossy().into_owned())
        .or_else(|| std::env::var("SHELL").ok());
    if OPENER.set(Opener { poster, shell }).is_err() {
        return;
    }
    type Handler = extern "C-unwind" fn(&AnyObject, Sel, &AnyObject, &NSArray<NSURL>);
    let handler: Handler = open_urls;
    // SAFETY: the function matches the method's Objective-C signature, which the type
    // encoding states: returns void, takes self, _cmd, an NSApplication and an NSArray.
    unsafe {
        add_app_delegate_method(
            sel!(application:openURLs:),
            std::mem::transmute::<Handler, Imp>(handler),
            c"v@:@@",
        )
    }
}

/// Whether a file was opened, so a cold launch skips its default window.
pub(crate) fn opened_any() -> bool {
    OPENED.load(Ordering::Relaxed)
}

extern "C-unwind" fn open_urls(
    _this: &AnyObject,
    _cmd: Sel,
    _app: &AnyObject,
    urls: &NSArray<NSURL>,
) {
    let Some(opener) = OPENER.get() else {
        return;
    };
    for url in urls.iter() {
        let Some(path) = url.isFileURL().then(|| url.path()).flatten() else {
            continue;
        };
        let path = path.to_string();
        OPENED.store(true, Ordering::Relaxed);
        let request = opened_request(Path::new(&path), opener.shell.as_deref());
        if !opener
            .poster
            .post(crate::app::AppEvent::Request(AppRequest::OpenWindow(
                request,
            )))
        {
            log::warn!("{} could not be opened: the app is closing", path);
        }
    }
}
