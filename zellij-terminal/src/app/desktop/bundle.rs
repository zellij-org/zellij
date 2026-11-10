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
            zellij_utils::distribution::app_id()
        )),
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    #[test]
    fn an_unbundled_zellij_names_the_app_id_it_needs() {
        let reason = super::notifications_need_a_bundle().expect("tests run outside a bundle");
        assert!(
            reason.contains(zellij_utils::distribution::app_id()),
            "{}",
            reason
        );
    }
}
