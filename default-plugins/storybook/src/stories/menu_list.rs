use super::{sample, Base};
use zellij_tile::prelude::*;

fn edit_items() -> Vec<MenuItem> {
    vec![
        MenuItem::new("Copy").shortcut("Ctrl c"),
        MenuItem::new("Paste").shortcut("Ctrl v"),
        MenuItem::separator(),
        MenuItem::new("Delete").disabled(),
        MenuItem::new("Rename").shortcut("F2"),
    ]
}

fn long_items() -> Vec<MenuItem> {
    vec![
        MenuItem::new("New pane").shortcut("Alt n"),
        MenuItem::new("New tab").shortcut("Ctrl t"),
        MenuItem::new("Split right").shortcut("Alt r"),
        MenuItem::new("Split down").shortcut("Alt d"),
        MenuItem::separator(),
        MenuItem::new("Float pane").shortcut("Alt f"),
        MenuItem::new("Embed pane").disabled(),
        MenuItem::new("Fullscreen").shortcut("Alt F"),
        MenuItem::separator(),
        MenuItem::new("Rename pane"),
        MenuItem::new("Rename tab"),
        MenuItem::new("Close pane").shortcut("Ctrl x"),
        MenuItem::new("Close tab").disabled(),
        MenuItem::new("Detach").shortcut("Ctrl d"),
        MenuItem::new("Quit").shortcut("Ctrl q"),
    ]
}

pub fn story() -> Base {
    Base::new("Menu list")
        .intro(vec![
            "Rows with right-aligned shortcuts, separators, disabled rows, an optional border.",
            "Up/Down skip separators and disabled rows, letters jump, Enter or a click chooses,",
            "Esc cancels, hover highlights, and the list scrolls when rows do not fit: ↑/↓ show",
            "how many rows are hidden, the wheel scrolls while hovering, a click on ↑/↓ pages.",
        ])
        .sample(sample("Unfocused", 24, 5, |x, y| {
            MenuList::new(edit_items()).serialize(x, y, 24, 5)
        }))
        .sample(sample("Focused, border", 24, 7, |x, y| {
            MenuList::new(edit_items())
                .with_border()
                .focused()
                .serialize(x, y, 24, 7)
        }))
        .sample(sample("Highlight on Rename", 24, 7, |x, y| {
            MenuList::new(edit_items())
                .with_border()
                .highlighted(4)
                .serialize(x, y, 24, 7)
        }))
        .sample(sample("Marked value", 24, 5, |x, y| {
            MenuList::new(vec![
                MenuItem::new("Left"),
                MenuItem::new("Center").marked(),
                MenuItem::new("Right"),
            ])
            .with_border()
            .highlighted(1)
            .serialize(x, y, 24, 5)
        }))
        .sample(sample("More items than fit", 24, 7, |x, y| {
            MenuList::new(long_items())
                .with_border()
                .serialize(x, y, 24, 7)
        }))
        .sample(sample("Scrolled, no border", 24, 7, |x, y| {
            MenuList::new(long_items())
                .highlighted(9)
                .serialize(x, y, 24, 7)
        }))
        .live(
            "Menu list",
            MenuList::new(long_items()).with_border(),
            30,
            9,
        )
        .live("Short menu", MenuList::new(edit_items()), 30, 6)
}
