use super::{sample, Base};
use zellij_tile::prelude::*;

pub fn story() -> Base {
    Base::new("Toggle")
        .intro(vec!["Space, Enter or a click switches it."])
        .sample(sample("Off", 11, 1, |x, y| {
            Toggle::new("Wrap", false).serialize(x, y)
        }))
        .sample(sample("On", 11, 1, |x, y| {
            Toggle::new("Wrap", true).serialize(x, y)
        }))
        .sample(sample("Hovered", 11, 1, |x, y| {
            Toggle::new("Wrap", false).hovered().serialize(x, y)
        }))
        .sample(sample("Focused off", 11, 1, |x, y| {
            Toggle::new("Wrap", false).focused().serialize(x, y)
        }))
        .sample(sample("Focused on", 11, 1, |x, y| {
            Toggle::new("Wrap", true).focused().serialize(x, y)
        }))
        .sample(sample("Disabled off", 12, 1, |x, y| {
            Toggle::new("Wrap", false).disabled().serialize(x, y)
        }))
        .sample(sample("Disabled on", 11, 1, |x, y| {
            Toggle::new("Wrap", true).disabled().serialize(x, y)
        }))
        .live(
            "Mouse mode",
            Toggle::new("Mouse mode", true).label_width(14),
            20,
            1,
        )
        .live(
            "Pane frames",
            Toggle::new("Pane frames", false).label_width(14),
            20,
            1,
        )
        .live(
            "Disabled",
            Toggle::new("Disabled", true).label_width(14).disabled(),
            20,
            1,
        )
}
