use crate::config::options::Options;
use crate::config::settings::Settings;
use crate::config::window_state::{self, Shown, Startup};
use crate::emulation::terminal::TerminalState;
use crate::input::clipboard::{Clipboard, ClipboardHandle};
use crate::input::composition::Composition;
use crate::input::keyboard;
use crate::input::momentum::Momentum;
use crate::input::mouse::PointerState;
use crate::platform::bell;
use crate::render::atlas::GlyphCache;
use crate::render::pacing::Pacer;
use crate::render::retained::RetainedScene;
use crate::render::scene::{BlinkPhase, Transparency};
use crate::render::scroll_animation::{ScrollAnimations, ScrollLayer};
use crate::session::client_loop::{self, Ending, LoopOptions, LoopOutcome};
use crate::session::connection::{Connection, Geometry, GeometryHandle, Role, SharedSender};
use crate::text::font::{CellMetrics, FontStack};
use crate::window::App;
use crate::window::AppPoster;
use crate::window::Appearance;
use crate::window::Composing;
use crate::window::Desktop;
use crate::window::DesktopMode;
use crate::window::Mouse;
use crate::window::Placement;
use crate::window::ProxySink;
use crate::window::Session;
use crate::window::Switching;
use crate::window::Timing;
use crate::window::Wake;
use crate::window::WindowLook;
use crate::window::WindowMemory;
use crate::window::WindowOptions;
use crate::window::WindowWake;
use anyhow::{anyhow, Context, Result};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use winit::event_loop::{EventLoop, EventLoopProxy};
use winit::keyboard::ModifiersState;
use zellij_utils::data::ConnectToSession;
use zellij_utils::input::terminal::StartupMode;

pub(crate) struct TerminalWindow {
    pub connection: Connection,
    pub loop_options: LoopOptions,
    pub window: WindowOptions,
    pub fonts: FontStack,
    pub switching: Switching,
    pub clipboard: ClipboardHandle,
    pub remember: bool,
}

pub(crate) fn terminal_app(made: TerminalWindow, key: u64, proxy: EventLoopProxy<Wake>) -> App {
    let TerminalWindow {
        connection,
        mut loop_options,
        window,
        fonts,
        switching,
        clipboard,
        remember,
    } = made;
    let renderer = Rendering::bring_up(&window.options, fonts, window.startup);
    let geometry = connection.geometry.clone();
    let role = connection.role;
    let sender = connection.sender.clone();
    let title = window_title(&connection.session_name);
    let session_name = connection.session_name.clone();
    let remote = connection.remote;
    let state_path = (remember && loop_options.record_path.is_none()).then(window_state::path);
    loop_options.clipboard = Some(clipboard.clone());

    let mut session = Session {
        client: Client::placeholder(),
        detacher: window.detacher,
        loop_options,
        proxy,
        key,
        switching,
        outcomes: Vec::new(),
    };
    session.spawn(connection);

    let mut app = renderer.into_app(
        Some(sender),
        role,
        geometry,
        title,
        window.settings,
        clipboard,
        Some(session),
    );
    app.key = key;
    app.placement.state_path = state_path;
    app.session_name = session_name;
    app.set_remote_session(remote);
    app
}

pub(crate) fn as_panel(
    mut app: App,
    position: Option<winit::dpi::Position>,
    border: Option<crate::emulation::color::Srgb>,
) -> App {
    app.appearance.look = WindowLook::Popup { position };
    app.appearance.border = border;
    app.options.transparency = Transparency::OPAQUE;
    app.options.blur = false;
    app
}

pub(crate) fn with_activation(mut app: App, token: Option<String>) -> App {
    app.activation_token = token;
    app
}

pub fn run(
    connection: Connection,
    loop_options: LoopOptions,
    window: WindowOptions,
    fonts: FontStack,
    switching: Switching,
    listen: bool,
) -> Result<LoopOutcome> {
    let windowing = Windowing::bring_up()?;
    let _listening = if listen {
        crate::app::control::standalone::listen(AppPoster::new(windowing.proxy()))
    } else {
        None
    };
    let clipboard: ClipboardHandle = Arc::new(Mutex::new(Clipboard::open()));
    let app = terminal_app(
        TerminalWindow {
            connection,
            loop_options,
            window,
            fonts,
            switching,
            clipboard,
            remember: true,
        },
        0,
        windowing.proxy(),
    );
    let mut desktop = Desktop::new(windowing.proxy(), DesktopMode::Single);
    desktop.adopt(app);
    let outcome = windowing.drive(&mut desktop);
    let mut sessions = desktop.into_sessions();
    for session in sessions.iter_mut() {
        session.collect();
    }
    outcome?;
    sessions
        .into_iter()
        .next()
        .map(Session::outcome)
        .unwrap_or_else(|| Err(anyhow!("the window never opened a session")))
}

pub(crate) fn run_desktop(desktop_mode: DesktopMode, windowing: Windowing) -> Result<()> {
    let mut desktop = Desktop::new(windowing.proxy(), desktop_mode);
    let outcome = windowing.drive(&mut desktop);
    drop(desktop);
    outcome
}

pub fn run_notice(
    message: &str,
    geometry: Geometry,
    fonts: FontStack,
    options: &Options,
) -> Result<()> {
    let metrics = fonts.metrics();
    let mut state = TerminalState::new(geometry.rows, geometry.cols);
    state.set_cell_size(metrics.width, metrics.height);
    let windowing = Windowing::bring_up()?;
    let startup = Startup::resolve(
        options.startup_mode,
        None,
        Some(geometry.rows),
        Some(geometry.cols),
    );
    let mut app = Rendering::from_state(state, options, fonts, startup).into_app(
        None,
        Role::Watcher,
        GeometryHandle::new(geometry),
        "zellij".to_owned(),
        Settings::default(),
        Arc::new(Mutex::new(Clipboard::open())),
        None,
    );
    app.show_notice(message);
    let mut desktop = Desktop::new(windowing.proxy(), DesktopMode::Single);
    desktop.adopt(app);
    windowing.drive(&mut desktop)
}

impl Session {
    pub(super) fn spawn(&mut self, connection: Connection) {
        let sink = ProxySink {
            proxy: self.proxy.clone(),
            key: self.key,
        };
        self.client = Client::spawn(connection, self.loop_options.clone(), sink);
    }

    pub(super) fn reconnect(
        &mut self,
        to: &ConnectToSession,
        geometry: Geometry,
    ) -> Result<Connection> {
        (self.switching)(to, geometry)
    }

    pub(crate) fn collect(&mut self) {
        match self.client.take().map(Client::join) {
            Some(Ok(outcome)) => self.outcomes.push(outcome),
            Some(Err(e)) => report!("{}", e),
            None => {},
        }
    }

    pub(super) fn outcome(self) -> Result<LoopOutcome> {
        let mut outcomes = self.outcomes.into_iter();
        let Some(first) = outcomes.next() else {
            return Err(anyhow!("the window never opened a session"));
        };
        Ok(outcomes.fold(first, |total, next| LoopOutcome {
            render_count: total.render_count + next.render_count,
            discarded_ansi: total.discarded_ansi + next.discarded_ansi,
            recorded_messages: total.recorded_messages + next.recorded_messages,
            exit_reason: next.exit_reason,
            switch_to: next.switch_to,
        }))
    }
}

pub(super) fn window_title(session_name: &str) -> String {
    format!("zellij: {}", session_name)
}

pub(super) struct Rendering {
    pub(super) state: TerminalState,
    pub(super) cache: GlyphCache,
    pub(super) metrics: CellMetrics,
    pub(super) options: Options,
    pub(super) startup: Startup,
}

impl Rendering {
    pub(super) fn bring_up(options: &Options, fonts: FontStack, startup: Startup) -> Self {
        let cache = GlyphCache::new(fonts);
        let metrics = cache.metrics();
        let mut state = TerminalState::new(startup.rows, startup.cols);
        state.set_cell_size(metrics.width, metrics.height);
        Self {
            state,
            cache,
            metrics,
            options: options.clone(),
            startup,
        }
    }

    pub(super) fn from_state(
        state: TerminalState,
        options: &Options,
        fonts: FontStack,
        startup: Startup,
    ) -> Self {
        let cache = GlyphCache::new(fonts);
        let metrics = cache.metrics();
        Self {
            state,
            cache,
            metrics,
            options: options.clone(),
            startup,
        }
    }

    pub(super) fn into_app(
        self,
        sender: Option<SharedSender>,
        role: Role,
        geometry: GeometryHandle,
        title: String,
        settings: Settings,
        clipboard: ClipboardHandle,
        session: Option<Session>,
    ) -> App {
        App {
            state: self.state,
            cache: self.cache,
            metrics: self.metrics,
            options: self.options,
            settings,
            theme_mode: None,
            scale: 1.0,
            zoom: 1.0,
            sender,
            role,
            geometry,
            modifiers: ModifiersState::empty(),
            alt_keys: keyboard::AltKeys::default(),
            origin: (0, 0),
            clipboard,
            title,
            session_name: String::new(),
            remote_session: false,
            notice: None,
            retained: RetainedScene::new(),
            surfaces: None,
            failure: None,
            leaving: false,
            session,
            ring: bell::ring,
            key: 0,
            activation_token: None,
            focused: true,
            scroll: ScrollAnimations::default(),
            layer: ScrollLayer::default(),
            wayland: false,
            momentum: Momentum::default(),
            standalone: true,
            ime: Composing {
                composition: Composition::new(),
                composed_row: None,
                ime_area: None,
            },
            mouse: Mouse {
                pointer: PointerState::new(),
                hovered_link: None,
                armed_link: None,
                selection: None,
                selecting: false,
                pointer_hidden: false,
            },
            timing: Timing {
                blink: BlinkPhase::On,
                blink_since: Instant::now(),
                pacer: Pacer::new(None),
                display_checked: Instant::now(),
            },
            placement: Placement {
                startup: self.startup,
                state_path: None,
                remembered: WindowMemory::default(),
                shown: Shown::of(self.startup.mode),
                windowed_cells: (self.startup.cols, self.startup.rows),
                windowed_known: self.startup.mode == StartupMode::Windowed,
                before_fullscreen: Shown::Windowed,
            },
            appearance: Appearance {
                look: WindowLook::Terminal,
                border: None,
                transparency_available: true,
                warned_opaque: false,
            },
        }
    }
}

pub(crate) struct Windowing {
    pub(super) event_loop: EventLoop<Wake>,
}

impl Windowing {
    pub(crate) fn bring_up() -> Result<Self> {
        crate::platform::static_build::ensure_windows()?;
        Ok(Self {
            event_loop: EventLoop::<Wake>::with_user_event()
                .build()
                .context("failed to create a windowing event loop")?,
        })
    }

    pub(crate) fn proxy(&self) -> EventLoopProxy<Wake> {
        self.event_loop.create_proxy()
    }

    pub(super) fn drive(self, desktop: &mut Desktop) -> Result<()> {
        let outcome = self
            .event_loop
            .run_app(desktop)
            .context("the windowing event loop failed");
        match desktop.take_failure() {
            Some(failure) => Err(failure),
            None => outcome,
        }
    }
}

pub(super) struct Client(Option<std::thread::JoinHandle<Result<LoopOutcome>>>);

impl Client {
    pub(super) fn placeholder() -> Self {
        Client(None)
    }

    pub(super) fn spawn(connection: Connection, options: LoopOptions, mut sink: ProxySink) -> Self {
        let proxy = sink.proxy.clone();
        let key = sink.key;
        Client(Some(std::thread::spawn(move || {
            guarded(
                || client_loop::run_with_sink(connection, options, &mut sink),
                |message| {
                    let _ = proxy.send_event(Wake::Window(
                        key,
                        WindowWake::Finished(Ending::Failed(message)),
                    ));
                },
            )
        })))
    }

    pub(super) fn take(&mut self) -> Option<Client> {
        self.0.take().map(|thread| Client(Some(thread)))
    }

    pub(super) fn join(self) -> Result<LoopOutcome> {
        match self.0 {
            Some(thread) => thread
                .join()
                .map_err(|_| anyhow!("the client thread panicked"))?,
            None => Err(anyhow!("there was no client thread to wait for")),
        }
    }
}

pub(super) fn guarded<T>(
    work: impl FnOnce() -> Result<T>,
    on_crash: impl FnOnce(String),
) -> Result<T> {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(work)) {
        Ok(outcome) => outcome,
        Err(payload) => {
            let message = format!(
                "The connection to the session crashed: {}",
                crate::diagnostics::panic_text(payload.as_ref())
            );
            on_crash(message.clone());
            Err(anyhow!(message))
        },
    }
}
