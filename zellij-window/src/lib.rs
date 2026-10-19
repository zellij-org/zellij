#[macro_use]
mod diagnostics;
#[cfg(test)]
mod adversarial;
mod app;
#[cfg(windows)]
mod app_registration;
mod atlas;
mod bell;
mod blur;
mod client_loop;
mod clipboard;
mod color;
#[cfg(windows)]
mod com;
mod composition;
mod connection;
mod dbus;
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
mod identity;
mod image_io;
mod input;
mod kitty;
#[cfg(windows)]
mod launcher;
mod links;
mod momentum;
mod mouse;
mod notice;
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
mod scroll_animation;
mod selection;
#[cfg(windows)]
mod session_end;
mod settings;
mod sixel;
mod spawn;
mod sprites;
#[cfg(windows)]
mod taskbar;
mod terminal;
#[cfg(test)]
mod test_server;
#[cfg(windows)]
mod unattended;
#[cfg(test)]
mod vte_terminal;
mod window;
mod window_state;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{anyhow, bail, Result};
use zellij_client::session_resolution::{self, ResolutionFailure};
use zellij_client::{first_message, ClientInfo};
use zellij_utils::cli::{AttachArgs, CliArgs, WindowArgs};
use zellij_utils::data::ConnectToSession;
use zellij_utils::input::actions::initial_panes_from_cli;
use zellij_utils::input::config::Config;
use zellij_utils::input::options::Options as SessionOptions;
use zellij_utils::ipc::ExitReason;
use zellij_utils::sessions::generate_unique_session_name;
use zellij_utils::setup::Setup;

use crate::client_loop::{detach_after, detach_on_signal, LoopOptions};
use crate::clipboard::{Clipboard, ClipboardHandle};
use crate::connection::{Capabilities, Detacher, Geometry, Opening};
use crate::font::FontStack;
use crate::options::Options;
use crate::window_state::{Startup, WindowState};
use zellij_utils::app_control::{AppReply, AppRequest, OpenWindowRequest};
use zellij_utils::input::window::StartupMode;

pub use app::probe::{NotificationProbe, ProbeEvent};
pub use app::{clear_all_sessions, detach_everywhere, stop_all_sessions};

pub fn run(args: WindowArgs, opts: CliArgs) -> Result<()> {
    diagnostics::log_crashes();
    identity::declare();
    #[cfg(windows)]
    let unattended = unattended::prepare();
    spawn::forget_launching_session();
    let result = open(args, opts);
    #[cfg(windows)]
    if let (Some(unattended), Err(error)) = (&unattended, &result) {
        unattended.report(error);
    }
    result
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
    startup: Startup,
}

fn remembered(args: &WindowArgs, options: &Options) -> Option<WindowState> {
    if args.headless || options.startup_mode != StartupMode::Remember {
        return None;
    }
    window_state::load_from(&window_state::path())
}

fn plan(args: &WindowArgs, options: &Options, saved: Option<WindowState>) -> Result<Plan> {
    let startup = Startup::resolve_with(
        options.startup_mode,
        saved,
        args.rows,
        args.cols,
        window_state::InitialCells {
            rows: options.initial_rows,
            cols: options.initial_cols,
        },
    );
    if startup.rows == 0 || startup.cols == 0 {
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
            rows: startup.rows,
            cols: startup.cols,
            cell_width,
            cell_height,
        },
        capabilities: connection::Capabilities {
            local_media: windowed && args.record.is_none(),
            paints: options.paints,
        },
        fonts,
        startup,
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
    let current_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let initial_panes = initial_panes_from_cli(
        attach.initial_command.clone(),
        None,
        Some(current_dir.clone()),
        current_dir,
        attach.close_on_exit,
        attach.start_suspended,
    );
    let info = if attach.session_name.is_none() && attach.index.is_none() {
        let Some(name) = generate_unique_session_name() else {
            return Err(Refusal::new(
                "Failed to generate a unique session name, giving up",
            ));
        };
        ClientInfo::New(name, None, None, initial_panes)
    } else {
        let mut resolved =
            session_resolution::resolve_attach(attach, config_options.clone(), false, None)?;
        if let Some(initial_panes) = initial_panes {
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
    switch_context: Option<zellij_utils::session_history::SwitchContext>,
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
        switch_context,
    );
    let session_name = info.get_session_name().to_owned();
    let socket_path = if handshake.starts_server {
        start_server(&session_name)?
    } else {
        connection::socket_path(&session_name)?
    };
    connection::open_at(
        &socket_path,
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
        to.switch_context.clone(),
    )
}

fn open(args: WindowArgs, opts: CliArgs) -> Result<()> {
    let windowed = !args.headless;
    let config_options = match Setup::from_cli_args(&opts) {
        Ok((_, _, config_options, _, _)) => config_options,
        Err(e) => return refuse(Refusal::new(notice::described(&e)), &args, None),
    };
    let settings = if windowed {
        match settings::load(Config::config_file_path(&opts).as_deref()) {
            Ok(settings) => settings,
            Err(e) => return refuse(Refusal::new(format!("{:#}", e)), &args, None),
        }
    } else {
        settings::Settings::default()
    };
    if hosted_by_the_app(&args) {
        return launch_app(args, opts, config_options, settings);
    }
    open_alone(args, opts, config_options, settings)
}

fn open_alone(
    args: WindowArgs,
    opts: CliArgs,
    config_options: SessionOptions,
    settings: settings::Settings,
) -> Result<()> {
    let windowed = !args.headless;
    let options = configured(&settings, &args);
    let saved = remembered(&args, &options);

    let Plan {
        geometry,
        capabilities,
        fonts,
        startup,
    } = match plan(&args, &options, saved) {
        Ok(plan) => plan,
        Err(e) => return refuse(Refusal::new(format!("{:#}", e)), &args, None),
    };

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
        None,
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
        startup,
    )
}

fn panel_view(args: &WindowArgs) -> Option<zellij_utils::app_control::PopupView> {
    use zellij_utils::app_control::PopupView;
    if args.inbox {
        Some(PopupView::Inbox)
    } else if args.sessions {
        Some(PopupView::Sessions)
    } else {
        None
    }
}

fn hosted_by_the_app(args: &WindowArgs) -> bool {
    hosted_by_the_app_with(args, std::env::var_os("ZELLIJ_WINDOW_STANDALONE").is_some())
}

fn hosted_by_the_app_with(args: &WindowArgs, standalone_env: bool) -> bool {
    if panel_view(args).is_some() {
        return true;
    }
    !args.standalone
        && !standalone_env
        && !args.headless
        && !args.watch
        && args.record.is_none()
        && args.duration_secs.is_none()
        && args.rows.is_none()
        && args.cols.is_none()
        && args.attach.remote_url().is_none()
        && args.attach.index.is_none()
        && args.attach.options.is_none()
}

fn window_request(args: &WindowArgs) -> OpenWindowRequest {
    OpenWindowRequest {
        session: args.attach.session_name.clone(),
        create: args.attach.create,
        pane_id: None,
        command: args.attach.initial_command.clone(),
        cwd: std::env::current_dir().ok(),
        close_on_exit: args.attach.close_on_exit,
        start_suspended: args.attach.start_suspended,
        force_run_commands: args.attach.force_run_commands,
        startup_mode: args.startup_mode,
        layout: None,
    }
}

fn launch_app(
    args: WindowArgs,
    opts: CliArgs,
    config_options: SessionOptions,
    settings: settings::Settings,
) -> Result<()> {
    let token = app::launch_token();
    let (handed, first) = match panel_view(&args) {
        Some(view) => (
            AppRequest::TogglePanel {
                view,
                token: token.clone(),
            },
            app::Intent::TogglePanel { view, token },
        ),
        None => {
            let request = window_request(&args);
            (
                AppRequest::OpenWindow(request.clone()),
                app::Intent::Open { request, token },
            )
        },
    };
    let dir = zellij_utils::consts::app_dir();
    match app::instance::claim(&dir, &handed, app::instance::HANDOFF_WAIT) {
        Ok(app::instance::Claim::Handed(AppReply::Refused(text))) => bail!("{}", text),
        Ok(app::instance::Claim::Handed(_)) => Ok(()),
        Ok(app::instance::Claim::Primary(instance)) => {
            run_app(instance, first, args, opts, config_options, settings)
        },
        Err(e) => {
            report!(
                "the desktop app could not take charge, so this window runs on its own: {:#}",
                e
            );
            open_alone(args, opts, config_options, settings)
        },
    }
}

fn run_app(
    instance: app::instance::Instance,
    first: app::Intent,
    args: WindowArgs,
    opts: CliArgs,
    config_options: SessionOptions,
    settings: settings::Settings,
) -> Result<()> {
    #[cfg(windows)]
    {
        app::platform_windows::set_process_app_id();
        app::platform_windows::register_app_id();
    }
    let options = configured(&settings, &args);
    let capabilities = connection::Capabilities {
        local_media: true,
        paints: options.paints,
    };
    let config_path = Config::config_file_path(&opts);
    let windowing = window::Windowing::bring_up()?;
    let poster = window::AppPoster::new(windowing.proxy());
    let holder = {
        let poster = poster.clone();
        instance.serve(move |request| match request {
            AppRequest::Ping => AppReply::Done,
            AppRequest::Describe => describe(&poster),
            request => {
                if poster.post(app::AppEvent::Request(request)) {
                    AppReply::Done
                } else {
                    AppReply::Refused("The Zellij app is closing.".to_owned())
                }
            },
        })?
    };
    let attention = app::attention_config(config_path.as_deref());
    let notifier = {
        let poster = poster.clone();
        app::notifier::Notifier::spawn(Arc::new(move |event| {
            poster.post(app::AppEvent::Notification(event));
        }))
    };
    let watcher = {
        let poster = poster.clone();
        match app::watcher::Watcher::spawn(
            zellij_utils::consts::ZELLIJ_SOCK_DIR.clone(),
            attention.notification_details(),
            move |snapshot| {
                poster.post(app::AppEvent::Snapshot(Box::new(snapshot)));
            },
        ) {
            Ok(watcher) => Some(watcher),
            Err(e) => {
                report!("the app cannot watch the running sessions: {:#}", e);
                None
            },
        }
    };
    if let Some(path) = config_path.clone() {
        let poster = poster.clone();
        app::watch_config(path, move |config| {
            poster.post(app::AppEvent::Config(Box::new(config)));
        });
    }
    terminate_on_signal(poster.clone());
    let tray_events: app::tray::TrayEvents = {
        let poster = poster.clone();
        Arc::new(move |event| {
            poster.post(app::AppEvent::Tray(event));
        })
    };
    let background = app::Background::new(
        config_path.clone(),
        notifier,
        watcher,
        Some(holder),
        Some(tray_events),
        app::state::CloseNotice::at(Some(app::state::path())),
    );
    let launcher = Launcher {
        dialer: Arc::new(Dialer {
            opts,
            config_options,
            capabilities,
        }),
        config_path,
        font_families: Some(installed_font_families()),
        clipboard: Arc::new(Mutex::new(Clipboard::open())),
    };
    window::run_desktop(
        window::DesktopMode::Background(Box::new(window::Hosting {
            background,
            maker: Box::new(launcher),
            first: Some(first),
        })),
        windowing,
    )
}

fn describe(poster: &window::AppPoster) -> AppReply {
    let (answer, answered) = std::sync::mpsc::channel();
    if !poster.post(app::AppEvent::Describe(answer)) {
        return AppReply::Refused("The Zellij app is closing.".to_owned());
    }
    match answered.recv_timeout(Duration::from_secs(1)) {
        Ok(state) => AppReply::State(state),
        Err(_) => AppReply::Refused("The Zellij app did not answer in time.".to_owned()),
    }
}

fn terminate_on_signal(poster: window::AppPoster) {
    #[cfg(unix)]
    {
        use signal_hook::consts::{SIGINT, SIGTERM};
        use signal_hook::iterator::Signals;
        std::thread::spawn(move || {
            let mut signals = match Signals::new([SIGINT, SIGTERM]) {
                Ok(signals) => signals,
                Err(e) => {
                    report!("failed to install signal handler: {}", e);
                    return;
                },
            };
            if signals.forever().next().is_some() {
                poster.post(app::AppEvent::Terminate);
            }
        });
    }
    #[cfg(not(unix))]
    {
        let _ = poster;
    }
}

struct Dialer {
    opts: CliArgs,
    config_options: SessionOptions,
    capabilities: Capabilities,
}

impl Dialer {
    fn connect(
        &self,
        request: &OpenWindowRequest,
        geometry: Geometry,
    ) -> Result<connection::Connection> {
        let caller_cwd = request
            .cwd
            .clone()
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_else(|| PathBuf::from("."));
        let initial_panes = initial_panes_from_cli(
            request.command.clone(),
            None,
            Some(caller_cwd.clone()),
            caller_cwd,
            request.close_on_exit,
            request.start_suspended,
        );
        let info = match &request.session {
            None => {
                let name = generate_unique_session_name().ok_or_else(|| {
                    anyhow!("Failed to generate a unique session name, giving up")
                })?;
                ClientInfo::New(
                    name,
                    request.layout.clone(),
                    request.cwd.clone(),
                    initial_panes,
                )
            },
            Some(name) => {
                let attach = AttachArgs {
                    session_name: Some(name.clone()),
                    create: request.create,
                    force_run_commands: request.force_run_commands,
                    ..AttachArgs::default()
                };
                let mut resolved = session_resolution::resolve_attach(
                    &attach,
                    self.config_options.clone(),
                    false,
                    None,
                )
                .map_err(|failure| anyhow!("{}", failure.text()))?;
                if let Some(initial_panes) = initial_panes {
                    resolved.client.set_initial_panes(initial_panes);
                }
                if let Some(layout) = request.layout.clone() {
                    resolved.client.set_layout_info(layout);
                }
                if let (Some(cwd), ClientInfo::New(..)) = (request.cwd.clone(), &resolved.client) {
                    resolved.client.set_cwd(cwd);
                }
                resolved.client
            },
        };
        let target = Target {
            reach: Reach::Local(info),
            opening: Opening::Participant(self.capabilities),
        };
        connect(
            &target,
            &self.opts,
            &self.config_options,
            geometry,
            (None, request.pane_id),
            Default::default(),
            None,
        )
    }
}

fn panel_layout(view: &str) -> String {
    format!(
        "keybinds clear-defaults=true {{\n}}\nlayout {{\n    pane {{\n        plugin location=\"zellij:session-manager\" {{\n            role \"app_panel\"\n            view \"{}\"\n        }}\n    }}\n}}\n",
        view
    )
}

impl Dialer {
    fn panel_options(&self) -> SessionOptions {
        let mut options = self.config_options.clone();
        options.default_mode = Some(zellij_utils::data::InputMode::Locked);
        options.show_startup_tips = Some(false);
        options.show_release_notes = Some(false);
        options.session_serialization = Some(false);
        options.pane_frames = Some(false);
        options
    }

    fn connect_panel(&self, geometry: Geometry, view: &str) -> Result<connection::Connection> {
        let name = zellij_utils::consts::APP_PANEL_SESSION.to_owned();
        let options = self.panel_options();
        let running =
            zellij_utils::session_link::session_answers(&name, Duration::from_millis(500));
        let info = if running {
            ClientInfo::Attach(name, options.clone())
        } else {
            ClientInfo::New(
                name,
                Some(zellij_utils::data::LayoutInfo::Stringified(panel_layout(
                    view,
                ))),
                None,
                None,
            )
        };
        let target = Target {
            reach: Reach::Local(info),
            opening: Opening::Participant(self.capabilities),
        };
        connect(
            &target,
            &self.opts,
            &options,
            geometry,
            (None, None),
            Default::default(),
            None,
        )
    }
}

const PANEL_GRIP_TOP: f32 = 10.0;
const PANEL_GRIP_SIDE: f32 = 8.0;

fn panel_border(settings: &settings::Settings, options: &Options) -> color::Srgb {
    let Some(theme) = settings.theme(None) else {
        return options.paints.indexed(8);
    };
    let styling = theme.palette;
    let color = styling
        .frame_unselected
        .map(|frame| frame.base)
        .unwrap_or(styling.frame_selected.base);
    match color {
        zellij_utils::data::PaletteColor::Rgb((r, g, b)) => [r, g, b],
        zellij_utils::data::PaletteColor::EightBit(index) => options.paints.indexed(index),
    }
}

struct Launcher {
    dialer: Arc<Dialer>,
    config_path: Option<PathBuf>,
    font_families: Option<Vec<String>>,
    clipboard: ClipboardHandle,
}

impl Launcher {
    fn current(&self) -> Result<(settings::Settings, Options)> {
        let settings = settings::load(self.config_path.as_deref())?;
        let options = options::resolve(&settings, None);
        Ok((settings, options))
    }
}

impl window::WindowMaker for Launcher {
    fn prepare(&self, request: &OpenWindowRequest) -> Result<window::PreparedWindow> {
        let (settings, mut options) = self.current()?;
        if let Some(mode) = request.startup_mode {
            options.startup_mode = mode;
        }
        let args = WindowArgs {
            cell_width: 10,
            cell_height: 20,
            ..WindowArgs::default()
        };
        let saved = remembered(&args, &options);
        let Plan {
            geometry,
            fonts,
            startup,
            ..
        } = plan(&args, &options, saved)?;
        Ok(window::PreparedWindow {
            fonts: fonts.ok_or_else(|| anyhow!("a window needs fonts"))?,
            startup,
            geometry,
            options,
            settings,
            border: None,
        })
    }

    fn connector(&self) -> window::Connector {
        let dialer = self.dialer.clone();
        Arc::new(move |request, geometry| dialer.connect(request, geometry))
    }

    fn make(
        &self,
        connection: connection::Connection,
        prepared: window::PreparedWindow,
    ) -> window::TerminalWindow {
        let detacher = Detacher::new(connection.sender.clone(), connection.role);
        let loop_options = LoopOptions {
            config_path: self.config_path.clone(),
            settings: prepared.settings.clone(),
            font_families: self.font_families.clone(),
            ..Default::default()
        };
        let dialer = self.dialer.clone();
        let switching: window::Switching = Box::new(move |to: &ConnectToSession, geometry| {
            switch(
                to,
                &dialer.opts,
                &dialer.config_options,
                geometry,
                dialer.capabilities,
            )
        });
        window::TerminalWindow {
            connection,
            loop_options,
            window: window::WindowOptions {
                options: prepared.options,
                settings: prepared.settings,
                detacher,
                startup: prepared.startup,
            },
            fonts: prepared.fonts,
            switching,
            clipboard: self.clipboard.clone(),
            remember: true,
        }
    }

    fn panel(&self, area: (f64, f64)) -> Result<window::PreparedWindow> {
        let (settings, mut options) = self.current()?;
        options.transparency = crate::scene::Transparency::OPAQUE;
        options.blur = false;
        options.padding = options::Padding {
            top: PANEL_GRIP_TOP,
            right: PANEL_GRIP_SIDE,
            bottom: PANEL_GRIP_SIDE,
            left: PANEL_GRIP_SIDE,
        };
        options.padding_balance = false;
        options.padding_color = zellij_utils::input::window::PaddingColor::Background;
        let border = panel_border(&settings, &options);
        let fonts = options.font.stack(1.0)?;
        let metrics = fonts.metrics();
        let (cell_width, cell_height) = (metrics.width.max(1), metrics.height.max(1));
        let cols = ((area.0 * 0.85) / cell_width as f64) as usize;
        let rows = ((area.1 * 0.75) / cell_height as f64) as usize;
        let cols = cols.clamp(60, 150);
        let rows = rows.clamp(16, 44);
        Ok(window::PreparedWindow {
            fonts,
            startup: window_state::Startup::windowed(rows, cols),
            geometry: Geometry {
                rows,
                cols,
                cell_width: cell_width as usize,
                cell_height: cell_height as usize,
            },
            options,
            settings,
            border: Some(border),
        })
    }

    fn panel_connector(&self) -> window::PanelConnector {
        let dialer = self.dialer.clone();
        Arc::new(move |geometry, view| dialer.connect_panel(geometry, view))
    }
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
    report!("{}", refusal.text);
    if !args.headless {
        match surface.map(Ok).unwrap_or_else(fallback_surface) {
            Ok((fonts, options)) => notice::show(&refusal.text, fonts, &options)?,
            Err(e) => report!("the message could not be shown in a window: {:#}", e),
        }
    }
    bail!("{}", refusal.text)
}

fn fallback_surface() -> Result<(FontStack, Options)> {
    let options = options::resolve(&settings::Settings::default(), None);
    let fonts = options.font.stack(1.0)?;
    Ok((fonts, options))
}

// Returns the new session's socket path, which is named by a freshly registered session id.
fn start_server(session_name: &str) -> Result<PathBuf> {
    let cwd = spawn::resolve_cwd(None)?;
    let program = spawn::resolve_program()?;
    spawn::prepare_socket_dir()?;
    let socket_path = connection::new_session_socket_path(session_name)?;
    if let Some(parent) = socket_path.parent() {
        zellij_utils::consts::prepare_private_dir(parent)
            .map_err(|e| anyhow!("failed to create {:?}: {}", parent, e))?;
    }
    spawn::spawn(&spawn::server_command(
        program,
        &socket_path,
        session_name,
        &cwd,
    ))?;
    Ok(socket_path)
}

fn installed_font_families() -> Vec<String> {
    discovery::Discovery::open()
        .map(|discovery| discovery.monospace_families())
        .unwrap_or_default()
}

fn drive(
    connection: connection::Connection,
    args: WindowArgs,
    fonts: Option<FontStack>,
    options: Options,
    settings: settings::Settings,
    opts: CliArgs,
    config_options: SessionOptions,
    startup: Startup,
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
        font_families: windowed.then(installed_font_families),
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
                startup,
            };
            let switching = move |to: &ConnectToSession, geometry: Geometry| {
                switch(to, &opts, &config_options, geometry, capabilities)
            };
            window::run(connection, loop_options, window, fonts, Box::new(switching))?;
        },
        None => {
            let outcome = client_loop::run(connection, loop_options)?;
            if outcome.switch_to.is_some() {
                report!(
                    "the session asked to switch to another one, \
                     which a window that was never opened cannot follow"
                );
            }
            if let Some(ExitReason::CustomExitStatus(status)) = outcome.exit_reason {
                if status != 0 {
                    bail!("{}", notice::exit_status(status));
                }
            }
        },
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use zellij_utils::data::LayoutInfo;

    fn args() -> WindowArgs {
        WindowArgs {
            rows: Some(40),
            cols: Some(120),
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
        planned_with(args, &sized(crate::font::pixels_to_points(16.0)))
    }

    fn planned_with(args: &WindowArgs, settings: &settings::Settings) -> Result<Plan> {
        plan(args, &options::resolve(settings, None), None)
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
            StartupMode::Remember
        );
        assert_eq!(
            configured(
                &configured_fullscreen,
                &WindowArgs {
                    startup_mode: Some(StartupMode::Remember),
                    ..args()
                }
            )
            .startup_mode,
            StartupMode::Remember
        );
        let configured_remember = settings::Settings {
            section: zellij_utils::input::window::WindowConfig {
                startup_mode: Some(StartupMode::Remember),
                ..Default::default()
            },
            ..settings::Settings::default()
        };
        assert_eq!(
            configured(
                &configured_remember,
                &WindowArgs {
                    startup_mode: Some(StartupMode::Windowed),
                    ..args()
                }
            )
            .startup_mode,
            StartupMode::Windowed
        );
    }

    fn remembering(startup_mode: StartupMode) -> Options {
        let mut options = options::resolve(&sized(16.0), None);
        options.startup_mode = startup_mode;
        options
    }

    fn saved(cols: usize, rows: usize, shown: window_state::Shown) -> Option<WindowState> {
        Some(WindowState {
            cols,
            rows,
            state: shown,
        })
    }

    fn no_size() -> WindowArgs {
        WindowArgs {
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
            saved(90, 30, window_state::Shown::Windowed),
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
            saved(90, 30, window_state::Shown::Maximized),
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
            saved(90, 30, window_state::Shown::Maximized),
        )
        .unwrap();
        assert_eq!((plan.geometry.cols, plan.geometry.rows), (120, 40));
        assert_eq!(plan.startup.mode, StartupMode::Windowed);
    }

    #[test]
    fn explicit_rows_and_cols_win_over_the_remembered_size() {
        let plan = plan(
            &WindowArgs {
                rows: Some(24),
                cols: Some(80),
                ..args()
            },
            &remembering(StartupMode::Remember),
            saved(90, 30, window_state::Shown::Windowed),
        )
        .unwrap();
        assert_eq!((plan.geometry.cols, plan.geometry.rows), (80, 24));
    }

    #[test]
    fn a_headless_run_never_consults_the_remembered_size() {
        assert!(remembered(
            &WindowArgs {
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
            WindowArgs {
                rows: Some(0),
                ..args()
            },
            WindowArgs {
                cols: Some(0),
                ..args()
            },
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

    fn windowed() -> WindowArgs {
        WindowArgs::default()
    }

    #[test]
    fn a_plain_window_is_handed_to_the_desktop_app() {
        assert!(hosted_by_the_app_with(&windowed(), false));
    }

    #[test]
    fn the_standalone_flag_or_variable_keeps_the_window_on_its_own() {
        assert!(!hosted_by_the_app_with(
            &WindowArgs {
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
            WindowArgs {
                inbox: true,
                ..windowed()
            },
            WindowArgs {
                sessions: true,
                ..windowed()
            },
        ] {
            assert!(hosted_by_the_app_with(&args, true));
            assert!(panel_view(&args).is_some());
        }
        assert_eq!(
            panel_view(&WindowArgs {
                inbox: true,
                ..windowed()
            }),
            Some(zellij_utils::app_control::PopupView::Inbox)
        );
        assert_eq!(
            panel_view(&WindowArgs {
                sessions: true,
                ..windowed()
            }),
            Some(zellij_utils::app_control::PopupView::Sessions)
        );
    }

    #[test]
    fn special_runs_never_join_the_desktop_app() {
        for args in [
            WindowArgs {
                headless: true,
                ..windowed()
            },
            WindowArgs {
                watch: true,
                ..windowed()
            },
            WindowArgs {
                rows: Some(10),
                ..windowed()
            },
            WindowArgs {
                cols: Some(10),
                ..windowed()
            },
            WindowArgs {
                duration_secs: Some(1),
                ..windowed()
            },
            WindowArgs {
                record: Some(PathBuf::from("/tmp/recording")),
                ..windowed()
            },
        ] {
            assert!(!hosted_by_the_app_with(&args, false), "{:?}", args);
        }
    }

    #[test]
    fn the_request_handed_to_the_app_keeps_the_window_flags() {
        use zellij_utils::input::window::StartupMode;
        let mut args = WindowArgs {
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

    fn themed(styling: zellij_utils::data::Styling) -> settings::Settings {
        settings::Settings {
            theme: Some(zellij_utils::input::theme::Theme {
                sourced_from_external_file: false,
                palette: styling,
                terminal_colors: None,
            }),
            ..settings::Settings::default()
        }
    }

    #[test]
    fn the_panel_border_takes_the_themes_frame_colour() {
        use zellij_utils::data::{PaletteColor, StyleDeclaration, Styling};
        let options = options::resolve(&sized(16.0), None);
        assert_eq!(
            panel_border(&settings::Settings::default(), &options),
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
    fn no_session_named_at_all_starts_a_new_session_with_the_default_layout() {
        let target = resolved(&args()).unwrap_or_else(|refusal| panic!("{}", refusal.text));
        match target.reach {
            Reach::Local(ClientInfo::New(name, layout, cwd, initial_panes)) => {
                assert!(!name.is_empty());
                assert_eq!(layout, None);
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
    fn a_command_without_a_session_name_runs_in_a_new_session() {
        let target = resolved(&WindowArgs {
            attach: AttachArgs {
                initial_command: vec!["htop".to_owned()],
                ..AttachArgs::default()
            },
            ..args()
        })
        .unwrap_or_else(|refusal| panic!("{}", refusal.text));
        match target.reach {
            Reach::Local(ClientInfo::New(name, layout, _cwd, initial_panes)) => {
                assert!(!name.is_empty());
                assert_eq!(layout, None);
                assert!(initial_panes.is_some());
            },
            Reach::Local(other) => panic!("a command without a session opened {:?}", other),
            #[cfg(feature = "web_server_capability")]
            Reach::Remote(_) => panic!("a command without a session opened a remote session"),
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
