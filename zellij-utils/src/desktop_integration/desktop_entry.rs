#[cfg(all(unix, not(target_os = "macos"), not(target_os = "android")))]
pub(crate) mod xdg {
    use std::ffi::OsStr;
    use std::path::{Path, PathBuf};
    use std::process::{Command, Stdio};

    use crate::desktop_integration::location::{
        self, entry_values, has_marker, FINGERPRINT_KEY, MARKER_KEY,
    };
    use crate::desktop_integration::{
        fingerprint, LauncherRecord, ICON_PNG, ICON_PNG_256, ICON_PNG_512,
    };
    use crate::distribution::app_id as application_id;
    use anyhow::{bail, Context, Result};

    pub fn icons() -> [(&'static str, &'static [u8]); 3] {
        [
            ("128x128", ICON_PNG),
            ("256x256", ICON_PNG_256),
            ("512x512", ICON_PNG_512),
        ]
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct Installed {
        pub entry: PathBuf,
        pub icons: Vec<PathBuf>,
        pub removed: Vec<PathBuf>,
        pub binary: PathBuf,
        pub fingerprint: String,
    }

    impl Installed {
        pub fn record(&self) -> LauncherRecord {
            LauncherRecord {
                path: self.entry.clone(),
                binary: self.binary.clone(),
                fingerprint: self.fingerprint.clone(),
            }
        }
    }

    pub fn install_into(data_home: &Path, binary: &Path) -> Result<Installed> {
        let binary_text = binary.to_str().with_context(|| {
            format!(
                "the path of the running binary is not valid UTF-8, which a desktop entry \
                 requires: {:?}",
                binary
            )
        })?;
        let entry = entry_path(data_home);
        if let Ok(existing) = std::fs::read_to_string(&entry) {
            if !has_marker(&existing) && !written_by_an_older_zellij(&existing) {
                bail!(
                    "{} exists and was not written by Zellij, so it was left alone",
                    entry.display()
                );
            }
        }
        let text = render_installed(&exec_argument(binary_text), binary_text);
        write(&entry, text.as_bytes())?;
        let mut written_icons = vec![];
        for (size, bytes) in icons() {
            let path = icon_path(data_home, size);
            write(&path, bytes)?;
            written_icons.push(path);
        }
        let removed = remove_old_launcher(data_home)?;
        Ok(Installed {
            entry,
            icons: written_icons,
            removed,
            binary: binary.to_path_buf(),
            fingerprint: fingerprint(&location::without_marker(&text)),
        })
    }

    pub fn files_to_remove(data_home: &Path) -> Vec<String> {
        let mut files = vec![];
        let entry = entry_path(data_home);
        if std::fs::read_to_string(&entry)
            .map(|text| has_marker(&text) || written_by_an_older_zellij(&text))
            .unwrap_or(false)
        {
            files.push(entry.display().to_string());
        }
        for (size, _) in icons() {
            let icon = icon_path(data_home, size);
            if icon.is_file() {
                files.push(icon.display().to_string());
            }
        }
        if old_name() != application_id() {
            let old_entry = data_home
                .join("applications")
                .join(format!("{}.desktop", old_name()));
            if std::fs::read_to_string(&old_entry)
                .map(|text| has_marker(&text) || written_by_an_older_zellij(&text))
                .unwrap_or(false)
            {
                files.push(old_entry.display().to_string());
            }
        }
        files
    }

    pub fn uninstall_from(data_home: &Path) -> Result<Vec<PathBuf>> {
        let mut removed = Vec::new();
        let entry = entry_path(data_home);
        match std::fs::read_to_string(&entry) {
            Ok(text) if has_marker(&text) || written_by_an_older_zellij(&text) => {
                remove(&entry, &mut removed)?;
            },
            Ok(_) => {
                println!(
                    "{} was not written by Zellij, so it was left alone",
                    entry.display()
                );
            },
            Err(_) => {},
        }
        for (size, _) in icons() {
            remove(&icon_path(data_home, size), &mut removed)?;
        }
        removed.extend(remove_old_launcher(data_home)?);
        Ok(removed)
    }

    fn remove(path: &Path, removed: &mut Vec<PathBuf>) -> Result<()> {
        match std::fs::remove_file(path) {
            Ok(()) => {
                removed.push(path.to_path_buf());
                Ok(())
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e).with_context(|| format!("failed to remove {:?}", path)),
        }
    }

    fn old_name() -> &'static str {
        crate::distribution::name()
    }

    fn remove_old_launcher(data_home: &Path) -> Result<Vec<PathBuf>> {
        let mut removed = vec![];
        if old_name() == application_id() {
            return Ok(removed);
        }
        let old_entry = data_home
            .join("applications")
            .join(format!("{}.desktop", old_name()));
        let Ok(text) = std::fs::read_to_string(&old_entry) else {
            return Ok(removed);
        };
        if !has_marker(&text) && !written_by_an_older_zellij(&text) {
            return Ok(removed);
        }
        remove(&old_entry, &mut removed)?;
        for (size, _) in icons() {
            let old_icon = data_home
                .join("icons/hicolor")
                .join(size)
                .join("apps")
                .join(format!("{}.png", old_name()));
            remove(&old_icon, &mut removed)?;
        }
        Ok(removed)
    }

    fn written_by_an_older_zellij(text: &str) -> bool {
        let values = entry_values(text);
        let Some(exec) = values.get("Exec") else {
            return false;
        };
        let Some(program) = exec.strip_suffix(" window") else {
            return false;
        };
        text == older_render(program, old_name()) || text == older_render(program, application_id())
    }

    fn older_render(exec: &str, id: &str) -> String {
        let name = crate::distribution::display_name();
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
             Keywords=terminal;multiplexer;shell;workspace;\n\
             Actions=inbox;sessions;new-window;\n\
             \n\
             [Desktop Action inbox]\n\
             Name=Show Inbox\n\
             Exec={exec} window --inbox\n\
             \n\
             [Desktop Action sessions]\n\
             Name=Show Sessions\n\
             Exec={exec} window --sessions\n\
             \n\
             [Desktop Action new-window]\n\
             Name=New Window\n\
             Exec={exec} window\n"
        )
    }

    pub fn write(path: &Path, contents: &[u8]) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {:?}", parent))?;
        }
        std::fs::write(path, contents).with_context(|| format!("failed to write {:?}", path))
    }

    pub fn entry_path(data_home: &Path) -> PathBuf {
        location::user_launcher_path(data_home)
    }

    pub fn icon_path(data_home: &Path, size: &str) -> PathBuf {
        data_home
            .join("icons")
            .join("hicolor")
            .join(size)
            .join("apps")
            .join(format!("{}.png", application_id()))
    }

    pub fn data_home() -> Result<PathBuf> {
        location::data_home().context(
            "neither XDG_DATA_HOME nor HOME names an absolute directory, so there is nowhere \
             to install to",
        )
    }

    pub fn render(exec: &str, try_exec: &str) -> String {
        let id = application_id();
        let name = crate::distribution::display_name();
        format!(
            "[Desktop Entry]\n\
             Version=1.0\n\
             Type=Application\n\
             Name={name}\n\
             GenericName=Terminal Workspace\n\
             Comment=Manage Your Terminal Applications\n\
             Exec={exec} terminal\n\
             TryExec={try_exec}\n\
             Icon={id}\n\
             Terminal=false\n\
             StartupNotify=true\n\
             StartupWMClass={id}\n\
             Categories=System;TerminalEmulator;\n\
             Keywords=terminal;multiplexer;shell;workspace;\n\
             X-TerminalArgExec=-e\n\
             X-TerminalArgDir=--working-directory=\n\
             X-TerminalArgHold=--hold\n\
             X-ExecArg=-e\n\
             Actions=inbox;sessions;new-window;\n\
             \n\
             [Desktop Action inbox]\n\
             Name=Show Inbox\n\
             Exec={exec} terminal --inbox\n\
             \n\
             [Desktop Action sessions]\n\
             Name=Show Sessions\n\
             Exec={exec} terminal --sessions\n\
             \n\
             [Desktop Action new-window]\n\
             Name=New Window\n\
             Exec={exec} terminal\n"
        )
    }

    #[cfg(test)]
    pub fn render_packaged() -> String {
        let name = crate::distribution::name();
        render(name, name)
    }

    pub fn render_installed(exec: &str, try_exec: &str) -> String {
        with_marker(&render(exec, try_exec))
    }

    pub fn with_marker(body: &str) -> String {
        let sum = fingerprint(&location::without_marker(body));
        let marker = format!("{}=true\n{}={}\n", MARKER_KEY, FINGERPRINT_KEY, sum);
        match body.find("\n\n[") {
            Some(end_of_main_group) => {
                let (main, rest) = body.split_at(end_of_main_group + 1);
                format!("{}{}{}", main, marker, rest)
            },
            None => format!("{}{}", body, marker),
        }
    }

    pub fn exec_argument(path: &str) -> String {
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

    pub fn refresh(data_home: &Path) {
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
            Ok(status) => eprintln!(
                "{program} exited with {status}; a launcher may need a restart to notice the entry"
            ),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {},
            Err(e) => eprintln!(
                "{program} could not be run ({e}); a launcher may need a restart to notice the \
                 entry"
            ),
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use crate::desktop_integration::location::marker_intact;

        const SHIPPED_ASSET: &str = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../assets/dev.zellij.Zellij.desktop"
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

        fn icon_dir(size: &str) -> String {
            format!("icons/hicolor/{}/apps", size)
        }

        #[test]
        fn install_writes_an_entry_and_every_icon_size_under_the_given_data_home() {
            let dir = tempfile::TempDir::new().unwrap();
            let exe = PathBuf::from("/opt/zellij/bin/zellij");
            let installed = install_into(dir.path(), &exe).unwrap();

            assert_eq!(
                installed.entry,
                dir.path()
                    .join("applications")
                    .join("dev.zellij.Zellij.desktop")
            );
            assert_eq!(
                installed.icons,
                ["128x128", "256x256", "512x512"]
                    .iter()
                    .map(|size| dir
                        .path()
                        .join(icon_dir(size))
                        .join("dev.zellij.Zellij.png"))
                    .collect::<Vec<_>>()
            );

            let entry = std::fs::read_to_string(&installed.entry).unwrap();
            assert_eq!(entry.lines().next(), Some("[Desktop Entry]"));
            assert_eq!(value(&entry, "Type"), "Application");
            assert_eq!(value(&entry, "Name"), "Zellij");
            assert_eq!(value(&entry, "Terminal"), "false");
            assert_eq!(value(&entry, "Icon"), "dev.zellij.Zellij");
            assert_eq!(value(&entry, "Exec"), "/opt/zellij/bin/zellij terminal");
            assert_eq!(value(&entry, "TryExec"), "/opt/zellij/bin/zellij");
            assert_eq!(value(&entry, "Categories"), "System;TerminalEmulator;");
            assert_eq!(value(&entry, "X-TerminalArgExec"), "-e");
            assert_eq!(value(&entry, "X-TerminalArgDir"), "--working-directory=");
            assert_eq!(value(&entry, "X-TerminalArgHold"), "--hold");
            assert_eq!(value(&entry, MARKER_KEY), "true");
            assert!(marker_intact(&entry));
            assert_eq!(installed.fingerprint, value(&entry, FINGERPRINT_KEY));
            assert!(
                keys(&entry).iter().all(|(key, _)| !key.is_empty()),
                "the entry has an empty key:\n{entry}"
            );

            assert_eq!(std::fs::read(&installed.icons[0]).unwrap(), ICON_PNG);
            assert_eq!(std::fs::read(&installed.icons[1]).unwrap(), ICON_PNG_256);
            assert_eq!(std::fs::read(&installed.icons[2]).unwrap(), ICON_PNG_512);
        }

        #[test]
        fn the_entry_passes_desktop_file_validate_where_it_exists() {
            let dir = tempfile::TempDir::new().unwrap();
            let installed = install_into(dir.path(), Path::new("/opt/zellij/bin/zellij")).unwrap();
            let shipped = dir.path().join("dev.zellij.Zellij.desktop");
            std::fs::write(&shipped, SHIPPED_ASSET).unwrap();
            for entry in [&installed.entry, &shipped] {
                let outcome = Command::new("desktop-file-validate").arg(entry).output();
                match outcome {
                    Ok(output) => assert!(
                        output.status.success(),
                        "desktop-file-validate refused {}: {}{}",
                        entry.display(),
                        String::from_utf8_lossy(&output.stdout),
                        String::from_utf8_lossy(&output.stderr)
                    ),
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {},
                    Err(e) => panic!("desktop-file-validate could not be run: {e}"),
                }
            }
        }

        #[test]
        fn the_names_agree() {
            let dir = tempfile::TempDir::new().unwrap();
            let installed = install_into(dir.path(), Path::new("/opt/zellij/bin/zellij")).unwrap();
            let entry = std::fs::read_to_string(&installed.entry).unwrap();

            let app_id = application_id();
            assert_eq!(app_id, "dev.zellij.Zellij");
            assert_eq!(
                installed.entry.file_stem().unwrap().to_str().unwrap(),
                app_id
            );
            assert_eq!(value(&entry, "Icon"), app_id);
            for icon in &installed.icons {
                assert_eq!(icon.file_stem().unwrap().to_str().unwrap(), app_id);
            }
            assert_eq!(value(&entry, "StartupWMClass"), app_id);
        }

        #[test]
        fn uninstall_removes_what_install_wrote_and_nothing_else() {
            let dir = tempfile::TempDir::new().unwrap();
            let installed = install_into(dir.path(), Path::new("/opt/zellij/bin/zellij")).unwrap();
            let decoy_entry = dir.path().join("applications").join("other.desktop");
            let decoy_icon = dir.path().join(icon_dir("128x128")).join("other.png");
            write(&decoy_entry, b"[Desktop Entry]\n").unwrap();
            write(&decoy_icon, b"not a png").unwrap();

            let removed = uninstall_from(dir.path()).unwrap();

            let mut expected = vec![installed.entry.clone()];
            expected.extend(installed.icons.clone());
            assert_eq!(removed, expected);
            assert!(!installed.entry.exists());
            assert!(installed.icons.iter().all(|icon| !icon.exists()));
            assert!(decoy_entry.exists());
            assert!(decoy_icon.exists());
        }

        #[test]
        fn a_launcher_zellij_did_not_write_is_never_replaced_or_removed() {
            let dir = tempfile::TempDir::new().unwrap();
            let entry = entry_path(dir.path());
            write(&entry, b"[Desktop Entry]\nName=Mine\n").unwrap();
            assert!(install_into(dir.path(), Path::new("/opt/zellij")).is_err());
            assert!(uninstall_from(dir.path()).unwrap().is_empty());
            assert_eq!(
                std::fs::read_to_string(&entry).unwrap(),
                "[Desktop Entry]\nName=Mine\n"
            );
        }

        #[test]
        fn installing_replaces_the_old_launcher_only_when_zellij_wrote_it() {
            let dir = tempfile::TempDir::new().unwrap();
            let old_entry = dir.path().join("applications").join("zellij.desktop");
            let old_icon = dir.path().join(icon_dir("128x128")).join("zellij.png");
            write(&old_entry, older_render("/opt/zellij", "zellij").as_bytes()).unwrap();
            write(&old_icon, ICON_PNG).unwrap();
            let installed = install_into(dir.path(), Path::new("/opt/zellij")).unwrap();
            assert_eq!(installed.removed, vec![old_entry.clone(), old_icon.clone()]);
            assert!(!old_entry.exists());

            write(
                &old_entry,
                b"[Desktop Entry]\nName=Hand written\nExec=zellij\n",
            )
            .unwrap();
            let installed = install_into(dir.path(), Path::new("/opt/zellij")).unwrap();
            assert!(installed.removed.is_empty());
            assert!(old_entry.exists());
        }

        #[test]
        fn a_second_install_overwrites_and_a_second_uninstall_removes_nothing() {
            let dir = tempfile::TempDir::new().unwrap();
            install_into(dir.path(), Path::new("/first/zellij")).unwrap();
            let installed = install_into(dir.path(), Path::new("/second/zellij")).unwrap();
            let entry = std::fs::read_to_string(&installed.entry).unwrap();
            assert_eq!(value(&entry, "Exec"), "/second/zellij terminal");
            assert_eq!(value(&entry, "TryExec"), "/second/zellij");

            assert_eq!(uninstall_from(dir.path()).unwrap().len(), 4);
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
        fn the_entry_offers_inbox_sessions_and_new_window_actions() {
            let entry = render("/opt/zellij", "/opt/zellij");
            assert_eq!(value(&entry, "Actions"), "inbox;sessions;new-window;");
            let section = |name: &str| {
                let header = format!("[Desktop Action {}]", name);
                let start = entry
                    .find(&header)
                    .unwrap_or_else(|| panic!("{header} is missing:\n{entry}"));
                entry[start + header.len()..]
                    .split("\n[")
                    .next()
                    .unwrap()
                    .to_owned()
            };
            assert_eq!(
                value(&section("inbox"), "Exec"),
                "/opt/zellij terminal --inbox"
            );
            assert_eq!(value(&section("inbox"), "Name"), "Show Inbox");
            assert_eq!(
                value(&section("sessions"), "Exec"),
                "/opt/zellij terminal --sessions"
            );
            assert_eq!(value(&section("new-window"), "Exec"), "/opt/zellij terminal");
        }

        #[test]
        fn the_marker_sits_in_the_main_group() {
            let entry = render_installed("/opt/zellij", "/opt/zellij");
            let main_group = entry.split("\n\n[").next().unwrap();
            assert!(main_group.contains(MARKER_KEY));
            assert!(main_group.contains(FINGERPRINT_KEY));
            assert!(marker_intact(&entry));
        }

        #[test]
        fn the_shipped_asset_is_the_packaged_entry_without_a_marker() {
            assert_eq!(SHIPPED_ASSET, render_packaged());
            assert!(!has_marker(SHIPPED_ASSET));
            assert_eq!(value(SHIPPED_ASSET, "Exec"), "zellij terminal");
            assert_eq!(value(SHIPPED_ASSET, "TryExec"), "zellij");
        }
    }
}

#[cfg(windows)]
pub(crate) mod start_menu {
    use std::path::{Path, PathBuf};

    use anyhow::{Context, Result};
    use windows::core::{Interface, HSTRING};
    use windows::Win32::Storage::EnhancedStorage::PKEY_AppUserModel_ID;
    use windows::Win32::System::Com::{CoCreateInstance, IPersistFile, CLSCTX_INPROC_SERVER};
    use windows::Win32::UI::Shell::PropertiesSystem::IPropertyStore;
    use windows::Win32::UI::Shell::{IShellLinkW, ShellLink};

    use crate::desktop_integration::com::set_string;
    use crate::desktop_integration::terminal_command::TerminalCommand;

    const DESCRIPTION: &str = "Manage Your Terminal Applications";

    pub fn shortcut_path(dir: &Path) -> PathBuf {
        dir.join(format!("{}.lnk", crate::distribution::display_name()))
    }

    pub fn install_into(dir: &Path, command: &TerminalCommand, start_in: &Path) -> Result<PathBuf> {
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
            set_string(
                &store,
                &PKEY_AppUserModel_ID,
                &crate::desktop_integration::windows_app_id(),
            )?;
            store.Commit()?;
            link.cast::<IPersistFile>()?
                .Save(&HSTRING::from(path.as_path()), true)
                .with_context(|| format!("failed to write {:?}", path))?;
        }
        Ok(path)
    }

    pub fn uninstall_from(dir: &Path) -> Result<Option<PathBuf>> {
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
        use crate::desktop_integration::com::{get_string, Com};
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

        fn launcher_in(dir: &Path) -> TerminalCommand {
            TerminalCommand {
                program: dir.join("zellij-terminal.exe"),
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
                shortcut.app_id, "zellij.terminal",
                "the shortcut and the window it opens must share one taskbar button"
            );
        }

        #[test]
        fn without_a_launcher_the_shortcut_runs_the_terminal_verb() {
            let _com = Com::initialize().unwrap();
            let dir = tempfile::TempDir::new().unwrap();
            let command = TerminalCommand {
                program: dir.path().join("zellij.exe"),
                arguments: "terminal",
            };
            let path = install_into(dir.path(), &command, dir.path()).unwrap();
            let shortcut = read(&path);
            assert_eq!(shortcut.target, command.program.to_string_lossy());
            assert_eq!(shortcut.arguments, "terminal");
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
                second.join("zellij-terminal.exe").to_string_lossy()
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
