use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io::prelude::*;
use zellij_tile::prelude::*;

// This is a fixture plugin used only for tests in Zellij
// it is not (and should not!) be included in the mainline executable
// it's included here for convenience so that it will be built by the CI

#[derive(Default)]
struct State {
    received_events: Vec<Event>,
    received_payload: Option<String>,
    configuration: BTreeMap<String, String>,
    message_to_plugin_payload: Option<String>,
}

#[derive(Default, Serialize, Deserialize)]
struct TestWorker {
    number_of_messages_received: usize,
}

impl<'de> ZellijWorker<'de> for TestWorker {
    fn on_message(&mut self, message: String, payload: String) {
        if message == "ping" {
            self.number_of_messages_received += 1;
            post_message_to_plugin(PluginMessage {
                worker_name: None,
                name: "pong".into(),
                payload: format!(
                    "{}, received {} messages",
                    payload, self.number_of_messages_received
                ),
            });
        }
    }
}

#[cfg(target_family = "wasm")]
register_plugin!(State);
#[cfg(target_family = "wasm")]
register_worker!(TestWorker, test_worker, TEST_WORKER);

#[cfg(target_family = "wasm")]
impl ZellijPlugin for State {
    fn load(&mut self, configuration: BTreeMap<String, String>) {
        request_permission(&[
            PermissionType::ChangeApplicationState,
            PermissionType::ReadApplicationState,
            PermissionType::ReadApplicationState,
            PermissionType::ChangeApplicationState,
            PermissionType::OpenFiles,
            PermissionType::RunCommands,
            PermissionType::OpenTerminalsOrPlugins,
            PermissionType::WriteToStdin,
            PermissionType::WebAccess,
            PermissionType::ReadCliPipes,
            PermissionType::MessageAndLaunchOtherPlugins,
        ]);
        self.configuration = configuration;
        subscribe(&[
            EventType::InputReceived,
            EventType::Key,
            EventType::SystemClipboardFailure,
            EventType::CustomMessage,
            EventType::FileSystemCreate,
            EventType::FileSystemUpdate,
            EventType::FileSystemDelete,
        ]);
        watch_filesystem();
    }

    fn update(&mut self, event: Event) -> bool {
        match &event {
            Event::Key(key) => match key.bare_key {
                BareKey::Char('a') if key.has_no_modifiers() => {
                    switch_to_input_mode(&InputMode::Tab);
                },
                BareKey::Char('b') if key.has_no_modifiers() => {
                    new_tabs_with_layout(
                        "layout {
                        tab {
                            pane
                            pane
                        }
                        tab split_direction=\"vertical\" {
                            pane
                            pane
                        }
                    }",
                    );
                },
                BareKey::Char('c') if key.has_no_modifiers() => new_tab(),
                BareKey::Char('d') if key.has_no_modifiers() => go_to_next_tab(),
                BareKey::Char('e') if key.has_no_modifiers() => go_to_previous_tab(),
                BareKey::Char('f') if key.has_no_modifiers() => {
                    let resize = Resize::Increase;
                    resize_focused_pane(resize)
                },
                BareKey::Char('g') if key.has_no_modifiers() => {
                    let resize = Resize::Increase;
                    let direction = Direction::Left;
                    resize_focused_pane_with_direction(resize, direction);
                },
                BareKey::Char('h') if key.has_no_modifiers() => focus_next_pane(),
                BareKey::Char('i') if key.has_no_modifiers() => focus_previous_pane(),
                BareKey::Char('j') if key.has_no_modifiers() => {
                    let direction = Direction::Left;
                    move_focus(direction)
                },
                BareKey::Char('k') if key.has_no_modifiers() => {
                    let direction = Direction::Left;
                    move_focus_or_tab(direction)
                },
                BareKey::Char('l') if key.has_no_modifiers() => detach(),
                BareKey::Char('m') if key.has_no_modifiers() => edit_scrollback(),
                BareKey::Char('n') if key.has_no_modifiers() => {
                    let bytes = vec![102, 111, 111];
                    write(bytes)
                },
                BareKey::Char('o') if key.has_no_modifiers() => {
                    let chars = "foo";
                    write_chars(chars);
                },
                BareKey::Char('p') if key.has_no_modifiers() => toggle_tab(),
                BareKey::Char('q') if key.has_no_modifiers() => move_pane(),
                BareKey::Char('r') if key.has_no_modifiers() => {
                    let direction = Direction::Left;
                    move_pane_with_direction(direction)
                },
                BareKey::Char('s') if key.has_no_modifiers() => clear_screen(),
                BareKey::Char('t') if key.has_no_modifiers() => scroll_up(),
                BareKey::Char('u') if key.has_no_modifiers() => scroll_down(),
                BareKey::Char('v') if key.has_no_modifiers() => scroll_to_top(),
                BareKey::Char('w') if key.has_no_modifiers() => scroll_to_bottom(),
                BareKey::Char('x') if key.has_no_modifiers() => page_scroll_up(),
                BareKey::Char('y') if key.has_no_modifiers() => page_scroll_down(),
                BareKey::Char('z') if key.has_no_modifiers() => toggle_focus_fullscreen(),
                BareKey::Char('1') if key.has_no_modifiers() => toggle_pane_frames(),
                BareKey::Char('2') if key.has_no_modifiers() => toggle_pane_embed_or_eject(),
                BareKey::Char('3') if key.has_no_modifiers() => undo_rename_pane(),
                BareKey::Char('4') if key.has_no_modifiers() => close_focus(),
                BareKey::Char('5') if key.has_no_modifiers() => toggle_active_tab_sync(),
                BareKey::Char('6') if key.has_no_modifiers() => close_focused_tab(),
                BareKey::Char('7') if key.has_no_modifiers() => undo_rename_tab(),
                BareKey::Char('8') if key.has_no_modifiers() => quit_zellij(),
                BareKey::Char('a') if key.has_modifiers(&[KeyModifier::Ctrl]) => {
                    previous_swap_layout()
                },
                BareKey::Char('b') if key.has_modifiers(&[KeyModifier::Ctrl]) => next_swap_layout(),
                BareKey::Char('c') if key.has_modifiers(&[KeyModifier::Ctrl]) => {
                    let tab_name = "my tab name";
                    go_to_tab_name(tab_name)
                },
                BareKey::Char('d') if key.has_modifiers(&[KeyModifier::Ctrl]) => {
                    let tab_name = "my tab name";
                    focus_or_create_tab(tab_name)
                },
                BareKey::Char('e') if key.has_modifiers(&[KeyModifier::Ctrl]) => {
                    let tab_index = 2;
                    go_to_tab(tab_index)
                },
                BareKey::Char('f') if key.has_modifiers(&[KeyModifier::Ctrl]) => {
                    let plugin_url = "file:/path/to/my/plugin.wasm";
                    start_or_reload_plugin(plugin_url)
                },
                BareKey::Char('g') if key.has_modifiers(&[KeyModifier::Ctrl]) => {
                    open_file(FileToOpen {
                        path: std::path::PathBuf::from("/path/to/my/file.rs"),
                        ..Default::default()
                    });
                },
                BareKey::Char('h') if key.has_modifiers(&[KeyModifier::Ctrl]) => {
                    open_file_floating(
                        FileToOpen {
                            path: std::path::PathBuf::from("/path/to/my/file.rs"),
                            ..Default::default()
                        },
                        None,
                    );
                },
                BareKey::Char('i') if key.has_modifiers(&[KeyModifier::Ctrl]) => {
                    open_file(FileToOpen {
                        path: std::path::PathBuf::from("/path/to/my/file.rs"),
                        line_number: Some(42),
                        ..Default::default()
                    });
                },
                BareKey::Char('j') if key.has_modifiers(&[KeyModifier::Ctrl]) => {
                    open_file_floating(
                        FileToOpen {
                            path: std::path::PathBuf::from("/path/to/my/file.rs"),
                            line_number: Some(42),
                            ..Default::default()
                        },
                        None,
                    );
                },
                BareKey::Char('k') if key.has_modifiers(&[KeyModifier::Ctrl]) => {
                    open_terminal(std::path::PathBuf::from("/path/to/my/file.rs").as_path());
                },
                BareKey::Char('l') if key.has_modifiers(&[KeyModifier::Ctrl]) => {
                    open_terminal_floating(
                        std::path::PathBuf::from("/path/to/my/file.rs").as_path(),
                        None,
                    );
                },
                BareKey::Char('m') if key.has_modifiers(&[KeyModifier::Ctrl]) => {
                    open_command_pane(CommandToRun {
                        path: std::path::PathBuf::from("/path/to/my/file.rs"),
                        args: vec!["arg1".to_owned(), "arg2".to_owned()],
                        ..Default::default()
                    });
                },
                BareKey::Char('n') if key.has_modifiers(&[KeyModifier::Ctrl]) => {
                    open_command_pane_floating(
                        CommandToRun {
                            path: std::path::PathBuf::from("/path/to/my/file.rs"),
                            args: vec!["arg1".to_owned(), "arg2".to_owned()],
                            ..Default::default()
                        },
                        None,
                    );
                },
                BareKey::Char('o') if key.has_modifiers(&[KeyModifier::Ctrl]) => {
                    switch_tab_to(1);
                },
                BareKey::Char('p') if key.has_modifiers(&[KeyModifier::Ctrl]) => {
                    hide_self();
                },
                BareKey::Char('q') if key.has_modifiers(&[KeyModifier::Ctrl]) => {
                    let should_float_if_hidden = false;
                    show_self(should_float_if_hidden);
                },
                BareKey::Char('r') if key.has_modifiers(&[KeyModifier::Ctrl]) => {
                    close_terminal_pane(1);
                },
                BareKey::Char('s') if key.has_modifiers(&[KeyModifier::Ctrl]) => {
                    close_plugin_pane(1);
                },
                BareKey::Char('t') if key.has_modifiers(&[KeyModifier::Ctrl]) => {
                    let should_float_if_hidden = false;
                    focus_terminal_pane(1, should_float_if_hidden);
                },
                BareKey::Char('u') if key.has_modifiers(&[KeyModifier::Ctrl]) => {
                    let should_float_if_hidden = false;
                    focus_plugin_pane(1, should_float_if_hidden);
                },
                BareKey::Char('v') if key.has_modifiers(&[KeyModifier::Ctrl]) => {
                    rename_terminal_pane(1, "new terminal_pane_name");
                },
                BareKey::Char('w') if key.has_modifiers(&[KeyModifier::Ctrl]) => {
                    rename_plugin_pane(1, "new plugin_pane_name");
                },
                BareKey::Char('x') if key.has_modifiers(&[KeyModifier::Ctrl]) => {
                    rename_tab(1, "new tab name");
                },
                BareKey::Char('z') if key.has_modifiers(&[KeyModifier::Ctrl]) => {
                    go_to_tab_name(&format!("{:?}", self.configuration));
                },
                BareKey::Char('1') if key.has_modifiers(&[KeyModifier::Ctrl]) => {
                    request_permission(&[PermissionType::ReadApplicationState]);
                },
                BareKey::Char('2') if key.has_modifiers(&[KeyModifier::Ctrl]) => {
                    let mut context = BTreeMap::new();
                    context.insert("user_key_1".to_owned(), "user_value_1".to_owned());
                    run_command(&["ls", "-l"], context);
                },
                BareKey::Char('3') if key.has_modifiers(&[KeyModifier::Ctrl]) => {
                    let mut context = BTreeMap::new();
                    context.insert("user_key_2".to_owned(), "user_value_2".to_owned());
                    let mut env_vars = BTreeMap::new();
                    env_vars.insert("VAR1".to_owned(), "some_value".to_owned());
                    run_command_with_env_variables_and_cwd(
                        &["ls", "-l"],
                        env_vars,
                        std::path::PathBuf::from("/some/custom/folder"),
                        context,
                    );
                },
                BareKey::Char('4') if key.has_modifiers(&[KeyModifier::Ctrl]) => {
                    let mut headers = BTreeMap::new();
                    let mut context = BTreeMap::new();
                    let body = vec![1, 2, 3];
                    headers.insert("header1".to_owned(), "value1".to_owned());
                    headers.insert("header2".to_owned(), "value2".to_owned());
                    context.insert("user_key_1".to_owned(), "user_value1".to_owned());
                    context.insert("user_key_2".to_owned(), "user_value2".to_owned());
                    web_request(
                        "https://example.com/foo?arg1=val1&arg2=val2",
                        HttpVerb::Post,
                        headers,
                        body,
                        context,
                    );
                },
                BareKey::Char('5') if key.has_modifiers(&[KeyModifier::Ctrl]) => {
                    switch_session(Some("my_new_session"));
                },
                BareKey::Char('6') if key.has_modifiers(&[KeyModifier::Ctrl]) => {
                    disconnect_other_clients()
                },
                BareKey::Char('7') if key.has_modifiers(&[KeyModifier::Ctrl]) => {
                    switch_session_with_layout(
                        Some("my_other_new_session"),
                        LayoutInfo::BuiltIn("compact".to_owned()),
                        None,
                    );
                },
                BareKey::Char('8') if key.has_modifiers(&[KeyModifier::Ctrl]) => {
                    let mut file = std::fs::File::create("/host/hi-from-plugin.txt").unwrap();
                    file.write_all(b"Hi there!").unwrap();
                },
                BareKey::Char('9') if key.has_modifiers(&[KeyModifier::Ctrl]) => {
                    switch_session_with_layout(
                        Some("my_other_new_session_with_cwd"),
                        LayoutInfo::BuiltIn("compact".to_owned()),
                        Some(std::path::PathBuf::from("/tmp")),
                    );
                },
                _ => {},
            },
            Event::CustomMessage(message, payload) => {
                if message == "pong" {
                    self.received_payload = Some(payload.clone());
                }
            },
            Event::SystemClipboardFailure => {
                // this is just to trigger the worker message
                post_message_to(PluginMessage {
                    worker_name: Some("test".into()),
                    name: "ping".into(),
                    payload: "gimme_back_my_payload".into(),
                });
            },
            _ => {},
        }
        let should_render = true;
        self.received_events.push(event);
        should_render
    }
    fn pipe(&mut self, pipe_message: PipeMessage) -> bool {
        let input_pipe_id = match pipe_message.source {
            PipeSource::Cli(id) => id.clone(),
            PipeSource::Plugin(id) => format!("{}", id),
            PipeSource::Keybind => format!("keybind"),
        };
        let name = pipe_message.name;
        let payload = pipe_message.payload;
        if name == "message_name" && payload == Some("message_payload".to_owned()) {
            unblock_cli_pipe_input(&input_pipe_id);
        } else if name == "message_name_block" {
            block_cli_pipe_input(&input_pipe_id);
        } else if name == "pipe_output" {
            cli_pipe_output(&name, "this_is_my_output");
        } else if name == "pipe_message_to_plugin" {
            pipe_message_to_plugin(
                MessageToPlugin::new("message_to_plugin").with_payload("my_cool_payload"),
            );
        } else if name == "message_to_plugin" {
            self.message_to_plugin_payload = payload.clone();
        }
        let should_render = true;
        should_render
    }

    fn render(&mut self, rows: usize, cols: usize) {
        if let Some(payload) = self.received_payload.as_ref() {
            println!("Payload from worker: {:?}", payload);
        } else if let Some(payload) = self.message_to_plugin_payload.take() {
            println!("Payload from self: {:?}", payload);
        } else {
            println!(
                "Rows: {:?}, Cols: {:?}, Received events: {:?}",
                rows, cols, self.received_events
            );
        }
    }
}
