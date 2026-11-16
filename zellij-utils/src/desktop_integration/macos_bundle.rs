//! Builds and installs Zellij.app.
//!
//! The app's executable has to be zellij itself, not a script that runs it, or macOS won't show
//! the menu-bar icon. It also has to be signed with the app's id, or notifications are refused.
//! So we copy the binary into the bundle and ad-hoc sign it.

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{bail, Context, Result};

use crate::distribution::{app_id, display_name, distribution};

const BINARY: &str = "zellij";
const ICON: &str = "zellij.icns";
const MINIMUM_SYSTEM_VERSION: &str = "11.0";
const CODESIGN: &str = "/usr/bin/codesign";
// Signing modifies the copied binary, so to tell later which zellij the app came from, we keep
// the original's size and mtime in Info.plist.
const SOURCE_LENGTH_KEY: &str = "ZellijSourceLength";
const SOURCE_MODIFIED_KEY: &str = "ZellijSourceModified";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Installed {
    pub version: String,
    /// The zellij the app was made from.
    pub build: Build,
}

/// Tells builds of zellij apart by file size and mtime (in nanoseconds).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Build {
    pub len: u64,
    pub modified: u128,
}

pub fn build_of(binary: &Path) -> Option<Build> {
    let metadata = binary.metadata().ok()?;
    let modified = metadata
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_nanos();
    Some(Build {
        len: metadata.len(),
        modified,
    })
}

pub fn applications_dir() -> Result<PathBuf> {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|home| home.is_absolute())
        .context("HOME does not name an absolute directory, so there is nowhere to install to")?;
    Ok(home.join("Applications"))
}

pub fn bundle_name() -> String {
    format!("{}.app", display_name())
}

pub fn bundle_path(applications: &Path) -> PathBuf {
    applications.join(bundle_name())
}

fn binary_in(bundle: &Path) -> PathBuf {
    bundle.join("Contents").join("MacOS").join(BINARY)
}

fn plist_in(bundle: &Path) -> PathBuf {
    bundle.join("Contents").join("Info.plist")
}

/// True for `*.app/Contents/MacOS/<binary>`, following symlinks.
pub fn is_inside_a_bundle(binary: &Path) -> bool {
    let resolved = std::fs::canonicalize(binary).unwrap_or_else(|_| binary.to_path_buf());
    let mut parents = resolved.ancestors().skip(1);
    let (Some(macos), Some(contents), Some(app)) = (parents.next(), parents.next(), parents.next())
    else {
        return false;
    };
    macos.file_name().is_some_and(|n| n == "MacOS")
        && contents.file_name().is_some_and(|n| n == "Contents")
        && app.extension().is_some_and(|e| e == "app")
}

// Temporary names used while installing. They don't end in `.app`, so nothing mistakes them
// for an app.
fn staging_prefix() -> String {
    format!(".{}.installing-", bundle_name())
}

fn replaced_prefix() -> String {
    format!(".{}.replaced-", bundle_name())
}

/// Builds the new app next to the old one and renames it into place, so an app that is
/// already running keeps working.
pub fn install(
    applications: &Path,
    binary: &Path,
    sign: impl Fn(&Path) -> Result<()>,
) -> Result<PathBuf> {
    let bundle = bundle_path(applications);
    let binary = binary
        .canonicalize()
        .with_context(|| format!("failed to resolve {:?}", binary))?;
    if let Ok(existing) = bundle.canonicalize() {
        if binary.starts_with(&existing) {
            bail!(
                "{:?} is the copy inside {:?}, so it cannot be installed over itself. Run this \
                 from the zellij the app should be made from",
                binary,
                bundle
            );
        }
    }
    std::fs::create_dir_all(applications)
        .with_context(|| format!("failed to create {:?}", applications))?;
    let _lock = lock(applications)?;
    ours_or_absent(&bundle)?;
    let icon = super::icns::zellij().context("failed to build the app icon")?;
    remove_leftovers(applications)?;

    let pid = std::process::id();
    let staging = applications.join(format!("{}{pid}", staging_prefix()));
    let replaced = applications.join(format!("{}{pid}", replaced_prefix()));
    let outcome =
        build(&staging, &binary, &icon, sign).and_then(|()| swap(&staging, &bundle, &replaced));
    if let Err(e) = outcome {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(e);
    }
    Ok(bundle)
}

/// Removes the app if it's ours. A running app keeps working until it exits.
pub fn uninstall(applications: &Path) -> Result<Option<PathBuf>> {
    let bundle = bundle_path(applications);
    if !applications.exists() {
        return Ok(None);
    }
    let _lock = lock(applications)?;
    let present = ours_or_absent(&bundle)?;
    remove_leftovers(applications)?;
    if present {
        std::fs::remove_dir_all(&bundle)
            .with_context(|| format!("failed to remove {:?}", bundle))?;
    }
    Ok(present.then_some(bundle))
}

pub fn installed(applications: &Path) -> Option<Installed> {
    let bundle = bundle_path(applications);
    if !is_ours(&bundle) {
        return None;
    }
    let plist = std::fs::read_to_string(plist_in(&bundle)).ok()?;
    Some(Installed {
        version: plist_string(&plist, "CFBundleShortVersionString")?.to_owned(),
        build: Build {
            len: plist_number(&plist, SOURCE_LENGTH_KEY),
            modified: plist_number(&plist, SOURCE_MODIFIED_KEY),
        },
    })
}

fn build(
    staging: &Path,
    binary: &Path,
    icon: &[u8],
    sign: impl Fn(&Path) -> Result<()>,
) -> Result<()> {
    let copy = binary_in(staging);
    let source = build_of(binary).with_context(|| format!("failed to read {:?}", binary))?;
    write(&plist_in(staging), info_plist(source).as_bytes())?;
    std::fs::create_dir_all(copy.parent().expect("a path inside the bundle"))
        .with_context(|| format!("failed to create {:?}", copy.parent()))?;
    // Not `fs::copy`: it would also copy the quarantine flag from a downloaded binary.
    let bytes = std::fs::read(binary).with_context(|| format!("failed to read {:?}", binary))?;
    std::fs::write(&copy, bytes)
        .with_context(|| format!("failed to copy {:?} to {:?}", binary, copy))?;
    std::fs::set_permissions(&copy, std::fs::Permissions::from_mode(0o755))
        .with_context(|| format!("failed to make {:?} executable", copy))?;
    write(&staging.join("Contents").join("Resources").join(ICON), icon)?;
    // Notifications only work if the binary itself is signed with the app's id.
    sign(&copy)?;
    sign(staging)
}

fn write(path: &Path, contents: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {:?}", parent))?;
    }
    std::fs::write(path, contents).with_context(|| format!("failed to write {:?}", path))
}

// rename() can't replace a non-empty directory, so move the old app out of the way first, and
// back again if the new one can't be moved in.
fn swap(staging: &Path, bundle: &Path, replaced: &Path) -> Result<()> {
    let had_previous = match std::fs::rename(bundle, replaced) {
        Ok(()) => true,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
        Err(e) => return Err(e).with_context(|| format!("failed to move {:?} aside", bundle)),
    };
    if let Err(e) = std::fs::rename(staging, bundle) {
        if had_previous {
            let _ = std::fs::rename(replaced, bundle);
        }
        return Err(e).with_context(|| format!("failed to move the new app to {:?}", bundle));
    }
    if had_previous {
        let _ = std::fs::remove_dir_all(replaced);
    }
    Ok(())
}

fn ours_or_absent(bundle: &Path) -> Result<bool> {
    match bundle.symlink_metadata() {
        Ok(_) => {},
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(e).with_context(|| format!("failed to inspect {:?}", bundle)),
    }
    if !is_ours(bundle) {
        bail!(
            "{:?} exists but is not Zellij's app (its Info.plist does not name {}), so it is \
             left alone. Move it away and try again",
            bundle,
            app_id()
        );
    }
    Ok(true)
}

fn is_ours(bundle: &Path) -> bool {
    let is_directory = bundle
        .symlink_metadata()
        .map(|m| m.is_dir())
        .unwrap_or(false);
    is_directory
        && std::fs::read_to_string(plist_in(bundle))
            .map(|plist| plist_string(&plist, "CFBundleIdentifier") == Some(app_id()))
            .unwrap_or(false)
}

fn plist_string<'a>(plist: &'a str, key: &str) -> Option<&'a str> {
    let (_, rest) = plist.split_once(&format!("<key>{}</key>", key))?;
    let value = rest.trim_start().strip_prefix("<string>")?;
    let (value, _) = value.split_once("</string>")?;
    Some(value)
}

fn plist_number<T: std::str::FromStr + Default>(plist: &str, key: &str) -> T {
    plist_string(plist, key)
        .and_then(|value| value.parse().ok())
        .unwrap_or_default()
}

// One install or removal at a time, so none deletes another's staging folder. Released when
// the returned file is dropped.
fn lock(applications: &Path) -> Result<std::fs::File> {
    let folder = std::fs::File::open(applications)
        .with_context(|| format!("failed to open {:?}", applications))?;
    folder
        .lock()
        .with_context(|| format!("failed to lock {:?}", applications))?;
    Ok(folder)
}

fn remove_leftovers(applications: &Path) -> Result<()> {
    let entries = match std::fs::read_dir(applications) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e).with_context(|| format!("failed to list {:?}", applications)),
    };
    for entry in entries {
        let entry = entry.with_context(|| format!("failed to list {:?}", applications))?;
        let is_directory = entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false);
        if is_directory && entry.file_name().to_str().is_some_and(is_leftover) {
            std::fs::remove_dir_all(entry.path())
                .with_context(|| format!("failed to remove {:?}", entry.path()))?;
        }
    }
    Ok(())
}

fn is_leftover(name: &str) -> bool {
    [staging_prefix(), replaced_prefix()].iter().any(|prefix| {
        name.strip_prefix(prefix.as_str())
            .is_some_and(|pid| !pid.is_empty() && pid.bytes().all(|b| b.is_ascii_digit()))
    })
}

fn info_plist(source: Build) -> String {
    let (source_len, source_modified) = (source.len, source.modified);
    let id = xml_escape(app_id());
    let name = xml_escape(display_name());
    let version = distribution().version;
    let short_version = xml_escape(version);
    let build_version = xml_escape(build_version(version));
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleIdentifier</key>
    <string>{id}</string>
    <key>CFBundleName</key>
    <string>{name}</string>
    <key>CFBundleDisplayName</key>
    <string>{name}</string>
    <key>CFBundleExecutable</key>
    <string>{BINARY}</string>
    <key>CFBundleIconFile</key>
    <string>{ICON}</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleShortVersionString</key>
    <string>{short_version}</string>
    <key>CFBundleVersion</key>
    <string>{build_version}</string>
    <key>NSHighResolutionCapable</key>
    <true/>
    <key>LSMinimumSystemVersion</key>
    <string>{MINIMUM_SYSTEM_VERSION}</string>
    <key>{SOURCE_LENGTH_KEY}</key>
    <string>{source_len}</string>
    <key>{SOURCE_MODIFIED_KEY}</key>
    <string>{source_modified}</string>
</dict>
</plist>
"#
    )
}

// CFBundleVersion only allows numbers and dots, so drop suffixes like `-rc1`.
fn build_version(version: &str) -> &str {
    version.split(['-', '+']).next().unwrap_or(version)
}

fn xml_escape(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for c in value.chars() {
        match c {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&apos;"),
            c => escaped.push(c),
        }
    }
    escaped
}

/// Ad-hoc signs a binary or bundle with the app id.
pub fn codesign(path: &Path) -> Result<()> {
    let output = codesign_command(path)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => {
                anyhow::anyhow!("{CODESIGN} is missing, and the app cannot be used unsigned")
            },
            _ => anyhow::Error::new(e).context(format!("failed to run {CODESIGN}")),
        })?;
    if !output.status.success() {
        bail!(
            "{CODESIGN} failed to sign {:?} ({}): {}",
            path,
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(())
}

fn codesign_command(path: &Path) -> Command {
    let mut command = Command::new(CODESIGN);
    command
        .args(["--force", "-s", "-", "--identifier"])
        .arg(app_id())
        .arg(path);
    command
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    struct Rig {
        dir: tempfile::TempDir,
        binary: PathBuf,
    }

    impl Rig {
        fn new() -> Self {
            let dir = tempfile::TempDir::new().unwrap();
            let binary = dir.path().join("bin").join("zellij");
            std::fs::create_dir_all(binary.parent().unwrap()).unwrap();
            std::fs::write(&binary, b"first build").unwrap();
            std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o755)).unwrap();
            std::fs::create_dir_all(dir.path().join("Applications")).unwrap();
            Rig { dir, binary }
        }

        fn applications(&self) -> PathBuf {
            self.dir.path().join("Applications")
        }

        fn bundle(&self) -> PathBuf {
            bundle_path(&self.applications())
        }

        fn install(&self) -> Result<PathBuf> {
            install(&self.applications(), &self.binary, |_| Ok(()))
        }

        fn entries(&self) -> Vec<String> {
            let mut names: Vec<String> = std::fs::read_dir(self.applications())
                .unwrap()
                .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
                .collect();
            names.sort();
            names
        }

        fn plant_foreign(&self, plist: Option<&str>) {
            let contents = self.bundle().join("Contents");
            std::fs::create_dir_all(&contents).unwrap();
            if let Some(plist) = plist {
                std::fs::write(contents.join("Info.plist"), plist).unwrap();
            }
            std::fs::write(contents.join("theirs"), b"keep me").unwrap();
        }
    }

    #[test]
    fn install_lays_out_the_bundle() {
        let rig = Rig::new();
        let bundle = rig.install().unwrap();
        assert_eq!(bundle, rig.bundle());
        assert_eq!(
            bundle.file_name().unwrap().to_string_lossy(),
            format!("{}.app", crate::distribution::display_name())
        );
        let contents = bundle.join("Contents");
        assert!(contents.join("Info.plist").is_file());
        let binary = contents.join("MacOS").join("zellij");
        assert_eq!(std::fs::read(&binary).unwrap(), b"first build");
        assert_eq!(binary.metadata().unwrap().mode() & 0o111, 0o111);
        let icon = std::fs::read(contents.join("Resources").join("zellij.icns")).unwrap();
        assert_eq!(&icon[..4], b"icns");
        assert_eq!(
            rig.entries(),
            vec![format!("{}.app", crate::distribution::display_name())]
        );
    }

    #[test]
    fn the_plist_names_the_binary_as_the_executable() {
        let rig = Rig::new();
        let bundle = rig.install().unwrap();
        let plist = std::fs::read_to_string(bundle.join("Contents").join("Info.plist")).unwrap();
        assert_eq!(plist_string(&plist, "CFBundleExecutable"), Some("zellij"));
        assert_eq!(
            plist_string(&plist, "CFBundleIdentifier"),
            Some(crate::distribution::app_id())
        );
        assert_eq!(
            plist_string(&plist, "CFBundleShortVersionString"),
            Some(crate::distribution::distribution().version)
        );
        assert_eq!(
            plist_string(&plist, "CFBundleIconFile"),
            Some("zellij.icns")
        );
        assert_eq!(plist_string(&plist, "CFBundlePackageType"), Some("APPL"));
    }

    #[test]
    fn the_build_version_keeps_only_the_dotted_numbers() {
        assert_eq!(build_version("0.46.0"), "0.46.0");
        assert_eq!(build_version("0.46.0-rc.1"), "0.46.0");
        assert_eq!(build_version("0.46.0+abc"), "0.46.0");
    }

    #[test]
    fn the_binary_is_signed_before_the_bundle() {
        let rig = Rig::new();
        let signed = RefCell::new(vec![]);
        install(&rig.applications(), &rig.binary, |path| {
            signed.borrow_mut().push(path.to_path_buf());
            Ok(())
        })
        .unwrap();
        let signed = signed.into_inner();
        assert_eq!(signed.len(), 2);
        assert!(signed[0].ends_with("Contents/MacOS/zellij"), "{:?}", signed);
        assert_eq!(
            Some(signed[1].as_path()),
            signed[0].ancestors().nth(3),
            "the second call signs the bundle that holds the binary"
        );
    }

    #[test]
    fn reinstalling_replaces_by_rename() {
        let rig = Rig::new();
        let bundle = rig.install().unwrap();
        let binary = bundle.join("Contents").join("MacOS").join("zellij");
        let held_open = std::fs::File::open(&binary).unwrap();
        let old_inode = held_open.metadata().unwrap().ino();
        std::fs::write(&rig.binary, b"second build").unwrap();
        rig.install().unwrap();
        assert_eq!(std::fs::read(&binary).unwrap(), b"second build");
        assert_ne!(binary.metadata().unwrap().ino(), old_inode);
        let mut old = String::new();
        use std::io::Read;
        (&held_open).read_to_string(&mut old).unwrap();
        assert_eq!(old, "first build", "a running copy keeps its file");
        assert_eq!(
            rig.entries(),
            vec![format!("{}.app", crate::distribution::display_name())]
        );
    }

    #[test]
    fn a_failed_install_leaves_the_old_app_in_place() {
        let rig = Rig::new();
        let bundle = rig.install().unwrap();
        std::fs::write(&rig.binary, b"second build").unwrap();
        let result = install(&rig.applications(), &rig.binary, |_| {
            anyhow::bail!("codesign said no")
        });
        assert!(result.is_err());
        let binary = bundle.join("Contents").join("MacOS").join("zellij");
        assert_eq!(std::fs::read(binary).unwrap(), b"first build");
        assert_eq!(
            rig.entries(),
            vec![format!("{}.app", crate::distribution::display_name())]
        );
    }

    #[test]
    fn an_app_that_is_not_ours_is_never_replaced_or_removed() {
        let other = "<plist><dict><key>CFBundleIdentifier</key><string>com.example.Other</string></dict></plist>";
        for plist in [Some(other), None] {
            let rig = Rig::new();
            rig.plant_foreign(plist);
            assert!(rig.install().is_err(), "{:?}", plist);
            assert!(uninstall(&rig.applications()).is_err(), "{:?}", plist);
            assert_eq!(
                std::fs::read(rig.bundle().join("Contents").join("theirs")).unwrap(),
                b"keep me"
            );
            assert!(installed(&rig.applications()).is_none());
        }
    }

    #[test]
    fn installing_from_the_copy_inside_the_app_is_refused() {
        let rig = Rig::new();
        let bundle = rig.install().unwrap();
        let inside = bundle.join("Contents").join("MacOS").join("zellij");
        assert!(install(&rig.applications(), &inside, |_| Ok(())).is_err());
        assert_eq!(std::fs::read(inside).unwrap(), b"first build");
    }

    #[test]
    fn uninstall_removes_it_and_reports_nothing_when_absent() {
        let rig = Rig::new();
        assert_eq!(uninstall(&rig.applications()).unwrap(), None);
        let bundle = rig.install().unwrap();
        assert_eq!(
            uninstall(&rig.applications()).unwrap(),
            Some(bundle.clone())
        );
        assert!(!bundle.exists());
        assert!(rig.entries().is_empty());
    }

    #[test]
    fn leftovers_of_a_killed_install_are_cleared() {
        let rig = Rig::new();
        let name = format!("{}.app", crate::distribution::display_name());
        for leftover in [
            format!(".{name}.installing-123"),
            format!(".{name}.replaced-45"),
        ] {
            std::fs::create_dir_all(rig.applications().join(leftover)).unwrap();
        }
        std::fs::create_dir_all(rig.applications().join(format!(".{name}.installing-x"))).unwrap();
        rig.install().unwrap();
        assert_eq!(rig.entries(), vec![format!(".{name}.installing-x"), name]);
    }

    #[test]
    fn installed_reports_the_version_and_binary_size() {
        let rig = Rig::new();
        assert!(installed(&rig.applications()).is_none());
        rig.install().unwrap();
        assert_eq!(
            installed(&rig.applications()),
            Some(Installed {
                version: crate::distribution::distribution().version.to_owned(),
                build: build_of(&rig.binary).unwrap(),
            })
        );
    }

    #[test]
    fn installed_reports_the_size_of_the_binary_it_was_made_from_even_after_signing() {
        let rig = Rig::new();
        install(&rig.applications(), &rig.binary, |path| {
            if path.is_file() {
                use std::io::Write;
                let mut file = std::fs::OpenOptions::new().append(true).open(path)?;
                file.write_all(b" and a signature")?;
            }
            Ok(())
        })
        .unwrap();
        assert_eq!(
            installed(&rig.applications()).map(|app| app.build),
            build_of(&rig.binary)
        );
    }

    #[test]
    fn a_binary_inside_an_app_bundle_is_recognised() {
        assert!(is_inside_a_bundle(Path::new(
            "/x/Zellij.app/Contents/MacOS/zellij"
        )));
        assert!(is_inside_a_bundle(Path::new(
            "/Users/me/Applications/Other Name.app/Contents/MacOS/zellij"
        )));
        assert!(!is_inside_a_bundle(Path::new("/opt/homebrew/bin/zellij")));
        assert!(!is_inside_a_bundle(Path::new("/x/Zellij.app/zellij")));
        assert!(!is_inside_a_bundle(Path::new(
            "/x/Zellij/Contents/MacOS/zellij"
        )));
    }

    #[test]
    fn two_installs_at_once_both_succeed() {
        let rig = Rig::new();
        let slow_sign = |_: &Path| {
            std::thread::sleep(std::time::Duration::from_millis(200));
            Ok(())
        };
        let results = std::thread::scope(|scope| {
            let first = scope.spawn(|| install(&rig.applications(), &rig.binary, slow_sign));
            let second = scope.spawn(|| install(&rig.applications(), &rig.binary, slow_sign));
            [first.join().unwrap(), second.join().unwrap()]
        });
        for result in results {
            assert!(result.is_ok(), "{:?}", result);
        }
        assert_eq!(rig.entries(), vec![bundle_name()]);
    }

    #[test]
    fn a_symlink_to_an_app_binary_is_recognised() {
        let dir = tempfile::TempDir::new().unwrap();
        let inside = dir.path().join("Zellij.app/Contents/MacOS/zellij");
        std::fs::create_dir_all(inside.parent().unwrap()).unwrap();
        std::fs::write(&inside, b"zellij").unwrap();
        let link = dir.path().join("zellij");
        std::os::unix::fs::symlink(&inside, &link).unwrap();
        assert!(is_inside_a_bundle(&link));
    }

    #[test]
    fn the_app_binary_does_not_keep_the_download_quarantine() {
        let rig = Rig::new();
        let marked = std::process::Command::new("/usr/bin/xattr")
            .args(["-w", "com.apple.quarantine", "0081;00000000;Safari;"])
            .arg(&rig.binary)
            .status()
            .unwrap();
        assert!(marked.success());
        let bundle = rig.install().unwrap();
        let kept = std::process::Command::new("/usr/bin/xattr")
            .args(["-p", "com.apple.quarantine"])
            .arg(binary_in(&bundle))
            .output()
            .unwrap();
        assert!(!kept.status.success(), "the copy kept the quarantine");
    }

    #[test]
    fn a_really_signed_app_names_the_app_id_in_both_signatures() {
        let rig = Rig::new();
        std::fs::copy("/usr/bin/true", &rig.binary).unwrap();
        let bundle = install(&rig.applications(), &rig.binary, codesign).unwrap();
        assert_eq!(
            installed(&rig.applications()).map(|app| app.build),
            build_of(&rig.binary),
            "a signed app still matches the zellij it was made from"
        );
        for path in [
            bundle.join("Contents").join("MacOS").join("zellij"),
            bundle.clone(),
        ] {
            let output = Command::new(CODESIGN)
                .arg("-dv")
                .arg(&path)
                .output()
                .unwrap();
            let details = String::from_utf8_lossy(&output.stderr);
            assert!(
                details.contains(&format!("Identifier={}\n", app_id())),
                "{:?}: {}",
                path,
                details
            );
        }
    }

    #[test]
    fn codesign_runs_ad_hoc_with_the_app_id() {
        let command = codesign_command(Path::new("/tmp/Zellij.app"));
        assert_eq!(command.get_program(), CODESIGN);
        let args: Vec<_> = command
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            args,
            vec![
                "--force",
                "-s",
                "-",
                "--identifier",
                crate::distribution::app_id(),
                "/tmp/Zellij.app"
            ]
        );
    }
}
