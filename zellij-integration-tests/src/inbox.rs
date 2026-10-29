use crate::attention::needs_you;
use crate::{
    keys, BackgroundTestSession, FakePtyHandle, GridSnapshot, TestClient, TestRunner, TestSession,
};

pub const INBOX_HELP: &str = "<?> more";
pub const INBOX_LIST_HELP: &str = "<?> all keys";
pub const ITEM_ACTIONS: &str = "<c> dismiss";
pub const GO_KEY: &str = "<ENTER> go";
pub const TYPE_INTO_IT: &str = "<Alt ENTER> type";
pub const TYPE_INTO_IT_HINT: &str = "<Alt ENTER> type into it";
pub const TYPING_INTO: &str = "typing into";
pub const RELEASE_KEY_HINT: &str = "<F9>";
pub const SESSIONS_HELP: &str = "<F1> all keys";
pub const ATTENTION_CONFIG: &str = "session_indicator true\nmouse_mode true";

pub fn open_inbox(zellij: &TestSession) -> GridSnapshot {
    zellij.send_stdin(&keys::F9);
    let opened = zellij.wait_until("the session manager opens", |grid_snapshot| {
        shows_inbox(grid_snapshot) || grid_snapshot.contains(SESSIONS_HELP)
    });
    if shows_inbox(&opened) {
        return opened;
    }
    zellij.send_stdin(&keys::TAB);
    zellij.wait_until("the inbox is shown", shows_inbox)
}

pub fn open_inbox_on(zellij: &TestSession, text: &str) -> GridSnapshot {
    zellij.send_stdin(&keys::F9);
    zellij.wait_until("the inbox selects the item", |grid_snapshot| {
        inbox_shows(grid_snapshot, text) && grid_snapshot.contains(ITEM_ACTIONS)
    })
}

pub fn shows_inbox(grid_snapshot: &GridSnapshot) -> bool {
    grid_snapshot.contains(INBOX_HELP) || grid_snapshot.contains(INBOX_LIST_HELP)
}

pub fn inbox_shows(grid_snapshot: &GridSnapshot, text: &str) -> bool {
    shows_inbox(grid_snapshot) && grid_snapshot.contains(text)
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

pub trait TypesKeys {
    fn type_bytes(&self, bytes: &[u8]);
}

impl TypesKeys for TestSession {
    fn type_bytes(&self, bytes: &[u8]) {
        self.send_stdin(bytes);
    }
}

impl TypesKeys for TestClient {
    fn type_bytes(&self, bytes: &[u8]) {
        self.send_stdin(bytes);
    }
}

pub fn press_esc(client: &impl TypesKeys) {
    client.type_bytes(&keys::ESC_KEY);
}

pub fn press_release_key(client: &impl TypesKeys) {
    client.type_bytes(&keys::F9);
}

pub fn press_type_key(client: &impl TypesKeys) {
    client.type_bytes(&keys::ALT_ENTER);
}
