use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

#[cfg(windows)]
pub mod com;
#[cfg(not(target_family = "wasm"))]
pub mod desktop_entry;
#[cfg(not(target_family = "wasm"))]
pub mod platform;
#[cfg(windows)]
pub mod window_command;

#[cfg(not(target_family = "wasm"))]
pub const ICON_PNG: &[u8] = include_bytes!("../../assets/icons/logo128.png");
#[cfg(not(target_family = "wasm"))]
pub const ICON_PNG_256: &[u8] = include_bytes!("../../assets/icons/logo256.png");
#[cfg(not(target_family = "wasm"))]
pub const ICON_PNG_512: &[u8] = include_bytes!("../../assets/icons/logo512.png");

pub const STATE_FILE_NAME: &str = "desktop_integration.json";

pub fn windows_app_id_for(name: &str) -> String {
    format!("{}.window", name)
}

pub fn windows_app_id() -> String {
    windows_app_id_for(crate::distribution::name())
}

pub const REMOTE_WARNING: &str = "The window opens on the computer running Zellij.";

pub const OPT_OUT_ATTENTION: [(&str, &str); 6] = [
    ("frame_markers", "false"),
    ("host_status", "false"),
    ("tab_markers", "false"),
    ("startup_card", "\"never\""),
    ("tray_icon", "false"),
    ("desktop_notifications", "\"none\""),
];

pub const OPT_OUT_TOP_LEVEL: [(&str, &str); 1] = [("session_indicator", "false")];

pub fn opt_out_kdl() -> String {
    let mut text = String::from("attention {\n");
    for (key, value) in OPT_OUT_ATTENTION {
        text.push_str(&format!("    {} {}\n", key, value));
    }
    text.push_str("}\n");
    for (key, value) in OPT_OUT_TOP_LEVEL {
        text.push_str(&format!("{} {}\n", key, value));
    }
    text
}

pub fn opt_out_record() -> BTreeMap<String, String> {
    OPT_OUT_ATTENTION
        .iter()
        .map(|(key, value)| (format!("attention.{}", key), value.to_string()))
        .chain(
            OPT_OUT_TOP_LEVEL
                .iter()
                .map(|(key, value)| (key.to_string(), value.to_string())),
        )
        .collect()
}

pub const DESKTOP_OVERRIDE_ENV: &str = "ZELLIJ_DESKTOP_INTEGRATION";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DesktopOverride {
    On,
    Off,
}

pub fn desktop_override(value: Option<&str>) -> Option<DesktopOverride> {
    match value.map(|v| v.trim().to_ascii_lowercase()).as_deref() {
        Some("on") | Some("1") | Some("true") => Some(DesktopOverride::On),
        Some("off") | Some("0") | Some("false") => Some(DesktopOverride::Off),
        _ => None,
    }
}

pub const DESKTOP_ENTRY_ITEMS: [Item; 3] = [Item::Launcher, Item::StartMenu, Item::ExplorerItem];

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SetupActions {
    pub copy_to_permanent_folder: bool,
    pub install_desktop_entry: bool,
    pub uninstall_desktop_entry: bool,
    pub make_default_terminal: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SetupPlan {
    pub install: Vec<Item>,
    pub remove: Vec<Item>,
    pub notes: Vec<String>,
    pub refused: Vec<String>,
}

impl SetupPlan {
    fn explain(&mut self, status: &Status, items: &[Item], what: &str) {
        let known: Vec<&ItemStatus> = items.iter().filter_map(|item| status.item(*item)).collect();
        let reason = known
            .iter()
            .map(|item| item.state_text.clone())
            .find(|text| !text.is_empty())
            .unwrap_or_else(|| "it is not available here".to_owned());
        let text = format!("{}: {}", what, reason);
        if known.iter().any(|item| item.installed) {
            self.notes.push(text);
        } else {
            self.refused.push(text);
        }
    }
}

pub fn plan_setup(status: &Status, actions: SetupActions) -> SetupPlan {
    let mut plan = SetupPlan::default();
    let installable = |item: &Item| {
        status.offered(*item) && !status.item(*item).map(|s| s.installed).unwrap_or(false)
    };
    let entry_installable: Vec<Item> = DESKTOP_ENTRY_ITEMS
        .into_iter()
        .filter(installable)
        .collect();
    let entry_present = DESKTOP_ENTRY_ITEMS
        .iter()
        .any(|item| status.item(*item).map(|s| s.installed).unwrap_or(false));
    if actions.copy_to_permanent_folder {
        if installable(&Item::CopyBinary) {
            plan.install.push(Item::CopyBinary);
        } else {
            plan.explain(status, &[Item::CopyBinary], "Copy Zellij to a permanent folder");
        }
    }
    let mut wants_entry = actions.install_desktop_entry;
    if actions.make_default_terminal {
        if installable(&Item::DefaultTerminal) {
            plan.install.push(Item::DefaultTerminal);
            if !entry_present {
                wants_entry = true;
            }
        } else {
            plan.explain(status, &[Item::DefaultTerminal], "Make Zellij your default terminal");
        }
    }
    if wants_entry {
        if entry_installable.is_empty() {
            plan.explain(status, &DESKTOP_ENTRY_ITEMS, "Install the desktop entry");
        } else {
            plan.install.extend(entry_installable);
        }
    }
    let needs_the_copy = plan.install.iter().any(|item| *item != Item::CopyBinary);
    if needs_the_copy
        && installable(&Item::CopyBinary)
        && !plan.install.contains(&Item::CopyBinary)
    {
        plan.install.insert(0, Item::CopyBinary);
        plan.notes.push(
            "Copying Zellij to a permanent folder first, so the desktop entry and the default \
             terminal run that copy"
                .to_owned(),
        );
    }
    if actions.uninstall_desktop_entry {
        let removable: Vec<Item> = DESKTOP_ENTRY_ITEMS
            .into_iter()
            .filter(|item| status.removable(*item))
            .collect();
        let present = DESKTOP_ENTRY_ITEMS
            .iter()
            .any(|item| status.item(*item).map(|s| s.installed).unwrap_or(false));
        if removable.is_empty() && present {
            plan.refused.push(
                "Uninstall the desktop entry: it was not installed by Zellij, so it is left alone"
                    .to_owned(),
            );
        } else if removable.is_empty() {
            plan.notes
                .push("Uninstall the desktop entry: there is nothing to remove".to_owned());
        } else {
            plan.remove.extend(removable);
        }
    }
    plan
}

pub fn version_number(text: &str) -> Option<Vec<u64>> {
    let word = text.split_whitespace().last()?.trim_start_matches('v');
    let numbers: Vec<u64> = word
        .split(['-', '+'])
        .next()?
        .split('.')
        .map(|part| part.parse().ok())
        .collect::<Option<_>>()?;
    (!numbers.is_empty()).then_some(numbers)
}

pub fn fingerprint(text: &str) -> String {
    fingerprint_bytes(text.as_bytes())
}

pub fn fingerprint_bytes(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{:02x}", byte)).collect()
}

pub const WINDOW_WELCOME_EMPHASIS: [(&str, usize); 4] = [
    ("keeps your sessions running", 0),
    ("tray icon", 0),
    ("Desktop notifications", 0),
    ("Right-click", 3),
];

pub fn window_welcome_points(only_missing: bool) -> Vec<String> {
    let mut lines = vec![];
    if !only_missing {
        lines.push(
            "Closing this window keeps your sessions running, reopen them from the session list."
                .to_owned(),
        );
        lines.push(
            "The tray icon shows when something needs you, click it for the inbox and sessions."
                .to_owned(),
        );
        lines.push(
            "Desktop notifications tell you when a command finishes, fails or needs you. Choose \
             which in Settings → Attention."
                .to_owned(),
        );
        lines.push(
            "Right-click Zellij's icon in the app menu or taskbar for Show Inbox and Show \
             Sessions."
                .to_owned(),
        );
    }
    lines
}

pub fn tiling_wm_commands() -> [(&'static str, &'static str); 3] {
    [
        ("zellij window", "open a new window"),
        ("zellij window --inbox", "show or hide the inbox"),
        ("zellij window --sessions", "show or hide the session list"),
    ]
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Item {
    Launcher,
    DefaultTerminal,
    CopyBinary,
    StartMenu,
    ExplorerItem,
}

impl Item {
    pub fn all() -> [Item; 5] {
        [
            Item::CopyBinary,
            Item::Launcher,
            Item::StartMenu,
            Item::DefaultTerminal,
            Item::ExplorerItem,
        ]
    }
    pub fn label(&self) -> &'static str {
        match self {
            Item::Launcher => "Desktop entry",
            Item::DefaultTerminal => "Default terminal",
            Item::CopyBinary => "Copy of Zellij in a permanent folder",
            Item::StartMenu => "Start menu shortcut",
            Item::ExplorerItem => "\"Open Zellij here\" in Explorer",
        }
    }
    pub fn opens_a_window(&self) -> bool {
        matches!(
            self,
            Item::Launcher | Item::DefaultTerminal | Item::StartMenu | Item::ExplorerItem
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Owner {
    Nobody,
    Zellij,
    PackageManager,
    Someone,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemStatus {
    pub item: Item,
    pub available: bool,
    pub installed: bool,
    pub changeable: bool,
    pub owner: Owner,
    pub state_text: String,
    pub hint: Option<String>,
    pub instructions: Vec<String>,
    #[serde(default)]
    pub upgrade: Option<Upgrade>,
    #[serde(default)]
    pub target: Option<PathBuf>,
    #[serde(default)]
    pub removes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Upgrade {
    pub path: PathBuf,
    pub from: String,
    pub to: String,
}

impl ItemStatus {
    pub fn unavailable(item: Item, reason: impl Into<String>) -> Self {
        ItemStatus {
            item,
            available: false,
            installed: false,
            changeable: false,
            owner: Owner::Nobody,
            state_text: reason.into(),
            hint: None,
            instructions: vec![],
            upgrade: None,
            target: None,
            removes: vec![],
        }
    }
    pub fn already(item: Item, reason: impl Into<String>) -> Self {
        ItemStatus {
            installed: true,
            ..ItemStatus::unavailable(item, reason)
        }
    }
    pub fn with_removes(mut self, removes: Vec<String>) -> Self {
        self.removes = removes;
        self
    }
    pub fn with_target(mut self, target: PathBuf) -> Self {
        self.target = Some(target);
        self
    }
    pub fn with_upgrade(mut self, upgrade: Upgrade) -> Self {
        self.upgrade = Some(upgrade);
        self
    }
    pub fn with_instructions(mut self, instructions: Vec<String>) -> Self {
        self.instructions = instructions;
        self
    }
    pub fn with_hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Platform {
    Linux,
    Windows,
    MacOs,
    #[default]
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Environment {
    pub platform: Platform,
    pub container: Option<String>,
    pub wsl: bool,
    pub package_manager: Option<String>,
    pub tiling_wm: bool,
    pub desktop: Option<String>,
    pub binary: Option<PathBuf>,
    pub binary_permanent: bool,
    pub binary_location_note: Option<String>,
    pub no_desktop: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepairOffer {
    pub missing: PathBuf,
    pub current: PathBuf,
}

impl RepairOffer {
    pub fn message(&self) -> String {
        format!(
            "Your Zellij launcher points to {}, which no longer exists. Point it at this copy ({})?",
            self.missing.display(),
            self.current.display()
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Status {
    pub environment: Environment,
    pub items: Vec<ItemStatus>,
    pub notes: Vec<String>,
    pub repair: Option<RepairOffer>,
    pub state: IntegrationState,
}

impl Status {
    pub fn item(&self, item: Item) -> Option<&ItemStatus> {
        self.items.iter().find(|status| status.item == item)
    }
    pub fn removable(&self, item: Item) -> bool {
        self.item(item)
            .map(|status| status.installed && status.changeable && status.owner == Owner::Zellij)
            .unwrap_or(false)
    }
    pub fn offered(&self, item: Item) -> bool {
        self.item(item)
            .map(|status| status.available && status.changeable)
            .unwrap_or(false)
    }
    pub fn visible_items(&self) -> impl Iterator<Item = &ItemStatus> {
        self.items
            .iter()
            .filter(|status| status.available || !status.instructions.is_empty())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RepairAnswer {
    Yes,
    No,
    Never,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Flag {
    DesktopAppIntroduced,
    WelcomeCardShown,
    MissingServicesShown,
    WizardDismissed,
    ExitLineShown,
    RemindersOff,
    SetupApproved,
    SetupNeverShow,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Request {
    Status,
    GetState,
    Apply {
        install: Vec<Item>,
        remove: Vec<Item>,
        overwrite_binary: bool,
    },
    Repair(RepairAnswer),
    SetFlag(Flag),
    RecordOptOut(BTreeMap<String, String>),
    ClearOptOut,
    ReportMissingServices(Vec<String>),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Outcome {
    pub item: Item,
    pub install: bool,
    pub succeeded: bool,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Response {
    Status(Status),
    Applied(Vec<Outcome>),
    Done(IntegrationState),
    Error(String),
}

impl Response {
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|e| {
            serde_json::to_string(&Response::Error(e.to_string())).unwrap_or_default()
        })
    }
    pub fn from_json(text: &str) -> Response {
        serde_json::from_str(text.trim()).unwrap_or_else(|e| {
            Response::Error(format!("The answer could not be read: {}", e))
        })
    }
}

impl Request {
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }
    pub fn from_json(text: &str) -> Result<Request, String> {
        serde_json::from_str(text.trim()).map_err(|e| e.to_string())
    }
    pub fn needs_platform(&self) -> bool {
        matches!(
            self,
            Request::Status | Request::Apply { .. } | Request::Repair(_)
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct LauncherRecord {
    pub path: PathBuf,
    pub binary: PathBuf,
    pub fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct BinaryCopyRecord {
    pub destination: PathBuf,
    pub created: bool,
    pub fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct WindowsRecord {
    pub start_menu: Option<PathBuf>,
    pub explorer_item: bool,
    pub jump_list: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct RepairRecord {
    pub declined: Vec<PathBuf>,
    pub never: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct IntegrationState {
    pub launcher: Option<LauncherRecord>,
    pub icons: Vec<PathBuf>,
    pub binary_copy: Option<BinaryCopyRecord>,
    pub windows: WindowsRecord,
    pub opt_out: BTreeMap<String, String>,
    pub repair: RepairRecord,
    pub flags: BTreeSet<Flag>,
    pub missing_services: Vec<String>,
}

impl IntegrationState {
    pub fn has(&self, flag: Flag) -> bool {
        self.flags.contains(&flag)
    }
    pub fn set(&mut self, flag: Flag) {
        self.flags.insert(flag);
    }
    pub fn reminders_allowed(&self, reminders: &ReminderContext) -> bool {
        if reminders.inside_desktop_window || reminders.web_client {
            return false;
        }
        if !reminders.show_startup_tips && !reminders.show_release_notes {
            return false;
        }
        !self.has(Flag::RemindersOff) && !self.has(Flag::DesktopAppIntroduced)
    }
    pub fn setup_resolved(&self) -> bool {
        self.has(Flag::SetupApproved) || self.has(Flag::SetupNeverShow)
    }
    pub fn should_offer_desktop_tip(&self, reminders: &ReminderContext) -> bool {
        reminders.show_startup_tips
            && self.has(Flag::WizardDismissed)
            && self.reminders_allowed(reminders)
    }
    pub fn needs_welcome_card(&self) -> bool {
        !self.has(Flag::WelcomeCardShown)
            || (!self.missing_services.is_empty() && !self.has(Flag::MissingServicesShown))
    }
    pub fn apply(&mut self, request: &Request) -> bool {
        match request {
            Request::SetFlag(flag) => {
                self.set(*flag);
                true
            },
            Request::RecordOptOut(values) => {
                self.opt_out = values.clone();
                true
            },
            Request::ClearOptOut => {
                self.opt_out.clear();
                true
            },
            Request::ReportMissingServices(services) => {
                if &self.missing_services == services {
                    return false;
                }
                self.missing_services = services.clone();
                true
            },
            Request::Status | Request::GetState | Request::Apply { .. } | Request::Repair(_) => {
                false
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ReminderContext {
    pub show_startup_tips: bool,
    pub show_release_notes: bool,
    pub inside_desktop_window: bool,
    pub web_client: bool,
}


#[cfg(not(target_family = "wasm"))]
mod storage {
    use super::*;
    use std::io::Write;
    use std::path::Path;

    pub fn state_path() -> PathBuf {
        crate::consts::ZELLIJ_PROJ_DIR
            .data_dir()
            .join(STATE_FILE_NAME)
    }

    pub fn load() -> IntegrationState {
        load_from(&state_path())
    }

    pub fn load_from(path: &Path) -> IntegrationState {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    pub fn save(state: &IntegrationState) -> std::io::Result<()> {
        save_to(&state_path(), state)
    }

    pub fn save_to(path: &Path, state: &IntegrationState) -> std::io::Result<()> {
        let parent = path
            .parent()
            .ok_or_else(|| std::io::Error::other("the state file has no folder"))?;
        std::fs::create_dir_all(parent)?;
        let text = serde_json::to_string_pretty(state).map_err(std::io::Error::other)?;
        let temporary = parent.join(format!(
            ".{}.{}.tmp",
            STATE_FILE_NAME,
            std::process::id()
        ));
        {
            let mut file = std::fs::File::create(&temporary)?;
            file.write_all(text.as_bytes())?;
            file.sync_all()?;
        }
        std::fs::rename(&temporary, path).inspect_err(|_| {
            let _ = std::fs::remove_file(&temporary);
        })
    }

    pub fn update<T>(change: impl FnOnce(&mut IntegrationState) -> T) -> std::io::Result<T> {
        update_at(&state_path(), change)
    }

    static UPDATES: std::sync::Mutex<()> = std::sync::Mutex::new(());

    pub fn update_at<T>(
        path: &Path,
        change: impl FnOnce(&mut IntegrationState) -> T,
    ) -> std::io::Result<T> {
        let _one_at_a_time = UPDATES.lock().unwrap_or_else(|e| e.into_inner());
        let mut state = load_from(path);
        let before = state.clone();
        let result = change(&mut state);
        if state != before {
            save_to(path, &state)?;
        }
        Ok(result)
    }

    pub fn handle_state_request(request: &Request) -> Response {
        handle_state_request_at(&state_path(), request)
    }

    pub fn handle_state_request_at(path: &Path, request: &Request) -> Response {
        match update_at(path, |state| {
            state.apply(request);
            state.clone()
        }) {
            Ok(state) => Response::Done(state),
            Err(e) => Response::Error(format!("{} could not be written: {}", path.display(), e)),
        }
    }
}

#[cfg(not(target_family = "wasm"))]
pub use storage::*;

#[cfg(not(target_family = "wasm"))]
pub mod location {
    use super::*;
    use std::path::Path;

    pub const MARKER_KEY: &str = "X-Zellij-Installed";
    pub const FINGERPRINT_KEY: &str = "X-Zellij-Fingerprint";

    pub fn entry_values(text: &str) -> BTreeMap<String, String> {
        let mut values = BTreeMap::new();
        let mut in_main_group = false;
        for line in text.lines() {
            let line = line.trim();
            if line.starts_with('[') {
                in_main_group = line == "[Desktop Entry]";
                continue;
            }
            if !in_main_group {
                continue;
            }
            if let Some((key, value)) = line.split_once('=') {
                values
                    .entry(key.trim().to_owned())
                    .or_insert_with(|| value.trim().to_owned());
            }
        }
        values
    }

    pub fn without_marker(text: &str) -> String {
        text.lines()
            .filter(|line| {
                let key = line.split_once('=').map(|(key, _)| key.trim());
                key != Some(MARKER_KEY) && key != Some(FINGERPRINT_KEY)
            })
            .map(|line| format!("{}\n", line))
            .collect()
    }

    pub fn has_marker(text: &str) -> bool {
        entry_values(text).get(MARKER_KEY).map(String::as_str) == Some("true")
    }

    pub fn marker_intact(text: &str) -> bool {
        has_marker(text)
            && entry_values(text).get(FINGERPRINT_KEY).map(String::as_str)
                == Some(fingerprint(&without_marker(text)).as_str())
    }

    pub fn package_manager_of(path: &Path) -> Option<String> {
        let canonical = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        let homebrew = std::env::var_os("HOMEBREW_PREFIX").map(PathBuf::from);
        package_manager_for(&canonical, homebrew.as_deref())
    }

    pub fn package_manager_for(canonical: &Path, homebrew_prefix: Option<&Path>) -> Option<String> {
        if canonical.starts_with("/nix/store") {
            return Some("Nix".to_owned());
        }
        if canonical.starts_with("/home/linuxbrew/.linuxbrew") {
            return Some("Homebrew".to_owned());
        }
        if let Some(prefix) = homebrew_prefix.filter(|p| !p.as_os_str().is_empty()) {
            if canonical.starts_with(prefix) {
                return Some("Homebrew".to_owned());
            }
        }
        if canonical.starts_with("/usr") && !canonical.starts_with("/usr/local") {
            return Some("your package manager".to_owned());
        }
        let cargo_home = std::env::var_os("CARGO_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".cargo")));
        if let Some(cargo_bin) = cargo_home.map(|home| home.join("bin")) {
            if canonical.starts_with(&cargo_bin) {
                return Some("cargo install".to_owned());
            }
        }
        if cfg!(windows) {
            return None;
        }
        if canonical.starts_with("/opt/homebrew") {
            return Some("Homebrew".to_owned());
        }
        None
    }

    pub fn same_file(first: &Path, second: &Path) -> bool {
        match (std::fs::canonicalize(first), std::fs::canonicalize(second)) {
            (Ok(first), Ok(second)) => first == second,
            _ => false,
        }
    }

    pub fn find_on_path(name: &str) -> Option<PathBuf> {
        let path = std::env::var_os("PATH")?;
        find_in(name, &path)
    }

    pub fn find_in(name: &str, path: &std::ffi::OsStr) -> Option<PathBuf> {
        let candidates: Vec<String> = if cfg!(windows) {
            vec![format!("{}.exe", name), name.to_owned()]
        } else {
            vec![name.to_owned()]
        };
        std::env::split_paths(path)
            .filter(|dir| dir.is_absolute())
            .flat_map(|dir| candidates.iter().map(move |c| dir.join(c)))
            .find(|candidate| candidate.is_file())
    }

    pub fn path_contains(dir: &Path) -> bool {
        std::env::var_os("PATH")
            .map(|path| {
                std::env::split_paths(&path).any(|entry| entry == dir || same_file(&entry, dir))
            })
            .unwrap_or(false)
    }

    pub fn started_from() -> Option<PathBuf> {
        let argv0 = std::env::args_os().next()?;
        let argv0 = PathBuf::from(argv0);
        if argv0.components().count() > 1 {
            if argv0.is_absolute() {
                return Some(argv0);
            }
            return std::env::current_dir().ok().map(|cwd| cwd.join(argv0));
        }
        find_on_path(argv0.to_str()?)
    }

    pub fn stable_binary_path(running: &Path) -> PathBuf {
        let user = std::env::var("USER").unwrap_or_default();
        let home = std::env::var_os("HOME").map(PathBuf::from);
        let mut candidates = vec![];
        if let Some(home) = &home {
            candidates.push(home.join(".nix-profile/bin").join(crate::distribution::name()));
        }
        if !user.is_empty() {
            candidates.push(
                PathBuf::from("/etc/profiles/per-user")
                    .join(&user)
                    .join("bin")
                    .join(crate::distribution::name()),
            );
        }
        candidates.push(
            PathBuf::from("/run/current-system/sw/bin").join(crate::distribution::name()),
        );
        stable_binary_path_from(running, started_from().as_deref(), &candidates)
    }

    pub fn stable_binary_path_from(
        running: &Path,
        started_from: Option<&Path>,
        nix_candidates: &[PathBuf],
    ) -> PathBuf {
        if let Some(started) = started_from {
            if !started.starts_with("/nix/store") && same_file(started, running) {
                return started.to_path_buf();
            }
        }
        let canonical = std::fs::canonicalize(running).unwrap_or_else(|_| running.to_path_buf());
        if canonical.starts_with("/nix/store") {
            if let Some(found) = nix_candidates
                .iter()
                .find(|candidate| same_file(candidate, running))
            {
                return found.clone();
            }
        }
        running.to_path_buf()
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum Place {
        Downloads,
        Temporary,
        Removable,
        Elsewhere,
    }

    pub fn temporary_dirs() -> Vec<PathBuf> {
        let mut dirs = vec![PathBuf::from("/tmp"), PathBuf::from("/var/tmp")];
        if let Some(tmpdir) = std::env::var_os("TMPDIR") {
            dirs.push(PathBuf::from(tmpdir));
        }
        dirs.push(std::env::temp_dir());
        dirs
    }

    pub fn downloads_dir() -> Option<PathBuf> {
        let home = std::env::var_os("HOME").map(PathBuf::from)?;
        let config_home = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .unwrap_or_else(|| home.join(".config"));
        let text = std::fs::read_to_string(config_home.join("user-dirs.dirs")).ok()?;
        downloads_from_user_dirs(&text, &home)
    }

    pub fn downloads_from_user_dirs(text: &str, home: &Path) -> Option<PathBuf> {
        text.lines()
            .map(str::trim)
            .filter(|line| !line.starts_with('#'))
            .find_map(|line| line.strip_prefix("XDG_DOWNLOAD_DIR="))
            .map(|value| value.trim().trim_matches('"'))
            .map(|value| match value.strip_prefix("$HOME") {
                Some(rest) => home.join(rest.trim_start_matches('/')),
                None => PathBuf::from(value),
            })
    }

    pub fn place_of(path: &Path, downloads: Option<&Path>, temporary: &[PathBuf]) -> Place {
        let inside = |dir: &Path| {
            path.starts_with(dir)
                || std::fs::canonicalize(dir)
                    .map(|dir| path.starts_with(dir))
                    .unwrap_or(false)
        };
        if downloads.map(inside).unwrap_or(false) {
            return Place::Downloads;
        }
        if temporary.iter().any(|dir| inside(dir)) {
            return Place::Temporary;
        }
        if path.starts_with("/media") || path.starts_with("/run/media") {
            return Place::Removable;
        }
        Place::Elsewhere
    }

    pub fn place_note(place: Place) -> String {
        match place {
            Place::Downloads => "Zellij is running from your Downloads folder.".to_owned(),
            Place::Temporary => {
                "Zellij is running from a temporary folder that may be emptied.".to_owned()
            },
            Place::Removable => "Zellij is running from a removable drive.".to_owned(),
            Place::Elsewhere => "This copy of Zellij isn't on your PATH.".to_owned(),
        }
    }

    pub fn system_install_of(path: &Path) -> Option<String> {
        if let Some(manager) = package_manager_of(path) {
            return Some(manager);
        }
        let canonical = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        system_folder_for(&canonical)
    }

    pub fn system_folder_for(canonical: &Path) -> Option<String> {
        if cfg!(windows) {
            return None;
        }
        ["/usr/local", "/opt"]
            .iter()
            .find(|prefix| canonical.starts_with(prefix))
            .map(|prefix| format!("a system folder ({})", prefix))
    }

    pub fn same_contents(first: &Path, second: &Path) -> bool {
        if same_file(first, second) {
            return true;
        }
        let size = |path: &Path| std::fs::metadata(path).ok().map(|m| m.len());
        match (size(first), size(second)) {
            (Some(a), Some(b)) if a == b => {},
            _ => return false,
        }
        match (std::fs::read(first), std::fs::read(second)) {
            (Ok(a), Ok(b)) => a == b,
            _ => false,
        }
    }

    pub fn data_home() -> Option<PathBuf> {
        if let Some(from_env) = std::env::var_os("XDG_DATA_HOME").map(PathBuf::from) {
            if from_env.is_absolute() {
                return Some(from_env);
            }
        }
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .filter(|home| home.is_absolute())
            .map(|home| home.join(".local").join("share"))
    }

    pub fn launcher_file_name() -> String {
        format!("{}.desktop", crate::distribution::app_id())
    }

    pub fn user_launcher_path(data_home: &Path) -> PathBuf {
        data_home.join("applications").join(launcher_file_name())
    }

    fn system_data_dirs() -> Vec<PathBuf> {
        let dirs = std::env::var_os("XDG_DATA_DIRS")
            .filter(|dirs| !dirs.is_empty())
            .unwrap_or_else(|| "/usr/local/share:/usr/share".into());
        std::env::split_paths(&dirs).collect()
    }

    pub fn system_launcher() -> Option<PathBuf> {
        system_data_dirs()
            .into_iter()
            .map(|dir| dir.join("applications").join(launcher_file_name()))
            .find(|path| path.is_file())
    }

    pub fn repair_offer(state: &IntegrationState, running: &Path) -> Option<RepairOffer> {
        let launcher = user_launcher_path(&data_home()?);
        let text = std::fs::read_to_string(&launcher).ok()?;
        repair_offer_for(state, &text, running, &temporary_dirs())
    }

    pub fn repair_offer_for(
        state: &IntegrationState,
        launcher_text: &str,
        running: &Path,
        temporary: &[PathBuf],
    ) -> Option<RepairOffer> {
        if state.repair.never || !marker_intact(launcher_text) {
            return None;
        }
        let values = entry_values(launcher_text);
        let missing = PathBuf::from(values.get("TryExec")?);
        if !missing.is_absolute() || missing.exists() {
            return None;
        }
        if state.repair.declined.contains(&missing) {
            return None;
        }
        if package_manager_of(running).is_some() {
            return None;
        }
        if place_of(running, None, temporary) == Place::Temporary {
            return None;
        }
        Some(RepairOffer {
            missing,
            current: running.to_path_buf(),
        })
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn marked(body: &str) -> String {
            format!(
                "{}{}=true\n{}={}\n",
                body,
                MARKER_KEY,
                FINGERPRINT_KEY,
                fingerprint(body)
            )
        }

        #[test]
        fn a_launcher_written_by_zellij_keeps_an_intact_marker_until_edited() {
            let body = "[Desktop Entry]\nName=Zellij\nTryExec=/gone/zellij\n";
            let text = marked(body);
            assert!(has_marker(&text));
            assert!(marker_intact(&text));
            let edited = text.replace("Name=Zellij", "Name=Mine");
            assert!(has_marker(&edited));
            assert!(!marker_intact(&edited));
            assert!(!marker_intact(body));
        }

        #[test]
        fn package_manager_paths_are_recognised() {
            for (path, expected) in [
                ("/nix/store/abc-zellij/bin/zellij", Some("Nix")),
                ("/usr/bin/zellij", Some("your package manager")),
                ("/home/linuxbrew/.linuxbrew/bin/zellij", Some("Homebrew")),
                ("/usr/local/bin/zellij", None),
                ("/home/me/.local/bin/zellij", None),
            ] {
                assert_eq!(
                    package_manager_for(Path::new(path), None).as_deref(),
                    expected,
                    "{}",
                    path
                );
            }
            assert_eq!(
                package_manager_for(
                    Path::new("/opt/brew/bin/zellij"),
                    Some(Path::new("/opt/brew"))
                )
                .as_deref(),
                Some("Homebrew")
            );
        }

        #[test]
        fn the_downloads_folder_is_read_from_user_dirs_in_any_language() {
            let home = Path::new("/home/me");
            let text = "# comment\nXDG_DESKTOP_DIR=\"$HOME/Schreibtisch\"\nXDG_DOWNLOAD_DIR=\"$HOME/Téléchargements\"\n";
            assert_eq!(
                downloads_from_user_dirs(text, home),
                Some(PathBuf::from("/home/me/Téléchargements"))
            );
            assert_eq!(downloads_from_user_dirs("", home), None);
        }

        #[test]
        fn places_are_named_only_when_certain() {
            let downloads = PathBuf::from("/home/me/Downloads");
            let temporary = vec![PathBuf::from("/tmp")];
            assert_eq!(
                place_of(
                    Path::new("/home/me/Downloads/zellij"),
                    Some(&downloads),
                    &temporary
                ),
                Place::Downloads
            );
            assert_eq!(
                place_of(Path::new("/tmp/x/zellij"), Some(&downloads), &temporary),
                Place::Temporary
            );
            assert_eq!(
                place_of(Path::new("/media/usb/zellij"), Some(&downloads), &temporary),
                Place::Removable
            );
            assert_eq!(
                place_of(Path::new("/srv/zellij"), Some(&downloads), &temporary),
                Place::Elsewhere
            );
        }

        #[test]
        fn the_repair_prompt_needs_every_condition() {
            let dir = tempfile::tempdir().unwrap();
            let running = dir.path().join("zellij");
            std::fs::write(&running, b"binary").unwrap();
            let missing = dir.path().join("moved").join("zellij");
            let body = format!(
                "[Desktop Entry]\nExec={} window\nTryExec={}\n",
                missing.display(),
                missing.display()
            );
            let text = marked(&body);
            let temporary: Vec<PathBuf> = vec![];
            let state = IntegrationState::default();
            let offer = repair_offer_for(&state, &text, &running, &temporary).unwrap();
            assert_eq!(offer.missing, missing);
            assert_eq!(offer.current, running);

            assert!(repair_offer_for(&state, &body, &running, &temporary).is_none());
            let edited = text.replace("window", "window --inbox");
            assert!(repair_offer_for(&state, &edited, &running, &temporary).is_none());

            let mut declined = IntegrationState::default();
            declined.repair.declined.push(missing.clone());
            assert!(repair_offer_for(&declined, &text, &running, &temporary).is_none());

            let mut never = IntegrationState::default();
            never.repair.never = true;
            assert!(repair_offer_for(&never, &text, &running, &temporary).is_none());

            assert!(
                repair_offer_for(&state, &text, &running, &[dir.path().to_path_buf()]).is_none()
            );

            std::fs::create_dir_all(missing.parent().unwrap()).unwrap();
            std::fs::write(&missing, b"binary").unwrap();
            assert!(repair_offer_for(&state, &text, &running, &temporary).is_none());
        }

        #[test]
        fn the_launcher_prefers_the_path_zellij_was_started_from() {
            let dir = tempfile::tempdir().unwrap();
            let store = dir.path().join("store-zellij");
            std::fs::write(&store, b"binary").unwrap();
            let profile = dir.path().join("profile-zellij");
            #[cfg(unix)]
            std::os::unix::fs::symlink(&store, &profile).unwrap();
            #[cfg(not(unix))]
            std::fs::copy(&store, &profile).unwrap();
            assert_eq!(
                stable_binary_path_from(&store, Some(&profile), &[]),
                if cfg!(unix) { profile.clone() } else { store.clone() }
            );
            assert_eq!(stable_binary_path_from(&store, None, &[]), store);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn context() -> ReminderContext {
        ReminderContext {
            show_startup_tips: true,
            show_release_notes: true,
            inside_desktop_window: false,
            web_client: false,
        }
    }

    #[test]
    fn a_missing_or_corrupt_state_file_records_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(STATE_FILE_NAME);
        assert_eq!(load_from(&path), IntegrationState::default());
        std::fs::write(&path, "{ not json").unwrap();
        assert_eq!(load_from(&path), IntegrationState::default());
    }

    #[test]
    fn the_state_survives_a_save_and_leaves_no_temporary_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join(STATE_FILE_NAME);
        let mut state = IntegrationState::default();
        state.set(Flag::ExitLineShown);
        state.launcher = Some(LauncherRecord {
            path: PathBuf::from("/home/me/.local/share/applications/dev.zellij.Zellij.desktop"),
            binary: PathBuf::from("/home/me/.local/bin/zellij"),
            fingerprint: "abc".to_owned(),
        });
        save_to(&path, &state).unwrap();
        assert_eq!(load_from(&path), state);
        let leftovers: Vec<_> = std::fs::read_dir(path.parent().unwrap())
            .unwrap()
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.file_name().to_string_lossy().ends_with(".tmp"))
            .collect();
        assert!(leftovers.is_empty());
    }

    #[test]
    fn fields_added_later_default_when_reading_an_older_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(STATE_FILE_NAME);
        std::fs::write(&path, r#"{"flags":["ExitLineShown"]}"#).unwrap();
        let state = load_from(&path);
        assert!(state.has(Flag::ExitLineShown));
        assert!(state.launcher.is_none());
    }

    #[test]
    fn concurrent_updates_do_not_overwrite_each_other() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(STATE_FILE_NAME);
        let handles: Vec<_> = [
            Request::RecordOptOut(opt_out_record()),
            Request::SetFlag(Flag::SetupApproved),
            Request::SetFlag(Flag::WizardDismissed),
            Request::SetFlag(Flag::ExitLineShown),
        ]
        .into_iter()
        .map(|request| {
            let path = path.clone();
            std::thread::spawn(move || handle_state_request_at(&path, &request))
        })
        .collect();
        for handle in handles {
            handle.join().unwrap();
        }
        let state = load_from(&path);
        assert_eq!(state.opt_out, opt_out_record());
        assert!(state.has(Flag::SetupApproved));
        assert!(state.has(Flag::WizardDismissed));
        assert!(state.has(Flag::ExitLineShown));
    }

    #[test]
    fn state_requests_update_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(STATE_FILE_NAME);
        let response =
            handle_state_request_at(&path, &Request::SetFlag(Flag::DesktopAppIntroduced));
        match response {
            Response::Done(state) => assert!(state.has(Flag::DesktopAppIntroduced)),
            other => panic!("{:?}", other),
        }
        assert!(load_from(&path).has(Flag::DesktopAppIntroduced));
        handle_state_request_at(&path, &Request::RecordOptOut(opt_out_record()));
        assert_eq!(load_from(&path).opt_out, opt_out_record());
        handle_state_request_at(&path, &Request::ClearOptOut);
        assert!(load_from(&path).opt_out.is_empty());
    }

    #[test]
    fn reminders_stop_for_every_listed_reason() {
        let state = IntegrationState::default();
        assert!(state.reminders_allowed(&context()));
        for flag in [Flag::RemindersOff, Flag::DesktopAppIntroduced] {
            let mut state = IntegrationState::default();
            state.set(flag);
            assert!(!state.reminders_allowed(&context()));
        }
        assert!(!state.reminders_allowed(&ReminderContext {
            show_startup_tips: false,
            show_release_notes: false,
            ..context()
        }));
        assert!(!state.reminders_allowed(&ReminderContext {
            inside_desktop_window: true,
            ..context()
        }));
        assert!(!state.reminders_allowed(&ReminderContext {
            web_client: true,
            ..context()
        }));
    }

    #[test]
    fn the_desktop_tip_follows_an_escaped_wizard_only_while_tips_are_on() {
        let mut state = IntegrationState::default();
        assert!(!state.should_offer_desktop_tip(&context()));
        state.set(Flag::WizardDismissed);
        assert!(state.should_offer_desktop_tip(&context()));
        assert!(!state.should_offer_desktop_tip(&ReminderContext {
            show_startup_tips: false,
            ..context()
        }));
    }

    fn status_with(items: Vec<ItemStatus>) -> Status {
        Status {
            items,
            ..Default::default()
        }
    }

    fn offered(item: Item) -> ItemStatus {
        ItemStatus {
            available: true,
            changeable: true,
            ..ItemStatus::unavailable(item, "")
        }
    }

    fn ours(item: Item) -> ItemStatus {
        ItemStatus {
            installed: true,
            owner: Owner::Zellij,
            ..offered(item)
        }
    }

    #[test]
    fn the_setup_plan_matches_what_the_setup_screen_would_do() {
        let status = status_with(vec![
            offered(Item::CopyBinary),
            offered(Item::Launcher),
            offered(Item::DefaultTerminal),
        ]);
        let plan = plan_setup(
            &status,
            SetupActions {
                make_default_terminal: true,
                ..Default::default()
            },
        );
        assert_eq!(
            plan.install,
            vec![Item::CopyBinary, Item::DefaultTerminal, Item::Launcher]
        );
        assert_eq!(plan.notes.len(), 1);
        assert!(plan.notes[0].contains("permanent folder first"));
        let without_a_copy = status_with(vec![
            ItemStatus::already(Item::CopyBinary, "Installed by Nix at /nix/store/zellij"),
            offered(Item::Launcher),
        ]);
        let plan = plan_setup(
            &without_a_copy,
            SetupActions {
                install_desktop_entry: true,
                ..Default::default()
            },
        );
        assert_eq!(plan.install, vec![Item::Launcher]);
        assert!(plan.notes.is_empty());
        let plan = plan_setup(
            &status,
            SetupActions {
                copy_to_permanent_folder: true,
                install_desktop_entry: true,
                ..Default::default()
            },
        );
        assert_eq!(plan.install, vec![Item::CopyBinary, Item::Launcher]);
    }

    #[test]
    fn the_setup_plan_explains_what_it_cannot_do() {
        let status = status_with(vec![
            ItemStatus::already(Item::CopyBinary, "Zellij is already installed at /usr/bin/zellij"),
            ItemStatus::unavailable(Item::Launcher, "Not available over SSH"),
            ItemStatus::already(Item::DefaultTerminal, "Zellij is the default terminal"),
        ]);
        let plan = plan_setup(
            &status,
            SetupActions {
                copy_to_permanent_folder: true,
                install_desktop_entry: true,
                uninstall_desktop_entry: true,
                make_default_terminal: true,
            },
        );
        assert!(plan.install.is_empty() && plan.remove.is_empty());
        assert_eq!(plan.notes.len(), 3);
        assert!(plan.notes[0].contains("already installed at /usr/bin/zellij"));
        assert!(plan.notes[1].contains("Zellij is the default terminal"));
        assert!(plan.notes[2].contains("nothing to remove"));
        assert_eq!(plan.refused.len(), 1);
        assert!(plan.refused[0].contains("Not available over SSH"));
    }

    #[test]
    fn only_a_desktop_entry_zellij_wrote_is_uninstalled() {
        let mut someone = ours(Item::Launcher);
        someone.owner = Owner::Someone;
        someone.changeable = false;
        let actions = SetupActions {
            uninstall_desktop_entry: true,
            ..Default::default()
        };
        assert_eq!(
            plan_setup(&status_with(vec![ours(Item::Launcher)]), actions).remove,
            vec![Item::Launcher]
        );
        let plan = plan_setup(&status_with(vec![someone]), actions);
        assert!(plan.remove.is_empty());
        assert_eq!(plan.refused.len(), 1);
    }

    #[test]
    fn the_desktop_override_reads_on_and_off() {
        assert_eq!(desktop_override(Some("on")), Some(DesktopOverride::On));
        assert_eq!(desktop_override(Some("OFF")), Some(DesktopOverride::Off));
        assert_eq!(desktop_override(Some("maybe")), None);
        assert_eq!(desktop_override(None), None);
    }

    #[test]
    fn system_folders_count_as_permanent_installs() {
        if cfg!(windows) {
            return;
        }
        assert!(location::system_folder_for(Path::new("/usr/local/bin/zellij")).is_some());
        assert!(location::system_folder_for(Path::new("/opt/zellij/zellij")).is_some());
        assert!(location::system_folder_for(Path::new("/home/me/code/target/zellij")).is_none());
    }

    #[test]
    fn files_with_the_same_bytes_are_the_same_copy() {
        let dir = tempfile::tempdir().unwrap();
        let (a, b, c) = (dir.path().join("a"), dir.path().join("b"), dir.path().join("c"));
        std::fs::write(&a, b"zellij").unwrap();
        std::fs::write(&b, b"zellij").unwrap();
        std::fs::write(&c, b"zelliJ").unwrap();
        assert!(location::same_contents(&a, &b));
        assert!(!location::same_contents(&a, &c));
        assert!(!location::same_contents(&a, &dir.path().join("missing")));
    }

    #[test]
    fn versions_are_read_from_the_version_output() {
        assert_eq!(version_number("zellij 0.43.1"), Some(vec![0, 43, 1]));
        assert_eq!(version_number("0.45.0-dev"), Some(vec![0, 45, 0]));
        assert!(version_number("zellij 0.44.0") < version_number("zellij 0.45.0"));
        assert_eq!(version_number("garbage"), None);
    }

    #[test]
    fn the_setup_is_resolved_once_approved_or_never_shown_again() {
        let state = IntegrationState::default();
        assert!(!state.setup_resolved());
        for flag in [Flag::SetupApproved, Flag::SetupNeverShow] {
            let mut state = IntegrationState::default();
            state.set(flag);
            assert!(state.setup_resolved());
        }
        let mut dismissed = IntegrationState::default();
        dismissed.set(Flag::WizardDismissed);
        assert!(!dismissed.setup_resolved());
    }

    #[test]
    fn the_opt_out_writes_every_attention_key_and_the_session_indicator() {
        let kdl = opt_out_kdl();
        for (key, value) in OPT_OUT_ATTENTION {
            assert!(kdl.contains(&format!("{} {}", key, value)), "{}", kdl);
        }
        assert!(kdl.contains("session_indicator false"));
    }

    #[test]
    fn requests_and_responses_survive_json() {
        let request = Request::Apply {
            install: vec![Item::Launcher],
            remove: vec![Item::DefaultTerminal],
            overwrite_binary: false,
        };
        assert_eq!(Request::from_json(&request.to_json()).unwrap(), request);
        let response = Response::Applied(vec![Outcome {
            item: Item::Launcher,
            install: true,
            succeeded: true,
            message: "Wrote it".to_owned(),
        }]);
        assert_eq!(Response::from_json(&response.to_json()), response);
        assert!(matches!(Response::from_json("garbage"), Response::Error(_)));
    }
}
