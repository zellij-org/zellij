use zellij_utils::distribution::{distribution, Distribution};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identity {
    pub app_id: &'static str,
    #[cfg_attr(target_os = "macos", allow(dead_code))]
    pub display_name: &'static str,
    #[cfg(windows)]
    pub windows_app_id: String,
}

pub fn identity() -> Identity {
    identity_of(distribution())
}

pub fn identity_of(distribution: &Distribution) -> Identity {
    Identity {
        app_id: distribution.name,
        display_name: distribution.display_name,
        #[cfg(windows)]
        windows_app_id: windows_app_id(distribution.name),
    }
}

#[cfg(any(windows, test))]
fn windows_app_id(name: &str) -> String {
    format!("{}.window", name)
}

pub fn declare() {
    platform::declare(&identity());
}

#[cfg(windows)]
mod platform {
    use windows_sys::Win32::UI::Shell::SetCurrentProcessExplicitAppUserModelID;

    use super::Identity;

    pub fn declare(identity: &Identity) {
        let app_id: Vec<u16> = identity
            .windows_app_id
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        let result = unsafe { SetCurrentProcessExplicitAppUserModelID(app_id.as_ptr()) };
        if result < 0 {
            eprintln!(
                "zellij-window: Windows refused the app identity {:?} ({:#010x}), so the taskbar \
                 will group the window by its executable instead",
                identity.windows_app_id, result
            );
        }
    }
}

#[cfg(not(windows))]
mod platform {
    use super::Identity;

    pub fn declare(_identity: &Identity) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stock_zellij_is_identified_as_zellij() {
        assert_eq!(
            identity_of(&Distribution::zellij()),
            Identity {
                app_id: "zellij",
                display_name: "Zellij",
                #[cfg(windows)]
                windows_app_id: "zellij.window".to_owned(),
            }
        );
    }

    #[test]
    fn a_distribution_brings_its_own_identity() {
        let acme = Distribution::new("acme", "1.0.0").with_display_name("Acme Shell");
        assert_eq!(
            identity_of(&acme),
            Identity {
                app_id: "acme",
                display_name: "Acme Shell",
                #[cfg(windows)]
                windows_app_id: "acme.window".to_owned(),
            },
            "the Windows app id follows the stable name, not the display name"
        );
    }

    #[test]
    fn the_windows_app_id_is_the_name_marked_as_the_window() {
        assert_eq!(windows_app_id("zellij"), "zellij.window");
        assert_eq!(windows_app_id("acme"), "acme.window");
    }

    #[test]
    fn the_windows_app_id_is_one_windows_accepts() {
        for name in [
            Distribution::zellij().name,
            "a-long_distribution-name-0123456789",
        ] {
            let app_id = windows_app_id(name);
            assert!(
                app_id.len() <= 128,
                "{} is longer than 128 characters",
                app_id
            );
            assert!(!app_id.contains(' '), "{} contains a space", app_id);
        }
    }

    #[cfg(windows)]
    #[test]
    fn declaring_makes_the_windows_app_id_the_process_one() {
        use windows_sys::Win32::System::Com::CoTaskMemFree;
        use windows_sys::Win32::UI::Shell::GetCurrentProcessExplicitAppUserModelID;

        declare();
        let mut declared = std::ptr::null_mut();
        let result = unsafe { GetCurrentProcessExplicitAppUserModelID(&mut declared) };
        assert!(
            result >= 0,
            "no app identity was declared ({:#010x})",
            result
        );
        let text = unsafe {
            let length = (0..).take_while(|&i| *declared.add(i) != 0).count();
            let text = String::from_utf16_lossy(std::slice::from_raw_parts(declared, length));
            CoTaskMemFree(declared as *const std::ffi::c_void);
            text
        };
        assert_eq!(text, identity().windows_app_id);
    }
}
