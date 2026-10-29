#[cfg(target_os = "macos")]
pub const BUNDLE_IDENTIFIER: &str = "org.zellij.Zellij";
#[cfg(windows)]
pub const WINDOWS_APP_ID: &str = "Zellij.Zellij";

#[cfg(target_os = "macos")]
pub fn running_bundle_identifier() -> Option<String> {
    let bundle = objc2_foundation::NSBundle::mainBundle();
    bundle.bundleIdentifier().map(|id| id.to_string())
}

#[cfg(target_os = "macos")]
pub fn notifications_need_a_bundle() -> Option<String> {
    match running_bundle_identifier() {
        Some(_) => None,
        None => Some(format!(
            "macOS shows notifications only for a bundled, signed app (Zellij.app with the \
             bundle identifier {}); this zellij runs outside a bundle",
            BUNDLE_IDENTIFIER
        )),
    }
}
