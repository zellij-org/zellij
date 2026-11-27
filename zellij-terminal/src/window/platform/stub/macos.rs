use winit::window::{Window, WindowAttributes};
use zellij_utils::input::terminal::OptionAsAlt;

pub(in crate::window) fn set_see_through(_window: &Window, _see_through: bool) {}

pub(in crate::window) fn with_option_as_alt(
    attributes: WindowAttributes,
    _side: OptionAsAlt,
) -> WindowAttributes {
    attributes
}

pub(in crate::window) fn set_option_as_alt(_window: &Window, _side: OptionAsAlt) {}

pub(in crate::window) fn set_accessory(_accessory: bool) {}

pub(in crate::window) fn activate_app() {}
