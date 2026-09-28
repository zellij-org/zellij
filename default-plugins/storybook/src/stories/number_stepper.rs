use super::{sample, Base};
use zellij_tile::prelude::*;

pub fn story() -> Base {
    Base::new("Number stepper")
        .intro(vec![
            "Left/Right step by a set amount, typed digits edit the value (Enter keeps it, Esc",
            "drops it), clicks on the arrows step. Values are clamped to the minimum and maximum.",
        ])
        .sample(sample("Unfocused", 16, 1, |x, y| {
            NumberStepper::new("Size", 10000).serialize(x, y)
        }))
        .sample(sample("Hovered", 16, 1, |x, y| {
            NumberStepper::new("Size", 10000).hovered().serialize(x, y)
        }))
        .sample(sample("Focused", 16, 1, |x, y| {
            NumberStepper::new("Size", 10000).focused().serialize(x, y)
        }))
        .sample(sample("Disabled", 16, 1, |x, y| {
            NumberStepper::new("Size", 10000).disabled().serialize(x, y)
        }))
        .sample(sample("At minimum", 16, 1, |x, y| {
            NumberStepper::new("Size", 0)
                .min(0)
                .max(100)
                .focused()
                .serialize(x, y)
        }))
        .sample(sample("At maximum", 16, 1, |x, y| {
            NumberStepper::new("Size", 100)
                .min(0)
                .max(100)
                .focused()
                .serialize(x, y)
        }))
        .sample(sample("Editing", 16, 1, |x, y| {
            NumberStepper::new("Size", 10000)
                .focused()
                .editing("123")
                .serialize(x, y)
        }))
        .live(
            "Scrollback",
            NumberStepper::new("Scrollback", 10000)
                .step(1000)
                .min(0)
                .max(50000)
                .label_width(14),
            30,
            1,
        )
        .live(
            "Columns (1-5)",
            NumberStepper::new("Columns", 3)
                .min(1)
                .max(5)
                .label_width(14),
            30,
            1,
        )
        .live(
            "Offset",
            NumberStepper::new("Offset", 0)
                .step(5)
                .min(-50)
                .max(50)
                .label_width(14),
            30,
            1,
        )
}
