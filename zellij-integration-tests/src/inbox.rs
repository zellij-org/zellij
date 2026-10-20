use crate::attention::needs_you;
use crate::{keys, BackgroundTestSession, FakePtyHandle, GridSnapshot, TestRunner, TestSession};

pub const INBOX_HELP: &str = "<?> all keys";
pub const SESSIONS_HELP: &str = "<F1> all keys";
pub const ATTENTION_CONFIG: &str = "session_indicator true\nmouse_mode true";

pub fn open_inbox(zellij: &TestSession) -> GridSnapshot {
    zellij.send_stdin(&keys::F9);
    let opened = zellij.wait_until("the session manager opens", |grid_snapshot| {
        grid_snapshot.contains(INBOX_HELP) || grid_snapshot.contains(SESSIONS_HELP)
    });
    if opened.contains(INBOX_HELP) {
        return opened;
    }
    zellij.send_stdin(&keys::TAB);
    zellij.wait_until("the inbox is shown", |grid_snapshot| {
        grid_snapshot.contains(INBOX_HELP)
    })
}

pub fn open_inbox_on(zellij: &TestSession, text: &str) -> GridSnapshot {
    zellij.send_stdin(&keys::F9);
    zellij.wait_until("the inbox selects the item", |grid_snapshot| {
        inbox_shows(grid_snapshot, text) && grid_snapshot.contains("<ENTER> go there")
    })
}

pub fn inbox_shows(grid_snapshot: &GridSnapshot, text: &str) -> bool {
    grid_snapshot.contains(INBOX_HELP) && grid_snapshot.contains(text)
}

pub fn background_session_with(config: &str) -> (BackgroundTestSession, FakePtyHandle) {
    let background = TestRunner::new(crate::TERMINAL_SIZE)
        .with_config(config)
        .skip_concurrency_slot()
        .start_in_background();
    let terminal = background.expect_pty_spawn();
    terminal.disable_echo();
    terminal.output(b"$ ");
    (background, terminal)
}

pub fn session_needing_you(text: &str) -> (BackgroundTestSession, FakePtyHandle) {
    let (background, terminal) = background_session_with(ATTENTION_CONFIG);
    terminal.output(&needs_you(text));
    (background, terminal)
}
