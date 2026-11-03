use crate::config::window_state::Shown;
use crate::platform::blur;
use crate::platform::Platform;
use crate::render::graphics::create_context;
use crate::render::renderer::Renderer;
use crate::window::is_wayland;
use crate::window::named;
use crate::window::pick_config;
use crate::window::popup_attributes;
use crate::window::requires_transparent_configs;
use crate::window::set_see_through;
use crate::window::square_without_system_border;
use crate::window::startup_attributes;
use crate::window::transparency_available;
use crate::window::window_icon;
use crate::window::with_activation_token;
use crate::window::with_option_as_alt;
use crate::window::App;
use crate::window::Shell;
use crate::window::Surfaces;
use crate::window::WindowLook;
use anyhow::{anyhow, Context, Result};
use glutin::config::ConfigTemplateBuilder;
use glutin::display::GetGlDisplay;
use glutin::prelude::*;
use glutin::surface::{SurfaceAttributesBuilder, SwapInterval};
use glutin_winit::{DisplayBuilder, GlWindow};
use winit::event_loop::ActiveEventLoop;
use winit::raw_window_handle::HasWindowHandle;
use winit::window::{ImePurpose, WindowAttributes};
use zellij_utils::input::terminal::StartupMode;

impl App {
    pub(in crate::window) fn requested_size(&self) -> winit::dpi::Size {
        let width = (self.metrics.width * self.placement.startup.cols as u32) as f64 / self.scale;
        let height = (self.metrics.height * self.placement.startup.rows as u32) as f64 / self.scale;
        let padding = self.options.padding;
        winit::dpi::LogicalSize::new(
            width + padding.horizontal() as f64,
            height + padding.vertical() as f64,
        )
        .into()
    }

    pub(in crate::window) fn open_window(&mut self, event_loop: &ActiveEventLoop) -> Result<()> {
        let surfaces = self.bring_up(event_loop)?;
        if matches!(self.appearance.look, WindowLook::Popup { .. }) {
            square_without_system_border(&surfaces.window);
        }
        let size = surfaces.window.inner_size();
        let scale = surfaces.window.scale_factor();
        self.surfaces = Some(surfaces);
        self.wayland = is_wayland(event_loop);
        self.placement.shown = Shown::of(self.placement.startup.mode);
        self.placement.windowed_cells = (self.placement.startup.cols, self.placement.startup.rows);
        self.placement.windowed_known = self.placement.startup.mode == StartupMode::Windowed;
        self.follow_display();
        self.schedule_draw();
        self.rescale(scale);
        self.resized(size.width, size.height);
        self.settle_startup_size();
        Ok(())
    }

    pub(in crate::window) fn bring_up(&mut self, event_loop: &ActiveEventLoop) -> Result<Surfaces> {
        let base = WindowAttributes::default()
            .with_title(self.title.clone())
            .with_window_icon(window_icon())
            .with_transparent(true)
            .with_blur(self.options.blur)
            .with_inner_size(self.requested_size());
        let base = match self.appearance.look {
            WindowLook::Terminal => startup_attributes(base, self.placement.startup.mode),
            WindowLook::Popup { position } => popup_attributes(base, position),
        };
        let base = with_activation_token(base, self.activation_token.take());
        let attributes = with_option_as_alt(named(base), self.options.option_as_alt);

        let (window, config) = DisplayBuilder::new()
            .with_window_attributes(Some(attributes))
            .build(
                event_loop,
                ConfigTemplateBuilder::new()
                    .with_alpha_size(8)
                    .with_transparency(requires_transparent_configs(Platform::current())),
                pick_config,
            )
            .map_err(|e| anyhow!("failed to create a window: {}", e))?;
        self.appearance.transparency_available = transparency_available(
            Platform::current(),
            config.supports_transparency(),
            config.alpha_size(),
        );
        self.warn_if_opaque();
        let window = window.ok_or_else(|| anyhow!("the windowing system produced no window"))?;
        blur::set(&window, self.options.blur);
        set_see_through(&window, self.see_through());
        window.set_ime_purpose(ImePurpose::Terminal);
        window.set_ime_allowed(true);

        let handle = window
            .window_handle()
            .context("the window has no raw handle")?
            .as_raw();
        let display = config.display();

        let context = create_context(&display, &config, handle, Platform::current())?;

        let surface_attributes = window
            .build_surface_attributes(SurfaceAttributesBuilder::new())
            .context("failed to describe the window surface")?;
        let surface = unsafe { display.create_window_surface(&config, &surface_attributes) }
            .context("failed to create the window surface")?;

        let context = context
            .make_current(&surface)
            .context("failed to make the window context current")?;
        if let Err(e) = surface.set_swap_interval(&context, SwapInterval::DontWait) {
            report!(
                "the display refused an unsynchronized buffer swap, so a swap may wait for the next refresh: {}",
                e
            );
        }

        let gl = unsafe {
            glow::Context::from_loader_function_cstr(|symbol| display.get_proc_address(symbol))
        };

        let size = window.inner_size();
        Ok(Surfaces {
            _shell: Shell::attach(
                &window,
                &self.placement.remembered,
                self.session
                    .as_ref()
                    .map(|session| session.detacher.clone()),
            ),
            window,
            surface,
            context,
            renderer: Renderer::new(gl)?,
            size,
        })
    }
}
