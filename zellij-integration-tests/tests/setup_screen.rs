#![cfg(all(unix, not(target_os = "macos")))]

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use zellij_integration_tests::{
    default_timeout, keys, test_env, TestRunner, TestSession, PROMPT, TERMINAL_SIZE,
};
use zellij_utils::desktop_integration::{self, platform, Flag, Item};

const TITLE: &str = "Welcome to Zellij!";
const COPY: &str = "Copy Zellij to a Permanent Folder";
const ENTRY: &str = "Install Desktop Entry";
const GRAYED_NOTE: &str = "Grayed out items are already set up.";
const UNINSTALL_QUESTION: &str = "Uninstalling the desktop entry deletes:";

fn home() -> PathBuf {
    test_env::init().join("home")
}

fn assume_a_desktop_without_zellij_on_the_path(extra_path: Option<&Path>) {
    test_env::init();
    std::env::set_var(desktop_integration::DESKTOP_OVERRIDE_ENV, "on");
    std::env::remove_var("XDG_CURRENT_DESKTOP");
    let mut path = String::new();
    if let Some(extra) = extra_path {
        path.push_str(&extra.display().to_string());
        path.push(':');
    }
    path.push_str("/usr/bin:/bin");
    std::env::set_var("PATH", path);
}

fn fake_zellij_reporting(version: &str) -> PathBuf {
    let bin = home().join(".local").join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let path = bin.join("zellij");
    std::fs::write(&path, format!("#!/bin/sh\necho \"zellij {}\"\n", version)).unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    bin
}

fn copy_of_this_binary() -> PathBuf {
    let bin = home().join(".local").join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::copy(std::env::current_exe().unwrap(), bin.join("zellij")).unwrap();
    bin
}

fn start_first_run() -> TestSession {
    let zellij = TestRunner::new(TERMINAL_SIZE)
        .without_config_file()
        .with_setup_wizard()
        .start();
    zellij.expect_pty_spawn().output(PROMPT);
    zellij
}

fn start_existing_config() -> TestSession {
    let zellij = TestRunner::new(TERMINAL_SIZE).start();
    zellij.expect_pty_spawn().output(PROMPT);
    zellij
}

fn wait_for_setup_items(zellij: &TestSession) {
    zellij.wait_until("the setup screen shows its items", |grid_snapshot| {
        grid_snapshot.contains(TITLE) && !grid_snapshot.contains("Checking this computer")
    });
}

fn click_on(zellij: &TestSession, needle: &str, offset: usize) {
    let grid_snapshot = zellij.wait_until("the text to click is shown", |grid_snapshot| {
        grid_snapshot.contains(needle)
    });
    let (column, row) = grid_snapshot.position_of(needle).unwrap();
    zellij.send_stdin(&keys::sgr_click(column + offset, row));
}

fn wait_for(what: &str, condition: impl Fn() -> bool) {
    let deadline = Instant::now() + default_timeout();
    while !condition() {
        assert!(Instant::now() < deadline, "timed out waiting for {}", what);
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn open_setup_from_settings(zellij: &TestSession) {
    zellij.send_stdin(&keys::ctrl('o'));
    zellij.send_stdin(&keys::key('c'));
    zellij.wait_until("settings screen opened", |grid_snapshot| {
        grid_snapshot.contains("Configuration") && grid_snapshot.contains("unsaved change")
    });
    zellij.send_stdin(&keys::ctrl('w'));
    wait_for_setup_items(zellij);
}

fn install_the_desktop_entry_and_resolve_the_setup() -> PathBuf {
    let outcomes = platform::apply(&[Item::Launcher], &[], false);
    assert!(outcomes.iter().all(|o| o.succeeded), "{:?}", outcomes);
    desktop_integration::update(|state| state.set(Flag::SetupNeverShow)).unwrap();
    let entry = desktop_integration::location::user_launcher_path(
        &desktop_integration::location::data_home().unwrap(),
    );
    assert!(entry.is_file());
    entry
}

#[test]
fn an_older_zellij_on_the_path_is_offered_an_update_instead_of_a_copy() {
    let bin = fake_zellij_reporting("0.1.0");
    assume_a_desktop_without_zellij_on_the_path(Some(&bin));
    let mut zellij = start_first_run();
    wait_for_setup_items(&zellij);
    let expected = format!(
        "[x] Update Zellij from 0.1.0 to {}",
        zellij_utils::consts::VERSION
    );
    let grid_snapshot = zellij.wait_until("the update is offered ticked", |grid_snapshot| {
        grid_snapshot.contains(&expected)
    });
    assert!(grid_snapshot.contains(&format!("at {}", bin.join("zellij").display())));
    assert!(!grid_snapshot.contains(COPY));
    zellij.kill_session();
}

fn approve_with_nothing_ticked(zellij: &TestSession) {
    wait_for_setup_items(zellij);
    for label in [COPY, ENTRY] {
        let ticked = format!("[x] {}", label);
        if zellij.snapshot().contains(&ticked) {
            click_on(zellij, &ticked, 4);
            zellij.wait_until("the item is unticked", |grid_snapshot| {
                !grid_snapshot.contains(&ticked)
            });
        }
    }
    zellij.send_stdin(&keys::ENTER);
    zellij.wait_until("the setup screen closed", |grid_snapshot| {
        !grid_snapshot.contains(TITLE)
    });
}

fn assert_no_setup_screen(zellij: &TestSession) {
    zellij.send_stdin(&keys::ctrl('o'));
    zellij.send_stdin(&keys::key('w'));
    zellij.wait_until("session manager opened", |grid_snapshot| {
        grid_snapshot.contains("Session: ")
    });
    zellij.send_stdin(&keys::ESC);
    let grid_snapshot = zellij.wait_until("session manager closed", |grid_snapshot| {
        !grid_snapshot.contains("Session: ")
    });
    assert!(
        !grid_snapshot.contains(TITLE),
        "the setup screen came back:\n{}",
        grid_snapshot.text
    );
}

#[test]
fn a_dismissed_setup_screen_returns_in_the_next_session() {
    assume_a_desktop_without_zellij_on_the_path(None);
    let mut first = start_first_run();
    wait_for_setup_items(&first);
    first.send_stdin(&keys::ESC);
    first.wait_until("the setup screen closed", |grid_snapshot| {
        !grid_snapshot.contains(TITLE)
    });
    first.kill_session();
    drop(first);

    let mut second = start_existing_config();
    wait_for_setup_items(&second);
    second.kill_session();
}

#[test]
fn an_approved_setup_screen_does_not_return() {
    assume_a_desktop_without_zellij_on_the_path(None);
    let mut first = start_first_run();
    approve_with_nothing_ticked(&first);
    let state = desktop_integration::state_path();
    wait_for("the approval to be saved", || {
        desktop_integration::load_from(&state).has(Flag::SetupApproved)
    });
    first.kill_session();
    drop(first);

    let mut second = start_existing_config();
    assert_no_setup_screen(&second);
    second.kill_session();
}

#[test]
fn from_settings_set_up_items_are_grayed_out_and_cannot_be_unticked() {
    let bin = copy_of_this_binary();
    assume_a_desktop_without_zellij_on_the_path(Some(&bin));
    install_the_desktop_entry_and_resolve_the_setup();
    let mut zellij = start_existing_config();
    open_setup_from_settings(&zellij);
    let grid_snapshot = zellij.wait_until("both set-up lines are shown", |grid_snapshot| {
        grid_snapshot.contains(&format!("[x] {}", COPY))
            && grid_snapshot.contains(&format!("[x] {} [?] [Uninstall]", ENTRY))
            && grid_snapshot.contains(GRAYED_NOTE)
    });
    let (column, row) = grid_snapshot.position_of(COPY).unwrap();
    assert!(
        grid_snapshot.region_has_dim(column..column + COPY.len(), row..row + 1),
        "the copy line is not grayed out"
    );
    assert!(!grid_snapshot.contains("Don't Show Again"));
    click_on(&zellij, &format!("[x] {}", COPY), 4);
    zellij.send_stdin(&keys::key('?'));
    zellij.wait_until("the info popup says it is already done", |grid_snapshot| {
        grid_snapshot.contains("Zellij is already in a permanent folder")
    });
    zellij.send_stdin(&keys::ESC);
    let grid_snapshot = zellij.wait_until("the info popup closed", |grid_snapshot| {
        !grid_snapshot.contains("Zellij is already in a permanent folder")
    });
    assert!(grid_snapshot.contains(&format!("[x] {}", COPY)));
    zellij.kill_session();
}

#[test]
fn uninstall_asks_with_the_files_and_removes_the_desktop_entry_only_when_approved() {
    let bin = copy_of_this_binary();
    assume_a_desktop_without_zellij_on_the_path(Some(&bin));
    let entry = install_the_desktop_entry_and_resolve_the_setup();
    let mut zellij = start_existing_config();
    open_setup_from_settings(&zellij);
    zellij.wait_until("the uninstall button is shown", |grid_snapshot| {
        grid_snapshot.contains("[Uninstall]")
    });

    click_on(&zellij, "[Uninstall]", 1);
    zellij.wait_until("the uninstall question lists the entry", |grid_snapshot| {
        grid_snapshot.contains(UNINSTALL_QUESTION)
            && grid_snapshot.contains("dev.zellij.Zellij.desktop")
    });
    zellij.send_stdin(&keys::ESC);
    zellij.wait_until("the question closed", |grid_snapshot| {
        !grid_snapshot.contains(UNINSTALL_QUESTION)
    });
    assert!(entry.is_file(), "cancelling removed the entry");

    click_on(&zellij, "[Uninstall]", 1);
    zellij.wait_until("the uninstall question is back", |grid_snapshot| {
        grid_snapshot.contains(UNINSTALL_QUESTION)
    });
    zellij.send_stdin(&keys::ENTER);
    wait_for("the desktop entry to be removed", || !entry.exists());
    zellij.wait_until("the line can be ticked again", |grid_snapshot| {
        grid_snapshot.contains("The desktop entry was uninstalled.")
            && grid_snapshot.contains(&format!("[ ] {}", ENTRY))
            && !grid_snapshot.contains("[Uninstall]")
    });
    zellij.kill_session();
}

#[test]
fn the_result_lists_every_written_file_under_one_wrote_heading() {
    let bin = copy_of_this_binary();
    assume_a_desktop_without_zellij_on_the_path(Some(&bin));
    let mut zellij = start_first_run();
    wait_for_setup_items(&zellij);
    zellij.wait_until("the desktop entry is ticked", |grid_snapshot| {
        grid_snapshot.contains(&format!("[x] {}", ENTRY))
    });
    zellij.send_stdin(&keys::ENTER);
    let grid_snapshot = zellij.wait_until("the result lists the files", |grid_snapshot| {
        grid_snapshot.contains("All set")
            && grid_snapshot.contains("- ")
            && grid_snapshot.contains("dev.zellij.Zellij.desktop")
            && grid_snapshot.contains("dev.zellij.Zellij.png")
    });
    let wrote_lines = grid_snapshot
        .lines()
        .iter()
        .filter(|line| line.trim_matches(|c: char| c == ' ' || c == '│') == "Wrote")
        .count();
    assert_eq!(wrote_lines, 1, "{}", grid_snapshot.text);
    let bullets = grid_snapshot
        .lines()
        .iter()
        .filter(|line| line.contains("- ") && line.contains("dev.zellij.Zellij."))
        .count();
    assert_eq!(bullets, 4, "{}", grid_snapshot.text);
    zellij.kill_session();
}

#[test]
fn the_desktop_entry_needs_the_copy_while_zellij_runs_from_a_temporary_place() {
    assume_a_desktop_without_zellij_on_the_path(None);
    let mut zellij = start_first_run();
    wait_for_setup_items(&zellij);
    zellij.wait_until("both lines are ticked", |grid_snapshot| {
        grid_snapshot.contains(&format!("[x] {}", COPY))
            && grid_snapshot.contains(&format!("[x] {}", ENTRY))
    });
    click_on(&zellij, &format!("[x] {}", COPY), 4);
    zellij.wait_until("unticking the copy unticks the entry", |grid_snapshot| {
        grid_snapshot.contains(&format!("[ ] {}", COPY))
            && grid_snapshot.contains(&format!("[ ] {}", ENTRY))
    });
    click_on(&zellij, &format!("[ ] {}", ENTRY), 4);
    zellij.wait_until("ticking the entry ticks the copy again", |grid_snapshot| {
        grid_snapshot.contains(&format!("[x] {}", COPY))
            && grid_snapshot.contains(&format!("[x] {}", ENTRY))
    });
    zellij.kill_session();
}
