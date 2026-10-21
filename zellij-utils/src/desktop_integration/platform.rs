use std::path::{Path, PathBuf};

use crate::desktop_integration::{
    handle_state_request, load, location, update, Environment, IntegrationState, Item,
    ItemStatus, Outcome, Owner, Platform, RepairAnswer, Request, Response, Status,
};

#[cfg(all(unix, not(target_os = "macos"), not(target_os = "android")))]
use linux as platform;
#[cfg(not(any(windows, all(unix, not(target_os = "macos"), not(target_os = "android")))))]
use unsupported as platform;
#[cfg(windows)]
use windows_desktop as platform;

pub fn handle(request: &str) -> String {
    let response = match Request::from_json(request) {
        Err(e) => Response::Error(format!("The request could not be read: {}", e)),
        Ok(Request::Status) => Response::Status(status()),
        Ok(Request::Apply {
            install,
            remove,
            overwrite_binary,
        }) => Response::Applied(apply(&install, &remove, overwrite_binary)),
        Ok(Request::Repair(answer)) => repair(answer),
        Ok(other) => handle_state_request(&other),
    };
    response.to_json()
}

pub fn status() -> Status {
    let state = load();
    let environment = environment();
    let items = platform::items(&environment, &state);
    let mut notes = vec![];
    if environment.tiling_wm && environment.platform == Platform::Linux {
        notes.push(
            "Bind these commands to keys in your window manager (for sway or i3: \
             bindsym $mod+i exec zellij window --inbox):"
                .to_owned(),
        );
        for (command, what) in crate::desktop_integration::tiling_wm_commands() {
            notes.push(format!("  {} - {}", command, what));
        }
    }
    let repair = platform::repair_offer(&state);
    let status = Status {
        environment,
        items,
        notes,
        repair,
        state,
    };
    status
}

pub fn should_offer_setup(first_run: bool) -> bool {
    if load().setup_resolved() {
        return false;
    }
    if first_run {
        return true;
    }
    let status = status();
    Item::all()
        .into_iter()
        .any(|item| status.offered(item) && !status.item(item).map(|s| s.installed).unwrap_or(false))
}

pub fn apply(install: &[Item], remove: &[Item], overwrite_binary: bool) -> Vec<Outcome> {
    let environment = environment();
    let mut outcomes = vec![];
    for item in removal_order(remove) {
        outcomes.push(platform::remove(item, &environment));
    }
    for item in install_order(install) {
        outcomes.push(platform::install(item, &environment, overwrite_binary));
    }
    outcomes
}

fn install_order(items: &[Item]) -> Vec<Item> {
    [
        Item::CopyBinary,
        Item::Launcher,
        Item::StartMenu,
        Item::DefaultTerminal,
        Item::ExplorerItem,
    ]
    .into_iter()
    .filter(|item| items.contains(item))
    .collect()
}

fn removal_order(items: &[Item]) -> Vec<Item> {
    [
        Item::DefaultTerminal,
        Item::ExplorerItem,
        Item::Launcher,
        Item::StartMenu,
        Item::CopyBinary,
    ]
    .into_iter()
    .filter(|item| items.contains(item))
    .collect()
}

pub fn launcher_repair_offer() -> Option<crate::desktop_integration::RepairOffer> {
    platform::repair_offer(&load())
}

fn repair(answer: RepairAnswer) -> Response {
    let state = load();
    let Some(offer) = platform::repair_offer(&state) else {
        return Response::Done(state);
    };
    let recorded = update(|state: &mut IntegrationState| match answer {
        RepairAnswer::Yes => {},
        RepairAnswer::No => {
            if !state.repair.declined.contains(&offer.missing) {
                state.repair.declined.push(offer.missing.clone());
            }
        },
        RepairAnswer::Never => state.repair.never = true,
    });
    if let Err(e) = recorded {
        return Response::Error(format!("The answer could not be saved: {}", e));
    }
    if answer != RepairAnswer::Yes {
        return Response::Done(load());
    }
    let outcome = platform::install(Item::Launcher, &environment(), false);
    Response::Applied(vec![outcome])
}

pub fn outcome(item: Item, install: bool, result: anyhow::Result<String>) -> Outcome {
    match result {
        Ok(message) => Outcome {
            item,
            install,
            succeeded: true,
            message,
        },
        Err(e) => Outcome {
            item,
            install,
            succeeded: false,
            message: format!("{:#}", e),
        },
    }
}

pub fn environment() -> Environment {
    let binary = std::env::current_exe().ok();
    let mut environment = Environment {
        platform: if cfg!(windows) {
            Platform::Windows
        } else if cfg!(target_os = "macos") {
            Platform::MacOs
        } else if cfg!(target_os = "linux") || cfg!(target_family = "unix") {
            Platform::Linux
        } else {
            Platform::Other
        },
        desktop: std::env::var("XDG_CURRENT_DESKTOP")
            .ok()
            .filter(|d| !d.is_empty()),
        binary: binary.clone(),
        ..Default::default()
    };
    platform::describe(&mut environment);
    let forced = crate::desktop_integration::desktop_override(
        std::env::var(crate::desktop_integration::DESKTOP_OVERRIDE_ENV)
            .ok()
            .as_deref(),
    );
    match forced {
        Some(crate::desktop_integration::DesktopOverride::On) => {
            environment.container = None;
            environment.wsl = false;
            environment.no_desktop = None;
        },
        Some(crate::desktop_integration::DesktopOverride::Off) => {
            environment.container = None;
            environment.wsl = false;
            environment.no_desktop = Some(format!(
                "Turned off by {}",
                crate::desktop_integration::DESKTOP_OVERRIDE_ENV
            ));
        },
        None => {},
    }
    environment.tiling_wm = environment.platform == Platform::Linux
        && is_tiling_wm(environment.desktop.as_deref());
    environment
}

pub fn is_tiling_wm(desktop: Option<&str>) -> bool {
    match desktop {
        None => true,
        Some(desktop) => {
            let desktop = desktop.to_ascii_lowercase();
            ["sway", "hyprland", "i3", "bspwm", "niri", "river", "qtile", "dwm"]
                .iter()
                .any(|wm| desktop.split(':').any(|part| part == *wm))
        },
    }
}

pub fn owned_status(item: Item, owner: Owner, state_text: impl Into<String>) -> ItemStatus {
    ItemStatus {
        item,
        available: true,
        installed: owner != Owner::Nobody,
        changeable: matches!(owner, Owner::Nobody | Owner::Zellij),
        owner,
        state_text: state_text.into(),
        hint: None,
        instructions: vec![],
        upgrade: None,
        target: None,
        removes: vec![],
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CopyState {
    Permanent(String),
    Copied,
    Update(crate::desktop_integration::Upgrade),
    Occupied,
    Fresh,
}

pub fn permanent_reason(environment: &Environment) -> Option<String> {
    let running = environment.binary.as_deref()?;
    if let Some(manager) = &environment.package_manager {
        return Some(format!("Installed by {} at {}", manager, running.display()));
    }
    location::system_folder_for(
        &std::fs::canonicalize(running).unwrap_or_else(|_| running.to_path_buf()),
    )
    .map(|folder| format!("Installed in {} at {}", folder, running.display()))
}

pub fn copy_state(environment: &Environment, destination: &Path) -> CopyState {
    let Some(running) = environment.binary.as_deref() else {
        return CopyState::Fresh;
    };
    if location::same_file(running, destination) {
        return CopyState::Permanent(format!("Zellij is running from {}", destination.display()));
    }
    if let Some(reason) = permanent_reason(environment) {
        return CopyState::Permanent(reason);
    }
    if !destination.is_file() {
        return CopyState::Fresh;
    }
    if location::same_contents(running, destination) {
        return CopyState::Copied;
    }
    match upgrade_offer(destination) {
        Some(upgrade) => CopyState::Update(upgrade),
        None => CopyState::Occupied,
    }
}

pub fn entry_binary(
    environment: &Environment,
    destination: Option<&Path>,
    state: &IntegrationState,
) -> anyhow::Result<PathBuf> {
    use anyhow::Context;
    let running = environment
        .binary
        .clone()
        .context("failed to locate the running binary")?;
    let running_from_destination = destination
        .map(|destination| location::same_file(&running, destination))
        .unwrap_or(false);
    if running_from_destination || permanent_reason(environment).is_some() {
        return Ok(location::stable_binary_path(&running));
    }
    if let Some(copy) = recorded_copy_destination(state) {
        return Ok(copy);
    }
    if let Some(destination) = destination.filter(|d| location::same_contents(&running, d)) {
        return Ok(destination.to_path_buf());
    }
    anyhow::bail!(
        "Zellij is not in a permanent folder yet, so it would run {}. Copy Zellij to a \
         permanent folder first.",
        running.display()
    )
}

fn folder_is_writable(folder: &Path) -> bool {
    let probe = folder.join(format!(".zellij-write-check-{}", std::process::id()));
    let writable = std::fs::write(&probe, b"").is_ok();
    let _ = std::fs::remove_file(&probe);
    writable
}

pub fn upgrade_offer(existing: &Path) -> Option<crate::desktop_integration::Upgrade> {
    use crate::desktop_integration::{version_number, Upgrade};
    if location::package_manager_of(existing).is_some() {
        return None;
    }
    let installed = version_of(existing)?;
    let installed_number = version_number(&installed)?;
    let running = crate::consts::VERSION;
    if version_number(running)? <= installed_number {
        return None;
    }
    if !folder_is_writable(existing.parent()?) {
        return None;
    }
    Some(Upgrade {
        path: existing.to_path_buf(),
        from: installed.split_whitespace().last().unwrap_or(&installed).to_owned(),
        to: running.to_owned(),
    })
}

pub fn version_of(binary: &Path) -> Option<String> {
    let output = std::process::Command::new(binary)
        .arg("--version")
        .stdin(std::process::Stdio::null())
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    (!text.is_empty()).then_some(text)
}

pub fn copy_binary(
    source: &Path,
    destination: &Path,
    overwrite: bool,
) -> anyhow::Result<Option<String>> {
    use anyhow::Context;
    if location::same_file(source, destination) {
        return Ok(None);
    }
    let bytes = std::fs::read(source).with_context(|| format!("failed to read {:?}", source))?;
    if destination.exists() {
        let existing = std::fs::read(destination).unwrap_or_default();
        if existing == bytes {
            return Ok(Some(crate::desktop_integration::fingerprint_bytes(
                &bytes,
            )));
        }
        if !overwrite {
            anyhow::bail!(
                "{} already exists ({}). This copy is {}. Confirm to replace it.",
                destination.display(),
                version_of(destination).unwrap_or_else(|| "unknown version".to_owned()),
                version_of(source).unwrap_or_else(|| "an unknown version".to_owned())
            );
        }
    }
    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {:?}", parent))?;
    }
    let temporary = destination.with_extension(format!("new-{}", std::process::id()));
    std::fs::write(&temporary, &bytes)
        .with_context(|| format!("failed to write {:?}", temporary))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&temporary, std::fs::Permissions::from_mode(0o755))
            .with_context(|| format!("failed to make {:?} runnable", temporary))?;
    }
    std::fs::rename(&temporary, destination).inspect_err(|_| {
        let _ = std::fs::remove_file(&temporary);
    })?;
    Ok(Some(crate::desktop_integration::fingerprint_bytes(
        &bytes,
    )))
}

pub fn remove_recorded_copy(state: &IntegrationState) -> anyhow::Result<String> {
    let Some(record) = &state.binary_copy else {
        return Ok("No copy made by Zellij is recorded, so nothing was removed.".to_owned());
    };
    if !record.created {
        return Ok(format!(
            "{} was already there before Zellij copied itself, so it was left alone.",
            record.destination.display()
        ));
    }
    let current = std::fs::read(&record.destination).ok();
    let matches = current
        .as_deref()
        .map(crate::desktop_integration::fingerprint_bytes)
        .map(|sum| sum == record.fingerprint)
        .unwrap_or(false);
    if !matches {
        return Ok(format!(
            "{} changed since Zellij copied itself there, so it was left alone.",
            record.destination.display()
        ));
    }
    if std::env::current_exe()
        .map(|exe| location::same_file(&exe, &record.destination))
        .unwrap_or(false)
    {
        anyhow::bail!(
            "{} is the copy running right now, so it cannot be removed from here.",
            record.destination.display()
        );
    }
    std::fs::remove_file(&record.destination)?;
    let _ = update(|state: &mut IntegrationState| state.binary_copy = None);
    Ok(format!("Removed {}", record.destination.display()))
}

pub fn recorded_copy_destination(state: &IntegrationState) -> Option<PathBuf> {
    state
        .binary_copy
        .as_ref()
        .map(|record| record.destination.clone())
        .filter(|destination| destination.is_file())
}

#[cfg(all(unix, not(target_os = "macos"), not(target_os = "android")))]
mod linux {
    use std::collections::BTreeMap;
    use std::io::Read;
    use std::path::{Path, PathBuf};
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

    use anyhow::{bail, Context, Result};
    use crate::desktop_integration::location::{
        self, has_marker, marker_intact, place_note, place_of,
    };
    use crate::desktop_integration::{
        load, update, BinaryCopyRecord, Environment, IntegrationState,
        Item, ItemStatus, Outcome, Owner, RepairOffer,
    };

    use super::{copy_binary, outcome, owned_status};
    use crate::desktop_integration::desktop_entry::xdg;

    pub fn describe(environment: &mut Environment) {
        environment.container = container();
        environment.wsl = is_wsl();
        environment.no_desktop = no_desktop();
        if let Some(binary) = environment.binary.clone() {
            environment.package_manager = location::package_manager_of(&binary);
            environment.binary_permanent = location::system_install_of(&binary).is_some()
                || copy_destination()
                    .map(|destination| location::same_file(&binary, &destination))
                    .unwrap_or(false);
            if !environment.binary_permanent {
                let place = place_of(
                    &binary,
                    location::downloads_dir().as_deref(),
                    &location::temporary_dirs(),
                );
                environment.binary_location_note = Some(place_note(place));
            }
        }
    }

    fn container() -> Option<String> {
        let env = |key: &str| std::env::var_os(key).filter(|v| !v.is_empty());
        if env("DISTROBOX_ENTER_PATH").is_some() || env("CONTAINER_ID").is_some() {
            return Some("Distrobox".to_owned());
        }
        if env("TOOLBOX_PATH").is_some() {
            return Some("Toolbox".to_owned());
        }
        if Path::new("/.flatpak-info").exists() {
            return Some("Flatpak".to_owned());
        }
        if Path::new("/run/.containerenv").exists() {
            return Some("Podman".to_owned());
        }
        if Path::new("/.dockerenv").exists() {
            return Some("Docker".to_owned());
        }
        if let Some(name) = std::env::var("container").ok().filter(|v| !v.is_empty()) {
            return Some(name);
        }
        detect_virt_container()
    }

    fn detect_virt_container() -> Option<String> {
        let mut child = Command::new("systemd-detect-virt")
            .arg("--container")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;
        let deadline = Instant::now() + Duration::from_millis(500);
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(20))
                },
                _ => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return None;
                },
            }
        };
        let mut output = String::new();
        child.stdout.take()?.read_to_string(&mut output).ok()?;
        let output = output.trim();
        (status.success() && !output.is_empty() && output != "none").then(|| output.to_owned())
    }

    fn no_desktop() -> Option<String> {
        let set = |key: &str| std::env::var_os(key).filter(|v| !v.is_empty()).is_some();
        if set("SSH_CONNECTION") || set("SSH_CLIENT") || set("SSH_TTY") {
            return Some("Not available over SSH".to_owned());
        }
        if !set("DISPLAY") && !set("WAYLAND_DISPLAY") {
            return Some("No desktop was found".to_owned());
        }
        None
    }

    fn is_wsl() -> bool {
        std::env::var_os("WSL_DISTRO_NAME").is_some()
            || std::fs::read_to_string("/proc/version")
                .map(|v| v.to_ascii_lowercase().contains("microsoft"))
                .unwrap_or(false)
    }

    fn home() -> Option<PathBuf> {
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .filter(|h| h.is_absolute())
    }

    fn config_home() -> Option<PathBuf> {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .or_else(|| home().map(|h| h.join(".config")))
    }

    fn copy_destination() -> Option<PathBuf> {
        home().map(|h| {
            h.join(".local")
                .join("bin")
                .join(crate::distribution::name())
        })
    }

    fn hidden_inside(environment: &Environment, item: Item) -> Option<ItemStatus> {
        if let Some(container) = &environment.container {
            let mut instructions = vec![format!(
                "Zellij is running inside {}, so a launcher written here would point at a path \
                 the host cannot see.",
                container
            )];
            if matches!(container.as_str(), "Distrobox" | "Toolbox") {
                instructions.push(
                    "Run `distrobox-export --app zellij` inside the container to add Zellij to \
                     the host's app menu."
                        .to_owned(),
                );
            }
            let status = ItemStatus::unavailable(item, format!("Not available inside {}", container));
            return Some(if item == Item::Launcher {
                status.with_instructions(instructions)
            } else {
                status
            });
        }
        if environment.wsl {
            let status = ItemStatus::unavailable(item, "Not available inside WSL");
            return Some(if item == Item::Launcher {
                status.with_instructions(vec![
                    "Launchers inside WSL go through WSLg. The Windows build of Zellij provides \
                     the desktop app on Windows."
                        .to_owned(),
                ])
            } else {
                status
            });
        }
        if let Some(reason) = &environment.no_desktop {
            return Some(ItemStatus::unavailable(item, reason.clone()));
        }
        None
    }

    pub fn items(environment: &Environment, state: &IntegrationState) -> Vec<ItemStatus> {
        let launcher = launcher_status(environment);
        let default_terminal = default_terminal_status(environment, &launcher);
        vec![
            copy_binary_status(environment, state),
            launcher,
            default_terminal,
            ItemStatus::unavailable(Item::StartMenu, "Windows only"),
            ItemStatus::unavailable(Item::ExplorerItem, "Windows only"),
        ]
    }

    fn launcher_status(environment: &Environment) -> ItemStatus {
        if let Some(hidden) = hidden_inside(environment, Item::Launcher) {
            return hidden;
        }
        let tiling_instructions = || -> Vec<String> {
            if !environment.tiling_wm {
                return vec![];
            }
            crate::desktop_integration::tiling_wm_commands()
                .iter()
                .map(|(command, what)| format!("{} - {}", command, what))
                .collect()
        };
        let hint = "Right-click it for Show Inbox and Show Sessions.";
        let user_entry = location::data_home().map(|home| location::user_launcher_path(&home));
        if let Some(text) = user_entry
            .as_ref()
            .and_then(|path| std::fs::read_to_string(path).ok())
        {
            let status = if marker_intact(&text) {
                owned_status(Item::Launcher, Owner::Zellij, "Installed by Zellij")
                    .with_removes(xdg::files_to_remove(&location::data_home().unwrap_or_default()))
            } else if has_marker(&text) {
                owned_status(
                    Item::Launcher,
                    Owner::Someone,
                    "Installed by Zellij and edited since, so it is left alone",
                )
            } else {
                owned_status(
                    Item::Launcher,
                    Owner::Someone,
                    "Installed, but not by Zellij, so it is left alone",
                )
            };
            return status
                .with_hint(hint)
                .with_instructions(tiling_instructions());
        }
        if location::system_launcher().is_some() {
            return owned_status(
                Item::Launcher,
                Owner::PackageManager,
                "Installed by your package manager",
            )
            .with_hint(hint)
            .with_instructions(tiling_instructions());
        }
        if let Some(manager) = &environment.package_manager {
            return owned_status(
                Item::Launcher,
                Owner::Nobody,
                format!("Zellij was installed by {}, which did not add a launcher", manager),
            )
            .with_hint(hint)
            .with_instructions(tiling_instructions());
        }
        owned_status(Item::Launcher, Owner::Nobody, "Not installed")
            .with_hint(hint)
            .with_instructions(tiling_instructions())
    }

    fn copy_binary_status(environment: &Environment, _state: &IntegrationState) -> ItemStatus {
        if environment.container.is_some() {
            return ItemStatus::unavailable(Item::CopyBinary, "Not needed inside a container");
        }
        let Some(destination) = copy_destination() else {
            return ItemStatus::unavailable(Item::CopyBinary, "There is no home folder");
        };
        let mut text = match super::copy_state(environment, &destination) {
            super::CopyState::Permanent(reason) => {
                return ItemStatus::already(Item::CopyBinary, reason);
            },
            super::CopyState::Copied => {
                return ItemStatus::already(
                    Item::CopyBinary,
                    format!("Copied to {}", destination.display()),
                );
            },
            super::CopyState::Update(upgrade) => {
                return owned_status(
                    Item::CopyBinary,
                    Owner::Nobody,
                    format!("{} is version {}", destination.display(), upgrade.from),
                )
                .with_upgrade(upgrade)
                .with_target(destination);
            },
            super::CopyState::Occupied => format!(
                "A different file is already at {}, replacing it asks first.",
                destination.display()
            ),
            super::CopyState::Fresh => String::new(),
        };
        text = environment
            .binary_location_note
            .clone()
            .map(|note| format!("{} {}", note, text))
            .unwrap_or(text);
        text.push_str(&format!(" It would be copied to {}.", destination.display()));
        if let Some(dir) = destination.parent() {
            if !location::path_contains(dir) {
                text.push_str(&format!(
                    " {} is not on your PATH, so add it to run `zellij` from a terminal.",
                    dir.display()
                ));
            }
        }
        owned_status(Item::CopyBinary, Owner::Nobody, text.trim().to_owned())
            .with_target(destination)
    }

    fn default_terminal_status(
        environment: &Environment,
        launcher: &ItemStatus,
    ) -> ItemStatus {
        if let Some(hidden) = hidden_inside(environment, Item::DefaultTerminal) {
            return hidden;
        }
        let Some(method) = Method::detect(environment.desktop.as_deref()) else {
            return ItemStatus::unavailable(
                Item::DefaultTerminal,
                "Your desktop has no terminal setting Zellij can change safely",
            )
            .with_instructions(manual_default_terminal_instructions(environment));
        };
        if !launcher.installed && !(launcher.available && launcher.changeable) {
            return ItemStatus::unavailable(Item::DefaultTerminal, "Needs the launcher entry");
        }
        let current = method.current();
        if method.is_zellij(&current) {
            return ItemStatus::already(Item::DefaultTerminal, "Zellij is the default terminal");
        }
        let text = format!("Current default: {}", method.describe(&current));
        let mut status = owned_status(Item::DefaultTerminal, Owner::Nobody, text);
        if !launcher.installed {
            status = status.with_hint("Needs the launcher entry");
        }
        status
    }

    fn manual_default_terminal_instructions(environment: &Environment) -> Vec<String> {
        let mut lines = vec![];
        if environment.tiling_wm {
            lines.push(
                "Bind `zellij window` to the key you use to open a terminal.".to_owned(),
            );
        }
        lines.push(
            "On Debian and Ubuntu, `sudo update-alternatives --config x-terminal-emulator` \
             chooses the default terminal (it needs admin rights, so Zellij does not run it)."
                .to_owned(),
        );
        lines.push(
            "If your desktop uses xdg-terminal-exec, installing it lets Zellij set this for you."
                .to_owned(),
        );
        lines
    }

    pub fn repair_offer(state: &IntegrationState) -> Option<RepairOffer> {
        let target = super::entry_binary(&super::environment(), copy_destination().as_deref(), state)
            .ok()?;
        location::repair_offer(state, &target)
    }

    pub fn install(item: Item, environment: &Environment, overwrite_binary: bool) -> Outcome {
        let result = match item {
            Item::Launcher => install_launcher(environment),
            Item::CopyBinary => install_copy(overwrite_binary),
            Item::DefaultTerminal => install_default_terminal(environment),
            Item::StartMenu | Item::ExplorerItem => Err(anyhow::anyhow!("Only on Windows")),
        };
        outcome(item, true, result)
    }

    pub fn remove(item: Item, _environment: &Environment) -> Outcome {
        let result = match item {
            Item::Launcher => remove_launcher(),
            Item::CopyBinary => super::remove_recorded_copy(&load()),
            Item::DefaultTerminal => Err(anyhow::anyhow!(
                "Zellij does not change the default terminal back"
            )),
            Item::StartMenu | Item::ExplorerItem => Err(anyhow::anyhow!("Only on Windows")),
        };
        outcome(item, false, result)
    }

    fn launcher_binary() -> Result<PathBuf> {
        super::entry_binary(&super::environment(), copy_destination().as_deref(), &load())
    }

    fn install_launcher(environment: &Environment) -> Result<String> {
        if environment.container.is_some() || environment.wsl {
            bail!("A launcher cannot be installed from here");
        }
        let data_home = xdg::data_home()?;
        let binary = launcher_binary()?;
        let installed = xdg::install_into(&data_home, &binary)?;
        xdg::refresh(&data_home);
        let record = installed.record();
        let icons = installed.icons.clone();
        update(move |state: &mut IntegrationState| {
            state.launcher = Some(record);
            state.icons = icons;
        })?;
        let mut lines = vec![format!("Wrote {}", installed.entry.display())];
        lines.extend(installed.icons.iter().map(|i| format!("Wrote {}", i.display())));
        lines.extend(
            installed
                .removed
                .iter()
                .map(|r| format!("Removed {}", r.display())),
        );
        lines.push(format!("It runs {}", binary.display()));
        Ok(lines.join("\n"))
    }

    fn remove_launcher() -> Result<String> {
        let data_home = xdg::data_home()?;
        let removed = xdg::uninstall_from(&data_home)?;
        xdg::refresh(&data_home);
        update(|state: &mut IntegrationState| {
            state.launcher = None;
            state.icons.clear();
        })?;
        if removed.is_empty() {
            return Ok("Nothing written by Zellij was found".to_owned());
        }
        Ok(removed
            .iter()
            .map(|r| format!("Removed {}", r.display()))
            .collect::<Vec<_>>()
            .join("\n"))
    }

    fn install_copy(overwrite: bool) -> Result<String> {
        let source = std::env::current_exe().context("failed to locate the running binary")?;
        let destination = copy_destination().context("there is no home folder")?;
        let upgrade = match super::copy_state(&super::environment(), &destination) {
            super::CopyState::Update(upgrade) => Some(upgrade),
            _ => None,
        };
        let overwrite = overwrite || upgrade.is_some();
        let existed = destination.exists();
        let fingerprint = copy_binary(&source, &destination, overwrite)?;
        let Some(fingerprint) = fingerprint else {
            return Ok(format!("{} is this copy already", destination.display()));
        };
        let record = BinaryCopyRecord {
            destination: destination.clone(),
            created: !existed || (overwrite && upgrade.is_none()),
            fingerprint,
        };
        update(move |state: &mut IntegrationState| state.binary_copy = Some(record))?;
        let mut message = format!("Wrote {}", destination.display());
        if let Some(upgrade) = &upgrade {
            message.push_str(&format!(
                "\nUpdated Zellij from {} to {}",
                upgrade.from, upgrade.to
            ));
        }
        if let Some(dir) = destination.parent() {
            if !location::path_contains(dir) {
                message.push_str(&format!(
                    "\n{} is not on your PATH yet; add it to run `zellij` from a terminal",
                    dir.display()
                ));
            }
        }
        let launcher = location::data_home().map(|h| location::user_launcher_path(&h));
        let ours = launcher
            .as_ref()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .map(|t| marker_intact(&t))
            .unwrap_or(false);
        if ours {
            if let Ok(data_home) = xdg::data_home() {
                if let Ok(installed) = xdg::install_into(&data_home, &destination) {
                    message.push_str(&format!("\nWrote {}", installed.entry.display()));
                    let record = installed.record();
                    let _ = update(move |state: &mut IntegrationState| {
                        state.launcher = Some(record)
                    });
                    message.push_str("\nThe launcher now runs the new copy");
                }
            }
        }
        Ok(message)
    }

    fn install_default_terminal(environment: &Environment) -> Result<String> {
        let method = Method::detect(environment.desktop.as_deref())
            .context("your desktop has no terminal setting Zellij can change safely")?;
        let binary = launcher_binary()?;
        let before = method.current();
        if method.is_zellij(&before) {
            return Ok("Zellij is already the default terminal".to_owned());
        }
        let wanted = method.wanted(&binary);
        method.write(&wanted)?;
        let mut lines = vec![];
        if let Some(file) = method.file() {
            lines.push(format!("Wrote {}", file.display()));
        }
        lines.push(format!(
            "Zellij is now the default terminal (was: {})",
            method.describe(&before)
        ));
        Ok(lines.join("\n"))
    }

    type Values = BTreeMap<String, Option<String>>;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum Method {
        XdgTerminalsList,
        Kde,
        Cinnamon,
        Mate,
    }

    impl Method {
        fn file(&self) -> Option<PathBuf> {
            match self {
                Method::XdgTerminalsList => terminals_list_path(),
                Method::Kde => config_home().map(|c| c.join("kdeglobals")),
                Method::Cinnamon | Method::Mate => None,
            }
        }

        pub fn detect(desktop: Option<&str>) -> Option<Method> {
            let desktop = desktop.unwrap_or_default().to_ascii_uppercase();
            let parts: Vec<&str> = desktop.split(':').collect();
            if parts.iter().any(|p| *p == "KDE") {
                return Some(Method::Kde);
            }
            if parts.iter().any(|p| p.contains("CINNAMON")) {
                return Some(Method::Cinnamon);
            }
            if parts.iter().any(|p| *p == "MATE") {
                return Some(Method::Mate);
            }
            location::find_on_path("xdg-terminal-exec").map(|_| Method::XdgTerminalsList)
        }

        fn schema(&self) -> Option<&'static str> {
            match self {
                Method::Cinnamon => Some("org.cinnamon.desktop.default-applications.terminal"),
                Method::Mate => Some("org.mate.applications-terminal"),
                _ => None,
            }
        }

        fn keys(&self) -> &'static [&'static str] {
            match self {
                Method::XdgTerminalsList => &["first"],
                Method::Kde => &["TerminalApplication", "TerminalService"],
                Method::Cinnamon | Method::Mate => &["exec", "exec-arg"],
            }
        }

        pub fn current(&self) -> Values {
            match self {
                Method::XdgTerminalsList => {
                    let file = terminals_list_path();
                    let first = file
                        .as_ref()
                        .and_then(|f| std::fs::read_to_string(f).ok())
                        .and_then(|text| first_terminal(&text));
                    BTreeMap::from([("first".to_owned(), first)])
                },
                Method::Kde => {
                    let text = config_home()
                        .and_then(|c| std::fs::read_to_string(c.join("kdeglobals")).ok())
                        .unwrap_or_default();
                    self.keys()
                        .iter()
                        .map(|key| (key.to_string(), ini_get(&text, "General", key)))
                        .collect()
                },
                Method::Cinnamon | Method::Mate => {
                    let schema = self.schema().unwrap_or_default();
                    self.keys()
                        .iter()
                        .map(|key| (key.to_string(), gsettings_get(schema, key)))
                        .collect()
                },
            }
        }

        pub fn wanted(&self, binary: &Path) -> BTreeMap<String, String> {
            let command = format!("{} window", binary.display());
            match self {
                Method::XdgTerminalsList => {
                    BTreeMap::from([("first".to_owned(), location::launcher_file_name())])
                },
                Method::Kde => BTreeMap::from([
                    ("TerminalApplication".to_owned(), command),
                    (
                        "TerminalService".to_owned(),
                        location::launcher_file_name(),
                    ),
                ]),
                Method::Cinnamon | Method::Mate => BTreeMap::from([
                    ("exec".to_owned(), command),
                    ("exec-arg".to_owned(), "-e".to_owned()),
                ]),
            }
        }

        pub fn is_zellij(&self, values: &Values) -> bool {
            let get = |key: &str| values.get(key).cloned().flatten().unwrap_or_default();
            match self {
                Method::XdgTerminalsList => get("first") == location::launcher_file_name(),
                Method::Kde => get("TerminalService") == location::launcher_file_name(),
                Method::Cinnamon | Method::Mate => {
                    let exec = get("exec");
                    exec.ends_with(" window")
                        && exec
                            .trim_end_matches(" window")
                            .ends_with(crate::distribution::name())
                },
            }
        }

        pub fn describe(&self, values: &Values) -> String {
            let get = |key: &str| values.get(key).cloned().flatten();
            match self {
                Method::XdgTerminalsList => get("first")
                    .map(|id| id.trim_end_matches(".desktop").to_owned())
                    .unwrap_or_else(|| "chosen automatically".to_owned()),
                Method::Kde => get("TerminalApplication").unwrap_or_else(|| "Konsole".to_owned()),
                Method::Cinnamon | Method::Mate => {
                    get("exec").unwrap_or_else(|| "the desktop's default".to_owned())
                },
            }
        }

        fn write(&self, wanted: &BTreeMap<String, String>) -> Result<()> {
            match self {
                Method::XdgTerminalsList => {
                    let file = terminals_list_path().context("there is no config folder")?;
                    let text = std::fs::read_to_string(&file).unwrap_or_default();
                    let id = location::launcher_file_name();
                    let mut lines: Vec<String> = text
                        .lines()
                        .filter(|line| line.trim() != id)
                        .map(str::to_owned)
                        .collect();
                    lines.insert(0, id);
                    xdg::write(&file, format!("{}\n", lines.join("\n")).as_bytes())
                },
                Method::Kde => {
                    for (key, value) in wanted {
                        kde_set(key, Some(value))?;
                    }
                    Ok(())
                },
                Method::Cinnamon | Method::Mate => {
                    let schema = self.schema().unwrap_or_default();
                    for (key, value) in wanted {
                        gsettings_set(schema, key, Some(value))?;
                    }
                    Ok(())
                },
            }
        }

    }

    fn terminals_list_path() -> Option<PathBuf> {
        let config = config_home()?;
        let desktops = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default();
        for desktop in desktops.split(':').filter(|d| !d.is_empty()) {
            let specific = config.join(format!(
                "{}-xdg-terminals.list",
                desktop.to_ascii_lowercase()
            ));
            if std::fs::read_to_string(&specific)
                .ok()
                .and_then(|text| first_terminal(&text))
                .is_some()
            {
                return Some(specific);
            }
        }
        Some(config.join("xdg-terminals.list"))
    }

    pub fn first_terminal(text: &str) -> Option<String> {
        text.lines()
            .map(str::trim)
            .find(|line| {
                !line.is_empty()
                    && !line.starts_with('#')
                    && !line.starts_with('/')
                    && !line.starts_with('-')
                    && !line.starts_with('+')
            })
            .map(str::to_owned)
    }

    pub fn ini_get(text: &str, group: &str, key: &str) -> Option<String> {
        let header = format!("[{}]", group);
        let mut in_group = false;
        for line in text.lines() {
            let line = line.trim();
            if line.starts_with('[') {
                in_group = line == header;
                continue;
            }
            if in_group {
                if let Some((k, v)) = line.split_once('=') {
                    if k.trim() == key {
                        return Some(v.trim().to_owned());
                    }
                }
            }
        }
        None
    }

    pub fn ini_set(text: &str, group: &str, key: &str, value: Option<&str>) -> String {
        let header = format!("[{}]", group);
        let mut lines: Vec<String> = text.lines().map(str::to_owned).collect();
        let group_start = lines.iter().position(|l| l.trim() == header);
        let Some(start) = group_start else {
            if let Some(value) = value {
                if !lines.is_empty() && !lines.last().map(|l| l.is_empty()).unwrap_or(true) {
                    lines.push(String::new());
                }
                lines.push(header);
                lines.push(format!("{}={}", key, value));
            }
            return format!("{}\n", lines.join("\n"));
        };
        let end = lines[start + 1..]
            .iter()
            .position(|l| l.trim().starts_with('['))
            .map(|p| p + start + 1)
            .unwrap_or(lines.len());
        let existing = (start + 1..end).find(|i| {
            lines[*i]
                .split_once('=')
                .map(|(k, _)| k.trim() == key)
                .unwrap_or(false)
        });
        match (existing, value) {
            (Some(i), Some(value)) => lines[i] = format!("{}={}", key, value),
            (Some(i), None) => {
                lines.remove(i);
            },
            (None, Some(value)) => {
                let mut insert_at = end;
                while insert_at > start + 1 && lines[insert_at - 1].trim().is_empty() {
                    insert_at -= 1;
                }
                lines.insert(insert_at, format!("{}={}", key, value));
            },
            (None, None) => {},
        }
        format!("{}\n", lines.join("\n"))
    }

    fn kde_set(key: &str, value: Option<&str>) -> Result<()> {
        for program in ["kwriteconfig6", "kwriteconfig5"] {
            if location::find_on_path(program).is_none() {
                continue;
            }
            let mut command = Command::new(program);
            command.args(["--file", "kdeglobals", "--group", "General", "--key", key]);
            match value {
                Some(value) => command.arg(value),
                None => command.arg("--delete"),
            };
            let status = command
                .stdin(Stdio::null())
                .status()
                .with_context(|| format!("{} could not be run", program))?;
            if !status.success() {
                bail!("{} exited with {}", program, status);
            }
            return Ok(());
        }
        let file = config_home()
            .context("there is no config folder")?
            .join("kdeglobals");
        let text = std::fs::read_to_string(&file).unwrap_or_default();
        xdg::write(&file, ini_set(&text, "General", key, value).as_bytes())
    }

    fn gsettings_get(schema: &str, key: &str) -> Option<String> {
        let output = Command::new("gsettings")
            .args(["get", schema, key])
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let text = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        let unquoted = text
            .strip_prefix('\'')
            .and_then(|t| t.strip_suffix('\''))
            .map(|t| t.replace("\\'", "'").replace("\\\\", "\\"))
            .unwrap_or(text);
        (!unquoted.is_empty()).then_some(unquoted)
    }

    fn gsettings_set(schema: &str, key: &str, value: Option<&str>) -> Result<()> {
        let mut command = Command::new("gsettings");
        match value {
            Some(value) => {
                let quoted = format!(
                    "'{}'",
                    value.replace('\\', "\\\\").replace('\'', "\\'")
                );
                command.args(["set", schema, key, &quoted]);
            },
            None => {
                command.args(["reset", schema, key]);
            },
        }
        let status = command
            .stdin(Stdio::null())
            .status()
            .context("gsettings could not be run")?;
        if !status.success() {
            bail!("gsettings exited with {}", status);
        }
        Ok(())
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn the_first_terminal_skips_comments_and_directives() {
            let text = "# mine\n/execarg_compat\n-foot.desktop\n+kitty.desktop\nalacritty.desktop\n";
            assert_eq!(first_terminal(text).as_deref(), Some("alacritty.desktop"));
            assert_eq!(first_terminal("# nothing\n"), None);
        }

        #[test]
        fn kdeglobals_keys_are_set_and_removed_without_touching_the_rest() {
            let text = "[General]\nfont=Noto\n\n[KDE]\nSingleClick=false\n";
            let set = ini_set(text, "General", "TerminalApplication", Some("/bin/zellij window"));
            assert_eq!(
                ini_get(&set, "General", "TerminalApplication").as_deref(),
                Some("/bin/zellij window")
            );
            assert_eq!(ini_get(&set, "General", "font").as_deref(), Some("Noto"));
            assert_eq!(ini_get(&set, "KDE", "SingleClick").as_deref(), Some("false"));
            let removed = ini_set(&set, "General", "TerminalApplication", None);
            assert_eq!(ini_get(&removed, "General", "TerminalApplication"), None);
            assert_eq!(ini_get(&removed, "General", "font").as_deref(), Some("Noto"));
            let fresh = ini_set("", "General", "TerminalService", Some("dev.zellij.Zellij.desktop"));
            assert_eq!(
                ini_get(&fresh, "General", "TerminalService").as_deref(),
                Some("dev.zellij.Zellij.desktop")
            );
        }

        #[test]
        fn desktops_map_to_the_setting_they_use() {
            assert_eq!(Method::detect(Some("KDE")), Some(Method::Kde));
            assert_eq!(Method::detect(Some("X-Cinnamon")), Some(Method::Cinnamon));
            assert_eq!(Method::detect(Some("MATE")), Some(Method::Mate));
        }

        #[test]
        fn zellij_is_recognised_as_the_default_for_each_method() {
            let id = location::launcher_file_name();
            let list = BTreeMap::from([("first".to_owned(), Some(id.clone()))]);
            assert!(Method::XdgTerminalsList.is_zellij(&list));
            let kde = BTreeMap::from([("TerminalService".to_owned(), Some(id))]);
            assert!(Method::Kde.is_zellij(&kde));
            let wanted = Method::Mate.wanted(Path::new("/home/me/.local/bin/zellij"));
            let values: Values = wanted.into_iter().map(|(k, v)| (k, Some(v))).collect();
            assert!(Method::Mate.is_zellij(&values));
            let other = BTreeMap::from([("exec".to_owned(), Some("mate-terminal".to_owned()))]);
            assert!(!Method::Mate.is_zellij(&other));
        }
    }
}

#[cfg(windows)]
mod windows_desktop {
    use std::path::{Path, PathBuf};

    use anyhow::{bail, Context, Result};
    use windows::Win32::UI::Shell::{FOLDERID_Downloads, FOLDERID_LocalAppData, FOLDERID_Programs};
    use crate::desktop_integration::location::{self, place_note, Place};
    use crate::desktop_integration::{
        load, update, BinaryCopyRecord, Environment, IntegrationState, Item, ItemStatus, Outcome,
        Owner, RepairOffer,
    };

    use super::{copy_binary, outcome, owned_status};
    use crate::desktop_integration::com::{known_folder, Com};
    use crate::desktop_integration::desktop_entry::start_menu;
    use crate::desktop_integration::window_command::window_command;

    const EXPLORER_KEYS: [&str; 2] = [
        r"Software\Classes\Directory\shell\Zellij",
        r"Software\Classes\Directory\Background\shell\Zellij",
    ];
    const ZELLIJ_KEY: &str = r"Software\Zellij";

    pub fn describe(environment: &mut Environment) {
        let Some(binary) = environment.binary.clone() else {
            return;
        };
        if installed_by_msi(&binary) {
            environment.package_manager = Some("the Zellij installer".to_owned());
            environment.binary_permanent = true;
            return;
        }
        environment.binary_permanent = copy_destination()
            .map(|destination| location::same_file(&binary, &destination))
            .unwrap_or(false);
        if !environment.binary_permanent {
            let downloads = known_folder(&FOLDERID_Downloads).ok();
            let temporary = vec![std::env::temp_dir()];
            let place = location::place_of(&binary, downloads.as_deref(), &temporary);
            let place = if place == Place::Removable {
                Place::Elsewhere
            } else {
                place
            };
            environment.binary_location_note = Some(place_note(place));
        }
    }

    fn installed_by_msi(binary: &Path) -> bool {
        let Some(folder) = binary.parent() else {
            return false;
        };
        [registry::Root::CurrentUser, registry::Root::LocalMachine]
            .into_iter()
            .filter_map(|root| registry::get_string(root, ZELLIJ_KEY, "InstallPath"))
            .any(|path| location::same_file(Path::new(path.trim_end_matches('\\')), folder))
    }

    fn msi_value(name: &str) -> bool {
        [registry::Root::CurrentUser, registry::Root::LocalMachine]
            .into_iter()
            .any(|root| registry::get_dword(root, ZELLIJ_KEY, name) == Some(1))
    }

    fn copy_folder() -> Result<PathBuf> {
        Ok(known_folder(&FOLDERID_LocalAppData)?
            .join("Programs")
            .join("Zellij"))
    }

    fn copy_destination() -> Option<PathBuf> {
        copy_folder()
            .ok()
            .map(|folder| folder.join(format!("{}.exe", crate::distribution::name())))
    }

    pub fn items(environment: &Environment, state: &IntegrationState) -> Vec<ItemStatus> {
        vec![
            copy_binary_status(environment, state),
            start_menu_status(state),
            ItemStatus::unavailable(
                Item::DefaultTerminal,
                "Not offered on Windows; use \"Open Zellij here\" in Explorer instead",
            ),
            explorer_status(state),
            ItemStatus::unavailable(Item::Launcher, "Linux only"),
        ]
    }

    fn copy_binary_status(environment: &Environment, _state: &IntegrationState) -> ItemStatus {
        let target = copy_destination();
        if let Some(destination) = &target {
            match super::copy_state(environment, destination) {
                super::CopyState::Permanent(reason) => {
                    return ItemStatus::already(Item::CopyBinary, reason);
                },
                super::CopyState::Copied => {
                    return ItemStatus::already(
                        Item::CopyBinary,
                        format!("Copied to {}", destination.display()),
                    );
                },
                super::CopyState::Update(upgrade) => {
                    return owned_status(
                        Item::CopyBinary,
                        Owner::Nobody,
                        format!("{} is version {}", destination.display(), upgrade.from),
                    )
                    .with_upgrade(upgrade)
                    .with_target(destination.clone());
                },
                super::CopyState::Occupied | super::CopyState::Fresh => {},
            }
        }
        let destination = copy_folder()
            .map(|f| format!(" It would be copied to {}.", f.display()))
            .unwrap_or_default();
        let status = owned_status(
            Item::CopyBinary,
            Owner::Nobody,
            format!(
                "{}{}",
                environment.binary_location_note.clone().unwrap_or_default(),
                destination
            )
            .trim()
            .to_owned(),
        );
        match target {
            Some(target) => status.with_target(target),
            None => status,
        }
    }

    fn start_menu_status(state: &IntegrationState) -> ItemStatus {
        let hint = "Right-click the taskbar icon for Show Inbox and Show Sessions.";
        let Ok(programs) = known_folder(&FOLDERID_Programs) else {
            return ItemStatus::unavailable(Item::StartMenu, "The Start menu folder was not found");
        };
        let shortcut = start_menu::shortcut_path(&programs);
        if msi_value("StartMenuShortcut") {
            return owned_status(Item::StartMenu, Owner::PackageManager, "Added by the installer")
                .with_hint(hint);
        }
        if shortcut.exists() {
            let owner = if state.windows.start_menu.is_some() {
                Owner::Zellij
            } else {
                Owner::Someone
            };
            return owned_status(Item::StartMenu, owner, "In the Start menu")
                .with_hint(hint)
                .with_removes(vec![shortcut.display().to_string()]);
        }
        owned_status(Item::StartMenu, Owner::Nobody, "Not in the Start menu").with_hint(hint)
    }

    fn explorer_status(state: &IntegrationState) -> ItemStatus {
        let present = registry::key_exists(registry::Root::CurrentUser, EXPLORER_KEYS[0]);
        if msi_value("ExplorerItem") {
            return owned_status(
                Item::ExplorerItem,
                Owner::PackageManager,
                "Added by the installer",
            );
        }
        if present {
            let owner = if state.windows.explorer_item {
                Owner::Zellij
            } else {
                Owner::Someone
            };
            return owned_status(Item::ExplorerItem, owner, "Right-click a folder to open Zellij there")
                .with_removes(
                    EXPLORER_KEYS
                        .iter()
                        .map(|key| format!(r"HKEY_CURRENT_USER\{}", key))
                        .collect(),
                );
        }
        owned_status(
            Item::ExplorerItem,
            Owner::Nobody,
            "Adds \"Open Zellij here\" when right-clicking a folder",
        )
    }

    pub fn repair_offer(_state: &IntegrationState) -> Option<RepairOffer> {
        None
    }

    pub fn install(item: Item, _environment: &Environment, overwrite_binary: bool) -> Outcome {
        let result = match item {
            Item::CopyBinary => install_copy(overwrite_binary),
            Item::StartMenu => install_start_menu(),
            Item::ExplorerItem => install_explorer_item(),
            _ => Err(anyhow::anyhow!("Not available on Windows")),
        };
        outcome(item, true, result)
    }

    pub fn remove(item: Item, _environment: &Environment) -> Outcome {
        let result = match item {
            Item::CopyBinary => remove_copy(),
            Item::StartMenu => remove_start_menu(),
            Item::ExplorerItem => remove_explorer_item(),
            _ => Err(anyhow::anyhow!("Not available on Windows")),
        };
        outcome(item, false, result)
    }

    fn current_binary() -> Result<PathBuf> {
        super::entry_binary(&super::environment(), copy_destination().as_deref(), &load())
    }

    fn install_copy(overwrite: bool) -> Result<String> {
        let source = std::env::current_exe().context("failed to locate the running binary")?;
        let folder = copy_folder()?;
        let name = source
            .file_name()
            .context("the running binary has no file name")?;
        let destination = folder.join(name);
        let overwrite = overwrite
            || matches!(
                super::copy_state(&super::environment(), &destination),
                super::CopyState::Update(_)
            );
        let existed = destination.exists();
        let launcher = window_command(&source);
        let mut lines = vec![];
        if launcher.uses_launcher() {
            let launcher_name = launcher
                .program
                .file_name()
                .context("the launcher has no file name")?;
            copy_binary(&launcher.program, &folder.join(launcher_name), overwrite)?;
            lines.push(format!("Wrote {}", folder.join(launcher_name).display()));
        }
        let Some(fingerprint) = copy_binary(&source, &destination, overwrite)? else {
            return Ok(format!("{} is this copy already", destination.display()));
        };
        lines.push(format!("Wrote {}", destination.display()));
        let record = BinaryCopyRecord {
            destination: destination.clone(),
            created: !existed || overwrite,
            fingerprint,
        };
        update(move |state: &mut IntegrationState| state.binary_copy = Some(record))?;
        if !location::path_contains(&folder) {
            lines.push(format!(
                "{} is not on your PATH; add it to run zellij from a terminal",
                folder.display()
            ));
        }
        Ok(lines.join("\n"))
    }

    fn remove_copy() -> Result<String> {
        let state = load();
        let message = super::remove_recorded_copy(&state)?;
        if let Some(record) = &state.binary_copy {
            if let Some(folder) = record.destination.parent() {
                let launcher = folder.join(format!(
                    "{}-window.exe",
                    crate::distribution::name()
                ));
                if !record.destination.exists() && launcher.exists() {
                    let _ = std::fs::remove_file(&launcher);
                }
            }
        }
        Ok(message)
    }

    fn install_start_menu() -> Result<String> {
        if msi_value("StartMenuShortcut") {
            return Ok("The installer already added Zellij to the Start menu".to_owned());
        }
        let _com = Com::initialize()?;
        let exe = current_binary()?;
        let command = window_command(&exe);
        let start_in = known_folder(&windows::Win32::UI::Shell::FOLDERID_Profile)?;
        let path = start_menu::install_into(&known_folder(&FOLDERID_Programs)?, &command, &start_in)?;
        let recorded = path.clone();
        update(move |state: &mut IntegrationState| state.windows.start_menu = Some(recorded))?;
        Ok(format!("Wrote {}", path.display()))
    }

    fn remove_start_menu() -> Result<String> {
        let state = load();
        if state.windows.start_menu.is_none() {
            return Ok("Zellij did not add the Start menu shortcut, so it was left alone".to_owned());
        }
        let _com = Com::initialize()?;
        let removed = start_menu::uninstall_from(&known_folder(&FOLDERID_Programs)?)?;
        update(|state: &mut IntegrationState| state.windows.start_menu = None)?;
        Ok(match removed {
            Some(path) => format!("Removed {}", path.display()),
            None => "Nothing to remove".to_owned(),
        })
    }

    fn install_explorer_item() -> Result<String> {
        if msi_value("ExplorerItem") {
            return Ok("The installer already added \"Open Zellij here\"".to_owned());
        }
        let exe = current_binary()?;
        let launcher = window_command(&exe);
        if !launcher.uses_launcher() {
            bail!(
                "zellij-window.exe was not found next to {}, so Explorer would open a console \
                 window alongside Zellij",
                exe.display()
            );
        }
        let program = launcher.program.display().to_string();
        for key in EXPLORER_KEYS {
            registry::set_string(registry::Root::CurrentUser, key, "", "Open Zellij here")?;
            registry::set_string(registry::Root::CurrentUser, key, "Icon", &program)?;
            registry::set_string(
                registry::Root::CurrentUser,
                &format!(r"{}\command", key),
                "",
                &format!("\"{}\" --working-directory \"%V\"", program),
            )?;
        }
        update(|state: &mut IntegrationState| state.windows.explorer_item = true)?;
        Ok("Added \"Open Zellij here\" to Explorer's folder menu".to_owned())
    }

    fn remove_explorer_item() -> Result<String> {
        if !load().windows.explorer_item {
            return Ok("Zellij did not add the Explorer item, so it was left alone".to_owned());
        }
        for key in EXPLORER_KEYS {
            registry::delete_tree(registry::Root::CurrentUser, key)?;
        }
        update(|state: &mut IntegrationState| state.windows.explorer_item = false)?;
        Ok("Removed \"Open Zellij here\" from Explorer".to_owned())
    }

    pub mod registry {
        use anyhow::{bail, Result};
        use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
        use windows_sys::Win32::System::Registry::{
            RegCloseKey, RegCreateKeyExW, RegDeleteTreeW, RegGetValueW, RegOpenKeyExW,
            RegSetValueExW, HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_SET_VALUE,
            REG_OPTION_NON_VOLATILE, REG_SZ, RRF_RT_REG_DWORD, RRF_RT_REG_SZ,
        };

        #[derive(Clone, Copy)]
        pub enum Root {
            CurrentUser,
            LocalMachine,
        }

        fn root(root: Root) -> HKEY {
            match root {
                Root::CurrentUser => HKEY_CURRENT_USER,
                Root::LocalMachine => HKEY_LOCAL_MACHINE,
            }
        }

        fn wide(text: &str) -> Vec<u16> {
            text.encode_utf16().chain(std::iter::once(0)).collect()
        }

        pub fn key_exists(at: Root, key: &str) -> bool {
            let mut handle: HKEY = std::ptr::null_mut();
            let opened =
                unsafe { RegOpenKeyExW(root(at), wide(key).as_ptr(), 0, KEY_READ, &mut handle) };
            if opened == ERROR_SUCCESS {
                unsafe { RegCloseKey(handle) };
                true
            } else {
                false
            }
        }

        pub fn get_string(at: Root, key: &str, name: &str) -> Option<String> {
            let mut size: u32 = 0;
            let first = unsafe {
                RegGetValueW(
                    root(at),
                    wide(key).as_ptr(),
                    wide(name).as_ptr(),
                    RRF_RT_REG_SZ,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    &mut size,
                )
            };
            if first != ERROR_SUCCESS || size == 0 {
                return None;
            }
            let mut buffer = vec![0u16; (size as usize).div_ceil(2)];
            let read = unsafe {
                RegGetValueW(
                    root(at),
                    wide(key).as_ptr(),
                    wide(name).as_ptr(),
                    RRF_RT_REG_SZ,
                    std::ptr::null_mut(),
                    buffer.as_mut_ptr() as *mut _,
                    &mut size,
                )
            };
            if read != ERROR_SUCCESS {
                return None;
            }
            let end = buffer.iter().position(|c| *c == 0).unwrap_or(buffer.len());
            Some(String::from_utf16_lossy(&buffer[..end]))
        }

        pub fn get_dword(at: Root, key: &str, name: &str) -> Option<u32> {
            let mut value: u32 = 0;
            let mut size: u32 = std::mem::size_of::<u32>() as u32;
            let read = unsafe {
                RegGetValueW(
                    root(at),
                    wide(key).as_ptr(),
                    wide(name).as_ptr(),
                    RRF_RT_REG_DWORD,
                    std::ptr::null_mut(),
                    &mut value as *mut u32 as *mut _,
                    &mut size,
                )
            };
            (read == ERROR_SUCCESS).then_some(value)
        }

        pub fn set_string(at: Root, key: &str, name: &str, value: &str) -> Result<()> {
            let mut handle: HKEY = std::ptr::null_mut();
            let created = unsafe {
                RegCreateKeyExW(
                    root(at),
                    wide(key).as_ptr(),
                    0,
                    std::ptr::null(),
                    REG_OPTION_NON_VOLATILE,
                    KEY_SET_VALUE,
                    std::ptr::null(),
                    &mut handle,
                    std::ptr::null_mut(),
                )
            };
            if created != ERROR_SUCCESS {
                bail!("failed to open {} (error {})", key, created);
            }
            let data = wide(value);
            let name = wide(name);
            let written = unsafe {
                RegSetValueExW(
                    handle,
                    name.as_ptr(),
                    0,
                    REG_SZ,
                    data.as_ptr() as *const u8,
                    (data.len() * std::mem::size_of::<u16>()) as u32,
                )
            };
            unsafe { RegCloseKey(handle) };
            if written != ERROR_SUCCESS {
                bail!("failed to write {} (error {})", key, written);
            }
            Ok(())
        }

        pub fn delete_tree(at: Root, key: &str) -> Result<()> {
            let result = unsafe { RegDeleteTreeW(root(at), wide(key).as_ptr()) };
            match result {
                ERROR_SUCCESS | ERROR_FILE_NOT_FOUND => Ok(()),
                error => bail!("failed to remove {} (error {})", key, error),
            }
        }
    }
}

#[cfg(not(any(windows, all(unix, not(target_os = "macos"), not(target_os = "android")))))]
mod unsupported {
    use crate::desktop_integration::{
        Environment, IntegrationState, Item, ItemStatus, Outcome, RepairOffer,
    };

    const NOT_YET: &str = "Not available on this system yet";

    pub fn describe(environment: &mut Environment) {
        if let Some(binary) = environment.binary.clone() {
            environment.package_manager =
                crate::desktop_integration::location::package_manager_of(&binary);
            environment.binary_permanent =
                crate::desktop_integration::location::system_install_of(&binary).is_some();
        }
    }

    pub fn items(_environment: &Environment, _state: &IntegrationState) -> Vec<ItemStatus> {
        Item::all()
            .into_iter()
            .map(|item| ItemStatus::unavailable(item, NOT_YET))
            .collect()
    }

    pub fn repair_offer(_state: &IntegrationState) -> Option<RepairOffer> {
        None
    }

    pub fn install(item: Item, _environment: &Environment, _overwrite: bool) -> Outcome {
        super::outcome(item, true, Err(anyhow::anyhow!(NOT_YET)))
    }

    pub fn remove(item: Item, _environment: &Environment) -> Outcome {
        super::outcome(item, false, Err(anyhow::anyhow!(NOT_YET)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn items_are_installed_in_dependency_order_and_removed_in_reverse() {
        assert_eq!(
            install_order(&[Item::DefaultTerminal, Item::Launcher, Item::CopyBinary]),
            vec![Item::CopyBinary, Item::Launcher, Item::DefaultTerminal]
        );
        assert_eq!(
            removal_order(&[Item::CopyBinary, Item::Launcher, Item::DefaultTerminal]),
            vec![Item::DefaultTerminal, Item::Launcher, Item::CopyBinary]
        );
    }

    #[test]
    fn tiling_window_managers_are_recognised() {
        assert!(is_tiling_wm(None));
        assert!(is_tiling_wm(Some("sway")));
        assert!(is_tiling_wm(Some("Hyprland")));
        assert!(!is_tiling_wm(Some("GNOME")));
        assert!(!is_tiling_wm(Some("KDE")));
    }

    #[test]
    fn a_copy_never_overwrites_a_different_file_without_confirmation() {
        let dir = tempfile::TempDir::new().unwrap();
        let source = dir.path().join("zellij");
        let destination = dir.path().join("bin").join("zellij");
        std::fs::write(&source, b"new").unwrap();
        std::fs::create_dir_all(destination.parent().unwrap()).unwrap();
        std::fs::write(&destination, b"old").unwrap();
        assert!(copy_binary(&source, &destination, false).is_err());
        assert_eq!(std::fs::read(&destination).unwrap(), b"old");
        assert!(copy_binary(&source, &destination, true).unwrap().is_some());
        assert_eq!(std::fs::read(&destination).unwrap(), b"new");
        assert!(copy_binary(&source, &destination, false).unwrap().is_some());
    }
}
