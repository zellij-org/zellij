use std::fs::OpenOptions;
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::OnceLock;
use std::time::Duration;

use nix::fcntl::{flock, FlockArg};

const TEMP_PARENT: &str = "/tmp/zellij-test";
const TEST_ROOT_SUBDIRS: [&str; 8] = [
    "sock", "home", "cache", "data", "config", "runtime", "cwd", "tmp",
];

static TEST_ROOT: OnceLock<PathBuf> = OnceLock::new();
static SESSION_COUNTER: AtomicUsize = AtomicUsize::new(0);

pub fn init() -> &'static Path {
    TEST_ROOT
        .get_or_init(|| {
            let test_root = create_test_root();
            isolate_process_environment(&test_root);
            zellij_utils::logging::configure_logger();
            test_root
        })
        .as_path()
}

fn create_test_root() -> PathBuf {
    let parent = PathBuf::from(TEMP_PARENT);
    std::fs::create_dir_all(&parent).unwrap();
    remove_roots_of_dead_test_processes(&parent);
    let test_root = parent.join(std::process::id().to_string());
    for subdir in TEST_ROOT_SUBDIRS {
        std::fs::create_dir_all(test_root.join(subdir)).unwrap();
    }
    test_root
}

fn remove_roots_of_dead_test_processes(parent: &Path) {
    let Ok(entries) = std::fs::read_dir(parent) else {
        return;
    };
    for entry in entries.flatten() {
        let entry_name = entry.file_name();
        let Some(owner_pid) = entry_name
            .to_str()
            .and_then(|name| name.parse::<u32>().ok())
        else {
            continue;
        };
        if owner_pid != std::process::id() && !process_is_alive(owner_pid) {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}

fn process_is_alive(pid: u32) -> bool {
    Path::new(&format!("/proc/{}", pid)).exists()
}

fn isolate_process_environment(test_root: &Path) {
    std::env::set_var("TMPDIR", test_root.join("tmp"));
    std::env::set_var("ZELLIJ_SOCKET_DIR", test_root.join("sock"));
    std::env::set_var("HOME", test_root.join("home"));
    std::env::set_var("XDG_CACHE_HOME", test_root.join("cache"));
    std::env::set_var("XDG_DATA_HOME", test_root.join("data"));
    std::env::set_var("XDG_CONFIG_HOME", test_root.join("config"));
    std::env::set_var("XDG_RUNTIME_DIR", test_root.join("runtime"));
    std::env::remove_var("ZELLIJ");
    std::env::remove_var("ZELLIJ_SESSION_NAME");
    std::env::remove_var("ZELLIJ_CONFIG_FILE");
    std::env::remove_var("ZELLIJ_CONFIG_DIR");
    std::env::set_current_dir(test_root.join("cwd")).unwrap();
}

pub fn unique_session_name() -> String {
    let session_index = SESSION_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("test-{}", session_index)
}

// caps concurrently active tests so nextest's per-cpu parallelism does not oversubscribe the
// machine (each test is itself many threads) and miss render deadlines; flock frees on panic
pub struct ConcurrencySlot {
    _slot_file: std::fs::File,
}

fn is_truthy(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes"
    )
}

fn slot_count() -> usize {
    if std::env::var("ZELLIJ_TEST_SERIAL")
        .map(|v| is_truthy(&v))
        .unwrap_or(false)
    {
        return 1;
    }
    if let Some(explicit) = std::env::var("ZELLIJ_TEST_CONCURRENCY")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
    {
        return explicit.max(1);
    }
    let cpus = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);
    (cpus / 4).max(1)
}

pub fn acquire_concurrency_slot() -> ConcurrencySlot {
    let slots = slot_count();
    let slot_dir = PathBuf::from(TEMP_PARENT).join("concurrency-slots");
    std::fs::create_dir_all(&slot_dir).unwrap();
    loop {
        for slot_index in 0..slots {
            let slot_file = OpenOptions::new()
                .create(true)
                .write(true)
                .open(slot_dir.join(format!("slot-{}", slot_index)))
                .unwrap();
            if flock(slot_file.as_raw_fd(), FlockArg::LockExclusiveNonblock).is_ok() {
                return ConcurrencySlot {
                    _slot_file: slot_file,
                };
            }
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

pub const DEFAULT_TEST_CONFIG: &str = r#"
show_startup_tips false
show_release_notes false
session_serialization false
disable_session_metadata true
visual_bell false
mouse_mode false
advanced_mouse_actions false
theme "default"
"#;

pub fn write_config(session_name: &str, extra_config_kdl: &str) -> PathBuf {
    let test_root = init();
    let config_path = test_root
        .join("config")
        .join(format!("{}-config.kdl", session_name));
    let contents = format!("{}\n{}\n", DEFAULT_TEST_CONFIG, extra_config_kdl);
    std::fs::write(&config_path, contents).unwrap();
    config_path
}

pub fn log_file_path() -> PathBuf {
    init();
    zellij_utils::consts::ZELLIJ_TMP_LOG_FILE.clone()
}

pub fn log_tail(max_lines: usize) -> String {
    let contents = std::fs::read_to_string(log_file_path()).unwrap_or_default();
    let lines: Vec<&str> = contents.lines().collect();
    let start = lines.len().saturating_sub(max_lines);
    lines[start..].join("\n")
}
