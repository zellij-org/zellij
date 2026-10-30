use zellij_utils::input::attention::AttentionConfig;

#[cfg(any(target_os = "linux", test))]
pub const TRAY_HOST: &str = "org.kde.StatusNotifierWatcher";
#[cfg(any(target_os = "linux", test))]
pub const NOTIFICATIONS: &str = "org.freedesktop.Notifications";

#[cfg(any(target_os = "linux", test))]
pub fn missing_descriptions(
    tray_missing: bool,
    notifications_missing: bool,
    desktop: Option<&str>,
) -> Vec<String> {
    let tiling = zellij_utils::desktop_integration::platform::is_tiling_wm(desktop);
    let gnome = desktop
        .map(|d| d.to_ascii_uppercase().contains("GNOME"))
        .unwrap_or(false);
    let mut missing = vec![];
    if tray_missing {
        missing.push(if gnome {
            "No tray host is running, so the tray icon cannot appear. On GNOME, install the \
             AppIndicator extension."
                .to_owned()
        } else if tiling {
            "No tray host is running, so the tray icon cannot appear. Use a bar with a tray, \
             such as waybar or i3bar."
                .to_owned()
        } else {
            "No tray host is running, so the tray icon cannot appear.".to_owned()
        });
    }
    if notifications_missing {
        missing.push(if tiling {
            "No notification service is running, so desktop notifications cannot appear. Run a \
             notification daemon such as mako or dunst."
                .to_owned()
        } else {
            "No notification service is running, so desktop notifications cannot appear.".to_owned()
        });
    }
    missing
}

#[cfg(target_os = "linux")]
pub fn check_soon(attention: &AttentionConfig) {
    let wants_tray = attention.tray_icon();
    let wants_notifications = !attention.desktop_notifications().is_empty();
    if !wants_tray && !wants_notifications {
        return;
    }
    let _ = std::thread::Builder::new()
        .name("app_services".to_owned())
        .spawn(move || {
            std::thread::sleep(std::time::Duration::from_secs(3));
            let Ok(bus) = crate::dbus::Bus::session(|_| Box::new(|_| {})) else {
                return;
            };
            let tray_missing = wants_tray && !bus.name_has_owner(TRAY_HOST);
            let notifications_missing = wants_notifications && !bus.name_has_owner(NOTIFICATIONS);
            let desktop = std::env::var("XDG_CURRENT_DESKTOP")
                .ok()
                .filter(|d| !d.is_empty());
            let missing =
                missing_descriptions(tray_missing, notifications_missing, desktop.as_deref());
            let request =
                zellij_utils::desktop_integration::Request::ReportMissingServices(missing);
            let _ = zellij_utils::desktop_integration::handle_state_request(&request);
        });
}

#[cfg(not(target_os = "linux"))]
pub fn check_soon(_attention: &AttentionConfig) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_services_are_explained_for_the_desktop_in_use() {
        let gnome = missing_descriptions(true, false, Some("ubuntu:GNOME"));
        assert_eq!(gnome.len(), 1);
        assert!(gnome[0].contains("AppIndicator"));
        let sway = missing_descriptions(true, true, Some("sway"));
        assert!(sway[0].contains("waybar"));
        assert!(sway[1].contains("mako"));
        assert!(missing_descriptions(false, false, None).is_empty());
    }
}
