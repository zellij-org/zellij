use crate::window::App;
use anyhow::Result;
use winit::window::{CursorIcon, ResizeDirection};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Grip {
    Move,
    Resize(ResizeDirection),
}

pub(crate) fn grip_at(
    position: (f64, f64),
    size: (u32, u32),
    margins: crate::render::scene::Margins,
) -> Option<Grip> {
    let (x, y) = position;
    let (width, height) = (size.0 as f64, size.1 as f64);
    let reach = |margin: u32| (margin as f64).max(4.0);
    let left = x < reach(margins.left);
    let right = x >= width - reach(margins.right);
    let bottom = y >= height - reach(margins.bottom);
    let top_edge = y < reach(margins.left.min(margins.right));
    let top_strip = y < reach(margins.top);
    let corner = reach(margins.left.max(margins.right)) * 2.0;
    use ResizeDirection::*;
    let direction = match (
        top_edge,
        bottom,
        left || x < corner && bottom,
        right || x >= width - corner && bottom,
    ) {
        (true, _, true, _) => Some(NorthWest),
        (true, _, _, true) => Some(NorthEast),
        (_, true, true, _) => Some(SouthWest),
        (_, true, _, true) => Some(SouthEast),
        (true, _, _, _) => Some(North),
        (_, true, _, _) => Some(South),
        (_, _, true, _) if left => Some(West),
        (_, _, _, true) if right => Some(East),
        _ => None,
    };
    match direction {
        Some(direction) => Some(Grip::Resize(direction)),
        None if top_strip => Some(Grip::Move),
        None => None,
    }
}

pub(super) fn grip_cursor(grip: Grip) -> CursorIcon {
    use ResizeDirection::*;
    match grip {
        Grip::Move => CursorIcon::Move,
        Grip::Resize(North) => CursorIcon::NResize,
        Grip::Resize(South) => CursorIcon::SResize,
        Grip::Resize(East) => CursorIcon::EResize,
        Grip::Resize(West) => CursorIcon::WResize,
        Grip::Resize(NorthEast) => CursorIcon::NeResize,
        Grip::Resize(NorthWest) => CursorIcon::NwResize,
        Grip::Resize(SouthEast) => CursorIcon::SeResize,
        Grip::Resize(SouthWest) => CursorIcon::SwResize,
    }
}

pub(crate) trait WindowOps {
    fn set_minimized(&self, minimized: bool);
    fn set_visible(&self, visible: bool);
    fn focus_window(&self);
    fn set_cursor(&self, icon: CursorIcon);
    fn drag_window(&self) -> Result<(), String>;
    fn drag_resize_window(&self, direction: ResizeDirection) -> Result<(), String>;
}

impl WindowOps for winit::window::Window {
    fn set_minimized(&self, minimized: bool) {
        winit::window::Window::set_minimized(self, minimized)
    }
    fn set_visible(&self, visible: bool) {
        winit::window::Window::set_visible(self, visible)
    }
    fn focus_window(&self) {
        winit::window::Window::focus_window(self)
    }
    fn set_cursor(&self, icon: CursorIcon) {
        winit::window::Window::set_cursor(self, icon)
    }
    fn drag_window(&self) -> Result<(), String> {
        winit::window::Window::drag_window(self).map_err(|e| e.to_string())
    }
    fn drag_resize_window(&self, direction: ResizeDirection) -> Result<(), String> {
        winit::window::Window::drag_resize_window(self, direction).map_err(|e| e.to_string())
    }
}

pub(crate) fn bring_forward(
    window: &dyn WindowOps,
    activate: Option<&mut dyn FnMut(Option<String>)>,
    token: Option<String>,
) {
    window.set_minimized(false);
    window.set_visible(true);
    window.focus_window();
    if let Some(activate) = activate {
        activate(token);
    }
}

#[cfg_attr(not(freedesktop), allow(dead_code))]
pub(crate) fn bring_panel_forward_with(
    activate: Option<&mut dyn FnMut(Option<String>)>,
    token: Option<String>,
) {
    if let (Some(activate), Some(token)) = (activate, token) {
        activate(Some(token));
    }
}

pub(crate) fn start_grip(window: &dyn WindowOps, grip: Grip) {
    let started = match grip {
        Grip::Move => window.drag_window(),
        Grip::Resize(direction) => window.drag_resize_window(direction),
    };
    if let Err(e) = started {
        report!("the panel could not be moved or resized: {}", e);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Hover {
    Grip,
    BackToNormal,
    Inside,
}

pub(crate) fn hover(window: Option<&dyn WindowOps>, grip: Option<Grip>, on_edge: bool) -> Hover {
    match (grip, window) {
        (Some(grip), Some(window)) => {
            window.set_cursor(grip_cursor(grip));
            Hover::Grip
        },
        (None, _) if on_edge => Hover::BackToNormal,
        _ => Hover::Inside,
    }
}

pub(crate) fn grip_or_header(
    position: (f64, f64),
    size: (u32, u32),
    margins: crate::render::scene::Margins,
    header_is_blank: impl FnOnce() -> bool,
) -> Option<Grip> {
    grip_at(position, size, margins).or_else(|| header_is_blank().then_some(Grip::Move))
}

pub(super) fn panel_grip(app: &App, position: winit::dpi::PhysicalPosition<f64>) -> Option<Grip> {
    let size = app.window()?.inner_size();
    grip_or_header(
        (position.x, position.y),
        (size.width, size.height),
        app.edge_margins(),
        || app.header_is_blank_at(position),
    )
}
