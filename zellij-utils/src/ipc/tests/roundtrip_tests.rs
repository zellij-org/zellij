use super::test_framework::*;
use crate::data::{
    BareKey, CommandOrPlugin, ConnectToSession, Direction, FloatingPaneCoordinates, InputMode,
    KeyModifier, KeyWithModifier, LayoutInfo, OriginatingPlugin, PaneId, PluginTag, Resize,
    WebSharing,
};
use crate::input::actions::{Action, SearchDirection, SearchOption};
use crate::input::cli_assets::CliAssets;
use crate::input::command::{OpenFilePayload, RunCommand, RunCommandAction};
use crate::input::layout::{
    FloatingPaneLayout, LayoutConstraint, PercentOrFixed, PluginAlias, PluginUserConfiguration,
    Run, RunPlugin, RunPluginLocation, RunPluginOrAlias, SplitDirection, SplitSize,
    TiledPaneLayout,
};
use crate::input::mouse::{MouseEvent, MouseEventType};
use crate::input::options::{Clipboard, OnForceClose, Options};
use crate::ipc::{
    ClientToServerMsg, ColorRegister, ExitReason, PaneReference, PixelDimensions, ServerToClientMsg,
};
use crate::pane_size::{Size, SizeInPixels};
use crate::position::Position;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

#[test]
fn server_client_contract() {
    // here we test all possible values of each of the nested types in the server/client contract
    // in the context of its message.
    //
    // we do a "roundtrip" test, meaning we take each message, encode it to protobuf bytes and
    // decode it back to its rust type and then assert its equality with the original type to make
    // sure they are identical and did not lose any information
    test_client_messages();
    test_server_messages();
}

fn test_client_messages() {
    let empty_context = BTreeMap::new();
    let mut demo_context = BTreeMap::new();
    demo_context.insert("demo_key1".to_owned(), "demo_value1".to_owned());
    demo_context.insert("demo_key2".to_owned(), "demo_value2".to_owned());
    demo_context.insert("demo_key3".to_owned(), "demo_value3".to_owned());
    let mut swap_tiled_layouts_1 = BTreeMap::new();
    swap_tiled_layouts_1.insert(
        LayoutConstraint::MaxPanes(1),
        TiledPaneLayout {
            name: Some("max_panes_1".to_owned()),
            ..Default::default()
        },
    );
    swap_tiled_layouts_1.insert(
        LayoutConstraint::MinPanes(2),
        TiledPaneLayout {
            name: Some("min_panes_2".to_owned()),
            ..Default::default()
        },
    );
    swap_tiled_layouts_1.insert(
        LayoutConstraint::ExactPanes(3),
        TiledPaneLayout {
            name: Some("exact_panes_2".to_owned()),
            ..Default::default()
        },
    );
    swap_tiled_layouts_1.insert(
        LayoutConstraint::NoConstraint,
        TiledPaneLayout {
            name: Some("no_constraint".to_owned()),
            ..Default::default()
        },
    );
    let swap_tiled_layouts_1 = (
        swap_tiled_layouts_1,
        Some("swap_tiled_layouts_1".to_owned()),
    );
    let mut swap_tiled_layouts_2 = BTreeMap::new();
    swap_tiled_layouts_2.insert(
        LayoutConstraint::MaxPanes(1),
        TiledPaneLayout {
            name: Some("max_panes_1".to_owned()),
            ..Default::default()
        },
    );
    swap_tiled_layouts_2.insert(
        LayoutConstraint::MinPanes(2),
        TiledPaneLayout {
            name: Some("min_panes_2".to_owned()),
            ..Default::default()
        },
    );
    swap_tiled_layouts_2.insert(
        LayoutConstraint::ExactPanes(3),
        TiledPaneLayout {
            name: Some("exact_panes_2".to_owned()),
            ..Default::default()
        },
    );
    swap_tiled_layouts_2.insert(
        LayoutConstraint::NoConstraint,
        TiledPaneLayout {
            name: Some("no_constraint".to_owned()),
            ..Default::default()
        },
    );
    let swap_tiled_layouts_2 = (
        swap_tiled_layouts_2,
        Some("swap_tiled_layouts_2".to_owned()),
    );
    let mut swap_tiled_layouts_3 = BTreeMap::new();
    swap_tiled_layouts_3.insert(
        LayoutConstraint::MaxPanes(1),
        TiledPaneLayout {
            name: Some("max_panes_1".to_owned()),
            ..Default::default()
        },
    );
    swap_tiled_layouts_3.insert(
        LayoutConstraint::MinPanes(2),
        TiledPaneLayout {
            name: Some("min_panes_2".to_owned()),
            ..Default::default()
        },
    );
    swap_tiled_layouts_3.insert(
        LayoutConstraint::ExactPanes(3),
        TiledPaneLayout {
            name: Some("exact_panes_2".to_owned()),
            ..Default::default()
        },
    );
    swap_tiled_layouts_3.insert(
        LayoutConstraint::NoConstraint,
        TiledPaneLayout {
            name: Some("no_constraint".to_owned()),
            ..Default::default()
        },
    );
    let swap_tiled_layouts_3 = (swap_tiled_layouts_3, None);

    let mut swap_floating_layouts_1 = BTreeMap::new();
    swap_floating_layouts_1.insert(
        LayoutConstraint::MaxPanes(1),
        vec![
            FloatingPaneLayout {
                name: Some("max_panes_1".to_owned()),
                ..Default::default()
            },
            FloatingPaneLayout {
                name: Some("max_panes_1_1".to_owned()),
                ..Default::default()
            },
        ],
    );
    swap_floating_layouts_1.insert(
        LayoutConstraint::MinPanes(2),
        vec![
            FloatingPaneLayout {
                name: Some("min_panes_2".to_owned()),
                ..Default::default()
            },
            FloatingPaneLayout {
                name: Some("min_panes_2_1".to_owned()),
                ..Default::default()
            },
        ],
    );
    swap_floating_layouts_1.insert(
        LayoutConstraint::ExactPanes(3),
        vec![
            FloatingPaneLayout {
                name: Some("exact_panes_2".to_owned()),
                ..Default::default()
            },
            FloatingPaneLayout {
                name: Some("exact_panes_2_1".to_owned()),
                ..Default::default()
            },
        ],
    );
    swap_floating_layouts_1.insert(
        LayoutConstraint::NoConstraint,
        vec![
            FloatingPaneLayout {
                name: Some("no_constraint".to_owned()),
                ..Default::default()
            },
            FloatingPaneLayout {
                name: Some("no_constraint_1".to_owned()),
                ..Default::default()
            },
        ],
    );
    let swap_floating_layouts_1 = (
        swap_floating_layouts_1,
        Some("swap_floating_layouts_1".to_owned()),
    );
    let mut swap_floating_layouts_2 = BTreeMap::new();
    swap_floating_layouts_2.insert(
        LayoutConstraint::MaxPanes(1),
        vec![
            FloatingPaneLayout {
                name: Some("max_panes_1".to_owned()),
                ..Default::default()
            },
            FloatingPaneLayout {
                name: Some("max_panes_1_1".to_owned()),
                ..Default::default()
            },
        ],
    );
    swap_floating_layouts_2.insert(
        LayoutConstraint::MinPanes(2),
        vec![
            FloatingPaneLayout {
                name: Some("min_panes_2".to_owned()),
                ..Default::default()
            },
            FloatingPaneLayout {
                name: Some("min_panes_2_1".to_owned()),
                ..Default::default()
            },
        ],
    );
    swap_floating_layouts_2.insert(
        LayoutConstraint::ExactPanes(3),
        vec![
            FloatingPaneLayout {
                name: Some("exact_panes_2".to_owned()),
                ..Default::default()
            },
            FloatingPaneLayout {
                name: Some("exact_panes_2_1".to_owned()),
                ..Default::default()
            },
        ],
    );
    swap_floating_layouts_2.insert(
        LayoutConstraint::NoConstraint,
        vec![
            FloatingPaneLayout {
                name: Some("no_constraint".to_owned()),
                ..Default::default()
            },
            FloatingPaneLayout {
                name: Some("no_constraint_1".to_owned()),
                ..Default::default()
            },
        ],
    );
    let swap_floating_layouts_2 = (
        swap_floating_layouts_2,
        Some("swap_floating_layouts_2".to_owned()),
    );
    let mut swap_floating_layouts_3 = BTreeMap::new();
    swap_floating_layouts_3.insert(
        LayoutConstraint::MaxPanes(1),
        vec![
            FloatingPaneLayout {
                name: Some("max_panes_1".to_owned()),
                ..Default::default()
            },
            FloatingPaneLayout {
                name: Some("max_panes_1_1".to_owned()),
                ..Default::default()
            },
        ],
    );
    swap_floating_layouts_3.insert(
        LayoutConstraint::MinPanes(2),
        vec![
            FloatingPaneLayout {
                name: Some("min_panes_2".to_owned()),
                ..Default::default()
            },
            FloatingPaneLayout {
                name: Some("min_panes_2_1".to_owned()),
                ..Default::default()
            },
        ],
    );
    swap_floating_layouts_3.insert(
        LayoutConstraint::ExactPanes(3),
        vec![
            FloatingPaneLayout {
                name: Some("exact_panes_2".to_owned()),
                ..Default::default()
            },
            FloatingPaneLayout {
                name: Some("exact_panes_2_1".to_owned()),
                ..Default::default()
            },
        ],
    );
    swap_floating_layouts_3.insert(
        LayoutConstraint::NoConstraint,
        vec![
            FloatingPaneLayout {
                name: Some("no_constraint".to_owned()),
                ..Default::default()
            },
            FloatingPaneLayout {
                name: Some("no_constraint_1".to_owned()),
                ..Default::default()
            },
        ],
    );
    let swap_floating_layouts_3 = (swap_floating_layouts_3, None);
    let mut demo_modifiers_1 = BTreeSet::new();
    let mut demo_modifiers_2 = BTreeSet::new();
    let mut demo_modifiers_3 = BTreeSet::new();
    let mut demo_modifiers_4 = BTreeSet::new();
    demo_modifiers_1.insert(KeyModifier::Ctrl);
    demo_modifiers_2.insert(KeyModifier::Ctrl);
    demo_modifiers_3.insert(KeyModifier::Ctrl);
    demo_modifiers_4.insert(KeyModifier::Ctrl);
    demo_modifiers_2.insert(KeyModifier::Alt);
    demo_modifiers_3.insert(KeyModifier::Alt);
    demo_modifiers_4.insert(KeyModifier::Alt);
    demo_modifiers_3.insert(KeyModifier::Shift);
    demo_modifiers_4.insert(KeyModifier::Shift);
    demo_modifiers_4.insert(KeyModifier::Super);

    test_client_roundtrip!(ClientToServerMsg::DetachSession { client_ids: vec![] });
    test_client_roundtrip!(ClientToServerMsg::DetachSession {
        client_ids: vec![1],
    });
    test_client_roundtrip!(ClientToServerMsg::DetachSession {
        client_ids: vec![1, 2, 999],
    });
    test_client_roundtrip!(ClientToServerMsg::TerminalPixelDimensions {
        pixel_dimensions: PixelDimensions {
            text_area_size: None,
            character_cell_size: None,
        },
    });
    test_client_roundtrip!(ClientToServerMsg::TerminalPixelDimensions {
        pixel_dimensions: PixelDimensions {
            text_area_size: Some(SizeInPixels {
                width: 800,
                height: 600
            }),
            character_cell_size: Some(SizeInPixels {
                width: 10,
                height: 20
            }),
        },
    });
    test_client_roundtrip!(ClientToServerMsg::BackgroundColor {
        color: "red".to_string(),
    });
    test_client_roundtrip!(ClientToServerMsg::BackgroundColor {
        color: "#FF0000".to_string(),
    });
    test_client_roundtrip!(ClientToServerMsg::BackgroundColor {
        color: "".to_string(),
    });
    test_client_roundtrip!(ClientToServerMsg::ForegroundColor {
        color: "blue".to_string(),
    });
    test_client_roundtrip!(ClientToServerMsg::ColorRegisters {
        color_registers: vec![],
    });
    test_client_roundtrip!(ClientToServerMsg::ColorRegisters {
        color_registers: vec![
            ColorRegister {
                index: 0,
                color: "black".to_string()
            },
            ColorRegister {
                index: 1,
                color: "red".to_string()
            },
            ColorRegister {
                index: 255,
                color: "#FFFFFF".to_string()
            },
        ],
    });
    test_client_roundtrip!(ClientToServerMsg::TerminalResize {
        new_size: Size { cols: 80, rows: 24 },
    });
    test_client_roundtrip!(ClientToServerMsg::TerminalResize {
        new_size: Size {
            cols: 200,
            rows: 50
        },
    });
    test_client_roundtrip!(ClientToServerMsg::FirstClientConnected {
        cli_assets: CliAssets::default(),
        is_web_client: false,
    });
    test_client_roundtrip!(ClientToServerMsg::FirstClientConnected {
        cli_assets: CliAssets::default(),
        is_web_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::FirstClientConnected {
        cli_assets: CliAssets {
            config_file_path: Some(PathBuf::from("/path/to/config/file.kdl")),
            config_dir: Some(PathBuf::from("/path/to/config/dir")),
            should_ignore_config: true,
            configuration_options: None,
            layout: None,
            terminal_window_size: Size { rows: 80, cols: 42 },
            data_dir: Some(PathBuf::from("/path/to/data/dir")),
            is_debug: true,
            max_panes: Some(4),
            force_run_layout_commands: true,
            cwd: Some(PathBuf::from("/path/to/cwd")),
        },
        is_web_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::FirstClientConnected {
        cli_assets: CliAssets {
            config_file_path: Some(PathBuf::from("/path/to/config/file.kdl")),
            config_dir: Some(PathBuf::from("/path/to/config/dir")),
            should_ignore_config: true,
            configuration_options: Some(Options::default()),
            layout: None,
            terminal_window_size: Size { rows: 80, cols: 42 },
            data_dir: Some(PathBuf::from("/path/to/data/dir")),
            is_debug: true,
            max_panes: Some(4),
            force_run_layout_commands: true,
            cwd: Some(PathBuf::from("/path/to/cwd")),
        },
        is_web_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::FirstClientConnected {
        cli_assets: CliAssets {
            config_file_path: Some(PathBuf::from("/path/to/config/file.kdl")),
            config_dir: Some(PathBuf::from("/path/to/config/dir")),
            should_ignore_config: true,
            configuration_options: Some(Options {
                simplified_ui: Some(true),
                theme: Some("theme".to_owned()),
                default_mode: Some(InputMode::Normal),
                default_shell: Some(PathBuf::from("default_shell")),
                default_cwd: Some(PathBuf::from("default_cwd")),
                default_layout: Some(PathBuf::from("default_layout")),
                layout_dir: Some(PathBuf::from("layout_dir")),
                theme_dir: Some(PathBuf::from("theme_dir")),
                mouse_mode: Some(true),
                pane_frames: Some(true),
                mirror_session: Some(true),
                on_force_close: Some(OnForceClose::Quit),
                scroll_buffer_size: Some(100000),
                copy_command: Some("copy_command".to_owned()),
                copy_clipboard: Some(Clipboard::System),
                copy_on_select: Some(true),
                scrollback_editor: Some(PathBuf::from("scrollback_editor")),
                session_name: Some("session_name".to_owned()),
                attach_to_session: Some(true),
                auto_layout: Some(true),
                session_serialization: Some(true),
                serialize_pane_viewport: Some(true),
                scrollback_lines_to_serialize: Some(10000),
                styled_underlines: Some(true),
                serialization_interval: Some(1),
                disable_session_metadata: Some(true),
                support_kitty_keyboard_protocol: Some(true),
                web_server: Some(true),
                web_sharing: Some(WebSharing::On),
                stacked_resize: Some(true),
                show_startup_tips: Some(true),
                show_release_notes: Some(true),
                advanced_mouse_actions: Some(true),
                web_server_ip: Some("1.1.1.1".parse().unwrap()),
                web_server_port: Some(8080),
                web_server_cert: Some(PathBuf::from("web_server_cert")),
                web_server_key: Some(PathBuf::from("web_server_key")),
                enforce_https_for_localhost: Some(true),
                post_command_discovery_hook: Some("post_command_discovery_hook".to_owned()),
            }),
            layout: None,
            terminal_window_size: Size { rows: 80, cols: 42 },
            data_dir: Some(PathBuf::from("/path/to/data/dir")),
            is_debug: true,
            max_panes: Some(4),
            force_run_layout_commands: true,
            cwd: Some(PathBuf::from("/path/to/cwd")),
        },
        is_web_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::FirstClientConnected {
        cli_assets: CliAssets {
            configuration_options: Some(Options {
                default_mode: Some(InputMode::Normal),
                ..Default::default()
            }),
            ..Default::default()
        },
        is_web_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::FirstClientConnected {
        cli_assets: CliAssets {
            configuration_options: Some(Options {
                default_mode: Some(InputMode::Locked),
                ..Default::default()
            }),
            ..Default::default()
        },
        is_web_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::FirstClientConnected {
        cli_assets: CliAssets {
            configuration_options: Some(Options {
                default_mode: Some(InputMode::Resize),
                ..Default::default()
            }),
            ..Default::default()
        },
        is_web_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::FirstClientConnected {
        cli_assets: CliAssets {
            configuration_options: Some(Options {
                default_mode: Some(InputMode::Pane),
                ..Default::default()
            }),
            ..Default::default()
        },
        is_web_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::FirstClientConnected {
        cli_assets: CliAssets {
            configuration_options: Some(Options {
                default_mode: Some(InputMode::Tab),
                ..Default::default()
            }),
            ..Default::default()
        },
        is_web_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::FirstClientConnected {
        cli_assets: CliAssets {
            configuration_options: Some(Options {
                default_mode: Some(InputMode::Scroll),
                ..Default::default()
            }),
            ..Default::default()
        },
        is_web_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::FirstClientConnected {
        cli_assets: CliAssets {
            configuration_options: Some(Options {
                default_mode: Some(InputMode::EnterSearch),
                ..Default::default()
            }),
            ..Default::default()
        },
        is_web_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::FirstClientConnected {
        cli_assets: CliAssets {
            configuration_options: Some(Options {
                default_mode: Some(InputMode::Search),
                ..Default::default()
            }),
            ..Default::default()
        },
        is_web_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::FirstClientConnected {
        cli_assets: CliAssets {
            configuration_options: Some(Options {
                default_mode: Some(InputMode::RenameTab),
                ..Default::default()
            }),
            ..Default::default()
        },
        is_web_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::FirstClientConnected {
        cli_assets: CliAssets {
            configuration_options: Some(Options {
                default_mode: Some(InputMode::RenamePane),
                ..Default::default()
            }),
            ..Default::default()
        },
        is_web_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::FirstClientConnected {
        cli_assets: CliAssets {
            configuration_options: Some(Options {
                default_mode: Some(InputMode::Session),
                ..Default::default()
            }),
            ..Default::default()
        },
        is_web_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::FirstClientConnected {
        cli_assets: CliAssets {
            configuration_options: Some(Options {
                default_mode: Some(InputMode::Move),
                ..Default::default()
            }),
            ..Default::default()
        },
        is_web_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::FirstClientConnected {
        cli_assets: CliAssets {
            configuration_options: Some(Options {
                default_mode: Some(InputMode::Prompt),
                ..Default::default()
            }),
            ..Default::default()
        },
        is_web_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::FirstClientConnected {
        cli_assets: CliAssets {
            configuration_options: Some(Options {
                default_mode: Some(InputMode::Tmux),
                ..Default::default()
            }),
            ..Default::default()
        },
        is_web_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::FirstClientConnected {
        cli_assets: CliAssets {
            configuration_options: Some(Options {
                on_force_close: Some(OnForceClose::Detach),
                ..Default::default()
            }),
            ..Default::default()
        },
        is_web_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::FirstClientConnected {
        cli_assets: CliAssets {
            configuration_options: Some(Options {
                copy_clipboard: Some(Clipboard::Primary),
                ..Default::default()
            }),
            ..Default::default()
        },
        is_web_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::FirstClientConnected {
        cli_assets: CliAssets {
            configuration_options: Some(Options {
                web_sharing: Some(WebSharing::Off),
                ..Default::default()
            }),
            ..Default::default()
        },
        is_web_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::FirstClientConnected {
        cli_assets: CliAssets {
            configuration_options: Some(Options {
                web_sharing: Some(WebSharing::Disabled),
                ..Default::default()
            }),
            ..Default::default()
        },
        is_web_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::AttachClient {
        // cli_assets tested extensively ijn FirstClientConnected, we can skip it here
        cli_assets: CliAssets::default(),
        tab_position_to_focus: None,
        pane_to_focus: None,
        is_web_client: false,
    });
    test_client_roundtrip!(ClientToServerMsg::AttachClient {
        cli_assets: CliAssets::default(),
        tab_position_to_focus: Some(0),
        pane_to_focus: None,
        is_web_client: false,
    });
    test_client_roundtrip!(ClientToServerMsg::AttachClient {
        cli_assets: CliAssets::default(),
        tab_position_to_focus: None,
        pane_to_focus: Some(PaneReference {
            pane_id: 0,
            is_plugin: false,
        }),
        is_web_client: false,
    });
    test_client_roundtrip!(ClientToServerMsg::AttachClient {
        cli_assets: CliAssets::default(),
        tab_position_to_focus: None,
        pane_to_focus: Some(PaneReference {
            pane_id: 100,
            is_plugin: true,
        }),
        is_web_client: true,
    });
    // TODO: Action
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::Quit,
        terminal_id: None,
        client_id: None,
        is_cli_client: false,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::Quit,
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::Write {
            key_with_modifier: None,
            bytes: vec![],
            is_kitty_keyboard_protocol: false,
        },
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::Write {
            // KeyWithModifier is tested extensively in the Key ClientToServerMsg
            key_with_modifier: Some(KeyWithModifier::new(BareKey::Char('a'))),
            bytes: "a".as_bytes().to_vec(),
            is_kitty_keyboard_protocol: true,
        },
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::WriteChars {
            chars: "my chars".to_owned(),
        },
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::SwitchToMode {
            // InputMode is tested extensively in the Options conversion tests above
            input_mode: InputMode::Locked,
        },
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::SwitchModeForAllClients {
            // InputMode is tested extensively in the Options conversion tests above
            input_mode: InputMode::Locked,
        },
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::Resize {
            resize: Resize::Increase,
            direction: None,
        },
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::Resize {
            resize: Resize::Decrease,
            direction: None,
        },
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::Resize {
            resize: Resize::Decrease,
            direction: Some(Direction::Left),
        },
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::Resize {
            resize: Resize::Decrease,
            direction: Some(Direction::Right),
        },
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::Resize {
            resize: Resize::Decrease,
            direction: Some(Direction::Up),
        },
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::Resize {
            resize: Resize::Decrease,
            direction: Some(Direction::Down),
        },
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::FocusNextPane,
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::FocusPreviousPane,
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::SwitchFocus,
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::MoveFocus {
            // Direction is tested extensively elsewhere
            direction: Direction::Up,
        },
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::MoveFocusOrTab {
            direction: Direction::Up,
        },
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::MovePane { direction: None },
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::MovePane {
            direction: Some(Direction::Up),
        },
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::MovePaneBackwards,
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::ClearScreen,
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::DumpScreen {
            file_path: "/path/to/file".to_owned(),
            include_scrollback: false,
        },
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::DumpScreen {
            file_path: "/path/to/file".to_owned(),
            include_scrollback: true,
        },
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::DumpLayout,
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::EditScrollback,
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::ScrollUp,
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::ScrollUpAt {
            position: Position::new(0, 0),
        },
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::ScrollUpAt {
            position: Position::new(-10, 0),
        },
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::ScrollDown,
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::ScrollDownAt {
            position: Position::new(0, 0),
        },
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::ScrollToBottom,
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::ScrollToTop,
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::PageScrollUp,
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::PageScrollDown,
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::HalfPageScrollUp,
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::HalfPageScrollDown,
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::ToggleFocusFullscreen,
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::TogglePaneFrames,
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::ToggleActiveSyncTab,
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::NewPane {
            direction: None,
            pane_name: None,
            start_suppressed: false,
        },
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::NewPane {
            direction: Some(Direction::Right),
            pane_name: Some("pane_name".to_owned()),
            start_suppressed: true,
        },
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::EditFile {
            payload: OpenFilePayload {
                path: PathBuf::from("/file/path"),
                line_number: None,
                cwd: None,
                originating_plugin: None,
            },
            direction: None,
            floating: false,
            in_place: false,
            start_suppressed: false,
            coordinates: None,
            near_current_pane: false,
        },
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::EditFile {
            payload: OpenFilePayload {
                path: PathBuf::from("/file/path"),
                line_number: Some(1),
                cwd: Some(PathBuf::from("/my/cwd")),
                originating_plugin: Some(OriginatingPlugin {
                    plugin_id: 11,
                    client_id: 22,
                    context: empty_context.clone(),
                })
            },
            direction: None,
            floating: false,
            in_place: false,
            start_suppressed: false,
            coordinates: None,
            near_current_pane: false,
        },
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::EditFile {
            payload: OpenFilePayload {
                path: PathBuf::from("/file/path"),
                line_number: Some(1),
                cwd: Some(PathBuf::from("/my/cwd")),
                originating_plugin: Some(OriginatingPlugin {
                    plugin_id: 11,
                    client_id: 22,
                    context: demo_context.clone(),
                })
            },
            direction: None,
            floating: false,
            in_place: false,
            start_suppressed: false,
            coordinates: None,
            near_current_pane: false,
        },
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::EditFile {
            payload: OpenFilePayload {
                path: PathBuf::from("/file/path"),
                line_number: Some(1),
                cwd: Some(PathBuf::from("/my/cwd")),
                originating_plugin: Some(OriginatingPlugin {
                    plugin_id: 11,
                    client_id: 22,
                    context: demo_context.clone(),
                })
            },
            direction: Some(Direction::Right),
            floating: true,
            in_place: true,
            start_suppressed: true,
            coordinates: FloatingPaneCoordinates::new(None, None, None, None, None),
            near_current_pane: false,
        },
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::EditFile {
            payload: OpenFilePayload {
                path: PathBuf::from("/file/path"),
                line_number: Some(1),
                cwd: Some(PathBuf::from("/my/cwd")),
                originating_plugin: Some(OriginatingPlugin {
                    plugin_id: 11,
                    client_id: 22,
                    context: demo_context.clone(),
                })
            },
            direction: Some(Direction::Right),
            floating: true,
            in_place: true,
            start_suppressed: true,
            coordinates: FloatingPaneCoordinates::new(
                Some("100%".to_owned()),
                None,
                None,
                None,
                None
            ),
            near_current_pane: false,
        },
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::EditFile {
            payload: OpenFilePayload {
                path: PathBuf::from("/file/path"),
                line_number: Some(1),
                cwd: Some(PathBuf::from("/my/cwd")),
                originating_plugin: Some(OriginatingPlugin {
                    plugin_id: 11,
                    client_id: 22,
                    context: demo_context.clone(),
                })
            },
            direction: Some(Direction::Right),
            floating: true,
            in_place: true,
            start_suppressed: true,
            coordinates: FloatingPaneCoordinates::new(
                Some("10".to_owned()),
                None,
                None,
                None,
                None
            ),
            near_current_pane: false,
        },
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::EditFile {
            payload: OpenFilePayload {
                path: PathBuf::from("/file/path"),
                line_number: Some(1),
                cwd: Some(PathBuf::from("/my/cwd")),
                originating_plugin: Some(OriginatingPlugin {
                    plugin_id: 11,
                    client_id: 22,
                    context: demo_context.clone(),
                })
            },
            direction: Some(Direction::Right),
            floating: true,
            in_place: true,
            start_suppressed: true,
            coordinates: FloatingPaneCoordinates::new(
                Some("10".to_owned()),
                Some("50%".to_owned()),
                Some("10".to_owned()),
                Some("20".to_owned()),
                Some(true)
            ),
            near_current_pane: false,
        },
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::EditFile {
            payload: OpenFilePayload {
                path: PathBuf::from("/file/path"),
                line_number: Some(1),
                cwd: Some(PathBuf::from("/my/cwd")),
                originating_plugin: Some(OriginatingPlugin {
                    plugin_id: 11,
                    client_id: 22,
                    context: demo_context.clone(),
                })
            },
            direction: Some(Direction::Right),
            floating: true,
            in_place: true,
            start_suppressed: true,
            coordinates: FloatingPaneCoordinates::new(None, None, None, None, Some(false)),
            near_current_pane: false,
        },
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::NewFloatingPane {
            command: None,
            pane_name: None,
            coordinates: None,
            near_current_pane: false,
        },
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::NewFloatingPane {
            command: Some(RunCommandAction {
                command: PathBuf::from("/path/to/command"),
                args: vec![],
                cwd: None,
                direction: None,
                hold_on_close: false,
                hold_on_start: false,
                originating_plugin: None,
                use_terminal_title: false,
            }),
            pane_name: Some("my_pane_name".to_owned()),
            coordinates: FloatingPaneCoordinates::new(
                Some("10".to_owned()),
                None,
                None,
                None,
                None
            ),
            near_current_pane: false,
        },
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::NewFloatingPane {
            command: Some(RunCommandAction {
                command: PathBuf::from("/path/to/command"),
                args: vec!["arg1".to_owned(), "arg2".to_owned()],
                cwd: Some(PathBuf::from("/path/to/cwd")),
                direction: Some(Direction::Right),
                hold_on_close: true,
                hold_on_start: true,
                originating_plugin: Some(OriginatingPlugin {
                    plugin_id: 1,
                    client_id: 2,
                    context: demo_context.clone(),
                }),
                use_terminal_title: false,
            }),
            pane_name: Some("my_pane_name".to_owned()),
            coordinates: FloatingPaneCoordinates::new(
                Some("10".to_owned()),
                None,
                None,
                None,
                None
            ),
            near_current_pane: false,
        },
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::NewTiledPane {
            command: None,
            direction: None,
            pane_name: None,
            near_current_pane: false,
        },
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::NewTiledPane {
            command: Some(RunCommandAction {
                command: PathBuf::from("/path/to/command"),
                args: vec!["arg1".to_owned(), "arg2".to_owned()],
                cwd: Some(PathBuf::from("/path/to/cwd")),
                direction: Some(Direction::Right),
                hold_on_close: true,
                hold_on_start: true,
                originating_plugin: Some(OriginatingPlugin {
                    plugin_id: 1,
                    client_id: 2,
                    context: demo_context.clone(),
                }),
                use_terminal_title: false,
            }),
            direction: Some(Direction::Right),
            pane_name: Some("my_pane_name".to_owned()),
            near_current_pane: false,
        },
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::NewInPlacePane {
            command: Some(RunCommandAction {
                command: PathBuf::from("/path/to/command"),
                args: vec!["arg1".to_owned(), "arg2".to_owned()],
                cwd: Some(PathBuf::from("/path/to/cwd")),
                direction: Some(Direction::Right),
                hold_on_close: true,
                hold_on_start: true,
                originating_plugin: Some(OriginatingPlugin {
                    plugin_id: 1,
                    client_id: 2,
                    context: demo_context.clone(),
                }),
                use_terminal_title: false,
            }),
            pane_name: Some("my_pane_name".to_owned()),
            near_current_pane: false,
            pane_id_to_replace: None,
            close_replace_pane: false,
        },
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::NewInPlacePane {
            command: None,
            pane_name: None,
            near_current_pane: false,
            pane_id_to_replace: None,
            close_replace_pane: false,
        },
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::NewStackedPane {
            command: Some(RunCommandAction {
                command: PathBuf::from("/path/to/command"),
                args: vec!["arg1".to_owned(), "arg2".to_owned()],
                cwd: Some(PathBuf::from("/path/to/cwd")),
                direction: Some(Direction::Right),
                hold_on_close: true,
                hold_on_start: true,
                originating_plugin: Some(OriginatingPlugin {
                    plugin_id: 1,
                    client_id: 2,
                    context: demo_context.clone(),
                }),
                use_terminal_title: false,
            }),
            pane_name: Some("my_pane_name".to_owned()),
            near_current_pane: false,
        },
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::NewStackedPane {
            command: None,
            pane_name: None,
            near_current_pane: false,
        },
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::TogglePaneEmbedOrFloating,
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::ToggleFloatingPanes,
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::CloseFocus,
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::PaneNameInput {
            input: "name input".as_bytes().to_vec(),
        },
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::UndoRenamePane,
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::NewTab {
            tiled_layout: None,
            floating_layouts: vec![],
            swap_tiled_layouts: None,
            swap_floating_layouts: None,
            tab_name: None,
            should_change_focus_to_new_tab: false,
            cwd: None,
            initial_panes: None,
            first_pane_unblock_condition: None,
        },
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::NewTab {
            tiled_layout: Some(TiledPaneLayout::default()),
            floating_layouts: vec![],
            swap_tiled_layouts: None,
            swap_floating_layouts: None,
            tab_name: None,
            should_change_focus_to_new_tab: false,
            cwd: None,
            initial_panes: None,
            first_pane_unblock_condition: None,
        },
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::NewTab {
            tiled_layout: Some(TiledPaneLayout {
                children_split_direction: SplitDirection::Vertical,
                ..Default::default()
            }),
            floating_layouts: vec![],
            swap_tiled_layouts: None,
            swap_floating_layouts: None,
            tab_name: None,
            should_change_focus_to_new_tab: false,
            cwd: None,
            initial_panes: None,
            first_pane_unblock_condition: None,
        },
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::NewTab {
            tiled_layout: Some(TiledPaneLayout {
                children_split_direction: SplitDirection::Horizontal,
                ..Default::default()
            }),
            floating_layouts: vec![],
            swap_tiled_layouts: None,
            swap_floating_layouts: None,
            tab_name: None,
            should_change_focus_to_new_tab: false,
            cwd: None,
            initial_panes: None,
            first_pane_unblock_condition: None,
        },
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::NewTab {
            tiled_layout: Some(TiledPaneLayout {
                children_split_direction: SplitDirection::Horizontal,
                name: Some("tiled_layout_name".to_owned()),
                ..Default::default()
            }),
            floating_layouts: vec![],
            swap_tiled_layouts: None,
            swap_floating_layouts: None,
            tab_name: None,
            should_change_focus_to_new_tab: false,
            cwd: None,
            initial_panes: None,
            first_pane_unblock_condition: None,
        },
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::NewTab {
            tiled_layout: Some(TiledPaneLayout {
                children: vec![
                    TiledPaneLayout {
                        name: Some("first_item_in_vec".to_owned()),
                        ..Default::default()
                    },
                    TiledPaneLayout {
                        name: Some("second_item_in_vec".to_owned()),
                        ..Default::default()
                    },
                    TiledPaneLayout {
                        name: Some("third_item_in_vec".to_owned()),
                        ..Default::default()
                    },
                ],
                ..Default::default()
            }),
            floating_layouts: vec![],
            swap_tiled_layouts: None,
            swap_floating_layouts: None,
            tab_name: None,
            should_change_focus_to_new_tab: false,
            cwd: None,
            initial_panes: None,
            first_pane_unblock_condition: None,
        },
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::NewTab {
            tiled_layout: Some(TiledPaneLayout {
                split_size: Some(SplitSize::Fixed(1)),
                ..Default::default()
            }),
            floating_layouts: vec![],
            swap_tiled_layouts: None,
            swap_floating_layouts: None,
            tab_name: None,
            should_change_focus_to_new_tab: false,
            cwd: None,
            initial_panes: None,
            first_pane_unblock_condition: None,
        },
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::NewTab {
            tiled_layout: Some(TiledPaneLayout {
                split_size: Some(SplitSize::Percent(50)),
                ..Default::default()
            }),
            floating_layouts: vec![],
            swap_tiled_layouts: None,
            swap_floating_layouts: None,
            tab_name: None,
            should_change_focus_to_new_tab: false,
            cwd: None,
            initial_panes: None,
            first_pane_unblock_condition: None,
        },
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::NewTab {
            tiled_layout: Some(TiledPaneLayout {
                run: Some(Run::Plugin(RunPluginOrAlias::RunPlugin(
                    RunPlugin::default()
                ))),
                ..Default::default()
            }),
            floating_layouts: vec![],
            swap_tiled_layouts: None,
            swap_floating_layouts: None,
            tab_name: None,
            should_change_focus_to_new_tab: false,
            cwd: None,
            initial_panes: None,
            first_pane_unblock_condition: None,
        },
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::NewTab {
            tiled_layout: Some(TiledPaneLayout {
                run: Some(Run::Plugin(RunPluginOrAlias::RunPlugin(RunPlugin {
                    _allow_exec_host_cmd: true,
                    ..Default::default()
                }))),
                ..Default::default()
            }),
            floating_layouts: vec![],
            swap_tiled_layouts: None,
            swap_floating_layouts: None,
            tab_name: None,
            should_change_focus_to_new_tab: false,
            cwd: None,
            initial_panes: None,
            first_pane_unblock_condition: None,
        },
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::NewTab {
            tiled_layout: Some(TiledPaneLayout {
                run: Some(Run::Plugin(RunPluginOrAlias::RunPlugin(RunPlugin {
                    location: RunPluginLocation::File(PathBuf::from("/my/plugin/location")),
                    ..Default::default()
                }))),
                ..Default::default()
            }),
            floating_layouts: vec![],
            swap_tiled_layouts: None,
            swap_floating_layouts: None,
            tab_name: None,
            should_change_focus_to_new_tab: false,
            cwd: None,
            initial_panes: None,
            first_pane_unblock_condition: None,
        },
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::NewTab {
            tiled_layout: Some(TiledPaneLayout {
                run: Some(Run::Plugin(RunPluginOrAlias::RunPlugin(RunPlugin {
                    location: RunPluginLocation::Zellij(PluginTag::new("my_plugin_tag")),
                    ..Default::default()
                }))),
                ..Default::default()
            }),
            floating_layouts: vec![],
            swap_tiled_layouts: None,
            swap_floating_layouts: None,
            tab_name: None,
            should_change_focus_to_new_tab: false,
            cwd: None,
            initial_panes: None,
            first_pane_unblock_condition: None,
        },
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::NewTab {
            tiled_layout: Some(TiledPaneLayout {
                run: Some(Run::Plugin(RunPluginOrAlias::RunPlugin(RunPlugin {
                    location: RunPluginLocation::Remote("my_remote_url".to_owned()),
                    ..Default::default()
                }))),
                ..Default::default()
            }),
            floating_layouts: vec![],
            swap_tiled_layouts: None,
            swap_floating_layouts: None,
            tab_name: None,
            should_change_focus_to_new_tab: false,
            cwd: None,
            initial_panes: None,
            first_pane_unblock_condition: None,
        },
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::NewTab {
            tiled_layout: Some(TiledPaneLayout {
                run: Some(Run::Plugin(RunPluginOrAlias::RunPlugin(RunPlugin {
                    configuration: PluginUserConfiguration::new(demo_context.clone()),
                    ..Default::default()
                }))),
                ..Default::default()
            }),
            floating_layouts: vec![],
            swap_tiled_layouts: None,
            swap_floating_layouts: None,
            tab_name: None,
            should_change_focus_to_new_tab: false,
            cwd: None,
            initial_panes: None,
            first_pane_unblock_condition: None,
        },
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::NewTab {
            tiled_layout: Some(TiledPaneLayout {
                run: Some(Run::Plugin(RunPluginOrAlias::RunPlugin(RunPlugin {
                    configuration: PluginUserConfiguration::new(demo_context.clone()),
                    ..Default::default()
                }))),
                ..Default::default()
            }),
            floating_layouts: vec![],
            swap_tiled_layouts: None,
            swap_floating_layouts: None,
            tab_name: None,
            should_change_focus_to_new_tab: false,
            cwd: None,
            initial_panes: None,
            first_pane_unblock_condition: None,
        },
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::NewTab {
            tiled_layout: Some(TiledPaneLayout {
                run: Some(Run::Plugin(RunPluginOrAlias::RunPlugin(RunPlugin {
                    configuration: PluginUserConfiguration::new(empty_context.clone()),
                    ..Default::default()
                }))),
                ..Default::default()
            }),
            floating_layouts: vec![],
            swap_tiled_layouts: None,
            swap_floating_layouts: None,
            tab_name: None,
            should_change_focus_to_new_tab: false,
            cwd: None,
            initial_panes: None,
            first_pane_unblock_condition: None,
        },
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::NewTab {
            tiled_layout: Some(TiledPaneLayout {
                run: Some(Run::Plugin(RunPluginOrAlias::RunPlugin(RunPlugin {
                    initial_cwd: Some(PathBuf::from("/initial/cwd")),
                    ..Default::default()
                }))),
                ..Default::default()
            }),
            floating_layouts: vec![],
            swap_tiled_layouts: None,
            swap_floating_layouts: None,
            tab_name: None,
            should_change_focus_to_new_tab: false,
            cwd: None,
            initial_panes: None,
            first_pane_unblock_condition: None,
        },
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::NewTab {
            tiled_layout: Some(TiledPaneLayout {
                run: Some(Run::Plugin(RunPluginOrAlias::Alias(PluginAlias::default()))),
                ..Default::default()
            }),
            floating_layouts: vec![],
            swap_tiled_layouts: None,
            swap_floating_layouts: None,
            tab_name: None,
            should_change_focus_to_new_tab: false,
            cwd: None,
            initial_panes: None,
            first_pane_unblock_condition: None,
        },
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::NewTab {
            tiled_layout: Some(TiledPaneLayout {
                run: Some(Run::Plugin(RunPluginOrAlias::Alias(PluginAlias {
                    name: "alias name".to_owned(),
                    configuration: Some(PluginUserConfiguration::new(demo_context.clone())),
                    initial_cwd: Some(PathBuf::from("initial_cwd")),
                    run_plugin: Some(RunPlugin::default()),
                    ..Default::default()
                }))),
                ..Default::default()
            }),
            floating_layouts: vec![],
            swap_tiled_layouts: None,
            swap_floating_layouts: None,
            tab_name: None,
            should_change_focus_to_new_tab: false,
            cwd: None,
            initial_panes: None,
            first_pane_unblock_condition: None,
        },
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::NewTab {
            tiled_layout: Some(TiledPaneLayout {
                run: Some(Run::Command(RunCommand {
                    command: PathBuf::from("/path/to/command"),
                    args: vec![],
                    cwd: None,
                    hold_on_close: false,
                    hold_on_start: false,
                    originating_plugin: None,
                    use_terminal_title: true,
                })),
                ..Default::default()
            }),
            floating_layouts: vec![],
            swap_tiled_layouts: None,
            swap_floating_layouts: None,
            tab_name: None,
            should_change_focus_to_new_tab: false,
            cwd: None,
            initial_panes: None,
            first_pane_unblock_condition: None,
        },
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::NewTab {
            tiled_layout: Some(TiledPaneLayout {
                run: Some(Run::Command(RunCommand {
                    command: PathBuf::from("/path/to/command"),
                    args: vec!["arg1".to_owned(), "arg2".to_owned(), "arg3".to_owned()],
                    cwd: Some(PathBuf::from("/path/to/cwd")),
                    hold_on_close: true,
                    hold_on_start: true,
                    originating_plugin: Some(OriginatingPlugin {
                        plugin_id: 11,
                        client_id: 22,
                        context: empty_context.clone(),
                    }),
                    use_terminal_title: true,
                })),
                ..Default::default()
            }),
            floating_layouts: vec![],
            swap_tiled_layouts: None,
            swap_floating_layouts: None,
            tab_name: None,
            should_change_focus_to_new_tab: false,
            cwd: None,
            initial_panes: None,
            first_pane_unblock_condition: None,
        },
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::NewTab {
            tiled_layout: Some(TiledPaneLayout {
                run: Some(Run::EditFile(PathBuf::from("/path/to/file"), None, None)),
                ..Default::default()
            }),
            floating_layouts: vec![],
            swap_tiled_layouts: None,
            swap_floating_layouts: None,
            tab_name: None,
            should_change_focus_to_new_tab: false,
            cwd: None,
            initial_panes: None,
            first_pane_unblock_condition: None,
        },
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::NewTab {
            tiled_layout: Some(TiledPaneLayout {
                run: Some(Run::EditFile(
                    PathBuf::from("/path/to/file"),
                    Some(10),
                    Some(PathBuf::from("/path"))
                )),
                ..Default::default()
            }),
            floating_layouts: vec![],
            swap_tiled_layouts: None,
            swap_floating_layouts: None,
            tab_name: None,
            should_change_focus_to_new_tab: false,
            cwd: None,
            initial_panes: None,
            first_pane_unblock_condition: None,
        },
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::NewTab {
            tiled_layout: Some(TiledPaneLayout {
                run: Some(Run::Cwd(PathBuf::from("/path/to/cwd"))),
                ..Default::default()
            }),
            floating_layouts: vec![],
            swap_tiled_layouts: None,
            swap_floating_layouts: None,
            tab_name: None,
            should_change_focus_to_new_tab: false,
            cwd: None,
            initial_panes: None,
            first_pane_unblock_condition: None,
        },
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::NewTab {
            tiled_layout: None,
            floating_layouts: vec![
                FloatingPaneLayout {
                    name: Some("first floating layout".to_owned()),
                    ..Default::default()
                },
                FloatingPaneLayout {
                    name: Some("third floating layout".to_owned()),
                    height: Some(PercentOrFixed::Percent(10)),
                    width: Some(PercentOrFixed::Fixed(20)),
                    x: Some(PercentOrFixed::Percent(30)),
                    y: Some(PercentOrFixed::Percent(40)),
                    pinned: Some(true),
                    run: Some(Run::Cwd(PathBuf::from("/path/to/cwd"))),
                    focus: Some(true),
                    already_running: true,
                    pane_initial_contents: Some("pane_initial_contents".to_owned()),
                    logical_position: Some(15),
                },
                FloatingPaneLayout {
                    name: Some("third floating layout".to_owned()),
                    ..Default::default()
                },
                FloatingPaneLayout::default(),
            ],
            swap_tiled_layouts: None,
            swap_floating_layouts: None,
            tab_name: None,
            should_change_focus_to_new_tab: false,
            cwd: None,
            initial_panes: None,
            first_pane_unblock_condition: None,
        },
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::NewTab {
            tiled_layout: None,
            floating_layouts: vec![],
            swap_tiled_layouts: Some(vec![
                swap_tiled_layouts_1.clone(),
                swap_tiled_layouts_2.clone(),
                swap_tiled_layouts_3.clone()
            ]),
            swap_floating_layouts: Some(vec![
                swap_floating_layouts_1,
                swap_floating_layouts_2,
                swap_floating_layouts_3
            ]),
            tab_name: None,
            should_change_focus_to_new_tab: false,
            cwd: None,
            initial_panes: None,
            first_pane_unblock_condition: None,
        },
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::NewTab {
            tiled_layout: None,
            floating_layouts: vec![],
            swap_tiled_layouts: None,
            swap_floating_layouts: None,
            tab_name: Some("tab_name".to_owned()),
            should_change_focus_to_new_tab: false,
            cwd: None,
            initial_panes: None,
            first_pane_unblock_condition: None,
        },
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::NewTab {
            tiled_layout: None,
            floating_layouts: vec![],
            swap_tiled_layouts: None,
            swap_floating_layouts: None,
            tab_name: None,
            should_change_focus_to_new_tab: true,
            cwd: Some(PathBuf::from("relative/path/to/cwd")),
            initial_panes: None,
            first_pane_unblock_condition: None,
        },
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::NewTab {
            tiled_layout: None,
            floating_layouts: vec![],
            swap_tiled_layouts: None,
            swap_floating_layouts: None,
            tab_name: None,
            should_change_focus_to_new_tab: true,
            cwd: None,
            initial_panes: Some(vec![
                CommandOrPlugin::Command(RunCommandAction {
                    command: PathBuf::from("/path/to/command"),
                    args: vec![],
                    cwd: None,
                    direction: None,
                    hold_on_close: false,
                    hold_on_start: false,
                    originating_plugin: None,
                    use_terminal_title: false,
                }),
                CommandOrPlugin::Plugin(RunPluginOrAlias::RunPlugin(RunPlugin::default())),
            ]),
            first_pane_unblock_condition: None,
        },
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::OverrideLayout {
            tiled_layout: None,
            floating_layouts: vec![],
            swap_tiled_layouts: None,
            swap_floating_layouts: None,
            tab_name: None,
            retain_existing_terminal_panes: false,
            retain_existing_plugin_panes: false,
        },
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::NoOp,
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::GoToNextTab,
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::GoToPreviousTab,
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::CloseTab,
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::GoToTab { index: 0 },
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::GoToTabName {
            name: "tab_name".to_owned(),
            create: false,
        },
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::GoToTabName {
            name: "tab_name".to_owned(),
            create: true,
        },
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::ToggleTab,
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::TabNameInput {
            input: "my cool tab_name".as_bytes().to_vec(),
        },
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::UndoRenameTab,
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::MoveTab {
            direction: Direction::Right,
        },
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::Run {
            // RunCommandAction serialization roundtrip is tested extensively upwards of here
            command: RunCommandAction {
                command: PathBuf::from("/path/to/command"),
                args: vec![],
                cwd: None,
                direction: None,
                hold_on_close: false,
                hold_on_start: false,
                originating_plugin: None,
                use_terminal_title: false,
            },
            near_current_pane: false,
        },
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::Detach,
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::LaunchOrFocusPlugin {
            // RunPluginOrAlias serialization roundtrip is tested extensively upwards
            plugin: RunPluginOrAlias::RunPlugin(RunPlugin::default()),
            should_float: true,
            move_to_focused_tab: true,
            should_open_in_place: true,
            skip_cache: true,
        },
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::LaunchOrFocusPlugin {
            // RunPluginOrAlias serialization roundtrip is tested extensively upwards
            plugin: RunPluginOrAlias::Alias(PluginAlias::default()),
            should_float: false,
            move_to_focused_tab: false,
            should_open_in_place: false,
            skip_cache: false,
        },
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::LaunchPlugin {
            // RunPluginOrAlias serialization roundtrip is tested extensively upwards
            plugin: RunPluginOrAlias::RunPlugin(RunPlugin::default()),
            should_float: true,
            should_open_in_place: true,
            skip_cache: true,
            cwd: None,
        },
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::LaunchPlugin {
            // RunPluginOrAlias serialization roundtrip is tested extensively upwards
            plugin: RunPluginOrAlias::Alias(PluginAlias::default()),
            should_float: false,
            should_open_in_place: false,
            skip_cache: false,
            cwd: Some(PathBuf::from("/path/to/cwd")),
        },
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::MouseEvent {
            event: MouseEvent {
                event_type: MouseEventType::Motion,
                left: true,
                right: true,
                middle: true,
                wheel_up: true,
                wheel_down: true,
                shift: true,
                alt: true,
                ctrl: true,
                position: Position::new(-10, 0),
            }
        },
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::MouseEvent {
            event: MouseEvent {
                event_type: MouseEventType::Press,
                left: true,
                right: true,
                middle: true,
                wheel_up: true,
                wheel_down: true,
                shift: true,
                alt: true,
                ctrl: true,
                position: Position::new(-10, 0),
            }
        },
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::MouseEvent {
            event: MouseEvent {
                event_type: MouseEventType::Release,
                left: true,
                right: true,
                middle: true,
                wheel_up: true,
                wheel_down: true,
                shift: true,
                alt: true,
                ctrl: true,
                position: Position::new(10, 0),
            }
        },
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::Copy,
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::Confirm,
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::Deny,
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::SkipConfirm {
            action: Box::new(Action::Quit)
        },
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::SearchInput {
            input: "my_search_input".as_bytes().to_vec(),
        },
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::Search {
            direction: SearchDirection::Up,
        },
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::Search {
            direction: SearchDirection::Down,
        },
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::SearchToggleOption {
            option: SearchOption::CaseSensitivity,
        },
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::SearchToggleOption {
            option: SearchOption::WholeWord,
        },
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::SearchToggleOption {
            option: SearchOption::Wrap,
        },
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::ToggleMouseMode,
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::PreviousSwapLayout,
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::NextSwapLayout,
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::QueryTabNames,
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::NewTiledPluginPane {
            plugin: RunPluginOrAlias::Alias(PluginAlias::default()),
            pane_name: Some("my_pane_name".to_owned()),
            skip_cache: false,
            cwd: Some(PathBuf::from("relative/path/to/cwd")),
        },
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::NewFloatingPluginPane {
            plugin: RunPluginOrAlias::RunPlugin(RunPlugin::default()),
            pane_name: Some("my_pane_name".to_owned()),
            skip_cache: true,
            cwd: Some(PathBuf::from("relative/path/to/cwd")),
            coordinates: None,
        },
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::NewFloatingPluginPane {
            plugin: RunPluginOrAlias::Alias(PluginAlias::default()),
            pane_name: Some("my_pane_name".to_owned()),
            skip_cache: true,
            cwd: Some(PathBuf::from("relative/path/to/cwd")),
            coordinates: FloatingPaneCoordinates::new(
                Some("10".to_owned()),
                Some("10%".to_owned()),
                None,
                None,
                Some(true)
            ),
        },
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::NewInPlacePluginPane {
            plugin: RunPluginOrAlias::Alias(PluginAlias::default()),
            pane_name: Some("my_pane_name".to_owned()),
            skip_cache: true,
        },
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::StartOrReloadPlugin {
            plugin: RunPluginOrAlias::Alias(PluginAlias::default()),
        },
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::CloseTerminalPane { pane_id: 11 },
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::ClosePluginPane { pane_id: 12 },
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::FocusTerminalPaneWithId {
            pane_id: 12,
            should_float_if_hidden: false,
            should_be_in_place_if_hidden: false,
        },
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::FocusTerminalPaneWithId {
            pane_id: 12,
            should_float_if_hidden: true,
            should_be_in_place_if_hidden: false,
        },
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::FocusPluginPaneWithId {
            pane_id: 12,
            should_float_if_hidden: false,
            should_be_in_place_if_hidden: true,
        },
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::FocusPluginPaneWithId {
            pane_id: 12,
            should_float_if_hidden: true,
            should_be_in_place_if_hidden: false,
        },
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::RenameTerminalPane {
            pane_id: 12,
            name: "terminal_pane_new_name".as_bytes().to_vec(),
        },
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::RenamePluginPane {
            pane_id: 12,
            name: "plugin_pane_new_name".as_bytes().to_vec(),
        },
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::RenameTab {
            tab_index: 11,
            name: "tab_new_name".as_bytes().to_vec(),
        },
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::BreakPane,
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::BreakPaneRight,
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::BreakPaneLeft,
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::RenameSession {
            name: "new_session_name".to_owned(),
        },
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::CliPipe {
            pipe_id: "pipe_id_name".to_owned(),
            name: None,
            payload: None,
            args: None,
            plugin: None,
            configuration: None,
            launch_new: false,
            skip_cache: false,
            floating: None,
            in_place: None,
            cwd: None,
            pane_title: None,
        },
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::CliPipe {
            pipe_id: "pipe_id_name".to_owned(),
            name: Some("pipe_name".to_owned()),
            payload: Some("pipe_payload".to_owned()),
            args: Some(demo_context.clone()),
            plugin: Some("zellij:status-bare".to_owned()),
            configuration: Some(demo_context.to_owned()),
            launch_new: true,
            skip_cache: true,
            floating: Some(true),
            in_place: Some(false),
            cwd: Some(PathBuf::from("/path/to/cwd")),
            pane_title: Some("pane_title".to_owned()),
        },
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::KeybindPipe {
            plugin_id: None,
            name: None,
            payload: None,
            args: None,
            plugin: None,
            configuration: None,
            launch_new: false,
            skip_cache: false,
            floating: None,
            in_place: None,
            cwd: None,
            pane_title: None,
        },
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::KeybindPipe {
            plugin_id: Some(112),
            name: Some("pipe_name".to_owned()),
            payload: Some("pipe_payload".to_owned()),
            args: Some(demo_context.clone()),
            plugin: Some("zellij:status-bare".to_owned()),
            configuration: Some(demo_context.to_owned()),
            launch_new: true,
            skip_cache: true,
            floating: Some(true),
            in_place: Some(false),
            cwd: Some(PathBuf::from("/path/to/cwd")),
            pane_title: Some("pane_title".to_owned()),
        },
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::ListClients,
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::TogglePanePinned,
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::StackPanes { pane_ids: vec![] },
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::StackPanes {
            pane_ids: vec![
                PaneId::Terminal(0),
                PaneId::Plugin(1),
                PaneId::Terminal(2),
                PaneId::Plugin(3)
            ],
        },
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::ChangeFloatingPaneCoordinates {
            pane_id: PaneId::Terminal(0),
            coordinates: FloatingPaneCoordinates::new(
                None,
                None,
                None,
                Some("10%".to_owned()),
                Some(false)
            )
            .unwrap(),
        },
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::TogglePaneInGroup,
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Action {
        action: Action::ToggleGroupMarking,
        terminal_id: Some(1),
        client_id: Some(100),
        is_cli_client: true,
    });
    test_client_roundtrip!(ClientToServerMsg::Key {
        key: KeyWithModifier {
            bare_key: BareKey::PageDown,
            key_modifiers: BTreeSet::new(),
        },
        raw_bytes: "raw_bytes".as_bytes().to_vec(),
        is_kitty_keyboard_protocol: false,
    });
    test_client_roundtrip!(ClientToServerMsg::Key {
        key: KeyWithModifier {
            bare_key: BareKey::PageDown,
            key_modifiers: demo_modifiers_1,
        },
        raw_bytes: "raw_bytes".as_bytes().to_vec(),
        is_kitty_keyboard_protocol: false,
    });
    test_client_roundtrip!(ClientToServerMsg::Key {
        key: KeyWithModifier {
            bare_key: BareKey::PageDown,
            key_modifiers: demo_modifiers_2,
        },
        raw_bytes: "raw_bytes".as_bytes().to_vec(),
        is_kitty_keyboard_protocol: false,
    });
    test_client_roundtrip!(ClientToServerMsg::Key {
        key: KeyWithModifier {
            bare_key: BareKey::PageDown,
            key_modifiers: demo_modifiers_3,
        },
        raw_bytes: "raw_bytes".as_bytes().to_vec(),
        is_kitty_keyboard_protocol: false,
    });
    test_client_roundtrip!(ClientToServerMsg::Key {
        key: KeyWithModifier {
            bare_key: BareKey::PageDown,
            key_modifiers: demo_modifiers_4,
        },
        raw_bytes: "raw_bytes".as_bytes().to_vec(),
        is_kitty_keyboard_protocol: false,
    });
    test_client_roundtrip!(ClientToServerMsg::Key {
        key: KeyWithModifier {
            bare_key: BareKey::PageUp,
            key_modifiers: BTreeSet::new(),
        },
        raw_bytes: "raw_bytes".as_bytes().to_vec(),
        is_kitty_keyboard_protocol: false,
    });
    test_client_roundtrip!(ClientToServerMsg::Key {
        key: KeyWithModifier {
            bare_key: BareKey::Down,
            key_modifiers: BTreeSet::new(),
        },
        raw_bytes: "raw_bytes".as_bytes().to_vec(),
        is_kitty_keyboard_protocol: false,
    });
    test_client_roundtrip!(ClientToServerMsg::Key {
        key: KeyWithModifier {
            bare_key: BareKey::Up,
            key_modifiers: BTreeSet::new(),
        },
        raw_bytes: "raw_bytes".as_bytes().to_vec(),
        is_kitty_keyboard_protocol: false,
    });
    test_client_roundtrip!(ClientToServerMsg::Key {
        key: KeyWithModifier {
            bare_key: BareKey::Right,
            key_modifiers: BTreeSet::new(),
        },
        raw_bytes: "raw_bytes".as_bytes().to_vec(),
        is_kitty_keyboard_protocol: false,
    });
    test_client_roundtrip!(ClientToServerMsg::Key {
        key: KeyWithModifier {
            bare_key: BareKey::Home,
            key_modifiers: BTreeSet::new(),
        },
        raw_bytes: "raw_bytes".as_bytes().to_vec(),
        is_kitty_keyboard_protocol: false,
    });
    test_client_roundtrip!(ClientToServerMsg::Key {
        key: KeyWithModifier {
            bare_key: BareKey::End,
            key_modifiers: BTreeSet::new(),
        },
        raw_bytes: "raw_bytes".as_bytes().to_vec(),
        is_kitty_keyboard_protocol: false,
    });
    test_client_roundtrip!(ClientToServerMsg::Key {
        key: KeyWithModifier {
            bare_key: BareKey::Backspace,
            key_modifiers: BTreeSet::new(),
        },
        raw_bytes: "raw_bytes".as_bytes().to_vec(),
        is_kitty_keyboard_protocol: false,
    });
    test_client_roundtrip!(ClientToServerMsg::Key {
        key: KeyWithModifier {
            bare_key: BareKey::Delete,
            key_modifiers: BTreeSet::new(),
        },
        raw_bytes: "raw_bytes".as_bytes().to_vec(),
        is_kitty_keyboard_protocol: false,
    });
    test_client_roundtrip!(ClientToServerMsg::Key {
        key: KeyWithModifier {
            bare_key: BareKey::Insert,
            key_modifiers: BTreeSet::new(),
        },
        raw_bytes: "raw_bytes".as_bytes().to_vec(),
        is_kitty_keyboard_protocol: false,
    });
    test_client_roundtrip!(ClientToServerMsg::Key {
        key: KeyWithModifier {
            bare_key: BareKey::Tab,
            key_modifiers: BTreeSet::new(),
        },
        raw_bytes: "raw_bytes".as_bytes().to_vec(),
        is_kitty_keyboard_protocol: false,
    });
    test_client_roundtrip!(ClientToServerMsg::Key {
        key: KeyWithModifier {
            bare_key: BareKey::Esc,
            key_modifiers: BTreeSet::new(),
        },
        raw_bytes: "raw_bytes".as_bytes().to_vec(),
        is_kitty_keyboard_protocol: false,
    });
    test_client_roundtrip!(ClientToServerMsg::Key {
        key: KeyWithModifier {
            bare_key: BareKey::Enter,
            key_modifiers: BTreeSet::new(),
        },
        raw_bytes: "raw_bytes".as_bytes().to_vec(),
        is_kitty_keyboard_protocol: false,
    });
    test_client_roundtrip!(ClientToServerMsg::Key {
        key: KeyWithModifier {
            bare_key: BareKey::CapsLock,
            key_modifiers: BTreeSet::new(),
        },
        raw_bytes: "raw_bytes".as_bytes().to_vec(),
        is_kitty_keyboard_protocol: false,
    });
    test_client_roundtrip!(ClientToServerMsg::Key {
        key: KeyWithModifier {
            bare_key: BareKey::ScrollLock,
            key_modifiers: BTreeSet::new(),
        },
        raw_bytes: "raw_bytes".as_bytes().to_vec(),
        is_kitty_keyboard_protocol: false,
    });
    test_client_roundtrip!(ClientToServerMsg::Key {
        key: KeyWithModifier {
            bare_key: BareKey::NumLock,
            key_modifiers: BTreeSet::new(),
        },
        raw_bytes: "raw_bytes".as_bytes().to_vec(),
        is_kitty_keyboard_protocol: false,
    });
    test_client_roundtrip!(ClientToServerMsg::Key {
        key: KeyWithModifier {
            bare_key: BareKey::PrintScreen,
            key_modifiers: BTreeSet::new(),
        },
        raw_bytes: "raw_bytes".as_bytes().to_vec(),
        is_kitty_keyboard_protocol: false,
    });
    test_client_roundtrip!(ClientToServerMsg::Key {
        key: KeyWithModifier {
            bare_key: BareKey::Pause,
            key_modifiers: BTreeSet::new(),
        },
        raw_bytes: "raw_bytes".as_bytes().to_vec(),
        is_kitty_keyboard_protocol: false,
    });
    test_client_roundtrip!(ClientToServerMsg::Key {
        key: KeyWithModifier {
            bare_key: BareKey::Menu,
            key_modifiers: BTreeSet::new(),
        },
        raw_bytes: "raw_bytes".as_bytes().to_vec(),
        is_kitty_keyboard_protocol: false,
    });
    test_client_roundtrip!(ClientToServerMsg::Key {
        key: KeyWithModifier {
            bare_key: BareKey::F(1),
            key_modifiers: BTreeSet::new(),
        },
        raw_bytes: "raw_bytes".as_bytes().to_vec(),
        is_kitty_keyboard_protocol: false,
    });
    test_client_roundtrip!(ClientToServerMsg::Key {
        key: KeyWithModifier {
            bare_key: BareKey::F(2),
            key_modifiers: BTreeSet::new(),
        },
        raw_bytes: "raw_bytes".as_bytes().to_vec(),
        is_kitty_keyboard_protocol: false,
    });
    test_client_roundtrip!(ClientToServerMsg::Key {
        key: KeyWithModifier {
            bare_key: BareKey::F(3),
            key_modifiers: BTreeSet::new(),
        },
        raw_bytes: "raw_bytes".as_bytes().to_vec(),
        is_kitty_keyboard_protocol: false,
    });
    test_client_roundtrip!(ClientToServerMsg::Key {
        key: KeyWithModifier {
            bare_key: BareKey::F(4),
            key_modifiers: BTreeSet::new(),
        },
        raw_bytes: "raw_bytes".as_bytes().to_vec(),
        is_kitty_keyboard_protocol: false,
    });
    test_client_roundtrip!(ClientToServerMsg::Key {
        key: KeyWithModifier {
            bare_key: BareKey::F(5),
            key_modifiers: BTreeSet::new(),
        },
        raw_bytes: "raw_bytes".as_bytes().to_vec(),
        is_kitty_keyboard_protocol: false,
    });
    test_client_roundtrip!(ClientToServerMsg::Key {
        key: KeyWithModifier {
            bare_key: BareKey::F(6),
            key_modifiers: BTreeSet::new(),
        },
        raw_bytes: "raw_bytes".as_bytes().to_vec(),
        is_kitty_keyboard_protocol: false,
    });
    test_client_roundtrip!(ClientToServerMsg::Key {
        key: KeyWithModifier {
            bare_key: BareKey::F(7),
            key_modifiers: BTreeSet::new(),
        },
        raw_bytes: "raw_bytes".as_bytes().to_vec(),
        is_kitty_keyboard_protocol: false,
    });
    test_client_roundtrip!(ClientToServerMsg::Key {
        key: KeyWithModifier {
            bare_key: BareKey::F(8),
            key_modifiers: BTreeSet::new(),
        },
        raw_bytes: "raw_bytes".as_bytes().to_vec(),
        is_kitty_keyboard_protocol: false,
    });
    test_client_roundtrip!(ClientToServerMsg::Key {
        key: KeyWithModifier {
            bare_key: BareKey::F(9),
            key_modifiers: BTreeSet::new(),
        },
        raw_bytes: "raw_bytes".as_bytes().to_vec(),
        is_kitty_keyboard_protocol: false,
    });
    test_client_roundtrip!(ClientToServerMsg::Key {
        key: KeyWithModifier {
            bare_key: BareKey::F(10),
            key_modifiers: BTreeSet::new(),
        },
        raw_bytes: "raw_bytes".as_bytes().to_vec(),
        is_kitty_keyboard_protocol: false,
    });
    test_client_roundtrip!(ClientToServerMsg::Key {
        key: KeyWithModifier {
            bare_key: BareKey::F(11),
            key_modifiers: BTreeSet::new(),
        },
        raw_bytes: "raw_bytes".as_bytes().to_vec(),
        is_kitty_keyboard_protocol: false,
    });
    test_client_roundtrip!(ClientToServerMsg::Key {
        key: KeyWithModifier {
            bare_key: BareKey::F(12),
            key_modifiers: BTreeSet::new(),
        },
        raw_bytes: "raw_bytes".as_bytes().to_vec(),
        is_kitty_keyboard_protocol: false,
    });
    test_client_roundtrip!(ClientToServerMsg::Key {
        key: KeyWithModifier {
            bare_key: BareKey::Char('a'),
            key_modifiers: BTreeSet::new(),
        },
        raw_bytes: "raw_bytes".as_bytes().to_vec(),
        is_kitty_keyboard_protocol: false,
    });
    test_client_roundtrip!(ClientToServerMsg::ClientExited);
    test_client_roundtrip!(ClientToServerMsg::KillSession);
    test_client_roundtrip!(ClientToServerMsg::ConnStatus);
    test_client_roundtrip!(ClientToServerMsg::WebServerStarted {
        base_url: "http://localhost:8080".to_string(),
    });
    test_client_roundtrip!(ClientToServerMsg::FailedToStartWebServer {
        error: "Port already in use".to_string(),
    });
}

fn test_server_messages() {
    test_server_roundtrip!(ServerToClientMsg::Render {
        content: "Hello, World!".to_string(),
    });
    test_server_roundtrip!(ServerToClientMsg::Render {
        content: "".to_string(),
    });
    test_server_roundtrip!(ServerToClientMsg::Render {
        content: "x".repeat(10000),
    });
    test_server_roundtrip!(ServerToClientMsg::UnblockInputThread);
    test_server_roundtrip!(ServerToClientMsg::Connected);
    test_server_roundtrip!(ServerToClientMsg::QueryTerminalSize);
    test_server_roundtrip!(ServerToClientMsg::StartWebServer);
    test_server_roundtrip!(ServerToClientMsg::ConfigFileUpdated);
    test_server_roundtrip!(ServerToClientMsg::RenamedSession {
        name: "my-session".to_string(),
    });
    test_server_roundtrip!(ServerToClientMsg::UnblockCliPipeInput {
        pipe_name: "stdout".to_string(),
    });
    test_server_roundtrip!(ServerToClientMsg::CliPipeOutput {
        pipe_name: "stderr".to_string(),
        output: "Error occurred\n".to_string(),
    });
    test_server_roundtrip!(ServerToClientMsg::Exit {
        exit_reason: ExitReason::Normal,
    });
    test_server_roundtrip!(ServerToClientMsg::Exit {
        exit_reason: ExitReason::Error("Something went wrong".to_string()),
    });
    test_server_roundtrip!(ServerToClientMsg::Exit {
        exit_reason: ExitReason::NormalDetached,
    });
    test_server_roundtrip!(ServerToClientMsg::Exit {
        exit_reason: ExitReason::ForceDetached,
    });
    test_server_roundtrip!(ServerToClientMsg::Exit {
        exit_reason: ExitReason::CannotAttach,
    });
    test_server_roundtrip!(ServerToClientMsg::Exit {
        exit_reason: ExitReason::Disconnect,
    });
    test_server_roundtrip!(ServerToClientMsg::Exit {
        exit_reason: ExitReason::WebClientsForbidden,
    });
    test_server_roundtrip!(ServerToClientMsg::Log { lines: vec![] });
    test_server_roundtrip!(ServerToClientMsg::Log {
        lines: vec![
            "Starting server...".to_string(),
            "Server ready on port 8080".to_string(),
            "Accepting connections".to_string(),
        ],
    });
    test_server_roundtrip!(ServerToClientMsg::LogError { lines: vec![] });
    test_server_roundtrip!(ServerToClientMsg::LogError {
        lines: vec![
            "ERROR: Failed to bind socket".to_string(),
            "ERROR: Retrying in 5 seconds".to_string(),
        ],
    });
    test_server_roundtrip!(ServerToClientMsg::SwitchSession {
        connect_to_session: ConnectToSession::default(),
    });
    test_server_roundtrip!(ServerToClientMsg::SwitchSession {
        connect_to_session: ConnectToSession {
            name: Some("new_session_name".to_owned()),
            tab_position: Some(5),
            pane_id: Some((5, true)),
            layout: Some(LayoutInfo::BuiltIn("compact".to_owned())),
            cwd: Some(PathBuf::from("/path/to/cwd")),
        }
    });
    test_server_roundtrip!(ServerToClientMsg::SwitchSession {
        connect_to_session: ConnectToSession {
            name: Some("new_session_name".to_owned()),
            tab_position: Some(5),
            pane_id: Some((5, true)),
            layout: Some(LayoutInfo::File("/path/to/my/file.kdl".to_owned())),
            cwd: Some(PathBuf::from("/path/to/cwd")),
        }
    });
    test_server_roundtrip!(ServerToClientMsg::SwitchSession {
        connect_to_session: ConnectToSession {
            name: Some("new_session_name".to_owned()),
            tab_position: Some(5),
            pane_id: Some((5, true)),
            layout: Some(LayoutInfo::Url("https://example.com/layout.kdl".to_owned())),
            cwd: Some(PathBuf::from("/path/to/cwd")),
        }
    });
    test_server_roundtrip!(ServerToClientMsg::SwitchSession {
        connect_to_session: ConnectToSession {
            name: Some("new_session_name".to_owned()),
            tab_position: Some(5),
            pane_id: Some((5, true)),
            layout: Some(LayoutInfo::Stringified(
                "layout { pane; pane; pane; }".to_owned()
            )),
            cwd: Some(PathBuf::from("/path/to/cwd")),
        }
    });
}
