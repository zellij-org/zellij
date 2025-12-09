# Implementation Plan: Add Tests for New Plugin APIs

## Overview

This plan details how to add comprehensive tests for 5 new plugin APIs that were added to the `macros-initial` branch:
1. `show_cursor` - Shows/hides cursor at position
2. `run_action` - Executes arbitrary Zellij actions
3. `copy_to_clipboard` - Copies text to clipboard
4. `send_sigint_to_pane_id` - Sends SIGINT to terminal pane
5. `send_sigkill_to_pane_id` - Sends SIGKILL to terminal pane

## Background: New Plugin APIs (from shim.rs)

### 1. show_cursor
- **Location**: `zellij-tile/src/shim.rs:51-60`
- **Signature**: `pub fn show_cursor(cursor_position: Option<(usize, usize)>)`
- **Parameters**: `Some((x, y))` shows cursor at coordinates, `None` hides it
- **Permissions**: None required
- **Server-side**: Sends `ScreenInstruction::ShowPluginCursor` to screen thread

### 2. run_action
- **Location**: `zellij-tile/src/shim.rs:1678-1687`
- **Signature**: `pub fn run_action(action: Action, context: BTreeMap<String, String>)`
- **Parameters**: Any `Action` enum variant (100+ types), plus context metadata
- **Permissions**: `PermissionType::RunActionsAsUser` (required)
- **Server-side**: Routes action via `route_action()` in spawned thread

### 3. copy_to_clipboard
- **Location**: `zellij-tile/src/shim.rs:575-584`
- **Signature**: `pub fn copy_to_clipboard(text: impl Into<String>)`
- **Parameters**: Text to copy
- **Permissions**: `PermissionType::WriteToClipboard` (required)
- **Server-side**: Sends `ScreenInstruction::CopyTextToClipboard`

### 4. send_sigint_to_pane_id
- **Location**: `zellij-tile/src/shim.rs:1127-1144`
- **Signature**: `pub fn send_sigint_to_pane_id(pane_id: PaneId)`
- **Parameters**: `PaneId::Terminal(u32)` or `PaneId::Plugin(u32)` (only Terminal works)
- **Permissions**: `PermissionType::ChangeApplicationState` (required)
- **Server-side**: Sends `PtyInstruction::SendSigintToPaneId` to PTY thread

### 5. send_sigkill_to_pane_id
- **Location**: `zellij-tile/src/shim.rs:1146-1163`
- **Signature**: `pub fn send_sigkill_to_pane_id(pane_id: PaneId)`
- **Parameters**: `PaneId::Terminal(u32)` or `PaneId::Plugin(u32)` (only Terminal works)
- **Permissions**: `PermissionType::ChangeApplicationState` (required)
- **Server-side**: Sends `PtyInstruction::SendSigkillToPaneId` to PTY thread

## Background: Test Infrastructure

### Test Locations
- **Primary test file**: `zellij-server/src/plugins/unit/plugin_tests.rs` (109+ existing tests)
- **Fixture plugin**: `default-plugins/fixture-plugin-for-tests/src/main.rs`
- **Snapshots**: `zellij-server/src/plugins/unit/snapshots/` (auto-generated)

### Test Pattern
All 109+ existing tests follow this pattern:

1. Create plugin thread with `create_plugin_thread()` or variant
2. Set up logging thread with helper macros (e.g., `grant_permissions_and_log_actions_in_thread!`)
3. Load the fixture plugin
4. Send a key event to trigger the API call
5. Wait for expected instruction to appear
6. Extract and verify result via snapshot testing

### Helper Functions Available
- `create_plugin_thread()` - Standard setup, exposes ScreenInstruction receiver
- `create_plugin_thread_with_pty_receiver()` - Also exposes PtyInstruction receiver
- `create_plugin_thread_with_server_receiver()` - Also exposes ServerInstruction receiver
- `create_plugin_thread_with_background_jobs_receiver()` - Also exposes BackgroundJob receiver

### Logging Macros Available
- `log_actions_in_thread!` - Logs instructions until exit condition
- `grant_permissions_and_log_actions_in_thread!` - Auto-grants permissions and logs
- `deny_permissions_and_log_actions_in_thread!` - Auto-denies permissions and logs

### Fixture Plugin Pattern
The fixture plugin maps key presses to plugin API calls. For example:
- `'a'` → `switch_to_mode(InputMode::Tab)`
- `'b'` → `new_tabs_with_layout(...)`
- etc.

We'll add new key mappings for the new APIs.

## Implementation Plan

### Step 1: Update Fixture Plugin

**File**: `default-plugins/fixture-plugin-for-tests/src/main.rs`

**Location**: In the `update()` function's key handler match statement (around line 170-400)

**Add these key mappings** in the `Event::Key(key)` match block:

```rust
KeyWithModifier::new(BareKey::Char('C')) => {
    // Test show_cursor with coordinates
    show_cursor(Some((5, 10)));
},
KeyWithModifier::new(BareKey::Char('D')) => {
    // Test hide_cursor
    show_cursor(None);
},
KeyWithModifier::new(BareKey::Char('E')) => {
    // Test copy_to_clipboard
    copy_to_clipboard("test clipboard text");
},
KeyWithModifier::new(BareKey::Char('F')) => {
    // Test run_action with MoveFocus
    let mut context = BTreeMap::new();
    context.insert("test_key".to_string(), "test_value".to_string());
    run_action(Action::MoveFocus(Direction::Left), context);
},
KeyWithModifier::new(BareKey::Char('G')) => {
    // Test send_sigint_to_pane_id
    send_sigint_to_pane_id(PaneId::Terminal(1));
},
KeyWithModifier::new(BareKey::Char('H')) => {
    // Test send_sigkill_to_pane_id
    send_sigkill_to_pane_id(PaneId::Terminal(1));
},
```

**Import requirements**: Ensure these are imported at the top of the file:
- `use zellij_tile::prelude::*;` (should already be there)
- `use std::collections::BTreeMap;` (should already be there)
- Verify `Action`, `Direction`, `PaneId` are accessible via prelude

### Step 2: Add Test Functions

**File**: `zellij-server/src/plugins/unit/plugin_tests.rs`

**Add 8 new test functions** at the end of the file (after line 9139):

#### Test 2.1: show_cursor_plugin_command

```rust
#[test]
#[ignore]
pub fn show_cursor_plugin_command() {
    let temp_folder = tempdir().unwrap();
    let plugin_host_folder = PathBuf::from(temp_folder.path());
    let (plugin_thread_sender, screen_receiver, teardown) =
        create_plugin_thread(Some(plugin_host_folder));

    let received_screen_instructions = Arc::new(Mutex::new(vec![]));
    let client_id = 1;

    let screen_thread = log_actions_in_thread!(
        received_screen_instructions,
        ScreenInstruction::ShowPluginCursor,
        screen_receiver,
        1
    );

    let plugin_should_float = Some(false);
    let plugin_title = Some("test_plugin".to_owned());
    let run_plugin = RunPluginOrAlias::RunPlugin(RunPlugin {
        _allow_exec_host_cmd: false,
        location: RunPluginLocation::File(PathBuf::from(&*PLUGIN_FIXTURE)),
        configuration: Default::default(),
        ..Default::default()
    });

    let _ = plugin_thread_sender.send(PluginInstruction::AddClient(client_id));
    let _ = plugin_thread_sender.send(PluginInstruction::Load(
        Some(LoadingPluginId::new(1)),
        run_plugin,
        1,
        client_id,
        plugin_should_float,
        None,
        plugin_title,
        None,
        None,
        None,
        None,
        None,
    ));

    std::thread::sleep(std::time::Duration::from_millis(500));

    let _ = plugin_thread_sender.send(PluginInstruction::Update(vec![(
        None,
        Some(client_id),
        Event::Key(KeyWithModifier::new(BareKey::Char('C'))),
    )]));

    screen_thread.join().unwrap();
    teardown();

    let show_cursor_instruction = received_screen_instructions
        .lock()
        .unwrap()
        .iter()
        .find_map(|i| {
            if let ScreenInstruction::ShowPluginCursor(plugin_id, client_id, cursor_position) = i {
                Some((*plugin_id, *client_id, *cursor_position))
            } else {
                None
            }
        });

    assert_snapshot!(format!("{:#?}", show_cursor_instruction));
}
```

#### Test 2.2: hide_cursor_plugin_command

```rust
#[test]
#[ignore]
pub fn hide_cursor_plugin_command() {
    let temp_folder = tempdir().unwrap();
    let plugin_host_folder = PathBuf::from(temp_folder.path());
    let (plugin_thread_sender, screen_receiver, teardown) =
        create_plugin_thread(Some(plugin_host_folder));

    let received_screen_instructions = Arc::new(Mutex::new(vec![]));
    let client_id = 1;

    let screen_thread = log_actions_in_thread!(
        received_screen_instructions,
        ScreenInstruction::ShowPluginCursor,
        screen_receiver,
        1
    );

    let plugin_should_float = Some(false);
    let plugin_title = Some("test_plugin".to_owned());
    let run_plugin = RunPluginOrAlias::RunPlugin(RunPlugin {
        _allow_exec_host_cmd: false,
        location: RunPluginLocation::File(PathBuf::from(&*PLUGIN_FIXTURE)),
        configuration: Default::default(),
        ..Default::default()
    });

    let _ = plugin_thread_sender.send(PluginInstruction::AddClient(client_id));
    let _ = plugin_thread_sender.send(PluginInstruction::Load(
        Some(LoadingPluginId::new(1)),
        run_plugin,
        1,
        client_id,
        plugin_should_float,
        None,
        plugin_title,
        None,
        None,
        None,
        None,
        None,
    ));

    std::thread::sleep(std::time::Duration::from_millis(500));

    let _ = plugin_thread_sender.send(PluginInstruction::Update(vec![(
        None,
        Some(client_id),
        Event::Key(KeyWithModifier::new(BareKey::Char('D'))),
    )]));

    screen_thread.join().unwrap();
    teardown();

    let hide_cursor_instruction = received_screen_instructions
        .lock()
        .unwrap()
        .iter()
        .find_map(|i| {
            if let ScreenInstruction::ShowPluginCursor(plugin_id, client_id, cursor_position) = i {
                Some((*plugin_id, *client_id, *cursor_position))
            } else {
                None
            }
        });

    assert_snapshot!(format!("{:#?}", hide_cursor_instruction));
}
```

#### Test 2.3: copy_to_clipboard_plugin_command

```rust
#[test]
#[ignore]
pub fn copy_to_clipboard_plugin_command() {
    let temp_folder = tempdir().unwrap();
    let plugin_host_folder = PathBuf::from(temp_folder.path());
    let cache_path = plugin_host_folder.join("permissions_test.kdl");
    let (plugin_thread_sender, screen_receiver, teardown) =
        create_plugin_thread(Some(plugin_host_folder));

    let received_screen_instructions = Arc::new(Mutex::new(vec![]));
    let client_id = 1;

    let screen_thread = grant_permissions_and_log_actions_in_thread!(
        received_screen_instructions,
        ScreenInstruction::CopyTextToClipboard,
        screen_receiver,
        1,
        &PermissionType::WriteToClipboard,
        cache_path,
        plugin_thread_sender,
        client_id
    );

    let plugin_should_float = Some(false);
    let plugin_title = Some("test_plugin".to_owned());
    let run_plugin = RunPluginOrAlias::RunPlugin(RunPlugin {
        _allow_exec_host_cmd: false,
        location: RunPluginLocation::File(PathBuf::from(&*PLUGIN_FIXTURE)),
        configuration: Default::default(),
        ..Default::default()
    });

    let _ = plugin_thread_sender.send(PluginInstruction::AddClient(client_id));
    let _ = plugin_thread_sender.send(PluginInstruction::Load(
        Some(LoadingPluginId::new(1)),
        run_plugin,
        1,
        client_id,
        plugin_should_float,
        None,
        plugin_title,
        None,
        None,
        None,
        None,
        None,
    ));

    std::thread::sleep(std::time::Duration::from_millis(500));

    let _ = plugin_thread_sender.send(PluginInstruction::Update(vec![(
        None,
        Some(client_id),
        Event::Key(KeyWithModifier::new(BareKey::Char('E'))),
    )]));

    screen_thread.join().unwrap();
    teardown();

    let copy_instruction = received_screen_instructions
        .lock()
        .unwrap()
        .iter()
        .find_map(|i| {
            if let ScreenInstruction::CopyTextToClipboard(text, plugin_id) = i {
                Some((text.clone(), *plugin_id))
            } else {
                None
            }
        });

    assert_snapshot!(format!("{:#?}", copy_instruction));
}
```

#### Test 2.4: run_action_plugin_command

```rust
#[test]
#[ignore]
pub fn run_action_plugin_command() {
    let temp_folder = tempdir().unwrap();
    let plugin_host_folder = PathBuf::from(temp_folder.path());
    let cache_path = plugin_host_folder.join("permissions_test.kdl");
    let (plugin_thread_sender, screen_receiver, teardown) =
        create_plugin_thread(Some(plugin_host_folder));

    let received_screen_instructions = Arc::new(Mutex::new(vec![]));
    let client_id = 1;

    // MoveFocus(Left) should generate ScreenInstruction::MoveFocus or similar
    // We'll log all screen instructions and filter for the relevant one
    let screen_thread = grant_permissions_and_log_actions_in_thread_naked_variant!(
        received_screen_instructions,
        ScreenInstruction::MoveFocus,
        screen_receiver,
        1,
        &PermissionType::RunActionsAsUser,
        cache_path,
        plugin_thread_sender,
        client_id
    );

    let plugin_should_float = Some(false);
    let plugin_title = Some("test_plugin".to_owned());
    let run_plugin = RunPluginOrAlias::RunPlugin(RunPlugin {
        _allow_exec_host_cmd: false,
        location: RunPluginLocation::File(PathBuf::from(&*PLUGIN_FIXTURE)),
        configuration: Default::default(),
        ..Default::default()
    });

    let _ = plugin_thread_sender.send(PluginInstruction::AddClient(client_id));
    let _ = plugin_thread_sender.send(PluginInstruction::Load(
        Some(LoadingPluginId::new(1)),
        run_plugin,
        1,
        client_id,
        plugin_should_float,
        None,
        plugin_title,
        None,
        None,
        None,
        None,
        None,
    ));

    std::thread::sleep(std::time::Duration::from_millis(500));

    let _ = plugin_thread_sender.send(PluginInstruction::Update(vec![(
        None,
        Some(client_id),
        Event::Key(KeyWithModifier::new(BareKey::Char('F'))),
    )]));

    screen_thread.join().unwrap();
    teardown();

    let move_focus_instruction = received_screen_instructions
        .lock()
        .unwrap()
        .iter()
        .find_map(|i| {
            if let ScreenInstruction::MoveFocus(direction, client_id) = i {
                Some((*direction, *client_id))
            } else {
                None
            }
        });

    assert_snapshot!(format!("{:#?}", move_focus_instruction));
}
```

#### Test 2.5: send_sigint_to_pane_id_plugin_command

```rust
#[test]
#[ignore]
pub fn send_sigint_to_pane_id_plugin_command() {
    let temp_folder = tempdir().unwrap();
    let plugin_host_folder = PathBuf::from(temp_folder.path());
    let cache_path = plugin_host_folder.join("permissions_test.kdl");
    let (plugin_thread_sender, screen_receiver, pty_receiver, teardown) =
        create_plugin_thread_with_pty_receiver(Some(plugin_host_folder));

    let received_pty_instructions = Arc::new(Mutex::new(vec![]));
    let client_id = 1;

    let pty_thread = grant_permissions_and_log_actions_in_thread_struct_variant!(
        received_pty_instructions,
        PtyInstruction::SendSigintToPaneId,
        pty_receiver,
        1,
        &PermissionType::ChangeApplicationState,
        cache_path,
        plugin_thread_sender,
        client_id
    );

    // Also need to consume screen instructions to prevent deadlock
    std::thread::spawn(move || {
        while let Ok(_) = screen_receiver.recv() {
            // Drain screen instructions
        }
    });

    let plugin_should_float = Some(false);
    let plugin_title = Some("test_plugin".to_owned());
    let run_plugin = RunPluginOrAlias::RunPlugin(RunPlugin {
        _allow_exec_host_cmd: false,
        location: RunPluginLocation::File(PathBuf::from(&*PLUGIN_FIXTURE)),
        configuration: Default::default(),
        ..Default::default()
    });

    let _ = plugin_thread_sender.send(PluginInstruction::AddClient(client_id));
    let _ = plugin_thread_sender.send(PluginInstruction::Load(
        Some(LoadingPluginId::new(1)),
        run_plugin,
        1,
        client_id,
        plugin_should_float,
        None,
        plugin_title,
        None,
        None,
        None,
        None,
        None,
    ));

    std::thread::sleep(std::time::Duration::from_millis(500));

    let _ = plugin_thread_sender.send(PluginInstruction::Update(vec![(
        None,
        Some(client_id),
        Event::Key(KeyWithModifier::new(BareKey::Char('G'))),
    )]));

    pty_thread.join().unwrap();
    teardown();

    let sigint_instruction = received_pty_instructions
        .lock()
        .unwrap()
        .iter()
        .find_map(|i| {
            if let PtyInstruction::SendSigintToPaneId(pane_id, client_id) = i {
                Some((*pane_id, *client_id))
            } else {
                None
            }
        });

    assert_snapshot!(format!("{:#?}", sigint_instruction));
}
```

#### Test 2.6: send_sigkill_to_pane_id_plugin_command

```rust
#[test]
#[ignore]
pub fn send_sigkill_to_pane_id_plugin_command() {
    let temp_folder = tempdir().unwrap();
    let plugin_host_folder = PathBuf::from(temp_folder.path());
    let cache_path = plugin_host_folder.join("permissions_test.kdl");
    let (plugin_thread_sender, screen_receiver, pty_receiver, teardown) =
        create_plugin_thread_with_pty_receiver(Some(plugin_host_folder));

    let received_pty_instructions = Arc::new(Mutex::new(vec![]));
    let client_id = 1;

    let pty_thread = grant_permissions_and_log_actions_in_thread_struct_variant!(
        received_pty_instructions,
        PtyInstruction::SendSigkillToPaneId,
        pty_receiver,
        1,
        &PermissionType::ChangeApplicationState,
        cache_path,
        plugin_thread_sender,
        client_id
    );

    // Also need to consume screen instructions to prevent deadlock
    std::thread::spawn(move || {
        while let Ok(_) = screen_receiver.recv() {
            // Drain screen instructions
        }
    });

    let plugin_should_float = Some(false);
    let plugin_title = Some("test_plugin".to_owned());
    let run_plugin = RunPluginOrAlias::RunPlugin(RunPlugin {
        _allow_exec_host_cmd: false,
        location: RunPluginLocation::File(PathBuf::from(&*PLUGIN_FIXTURE)),
        configuration: Default::default(),
        ..Default::default()
    });

    let _ = plugin_thread_sender.send(PluginInstruction::AddClient(client_id));
    let _ = plugin_thread_sender.send(PluginInstruction::Load(
        Some(LoadingPluginId::new(1)),
        run_plugin,
        1,
        client_id,
        plugin_should_float,
        None,
        plugin_title,
        None,
        None,
        None,
        None,
        None,
    ));

    std::thread::sleep(std::time::Duration::from_millis(500));

    let _ = plugin_thread_sender.send(PluginInstruction::Update(vec![(
        None,
        Some(client_id),
        Event::Key(KeyWithModifier::new(BareKey::Char('H'))),
    )]));

    pty_thread.join().unwrap();
    teardown();

    let sigkill_instruction = received_pty_instructions
        .lock()
        .unwrap()
        .iter()
        .find_map(|i| {
            if let PtyInstruction::SendSigkillToPaneId(pane_id, client_id) = i {
                Some((*pane_id, *client_id))
            } else {
                None
            }
        });

    assert_snapshot!(format!("{:#?}", sigkill_instruction));
}
```

#### Test 2.7: copy_to_clipboard_without_permission

```rust
#[test]
#[ignore]
pub fn copy_to_clipboard_without_permission() {
    let temp_folder = tempdir().unwrap();
    let plugin_host_folder = PathBuf::from(temp_folder.path());
    let cache_path = plugin_host_folder.join("permissions_test.kdl");
    let (plugin_thread_sender, screen_receiver, teardown) =
        create_plugin_thread(Some(plugin_host_folder));

    let received_screen_instructions = Arc::new(Mutex::new(vec![]));
    let client_id = 1;

    // This test denies the permission and expects no CopyTextToClipboard instruction
    let screen_thread = deny_permissions_and_log_actions_in_thread!(
        received_screen_instructions,
        ScreenInstruction::CopyTextToClipboard,
        screen_receiver,
        &PermissionType::WriteToClipboard,
        cache_path,
        plugin_thread_sender,
        client_id
    );

    let plugin_should_float = Some(false);
    let plugin_title = Some("test_plugin".to_owned());
    let run_plugin = RunPluginOrAlias::RunPlugin(RunPlugin {
        _allow_exec_host_cmd: false,
        location: RunPluginLocation::File(PathBuf::from(&*PLUGIN_FIXTURE)),
        configuration: Default::default(),
        ..Default::default()
    });

    let _ = plugin_thread_sender.send(PluginInstruction::AddClient(client_id));
    let _ = plugin_thread_sender.send(PluginInstruction::Load(
        Some(LoadingPluginId::new(1)),
        run_plugin,
        1,
        client_id,
        plugin_should_float,
        None,
        plugin_title,
        None,
        None,
        None,
        None,
        None,
    ));

    std::thread::sleep(std::time::Duration::from_millis(500));

    let _ = plugin_thread_sender.send(PluginInstruction::Update(vec![(
        None,
        Some(client_id),
        Event::Key(KeyWithModifier::new(BareKey::Char('E'))),
    )]));

    // Give it time to potentially (incorrectly) send the instruction
    std::thread::sleep(std::time::Duration::from_millis(1000));

    let _ = plugin_thread_sender.send(PluginInstruction::Exit);
    screen_thread.join().unwrap();
    teardown();

    let copy_instruction = received_screen_instructions
        .lock()
        .unwrap()
        .iter()
        .find_map(|i| {
            if let ScreenInstruction::CopyTextToClipboard(text, plugin_id) = i {
                Some((text.clone(), *plugin_id))
            } else {
                None
            }
        });

    // Should be None because permission was denied
    assert_snapshot!(format!("{:#?}", copy_instruction));
}
```

#### Test 2.8: run_action_without_permission

```rust
#[test]
#[ignore]
pub fn run_action_without_permission() {
    let temp_folder = tempdir().unwrap();
    let plugin_host_folder = PathBuf::from(temp_folder.path());
    let cache_path = plugin_host_folder.join("permissions_test.kdl");
    let (plugin_thread_sender, screen_receiver, teardown) =
        create_plugin_thread(Some(plugin_host_folder));

    let received_screen_instructions = Arc::new(Mutex::new(vec![]));
    let client_id = 1;

    let screen_thread = deny_permissions_and_log_actions_in_thread!(
        received_screen_instructions,
        ScreenInstruction::MoveFocus,
        screen_receiver,
        &PermissionType::RunActionsAsUser,
        cache_path,
        plugin_thread_sender,
        client_id
    );

    let plugin_should_float = Some(false);
    let plugin_title = Some("test_plugin".to_owned());
    let run_plugin = RunPluginOrAlias::RunPlugin(RunPlugin {
        _allow_exec_host_cmd: false,
        location: RunPluginLocation::File(PathBuf::from(&*PLUGIN_FIXTURE)),
        configuration: Default::default(),
        ..Default::default()
    });

    let _ = plugin_thread_sender.send(PluginInstruction::AddClient(client_id));
    let _ = plugin_thread_sender.send(PluginInstruction::Load(
        Some(LoadingPluginId::new(1)),
        run_plugin,
        1,
        client_id,
        plugin_should_float,
        None,
        plugin_title,
        None,
        None,
        None,
        None,
        None,
    ));

    std::thread::sleep(std::time::Duration::from_millis(500));

    let _ = plugin_thread_sender.send(PluginInstruction::Update(vec![(
        None,
        Some(client_id),
        Event::Key(KeyWithModifier::new(BareKey::Char('F'))),
    )]));

    std::thread::sleep(std::time::Duration::from_millis(1000));

    let _ = plugin_thread_sender.send(PluginInstruction::Exit);
    screen_thread.join().unwrap();
    teardown();

    let move_focus_instruction = received_screen_instructions
        .lock()
        .unwrap()
        .iter()
        .find_map(|i| {
            if let ScreenInstruction::MoveFocus(direction, client_id) = i {
                Some((*direction, *client_id))
            } else {
                None
            }
        });

    // Should be None because permission was denied
    assert_snapshot!(format!("{:#?}", move_focus_instruction));
}
```

### Step 3: Build and Run Tests

**Commands to execute:**

1. **Build the fixture plugin** (required before running tests): (ASK THE USER TO DO THIS, DO NOT DO THIS ON YOUR OWN!!!!!)

2. **Run individual tests** (from repository root):
   ```bash
   cd zellij-server
   cargo test show_cursor_plugin_command -- --ignored --nocapture --test-threads 1
   cargo test hide_cursor_plugin_command -- --ignored --nocapture --test-threads 1
   cargo test copy_to_clipboard_plugin_command -- --ignored --nocapture --test-threads 1
   cargo test run_action_plugin_command -- --ignored --nocapture --test-threads 1
   cargo test send_sigint_to_pane_id_plugin_command -- --ignored --nocapture --test-threads 1
   cargo test send_sigkill_to_pane_id_plugin_command -- --ignored --nocapture --test-threads 1
   cargo test copy_to_clipboard_without_permission -- --ignored --nocapture --test-threads 1
   cargo test run_action_without_permission -- --ignored --nocapture --test-threads 1
   ```

3. **Run all new tests at once**:
   ```bash
   cd zellij-server
   cargo test cursor_plugin_command -- --ignored --nocapture --test-threads 1
   cargo test clipboard -- --ignored --nocapture --test-threads 1
   cargo test run_action -- --ignored --nocapture --test-threads 1
   cargo test sigint -- --ignored --nocapture --test-threads 1
   cargo test sigkill -- --ignored --nocapture --test-threads 1
   ```

4. **Review and accept snapshots**:
   ```bash
   cargo insta review
   # Or to accept all:
   cargo insta accept
   ```

### Step 4: Verify Snapshots

After running tests, new snapshot files will be created in:
`zellij-server/src/plugins/unit/snapshots/`

**Expected snapshot files** (8 new files):
1. `plugin_tests__show_cursor_plugin_command.snap`
2. `plugin_tests__hide_cursor_plugin_command.snap`
3. `plugin_tests__copy_to_clipboard_plugin_command.snap`
4. `plugin_tests__run_action_plugin_command.snap`
5. `plugin_tests__send_sigint_to_pane_id_plugin_command.snap`
6. `plugin_tests__send_sigkill_to_pane_id_plugin_command.snap`
7. `plugin_tests__copy_to_clipboard_without_permission.snap`
8. `plugin_tests__run_action_without_permission.snap`

**Expected snapshot content examples**:

For `show_cursor_plugin_command`:
```
Some(
    (
        1,
        1,
        Some(
            (
                5,
                10,
            ),
        ),
    ),
)
```

For `copy_to_clipboard_plugin_command`:
```
Some(
    (
        "test clipboard text",
        1,
    ),
)
```

For permission-denied tests:
```
None
```

## Critical Files

### Files to Modify:
1. **`default-plugins/fixture-plugin-for-tests/src/main.rs`** - Add 6 new key handlers (C, D, E, F, G, H)
2. **`zellij-server/src/plugins/unit/plugin_tests.rs`** - Add 8 new test functions

### Files Referenced (Read-only):
1. **`zellij-tile/src/shim.rs`** - Contains the new API implementations
2. **`zellij-server/src/plugins/zellij_exports.rs`** - Server-side handlers
3. **`zellij-utils/src/input/actions.rs`** - Action enum definitions
4. **`zellij-utils/src/data.rs`** - PaneId and other type definitions

### Snapshot Files Created (Auto-generated):
- **`zellij-server/src/plugins/unit/snapshots/plugin_tests__*.snap`** - 8 new files

## Testing Checklist

- [ ] Added 6 key handlers to fixture plugin (C, D, E, F, G, H)
- [ ] Added test function: `show_cursor_plugin_command`
- [ ] Added test function: `hide_cursor_plugin_command`
- [ ] Added test function: `copy_to_clipboard_plugin_command`
- [ ] Added test function: `run_action_plugin_command`
- [ ] Added test function: `send_sigint_to_pane_id_plugin_command`
- [ ] Added test function: `send_sigkill_to_pane_id_plugin_command`
- [ ] Added test function: `copy_to_clipboard_without_permission`
- [ ] Added test function: `run_action_without_permission`
- [ ] All 8 tests pass
- [ ] Reviewed and accepted all 8 snapshots
- [ ] Verified snapshot contents match expected behavior

## Notes and Considerations

1. **Why these instruction types?**
   - `show_cursor` → `ScreenInstruction::ShowPluginCursor` (screen handles cursor rendering)
   - `copy_to_clipboard` → `ScreenInstruction::CopyTextToClipboard` (screen handles clipboard)
   - `run_action` → Various screen instructions depending on action (we test `MoveFocus`)
   - `send_sigint/sigkill` → `PtyInstruction::SendSigint/SigkillToPaneId` (PTY handles signals)

2. **Why MoveFocus for run_action test?**
   - Simple action that generates a clear `ScreenInstruction::MoveFocus`
   - Easy to verify in snapshots
   - Doesn't require complex setup like panes or tabs

3. **Permission test strategy**
   - Tests with permissions verify the API works
   - Tests without permissions verify security (should return None)
   - Uses `deny_permissions_and_log_actions_in_thread!` macro

4. **Thread management**
   - PTY tests spawn an extra thread to drain screen_receiver (prevents deadlock)
   - Uses `std::thread::sleep()` to ensure plugin is loaded before sending events
   - All tests marked `#[ignore]` to run separately with `--test-threads 1`

5. **Test timeout consideration**
   - Tests include 500ms sleep after loading plugin (standard pattern)
   - Permission-denied tests include 1000ms sleep before exit to ensure no delayed instruction

## Troubleshooting

### If tests fail to compile:
1. Check that all imports are available in `plugin_tests.rs`
2. Verify `BareKey`, `KeyWithModifier`, `Event` are imported
3. Ensure `PtyInstruction` enum is accessible

### If tests hang:
1. Verify screen_receiver is being drained in PTY tests
2. Check that the exit condition in logging macro matches the expected instruction
3. Ensure plugin_thread_sender.send() calls are not blocking

### If snapshots don't match expected:
1. Review the actual instruction sent (check test output)
2. Verify the key binding in fixture plugin is correct
3. Check that permissions are granted in the test

### If fixture plugin doesn't rebuild:
ASK THE USER WHAT YOU SHOULD DO!!!
