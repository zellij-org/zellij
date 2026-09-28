use super::{sample, Base};
use zellij_tile::prelude::*;

fn categories() -> Vec<&'static str> {
    vec![
        "General",
        "Appearance",
        "Keybindings",
        "Plugins",
        "Sessions",
        "Web server",
        "Advanced",
        "About",
    ]
}

pub fn story() -> Base {
    Base::new("Side menu")
        .intro(vec![
            "A vertical list of categories. Up/Down or a click changes the current one.",
            "Hovering highlights a row. When categories do not fit, ↑/↓ rows show how many are",
            "hidden; the wheel scrolls while hovering and a click on ↑/↓ pages.",
        ])
        .sample(sample("Unfocused", 16, 5, |x, y| {
            SideMenu::new(categories())
                .selected(1)
                .serialize(x, y, 16, 5)
        }))
        .sample(sample("Focused", 16, 5, |x, y| {
            SideMenu::new(categories())
                .selected(1)
                .focused()
                .serialize(x, y, 16, 5)
        }))
        .sample(sample("Hovered row", 16, 5, |x, y| {
            SideMenu::new(categories())
                .selected(1)
                .hovered_item(3)
                .serialize(x, y, 16, 5)
        }))
        .sample(sample("Scrolled", 16, 5, |x, y| {
            SideMenu::new(categories())
                .selected(6)
                .focused()
                .serialize(x, y, 16, 5)
        }))
        .live("Side menu", SideMenu::new(categories()), 18, 6)
}
