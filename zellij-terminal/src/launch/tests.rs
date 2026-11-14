use super::*;
use crate::config;
use crate::config::options::Options;
use crate::config::window_state::WindowState;
use crate::launch::desktop_app::Dialer;
use crate::launch::open::hosted_by_the_app_with;
use crate::launch::target::switch_target;
use crate::session;
use crate::session::connection::Opening;
use crate::text::font::FontStack;
use crate::window;
use anyhow::Result;
use std::path::PathBuf;
use zellij_client::ClientInfo;
use zellij_utils::app_control::OpenWindowRequest;
use zellij_utils::cli::AttachArgs;
use zellij_utils::cli::CliArgs;
use zellij_utils::cli::TerminalArgs;
use zellij_utils::data::ConnectToSession;
use zellij_utils::data::LayoutInfo;
use zellij_utils::input::options::Options as SessionOptions;
use zellij_utils::input::terminal::StartupMode;

fn args() -> TerminalArgs {
    TerminalArgs {
        rows: Some(40),
        cols: Some(120),
        cell_width: 10,
        cell_height: 20,
        ..Default::default()
    }
}

fn sized(size: f32) -> config::settings::Settings {
    config::settings::Settings {
        section: zellij_utils::input::terminal::TerminalConfig {
            font_size: Some(size),
            system_fonts: Some(false),
            ..Default::default()
        },
        ..config::settings::Settings::default()
    }
}

fn planned(args: &TerminalArgs) -> Result<Plan> {
    planned_with(
        args,
        &sized(crate::text::font::test_helpers::pixels_to_points(16.0)),
    )
}

fn planned_with(args: &TerminalArgs, settings: &config::settings::Settings) -> Result<Plan> {
    plan(args, &config::options::resolve(settings, None), None)
}

#[test]
fn a_startup_mode_flag_overrides_the_configured_one() {
    use zellij_utils::input::terminal::StartupMode;

    let configured_fullscreen = config::settings::Settings {
        section: zellij_utils::input::terminal::TerminalConfig {
            startup_mode: Some(StartupMode::Fullscreen),
            ..Default::default()
        },
        ..config::settings::Settings::default()
    };
    assert_eq!(
        configured(&configured_fullscreen, &args()).startup_mode,
        StartupMode::Fullscreen
    );
    assert_eq!(
        configured(
            &configured_fullscreen,
            &TerminalArgs {
                startup_mode: Some(StartupMode::Maximized),
                ..args()
            }
        )
        .startup_mode,
        StartupMode::Maximized
    );
    assert_eq!(
        configured(&config::settings::Settings::default(), &args()).startup_mode,
        StartupMode::Remember
    );
    assert_eq!(
        configured(
            &configured_fullscreen,
            &TerminalArgs {
                startup_mode: Some(StartupMode::Remember),
                ..args()
            }
        )
        .startup_mode,
        StartupMode::Remember
    );
    let configured_remember = config::settings::Settings {
        section: zellij_utils::input::terminal::TerminalConfig {
            startup_mode: Some(StartupMode::Remember),
            ..Default::default()
        },
        ..config::settings::Settings::default()
    };
    assert_eq!(
        configured(
            &configured_remember,
            &TerminalArgs {
                startup_mode: Some(StartupMode::Windowed),
                ..args()
            }
        )
        .startup_mode,
        StartupMode::Windowed
    );
}

fn remembering(startup_mode: StartupMode) -> Options {
    let mut options = config::options::resolve(&sized(16.0), None);
    options.startup_mode = startup_mode;
    options
}

fn saved(cols: usize, rows: usize, shown: config::window_state::Shown) -> Option<WindowState> {
    Some(WindowState {
        cols,
        rows,
        state: shown,
    })
}

fn no_size() -> TerminalArgs {
    TerminalArgs {
        rows: None,
        cols: None,
        ..args()
    }
}

#[test]
fn remember_opens_at_the_saved_size() {
    let plan = plan(
        &no_size(),
        &remembering(StartupMode::Remember),
        saved(90, 30, config::window_state::Shown::Windowed),
    )
    .unwrap();
    assert_eq!((plan.geometry.cols, plan.geometry.rows), (90, 30));
    assert_eq!(plan.startup.mode, StartupMode::Windowed);
    assert!(plan.startup.restored);
}

#[test]
fn a_saved_maximized_state_opens_maximized() {
    let plan = plan(
        &no_size(),
        &remembering(StartupMode::Remember),
        saved(90, 30, config::window_state::Shown::Maximized),
    )
    .unwrap();
    assert_eq!(plan.startup.mode, StartupMode::Maximized);
    let attributes = window::startup_attributes(
        winit::window::WindowAttributes::default(),
        plan.startup.mode,
    );
    assert!(attributes.maximized);
    assert!(attributes.fullscreen.is_none());
}

#[test]
fn remember_with_nothing_saved_opens_windowed_at_the_defaults() {
    let plan = plan(&no_size(), &remembering(StartupMode::Remember), None).unwrap();
    assert_eq!((plan.geometry.cols, plan.geometry.rows), (120, 40));
    assert_eq!(plan.startup.mode, StartupMode::Windowed);
    assert!(!plan.startup.restored);
}

#[test]
fn windowed_ignores_the_saved_state() {
    let plan = plan(
        &no_size(),
        &remembering(StartupMode::Windowed),
        saved(90, 30, config::window_state::Shown::Maximized),
    )
    .unwrap();
    assert_eq!((plan.geometry.cols, plan.geometry.rows), (120, 40));
    assert_eq!(plan.startup.mode, StartupMode::Windowed);
}

#[test]
fn explicit_rows_and_cols_win_over_the_remembered_size() {
    let plan = plan(
        &TerminalArgs {
            rows: Some(24),
            cols: Some(80),
            ..args()
        },
        &remembering(StartupMode::Remember),
        saved(90, 30, config::window_state::Shown::Windowed),
    )
    .unwrap();
    assert_eq!((plan.geometry.cols, plan.geometry.rows), (80, 24));
}

#[test]
fn a_headless_run_never_consults_the_remembered_size() {
    assert!(remembered(
        &TerminalArgs {
            headless: true,
            ..args()
        },
        &remembering(StartupMode::Remember)
    )
    .is_none());
    assert!(remembered(&args(), &remembering(StartupMode::Windowed)).is_none());
}

#[test]
fn a_headless_client_advertises_the_cell_metrics_it_was_given() {
    let plan = planned(&TerminalArgs {
        headless: true,
        ..args()
    })
    .unwrap();
    assert!(plan.fonts.is_none());
    assert_eq!(plan.geometry.cell_width, 10);
    assert_eq!(plan.geometry.cell_height, 20);
    assert_eq!(plan.geometry.rows, 40);
    assert_eq!(plan.geometry.cols, 120);
}

#[test]
fn a_windowed_client_measures_its_cell_instead_of_being_told() {
    let plan = planned(&args()).unwrap();
    let metrics = FontStack::embedded(16.0).unwrap().metrics();
    assert!(plan.fonts.is_some());
    assert_eq!(plan.geometry.cell_width, metrics.width as usize);
    assert_eq!(plan.geometry.cell_height, metrics.height as usize);
}

#[test]
fn the_configured_font_size_reaches_the_measured_cell() {
    let small = planned_with(&args(), &sized(12.0)).unwrap();
    let large = planned_with(&args(), &sized(24.0)).unwrap();
    assert!(large.geometry.cell_height > small.geometry.cell_height);
}

#[test]
fn local_media_is_asked_for_only_when_a_window_will_read_it() {
    assert!(planned(&args()).unwrap().capabilities.local_media);
    assert!(
        !planned(&TerminalArgs {
            headless: true,
            ..args()
        })
        .unwrap()
        .capabilities
        .local_media,
        "a headless client cannot read shared memory it never renders"
    );
    assert!(
        !planned(&TerminalArgs {
            record: Some(PathBuf::from("capture.jsonl")),
            ..args()
        })
        .unwrap()
        .capabilities
        .local_media,
        "a recording must stay replayable on another machine"
    );
}

#[test]
fn a_degenerate_geometry_is_refused_before_anything_is_spawned() {
    for args in [
        TerminalArgs {
            rows: Some(0),
            ..args()
        },
        TerminalArgs {
            cols: Some(0),
            ..args()
        },
        TerminalArgs {
            cell_width: 0,
            headless: true,
            ..args()
        },
        TerminalArgs {
            cell_height: 0,
            headless: true,
            ..args()
        },
    ] {
        assert!(planned(&args).is_err());
    }
}

fn windowed() -> TerminalArgs {
    TerminalArgs::default()
}

#[test]
fn a_plain_window_is_handed_to_the_desktop_app() {
    assert!(hosted_by_the_app_with(&windowed(), false));
}

#[test]
fn the_standalone_flag_or_variable_keeps_the_window_on_its_own() {
    assert!(!hosted_by_the_app_with(
        &TerminalArgs {
            standalone: true,
            ..windowed()
        },
        false
    ));
    assert!(!hosted_by_the_app_with(&windowed(), true));
}

#[test]
fn panel_shortcuts_always_go_to_the_desktop_app() {
    for args in [
        TerminalArgs {
            inbox: true,
            ..windowed()
        },
        TerminalArgs {
            sessions: true,
            ..windowed()
        },
    ] {
        assert!(hosted_by_the_app_with(&args, true));
        assert!(panel_view(&args).is_some());
    }
    assert_eq!(
        panel_view(&TerminalArgs {
            inbox: true,
            ..windowed()
        }),
        Some(zellij_utils::app_control::PopupView::Inbox)
    );
    assert_eq!(
        panel_view(&TerminalArgs {
            sessions: true,
            ..windowed()
        }),
        Some(zellij_utils::app_control::PopupView::Sessions)
    );
}

#[test]
fn special_runs_never_join_the_desktop_app() {
    for args in [
        TerminalArgs {
            headless: true,
            ..windowed()
        },
        TerminalArgs {
            watch: true,
            ..windowed()
        },
        TerminalArgs {
            rows: Some(10),
            ..windowed()
        },
        TerminalArgs {
            cols: Some(10),
            ..windowed()
        },
        TerminalArgs {
            duration_secs: Some(1),
            ..windowed()
        },
        TerminalArgs {
            record: Some(PathBuf::from("/tmp/recording")),
            ..windowed()
        },
    ] {
        assert!(!hosted_by_the_app_with(&args, false), "{:?}", args);
    }
}

#[test]
fn the_request_handed_to_the_app_keeps_the_window_flags() {
    use zellij_utils::input::terminal::StartupMode;
    let mut args = TerminalArgs {
        startup_mode: Some(StartupMode::Maximized),
        ..windowed()
    };
    args.attach.session_name = Some("api".to_owned());
    args.attach.create = true;
    args.attach.force_run_commands = true;
    let request = window_request(&args);
    assert_eq!(request.session.as_deref(), Some("api"));
    assert!(request.create);
    assert!(request.force_run_commands);
    assert_eq!(request.startup_mode, Some(StartupMode::Maximized));
    assert_eq!(request.cwd, std::env::current_dir().ok());
    assert_eq!(request.pane_id, None);
    assert_eq!(request.layout, None);
}

#[test]
fn the_panel_layout_clears_every_keybinding_and_runs_the_session_manager_in_its_view() {
    let defaults = zellij_utils::input::config::Config::from_default_assets().unwrap();
    assert!(defaults.keybinds.0.values().any(|keys| !keys.is_empty()));
    let (layout, config) = zellij_utils::input::layout::Layout::from_stringified_layout(
        &panel_layout("sessions"),
        defaults,
    )
    .unwrap();
    assert!(config.keybinds.0.values().all(|keys| keys.is_empty()));
    let described = format!("{:?}", layout);
    assert!(described.contains("session-manager"), "{}", described);
    assert!(
        described.contains("\"role\": \"app_panel\""),
        "{}",
        described
    );
    assert!(
        described.contains("\"view\": \"sessions\""),
        "{}",
        described
    );
}

#[test]
fn the_panel_session_is_locked_quiet_frameless_and_never_saved() {
    let dialer = Dialer {
        opts: CliArgs::default(),
        config_options: SessionOptions {
            default_mode: Some(zellij_utils::data::InputMode::Normal),
            pane_frames: Some(true),
            mouse_mode: Some(true),
            ..Default::default()
        },
        capabilities: capabilities(),
    };
    let options = dialer.panel_options();
    assert_eq!(
        options.default_mode,
        Some(zellij_utils::data::InputMode::Locked)
    );
    assert_eq!(options.show_startup_tips, Some(false));
    assert_eq!(options.show_release_notes, Some(false));
    assert_eq!(options.session_serialization, Some(false));
    assert_eq!(options.pane_frames, Some(false));
    assert_eq!(options.mouse_mode, Some(true));
}

fn themed(styling: zellij_utils::data::Styling) -> config::settings::Settings {
    config::settings::Settings {
        theme: Some(zellij_utils::input::theme::Theme {
            sourced_from_external_file: false,
            palette: styling,
            terminal_colors: None,
        }),
        ..config::settings::Settings::default()
    }
}

#[test]
fn the_panel_border_takes_the_themes_frame_colour() {
    use zellij_utils::data::{PaletteColor, StyleDeclaration, Styling};
    let options = config::options::resolve(&sized(16.0), None);
    assert_eq!(
        panel_border(&config::settings::Settings::default(), &options),
        options.paints.indexed(8)
    );
    let unselected = Styling {
        frame_unselected: Some(StyleDeclaration {
            base: PaletteColor::Rgb((1, 2, 3)),
            ..Default::default()
        }),
        frame_selected: StyleDeclaration {
            base: PaletteColor::Rgb((9, 9, 9)),
            ..Default::default()
        },
        ..Styling::default()
    };
    assert_eq!(panel_border(&themed(unselected), &options), [1, 2, 3]);
    let selected_only = Styling {
        frame_unselected: None,
        frame_selected: StyleDeclaration {
            base: PaletteColor::Rgb((9, 8, 7)),
            ..Default::default()
        },
        ..Styling::default()
    };
    assert_eq!(panel_border(&themed(selected_only), &options), [9, 8, 7]);
    let eight_bit = Styling {
        frame_unselected: Some(StyleDeclaration {
            base: PaletteColor::EightBit(4),
            ..Default::default()
        }),
        ..Styling::default()
    };
    assert_eq!(
        panel_border(&themed(eight_bit), &options),
        options.paints.indexed(4)
    );
}

fn dialer() -> Dialer {
    Dialer {
        opts: CliArgs::default(),
        config_options: SessionOptions::default(),
        capabilities: capabilities(),
    }
}

#[test]
fn a_window_request_without_a_session_starts_a_new_session_in_its_folder() {
    let cwd = PathBuf::from("/tmp/zellij-terminal-chosen-folder");
    let info = dialer()
        .client_info(&OpenWindowRequest {
            session: None,
            create: true,
            cwd: Some(cwd.clone()),
            ..Default::default()
        })
        .unwrap();
    match info {
        ClientInfo::New(name, layout, folder, _initial_panes) => {
            assert!(!name.is_empty());
            assert_eq!(layout, None);
            assert_eq!(folder, Some(cwd));
        },
        other => panic!("a nameless window request resolved to {:?}", other),
    }
}

#[test]
fn a_window_request_creating_an_unknown_session_starts_it_in_its_folder() {
    let name = format!("zellij-terminal-new-in-folder-{}", std::process::id());
    let cwd = PathBuf::from("/tmp/zellij-terminal-chosen-folder");
    let info = dialer()
        .client_info(&OpenWindowRequest {
            session: Some(name.clone()),
            create: true,
            cwd: Some(cwd.clone()),
            ..Default::default()
        })
        .unwrap();
    match info {
        ClientInfo::New(found, _layout, folder, _initial_panes) => {
            assert_eq!(found, name);
            assert_eq!(folder, Some(cwd));
        },
        other => panic!("a new named session resolved to {:?}", other),
    }
}

fn capabilities() -> session::connection::Capabilities {
    session::connection::Capabilities {
        local_media: false,
        paints: config::options::resolve(&sized(16.0), None).paints,
    }
}

fn local(reach: Reach) -> Option<ClientInfo> {
    match reach {
        Reach::Local(info) => Some(info),
        Reach::Remote(_) => None,
    }
}

fn resolved(args: &TerminalArgs) -> Result<Target, Refusal> {
    resolve(args, &SessionOptions::default(), capabilities())
}

#[test]
fn no_session_named_at_all_starts_a_new_session_with_the_default_layout() {
    let target = resolved(&args()).unwrap_or_else(|refusal| panic!("{}", refusal.text));
    match local(target.reach) {
        Some(ClientInfo::New(name, layout, cwd, initial_panes)) => {
            assert!(!name.is_empty());
            assert_eq!(layout, None);
            assert_eq!(cwd, None);
            assert!(initial_panes.is_none());
        },
        other => panic!("an unnamed window opened {:?}", other),
    }
    assert!(matches!(target.opening, Opening::Participant(_)));
}

#[test]
fn a_command_without_a_session_name_runs_in_a_new_session() {
    let target = resolved(&TerminalArgs {
        attach: AttachArgs {
            initial_command: vec!["htop".to_owned()],
            ..AttachArgs::default()
        },
        ..args()
    })
    .unwrap_or_else(|refusal| panic!("{}", refusal.text));
    match local(target.reach) {
        Some(ClientInfo::New(name, layout, _cwd, initial_panes)) => {
            assert!(!name.is_empty());
            assert_eq!(layout, None);
            assert!(initial_panes.is_some());
        },
        other => panic!("a command without a session opened {:?}", other),
    }
}

#[test]
fn a_switch_to_a_nameless_session_gets_a_generated_name_rather_than_an_error() {
    let to = ConnectToSession {
        name: None,
        layout: Some(LayoutInfo::BuiltIn("compact".to_owned())),
        ..ConnectToSession::default()
    };
    let (target, _options) = switch_target(&to, &SessionOptions::default(), capabilities())
        .expect("a nameless switch to resolve to a new session");
    match local(target.reach) {
        Some(ClientInfo::New(name, layout, _cwd, _initial_panes)) => {
            assert!(!name.is_empty());
            assert_eq!(layout, Some(LayoutInfo::BuiltIn("compact".to_owned())));
        },
        other => panic!("a nameless switch resolved to {:?}", other),
    }
}

#[test]
fn the_session_the_window_was_launched_from_can_be_opened_and_switched_to() {
    let name = format!("zellij-terminal-launched-from-{}", std::process::id());
    std::env::set_var(zellij_utils::envs::SESSION_NAME_ENV_KEY, &name);
    let opened = resolved(&TerminalArgs {
        attach: AttachArgs {
            session_name: Some(name.clone()),
            create: true,
            ..AttachArgs::default()
        },
        ..args()
    });
    let switched = switch_target(
        &ConnectToSession {
            name: Some(name.clone()),
            ..ConnectToSession::default()
        },
        &SessionOptions::default(),
        capabilities(),
    );
    std::env::remove_var(zellij_utils::envs::SESSION_NAME_ENV_KEY);

    match opened.map(|target| local(target.reach)) {
        Ok(Some(info)) => assert_eq!(info.get_session_name(), name),
        Ok(None) => panic!("the launching session resolved to a remote one"),
        Err(refusal) => panic!(
            "opening the launching session was refused: {}",
            refusal.text
        ),
    }
    match switched.map(|(target, _)| local(target.reach)) {
        Ok(Some(info)) => assert_eq!(info.get_session_name(), name),
        Ok(None) => panic!("the launching session resolved to a remote one"),
        Err(error) => panic!("switching to the launching session was refused: {}", error),
    }
}

#[test]
fn an_unknown_session_name_is_refused_with_the_clients_own_words() {
    let refusal = resolved(&TerminalArgs {
        attach: AttachArgs {
            session_name: Some("zellij-terminal-no-such-session".to_owned()),
            ..AttachArgs::default()
        },
        ..args()
    })
    .err()
    .expect("an unknown session name to be refused");
    assert_eq!(
        refusal.text,
        "No session with the name 'zellij-terminal-no-such-session' found!"
    );
}
