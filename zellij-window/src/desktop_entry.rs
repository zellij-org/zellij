#[cfg(all(unix, not(target_os = "macos"), not(target_os = "android")))]
pub use xdg::{install, uninstall};

#[cfg(target_os = "macos")]
pub fn install() -> anyhow::Result<()> {
    anyhow::bail!(MACOS)
}

#[cfg(target_os = "macos")]
pub fn uninstall() -> anyhow::Result<()> {
    anyhow::bail!(MACOS)
}

#[cfg(target_os = "macos")]
const MACOS: &str = "installing a desktop entry is not supported on this platform yet. On macOS \
                     the launcher identity comes from a Zellij.app bundle (Contents/Info.plist, a \
                     Contents/MacOS entry running this binary with the window verb, and \
                     Contents/Resources/zellij.icns) rather than from the binary, and writing that \
                     bundle is part of the macOS bring-up";

#[cfg(windows)]
pub use start_menu::{install, uninstall};

#[cfg(not(any(
    all(unix, not(target_os = "macos"), not(target_os = "android")),
    target_os = "macos",
    target_os = "windows"
)))]
pub fn install() -> anyhow::Result<()> {
    anyhow::bail!("installing a desktop entry is not supported on this platform")
}

#[cfg(not(any(
    all(unix, not(target_os = "macos"), not(target_os = "android")),
    target_os = "macos",
    target_os = "windows"
)))]
pub fn uninstall() -> anyhow::Result<()> {
    anyhow::bail!("removing a desktop entry is not supported on this platform")
}

#[cfg(all(unix, not(target_os = "macos"), not(target_os = "android")))]
mod xdg {
    use std::ffi::OsStr;
    use std::path::{Path, PathBuf};
    use std::process::{Command, Stdio};

    use anyhow::{bail, Context, Result};

    use crate::window::{application_id, ICON_PNG};

    const ICON_SIZE: &str = "128x128";

    struct Installed {
        entry: PathBuf,
        icon: PathBuf,
    }

    pub fn install() -> Result<()> {
        let data_home = data_home()?;
        let exe = std::env::current_exe().context("failed to locate the running binary")?;
        if !exe.is_absolute() {
            bail!(
                "the running binary resolved to the relative path {:?}, so no launcher entry can \
                 be written for it",
                exe
            );
        }
        let installed = install_into(&data_home, &exe)?;
        println!("Wrote {}", installed.entry.display());
        println!("Wrote {}", installed.icon.display());
        refresh(&data_home);
        println!(
            "The entry runs {} — re-run this command if that binary moves.",
            exe.display()
        );
        Ok(())
    }

    pub fn uninstall() -> Result<()> {
        let data_home = data_home()?;
        let removed = uninstall_from(&data_home)?;
        if removed.is_empty() {
            println!("Nothing to remove under {}", data_home.display());
        } else {
            for path in &removed {
                println!("Removed {}", path.display());
            }
            refresh(&data_home);
        }
        Ok(())
    }

    fn install_into(data_home: &Path, exe: &Path) -> Result<Installed> {
        let exe = exe.to_str().with_context(|| {
            format!(
                "the path of the running binary is not valid UTF-8, which a desktop entry \
                 requires: {:?}",
                exe
            )
        })?;
        let entry = entry_path(data_home);
        let icon = icon_path(data_home);
        write(&entry, render(&exec_argument(exe)).as_bytes())?;
        write(&icon, ICON_PNG)?;
        Ok(Installed { entry, icon })
    }

    fn uninstall_from(data_home: &Path) -> Result<Vec<PathBuf>> {
        let mut removed = Vec::new();
        for path in [entry_path(data_home), icon_path(data_home)] {
            match std::fs::remove_file(&path) {
                Ok(()) => removed.push(path),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {},
                Err(e) => {
                    return Err(e).with_context(|| format!("failed to remove {:?}", path));
                },
            }
        }
        Ok(removed)
    }

    fn write(path: &Path, contents: &[u8]) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {:?}", parent))?;
        }
        std::fs::write(path, contents).with_context(|| format!("failed to write {:?}", path))
    }

    fn entry_path(data_home: &Path) -> PathBuf {
        data_home
            .join("applications")
            .join(format!("{}.desktop", application_id()))
    }

    fn icon_path(data_home: &Path) -> PathBuf {
        data_home
            .join("icons")
            .join("hicolor")
            .join(ICON_SIZE)
            .join("apps")
            .join(format!("{}.png", application_id()))
    }

    fn data_home() -> Result<PathBuf> {
        if let Some(from_env) = std::env::var_os("XDG_DATA_HOME") {
            let from_env = PathBuf::from(from_env);
            if from_env.is_absolute() {
                return Ok(from_env);
            }
        }
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .filter(|home| home.is_absolute())
            .context(
                "neither XDG_DATA_HOME nor HOME names an absolute directory, so there is nowhere \
                 to install to",
            )?;
        Ok(home.join(".local").join("share"))
    }

    pub fn render(exec: &str) -> String {
        let id = application_id();
        let name = crate::identity::identity().display_name;
        format!(
            "[Desktop Entry]\n\
             Version=1.0\n\
             Type=Application\n\
             Name={name}\n\
             GenericName=Terminal Workspace\n\
             Comment=Manage Your Terminal Applications\n\
             Exec={exec} window\n\
             Icon={id}\n\
             Terminal=false\n\
             StartupWMClass={id}\n\
             Categories=System;TerminalEmulator;\n\
             Keywords=terminal;multiplexer;shell;workspace;\n"
        )
    }

    fn exec_argument(path: &str) -> String {
        const RESERVED: &[char] = &[
            ' ', '\t', '\n', '"', '\'', '\\', '>', '<', '~', '|', '&', ';', '$', '*', '?', '#',
            '(', ')', '`',
        ];
        let path = path.replace('%', "%%");
        if !path.chars().any(|c| RESERVED.contains(&c)) {
            return path;
        }
        let mut quoted = String::with_capacity(path.len() + 2);
        quoted.push('"');
        for c in path.chars() {
            if matches!(c, '"' | '`' | '$' | '\\') {
                quoted.push('\\');
            }
            quoted.push(c);
        }
        quoted.push('"');
        quoted
    }

    fn refresh(data_home: &Path) {
        best_effort(
            "update-desktop-database",
            &[data_home.join("applications").as_os_str()],
        );
        best_effort(
            "gtk-update-icon-cache",
            &[
                OsStr::new("-q"),
                OsStr::new("-t"),
                OsStr::new("-f"),
                data_home.join("icons").join("hicolor").as_os_str(),
            ],
        );
    }

    fn best_effort(program: &str, args: &[&OsStr]) {
        let outcome = Command::new(program)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        match outcome {
            Ok(status) if status.success() => {},
            Ok(status) => println!(
                "{program} exited with {status}; a launcher may need a restart to notice the entry"
            ),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {},
            Err(e) => println!(
                "{program} could not be run ({e}); a launcher may need a restart to notice the \
                 entry"
            ),
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        const SHIPPED_ASSET: &str = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../assets/zellij.desktop"
        ));

        fn keys(entry: &str) -> Vec<(String, String)> {
            entry
                .lines()
                .filter_map(|line| line.split_once('='))
                .map(|(key, value)| (key.to_string(), value.to_string()))
                .collect()
        }

        fn value<'a>(entry: &'a str, key: &str) -> &'a str {
            entry
                .lines()
                .filter_map(|line| line.split_once('='))
                .find(|(found, _)| *found == key)
                .unwrap_or_else(|| panic!("{key} is missing from the entry:\n{entry}"))
                .1
        }

        #[test]
        fn install_writes_an_entry_and_an_icon_under_the_given_data_home() {
            let dir = tempfile::TempDir::new().unwrap();
            let exe = PathBuf::from("/opt/zellij/bin/zellij");
            let installed = install_into(dir.path(), &exe).unwrap();

            assert_eq!(
                installed.entry,
                dir.path().join("applications").join("zellij.desktop")
            );
            assert_eq!(
                installed.icon,
                dir.path()
                    .join("icons/hicolor/128x128/apps")
                    .join("zellij.png")
            );

            let entry = std::fs::read_to_string(&installed.entry).unwrap();
            assert_eq!(entry.lines().next(), Some("[Desktop Entry]"));
            assert_eq!(value(&entry, "Type"), "Application");
            assert_eq!(value(&entry, "Name"), "Zellij");
            assert_eq!(value(&entry, "Terminal"), "false");
            assert_eq!(value(&entry, "Icon"), "zellij");
            assert_eq!(value(&entry, "Exec"), "/opt/zellij/bin/zellij window");
            let exec = value(&entry, "Exec");
            assert!(exec.starts_with('/'), "Exec is not absolute: {exec}");
            assert!(
                exec.ends_with(" window"),
                "Exec does not run the window verb: {exec}"
            );
            assert!(
                keys(&entry).iter().all(|(key, _)| !key.is_empty()),
                "the entry has an empty key:\n{entry}"
            );

            assert_eq!(std::fs::read(&installed.icon).unwrap(), ICON_PNG);
        }

        #[test]
        fn the_entry_passes_desktop_file_validate_where_it_exists() {
            let dir = tempfile::TempDir::new().unwrap();
            let installed = install_into(dir.path(), Path::new("/opt/zellij/bin/zellij")).unwrap();
            let outcome = Command::new("desktop-file-validate")
                .arg(&installed.entry)
                .output();
            match outcome {
                Ok(output) => assert!(
                    output.status.success(),
                    "desktop-file-validate refused the entry: {}{}",
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                ),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {},
                Err(e) => panic!("desktop-file-validate could not be run: {e}"),
            }
        }

        #[test]
        fn the_three_names_agree() {
            let dir = tempfile::TempDir::new().unwrap();
            let installed = install_into(dir.path(), Path::new("/opt/zellij/bin/zellij")).unwrap();
            let entry = std::fs::read_to_string(&installed.entry).unwrap();

            let app_id = application_id();
            assert_eq!(
                installed.entry.file_stem().unwrap().to_str().unwrap(),
                app_id
            );
            assert_eq!(value(&entry, "Icon"), app_id);
            assert_eq!(
                installed.icon.file_stem().unwrap().to_str().unwrap(),
                value(&entry, "Icon")
            );
            assert_eq!(value(&entry, "StartupWMClass"), app_id);
        }

        #[test]
        fn uninstall_removes_what_install_wrote_and_nothing_else() {
            let dir = tempfile::TempDir::new().unwrap();
            let installed = install_into(dir.path(), Path::new("/opt/zellij/bin/zellij")).unwrap();
            let decoy_entry = dir.path().join("applications").join("other.desktop");
            let decoy_icon = dir
                .path()
                .join("icons/hicolor/128x128/apps")
                .join("other.png");
            write(&decoy_entry, b"[Desktop Entry]\n").unwrap();
            write(&decoy_icon, b"not a png").unwrap();

            let removed = uninstall_from(dir.path()).unwrap();

            assert_eq!(
                removed,
                vec![installed.entry.clone(), installed.icon.clone()]
            );
            assert!(!installed.entry.exists());
            assert!(!installed.icon.exists());
            assert!(decoy_entry.exists());
            assert!(decoy_icon.exists());
            assert!(dir.path().join("applications").is_dir());
            assert!(installed.icon.parent().unwrap().is_dir());
        }

        #[test]
        fn a_second_install_overwrites_and_a_second_uninstall_removes_nothing() {
            let dir = tempfile::TempDir::new().unwrap();
            install_into(dir.path(), Path::new("/first/zellij")).unwrap();
            let installed = install_into(dir.path(), Path::new("/second/zellij")).unwrap();
            let entry = std::fs::read_to_string(&installed.entry).unwrap();
            assert_eq!(value(&entry, "Exec"), "/second/zellij window");
            assert_eq!(std::fs::read(&installed.icon).unwrap(), ICON_PNG);

            assert_eq!(uninstall_from(dir.path()).unwrap().len(), 2);
            assert!(uninstall_from(dir.path()).unwrap().is_empty());
        }

        #[test]
        fn an_exec_path_needing_quotes_is_quoted_and_escaped() {
            assert_eq!(
                exec_argument("/opt/zellij/bin/zellij"),
                "/opt/zellij/bin/zellij"
            );
            assert_eq!(exec_argument("/home/a b/zellij"), "\"/home/a b/zellij\"");
            assert_eq!(exec_argument("/opt/100%/zellij"), "/opt/100%%/zellij");
            assert_eq!(
                exec_argument("/opt/a$b`c\"d/zellij"),
                "\"/opt/a\\$b\\`c\\\"d/zellij\""
            );
        }

        #[test]
        fn the_shipped_asset_is_the_same_entry_with_a_path_relative_exec() {
            assert_eq!(SHIPPED_ASSET, render("zellij"));
        }
    }
}

#[cfg(windows)]
mod start_menu {
    use std::path::{Path, PathBuf};

    use anyhow::{Context, Result};
    use windows::core::{Interface, HSTRING};
    use windows::Win32::Storage::EnhancedStorage::PKEY_AppUserModel_ID;
    use windows::Win32::System::Com::{CoCreateInstance, IPersistFile, CLSCTX_INPROC_SERVER};
    use windows::Win32::UI::Shell::PropertiesSystem::IPropertyStore;
    use windows::Win32::UI::Shell::{FOLDERID_Profile, FOLDERID_Programs, IShellLinkW, ShellLink};

    use crate::com::{known_folder, set_string, Com};
    use crate::identity::identity;
    use crate::launcher::{window_command, WindowCommand};

    const DESCRIPTION: &str = "Manage Your Terminal Applications";

    pub fn install() -> Result<()> {
        let _com = Com::initialize()?;
        let exe = std::env::current_exe().context("failed to locate the running binary")?;
        let command = window_command(&exe);
        let start_in = known_folder(&FOLDERID_Profile)?;
        let path = install_into(&known_folder(&FOLDERID_Programs)?, &command, &start_in)?;
        println!("Wrote {}", path.display());
        if !command.uses_launcher() {
            println!(
                "No launcher was found next to {}, so the shortcut runs it with `window` and a \
                 console window opens alongside the zellij window.",
                exe.display()
            );
        }
        println!(
            "The shortcut runs {} — re-run this command if that binary moves.",
            command.program.display()
        );
        Ok(())
    }

    pub fn uninstall() -> Result<()> {
        let _com = Com::initialize()?;
        let dir = known_folder(&FOLDERID_Programs)?;
        let mut removed: Vec<String> = uninstall_from(&dir)?
            .map(|path| path.display().to_string())
            .into_iter()
            .collect();
        removed.extend(crate::app_registration::unregister()?);
        if removed.is_empty() {
            println!("Nothing to remove");
        }
        for item in removed {
            println!("Removed {}", item);
        }
        Ok(())
    }

    fn shortcut_path(dir: &Path) -> PathBuf {
        dir.join(format!("{}.lnk", identity().display_name))
    }

    fn install_into(dir: &Path, command: &WindowCommand, start_in: &Path) -> Result<PathBuf> {
        std::fs::create_dir_all(dir).with_context(|| format!("failed to create {:?}", dir))?;
        let path = shortcut_path(dir);
        unsafe {
            let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)
                .context("failed to create a shell link")?;
            link.SetPath(&HSTRING::from(command.program.as_path()))?;
            link.SetArguments(&HSTRING::from(command.arguments))?;
            link.SetWorkingDirectory(&HSTRING::from(start_in))?;
            link.SetDescription(&HSTRING::from(DESCRIPTION))?;
            let store: IPropertyStore = link.cast()?;
            set_string(&store, &PKEY_AppUserModel_ID, &identity().windows_app_id)?;
            store.Commit()?;
            link.cast::<IPersistFile>()?
                .Save(&HSTRING::from(path.as_path()), true)
                .with_context(|| format!("failed to write {:?}", path))?;
        }
        Ok(path)
    }

    fn uninstall_from(dir: &Path) -> Result<Option<PathBuf>> {
        let path = shortcut_path(dir);
        match std::fs::remove_file(&path) {
            Ok(()) => Ok(Some(path)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e).with_context(|| format!("failed to remove {:?}", path)),
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use crate::com::get_string;
        use windows::Win32::System::Com::STGM_READ;

        struct Read {
            target: String,
            arguments: String,
            start_in: String,
            app_id: String,
        }

        fn read(path: &Path) -> Read {
            let text = |buffer: &[u16]| {
                let end = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
                String::from_utf16_lossy(&buffer[..end])
            };
            unsafe {
                let link: IShellLinkW =
                    CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).unwrap();
                link.cast::<IPersistFile>()
                    .unwrap()
                    .Load(&HSTRING::from(path), STGM_READ)
                    .unwrap();
                let (mut target, mut arguments, mut start_in) =
                    ([0u16; 512], [0u16; 512], [0u16; 512]);
                link.GetPath(&mut target, std::ptr::null_mut(), 0).unwrap();
                link.GetArguments(&mut arguments).unwrap();
                link.GetWorkingDirectory(&mut start_in).unwrap();
                let store: IPropertyStore = link.cast().unwrap();
                Read {
                    target: text(&target),
                    arguments: text(&arguments),
                    start_in: text(&start_in),
                    app_id: get_string(&store, &PKEY_AppUserModel_ID).unwrap(),
                }
            }
        }

        fn launcher_in(dir: &Path) -> WindowCommand {
            WindowCommand {
                program: dir.join("zellij-window.exe"),
                arguments: "",
            }
        }

        #[test]
        fn install_writes_a_shortcut_that_starts_the_window_under_the_app_id() {
            let _com = Com::initialize().unwrap();
            let dir = tempfile::TempDir::new().unwrap();
            let home = dir.path().join("home");
            let command = launcher_in(dir.path());

            let path = install_into(&dir.path().join("Programs"), &command, &home).unwrap();

            assert_eq!(path, dir.path().join("Programs").join("Zellij.lnk"));
            let shortcut = read(&path);
            assert_eq!(shortcut.target, command.program.to_string_lossy());
            assert_eq!(shortcut.arguments, "");
            assert_eq!(shortcut.start_in, home.to_string_lossy());
            assert_eq!(
                shortcut.app_id, "zellij.window",
                "the shortcut and the window it opens must share one taskbar button"
            );
        }

        #[test]
        fn without_a_launcher_the_shortcut_runs_the_window_verb() {
            let _com = Com::initialize().unwrap();
            let dir = tempfile::TempDir::new().unwrap();
            let command = WindowCommand {
                program: dir.path().join("zellij.exe"),
                arguments: "window",
            };
            let path = install_into(dir.path(), &command, dir.path()).unwrap();
            let shortcut = read(&path);
            assert_eq!(shortcut.target, command.program.to_string_lossy());
            assert_eq!(shortcut.arguments, "window");
        }

        #[test]
        fn a_second_install_overwrites_and_uninstall_removes_only_the_shortcut() {
            let _com = Com::initialize().unwrap();
            let dir = tempfile::TempDir::new().unwrap();
            let decoy = dir.path().join("Other.lnk");
            std::fs::write(&decoy, b"not ours").unwrap();

            let (first, second) = (dir.path().join("first"), dir.path().join("second"));
            install_into(dir.path(), &launcher_in(&first), dir.path()).unwrap();
            let path = install_into(dir.path(), &launcher_in(&second), dir.path()).unwrap();
            assert_eq!(
                read(&path).target,
                second.join("zellij-window.exe").to_string_lossy()
            );

            assert_eq!(uninstall_from(dir.path()).unwrap(), Some(path.clone()));
            assert!(!path.exists());
            assert!(
                decoy.exists(),
                "uninstall removed a shortcut it did not write"
            );
            assert_eq!(uninstall_from(dir.path()).unwrap(), None);
        }
    }
}
