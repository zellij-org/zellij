use crate::config::options::Options;
use crate::input::clipboard::{Clipboard, ClipboardHandle};
use crate::launch::configured;
use crate::launch::connect;
use crate::launch::installed_font_families;
use crate::launch::load;
use crate::launch::open_alone;
use crate::launch::panel_border;
use crate::launch::panel_layout;
use crate::launch::panel_look;
use crate::launch::panel_view;
use crate::launch::plan;
use crate::launch::refuse;
use crate::launch::remembered;
use crate::launch::switch;
use crate::launch::window_request;
use crate::launch::Loaded;
use crate::launch::Plan;
use crate::launch::Reach;
use crate::launch::Refusal;
use crate::launch::Target;
use crate::platform::os;
use crate::session::client_loop::LoopOptions;
use crate::session::connection::{Capabilities, Detacher, Geometry, Opening};
use crate::{app, config, session, window};
use anyhow::{anyhow, bail, Result};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use zellij_client::session_resolution::{self};
use zellij_client::ClientInfo;
use zellij_utils::app_control::{AppReply, AppRequest, OpenWindowRequest};
use zellij_utils::cli::{AttachArgs, CliArgs, TerminalArgs};
use zellij_utils::data::ConnectToSession;
use zellij_utils::input::actions::initial_panes_from_cli;
use zellij_utils::input::config::Config;
use zellij_utils::input::options::Options as SessionOptions;
use zellij_utils::sessions::generate_unique_session_name;

pub(crate) fn launch_app(args: TerminalArgs, opts: CliArgs, loaded: Option<Loaded>) -> Result<()> {
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
    match app::control::instance::claim(&dir, &handed, app::control::instance::HANDOFF_WAIT) {
        Ok(app::control::instance::Claim::Handed(AppReply::Refused(text))) => bail!("{}", text),
        Ok(app::control::instance::Claim::Handed(_)) => Ok(()),
        Ok(app::control::instance::Claim::Primary(instance)) => {
            let (config_options, settings) = match loaded.map_or_else(|| load(&args, &opts), Ok) {
                Ok(loaded) => loaded,
                Err(done) => return done,
            };
            run_app(instance, Some(first), args, opts, config_options, settings)
        },
        Err(e) if panel_view(&args).is_some() => refuse(
            Refusal::new(format!(
                "The panel could not be opened because the desktop app did not answer: {:#}",
                e
            )),
            &args,
            None,
        ),
        Err(e) => {
            report!(
                "the desktop app could not take charge, so this window runs on its own: {:#}",
                e
            );
            let (config_options, settings) = match loaded.map_or_else(|| load(&args, &opts), Ok) {
                Ok(loaded) => loaded,
                Err(done) => return done,
            };
            open_alone(args, opts, config_options, settings)
        },
    }
}

pub(crate) fn run_app(
    instance: app::control::instance::Instance,
    first: Option<app::Intent>,
    args: TerminalArgs,
    opts: CliArgs,
    config_options: SessionOptions,
    settings: config::settings::Settings,
) -> Result<()> {
    os::claim_app_identity();
    let options = configured(&settings, &args);
    let capabilities = session::connection::Capabilities {
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
            AppRequest::Describe => app::control::standalone::with_standalone(describe(&poster)),
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
    app::desktop::services::check_soon(&attention);
    let notifier = {
        let poster = poster.clone();
        app::desktop::notifier::Notifier::spawn(Arc::new(move |event| {
            poster.post(app::AppEvent::Notification(event));
        }))
    };
    let watcher = {
        let poster = poster.clone();
        match app::control::watcher::Watcher::spawn(
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
    os::terminate_on_signal(poster.clone());
    let tray_events: app::desktop::tray::TrayEvents = {
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
        app::policy::state::CloseNotice::at(Some(app::policy::state::path())),
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
            first,
        })),
        windowing,
    )
}

pub(crate) fn describe(poster: &window::AppPoster) -> AppReply {
    let (answer, answered) = std::sync::mpsc::channel();
    if !poster.post(app::AppEvent::Describe(answer)) {
        return AppReply::Refused("The Zellij app is closing.".to_owned());
    }
    match answered.recv_timeout(Duration::from_secs(1)) {
        Ok(state) => AppReply::State(state),
        Err(_) => AppReply::Refused("The Zellij app did not answer in time.".to_owned()),
    }
}

pub(crate) struct Dialer {
    pub(crate) opts: CliArgs,
    pub(crate) config_options: SessionOptions,
    pub(crate) capabilities: Capabilities,
}

impl Dialer {
    pub(crate) fn connect(
        &self,
        request: &OpenWindowRequest,
        geometry: Geometry,
    ) -> Result<session::connection::Connection> {
        let target = Target {
            reach: Reach::Local(self.client_info(request)?),
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

    pub(crate) fn client_info(&self, request: &OpenWindowRequest) -> Result<ClientInfo> {
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
        Ok(info)
    }
}

impl Dialer {
    pub(crate) fn panel_options(&self) -> SessionOptions {
        let mut options = self.config_options.clone();
        options.default_mode = Some(zellij_utils::data::InputMode::Locked);
        options.show_startup_tips = Some(false);
        options.show_release_notes = Some(false);
        options.session_serialization = Some(false);
        options.pane_frames = Some(false);
        options
    }

    pub(crate) fn connect_panel(
        &self,
        geometry: Geometry,
        view: &str,
    ) -> Result<session::connection::Connection> {
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

pub(crate) struct Launcher {
    pub(crate) dialer: Arc<Dialer>,
    pub(crate) config_path: Option<PathBuf>,
    pub(crate) font_families: Option<Vec<String>>,
    pub(crate) clipboard: ClipboardHandle,
}

impl Launcher {
    pub(crate) fn current(&self) -> Result<(config::settings::Settings, Options)> {
        let settings = config::settings::load(self.config_path.as_deref())?;
        let options = config::options::resolve(&settings, None);
        Ok((settings, options))
    }
}

impl window::WindowMaker for Launcher {
    fn prepare(&self, request: &OpenWindowRequest) -> Result<window::PreparedWindow> {
        let (settings, mut options) = self.current()?;
        if let Some(mode) = request.startup_mode {
            options.startup_mode = mode;
        }
        let args = TerminalArgs {
            cell_width: 10,
            cell_height: 20,
            ..TerminalArgs::default()
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
        connection: session::connection::Connection,
        prepared: window::PreparedWindow,
    ) -> window::TerminalWindow {
        let detacher = Detacher::new(connection.sender.clone(), connection.role);
        let loop_options = LoopOptions {
            config_path: self.config_path.clone(),
            settings: prepared.settings.clone(),
            font_families: self.font_families.clone(),
            allow_remote_clipboard_read: prepared.options.allow_remote_clipboard_read,
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
        panel_look(&mut options);
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
            startup: config::window_state::Startup::windowed(rows, cols),
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
