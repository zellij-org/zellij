use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    Unix,
    Windows,
}

impl Default for Platform {
    fn default() -> Self {
        Platform::Unix
    }
}

impl Platform {
    /// Detect the host platform from the initial_cwd string.
    /// Checks for drive letter (e.g. `C:\` or `C:/`) or UNC path (`\\server\`).
    pub fn detect(initial_cwd: &str) -> Self {
        let bytes = initial_cwd.as_bytes();
        // Drive letter: X:\ or X:/
        if bytes.len() >= 3
            && bytes[0].is_ascii_alphabetic()
            && bytes[1] == b':'
            && (bytes[2] == b'\\' || bytes[2] == b'/')
        {
            return Platform::Windows;
        }
        // UNC path: \\server\share
        if bytes.len() >= 2 && bytes[0] == b'\\' && bytes[1] == b'\\' {
            return Platform::Windows;
        }
        Platform::Unix
    }

    /// Replace `\` with `/` so WASM PathBuf can parse the path correctly.
    /// No-op for paths that don't contain backslashes.
    pub fn normalize(path: &Path) -> PathBuf {
        let s = path.to_string_lossy();
        if s.contains('\\') {
            PathBuf::from(s.replace('\\', "/"))
        } else {
            path.to_path_buf()
        }
    }

    /// Convert internal forward-slash path back to native backslashes for Windows display.
    /// Identity on Unix.
    pub fn to_host_display(path: &Path, platform: Platform) -> String {
        let s = path.to_string_lossy();
        match platform {
            Platform::Windows => s.replace('/', "\\"),
            Platform::Unix => s.into_owned(),
        }
    }

    /// Path separator character for the host platform.
    pub fn separator(self) -> char {
        match self {
            Platform::Windows => '\\',
            Platform::Unix => '/',
        }
    }

    /// Ensure bare drive letters like `C:` become `C:/`.
    /// WASM PathBuf's `parent()` strips the trailing slash, but on Windows
    /// `C:` means "current directory on drive C", not the drive root.
    pub fn ensure_drive_root(path: PathBuf, platform: Platform) -> PathBuf {
        if platform == Platform::Windows {
            let s = path.to_string_lossy();
            if s.len() == 2 && s.as_bytes()[0].is_ascii_alphabetic() && s.as_bytes()[1] == b':' {
                return PathBuf::from(format!("{}/", s));
            }
        }
        path
    }

    /// Display name for a virtual root entry.
    /// `C:/` → `C:\`, `//wsl.localhost/Ubuntu/` → `Ubuntu (WSL)`, `/` → `/`.
    pub fn virtual_root_display_name(path: &Path, platform: Platform) -> String {
        let s = path.to_string_lossy();
        match platform {
            Platform::Windows => {
                if s.starts_with("//wsl.localhost/") {
                    let rest = &s["//wsl.localhost/".len()..];
                    let distro = rest.trim_end_matches('/');
                    return format!("{} (WSL)", distro);
                }
                Platform::to_host_display(path, platform)
            },
            Platform::Unix => s.into_owned(),
        }
    }

    /// Check if a path is a filesystem root.
    /// Unix: `/` or empty.
    /// Windows: `X:/` or `X:` (drive root), or `//server/share` (UNC root with <= 4 components).
    pub fn is_root(path: &Path, platform: Platform) -> bool {
        let s = path.to_string_lossy();
        match platform {
            Platform::Unix => s == "/" || s.is_empty(),
            Platform::Windows => {
                // Drive root: "C:/" or "C:"
                if s.len() == 3
                    && s.as_bytes()[0].is_ascii_alphabetic()
                    && s.as_bytes()[1] == b':'
                    && s.as_bytes()[2] == b'/'
                {
                    return true;
                }
                if s.len() == 2 && s.as_bytes()[0].is_ascii_alphabetic() && s.as_bytes()[1] == b':'
                {
                    return true;
                }
                // UNC root: //server/share (at most 4 path components)
                if s.starts_with("//") {
                    let without_prefix = &s[2..];
                    let parts: Vec<&str> = without_prefix
                        .split('/')
                        .filter(|p| !p.is_empty())
                        .collect();
                    return parts.len() <= 2;
                }
                s.is_empty()
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_unix() {
        assert_eq!(Platform::detect("/home/user"), Platform::Unix);
        assert_eq!(Platform::detect("/"), Platform::Unix);
    }

    #[test]
    fn detect_windows_drive() {
        assert_eq!(Platform::detect("C:\\Users\\user"), Platform::Windows);
        assert_eq!(Platform::detect("D:/Projects"), Platform::Windows);
    }

    #[test]
    fn detect_windows_unc() {
        assert_eq!(
            Platform::detect("\\\\wsl.localhost\\Ubuntu"),
            Platform::Windows
        );
    }

    #[test]
    fn normalize_backslashes() {
        let p = PathBuf::from("C:\\Users\\user");
        assert_eq!(Platform::normalize(&p), PathBuf::from("C:/Users/user"));
    }

    #[test]
    fn normalize_noop_unix() {
        let p = PathBuf::from("/home/user");
        assert_eq!(Platform::normalize(&p), PathBuf::from("/home/user"));
    }

    #[test]
    fn to_host_display_windows() {
        let p = PathBuf::from("C:/Users/user");
        assert_eq!(
            Platform::to_host_display(&p, Platform::Windows),
            "C:\\Users\\user"
        );
    }

    #[test]
    fn to_host_display_unix() {
        let p = PathBuf::from("/home/user");
        assert_eq!(Platform::to_host_display(&p, Platform::Unix), "/home/user");
    }

    #[test]
    fn is_root_unix() {
        assert!(Platform::is_root(&PathBuf::from("/"), Platform::Unix));
        assert!(Platform::is_root(&PathBuf::from(""), Platform::Unix));
        assert!(!Platform::is_root(&PathBuf::from("/home"), Platform::Unix));
    }

    #[test]
    fn is_root_windows_drive() {
        assert!(Platform::is_root(&PathBuf::from("C:/"), Platform::Windows));
        assert!(Platform::is_root(&PathBuf::from("C:"), Platform::Windows));
        assert!(!Platform::is_root(
            &PathBuf::from("C:/Users"),
            Platform::Windows
        ));
    }

    #[test]
    fn ensure_drive_root_fixes_bare_drive() {
        assert_eq!(
            Platform::ensure_drive_root(PathBuf::from("C:"), Platform::Windows),
            PathBuf::from("C:/")
        );
    }

    #[test]
    fn ensure_drive_root_noop_with_slash() {
        assert_eq!(
            Platform::ensure_drive_root(PathBuf::from("C:/"), Platform::Windows),
            PathBuf::from("C:/")
        );
    }

    #[test]
    fn ensure_drive_root_noop_unix() {
        assert_eq!(
            Platform::ensure_drive_root(PathBuf::from("/"), Platform::Unix),
            PathBuf::from("/")
        );
    }

    #[test]
    fn is_root_windows_unc() {
        assert!(Platform::is_root(
            &PathBuf::from("//server/share"),
            Platform::Windows
        ));
        assert!(!Platform::is_root(
            &PathBuf::from("//server/share/folder"),
            Platform::Windows
        ));
    }
}
