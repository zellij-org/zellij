use super::grips::bring_panel_forward_with;
use super::*;
use crate::render::scene::Margins;
use crate::window::desktop::grips::grip_at;
use crate::window::desktop::grips::grip_or_header;
use crate::window::desktop::grips::Grip;
use winit::window::CursorIcon;
use winit::window::ResizeDirection;

fn margins() -> Margins {
    Margins {
        left: 6,
        top: 12,
        right: 6,
        bottom: 6,
    }
}

#[test]
fn edges_resize_the_top_strip_moves_and_the_inside_belongs_to_the_panel() {
    let size = (800, 600);
    let at = |x: f64, y: f64| grip_at((x, y), size, margins());
    assert_eq!(at(400.0, 300.0), None);
    assert_eq!(at(400.0, 9.0), Some(Grip::Move));
    assert_eq!(at(400.0, 2.0), Some(Grip::Resize(ResizeDirection::North)));
    assert_eq!(at(400.0, 597.0), Some(Grip::Resize(ResizeDirection::South)));
    assert_eq!(at(2.0, 300.0), Some(Grip::Resize(ResizeDirection::West)));
    assert_eq!(at(797.0, 300.0), Some(Grip::Resize(ResizeDirection::East)));
    assert_eq!(at(2.0, 2.0), Some(Grip::Resize(ResizeDirection::NorthWest)));
    assert_eq!(
        at(797.0, 2.0),
        Some(Grip::Resize(ResizeDirection::NorthEast))
    );
    assert_eq!(
        at(3.0, 598.0),
        Some(Grip::Resize(ResizeDirection::SouthWest))
    );
    assert_eq!(
        at(797.0, 598.0),
        Some(Grip::Resize(ResizeDirection::SouthEast))
    );
    assert_eq!(
        at(8.0, 597.0),
        Some(Grip::Resize(ResizeDirection::SouthWest))
    );
}

#[derive(Default)]
struct FakeWindow {
    calls: std::cell::RefCell<Vec<String>>,
}

impl FakeWindow {
    fn log(&self, call: impl Into<String>) {
        self.calls.borrow_mut().push(call.into());
    }
    fn calls(&self) -> Vec<String> {
        self.calls.borrow().clone()
    }
}

impl WindowOps for FakeWindow {
    fn set_minimized(&self, minimized: bool) {
        self.log(format!("minimized {}", minimized));
    }
    fn set_visible(&self, visible: bool) {
        self.log(format!("visible {}", visible));
    }
    fn focus_window(&self) {
        self.log("focus");
    }
    fn set_cursor(&self, icon: CursorIcon) {
        self.log(format!("cursor {:?}", icon));
    }
    fn drag_window(&self) -> Result<(), String> {
        self.log("drag");
        Ok(())
    }
    fn drag_resize_window(&self, direction: ResizeDirection) -> Result<(), String> {
        self.log(format!("resize {:?}", direction));
        Ok(())
    }
}

#[test]
fn pressing_an_edge_resizes_the_panel_in_that_direction() {
    let window = FakeWindow::default();
    let size = (800, 600);
    for (position, direction) in [
        ((2.0, 300.0), ResizeDirection::West),
        ((797.0, 300.0), ResizeDirection::East),
        ((400.0, 597.0), ResizeDirection::South),
        ((797.0, 598.0), ResizeDirection::SouthEast),
    ] {
        let grip = grip_or_header(position, size, margins(), || false).unwrap();
        start_grip(&window, grip);
        assert_eq!(
            window.calls().last().cloned(),
            Some(format!("resize {:?}", direction))
        );
    }
}

#[test]
fn a_blank_header_cell_moves_the_panel_and_a_busy_one_belongs_to_the_panel() {
    let window = FakeWindow::default();
    let size = (800, 600);
    assert_eq!(
        grip_or_header((400.0, 30.0), size, margins(), || true),
        Some(Grip::Move)
    );
    assert_eq!(
        grip_or_header((400.0, 30.0), size, margins(), || false),
        None
    );
    start_grip(&window, Grip::Move);
    assert_eq!(window.calls(), vec!["drag".to_owned()]);
}

#[test]
fn hovering_a_grip_shows_its_cursor_and_leaving_it_restores_the_pointer() {
    let window = FakeWindow::default();
    assert_eq!(
        hover(
            Some(&window),
            Some(Grip::Resize(ResizeDirection::NorthEast)),
            false
        ),
        Hover::Grip
    );
    assert_eq!(window.calls(), vec!["cursor NeResize".to_owned()]);
    assert_eq!(hover(Some(&window), None, true), Hover::BackToNormal);
    assert_eq!(hover(Some(&window), None, false), Hover::Inside);
    assert_eq!(window.calls().len(), 1);
}

#[test]
fn raising_unminimizes_shows_and_focuses_before_activating_with_the_token() {
    let window = FakeWindow::default();
    let mut tokens = vec![];
    let mut activate = |token: Option<String>| tokens.push(token);
    bring_forward(&window, Some(&mut activate), Some("t".to_owned()));
    assert_eq!(
        window.calls(),
        vec![
            "minimized false".to_owned(),
            "visible true".to_owned(),
            "focus".to_owned()
        ]
    );
    assert_eq!(tokens, vec![Some("t".to_owned())]);
}

#[test]
fn without_wayland_activation_raising_only_focuses() {
    let window = FakeWindow::default();
    bring_forward(&window, None, Some("t".to_owned()));
    assert_eq!(window.calls().last().map(String::as_str), Some("focus"));
}

#[test]
fn the_panel_is_activated_on_a_view_switch_only_with_a_token() {
    let mut tokens = vec![];
    let mut activate = |token: Option<String>| tokens.push(token);
    bring_panel_forward_with(Some(&mut activate), None);
    bring_panel_forward_with(Some(&mut activate), Some("t".to_owned()));
    bring_panel_forward_with(None, Some("u".to_owned()));
    assert_eq!(tokens, vec![Some("t".to_owned())]);
}
