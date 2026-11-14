use crate::config::options::Options;
use crate::config::settings::Settings;
use crate::config::window_state::Startup;
use crate::platform::Platform;
use crate::session::connection::Detacher;
use crate::window::skip_taskbar;
use crate::window::ICON_PNG;
use glutin::config::Config;
use glutin::prelude::*;
use winit::window::{Icon, Window, WindowAttributes, WindowLevel};
use zellij_utils::input::terminal::StartupMode;

pub struct WindowOptions {
    pub options: Options,
    pub settings: Settings,
    pub detacher: Detacher,
    pub startup: Startup,
}

pub(super) fn popup_attributes(
    attributes: WindowAttributes,
    position: Option<winit::dpi::Position>,
) -> WindowAttributes {
    let attributes = attributes
        .with_decorations(false)
        .with_resizable(true)
        .with_window_level(WindowLevel::AlwaysOnTop)
        .with_title("Zellij");
    let attributes = skip_taskbar(attributes);
    match position {
        Some(position) => attributes.with_position(position),
        None => attributes,
    }
}

pub(crate) fn startup_attributes(
    attributes: WindowAttributes,
    mode: StartupMode,
) -> WindowAttributes {
    match mode {
        StartupMode::Windowed | StartupMode::Remember => attributes,
        StartupMode::Maximized => attributes.with_maximized(true),
        StartupMode::Fullscreen => {
            attributes.with_fullscreen(Some(winit::window::Fullscreen::Borderless(None)))
        },
    }
}

pub(super) fn refresh_millihertz(window: &Window) -> Option<u32> {
    window
        .current_monitor()
        .or_else(|| window.primary_monitor())
        .and_then(|monitor| monitor.refresh_rate_millihertz())
}

pub(super) fn window_icon() -> Option<Icon> {
    let image = match crate::render::image_io::decode(ICON_PNG) {
        Ok(image) => image,
        Err(e) => {
            report!("the embedded window icon is unreadable: {e}");
            return None;
        },
    };
    match Icon::from_rgba(image.pixels, image.width, image.height) {
        Ok(icon) => Some(icon),
        Err(e) => {
            report!("the embedded window icon was refused: {e}");
            None
        },
    }
}

pub(super) fn pick_config(configs: Box<dyn Iterator<Item = Config> + '_>) -> Config {
    let configs: Vec<Config> = configs.collect();
    let best = best_config(
        configs
            .iter()
            .map(|config| (config.supports_transparency(), config.num_samples())),
    )
    .expect("the display offered no configs");
    configs
        .into_iter()
        .nth(best)
        .expect("the chosen config is one of those offered")
}

pub(super) fn requires_transparent_configs(platform: Platform) -> bool {
    platform == Platform::MacOs
}

pub(super) fn transparency_available(
    platform: Platform,
    supports_transparency: Option<bool>,
    alpha_size: u8,
) -> bool {
    match platform {
        Platform::Windows => alpha_size >= 8,
        Platform::Linux | Platform::MacOs => supports_transparency != Some(false),
    }
}

pub(super) fn best_config(candidates: impl Iterator<Item = (Option<bool>, u8)>) -> Option<usize> {
    candidates
        .enumerate()
        .min_by_key(|(_, (transparency, samples))| {
            let rank = match transparency {
                Some(true) => 0u8,
                None => 1,
                Some(false) => 2,
            };
            (rank, *samples)
        })
        .map(|(index, _)| index)
}
