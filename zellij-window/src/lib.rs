#[cfg(test)]
mod adversarial;
mod atlas;
mod bell;
mod client_loop;
mod clipboard;
mod color;
mod composition;
mod connection;
mod desktop_entry;
#[cfg(test)]
mod differential;
mod discovery;
#[cfg(test)]
mod dump;
#[cfg(test)]
mod equivalence;
mod fixture;
mod font;
#[cfg(test)]
mod goldens;
mod graphics;
#[cfg(test)]
mod headless;
mod host_reply;
pub mod icns;
mod image_io;
mod input;
mod kitty;
mod links;
mod mouse;
mod notice;
mod notify;
mod options;
mod pacing;
mod palette;
mod platform;
#[cfg(test)]
mod projection;
mod recorder;
#[cfg(test)]
mod reference;
#[cfg(feature = "web_server_capability")]
mod remote;
mod renderer;
#[cfg(test)]
mod replay;
mod retained;
mod scene;
mod screen_buffer;
mod settings;
mod sixel;
mod spawn;
mod terminal;
#[cfg(test)]
mod test_server;
#[cfg(test)]
mod vte_terminal;
mod window;

use std::path::PathBuf;
use std::time::Duration;

use anyhow::{anyhow, bail, Result};
use zellij_client::session_resolution::{self, ResolutionFailure};
use zellij_client::{first_message, ClientInfo};
use zellij_utils::cli::{AttachArgs, CliArgs, WindowArgs};
use zellij_utils::data::{ConnectToSession, LayoutInfo};
use zellij_utils::input::actions::initial_panes_from_cli;
use zellij_utils::input::config::Config;
use zellij_utils::input::options::Options as SessionOptions;
use zellij_utils::sessions::generate_unique_session_name;
use zellij_utils::setup::Setup;

use crate::client_loop::{detach_after, detach_on_signal, LoopOptions};
use crate::connection::{Capabilities, Detacher, Geometry, Opening};
use crate::font::FontStack;
use crate::options::Options;

const WELCOME_LAYOUT: &str = "welcome";

pub fn run(args: WindowArgs, opts: CliArgs) -> Result<()> {
    spawn::forget_launching_session();
    open(args, opts)
}

pub fn install_desktop_entry() -> Result<()> {
    desktop_entry::install()
}

pub fn uninstall_desktop_entry() -> Result<()> {
    desktop_entry::uninstall()
}

struct Plan {
    geometry: Geometry,
    capabilities: connection::Capabilities,
    fonts: Option<FontStack>,
}

fn plan(args: &WindowArgs, options: &Options) -> Result<Plan> {
    if args.rows == 0 || args.cols == 0 {
        bail!("rows and cols must both be greater than zero");
    }
    if args.cell_width == 0 || args.cell_height == 0 {
        bail!("cell dimensions must both be greater than zero");
    }

    let windowed = !args.headless;
    let fonts = if windowed {
        Some(options.font.stack(1.0)?)
    } else {
        None
    };
    let (cell_width, cell_height) = match &fonts {
        Some(fonts) => {
            let metrics = fonts.metrics();
            (metrics.width as usize, metrics.height as usize)
        },
        None => (args.cell_width, args.cell_height),
    };

    Ok(Plan {
        geometry: Geometry {
            rows: args.rows,
            cols: args.cols,
            cell_width,
            cell_height,
        },
        capabilities: connection::Capabilities {
            local_media: windowed && args.record.is_none(),
            paints: options.paints,
        },
        fonts,
    })
}

#[cfg(feature = "web_server_capability")]
type LocalConfig = Option<remote::WatchedConfig>;

#[cfg(not(feature = "web_server_capability"))]
type LocalConfig = ();

#[cfg(feature = "web_server_capability")]
fn watched_config(opts: &CliArgs) -> LocalConfig {
    Config::config_file_path(opts).map(|path| remote::WatchedConfig {
        path,
        dir: opts.config_dir.clone(),
    })
}

#[cfg(not(feature = "web_server_capability"))]
fn watched_config(_opts: &CliArgs) -> LocalConfig {}

enum Reach {
    Local(ClientInfo),
    #[cfg(feature = "web_server_capability")]
    Remote(Box<AttachArgs>),
}

struct Target {
    reach: Reach,
    opening: Opening,
}

struct Refusal {
    text: String,
}

impl Refusal {
    fn new(text: impl Into<String>) -> Self {
        Refusal { text: text.into() }
    }
}

impl From<ResolutionFailure> for Refusal {
    fn from(failure: ResolutionFailure) -> Self {
        Refusal {
            text: failure.text(),
        }
    }
}

fn resolve(
    args: &WindowArgs,
    config_options: &SessionOptions,
    capabilities: Capabilities,
) -> Result<Target, Refusal> {
    let attach = &args.attach;
    if attach.remote_url().is_some() {
        return remote_target(args, capabilities);
    }
    if args.watch {
        let info =
            session_resolution::resolve_watch(attach.session_name.clone(), config_options.clone())?;
        return Ok(Target {
            reach: Reach::Local(info),
            opening: Opening::Watcher,
        });
    }
    let info = if attach.session_name.is_none() && attach.index.is_none() {
        if !attach.initial_command.is_empty() {
            return Err(Refusal::new(
                "A command needs a session to run in: name the session to open it in.",
            ));
        }
        let Some(name) = generate_unique_session_name() else {
            return Err(Refusal::new(
                "Failed to generate a unique session name, giving up",
            ));
        };
        ClientInfo::New(
            name,
            Some(LayoutInfo::BuiltIn(WELCOME_LAYOUT.to_owned())),
            None,
            None,
        )
    } else {
        let mut resolved =
            session_resolution::resolve_attach(attach, config_options.clone(), false, None)?;
        let current_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        if let Some(initial_panes) = initial_panes_from_cli(
            attach.initial_command.clone(),
            None,
            Some(current_dir.clone()),
            current_dir,
            attach.close_on_exit,
            attach.start_suspended,
        ) {
            resolved.client.set_initial_panes(initial_panes);
        }
        resolved.client
    };
    Ok(Target {
        reach: Reach::Local(info),
        opening: Opening::Participant(capabilities),
    })
}

#[cfg(feature = "web_server_capability")]
fn remote_target(args: &WindowArgs, capabilities: Capabilities) -> Result<Target, Refusal> {
    if args.watch {
        return Err(Refusal::new(
            "A remote session cannot be watched: watching follows a session's first client, and \
             the web server attaches one of its own. Open the session itself instead.",
        ));
    }
    Ok(Target {
        reach: Reach::Remote(Box::new(args.attach.clone())),
        opening: Opening::Participant(capabilities),
    })
}

#[cfg(not(feature = "web_server_capability"))]
fn remote_target(_args: &WindowArgs, _capabilities: Capabilities) -> Result<Target, Refusal> {
    Err(Refusal::new(
        "This zellij was built without web server support, so it cannot reach a remote session. \
         Rebuild it with the `web_server_capability` feature.",
    ))
}

fn connect(
    target: &Target,
    opts: &CliArgs,
    config_options: &SessionOptions,
    geometry: Geometry,
    focus: (Option<usize>, Option<(u32, bool)>),
    local_config: LocalConfig,
) -> Result<connection::Connection> {
    let info = match &target.reach {
        Reach::Local(info) => info,
        #[cfg(feature = "web_server_capability")]
        Reach::Remote(attach) => return connect_remote(attach, geometry, local_config),
    };
    let _ = local_config;
    let handshake = first_message(
        info,
        opts,
        config_options,
        geometry.size(),
        spawn::advertised_host_terminal_env(),
        focus.0,
        focus.1,
    );
    let session_name = info.get_session_name().to_owned();
    if handshake.starts_server {
        start_server(&session_name)?;
    }
    connection::open(
        &session_name,
        geometry,
        target.opening,
        handshake.message,
        handshake.starts_server,
    )
}

#[cfg(feature = "web_server_capability")]
fn connect_remote(
    attach: &AttachArgs,
    geometry: Geometry,
    local_config: LocalConfig,
) -> Result<connection::Connection> {
    let url = attach
        .remote_url()
        .ok_or_else(|| anyhow!("a remote target without a URL"))?
        .to_owned();
    remote::open(&url, attach, geometry, local_config)
}

fn switch_target(
    to: &ConnectToSession,
    config_options: &SessionOptions,
    capabilities: Capabilities,
) -> Result<(Target, SessionOptions)> {
    let name = match to.name.clone() {
        Some(name) => name,
        None => generate_unique_session_name()
            .ok_or_else(|| anyhow!("Failed to generate a unique session name, giving up"))?,
    };
    let args = AttachArgs {
        session_name: Some(name),
        create: true,
        ..AttachArgs::default()
    };
    let mut resolved =
        session_resolution::resolve_attach(&args, config_options.clone(), false, to.cwd.clone())
            .map_err(|failure| anyhow!("{}", failure.text()))?;
    if let Some(layout) = &to.layout {
        resolved.client.set_layout_info(layout.clone());
    }
    if let Some(cwd) = &to.cwd {
        resolved.client.set_cwd(cwd.clone());
    }
    Ok((
        Target {
            reach: Reach::Local(resolved.client),
            opening: Opening::Participant(capabilities),
        },
        resolved.config_options,
    ))
}

fn switch(
    to: &ConnectToSession,
    opts: &CliArgs,
    config_options: &SessionOptions,
    geometry: Geometry,
    capabilities: Capabilities,
) -> Result<connection::Connection> {
    let (target, resolved_options) = switch_target(to, config_options, capabilities)?;
    connect(
        &target,
        opts,
        &resolved_options,
        geometry,
        (to.tab_position, to.pane_id),
        Default::default(),
    )
}

fn open(args: WindowArgs, opts: CliArgs) -> Result<()> {
    let windowed = !args.headless;
    let config_options = match Setup::from_cli_args(&opts) {
        Ok((_, _, config_options, _, _)) => config_options,
        Err(e) => return refuse(Refusal::new(format!("{}", e)), &args, None),
    };
    let settings = if windowed {
        match settings::load(Config::config_file_path(&opts).as_deref()) {
            Ok(settings) => settings,
            Err(e) => return refuse(Refusal::new(format!("{:#}", e)), &args, None),
        }
    } else {
        settings::Settings::default()
    };
    let options = configured(&settings, &args);

    let Plan {
        geometry,
        capabilities,
        fonts,
    } = plan(&args, &options)?;

    let target = match resolve(&args, &config_options, capabilities) {
        Ok(target) => target,
        Err(refusal) => return refuse(refusal, &args, fonts.map(|fonts| (fonts, options))),
    };
    let local_config = if windowed {
        watched_config(&opts)
    } else {
        Default::default()
    };
    let connection = match connect(
        &target,
        &opts,
        &config_options,
        geometry,
        (None, None),
        local_config,
    ) {
        Ok(connection) => connection,
        Err(e) => {
            return refuse(
                Refusal::new(format!("{:#}", e)),
                &args,
                fonts.map(|fonts| (fonts, options)),
            )
        },
    };
    drive(
        connection,
        args,
        fonts,
        options,
        settings,
        opts,
        config_options,
    )
}

fn configured(settings: &settings::Settings, args: &WindowArgs) -> Options {
    let mut options = options::resolve(settings, None);
    if let Some(mode) = args.startup_mode {
        options.startup_mode = mode;
    }
    options
}

fn refuse(
    refusal: Refusal,
    args: &WindowArgs,
    surface: Option<(FontStack, Options)>,
) -> Result<()> {
    eprintln!("zellij-window: {}", refusal.text);
    if let Some((fonts, options)) = surface {
        if !args.headless {
            notice::show(&refusal.text, fonts, &options)?;
        }
    }
    bail!("{}", refusal.text)
}

fn start_server(session_name: &str) -> Result<PathBuf> {
    let cwd = spawn::resolve_cwd(None)?;
    let program = spawn::resolve_program()?;
    spawn::prepare_socket_dir()?;
    let socket_path = connection::socket_path(session_name)?;
    spawn::spawn(&spawn::server_command(
        program,
        &socket_path,
        session_name,
        &cwd,
    ))?;
    Ok(cwd)
}

fn drive(
    connection: connection::Connection,
    args: WindowArgs,
    fonts: Option<FontStack>,
    options: Options,
    settings: settings::Settings,
    opts: CliArgs,
    config_options: SessionOptions,
) -> Result<()> {
    let windowed = fonts.is_some();
    let detacher = Detacher::new(connection.sender.clone(), connection.role);
    detach_on_signal(detacher.clone());
    if let Some(seconds) = args.duration_secs {
        detach_after(detacher.clone(), Duration::from_secs(seconds));
    }

    let loop_options = LoopOptions {
        record_path: args.record.clone(),
        config_path: windowed.then(|| Config::config_file_path(&opts)).flatten(),
        settings: settings.clone(),
        ..Default::default()
    };
    match fonts {
        Some(fonts) => {
            let capabilities = connection::Capabilities {
                local_media: args.record.is_none(),
                paints: options.paints,
            };
            let window = window::WindowOptions {
                options,
                settings,
                detacher,
            };
            let switching = move |to: &ConnectToSession, geometry: Geometry| {
                switch(to, &opts, &config_options, geometry, capabilities)
            };
            window::run(connection, loop_options, window, fonts, Box::new(switching))?;
        },
        None => {
            let outcome = client_loop::run(connection, loop_options)?;
            if outcome.switch_to.is_some() {
                eprintln!(
                    "zellij-window: the session asked to switch to another one, \
                     which a window that was never opened cannot follow"
                );
            }
        },
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args() -> WindowArgs {
        WindowArgs {
            rows: 40,
            cols: 120,
            cell_width: 10,
            cell_height: 20,
            ..Default::default()
        }
    }

    fn sized(size: f32) -> settings::Settings {
        settings::Settings {
            section: zellij_utils::input::window::WindowConfig {
                font_size: Some(size),
                system_fonts: Some(false),
                ..Default::default()
            },
            ..settings::Settings::default()
        }
    }

    fn planned(args: &WindowArgs) -> Result<Plan> {
        planned_with(args, &sized(16.0))
    }

    fn planned_with(args: &WindowArgs, settings: &settings::Settings) -> Result<Plan> {
        plan(args, &options::resolve(settings, None))
    }

    #[test]
    fn a_startup_mode_flag_overrides_the_configured_one() {
        use zellij_utils::input::window::StartupMode;

        let configured_fullscreen = settings::Settings {
            section: zellij_utils::input::window::WindowConfig {
                startup_mode: Some(StartupMode::Fullscreen),
                ..Default::default()
            },
            ..settings::Settings::default()
        };
        assert_eq!(
            configured(&configured_fullscreen, &args()).startup_mode,
            StartupMode::Fullscreen
        );
        assert_eq!(
            configured(
                &configured_fullscreen,
                &WindowArgs {
                    startup_mode: Some(StartupMode::Maximized),
                    ..args()
                }
            )
            .startup_mode,
            StartupMode::Maximized
        );
        assert_eq!(
            configured(&settings::Settings::default(), &args()).startup_mode,
            StartupMode::Windowed
        );
    }

    #[test]
    fn a_headless_client_advertises_the_cell_metrics_it_was_given() {
        let plan = planned(&WindowArgs {
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
            !planned(&WindowArgs {
                headless: true,
                ..args()
            })
            .unwrap()
            .capabilities
            .local_media,
            "a headless client cannot read shared memory it never renders"
        );
        assert!(
            !planned(&WindowArgs {
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
            WindowArgs { rows: 0, ..args() },
            WindowArgs { cols: 0, ..args() },
            WindowArgs {
                cell_width: 0,
                headless: true,
                ..args()
            },
            WindowArgs {
                cell_height: 0,
                headless: true,
                ..args()
            },
        ] {
            assert!(planned(&args).is_err());
        }
    }

    fn capabilities() -> connection::Capabilities {
        connection::Capabilities {
            local_media: false,
            paints: options::resolve(&sized(16.0), None).paints,
        }
    }

    fn resolved(args: &WindowArgs) -> Result<Target, Refusal> {
        resolve(args, &SessionOptions::default(), capabilities())
    }

    #[test]
    fn no_session_named_at_all_opens_the_welcome_layout_in_a_new_session() {
        let target = resolved(&args()).unwrap_or_else(|refusal| panic!("{}", refusal.text));
        match target.reach {
            Reach::Local(ClientInfo::New(name, layout, cwd, initial_panes)) => {
                assert!(!name.is_empty());
                assert_eq!(layout, Some(LayoutInfo::BuiltIn(WELCOME_LAYOUT.to_owned())));
                assert_eq!(cwd, None);
                assert!(initial_panes.is_none());
            },
            other => panic!(
                "an unnamed window opened {}",
                match other {
                    Reach::Local(info) => format!("{:?}", info),
                    #[cfg(feature = "web_server_capability")]
                    Reach::Remote(_) => "a remote session".to_owned(),
                }
            ),
        }
        assert!(matches!(target.opening, Opening::Participant(_)));
    }

    #[test]
    fn a_command_without_a_session_to_run_it_in_is_refused_before_the_welcome_layout() {
        let refusal = resolved(&WindowArgs {
            attach: AttachArgs {
                initial_command: vec!["htop".to_owned()],
                ..AttachArgs::default()
            },
            ..args()
        })
        .err()
        .expect("a command with no session to be refused");
        assert_eq!(
            refusal.text,
            "A command needs a session to run in: name the session to open it in."
        );
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
        match target.reach {
            Reach::Local(ClientInfo::New(name, layout, _cwd, _initial_panes)) => {
                assert!(!name.is_empty());
                assert_eq!(layout, Some(LayoutInfo::BuiltIn("compact".to_owned())));
            },
            Reach::Local(other) => panic!("a nameless switch resolved to {:?}", other),
            #[cfg(feature = "web_server_capability")]
            Reach::Remote(_) => panic!("a nameless switch resolved to a remote session"),
        }
    }

    #[test]
    fn the_session_the_window_was_launched_from_can_be_opened_and_switched_to() {
        let name = format!("zellij-window-launched-from-{}", std::process::id());
        std::env::set_var(zellij_utils::envs::SESSION_NAME_ENV_KEY, &name);
        let opened = resolved(&WindowArgs {
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

        match opened {
            Ok(Target {
                reach: Reach::Local(info),
                ..
            }) => assert_eq!(info.get_session_name(), name),
            #[cfg(feature = "web_server_capability")]
            Ok(_) => panic!("the launching session resolved to a remote one"),
            Err(refusal) => panic!(
                "opening the launching session was refused: {}",
                refusal.text
            ),
        }
        match switched {
            Ok((
                Target {
                    reach: Reach::Local(info),
                    ..
                },
                _,
            )) => assert_eq!(info.get_session_name(), name),
            #[cfg(feature = "web_server_capability")]
            Ok(_) => panic!("the launching session resolved to a remote one"),
            Err(error) => panic!("switching to the launching session was refused: {}", error),
        }
    }

    #[test]
    fn an_unknown_session_name_is_refused_with_the_clients_own_words() {
        let refusal = resolved(&WindowArgs {
            attach: AttachArgs {
                session_name: Some("zellij-window-no-such-session".to_owned()),
                ..AttachArgs::default()
            },
            ..args()
        })
        .err()
        .expect("an unknown session name to be refused");
        assert_eq!(
            refusal.text,
            "No session with the name 'zellij-window-no-such-session' found!"
        );
    }
}
