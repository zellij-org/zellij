use anyhow::Result;
use zellij_utils::{data::CopyDestination, input::options::Clipboard};

use crate::ClientId;

use super::{copy_command::CopyCommand, Output};

pub(crate) enum ClipboardProvider {
    Command(CopyCommand),
    Osc52(Clipboard),
}

impl ClipboardProvider {
    pub(crate) fn set_content(
        &self,
        content: &str,
        output: &mut Output,
        client_ids: impl Iterator<Item = ClientId>,
    ) -> Result<()> {
        match &self {
            ClipboardProvider::Command(command) => {
                command.set(content.to_string())?;
            },
            ClipboardProvider::Osc52(clipboard) => {
                let dest = match clipboard {
                    #[cfg(not(target_os = "macos"))]
                    Clipboard::Primary => 'p',
                    #[cfg(target_os = "macos")] // primary selection does not exist on macos
                    Clipboard::Primary => 'c',
                    Clipboard::System => 'c',
                };
                output.add_pre_vte_instruction_to_multiple_clients(
                    client_ids,
                    &format!("\u{1b}]52;{};{}\u{1b}\\", dest, base64::encode(content)),
                );
            },
        };
        Ok(())
    }

    pub(crate) fn as_copy_destination(&self) -> CopyDestination {
        match self {
            ClipboardProvider::Command(_) => CopyDestination::Command,
            ClipboardProvider::Osc52(clipboard) => match clipboard {
                Clipboard::Primary => CopyDestination::Primary,
                Clipboard::System => CopyDestination::System,
            },
        }
    }
}
