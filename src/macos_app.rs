//! Zellij.app started from Finder, the Dock or Spotlight. `double_click` turns that start into
//! a window; this gives it the environment launchd leaves out: the login shell's variables, and
//! a UTF-8 LANG when none is set.
//!
//! If a `zellij terminal` started outside the app is already running, the app hands over to
//! it. Notifications then won't work until that one is quit, because macOS only allows them
//! for the signed binary inside the app.

use std::ffi::OsString;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use zellij_utils::desktop_integration::macos_bundle::is_inside_a_bundle;

const LOGIN_SHELL_LIMIT: Duration = Duration::from_secs(10);
const FALLBACK_SHELL: &str = "/bin/zsh";
const FALLBACK_LANG: &str = "en_US.UTF-8";
const LOCALES: &str = "/usr/share/locale";
// Variables the shell sets for itself, which we don't want to copy over.
const SHELL_OWN: [&str; 4] = ["PWD", "OLDPWD", "SHLVL", "_"];

/// Must be called before any threads are spawned, since it calls `set_var`. Returns why the
/// login environment couldn't be read, to log once logging is up.
pub(crate) fn adopt_login_environment() -> Option<String> {
    let exe = std::env::current_exe().ok()?;
    let launch = crate::double_click::current_launch();
    if !is_inside_a_bundle(&exe) || !crate::double_click::opens_window(launch) {
        return None;
    }
    let shell = std::env::var_os("SHELL")
        .filter(|shell| !shell.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(FALLBACK_SHELL));
    let problem = match login_environment(&shell, LOGIN_SHELL_LIMIT) {
        Ok(environment) => {
            for (key, value) in environment {
                std::env::set_var(key, value);
            }
            None
        },
        Err(reason) => Some(format!(
            "the login environment could not be read, continuing without it: {}",
            reason
        )),
    };
    // launchd doesn't set LANG, so without this, shells fall back to the C locale.
    if needs_a_locale(|key| std::env::var_os(key)) {
        let lang = lang_for(&macos_locale().unwrap_or_default(), |name| {
            Path::new(LOCALES).join(name).is_dir()
        });
        std::env::set_var("LANG", lang);
    }
    problem
}

fn needs_a_locale(get: impl Fn(&str) -> Option<OsString>) -> bool {
    ["LANG", "LC_ALL", "LC_CTYPE"]
        .iter()
        .all(|key| get(key).map(|value| value.is_empty()).unwrap_or(true))
}

// The language and region from System Settings, e.g. `de_DE` or `en_US@rg=dezzzz`.
fn macos_locale() -> Option<String> {
    let output = Command::new("/usr/bin/defaults")
        .args(["read", "-g", "AppleLocale"])
        .stderr(Stdio::null())
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn lang_for(macos_locale: &str, exists: impl Fn(&str) -> bool) -> String {
    let base = macos_locale.split('@').next().unwrap_or_default();
    let candidate = format!("{}.UTF-8", base);
    if !base.is_empty() && exists(&candidate) {
        candidate
    } else {
        FALLBACK_LANG.to_owned()
    }
}

fn login_environment(shell: &Path, limit: Duration) -> Result<Vec<(OsString, OsString)>, String> {
    // The shell writes its environment to a file rather than to us through a pipe, so whatever
    // the user's profile prints or leaves running in the background doesn't get in the way.
    static CALLS: AtomicUsize = AtomicUsize::new(0);
    let file = std::env::temp_dir().join(format!(
        "zellij-login-env-{}-{}",
        std::process::id(),
        CALLS.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_file(&file);
    let mut child = Command::new(shell)
        .arg("-l")
        .arg("-c")
        .arg(format!("env -0 > '{}'", file.display()))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("{} could not be started: {}", shell.display(), e))?;
    let deadline = Instant::now() + limit;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(10)),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = std::fs::remove_file(&file);
                return Err(format!(
                    "{} did not finish within {} seconds",
                    shell.display(),
                    limit.as_secs()
                ));
            },
            Err(e) => {
                return Err(format!(
                    "{} could not be waited for: {}",
                    shell.display(),
                    e
                ))
            },
        }
    };
    let output = std::fs::read(&file).unwrap_or_default();
    let _ = std::fs::remove_file(&file);
    if !status.success() {
        return Err(format!("{} failed ({})", shell.display(), status));
    }
    parse_env(&output)
}

fn parse_env(output: &[u8]) -> Result<Vec<(OsString, OsString)>, String> {
    let environment: Vec<(OsString, OsString)> = output
        .split(|byte| *byte == 0)
        .filter_map(|entry| {
            let split = entry.iter().position(|byte| *byte == b'=')?;
            let (key, value) = (&entry[..split], &entry[split + 1..]);
            let key = std::str::from_utf8(key).ok()?;
            if key.is_empty() || SHELL_OWN.contains(&key) {
                return None;
            }
            Some((
                OsString::from(key),
                std::ffi::OsStr::from_bytes(value).to_os_string(),
            ))
        })
        .collect();
    if environment.is_empty() {
        return Err("the login shell printed no environment".to_owned());
    }
    Ok(environment)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use std::time::Instant;

    fn fake_shell(dir: &tempfile::TempDir, body: &str) -> PathBuf {
        let path = dir.path().join("fake-shell");
        std::fs::write(&path, format!("#!/bin/sh\n{}\n", body)).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    fn pair(key: &str, value: &str) -> (OsString, OsString) {
        (OsString::from(key), OsString::from(value))
    }

    #[test]
    fn a_locale_is_needed_only_when_none_is_set() {
        let set = |pairs: &[(&str, &str)]| {
            let pairs: Vec<(String, String)> = pairs
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect();
            move |key: &str| {
                pairs
                    .iter()
                    .find(|(k, _)| k == key)
                    .map(|(_, v)| OsString::from(v))
            }
        };
        assert!(needs_a_locale(set(&[])));
        assert!(needs_a_locale(set(&[("LANG", "")])));
        assert!(!needs_a_locale(set(&[("LANG", "de_DE.UTF-8")])));
        assert!(!needs_a_locale(set(&[("LC_ALL", "C")])));
        assert!(!needs_a_locale(set(&[("LC_CTYPE", "UTF-8")])));
    }

    #[test]
    fn the_locale_follows_the_macos_setting_when_it_exists() {
        let only = |name: &'static str| move |candidate: &str| candidate == name;
        assert_eq!(lang_for("de_DE", only("de_DE.UTF-8")), "de_DE.UTF-8");
        assert_eq!(
            lang_for("de_DE@rg=chzzzz", only("de_DE.UTF-8")),
            "de_DE.UTF-8"
        );
        assert_eq!(lang_for("en_IT", only("de_DE.UTF-8")), "en_US.UTF-8");
        assert_eq!(lang_for("", only("de_DE.UTF-8")), "en_US.UTF-8");
    }

    // Runs `before`, then the command it's given, the way `$SHELL -l -c <command>` would.
    fn fake_login_shell(dir: &tempfile::TempDir, before: &str) -> PathBuf {
        fake_shell(dir, &format!("{}\n/bin/sh -c \"$3\"", before))
    }

    fn has(environment: &[(OsString, OsString)], key: &str, value: &str) -> bool {
        environment.contains(&pair(key, value))
    }

    #[test]
    fn parse_env_reads_every_entry() {
        assert_eq!(
            parse_env(b"A=1\0B=two\nlines\0C=x=y\0"),
            Ok(vec![
                pair("A", "1"),
                pair("B", "two\nlines"),
                pair("C", "x=y")
            ])
        );
    }

    #[test]
    fn parse_env_drops_what_belongs_to_the_shell_that_printed_it() {
        assert_eq!(
            parse_env(b"PWD=/x\0OLDPWD=/y\0SHLVL=2\0_=/usr/bin/env\0KEEP=1\0"),
            Ok(vec![pair("KEEP", "1")])
        );
    }

    #[test]
    fn parse_env_skips_entries_without_a_name() {
        assert_eq!(
            parse_env(b"novalue\0=empty\0OK=1\0"),
            Ok(vec![pair("OK", "1")])
        );
    }

    #[test]
    fn an_empty_environment_is_a_failure() {
        let failure = parse_env(b"").unwrap_err();
        assert!(failure.contains("no environment"), "{}", failure);
    }

    #[test]
    fn the_login_shells_environment_is_read() {
        let dir = tempfile::TempDir::new().unwrap();
        let shell = fake_login_shell(&dir, "echo welcome; export ZELLIJ_TEST_A=1");
        let environment = login_environment(&shell, Duration::from_secs(5)).unwrap();
        assert!(has(&environment, "ZELLIJ_TEST_A", "1"), "{:?}", environment);
    }

    #[test]
    fn a_background_job_from_the_profile_does_not_hold_up_the_start() {
        let dir = tempfile::TempDir::new().unwrap();
        let shell = fake_login_shell(
            &dir,
            "(i=0; while [ $i -lt 300 ]; do printf x; sleep 0.01; i=$((i+1)); done) & \
             export ZELLIJ_TEST_A=1",
        );
        let started = Instant::now();
        let environment = login_environment(&shell, Duration::from_secs(5)).unwrap();
        assert!(has(&environment, "ZELLIJ_TEST_A", "1"), "{:?}", environment);
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "took {:?}",
            started.elapsed()
        );
    }

    #[test]
    fn a_login_shell_that_hangs_is_given_up_on() {
        let dir = tempfile::TempDir::new().unwrap();
        let shell = fake_shell(&dir, "exec sleep 30");
        let started = Instant::now();
        let failure = login_environment(&shell, Duration::from_millis(300)).unwrap_err();
        assert!(started.elapsed() < Duration::from_secs(5));
        assert!(failure.contains("did not finish"), "{}", failure);
    }

    #[test]
    fn a_failed_start_from_the_app_is_explained() {
        let dir = tempfile::TempDir::new().unwrap();
        let missing = dir.path().join("no-such-shell");
        let failure = login_environment(&missing, Duration::from_secs(5)).unwrap_err();
        assert!(failure.contains("no-such-shell"), "{}", failure);
        let failing = fake_login_shell(&dir, "exit 3");
        let failure = login_environment(&failing, Duration::from_secs(5)).unwrap_err();
        assert!(failure.contains("exit status: 3"), "{}", failure);
    }

    #[test]
    fn a_login_shell_that_writes_nothing_gives_nothing() {
        let dir = tempfile::TempDir::new().unwrap();
        let shell = fake_shell(&dir, "true");
        assert!(login_environment(&shell, Duration::from_secs(5)).is_err());
    }

    #[test]
    fn the_real_login_shell_is_asked_for_env() {
        let dir = tempfile::TempDir::new().unwrap();
        let shell = fake_login_shell(&dir, "printf '%s\\n' \"$@\" > \"$0.args\"");
        let _ = login_environment(&shell, Duration::from_secs(5));
        let recorded = std::fs::read_to_string(dir.path().join("fake-shell.args")).unwrap();
        let recorded: Vec<&str> = recorded.lines().collect();
        assert_eq!(recorded[0], "-l");
        assert_eq!(recorded[1], "-c");
        assert!(recorded[2].starts_with("env -0 > "), "{:?}", recorded);
    }
}
