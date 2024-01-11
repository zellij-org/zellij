use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
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
    }

    fn update(&mut self, event: Event) -> bool {
        match &event {
            Event::Key(key) => match key {
                Key::Char('a') => {
                    switch_to_input_mode(&InputMode::Tab);
                },
                Key::Char('b') => {
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
                Key::Char('c') => new_tab(),
                Key::Char('d') => go_to_next_tab(),
                Key::Char('e') => go_to_previous_tab(),
                Key::Char('f') => {
                    let resize = Resize::Increase;
                    resize_focused_pane(resize)
                },
                Key::Char('g') => {
                    let resize = Resize::Increase;
                    let direction = Direction::Left;
                    resize_focused_pane_with_direction(resize, direction);
                },
                Key::Char('h') => focus_next_pane(),
                Key::Char('i') => focus_previous_pane(),
                Key::Char('j') => {
                    let direction = Direction::Left;
                    move_focus(direction)
                },
                Key::Char('k') => {
                    let direction = Direction::Left;
                    move_focus_or_tab(direction)
                },
                Key::Char('l') => detach(),
                Key::Char('m') => edit_scrollback(),
                Key::Char('n') => {
                    let bytes = vec![102, 111, 111];
                    write(bytes)
                },
                Key::Char('o') => {
                    let chars = "foo";
                    write_chars(chars);
                },
                Key::Char('p') => toggle_tab(),
                Key::Char('q') => move_pane(),
                Key::Char('r') => {
                    let direction = Direction::Left;
                    move_pane_with_direction(direction)
                },
                Key::Char('s') => clear_screen(),
                Key::Char('t') => scroll_up(),
                Key::Char('u') => scroll_down(),
                Key::Char('v') => scroll_to_top(),
                Key::Char('w') => scroll_to_bottom(),
                Key::Char('x') => page_scroll_up(),
                Key::Char('y') => page_scroll_down(),
                Key::Char('z') => toggle_focus_fullscreen(),
                Key::Char('1') => toggle_pane_frames(),
                Key::Char('2') => toggle_pane_embed_or_eject(),
                Key::Char('3') => undo_rename_pane(),
                Key::Char('4') => close_focus(),
                Key::Char('5') => toggle_active_tab_sync(),
                Key::Char('6') => close_focused_tab(),
                Key::Char('7') => undo_rename_tab(),
                Key::Char('8') => quit_zellij(),
                Key::Ctrl('a') => previous_swap_layout(),
                Key::Ctrl('b') => next_swap_layout(),
                Key::Ctrl('c') => {
                    let tab_name = "my tab name";
                    go_to_tab_name(tab_name)
                },
                Key::Ctrl('d') => {
                    let tab_name = "my tab name";
                    focus_or_create_tab(tab_name)
                },
                Key::Ctrl('e') => {
                    let tab_index = 2;
                    go_to_tab(tab_index)
                },
                Key::Ctrl('f') => {
                    let plugin_url = "file:/path/to/my/plugin.wasm";
                    start_or_reload_plugin(plugin_url)
                },
                Key::Ctrl('g') => {
                    open_file(FileToOpen {
                        path: std::path::PathBuf::from("/path/to/my/file.rs"),
                        ..Default::default()
                    });
                },
                Key::Ctrl('h') => {
                    open_file_floating(FileToOpen {
                        path: std::path::PathBuf::from("/path/to/my/file.rs"),
                        ..Default::default()
                    });
                },
                Key::Ctrl('i') => {
                    open_file(FileToOpen {
                        path: std::path::PathBuf::from("/path/to/my/file.rs"),
                        line_number: Some(42),
                        ..Default::default()
                    });
                },
                Key::Ctrl('j') => {
                    open_file_floating(FileToOpen {
                        path: std::path::PathBuf::from("/path/to/my/file.rs"),
                        line_number: Some(42),
                        ..Default::default()
                    });
                },
                Key::Ctrl('k') => {
                    open_terminal(std::path::PathBuf::from("/path/to/my/file.rs").as_path());
                },
                Key::Ctrl('l') => {
                    open_terminal_floating(
                        std::path::PathBuf::from("/path/to/my/file.rs").as_path(),
                    );
                },
                Key::Ctrl('m') => {
                    open_command_pane(CommandToRun {
                        path: std::path::PathBuf::from("/path/to/my/file.rs"),
                        args: vec!["arg1".to_owned(), "arg2".to_owned()],
                        ..Default::default()
                    });
                },
                Key::Ctrl('n') => {
                    open_command_pane_floating(CommandToRun {
                        path: std::path::PathBuf::from("/path/to/my/file.rs"),
                        args: vec!["arg1".to_owned(), "arg2".to_owned()],
                        ..Default::default()
                    });
                },
                Key::Ctrl('o') => {
                    switch_tab_to(1);
                },
                Key::Ctrl('p') => {
                    hide_self();
                },
                Key::Ctrl('q') => {
                    let should_float_if_hidden = false;
                    show_self(should_float_if_hidden);
                },
                Key::Ctrl('r') => {
                    close_terminal_pane(1);
                },
                Key::Ctrl('s') => {
                    close_plugin_pane(1);
                },
                Key::Ctrl('t') => {
                    let should_float_if_hidden = false;
                    focus_terminal_pane(1, should_float_if_hidden);
                },
                Key::Ctrl('u') => {
                    let should_float_if_hidden = false;
                    focus_plugin_pane(1, should_float_if_hidden);
                },
                Key::Ctrl('v') => {
                    rename_terminal_pane(1, "new terminal_pane_name");
                },
                Key::Ctrl('w') => {
                    rename_plugin_pane(1, "new plugin_pane_name");
                },
                Key::Ctrl('x') => {
                    rename_tab(1, "new tab name");
                },
                Key::Ctrl('z') => {
                    go_to_tab_name(&format!("{:?}", self.configuration));
                },
                Key::Ctrl('1') => {
                    request_permission(&[PermissionType::ReadApplicationState]);
                },
                Key::Ctrl('2') => {
                    let mut context = BTreeMap::new();
                    context.insert("user_key_1".to_owned(), "user_value_1".to_owned());
                    run_command(&["ls", "-l"], context);
                },
                Key::Ctrl('3') => {
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
                Key::Ctrl('4') => {
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
