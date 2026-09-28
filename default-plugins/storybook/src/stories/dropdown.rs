use super::{sample, Base};
use zellij_tile::prelude::*;

const LABEL_WIDTH: usize = 14;
const FIELD_WIDTH: usize = 38;

fn themes() -> Vec<&'static str> {
    vec![
        "default",
        "dracula",
        "gruvbox-dark",
        "nord",
        "catppuccin-mocha",
        "tokyo-night",
        "solarized-light",
    ]
}

fn many_choices() -> Vec<String> {
    (1..=40).map(|i| format!("Choice number {}", i)).collect()
}

fn long_values() -> Vec<&'static str> {
    vec![
        "a value that is far too long to fit inside the field",
        "/home/user/projects/zellij/default-plugins/storybook/src/main.rs",
        "short",
    ]
}

pub fn story() -> Base {
    Base::new("Dropdown")
        .intro(vec![
            "Enter, Space or a click opens the list; Left/Right change the value while closed.",
            "While open: Up/Down move, letters jump, Enter or a click chooses, Esc or an outside",
            "click cancels. The open list is drawn last (render_overlay) so it covers what is below.",
        ])
        .sample(sample("Closed", 30, 1, |x, y| {
            Dropdown::new("Mode", vec!["normal", "titles"]).serialize(x, y, 30)
        }))
        .sample(sample("Hovered", 30, 1, |x, y| {
            Dropdown::new("Mode", vec!["normal", "titles"])
                .hovered()
                .serialize(x, y, 30)
        }))
        .sample(sample("Focused", 30, 1, |x, y| {
            Dropdown::new("Mode", vec!["normal", "titles"])
                .selected(1)
                .focused()
                .serialize(x, y, 30)
        }))
        .sample(sample("Disabled", 30, 1, |x, y| {
            Dropdown::new("Mode", vec!["normal", "titles"])
                .disabled()
                .serialize(x, y, 30)
        }))
        .sample(sample("Long value (cut short)", 30, 1, |x, y| {
            Dropdown::new("Path", long_values())
                .selected(1)
                .serialize(x, y, 30)
        }))
        .sample(sample("Open", 30, 6, |x, y| {
            let mut dropdown = Dropdown::new("Mode", vec!["normal", "titles", "compact"])
                .selected(1)
                .focused()
                .opened();
            let mut serialized = dropdown.serialize(x, y, 30);
            serialized.push_str(&dropdown.serialize_overlay(y + 6, x + 30));
            serialized
        }))
        .live(
            "Theme",
            Dropdown::new("Theme", themes()).label_width(LABEL_WIDTH),
            FIELD_WIDTH,
            1,
        )
        .live(
            "Many choices",
            Dropdown::new("Many choices", many_choices())
                .label_width(LABEL_WIDTH)
                .max_list_rows(6)
                .selected(12),
            FIELD_WIDTH,
            1,
        )
        .live(
            "Long values",
            Dropdown::new("Long values", long_values()).label_width(LABEL_WIDTH),
            FIELD_WIDTH,
            1,
        )
        .live(
            "Disabled",
            Dropdown::new("Disabled", vec!["cannot change"])
                .label_width(LABEL_WIDTH)
                .disabled(),
            FIELD_WIDTH,
            1,
        )
        .live_at_bottom(
            "Opens upward",
            Dropdown::new("Opens upward", themes()).label_width(LABEL_WIDTH),
            FIELD_WIDTH,
        )
}
