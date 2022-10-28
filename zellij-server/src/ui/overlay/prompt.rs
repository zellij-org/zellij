use zellij_utils::pane_size::Size;

use super::{Overlay, OverlayType, Overlayable};
use crate::{ClientId, ServerInstruction};
use zellij_utils::errors::prelude::*;

use std::fmt::Write;

#[derive(Clone, Debug)]
pub struct Prompt {
    pub message: String,
    on_confirm: Option<Box<ServerInstruction>>,
    on_deny: Option<Box<ServerInstruction>>,
}

impl Prompt {
    pub fn new(
        message: String,
        on_confirm: Option<Box<ServerInstruction>>,
        on_deny: Option<Box<ServerInstruction>>,
    ) -> Self {
        Self {
            message,
            on_confirm,
            on_deny,
        }
    }
    pub fn confirm(self) -> Option<Box<ServerInstruction>> {
        self.on_confirm
    }
    pub fn deny(self) -> Option<Box<ServerInstruction>> {
        self.on_deny
    }
}

impl Overlayable for Prompt {
    fn generate_overlay(&self, size: Size) -> Result<String> {
        let mut output = String::new();
        let rows = size.rows;
        let mut vte_output = self.message.clone();
        Overlay::pad_cols(&mut vte_output, size.cols);
        for (x, h) in vte_output.chars().enumerate() {
            write!(
                &mut output,
                "\u{1b}[{};{}H\u{1b}[48;5;238m{}",
                rows,
                x + 1,
                h,
            )
            .context("failed to generate overlay for prompt")?;
        }
        Ok(output)
    }
}

pub fn _generate_quit_prompt(client_id: ClientId) -> Overlay {
    let prompt = Prompt::new(
        (" Do you want to quit zellij? [Y]es / [N]o").to_string(),
        Some(Box::new(ServerInstruction::ClientExit(client_id))),
        None,
    );
    Overlay {
        overlay_type: OverlayType::Prompt(prompt),
    }
}
