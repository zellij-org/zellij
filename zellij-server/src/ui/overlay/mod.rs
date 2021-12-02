//! This module handles the overlay's over the [`Screen`]
//!
//! They consist of:
//!
//! prompt's:
//!
//! notification's:

pub mod prompt;

use crate::ServerInstruction;
use zellij_utils::pane_size::Size;

#[derive(Clone, Debug)]
pub struct Overlay {
    pub overlay_type: OverlayType,
}

pub trait Overlayable {
    /// Generates vte_output that can be passed into
    /// the `render()` function
    fn generate_overlay(&self, size: Size) -> String;
}

#[derive(Clone, Debug)]
struct Padding {
    rows: usize,
    cols: usize,
}

#[derive(Clone, Debug)]
pub enum OverlayType {
    Prompt(prompt::Prompt),
}

impl Overlayable for OverlayType {
    fn generate_overlay(&self, size: Size) -> String {
        match &self {
            OverlayType::Prompt(prompt) => prompt.generate_overlay(size),
        }
    }
}

/// Entrypoint from [`Screen`], which holds the context in which
/// the overlays are being rendered.
/// The most recent overlays draw over the previous overlays.
#[derive(Clone, Debug, Default)]
pub struct OverlayWindow {
    pub overlay_stack: Vec<Overlay>,
}

impl Overlayable for OverlayWindow {
    fn generate_overlay(&self, size: Size) -> String {
        let mut output = String::new();
        //let clear_display = "\u{1b}[2J";
        //output.push_str(&clear_display);
        for overlay in &self.overlay_stack {
            let vte_output = overlay.generate_overlay(size);
            output.push_str(&vte_output);
        }
        output
    }
}

impl Overlay {
    pub fn prompt_confirm(self) -> Option<Box<ServerInstruction>> {
        match self.overlay_type {
            OverlayType::Prompt(p) => p.confirm(),
        }
    }
    pub fn prompt_deny(self) -> Option<Box<ServerInstruction>> {
        match self.overlay_type {
            OverlayType::Prompt(p) => p.deny(),
        }
    }
}

impl Overlayable for Overlay {
    fn generate_overlay(&self, size: Size) -> String {
        self.overlay_type.generate_overlay(size)
    }
}

impl Overlay {
    pub fn new(overlay_type: OverlayType) -> Self {
        Self { overlay_type }
    }

    fn pad_cols(output: &mut String, cols: usize) {
        if let Some(padding) = cols.checked_sub(output.len()) {
            for _ in 0..padding {
                output.push(' ');
            }
        }
    }
}
